//! The chart's bar series, merged by one set of rules for history and live updates.

mod options;
mod paint;
mod scales;

pub use options::{PaneScale, SeriesOptions};
pub(super) use options::{PriceSeries, SeriesLegend, base_line};
pub(super) use paint::paint_last_price_pulse;
pub(super) use scales::axis_label_rect;

use super::change_marks::ChangeMarks;
use super::{Chart, ChartElementId, DataOperation, SeriesId};
use crate::data::FeedState;
use crate::data::merge::{Merged, Origin, merge, valid};
use crate::model::{
    Bar, BarClock, BarData, ChartType, LogicalRange, Marker, MarkerOptions, PriceLine,
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

/// The index in time-ordered `bars` of the bar at `time`, or else of its nearest neighbour in
/// `direction`. An exact match returns the first of several bars with that time.
pub(super) fn index_at_time(
    bars: &[Bar],
    time: DateTime<Utc>,
    direction: MismatchDirection,
) -> Option<usize> {
    index_of_key(bars, |bar| bar.time, time, direction)
}

/// [`index_at_time`] over any items in time order.
pub(super) fn index_of_key<T>(
    items: &[T],
    key: impl Fn(&T) -> DateTime<Utc>,
    time: DateTime<Utc>,
    direction: MismatchDirection,
) -> Option<usize> {
    let at = items.partition_point(|item| key(item) < time);
    if items.get(at).is_some_and(|item| key(item) == time) {
        return Some(at);
    }
    match direction {
        MismatchDirection::Exact => None,
        MismatchDirection::NearestLeft => at.checked_sub(1),
        MismatchDirection::NearestRight => (at < items.len()).then_some(at),
    }
}

/// How discrete time and series queries resolve a missing timestamp or logical position.
///
/// Every direction returns an exact match when present. A neighbour search returns `None`
/// when no point lies on the requested side, without falling back to the opposite side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchDirection {
    /// Only the requested timestamp or logical position.
    Exact,
    /// The greatest timestamp or logical position below the requested one.
    NearestLeft,
    /// The smallest timestamp or logical position above the requested one.
    NearestRight,
}

/// The bars of a series inside a range of logical indices, from
/// [`Chart::bars_in_logical_range`], or the points of a custom series, from
/// [`Chart::custom_points_in_logical_range`].
#[derive(Debug, Clone, PartialEq)]
pub struct BarsInRange {
    /// Indices into [`Chart::bars`] or [`Chart::custom_points`] of the items inside the range,
    /// empty when none is.
    pub bars: std::ops::Range<usize>,
    /// The logical distance from the series' first bar to the first selected bar. If no bars
    /// are selected, or the series' first bar is selected, measured to the unrounded left
    /// edge instead. Negative when that edge lies before the first bar.
    pub bars_before: f64,
    /// The logical distance from the last selected bar to the series' last bar. If no bars
    /// are selected, or the series' last bar is selected, measured from the unrounded right
    /// edge instead. Negative when that edge lies after the last bar.
    pub bars_after: f64,
}

/// A valued bar selected by [`Chart::series_last_value`], on its series' price scale.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesLastValue {
    /// The selected bar's close in raw price units.
    pub value: f64,
    /// Its logical position on the shared horizontal scale, including gaps.
    pub index: f64,
    /// Its y coordinate in the chart's [`Ui`](egui::Ui), possibly outside the pane.
    pub coordinate: f32,
    /// Its last-value tag colour, including any per-bar override.
    pub color: egui::Color32,
    /// The value formatted as [`Chart::format_series_price`] formats it, without a name.
    pub text: String,
}

/// Handle of a price line added with [`Chart::add_price_line`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PriceLineId(u64);

impl Chart {
    /// Take the settings request from a legend button or a double-click in a pane: a study, a
    /// bar series shown in a pane other than the main one, or [`SeriesId::MAIN`] from the main
    /// series' legend button.
    pub fn take_indicator_settings_request(&mut self) -> Option<SeriesId> {
        self.indicator_settings_request
            .take()
            .filter(|id| !self.study_ids.contains(id) || self.indicator_user_edit_enabled(*id))
    }

