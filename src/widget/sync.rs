//! Multi-chart crosshair and time-scale synchronization.

use super::Chart;
use chrono::{DateTime, Utc};

// Keep a short received span readable even when manual zoom has no configured maximum.
const SYNC_MAX_BAR_SPACING: f64 = 50.0;

pub(super) type CrosshairPeriod = (DateTime<Utc>, DateTime<Utc>);

/// UTC timestamps at the left and right edges of a chart's plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct VisibleTimeRange {
    /// Time at the leftmost plot pixel.
    pub from: DateTime<Utc>,
    /// Time at the rightmost plot pixel.
    pub to: DateTime<Utc>,
}

impl VisibleTimeRange {
    /// The range from `from` at the leftmost plot pixel to `to` at the rightmost.
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        Self { from, to }
    }
}

/// The bar at the left edge when the interval changed, kept by
/// [`TimeScaleOptions::keep_left_edge_on_interval_change`](crate::config::TimeScaleOptions::keep_left_edge_on_interval_change).
/// Its open is the pending scroll time.
#[derive(Debug, Clone, Copy)]
pub(super) struct LeftEdge {
    /// The end of the bar's period.
    end: DateTime<Utc>,
    /// Where the left edge cut the bar's slot, from -0.5 at its left side to 0.5 at its right.
    offset: f64,
    /// The bars held at the change, which the new interval's bars replace.
    pub(super) bars: super::layout::BarsKey,
}

impl Chart {
    /// The time of the nearest eligible hovered point, the start of the hovered whitespace slot
    /// after the last bar, or `None` elsewhere outside the data.
    pub fn hover_time(&self) -> Option<DateTime<Utc>> {
        self.hover_period().map(|(open, _)| open)
    }

    /// The start of the whitespace slot at `index` after the last bar, extrapolated as in
    /// [`Self::index_to_time`]; `None` at or before the last bar.
    pub(super) fn whitespace_time(&self, index: f64) -> Option<DateTime<Utc>> {
        (index >= self.state.shown().len() as f64)
            .then(|| self.index_to_time(index))
            .flatten()
    }

    /// The hovered bar's period, an independently keyed point's exact timestamp as both bounds,
    /// or the start of the hovered whitespace slot after the last bar as both bounds.
    pub(crate) fn hover_period(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let Some(index) = self.last_hover_bar_idx else {
            return self.last_hover_whitespace.map(|time| (time, time));
        };
        if index.fract() != 0.0 || index < 0.0 || index >= self.state.shown().len() as f64 {
            let time = self.last_crosshair?.0;
            return Some((time, time));
        }
        Some(self.bar_period(index as usize))
    }

    // Until the next bar opens if that comes sooner, as after a repeated element time.
    fn bar_period(&self, index: usize) -> (DateTime<Utc>, DateTime<Utc>) {
        let bars = self.state.shown();
        let open = bars[index].time;
        // Without an interval, the first bar takes the spacing after the second.
        let timeframe = self.calendar_timeframe();
        let at = match timeframe {
            Some(_) => index,
            None => index.max(1).min(bars.len() - 1),
        };
        // One slot past the bar as if it were the last, so a session gap after it is no interval.
        let step = crate::model::time_at_with_timeframe(
            &bars[..=at],
            at as f64 + 1.0,
            timeframe,
            self.calendar_timezone().as_ref(),
        )
        .map_or(chrono::TimeDelta::zero(), |end| end - bars[at].time);
        let end = open.checked_add_signed(step).unwrap_or(open);
        let end = bars.get(index + 1).map_or(end, |next| end.min(next.time));
        (open, end.max(open))
    }

