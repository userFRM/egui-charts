//! Multi-chart crosshair and time-scale synchronization.

use super::Chart;
use crate::model::Bar;
use chrono::{DateTime, Duration, Utc};

/// UTC timestamps at the left and right edges of a chart's plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleTimeRange {
    /// Time at the leftmost plot pixel.
    pub from: DateTime<Utc>,
    /// Time at the rightmost plot pixel.
    pub to: DateTime<Utc>,
}

impl Chart {
    /// The time of the nearest hovered bar or price-based element, or `None` outside the data.
    pub fn hover_time(&self) -> Option<DateTime<Utc>> {
        let index = self.last_hover_bar_idx?.round();
        (index >= 0.0 && index < self.state.shown().len() as f64)
            .then(|| self.state.shown()[index as usize].time)
    }

    /// Shows a crosshair at the nearest local bar to `time`; ties choose the earlier bar.
    /// Times outside the local data and `None` clear it. Repeated element times choose the
    /// first element. Each receiver resolves the timestamp against its own series.
    pub fn set_synced_crosshair_time(&mut self, time: Option<DateTime<Utc>>) {
        let bars = self.state.shown();
        self.synced_crosshair_bar_idx = time.and_then(|time| {
            if time < bars.first()?.time || time > bars.last()?.time {
                return None;
            }
            let after = bars.partition_point(|bar| bar.time < time);
            let index = if after > 0
                && after < bars.len()
                && time - bars[after - 1].time <= bars[after].time - time
            {
                after - 1
            } else {
                after
            };
            Some(index as f64)
        });
    }

    /// The visible UTC range, including whitespace extrapolated with the nearest distinct
    /// pair of bar times (one second for a single bar). Empty charts return `None`.
    /// Between bars, time is interpolated, so session gaps retain fractional edge positions.
    pub fn visible_time_range(&self) -> Option<VisibleTimeRange> {
        let scale = self.state.time_scale();
        let bars = self.state.shown();
        Some(VisibleTimeRange {
            from: time_at(bars, scale.coord_to_idx(0.0, 0.0, scale.width()) as f64)?,
            to: time_at(
                bars,
                scale.coord_to_idx(scale.width() - 1.0, 0.0, scale.width()) as f64,
            )?,
        })
    }

    /// Fits the receiver's bars to a UTC range using its own plot width.
    /// Returns `false` without changing the view for empty data, reversed or zero ranges,
    /// or a range whose endpoints map to the same element. Local scale constraints still apply.
    pub fn apply_synced_time_range(&mut self, range: VisibleTimeRange) -> bool {
        if range.from >= range.to {
            return false;
        }
        let bars = self.state.shown();
        let Some((from, to)) = index_at(bars, range.from).zip(index_at(bars, range.to)) else {
            return false;
        };
        let scale = self.state.time_scale_mut();
        if to <= from || scale.width() <= 1.0 {
            return false;
        }
        scale.set_bar_spacing(((scale.width() - 1.0) as f64 / (to - from)) as f32);
        scale.set_right_offset((to - scale.base_idx() as f64 - 0.5) as f32);
        self.tracking_mode_active = false;
        self.kinetic_scroll = super::state::KineticScrollState::new();
        true
    }

    // =========================================================================
    // Multi-Chart Sync Methods
    // =========================================================================

    /// Sets a crosshair in local index coordinates, counting elements on price-based types.
    /// `None` clears it. Use [`Self::set_synced_crosshair_time`] between independent charts.
    pub fn set_synced_crosshair_bar_idx(&mut self, bar_idx: Option<f64>) {
        self.synced_crosshair_bar_idx = bar_idx;
    }

    /// Returns the nearest hovered local index from the last frame, counting elements on
    /// price-based types. Use [`Self::hover_time`] to exchange positions between charts.
    pub fn get_hover_bar_idx(&self) -> Option<f64> {
        self.last_hover_bar_idx
    }

    /// Restores local bar spacing and right offset, for example after inspecting another view.
    /// These coordinates depend on the data and plot width. Use [`Self::apply_synced_time_range`]
    /// between independent charts.
    pub fn apply_synced_time_scale(&mut self, bar_spacing: f32, right_offset: f32) {
        self.state.time_scale_mut().set_bar_spacing(bar_spacing);
        self.state.time_scale_mut().set_right_offset(right_offset);
    }

    /// Local time-scale state as `(bar_spacing, right_offset)`, for view restoration.
    /// Use [`Self::visible_time_range`] to exchange viewports between independent charts.
    pub fn get_time_scale_state(&self) -> (f32, f32) {
        (
            self.state.time_scale().bar_spacing(),
            self.state.time_scale().right_offset(),
        )
    }
}

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
    (left.saturating_sub(1), index)
}