    /// Replaces the bars of `series`, for history or a replay window. Invalid bars (a non-finite
    /// value, a high or low on the wrong side of the open and close, negative volume) are dropped
    /// and counted in [`feed_state`](Self::feed_state), except [whitespace](Bar::whitespace) bars,
    /// which keep their slots empty. The rest are sorted by time, and of several
    /// bars with one open time, or in one slot of the timeframe for a chart built with a feed, the
    /// last is kept. Drawings retain their timestamps across replacements and interval changes.
    /// The studies are updated from the first bar that changed on the next [`show`](Self::show),
    /// and near the live edge the view follows a new last bar, as with [`update`](Self::update).
    /// A view scrolled back stays on its bars when older or newer bars arrive.
    ///
    /// Bar series accept data; unknown ids and study ids are ignored with a warning, and so is
    /// the main series while it is removed from a chart that keeps the original pane.
    /// See [`set_data_operation_notifications`](Self::set_data_operation_notifications) to
    /// observe every accepted call, including empty and unchanged replacements.
    pub fn set_data(&mut self, series: SeriesId, bars: Vec<Bar>) {
        if (series == SeriesId::MAIN && self.main_bars_live()
            || self.price_series.iter().any(|item| item.id == series))
            && self.replay_retains(series)
        {
            self.replay_set_data(series, bars);
            self.data_set(series);
            return;
        }
        if let Some(at) = self.price_series.iter().position(|item| item.id == series) {
            let (mut bars, rejected) = clean(bars, &self.clock());
            retain_bars(&mut bars, self.bar_limit(series));
            let item = &mut self.price_series[at];
            item.bars = bars;
            item.revision = crate::model::next_revision();
            if let Some(marks) = &mut item.change_marks {
                marks.clear();
            }
            item.feed.rejected_bars += rejected;
            self.data_set(series);
            return;
        }
        if !self.main_bars_live() || !holds_bars(series, "set_data") {
            return;
        }
        self.replace_main_data(bars);
        self.data_set(series);
    }

    pub(super) fn replace_main_data(&mut self, bars: Vec<Bar>) {
        let clock = self.clock();
        let (mut bars, rejected) = clean(bars, &clock);
        retain_bars(&mut bars, self.bar_limit(SeriesId::MAIN));
        self.feed.rejected_bars += rejected;
        self.from_ticks.clear();
        if let Some(marks) = &mut self.change_marks {
            marks.clear();
        }
        self.pending_box_sizes = self
            .chart_type_params
            .size_methods()
            .map(|method| method != crate::model::BoxSizeMethod::Fixed);
        let before = self.edge();
        #[cfg(feature = "drawings")]
        self.anchor_drawing_points();
        let first_changed = self.state.set_data(BarData::from_bars(bars));
        #[cfg(feature = "drawings")]
        self.project_drawing_points();
        self.bars_changed(before, first_changed, &clock);
    }

    /// Merges `bar` into the bars of `series` as [`Sink::push_bar`](crate::data::Sink::push_bar)
    /// does, matched by open time, or by the slot of the timeframe for a chart built with a feed:
    /// a bar that matches a stored bar replaces it, a newer one is appended and an older one is
    /// inserted in order. An invalid bar is dropped and counted in
    /// [`feed_state`](Self::feed_state). Drawings move with their bars, studies follow them on the
    /// next [`show`](Self::show), and near the live edge the view follows a new last bar. A view
    /// scrolled back stays on its bars, also when the bars of an added series lay out the time
    /// scale of a chart without main bars, from the next [`show`](Self::show). A merged bar is
    /// reported by [`ChartEvent::DataUpdated`](super::ChartEvent::DataUpdated).
    ///
    /// Bar series accept data; unknown ids and study ids are ignored with a warning.
    /// See [`set_data_operation_notifications`](Self::set_data_operation_notifications) to
    /// observe every accepted call, including unchanged updates.
    pub fn update(&mut self, series: SeriesId, bar: Bar) {
        let operation = if self.host_events.data_operations && valid(&bar) {
            let clock = self.clock();
            let slot = clock.bar_open(bar.time);
            let bars = self
                .replay_window()
                .filter(|_| self.replay_retains(series))
                .and_then(|replay| replay.bars.get(&series))
                .map_or_else(|| self.bars(series), Vec::as_slice);
            Some(
                match bars.binary_search_by_key(&slot, |bar| clock.bar_open(bar.time)) {
                    Ok(_) => DataOperation::Update(bar.time),
                    Err(at) if at == bars.len() => DataOperation::Append(bar.time),
                    Err(_) => DataOperation::Insert(bar.time),
                },
            )
        } else {
            None
        };
        let changed = if (series == SeriesId::MAIN && self.main_bars_live()
            || self.price_series.iter().any(|item| item.id == series))
            && self.replay_retains(series)
        {
            self.replay_update(series, bar)
        } else if let Some(at) = self.price_series.iter().position(|item| item.id == series) {
            let clock = self.clock();
            let item = &mut self.price_series[at];
            let len = item.bars.len();
            let incoming = vec![(bar, Origin::Live)];
            let before = ChangeMarks::before(&item.bars, &clock, &incoming);
            let merged = merge(&mut item.bars, &mut BTreeMap::new(), &clock, incoming);
            item.feed.rejected_bars += merged.rejected;
            if let Some(marks) = &mut item.change_marks {
                marks.record(&item.bars, &clock, before);
            }
            if merged.first_changed.is_some_and(|first| first + 1 < len) {
                item.revision = crate::model::next_revision();
            }
            let changed = merged.first_changed.is_some();
            self.enforce_bar_limit(series);
            changed
        } else if self.main_bars_live() && holds_bars(series, "update") {
            self.apply(vec![(bar, Origin::Live)])
                .first_changed
                .is_some()
        } else {
            return;
        };
        if changed {
            self.data_updated(series);
        }
        if let Some(operation) = operation {
            self.data_operation(series, operation);
        }
    }