    /// Shows a crosshair at the local bar whose period holds `time`, from its open until the
    /// next bar of its interval opens, with the vertical guide and time label alone. Between
    /// bars it marks the nearest one; ties choose the earlier bar. A time after the last bar's
    /// period marks the whitespace slot that holds it, stepped by the chart's own interval or
    /// bar spacing, and is drawn while that slot is in view. Times before the first bar and
    /// `None` clear it. Repeated element times choose the first element. Each receiver resolves
    /// the timestamp against its own series, including after bars are evicted or older history
    /// is loaded.
    /// An exact added or custom point between main bars keeps its fractional position.
    /// Inside a [`super::ChartLayout`] the position stays until it is cleared or replaced, and
    /// returns after a linked pointer leaves.
    pub fn set_synced_crosshair_time(&mut self, time: Option<DateTime<Utc>>) {
        self.set_synced_crosshair(time.map(|time| (time, time)), None);
        self.host_crosshair = self
            .synced_crosshair_period
            .map(|period| (period, None, None));
    }

    /// Shows a crosshair at the local bar for `time`, as [`Self::set_synced_crosshair_time`]
    /// does, with its horizontal line and price label at `price` on the main price scale.
    /// `None`, a time before the first bar or a non-finite price clears it. The pointer's own
    /// crosshair takes its place while the pointer is over the plots.
    /// Use [`Self::set_series_crosshair_position`] to target another bar series or pane.
    pub fn set_crosshair_position(&mut self, position: Option<(DateTime<Utc>, f64)>) {
        self.set_synced_crosshair(
            position.map(|(time, _)| (time, time)),
            position.map(|(_, price)| price),
        );
        self.host_crosshair = self
            .synced_crosshair_period
            .map(|period| (period, self.synced_crosshair_price, None));
        if self.synced_crosshair_bar_idx.is_none() && self.last_hover_bar_idx.is_none() {
            self.last_crosshair = None;
        }
    }

    /// Shows a crosshair at `time` in the raw price units and current pane of the bar series
    /// `series`, including [`super::SeriesId::MAIN`]. Time resolution follows
    /// [`Self::set_synced_crosshair_time`]; the target's scale mode, inversion and formatter
    /// apply without price snapping. Moving the series or changing its scale updates the
    /// position on the next frame. Hidden axes have no price label.
    ///
    /// `None`, a non-finite price, a time before the chart's first bar or an unknown or removed
    /// target clears the position. A hidden series or collapsed pane retains the position;
    /// its horizontal line and price label wait until it is shown. Offscreen prices have
    /// only the time line and label. The pointer takes precedence over all host positions
    /// while over any plot, and the saved position returns when it leaves.
    pub fn set_series_crosshair_position(
        &mut self,
        series: super::SeriesId,
        position: Option<(DateTime<Utc>, f64)>,
    ) {
        self.set_crosshair_position(position.filter(|_| self.series_scale(series).is_some()));
        self.synced_crosshair_series = self.synced_crosshair_bar_idx.map(|_| series);
        self.host_crosshair = self
            .host_crosshair
            .map(|(period, price, _)| (period, price, self.synced_crosshair_series));
    }

    /// Restores the host's request after a linked pointer leaves, resolving its time again.
    pub(crate) fn restore_host_crosshair(&mut self) {
        let (period, price, series) = self
            .host_crosshair
            .map_or((None, None, None), |(period, price, series)| {
                (Some(period), price, series)
            });
        self.set_synced_crosshair_for_series(series, period, price);
    }

    /// As [`Self::set_crosshair_position`] for a source bar's `period`: when no local bar's
    /// period holds its start, the first local bar opening inside it is marked, so a coarse bar
    /// marks the first bar of its period on a finer chart closed at its open.
    pub(crate) fn set_synced_crosshair(
        &mut self,
        period: Option<(DateTime<Utc>, DateTime<Utc>)>,
        price: Option<f64>,
    ) {
        self.set_synced_crosshair_for_series(None, period, price);
    }

