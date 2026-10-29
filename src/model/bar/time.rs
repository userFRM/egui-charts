//! Fractional index and UTC interpolation for sorted bars.

use super::Bar;
use crate::model::{Timeframe, TimezoneId, local_instant, local_time};
use chrono::{DateTime, Datelike, Duration, Months, NaiveDateTime, NaiveTime, Utc};

// Use differences relative to an actual bar, preserving sub-millisecond execution times.
fn seconds(duration: Duration) -> f64 {
    duration.num_seconds() as f64 + duration.subsec_nanos() as f64 / 1e9
}

fn segment(bars: &[Bar], index: usize) -> (usize, usize) {
    let mut right = index + 1;
    while right < bars.len() && bars[right].time == bars[index].time {
        right += 1;
    }
    if right < bars.len() {
        return (index, right);
    }
    let left = bars.partition_point(|bar| bar.time < bars[index].time);
    (if left == 0 { index } else { left - 1 }, index)
}

// Sample the loaded edge, never the viewport: a session closure is not a bar interval.
fn edge_sample(bars: &[Bar], at: usize) -> &[Bar] {
    if at == 0 {
        &bars[..bars.len().min(16)]
    } else {
        &bars[bars.len().saturating_sub(16)..]
    }
}

// Returns the seconds from the edge bar to the next slot out, and between later slots.
fn edge_step(bars: &[Bar], at: usize, timeframe: Option<Timeframe>) -> (f64, f64) {
    let sample = edge_sample(bars, at);
    let step = if let Some(duration) = timeframe
        .map(|tf| tf.duration())
        .filter(|d| *d > Duration::zero())
    {
        seconds(duration)
    } else {
        let (left, right) = segment(bars, at);
        // Keep the per-index spacing of repeated edge elements.
        if right - left > 1 {
            let step = seconds(bars[right].time - bars[left].time) / (right - left) as f64;
            return (step, step);
        }
        let mut steps = [0.0; 15];
        for (step, pair) in steps.iter_mut().zip(sample.windows(2)) {
            *step = seconds(pair[1].time - pair[0].time);
        }
        steps
            .into_iter()
            .filter(|step| *step > 0.0)
            .max_by(|a, b| {
                let count = |step| steps.iter().filter(|&&value| value == step).count();
                // Prefer the shorter spacing when session gaps tie with regular intervals.
                count(*a).cmp(&count(*b)).then_with(|| b.total_cmp(a))
            })
            .unwrap_or(1.0)
    };
    // A clock change moves local keys by an hour, so only steps dividing an hour keep a UTC grid.
    if 3600.0 % step != 0.0 {
        return (step, step);
    }
    // Slots stay on the grid of the regular pair nearest the edge, past a partial edge bar.
    let regular = |pair: &&[Bar]| seconds(pair[1].time - pair[0].time) == step;
    let behind = if at == 0 {
        sample
            .windows(2)
            .find(regular)
            .map(|pair| pair[0].time - bars[at].time)
    } else {
        sample
            .windows(2)
            .rev()
            .find(regular)
            .map(|pair| bars[at].time - pair[1].time)
    };
    (step - behind.map_or(0.0, seconds) % step, step)
}

fn calendar_months(timeframe: Option<Timeframe>) -> Option<u32> {
    match timeframe? {
        Timeframe::Month1 => Some(1),
        Timeframe::Months(count) => Some(count.get()),
        _ => None,
    }
}

// Months keyed at midnight in the display zone follow its calendar; other keys, such as
// UTC midnight under a New York display zone, step in UTC.
fn calendar_zone<'a>(
    bars: &[Bar],
    at: usize,
    timezone: Option<&'a TimezoneId>,
    utc: &'a TimezoneId,
) -> Option<&'a TimezoneId> {
    let midnight = edge_sample(bars, at)
        .iter()
        .all(|bar| calendar_time(bar.time, timezone).time() == NaiveTime::MIN);
    if midnight { timezone } else { Some(utc) }
}

fn calendar_time(time: DateTime<Utc>, timezone: Option<&TimezoneId>) -> NaiveDateTime {
    match timezone {
        Some(zone) => local_time(zone.clone(), time),
        None => local_time(chrono::Local, time),
    }
}

fn shift_months(
    origin: DateTime<Utc>,
    months: i64,
    timezone: Option<&TimezoneId>,
) -> Option<DateTime<Utc>> {
    let origin = calendar_time(origin, timezone);
    let count = Months::new(u32::try_from(months.unsigned_abs()).ok()?);
    let shifted = if months < 0 {
        origin.checked_sub_months(count)?
    } else {
        origin.checked_add_months(count)?
    };
    // Month-end anchors stay at month end after a short month is appended.
    let shifted = if origin.date().succ_opt()?.month() != origin.month() {
        let end = shifted
            .with_day(1)?
            .checked_add_months(Months::new(1))?
            .date()
            .pred_opt()?
            .day();
        shifted.with_day(end)?
    } else {
        shifted
    };
    match timezone {
        Some(zone) => local_instant(zone.clone(), shifted),
        None => local_instant(chrono::Local, shifted),
    }
}