    /// Removes the last `count` bars of `series`, or all of them for a larger count, and returns
    /// them oldest first, leaving the other bars in place. Drawings, studies and the view follow
    /// as for [`update`](Self::update), and [`ChartEvent::DataUpdated`](super::ChartEvent::DataUpdated)
    /// reports the removal. During a replay the bars are removed from the retained history.
    ///
    /// Bar series accept this; unknown ids and study ids are ignored with a warning.
    pub fn pop_bars(&mut self, series: SeriesId, count: usize) -> Vec<Bar> {
        let present = series == SeriesId::MAIN && self.main_bars_live()
            || self.price_series.iter().any(|item| item.id == series);
        let removed = if present && self.replay_retains(series) {
            self.replay_pop(series, count)
        } else if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            item.revision = crate::model::next_revision();
            item.bars.split_off(item.bars.len().saturating_sub(count))
        } else if self.main_bars_live() && holds_bars(series, "pop_bars") {
            let before = self.edge();
            #[cfg(feature = "drawings")]
            self.anchor_drawing_points();
            let mut removed = Vec::new();
            let first_changed = self.state.edit_bars(|bars| {
                let at = bars.len().saturating_sub(count);
                removed = bars.split_off(at);
                (!removed.is_empty()).then_some(at)
            });
            if let Some(first) = removed.first() {
                self.from_ticks.split_off(&first.time);
            }
            #[cfg(feature = "drawings")]
            self.project_drawing_points();
            self.bars_changed(before, first_changed, &self.clock());
            removed
        } else {
            return Vec::new();
        };
        if !removed.is_empty() {
            self.data_updated(series);
        }
        removed
    }

    /// Keeps at most `limit` bars of `series`. When it holds more, now or after history loads
    /// and updates, the oldest are evicted down to `limit - limit / 8`, so a series that
    /// has reached its limit holds between seven eighths of it and all of it. `None` (the
    /// default) keeps all bars. The limit survives symbol and timeframe changes and also bounds
    /// retained replay history. Removed data must be supplied again by the host to recover it.
    /// Feed scroll-back stops once the main series holds `limit - limit / 8` bars, the number an
    /// eviction keeps; raising or clearing the limit allows paging again.
    ///
    /// Each eviction restarts the studies on the retained window, including their warm-up, so a
    /// stream of new bars restarts them once per `limit / 8 + 1` bars. Drawing timestamps
    /// are kept, even for evicted bars; markers and raw trades have independent lifetimes.
    /// Unknown ids and study ids are ignored with a warning.
    pub fn set_bar_limit(&mut self, series: SeriesId, limit: Option<NonZeroUsize>) {
        if !self.bar_series_present(series, "set_bar_limit") {
            return;
        }
        match limit {
            Some(limit) => {
                self.bar_limits.insert(series, limit);
            }
            None => {
                self.bar_limits.remove(&series);
            }
        }
        self.apply_replay_window();
        if !self.replay_windows(series) && self.enforce_bar_limit(series) > 0 {
            self.data_updated(series);
        }
    }

    /// The retained-bar limit of `series`, or `None` when unlimited or absent. The main bars keep
    /// their limit while they stay live without the original pane, as
    /// [`remove_pane`](Self::remove_pane) describes.
    pub fn bar_limit(&self, series: SeriesId) -> Option<NonZeroUsize> {
        let limit = self.bar_limits.get(&series).copied();
        // A removed main series keeps its limit for when its bars take data again.
        limit.filter(|_| series != SeriesId::MAIN || self.main_bars_live())
    }

    /// Removes up to `count` oldest bars of `series`, returning them oldest first. Studies
    /// restart on the retained window; drawings keep their timestamps and the view keeps its
    /// time position where possible. During replay this removes from the full retained history
    /// and its matching finer-source slots; in market replay the main series and the series a
    /// feed supplies lose the bars the driver built instead.
    /// Emits [`ChartEvent::DataUpdated`](super::ChartEvent::DataUpdated) when bars are removed.
    /// Unknown ids and study ids are ignored with a warning.
    pub fn remove_oldest_bars(&mut self, series: SeriesId, count: usize) -> Vec<Bar> {
        let removed = self.take_oldest_bars(series, count);
        if !removed.is_empty() {
            self.data_updated(series);
        }
        removed
    }

    fn take_oldest_bars(&mut self, series: SeriesId, count: usize) -> Vec<Bar> {
        if !self.bar_series_present(series, "remove_oldest_bars") || count == 0 {
            return Vec::new();
        }
        let clock = self
            .timeframe
            .map_or_else(BarClock::exact, |_| self.bar_clock());
        let removed = if self.replay_windows(series)
            && let Some(replay) = self.replay_window_mut()
        {
            let removed = replay.remove_oldest_bars(series, count, &clock);
            self.apply_replay_window();
            removed
        } else if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            let count = count.min(item.bars.len());
            let removed = item.bars.drain(..count).collect();
            if count > 0 {
                item.revision = crate::model::next_revision();
                shrink_bars(&mut item.bars);
                if let Some(marks) = &mut item.change_marks {
                    marks.clear();
                }
            }
            removed
        } else {
            #[cfg(feature = "drawings")]
            self.anchor_drawing_points();
            let mut removed = Vec::new();
            self.state.edit_bars(|bars| {
                let count = count.min(bars.len());
                removed = bars.drain(..count).collect();
                shrink_bars(bars);
                (count > 0).then_some(0)
            });
            if !removed.is_empty() {
                if let Some(first) = self.state.data().bars.first() {
                    self.from_ticks = self.from_ticks.split_off(&first.time);
                } else {
                    self.from_ticks.clear();
                }
                if let Some(marks) = &mut self.change_marks {
                    marks.clear();
                }
                // A study that keeps the values of the remaining bars draws them until its turn.
                for (at, study) in self.studies.indicators_mut().iter_mut().enumerate() {
                    if self.pending_from[at] != Some(0) && study.remove_oldest_bars(removed.len()) {
                        self.pending_from[at] = None;
                        self.stale[at] = true;
                    } else {
                        self.pending_from[at] = Some(0);
                    }
                }
                if let Some(start) = &mut self.pending_start_idx {
                    *start = start.saturating_sub(removed.len());
                }
                self.scroll_animation = None;
            }
            #[cfg(feature = "drawings")]
            self.project_drawing_points();
            removed
        };
        if series == SeriesId::MAIN && !removed.is_empty() {
            self.feed.history_complete = false;
        }
        removed
    }

    fn bar_series_present(&self, series: SeriesId, call: &str) -> bool {
        self.price_series.iter().any(|item| item.id == series)
            || self.main_bars_live() && holds_bars(series, call)
    }

    /// Whether the main bars take data. A main series removed with the original pane stays out
    /// of the layout, but its bars still supply the surviving panes' studies and time axis, and
    /// without that pane nothing brings the series back to supply them later.
    pub(super) fn main_bars_live(&self) -> bool {
        self.main_series_present || self.pane_index(super::PaneId::MAIN).is_none()
    }

    fn enforce_bar_limit(&mut self, series: SeriesId) -> usize {
        let count = self
            .bar_limit(series)
            .map_or(0, |limit| evicted_bars(self.bars(series).len(), limit));
        if count > 0 {
            self.take_oldest_bars(series, count);
        }
        count
    }

    /// The bars of `series`, oldest first, with their colours and payloads: `bars(id).get(index)`
    /// is the bar at an index and `bars(id).last()` the last bar, which can be a
    /// [whitespace](Bar::whitespace) bar. Removed series, volume and studies hold no bars, except
    /// the main series on a chart without the original pane, whose bars stay live.
    pub fn bars(&self, series: SeriesId) -> &[Bar] {
        if series == SeriesId::MAIN && self.main_bars_live() {
            &self.state.data().bars
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == series)
                .map_or(&[], |item| item.bars.as_slice())
        }
    }

    /// The index in [`bars`](Self::bars) of the bar of `series` opened at `time`, or else of its
    /// nearest neighbour in `direction`. `None` when there is no such bar. This is a stored-row
    /// offset; use [`bar_at_index`](Self::bar_at_index) to query by shared logical position.
    pub fn bar_index_at(
        &self,
        series: SeriesId,
        time: DateTime<Utc>,
        direction: MismatchDirection,
    ) -> Option<usize> {
        index_at_time(self.bars(series), time, direction)
    }

    /// The bar of `series` at a logical position on the shared horizontal scale, or its nearest
    /// neighbour in `direction`. This includes stored [whitespace](Bar::whitespace) bars.
    ///
    /// Unlike an offset into [`bars`](Self::bars), the position counts gaps: a series at logical
    /// indices 0, 2 and 4 returns its second stored bar for index 2. Fractional positions can
    /// select neighbours without rounding. `None` for a non-finite index, an absent series, an
    /// empty time scale or when there is no bar in the requested direction.
    pub fn bar_at_index(
        &self,
        series: SeriesId,
        index: f64,
        direction: MismatchDirection,
    ) -> Option<&Bar> {
        let bars = self.bars(series);
        bars.get(self.key_at_index(bars, |bar| bar.time, index, direction)?)
    }

    /// The position in `items`, in time order, at a logical position on the shared scale.
    pub(super) fn key_at_index<T>(
        &self,
        items: &[T],
        key: impl Fn(&T) -> DateTime<Utc>,
        index: f64,
        direction: MismatchDirection,
    ) -> Option<usize> {
        if !index.is_finite() {
            return None;
        }
        self.state.shown().first()?;
        if let Some(time) = self.index_to_time(index) {
            return index_of_key(items, key, time, direction);
        }
        // Extrapolation beyond the timestamp domain still has a neighbour on one side.
        match direction {
            MismatchDirection::NearestLeft if index > 0.0 => items.len().checked_sub(1),
            MismatchDirection::NearestRight if index < 0.0 => (!items.is_empty()).then_some(0),
            _ => None,
        }
    }

    /// The bars of `series` inside `range`, a range of logical indices of the time scale such as
    /// [`visible_logical_range`](Self::visible_logical_range) returns, with the distances to the
    /// ends of the series, for example to request older history before the user scrolls to its
    /// start. A bar's logical index is the position of its open time on the time scale, so a
    /// series that skips positions counts them. The range's edges are rounded to whole indices
    /// to select the bars. An empty intersection returns an empty slice at the insertion point,
    /// with `bars_before = range.left - first_index` and `bars_after = last_index - range.right`.
    /// Distances at either end of the series also retain the unrounded requested edge.
    /// `None` for a series without bars or a chart without a time scale.
    pub fn bars_in_logical_range(
        &self,
        series: SeriesId,
        range: LogicalRange,
    ) -> Option<BarsInRange> {
        self.keys_in_logical_range(self.bars(series), |bar| bar.time, range)
    }

    /// [`bars_in_logical_range`](Self::bars_in_logical_range) over any items in time order.
    pub(super) fn keys_in_logical_range<T>(
        &self,
        items: &[T],
        key: impl Fn(&T) -> DateTime<Utc>,
        range: LogicalRange,
    ) -> Option<BarsInRange> {
        let shown = self.state.shown();
        let (first, last) = (items.first()?, items.last()?);
        let index = |item: &T| crate::model::index_at(shown, key(item)).unwrap_or(f64::NAN);
        let (left, right) = (f64::from(range.left), f64::from(range.right));
        shown.first()?;
        let start = items.partition_point(|item| index(item) < left.round());
        let end = items
            .partition_point(|item| index(item) <= right.round())
            .max(start);
        let from = if start > 0 && start < end {
            index(&items[start])
        } else {
            left
        };
        let to = if end < items.len() && start < end {
            index(&items[end - 1])
        } else {
            right
        };
        Some(BarsInRange {
            bars: start..end,
            bars_before: from - index(first),
            bars_after: index(last) - to,
        })
    }

    /// Replaces the markers drawn on the bars of `series` (trade signals, events).
    ///
    /// Bar series accept data; unknown ids and study ids are ignored with a warning.
    pub fn set_markers(&mut self, series: SeriesId, markers: Vec<Marker>) {
        if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            item.markers = markers.into();
        } else if self.main_series_present && holds_bars(series, "set_markers") {
            self.marks = markers.into();
        }
    }

    /// The markers of `series` from [`set_markers`](Self::set_markers), without those of a
    /// [mark source](Self::set_mark_source). Empty for ids that hold no bars.
    pub fn markers(&self, series: SeriesId) -> &[Marker] {
        if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            &item.markers
        } else if series == SeriesId::MAIN && self.main_series_present {
            &self.marks
        } else {
            &[]
        }
    }

    /// Marker fitting and layering of a bar series, or `None` for an absent series.
    pub fn marker_options(&self, series: SeriesId) -> Option<MarkerOptions> {
        if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            Some(item.marker_options)
        } else if series == SeriesId::MAIN && self.main_series_present {
            Some(self.marker_options)
        } else {
            None
        }
    }

    /// Sets whether the markers of `series` widen its autoscaled price range and where they
    /// are painted. The main series' options also apply to the bar marks of a mark source.
    ///
    /// Bar series accept options; unknown ids and study ids are ignored with a warning.
    pub fn set_marker_options(&mut self, series: SeriesId, options: MarkerOptions) {
        if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            item.marker_options = options;
        } else if self.main_series_present && holds_bars(series, "set_marker_options") {
            self.marker_options = options;
        }
    }

    /// Replaces the price lines drawn on the scale of `series`. Handles of its previous lines,
    /// from [`add_price_line`](Self::add_price_line), no longer refer to a line.
    ///
    /// Every bar series accepts price lines. The price
    /// scale does not fit to them: a line whose price is off the scale or not finite is not drawn.
    pub fn set_price_lines(&mut self, series: SeriesId, lines: Vec<PriceLine>) {
        let first = self.next_price_line;
        self.next_price_line += lines.len() as u64;
        if let Some(stored) = self.price_lines_mut(series, "set_price_lines") {
            *stored = (first..).map(PriceLineId).zip(lines).collect();
        }
    }

    /// Adds `line` to the price lines drawn on the scale of `series` and returns its handle,
    /// with which [`update_price_line`](Self::update_price_line) moves or restyles it and
    /// [`remove_price_line`](Self::remove_price_line) removes it. `None` when `series` holds no
    /// bars, as with [`set_price_lines`](Self::set_price_lines).
    pub fn add_price_line(&mut self, series: SeriesId, line: PriceLine) -> Option<PriceLineId> {
        let id = PriceLineId(self.next_price_line);
        self.price_lines_mut(series, "add_price_line")?
            .push((id, line));
        self.next_price_line += 1;
        Some(id)
    }

    /// The price lines of a bar series in drawing order, with their handles for
    /// [`price_line`](Self::price_line), [`update_price_line`](Self::update_price_line) and
    /// [`remove_price_line`](Self::remove_price_line). The order is that of
    /// [`set_price_lines`](Self::set_price_lines) followed by later
    /// [`add_price_line`](Self::add_price_line) calls; an update keeps a line's place.
    /// A series without lines, a study, an unknown id and a removed series yield no lines.
    pub fn price_lines(&self, series: SeriesId) -> impl Iterator<Item = (PriceLineId, &PriceLine)> {
        let lines = self
            .price_series
            .iter()
            .find(|item| item.id == series)
            .map(|item| item.lines.as_slice())
            .or_else(|| {
                (series == SeriesId::MAIN && self.main_series_present)
                    .then_some(self.price_lines.as_slice())
            })
            .unwrap_or_default();
        lines.iter().map(|(id, line)| (*id, line))
    }

    /// The price line of `id`, or `None` once it is removed, replaced by
    /// [`set_price_lines`](Self::set_price_lines) or its series is removed.
    pub fn price_line(&self, id: PriceLineId) -> Option<&PriceLine> {
        std::iter::once(&self.price_lines)
            .chain(self.price_series.iter().map(|item| &item.lines))
            .flatten()
            .find_map(|(line_id, line)| (*line_id == id).then_some(line))
    }

    /// Replaces the price line of `id`, keeping its place in the drawing order. Returns `false`
    /// when `id` no longer refers to a line.
    pub fn update_price_line(&mut self, id: PriceLineId, line: PriceLine) -> bool {
        let stored = std::iter::once(&mut self.price_lines)
            .chain(self.price_series.iter_mut().map(|item| &mut item.lines))
            .flatten()
            .find(|(line_id, _)| *line_id == id);
        stored.map(|(_, stored)| *stored = line).is_some()
    }

    /// Removes the price line of `id`. Returns `false` when `id` no longer refers to a line.
    pub fn remove_price_line(&mut self, id: PriceLineId) -> bool {
        std::iter::once(&mut self.price_lines)
            .chain(self.price_series.iter_mut().map(|item| &mut item.lines))
            .any(|lines| {
                let len = lines.len();
                lines.retain(|(line_id, _)| *line_id != id);
                lines.len() < len
            })
    }

    fn price_lines_mut(
        &mut self,
        series: SeriesId,
        call: &str,
    ) -> Option<&mut Vec<(PriceLineId, PriceLine)>> {
        if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            Some(&mut item.lines)
        } else if self.main_series_present && holds_bars(series, call) {
            Some(&mut self.price_lines)
        } else {
            None
        }
    }

    /// What the chart knows about the feed of `series`, such as the bars dropped as invalid.
    /// `Some` for every present bar series and custom data series, whose invalid points count
    /// as `rejected_bars`, and for the main series while its bars are live without the original
    /// pane.
    pub fn feed_state(&self, series: SeriesId) -> Option<&FeedState> {
        if series == SeriesId::MAIN && self.main_bars_live() {
            Some(&self.feed)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == series)
                .map(|item| &item.feed)
                .or_else(|| self.custom_feed_state(series))
        }
    }

    /// Merges `incoming` into the main bars by the rules of each bar's origin and counts the
    /// invalid ones in the feed state.
    pub(crate) fn apply(&mut self, incoming: Vec<(Bar, Origin)>) -> Merged {
        let clock = self.clock();
        self.apply_by(&clock, incoming)
    }

    /// [`apply`](Self::apply) with the slots of `clock`, the chart's.
    pub(super) fn apply_by(&mut self, clock: &BarClock, incoming: Vec<(Bar, Origin)>) -> Merged {
        let before = self.edge();
        #[cfg(feature = "drawings")]
        self.anchor_drawing_points();
        let from_ticks = &mut self.from_ticks;
        let closes = self
            .change_marks
            .as_ref()
            .map(|_| ChangeMarks::before(&self.state.data().bars, clock, &incoming));
        let mut merged = Merged::default();
        self.state.edit_bars(|bars| {
            merged = merge(bars, from_ticks, clock, incoming);
            merged.first_changed
        });
        if let (Some(marks), Some(closes)) = (&mut self.change_marks, closes) {
            marks.record(&self.state.data().bars, clock, closes);
        }
        self.feed.rejected_bars += merged.rejected;
        #[cfg(feature = "drawings")]
        self.project_drawing_points();
        self.bars_changed(before, merged.first_changed, clock);
        if self.enforce_bar_limit(SeriesId::MAIN) > 0 {
            merged.first_changed = Some(0);
        }
        merged
    }

    /// Removes a bar series or study and clears its selection. Removing the main series retains
    /// the time axis for the remaining series and drawings and pauses its feed; on a chart without
    /// the original pane its bars stay live instead, as [`remove_pane`](Self::remove_pane)
    /// describes. Undo of the [`user_action`](Self::user_action) that removed a bar series
    /// restores it with its markers, price lines, formatter and bar limit, and for the main
    /// series the feed follows [`set_feed_paused`](Self::set_feed_paused) again. Returns `false`
    /// for an unknown or already removed id.
    pub fn remove_series(&mut self, id: SeriesId) -> bool {
        if id == SeriesId::MAIN {
            if !self.main_series_present {
                return false;
            }
            self.set_main_series_visible(false);
            self.remove_main_series();
            return true;
        }
        if let Some(at) = self.price_series.iter().position(|item| item.id == id) {
            let removed = self.price_series.remove(at);
            self.retain_removed_series(removed);
            self.host_events.push(super::ChartEvent::SeriesRemoved(id));
            if self.selection.selected_id() == Some(ChartElementId::Series(id)) {
                self.selection.deselect();
            }
            self.prune_series_scales();
            return true;
        }
        let Some(indicator) = self.take_study(id) else {
            return false;
        };
        self.retain_removed_study(id, indicator);
        true
    }

    /// Show or hide the main price series, its volume and its legend without unloading its bars.
    /// Studies and drawings remain visible and continue to receive data.
    pub fn set_main_series_visible(&mut self, visible: bool) {
        self.main_series_visible = visible && self.main_series_present;
        if !visible
            && matches!(
                self.selection.selected_id(),
                Some(ChartElementId::Series(_))
            )
        {
            self.selection.deselect();
        }
    }

    /// Whether the main price series is visible.
    pub fn is_main_series_visible(&self) -> bool {
        self.main_series_visible
    }

    /// Keeps studies and the view in step after the main bars changed. `first_changed` is the
    /// index of the first bar inserted, changed or removed. The studies are updated once on the
    /// next frame, for all changes since the previous one.
    fn bars_changed(&mut self, before: Edge, first_changed: Option<usize>, clock: &BarClock) {
        if let Some(from) = first_changed {
            for pending in &mut self.pending_from {
                *pending = Some(pending.map_or(from, |pending| pending.min(from)));
            }
            // The history revision leaves out edits of the last bar, which change the table too.
            self.readout_revision = crate::model::next_revision();
        }
        self.follow_live_edge(before, clock);
    }

    fn edge(&self) -> Edge {
        let bars = &self.state.data().bars;
        Edge {
            // Within ~1.5 bars of the right edge. With fix_right_edge the right offset is clamped
            // to <= 0, so near means no farther than 1.5 bars to the left of the edge.
            near_live: self.state.time_scale().is_near_live_edge(),
            tail: bars.last().cloned(),
        }
    }

    /// Shows the cached elements before input, keeping the view in the series' slots.
    pub(super) fn show_elements(&mut self) {
        #[cfg(feature = "drawings")]
        self.anchor_drawing_points();
        self.chart_type_params
            .resolve_sizes(&self.state.data().bars, &mut self.pending_box_sizes);
        // Range bars come from the held trades, as of the replay cutoff, while there are any.
        let cutoff = self.replay_time();
        let ranges = &mut self.execution_ranges;
        let executions = self
            .feed_link
            .as_ref()
            .filter(|_| self.chart_type == ChartType::Range)
            .and_then(|link| {
                ranges.bars(
                    &self.chart_type_params,
                    link.trades.revision(),
                    link.trades.iter(),
                    cutoff,
                )
            });
        let built = executions.or_else(|| {
            crate::chart::rendering::candles::price_based::elements(
                self.chart_type,
                &self.chart_type_params,
                &self.state.data().bars,
                self.state.revision(),
            )
        });
        self.older_bars_drawn = built.as_ref().is_none_or(|(_, trimmed)| !trimmed);
        if self.chart_type.is_time_independent() {
            self.element_studies.sync(
                &self.studies,
                built
                    .as_ref()
                    .map_or_else(|| std::sync::Arc::from([]), |(bars, _)| bars.clone()),
                self.timeframe,
            );
        }
        let follow = self.chart_options.time_scale.shift_visible_range_on_new_bar;
        self.state
            .set_elements(built.map(|(elements, _)| elements), follow);
        self.refresh_time_scale_content();
        #[cfg(feature = "drawings")]
        self.project_drawing_points();
    }

    /// Scrolls to the live edge when the view was near it and a bar was appended, or a
    /// whitespace tail gained a value while the options allow shifting on whitespace
    /// replacement. Any other view stays on its bars.
    fn follow_live_edge(&mut self, before: Edge, clock: &BarClock) {
        let bars = &self.state.data().bars;
        let tail = before.tail.as_ref().zip(bars.last());
        // Appended only if the last bar moved to a later slot, whatever the length: a rolling
        // window or a bar limit keeps the series from growing. Older bars loaded in front make
        // the series longer and keep its last bar. They need no change of the right offset
        // either, since the bar indices and the base index both grow by their count.
        let appended =
            tail.is_some_and(|(old, new)| clock.bar_open(new.time) > clock.bar_open(old.time));
        let whitespace_replaced =
            tail.is_some_and(|(old, new)| old.is_whitespace() && !new.is_whitespace());
        let options = &self.chart_options.time_scale;
        let shift_for_whitespace = whitespace_replaced
            && !appended
            && options.allow_shift_visible_range_on_whitespace_replacement;
        let follow = options.shift_visible_range_on_new_bar
            && before.near_live
            && (appended || shift_for_whitespace);
        if follow {
            self.scroll_to_live_edge();
        } else if !self.state.shows_elements()
            && let Some(tail) = before.tail
        {
            self.keep_bars_in_view(clock, tail.time);
        }
    }

    /// [`follow_live_edge`](Self::follow_live_edge) for the slots an added series gave a chart
    /// without main bars after `tail`, the last slot before the refresh.
    pub(super) fn added_slots_arrived(&mut self, tail: DateTime<Utc>) {
        if !self.state.shows_fallback()
            || self.state.shown().last().is_none_or(|bar| bar.time <= tail)
        {
            return;
        }
        if self.chart_options.time_scale.shift_visible_range_on_new_bar
            && self.state.time_scale().is_near_live_edge()
        {
            self.scroll_to_live_edge();
        } else {
            self.keep_bars_in_view(&self.clock(), tail);
        }
    }

    fn scroll_to_live_edge(&mut self) {
        self.state.time_scale_mut().scroll_to_realtime();
        // A pending start index would move the view away from the edge again.
        self.pending_start_idx = None;
    }

    /// Moves the right offset against the slots after the one of `tail`. The right edge is the
    /// last slot plus the right offset: without this a view scrolled back would move one bar
    /// forward per appended bar. A bar stamped later in the slot of the last bar replaces it
    /// and appends nothing.
    fn keep_bars_in_view(&mut self, clock: &BarClock, tail: DateTime<Utc>) {
        let tail = clock.bar_open(tail);
        let shown = self.state.shown();
        let appended = shown.len() - shown.partition_point(|bar| clock.bar_open(bar.time) <= tail);
        let time_scale = self.state.time_scale_mut();
        time_scale.set_right_offset(time_scale.right_offset_precise() - appended as f64);
    }
}