    /// As [`Self::set_synced_crosshair`], with the price read on the scale of the bar series
    /// `series`; a series that is not present clears the position.
    pub(super) fn set_synced_crosshair_for_series(
        &mut self,
        series: Option<super::SeriesId>,
        period: Option<(DateTime<Utc>, DateTime<Utc>)>,
        price: Option<f64>,
    ) {
        let period = period.filter(|_| {
            price.is_none_or(f64::is_finite)
                && series.is_none_or(|id| self.series_scale(id).is_some())
        });
        self.synced_crosshair_bar_idx = period.and_then(|(from, to)| self.synced_index(from, to));
        self.synced_crosshair_period = period.filter(|_| self.synced_crosshair_bar_idx.is_some());
        self.synced_crosshair_price = price.filter(|_| self.synced_crosshair_bar_idx.is_some());
        self.synced_crosshair_series = series.filter(|_| self.synced_crosshair_bar_idx.is_some());
    }

    fn synced_index(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Option<f64> {
        let bars = self.state.shown();
        let added_point = self.price_series.iter().any(|item| {
            item.bars
                .binary_search_by_key(&from, |bar| bar.time)
                .is_ok()
        }) || self.custom_series().any(|item| {
            item.shown(&item.keys())
                .binary_search_by_key(&from, |key| key.time)
                .is_ok()
        });
        if added_point && let Some(index) = crate::model::index_at(bars, from) {
            return Some(index);
        }
        let after = bars.partition_point(|bar| bar.time <= from);
        let index = if after > 0 && from < self.bar_period(after - 1).1 {
            after - 1
        } else if after < bars.len() && bars[after].time < to {
            after
        } else if after == 0 {
            return None;
        } else if after == bars.len() {
            // The slot holding the time; the tolerance absorbs rounding at a slot's start.
            return self
                .time_to_index(from)
                .map(|index| (index + 1e-9).floor().max(bars.len() as f64));
        } else if from - bars[after - 1].time <= bars[after].time - from {
            after - 1
        } else {
            after
        };
        Some(bars.partition_point(|bar| bar.time < bars[index].time) as f64)
    }

    /// The open of the bar at the left edge and how to keep it there, `None` without bars or
    /// before the first laid-out frame.
    pub(super) fn left_edge(&self) -> Option<(DateTime<Utc>, LeftEdge)> {
        let shown = self.state.shown();
        let scale = self.state.time_scale();
        let left = scale.coord_to_idx_precise(0.0, 0.0, scale.width());
        if shown.is_empty() || self.prev_width.is_none() || !left.is_finite() {
            return None;
        }
        let index = (left + 0.5).floor().clamp(0.0, (shown.len() - 1) as f64);
        let (open, end) = self.bar_period(index as usize);
        let offset = (left - index).clamp(-0.5, 0.5);
        let bars = super::layout::bars_key(self);
        Some((open, LeftEdge { end, offset, bars }))
    }

    /// Puts the bar of the shown interval for the kept left edge at the left of the plot,
    /// keeping the bar spacing; the oldest bar while older history is loading.
    pub(super) fn keep_left_edge(&mut self, open: DateTime<Utc>, edge: LeftEdge) {
        let Some(last) = self.state.shown().len().checked_sub(1) else {
            return;
        };
        let index = self
            .synced_index(open, edge.end)
            .unwrap_or(0.0)
            .min(last as f64);
        let scale = self.state.time_scale_mut();
        let slots = (f64::from(scale.width()) - 1.0) / f64::from(scale.bar_spacing());
        let left = index + edge.offset;
        scale.set_right_offset(scale.position(left) - scale.base_idx() as f64 + slots - 0.5);
    }

    /// The visible UTC range, including whitespace mapped by [`Self::index_to_time`].
    /// Empty charts return `None`.
    /// Between bars, time is interpolated, so session gaps retain fractional edge positions.
    /// [`Self::set_visible_time_range`] shows a range.
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

    /// Shows `range` across the plot width, `from` at the leftmost plot pixel and `to` at the
    /// rightmost, as [`Self::visible_time_range`] reports them. Like
    /// [`Self::set_visible_logical_range`], it ends tracking mode and pending scrolls, and before
    /// the first frame the range is fitted again once the plot has its size, with the same dates
    /// when bars are merged in between.
    ///
    /// Returns `false` without changing the view for empty data, reversed or zero ranges, a
    /// range that ends before the first bar's slot or starts after the last bar's period, or
    /// endpoints mapping to the same element. The spacing and edge limits of the time scale
    /// options still apply: a span above the maximum spacing is centred, and the fit keeps a
    /// bar in view, so a view that would start after the last bar's open moves left to show
    /// that bar, or the newest bars across the plot when the range is longer than the plot at
    /// the minimum spacing. Unlike [`Self::apply_synced_time_range`], which fits a range from
    /// another chart, it has no 50-pixel cap.
    pub fn set_visible_time_range(&mut self, range: VisibleTimeRange) -> bool {
        let fitted = self.fit_time_range(range, f64::INFINITY);
        if fitted {
            self.cancel_view_motion();
            self.defer_visible_range(super::time_scale::PendingRange::Time(range));
        }
        fitted
    }

    /// Fits the receiver's bars to a UTC range from another chart, using its own plot width.
    /// Returns `false` without changing the view for empty data, reversed or zero ranges,
    /// a range that ends before the first bar's slot or starts after the last bar's period,
    /// or endpoints mapping to the same element.
    /// Spacing stops at the configured maximum, or 50 pixels when unlimited, centring short
    /// spans. The minimum spacing and local edge constraints still apply, and the fit keeps a
    /// bar in view: a range longer than the plot at the minimum spacing shows the newest bars.
    /// Use [`Self::set_visible_time_range`] to show dates on this chart without the cap.
    /// Before the first frame the range is fitted again once the plot has its size, with the
    /// same dates when bars are merged in between.
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
        let fitted = self.fit_time_range(range, cap);
        if fitted {
            self.defer_visible_range(super::time_scale::PendingRange::Synced(range, source));
        }
        fitted
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
        self.history.clear_view_restore();
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
        self.host_crosshair = None;
        self.synced_crosshair_bar_idx = bar_idx;
        self.synced_crosshair_period = None;
        self.synced_crosshair_price = None;
        self.synced_crosshair_series = None;
    }