fn add_seconds(time: DateTime<Utc>, seconds: f64) -> Option<DateTime<Utc>> {
    let nanos = seconds * 1e9;
    if !nanos.is_finite() || nanos.abs() >= i64::MAX as f64 {
        return None;
    }
    time.checked_add_signed(Duration::nanoseconds(nanos.round() as i64))
}

pub(crate) fn time_at(bars: &[Bar], index: f64) -> Option<DateTime<Utc>> {
    time_at_with_timeframe(bars, index, None, Some(&TimezoneId::utc()))
}

pub(crate) fn time_at_with_timeframe(
    bars: &[Bar],
    index: f64,
    timeframe: Option<Timeframe>,
    timezone: Option<&TimezoneId>,
) -> Option<DateTime<Utc>> {
    if bars.is_empty() || !index.is_finite() {
        return None;
    }
    let at = index.floor().clamp(0.0, (bars.len() - 1) as f64) as usize;
    let offset = index - at as f64;
    if index < 0.0 || index > (bars.len() - 1) as f64 {
        if let Some(months) = calendar_months(timeframe) {
            let utc = TimezoneId::utc();
            let timezone = calendar_zone(bars, at, timezone, &utc);
            let whole = offset.floor();
            let count = whole * f64::from(months);
            if count.abs() > u32::MAX as f64 {
                return None;
            }
            let left = shift_months(bars[at].time, count as i64, timezone)?;
            if offset == whole {
                return Some(left);
            }
            let right = shift_months(bars[at].time, count as i64 + i64::from(months), timezone)?;
            return add_seconds(left, (offset - whole) * seconds(right - left));
        }
        let (near, step) = edge_step(bars, at, timeframe);
        let slots = offset.abs();
        let distance = if slots <= 1.0 {
            slots * near
        } else {
            near + (slots - 1.0) * step
        };
        return add_seconds(bars[at].time, distance.copysign(offset));
    }
    if offset == 0.0 {
        return Some(bars[at].time);
    }
    add_seconds(
        bars[at].time,
        offset * seconds(bars[at + 1].time - bars[at].time),
    )
}

pub(crate) fn index_at(bars: &[Bar], time: DateTime<Utc>) -> Option<f64> {
    index_at_with_timeframe(bars, time, None, Some(&TimezoneId::utc()))
}

pub(crate) fn index_at_with_timeframe(
    bars: &[Bar],
    time: DateTime<Utc>,
    timeframe: Option<Timeframe>,
    timezone: Option<&TimezoneId>,
) -> Option<f64> {
    let first = bars.first()?;
    let at = bars
        .partition_point(|bar| bar.time < time)
        .min(bars.len() - 1);
    if bars[at].time == time {
        return Some(at as f64);
    }
    if time < first.time || time > bars[at].time {
        let origin = bars[at].time;
        if let Some(months) = calendar_months(timeframe) {
            let utc = TimezoneId::utc();
            let timezone = calendar_zone(bars, at, timezone, &utc);
            let local = calendar_time(time, timezone);
            let origin_local = calendar_time(origin, timezone);
            let distance = i64::from(local.year() - origin_local.year()) * 12
                + i64::from(local.month())
                - i64::from(origin_local.month());
            let mut whole = distance.div_euclid(i64::from(months));
            let mut left = shift_months(origin, whole * i64::from(months), timezone)?;
            if time < left {
                whole -= 1;
                left = shift_months(origin, whole * i64::from(months), timezone)?;
            }
            if time == left {
                return Some(at as f64 + whole as f64);
            }
            let right = shift_months(origin, (whole + 1) * i64::from(months), timezone)?;
            return Some(at as f64 + whole as f64 + seconds(time - left) / seconds(right - left));
        }
        let (near, step) = edge_step(bars, at, timeframe);
        let distance = seconds(time - origin);
        let slots = if distance.abs() <= near {
            distance.abs() / near
        } else {
            1.0 + (distance.abs() - near) / step
        };
        return Some(at as f64 + slots.copysign(distance));
    }
    let (left, right) = segment(bars, at.saturating_sub(1));
    let step = seconds(bars[right].time - bars[left].time) / (right - left) as f64;
    Some(left as f64 + seconds(time - bars[left].time) / step)
}