/// The bars and the view before the bars change, for [`Chart::bars_changed`].
struct Edge {
    near_live: bool,
    tail: Option<Bar>,
}

/// `bars` without the invalid ones, sorted by time, with the last of several bars of one slot of
/// `clock`; and the number of invalid bars dropped.
pub(super) fn clean(bars: Vec<Bar>, clock: &BarClock) -> (Vec<Bar>, u64) {
    // Bars already in order pass as they are, without the copy through the merge.
    let slot = |bar: &Bar| clock.bar_open(bar.time);
    if bars.windows(2).all(|pair| slot(&pair[0]) < slot(&pair[1])) && bars.iter().all(valid) {
        return (bars, 0);
    }
    let mut clean = Vec::with_capacity(bars.len());
    let incoming = bars.into_iter().map(|bar| (bar, Origin::History)).collect();
    let merged = merge(&mut clean, &mut BTreeMap::new(), clock, incoming);
    (clean, merged.rejected)
}

/// Evicts the oldest bars [`evicted_bars`] counts and releases excessive capacity after a large
/// import.
pub(super) fn retain_bars(bars: &mut Vec<Bar>, limit: Option<NonZeroUsize>) {
    if let Some(limit) = limit {
        let count = evicted_bars(bars.len(), limit);
        if count > 0 {
            bars.drain(..count);
            shrink_bars(bars);
        }
    }
}

/// The bars a series keeps when it exceeds `limit`: `limit - limit / 8`, seven eighths of it
/// rounded up. Evicting an eighth of the limit at once moves the bars and restarts the studies
/// once per `limit / 8 + 1` new bars instead of on every bar.
pub(super) fn kept_bars(limit: NonZeroUsize) -> usize {
    limit.get() - limit.get() / 8
}

/// How many of the oldest of `len` bars `limit` evicts: none within the limit, otherwise all
/// but the newest [`kept_bars`].
pub(super) fn evicted_bars(len: usize, limit: NonZeroUsize) -> usize {
    if len > limit.get() {
        len - kept_bars(limit)
    } else {
        0
    }
}

pub(super) fn shrink_bars(bars: &mut Vec<Bar>) {
    if bars.capacity() / 2 > bars.len() {
        bars.shrink_to_fit();
    }
}

/// Warn about an id that was not handled as a secondary bar series.
pub(super) fn holds_bars(series: SeriesId, call: &str) -> bool {
    if series != SeriesId::MAIN {
        log::warn!("Chart::{call} ignored: {series:?} is not a present bar series");
    }
    series == SeriesId::MAIN
}

#[cfg(all(test, feature = "studies"))]
mod tests;