    /// Returns the selected local index, including fractional added or custom points, from the
    /// last frame. Counts elements on price-based types. Use [`Self::hover_time`] between charts.
    pub fn get_hover_bar_idx(&self) -> Option<f64> {
        self.last_hover_bar_idx
    }

    /// Restores local bar spacing and right offset, for example after inspecting another view.
    /// These coordinates depend on the data and plot width. Use [`Self::apply_synced_time_range`]
    /// between independent charts. The restored view replaces a left edge that an interval
    /// change is keeping until its bars arrive.
    pub fn apply_synced_time_scale(&mut self, bar_spacing: f32, right_offset: f32) {
        self.history.clear_view_restore();
        if self.pending_left_edge.take().is_some() {
            self.pending_scroll_time = None;
        }
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
                frame(&ctx, &mut chart, 0.2, Vec::new());
                assert_eq!(chart.synced_crosshair_bar_idx, expected_index);
                assert_eq!(chart.last_crosshair, expected);
                let options = ImageExportOptions {
                    include_crosshair: true,
                    ..Default::default()
                };
                chart
                    .export_layout_image(&ctx, &options, |ui, snapshot| {
                        snapshot.show(ui);
                        assert_eq!(snapshot.last_crosshair, expected);
                    })
                    .unwrap()
                    .textures_delta
                    .clear();
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

    /// On a custom axis a key's period ends at the next key whatever the displayed interval:
    /// key 4 lies past the period of key 3, the last of keys 0 to 3, so it marks the whitespace
    /// slot 4, and the period of key 4, the last of keys 0 to 4, ends at key 5.
    #[test]
    fn custom_key_crosshair_period_ignores_the_timeframe() {
        let scale =
            std::sync::Arc::new(crate::scales::CustomHorizontalScale::new(0..6_u8, |key| {
                key.to_string()
            }));
        let keyed = |keys: &[u8]| {
            let mut chart = Chart::new(BarData::from_bars(
                keys.iter()
                    .map(|key| scale.value(key, 10.0).unwrap())
                    .collect(),
            ));
            chart.set_horizontal_scale(Some(scale.clone()));
            chart.set_timeframe(crate::model::Timeframe::Day1);
            chart
        };
        let mut receiver = keyed(&[0, 1, 2, 3]);
        receiver.set_synced_crosshair_time(scale.time(&4));
        assert_eq!(receiver.synced_crosshair_bar_idx, Some(4.0));
        let mut source = keyed(&[0, 1, 2, 3, 4]);
        source.last_hover_bar_idx = Some(4.0);
        assert_eq!(source.hover_period(), scale.time(&4).zip(scale.time(&5)));
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
            // After the last bar's period a time marks the whitespace slot holding it.
            (&one(30 * D + 1800), Some(0.0), &days, Some(30.0)),
            (&one(31 * D), Some(0.0), &days, Some(31.0)),
            // 25:30 lies in the four-hour slot from 24:00, slot 6; 28:00 opens slot 7.
            (&one(25 * H + 1800), Some(0.0), &four_hours, Some(6.0)),
            (&one(28 * H), Some(0.0), &four_hours, Some(7.0)),
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

    /// The dates of ten daily bars need more than the 50 px per bar that a synchronised receiver
    /// stops at, yet show exactly on a chart with default options; setting the dates of a view
    /// zoomed further keeps its spacing and edges, and a scroll requested before the dates does
    /// not move them on the next frame.
    #[test]
    fn set_visible_time_range_shows_the_requested_dates() {
        use crate::model::LogicalRange;
        use crate::widget::test_frames::frame;
        let day = |day: f64| time((day * 86_400.0) as i64);
        let mut chart = Chart::new(BarData::from_bars(
            (0..200)
                .map(|i| Bar::new(day(f64::from(i)), 10.0, 11.0, 9.0, 10.0, 1.0))
                .collect(),
        ));
        let ctx = egui::Context::default();
        for t in [0.0, 0.1] {
            frame(&ctx, &mut chart, t, Vec::new());
        }
        let plot = f64::from(chart.state.time_scale().width()) - 1.0;
        assert!(plot / 10.0 > 50.0, "{plot}");
        let assert_view = |chart: &Chart, range: VisibleTimeRange, spacing: f64| {
            let shown = chart.visible_time_range().unwrap();
            for (actual, expected) in [(shown.from, range.from), (shown.to, range.to)] {
                assert!(
                    (actual - expected).num_milliseconds().abs() < 1,
                    "{shown:?} != {range:?}"
                );
            }
            let actual = f64::from(chart.get_time_scale_state().0);
            assert!((actual - spacing).abs() < 1e-3, "{actual} != {spacing}");
        };
        // From the open of bar 100 to the close of bar 109.
        let requested = VisibleTimeRange::new(day(100.0), day(110.0));
        chart.scroll_to_time(day(20.0));
        assert!(chart.set_visible_time_range(requested));
        frame(&ctx, &mut chart, 0.2, Vec::new());
        assert_view(&chart, requested, plot / 10.0);
        // Five slots from index 99.5 to 104.5: noon of day 99 to noon of day 104, within the
        // rounding of the spacing to f32.
        assert!(chart.set_visible_logical_range(LogicalRange::new(100.0, 104.0)));
        let zoomed = chart.visible_time_range().unwrap();
        for (actual, expected) in [(zoomed.from, day(99.5)), (zoomed.to, day(104.5))] {
            assert!(
                (actual - expected).num_milliseconds().abs() < 100,
                "{zoomed:?}"
            );
        }
        let (spacing, offset) = chart.get_time_scale_state();
        assert!(chart.set_visible_time_range(zoomed));
        assert_view(&chart, zoomed, f64::from(spacing));
        assert!((chart.get_time_scale_state().1 - offset).abs() < 1e-4);
    }
}
