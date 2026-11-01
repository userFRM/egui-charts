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
        self.hover_period().map(|(open, _)| open)
    }

    /// The hovered bar's period, from its open until the next bar of its interval opens.
    pub(crate) fn hover_period(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let index = self.last_hover_bar_idx?.round();
        (index >= 0.0 && index < self.state.shown().len() as f64)
            .then(|| self.bar_period(index as usize))
    }

    // Until the next bar opens if that comes sooner, as after a repeated element time.
    fn bar_period(&self, index: usize) -> (DateTime<Utc>, DateTime<Utc>) {
        let bars = self.state.shown();
        let open = bars[index].time;
        // Without an interval, the first bar takes the spacing after the second.
        let at = match self.timeframe() {
            Some(_) => index,
            None => index.max(1).min(bars.len() - 1),
        };
        // One slot past the bar as if it were the last, so a session gap after it is no interval.
        let step = crate::model::time_at_with_timeframe(
            &bars[..=at],
            at as f64 + 1.0,
            self.timeframe(),
            self.calendar_timezone().as_ref(),
        )
        .map_or(chrono::TimeDelta::zero(), |end| end - bars[at].time);
        let end = open.checked_add_signed(step).unwrap_or(open);
        let end = bars.get(index + 1).map_or(end, |next| end.min(next.time));
        (open, end.max(open))
    }

    /// Shows a crosshair at the local bar whose period holds `time`, from its open until the
    /// next bar of its interval opens. Between bars it marks the nearest one; ties choose the
    /// earlier bar. Times before the first bar or after the last bar's period, and `None`, clear
    /// it. Repeated element times choose the first element. Each receiver resolves the timestamp
    /// against its own series, including after bars are evicted or older history is loaded.
    pub fn set_synced_crosshair_time(&mut self, time: Option<DateTime<Utc>>) {
        self.set_synced_crosshair(time.map(|time| (time, time)), None);
    }

    /// Shows a crosshair at the local bar for `time`, as [`Self::set_synced_crosshair_time`]
    /// does, with its horizontal line and price label at `price` on the main price scale.
    /// `None`, a time outside the data or a non-finite price clears it. The pointer's own
    /// crosshair takes its place while the pointer is over the plots.
    pub fn set_crosshair_position(&mut self, position: Option<(DateTime<Utc>, f64)>) {
        self.set_synced_crosshair(
            position.map(|(time, _)| (time, time)),
            position.map(|(_, price)| price),
        );
    }

    /// As [`Self::set_crosshair_position`] for a source bar's `period`: when no local bar's
    /// period holds its start, the first local bar opening inside it is marked, so a coarse bar
    /// marks the first bar of its period on a finer chart closed at its open.
    pub(crate) fn set_synced_crosshair(
        &mut self,
        period: Option<(DateTime<Utc>, DateTime<Utc>)>,
        price: Option<f64>,
    ) {
        let period = period.filter(|_| price.is_none_or(f64::is_finite));
        self.synced_crosshair_bar_idx = period.and_then(|(from, to)| self.synced_index(from, to));
        self.synced_crosshair_period = period.filter(|_| self.synced_crosshair_bar_idx.is_some());
        self.synced_crosshair_price = price.filter(|_| self.synced_crosshair_bar_idx.is_some());
    }

    fn synced_index(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Option<f64> {
        let bars = self.state.shown();
        let after = bars.partition_point(|bar| bar.time <= from);
        let index = if after > 0 && from < self.bar_period(after - 1).1 {
            after - 1
        } else if after < bars.len() && bars[after].time < to {
            after
        } else if after == 0 || after == bars.len() {
            return None;
        } else if from - bars[after - 1].time <= bars[after].time - from {
            after - 1
        } else {
            after
        };
        Some(bars.partition_point(|bar| bar.time < bars[index].time) as f64)
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
    /// a range that ends before the first bar's slot or starts after the last bar's period,
    /// or endpoints mapping to the same element.
    /// Spacing stops at the configured maximum, or 50 pixels when unlimited, centring short
    /// spans. The minimum spacing and local edge constraints still apply, and the fit keeps a
    /// bar in view: a range longer than the plot at the minimum spacing shows the newest bars.
    pub fn apply_synced_time_range(&mut self, range: VisibleTimeRange) -> bool {
        self.apply_synced_time_range_from(range, None)
    }

    /// As [`apply_synced_time_range`](Self::apply_synced_time_range) for a range from a chart
    /// with the interval and plot slots of [`Self::sync_source`]. A receiver of that interval
    /// may exceed the 50-pixel cap up to the spacing at which its plot spans the source's slot
    /// count, so charts with the same bar slots zoom together, while one with fewer slots in
    /// the range, as across a closure the source trades through, stays readable.
    pub(crate) fn apply_synced_time_range_from(
        &mut self,
        range: VisibleTimeRange,
        source: Option<(crate::model::Timeframe, f64)>,
    ) -> bool {
        let cap = match source {
            Some((timeframe, slots)) if Some(timeframe) == self.timeframe() => {
                SYNC_MAX_BAR_SPACING.max((f64::from(self.state.time_scale().width()) - 1.0) / slots)
            }
            _ => SYNC_MAX_BAR_SPACING,
        };
        self.fit_time_range(range, cap)
    }

    /// The interval and the bar slots across the plot, `None` without an interval.
    pub(crate) fn sync_source(&self) -> Option<(crate::model::Timeframe, f64)> {
        let scale = self.state.time_scale();
        let edge = |x| scale.coord_to_idx_precise(x, 0.0, scale.width());
        Some((self.timeframe()?, edge(scale.width() - 1.0) - edge(0.0)))
    }

    /// As [`Self::apply_synced_time_range`] with `default_max` in place of the 50-pixel cap.
    pub(super) fn fit_time_range(&mut self, range: VisibleTimeRange, default_max: f64) -> bool {
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
        // The last bar's period runs to the next slot at `base_idx + 1`.
        if to <= from || scale.width() <= 1.0 || to < -0.5 || from >= scale.base_idx() as f64 + 1.0
        {
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
        self.synced_crosshair_period = None;
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

    #[test]
    fn synced_crosshair_keeps_its_time_when_history_changes() {
        use crate::widget::test_frames::frame;
        use crate::widget::{ImageExportOptions, SeriesId};

        let bar = |minute: i64| Bar::new(time(minute * 60), 10.0, 11.0, 9.0, 10.0, 1.0);
        for price in [Some(10.5), None] {
            for (prepend, minute, expected_index) in [
                (false, 10, Some(7.0)),
                (true, 10, Some(14.0)),
                (false, 1, None),
            ] {
                let mut chart = Chart::new(BarData::from_bars((0..16).map(bar).collect()));
                if !prepend {
                    chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(16));
                }
                let ctx = egui::Context::default();
                frame(&ctx, &mut chart, 0.0, Vec::new());
                let selected = time(minute * 60);
                match price {
                    Some(price) => chart.set_crosshair_position(Some((selected, price))),
                    None => chart.set_synced_crosshair_time(Some(selected)),
                }
                frame(&ctx, &mut chart, 0.1, Vec::new());
                assert_eq!(chart.last_crosshair, Some((selected, price)));
                if prepend {
                    for minute in -4..0 {
                        chart.update(SeriesId::MAIN, bar(minute));
                    }
                    assert_eq!(chart.bars(SeriesId::MAIN)[0].time, time(-240));
                } else {
                    chart.update(SeriesId::MAIN, bar(16));
                    assert_eq!(chart.bars(SeriesId::MAIN)[0].time, time(180));
                }
                let expected = expected_index.map(|_| (selected, price));
                // Export both before the next screen frame and after it.
                for pass in 0..2 {
                    if pass == 1 {
                        frame(&ctx, &mut chart, 0.2, Vec::new());
                        assert_eq!(chart.synced_crosshair_bar_idx, expected_index);
                        assert_eq!(chart.last_crosshair, expected);
                    }
                    let options = ImageExportOptions {
                        include_crosshair: true,
                        ..Default::default()
                    };
                    chart
                        .export_layout_svg(&ctx, &options, |ui, snapshot| {
                            snapshot.show(ui);
                            assert_eq!(snapshot.last_crosshair, expected);
                        })
                        .unwrap();
                }
                // An explicit index replaces the time anchor and keeps its local semantics.
                chart.set_synced_crosshair_bar_idx(Some(5.0));
                chart.update(SeriesId::MAIN, bar(-5));
                frame(&ctx, &mut chart, 0.3, Vec::new());
                assert_eq!(chart.synced_crosshair_bar_idx, Some(5.0));
                assert_eq!(
                    chart.last_crosshair,
                    Some((chart.state.shown()[5].time, None))
                );
            }
        }
    }

    fn time(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn sync_keeps_coarse_receivers_readable() {
        let hourly: Vec<_> = (0..24).map(|i| i * 3600).collect();
        let daily: Vec<_> = (0..30).map(|i| i * 86_400).collect();
        let minutes: Vec<_> = (0..1000).map(|i| i * 60).collect();
        // A 65.5-hour closure before the last minute.
        let mut closed = minutes.clone();
        closed.push(999 * 60 + 235_800);
        // Minutes from the last bar's open, and the time at the last plot pixel. A receiver
        // without an interval takes a range as from a source without one; given the slots of a
        // one-minute source, the receiver is a one-minute chart.
        for (times, slots, from, to, maximum, expected, right) in [
            (&hourly, None, 70, 170, 0.0, None, None),
            (&hourly, None, -1550, -1450, 0.0, None, None),
            // Centred on index 23.5, 50 px per slot shows five slots each side.
            (&hourly, None, 10, 50, 0.0, Some(50.0), Some(330.0)),
            (&hourly, None, 10, 50, 25.0, Some(25.0), Some(630.0)),
            (&hourly, None, 10, 50, 100.0, Some(100.0), Some(180.0)),
            (&hourly, None, -121_380, 0, 0.0, Some(0.5), Some(0.0)),
            // The second half of the last bar's period.
            (&hourly, None, 35, 55, 0.0, Some(50.0), Some(345.0)),
            (&daily, None, 1050, 1170, 0.0, Some(50.0), Some(8310.0)),
            // Half a slot across the plot keeps the last bar at its centre.
            (&hourly, None, 35, 55, 1000.0, Some(1000.0), Some(15.0)),
            // 90 days ending three days after the last minute show the newest minutes.
            (&minutes, None, -125_280, 4320, 0.0, Some(0.5), Some(0.5)),
            // Four hours of the closure are 0.06 of a slot here, but 240 at the source.
            (&closed, Some(240.0), -3330, -3090, 0.0, Some(50.0), None),
        ] {
            let last_open = *times.last().unwrap();
            let (from, to) = (last_open + from * 60, last_open + to * 60);
            let mut receiver = chart(times, 501.0);
            let source = slots.map(|slots| (crate::model::Timeframe::Min1, slots));
            receiver.timeframe = source.map(|(timeframe, _)| timeframe);
            receiver.state.time_scale_mut().set_max_bar_spacing(maximum);
            receiver.tracking_mode_active = true;
            let before = receiver.get_time_scale_state();
            assert_eq!(
                receiver.apply_synced_time_range_from(
                    VisibleTimeRange {
                        from: time(from),
                        to: time(to),
                    },
                    source,
                ),
                expected.is_some(),
                "{from}..{to}, max {maximum}"
            );
            let Some(spacing) = expected else {
                assert_eq!(receiver.get_time_scale_state(), before);
                assert!(receiver.tracking_mode_active);
                continue;
            };
            let scale = receiver.state.time_scale();
            assert!((scale.bar_spacing() - spacing).abs() < 1e-4, "{from}..{to}");
            let edge = |x| scale.coord_to_idx_precise(x, 0.0, 501.0);
            let (left, last) = (edge(0.0), (times.len() - 1) as f64);
            assert!(
                left.ceil().max(0.0) <= edge(500.0).floor().min(last),
                "{from}..{to}"
            );
            assert!(!receiver.tracking_mode_active);
            if let Some(right) = right {
                let actual = receiver.visible_time_range().unwrap().to;
                let expected = time(last_open + (right * 60.0) as i64);
                assert!(
                    (actual - expected).num_milliseconds().abs() < 1,
                    "{from}..{to}"
                );
            }
        }
    }

    #[test]
    fn sync_resolves_receiver_times_and_fractional_edges() {
        const H: i64 = 3600;
        const D: i64 = 86_400;
        let minutes = vec![0, 60, 120, 180, 240, 300];
        let (gap, doubled) = (vec![60, 120, 240, 300], vec![60, 120, 120, 300]);
        let days: Vec<_> = (0..30).map(|i| i * D).collect();
        let four_hours: Vec<_> = (0..6).map(|i| i * 4 * H).collect();
        // Sessions from 13:30 to 19:59 on days 0 and 1.
        let sessions: Vec<_> = (0..2)
            .flat_map(|day| (0..390).map(move |i| day * D + 13 * H + 1800 + i * 60))
            .collect();
        // A source of one bar has a period of one second.
        let one = |time| vec![time];
        // 120 seconds is source bar 2, receiver bar 1; 180 is halfway across its session gap.
        for (source, hovered, times, expected) in [
            (&minutes, Some(2.0), &gap, Some(1.0)),
            (&minutes, Some(3.0), &gap, Some(1.0)),
            (&minutes, Some(2.0), &doubled, Some(1.0)),
            (&minutes, Some(3.0), &doubled, Some(1.0)),
            (&minutes, Some(0.0), &gap, None),
            (&minutes, Some(6.0), &gap, None),
            (&minutes, Some(2.0), &Vec::new(), None),
            (&minutes, None, &one(120), None),
            // A time inside a coarser bar's period marks that bar, the latest one included.
            (&one(10 * D + 14 * H + 1800), Some(0.0), &days, Some(10.0)),
            (&one(29 * D + 14 * H + 1800), Some(0.0), &days, Some(29.0)),
            (&one(30 * D + 1800), Some(0.0), &days, None),
            (&one(3 * H), Some(0.0), &four_hours, Some(0.0)),
            // The daily bar of day 1 marks its first minute, not the last minute of day 0.
            (&days, Some(1.0), &sessions, Some(390.0)),
        ] {
            let mut source = chart(source, 1000.0);
            source.last_hover_bar_idx = hovered;
            let mut receiver = chart(times, 501.0);
            receiver.set_synced_crosshair(source.hover_period(), None);
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
