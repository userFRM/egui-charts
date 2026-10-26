//! Fractional index and UTC interpolation for sorted bars.

use super::Bar;
use chrono::{DateTime, Duration, Utc};

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

pub(crate) fn time_at(bars: &[Bar], index: f64) -> Option<DateTime<Utc>> {
    if bars.is_empty() || !index.is_finite() {
        return None;
    }
    let at = index.floor().clamp(0.0, (bars.len() - 1) as f64) as usize;
    let (left, right) = if index >= 0.0 && at + 1 < bars.len() {
        (at, at + 1)
    } else {
        segment(bars, at)
    };
    let step = if left == right {
        1.0
    } else {
        seconds(bars[right].time - bars[left].time) / (right - left) as f64
    };
    let nanos = (index - at as f64) * step * 1e9;
    if !nanos.is_finite() || nanos.abs() >= i64::MAX as f64 {
        return None;
    }
    bars[at]
        .time
        .checked_add_signed(Duration::nanoseconds(nanos.round() as i64))
}

pub(crate) fn index_at(bars: &[Bar], time: DateTime<Utc>) -> Option<f64> {
    bars.first()?;
    let at = bars
        .partition_point(|bar| bar.time < time)
        .min(bars.len() - 1);
    if bars[at].time == time {
        return Some(at as f64);
    }
    let beyond = time > bars[at].time;
    let (left, right) = segment(bars, if beyond { at } else { at.saturating_sub(1) });
    let step = if bars[left].time == bars[right].time {
        1.0
    } else {
        seconds(bars[right].time - bars[left].time) / (right - left) as f64
    };
    let origin = if beyond { at } else { left };
    Some(origin as f64 + seconds(time - bars[origin].time) / step)
}