pub(super) fn time_at(bars: &[Bar], index: f64) -> Option<DateTime<Utc>> {
    if bars.is_empty() || !index.is_finite() {
        return None;
    }
    let at = index.floor().clamp(0.0, (bars.len() - 1) as f64) as usize;
    let (left, right) = segment(bars, at);
    let step = if bars[left].time == bars[right].time {
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

pub(super) fn index_at(bars: &[Bar], time: DateTime<Utc>) -> Option<f64> {
    bars.first()?;
    let at = bars
        .partition_point(|bar| bar.time < time)
        .min(bars.len() - 1);
    if bars[at].time == time {
        return Some(at as f64);
    }
    let (left, right) = segment(bars, at.saturating_sub(1));
    let step = if bars[left].time == bars[right].time {
        1.0
    } else {
        seconds(bars[right].time - bars[left].time) / (right - left) as f64
    };
    Some(left as f64 + seconds(time - bars[left].time) / step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarData;

    fn chart(times: &[i64], width: f32) -> Chart {
        let bars = times
            .iter()
            .map(|&t| Bar::new(time(t), 10.0, 11.0, 9.0, 10.0, 1.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.state.time_scale_mut().set_width(width);
        chart
    }

    fn time(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn sync_resolves_receiver_times_and_fractional_edges() {
        // 120 seconds is source bar 2, receiver bar 1; 180 is halfway across its session gap.
        for (times, hovered, expected) in [
            (vec![60, 120, 240, 300], Some(120), Some(1.0)),
            (vec![60, 120, 240, 300], Some(180), Some(1.0)),
            (vec![60, 120, 120, 300], Some(120), Some(1.0)),
            (vec![60, 120, 240, 300], Some(0), None),
            (vec![60, 120, 240, 300], Some(360), None),
            (vec![], Some(120), None),
            (vec![120], None, None),
        ] {
            let mut source = chart(&[0, 60, 120, 180, 240, 300], 1000.0);
            source.last_hover_bar_idx = hovered.map(|t| t as f64 / 60.0);
            let mut receiver = chart(&times, 501.0);
            receiver.set_synced_crosshair_time(source.hover_time());
            assert_eq!(
                receiver.synced_crosshair_bar_idx, expected,
                "{times:?} {hovered:?}"
            );
        }
        let mut source = chart(&[0, 60, 120, 180, 240, 300], 1001.0);
        // Pixel edges at source indices 0.5 and 4.5 represent 30 and 270 seconds.
        source.apply_synced_time_scale(250.0, -1.0);
        let range = source.visible_time_range().unwrap();
        assert_eq!(
            range,
            VisibleTimeRange {
                from: time(30),
                to: time(270)
            }
        );
        for (times, expected_spacing, expected_offset) in [
            (vec![60, 120, 240, 300], 500.0 / 3.0, -1.0),
            (vec![0, 120, 240, 360], 250.0, -1.25),
        ] {
            let mut receiver = chart(&times, 501.0);
            assert!(receiver.apply_synced_time_range(range));
            let (spacing, offset) = receiver.get_time_scale_state();
            assert!((spacing - expected_spacing).abs() < 0.001);
            assert!((offset - expected_offset).abs() < 0.001);
            let actual = receiver.visible_time_range().unwrap();
            assert!((actual.from - range.from).num_milliseconds().abs() < 1);
            assert!((actual.to - range.to).num_milliseconds().abs() < 1);
            let before = receiver.get_time_scale_state();
            assert!(!receiver.apply_synced_time_range(VisibleTimeRange {
                from: range.to,
                to: range.from
            }));
            assert_eq!(receiver.get_time_scale_state(), before);
        }
        assert!(!chart(&[], 501.0).apply_synced_time_range(range));
        // Execution bars need sub-millisecond precision; no epoch-sized float conversion.
        let mut fine = chart(&[0, 1], 501.0);
        fine.state.set_data(BarData::from_bars(
            [0, 100, 200]
                .map(|ns| {
                    Bar::new(
                        time(1_700_000_000) + Duration::nanoseconds(ns),
                        10.0,
                        11.0,
                        9.0,
                        10.0,
                        1.0,
                    )
                })
                .to_vec(),
        ));
        let t = time(1_700_000_000) + Duration::nanoseconds(150);
        assert_eq!(time_at(fine.state.shown(), 1.5), Some(t));
        assert_eq!(index_at(fine.state.shown(), t), Some(1.5));
    }
}
