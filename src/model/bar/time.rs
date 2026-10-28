//! Fractional index and UTC interpolation for sorted bars.

use super::Bar;
use crate::model::Timeframe;
use chrono::{DateTime, Datelike, Duration, Months, Utc};

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
fn edge_step(bars: &[Bar], at: usize) -> f64 {
    let sample = if at == 0 {
        &bars[..bars.len().min(16)]
    } else {
        &bars[bars.len().saturating_sub(16)..]
    };
    let (left, right) = segment(bars, at);
    let distinct = (left != right)
        .then(|| seconds(bars[right].time - bars[left].time) / (right - left) as f64);
    sample
        .windows(2)
        .map(|pair| seconds(pair[1].time - pair[0].time))
        // Keep the per-index spacing of repeated edge elements.
        .chain(distinct)
        .filter(|step| *step > 0.0)
        .reduce(f64::min)
        .unwrap_or(1.0)
}

fn calendar_months(timeframe: Option<Timeframe>) -> Option<u32> {
    match timeframe? {
        Timeframe::Month1 => Some(1),
        Timeframe::Months(count) => Some(count.get()),
        _ => None,
    }
}

fn shift_months(origin: DateTime<Utc>, months: i64) -> Option<DateTime<Utc>> {
    let count = Months::new(u32::try_from(months.unsigned_abs()).ok()?);
    let shifted = if months < 0 {
        origin.checked_sub_months(count)?
    } else {
        origin.checked_add_months(count)?
    };
    // Month-end anchors stay at month end after a short month is appended.
    if origin.date_naive().succ_opt()?.month() != origin.month() {
        let end = shifted
            .with_day(1)?
            .checked_add_months(Months::new(1))?
            .date_naive()
            .pred_opt()?
            .day();
        shifted.with_day(end)
    } else {
        Some(shifted)
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
    time_at_with_timeframe(bars, index, None)
}

pub(crate) fn time_at_with_timeframe(
    bars: &[Bar],
    index: f64,
    timeframe: Option<Timeframe>,
) -> Option<DateTime<Utc>> {
    if bars.is_empty() || !index.is_finite() {
        return None;
    }
    let at = index.floor().clamp(0.0, (bars.len() - 1) as f64) as usize;
    let offset = index - at as f64;
    if index < 0.0 || index > (bars.len() - 1) as f64 {
        if let Some(months) = calendar_months(timeframe) {
            let whole = offset.floor();
            let count = whole * f64::from(months);
            if count.abs() > u32::MAX as f64 {
                return None;
            }
            let left = shift_months(bars[at].time, count as i64)?;
            if offset == whole {
                return Some(left);
            }
            let right = shift_months(bars[at].time, count as i64 + i64::from(months))?;
            return add_seconds(left, (offset - whole) * seconds(right - left));
        }
        return add_seconds(bars[at].time, offset * edge_step(bars, at));
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
    index_at_with_timeframe(bars, time, None)
}

pub(crate) fn index_at_with_timeframe(
    bars: &[Bar],
    time: DateTime<Utc>,
    timeframe: Option<Timeframe>,
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
            let distance = i64::from(time.year() - origin.year()) * 12 + i64::from(time.month())
                - i64::from(origin.month());
            let mut whole = distance.div_euclid(i64::from(months));
            let mut left = shift_months(origin, whole * i64::from(months))?;
            if time < left {
                whole -= 1;
                left = shift_months(origin, whole * i64::from(months))?;
            }
            if time == left {
                return Some(at as f64 + whole as f64);
            }
            let right = shift_months(origin, (whole + 1) * i64::from(months))?;
            return Some(at as f64 + whole as f64 + seconds(time - left) / seconds(right - left));
        }
        return Some(at as f64 + seconds(time - origin) / edge_step(bars, at));
    }
    let (left, right) = segment(bars, at.saturating_sub(1));
    let step = seconds(bars[right].time - bars[left].time) / (right - left) as f64;
    Some(left as f64 + seconds(time - bars[left].time) / step)
}
