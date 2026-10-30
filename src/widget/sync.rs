//! Multi-chart crosshair and time-scale synchronization.

use super::Chart;
use chrono::{DateTime, Utc};

// Keep a short received span readable even when manual zoom has no configured maximum.
const SYNC_MAX_BAR_SPACING: f64 = 50.0;

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
        self.synced_crosshair_price = None;
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
            Some(bars.partition_point(|bar| bar.time < bars[index].time) as f64)
        });
    }

    /// Shows a crosshair at the nearest local bar to `time`, as
    /// [`Self::set_synced_crosshair_time`] does, with its horizontal line and price label at
    /// `price` on the main price scale. `None`, a time outside the data or a non-finite price
    /// clears it. The pointer's own crosshair takes its place while the pointer is over the plots.
    pub fn set_crosshair_position(&mut self, position: Option<(DateTime<Utc>, f64)>) {
        let position = position.filter(|(_, price)| price.is_finite());
        self.set_synced_crosshair_time(position.map(|(time, _)| time));
        self.synced_crosshair_price = position
            .filter(|_| self.synced_crosshair_bar_idx.is_some())
            .map(|(_, price)| price);
    }

    /// The visible UTC range, including whitespace mapped by [`Self::index_to_time`].
    /// Empty charts return `None`.
    /// Between bars, time is interpolated, so session gaps retain fractional edge positions.
    pub fn visible_time_range(&self) -> Option<VisibleTimeRange> {
        let scale = self.state.time_scale();
        // Keep fractional indices in f64: a rounded edge can move minutes across a session gap.
        Some(VisibleTimeRange {
            from: self.index_to_time(scale.coord_to_idx_precise(0.0, 0.0, scale.width()))?,
            to: self.index_to_time(scale.coord_to_idx_precise(
                scale.width() - 1.0,
                0.0,
                scale.width(),
            ))?,
        })
    }

    /// Fits the receiver's bars to a UTC range using its own plot width.
    /// Returns `false` without changing the view for empty data, reversed or zero ranges,
    /// a range outside the loaded bar slots, or endpoints mapping to the same element.
    /// Spacing stops at the configured maximum, or 50 pixels when unlimited, centring short
    /// spans. The minimum spacing and local edge constraints still apply.
    pub fn apply_synced_time_range(&mut self, range: VisibleTimeRange) -> bool {
        self.fit_time_range(range, SYNC_MAX_BAR_SPACING)
    }

    /// As [`apply_synced_time_range`](Self::apply_synced_time_range) for a range from a chart
    /// of interval `source`; only a different interval falls back to the 50-pixel cap.
    pub(crate) fn apply_synced_time_range_from(
        &mut self,
        range: VisibleTimeRange,
        source: Option<crate::model::Timeframe>,
    ) -> bool {
        let cap = if source == self.timeframe() {
            f64::INFINITY
        } else {
            SYNC_MAX_BAR_SPACING
        };
        self.fit_time_range(range, cap)
    }

    fn fit_time_range(&mut self, range: VisibleTimeRange, default_max: f64) -> bool {
        if range.from >= range.to {
            return false;
        }
        let Some((from, to)) = self
            .time_to_index(range.from)
            .zip(self.time_to_index(range.to))
        else {
            return false;
        };
        let scale = self.state.time_scale_mut();
        if to <= from || scale.width() <= 1.0 || to < -0.5 || from > scale.base_idx() as f64 + 0.5 {
            return false;
        }
        scale.fit_logical_range(from, to, default_max);
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
        self.synced_crosshair_price = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, index_at, time_at};
    use chrono::Duration;

    fn chart(times: &[i64], width: f32) -> Chart {
        let bars = times
            .iter()
            .map(|&t| Bar::new(time(t), 10.0, 11.0, 9.0, 10.0, 1.0))
            .collect();
        let mut chart = Chart::new(BarData::new());
        // Shown price-based elements can repeat a timestamp, unlike cleaned input bars.
        chart.state.set_data(BarData::from_bars(bars));
        chart.state.time_scale_mut().set_width(width);
        chart
    }

    fn time(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn sync_keeps_coarse_receivers_readable() {
        let times: Vec<_> = (0..24).map(|i| i * 3600).collect();
        let last = 23 * 3600;
        for (from, to, maximum, expected) in [
            (last + 70 * 60, last + 170 * 60, 0.0, None),
            (-170 * 60, -70 * 60, 0.0, None),
            (last + 10 * 60, last + 50 * 60, 0.0, Some(50.0)),
            (last + 10 * 60, last + 50 * 60, 25.0, Some(25.0)),
            (last + 10 * 60, last + 50 * 60, 100.0, Some(100.0)),
            (-3600 * 2000, last, 0.0, Some(0.5)),
        ] {
            let mut receiver = chart(&times, 501.0);
            receiver.state.time_scale_mut().set_max_bar_spacing(maximum);
            receiver.tracking_mode_active = true;
            let before = receiver.get_time_scale_state();
            assert_eq!(
                receiver.apply_synced_time_range(VisibleTimeRange {
                    from: time(from),
                    to: time(to),
                }),
                expected.is_some(),
                "{from}..{to}, max {maximum}"
            );
            let Some(spacing) = expected else {
                assert_eq!(receiver.get_time_scale_state(), before);
                assert!(receiver.tracking_mode_active);
                continue;
            };
            let scale = receiver.state.time_scale();
            assert!((scale.bar_spacing() - spacing).abs() < 1e-4);
            let visible = scale.visible_logical_range();
            assert!(visible.left.ceil().max(0.0) <= visible.right.floor().min(23.0));
            assert!(!receiver.tracking_mode_active);
            if from == last + 10 * 60 {
                // The midpoint is index 23.5; 500 px / 50 px spans ten hourly slots.
                let center = scale.idx_to_coord_precise(23.5, 0.0, 501.0);
                assert!((center - 250.0).abs() < 1e-4);
            } else {
                assert_eq!(receiver.visible_time_range().unwrap().to, time(to));
            }
        }
    }

    #[test]
    fn sync_resolves_receiver_times_and_fractional_edges() {
        // 120 seconds is source bar 2, receiver bar 1; 180 is halfway across its session gap.
        for (times, hovered, expected) in [
            (vec![60, 120, 240, 300], Some(120), Some(1.0)),
            (vec![60, 120, 240, 300], Some(180), Some(1.0)),
            (vec![60, 120, 120, 300], Some(120), Some(1.0)),
            (vec![60, 120, 120, 300], Some(180), Some(1.0)),
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
        let repeated = chart(&[0, 60, 60, 120], 501.0);
        for (index, seconds) in [(0.5, 30), (1.5, 60), (2.5, 90), (3.5, 150)] {
            assert_eq!(time_at(repeated.state.shown(), index), Some(time(seconds)));
        }
        // Repeated trailing times still extend the same last segment in both directions:
        // index 3.5 is 60 + (3.5 - 2) * (60 / 2) = 105 seconds.
        let trailing = chart(&[0, 60, 60], 501.0);
        assert_eq!(time_at(trailing.state.shown(), 3.5), Some(time(105)));
        assert_eq!(index_at(trailing.state.shown(), time(105)), Some(3.5));
        #[cfg(feature = "drawings")]
        {
            let mut repeated = repeated;
            let mut drawing =
                crate::drawings::Drawing::new(7, crate::drawings::DrawingToolType::TrendLine);
            drawing
                .chart_points
                .push(crate::drawings::ChartPoint::new(1.5, 10.0));
            repeated.drawings.drawings.push(drawing);
            repeated.anchor_drawing_points();
            repeated.project_drawing_points();
            assert_eq!(repeated.drawings.drawings[0].chart_points[0].bar_idx, 1.5);
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
            receiver.state.time_scale_mut().set_max_bar_spacing(5000.0);
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
        // A 65.5-hour closure: the requested edges are 8 and 32 hours (less one second)
        // after its first bar, at fractions 28_800/235_800 and 115_199/235_800.
        let mut times: Vec<i64> = (0..=4096).map(|index| index * 60).collect();
        let start = *times.last().unwrap();
        times.extend((0..1024).map(|index| start + 235_800 + index * 60));
        let range = VisibleTimeRange {
            from: time(start + 28_800),
            to: time(start + 115_199),
        };
        for width in [251.0, 592.125] {
            let mut receiver = chart(&times, width);
            receiver.state.time_scale_mut().set_max_bar_spacing(5000.0);
            assert!(receiver.apply_synced_time_range(range));
            let actual = receiver.visible_time_range().unwrap();
            assert!((actual.from - range.from).num_milliseconds().abs() < 1);
            assert!((actual.to - range.to).num_milliseconds().abs() < 1);
        }
        // A day inside a three-day session gap spans one third of a bar, even after long history.
        for count in [2, 20_000] {
            let mut times: Vec<_> = (0..count).map(|i| i * 60).collect();
            let last = times[count as usize - 1];
            times.push(last + 3 * 86_400);
            let mut receiver = chart(&times, 1001.0);
            receiver.state.time_scale_mut().set_max_bar_spacing(5000.0);
            let range = VisibleTimeRange {
                from: time(last + 86_400),
                to: time(last + 2 * 86_400),
            };
            assert!(receiver.apply_synced_time_range(range));
            let actual = receiver.visible_time_range().unwrap();
            assert!((actual.from - range.from).num_milliseconds().abs() < 10);
            assert!((actual.to - range.to).num_milliseconds().abs() < 10);
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
