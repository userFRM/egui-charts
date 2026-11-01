//! The chart's bars, merged by one set of rules for history and live updates, and the studies
//! computed from them.

mod options;
mod scales;

pub use options::SeriesOptions;
pub(super) use options::{PriceSeries, SeriesLegend, base_line};

use super::change_marks::ChangeMarks;
use super::{Chart, ChartElementId, SeriesId};
use crate::data::FeedState;
use crate::data::merge::{Merged, Origin, merge, valid};
use crate::model::{
    Bar, BarClock, BarData, ChartType, LogicalRange, Marker, MarkerOptions, PriceLine,
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

/// Which bar [`Chart::bar_index_at`] finds for a time no bar opens at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchDirection {
    /// Only a bar opened at the time.
    Exact,
    /// The latest bar opened before the time.
    NearestLeft,
    /// The earliest bar opened after the time.
    NearestRight,
}

/// The bars of a series inside a range of logical indices, from
/// [`Chart::bars_in_logical_range`].
#[derive(Debug, Clone, PartialEq)]
pub struct BarsInRange {
    /// Indices into [`Chart::bars`] of the bars inside the range, empty when none is.
    pub bars: std::ops::Range<usize>,
    /// The number of bars before the range. When there are none, the logical distance from
    /// the range's left edge to the first bar, negated: zero or less.
    pub bars_before: f64,
    /// The number of bars after the range. When there are none, the logical distance from the
    /// last bar to the range's right edge, negated: zero or less.
    pub bars_after: f64,
}

/// Handle of a price line added with [`Chart::add_price_line`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PriceLineId(u64);

impl Chart {
    /// Take the settings request from a legend button or a double-click in a pane: a study, a
    /// bar series shown in a pane other than the main one, or [`SeriesId::MAIN`] from the main
    /// series' legend button.
    pub fn take_indicator_settings_request(&mut self) -> Option<SeriesId> {
        self.indicator_settings_request.take()
    }

    /// Replaces the bars of `series`, for history or a replay window. Invalid bars (a non-finite
    /// value, a high or low on the wrong side of the open and close, negative volume) are dropped
    /// and counted in [`feed_state`](Self::feed_state), except [whitespace](Bar::whitespace) bars,
    /// which keep their slots empty. The rest are sorted by time, and of several
    /// bars with one open time, or in one slot of the timeframe for a chart built with a feed, the
    /// last is kept. Drawings retain their timestamps across replacements and interval changes.
    /// The studies are updated from the first bar that changed on the next [`show`](Self::show),
    /// and near the live edge the view follows a new last bar, as with [`update`](Self::update).
    ///
    /// Bar series accept data; unknown ids and study ids are ignored with a warning.
    pub fn set_data(&mut self, series: SeriesId, bars: Vec<Bar>) {
        if (series == SeriesId::MAIN && self.main_series_present
            || self.price_series.iter().any(|item| item.id == series))
            && self.replay.is_some()
        {
            self.replay_set_data(series, bars);
            self.data_loaded(series);
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
            self.data_loaded(series);
            return;
        }
        if !self.main_series_present || !holds_bars(series, "set_data") {
            return;
        }
        self.replace_main_data(bars);
        self.data_loaded(series);
    }

    pub(super) fn replace_main_data(&mut self, bars: Vec<Bar>) {
        let (mut bars, rejected) = clean(bars, &self.clock());
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
        self.bars_changed(before, first_changed);
    }

    /// Merges `bar` into the bars of `series` as [`Sink::push_bar`](crate::data::Sink::push_bar)
    /// does, matched by open time, or by the slot of the timeframe for a chart built with a feed:
    /// a bar that matches a stored bar replaces it, a newer one is appended and an older one is
    /// inserted in order. An invalid bar is dropped and counted in
    /// [`feed_state`](Self::feed_state). Drawings move with their bars, studies follow them on the
    /// next [`show`](Self::show), and near the live edge the view follows a new last bar. A merged
    /// bar is reported by [`ChartEvent::DataUpdated`](super::ChartEvent::DataUpdated).
    ///
    /// Bar series accept data; unknown ids and study ids are ignored with a warning.
    pub fn update(&mut self, series: SeriesId, bar: Bar) {
        let changed = if (series == SeriesId::MAIN && self.main_series_present
            || self.price_series.iter().any(|item| item.id == series))
            && self.replay.is_some()
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
        } else if self.main_series_present && holds_bars(series, "update") {
            self.apply(vec![(bar, Origin::Live)])
                .first_changed
                .is_some()
        } else {
            false
        };
        if changed {
            self.data_updated(series);
        }
    }

    /// Removes the last `count` bars of `series`, or all of them for a larger count, and returns
    /// them oldest first, leaving the other bars in place. Drawings, studies and the view follow
    /// as for [`update`](Self::update), and [`ChartEvent::DataUpdated`](super::ChartEvent::DataUpdated)
    /// reports the removal. During a replay the bars are removed from the retained history.
    ///
    /// Bar series accept this; unknown ids and study ids are ignored with a warning.
    pub fn pop_bars(&mut self, series: SeriesId, count: usize) -> Vec<Bar> {
        let present = series == SeriesId::MAIN && self.main_series_present
            || self.price_series.iter().any(|item| item.id == series);
        let removed = if present && self.replay.is_some() {
            self.replay_pop(series, count)
        } else if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            item.revision = crate::model::next_revision();
            item.bars.split_off(item.bars.len().saturating_sub(count))
        } else if self.main_series_present && holds_bars(series, "pop_bars") {
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
            self.bars_changed(before, first_changed);
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
        if self.replay.is_some() {
            self.apply_replay_window();
        } else if self.enforce_bar_limit(series) > 0 {
            self.data_updated(series);
        }
    }

    /// The retained-bar limit of `series`, or `None` when unlimited or absent.
    pub fn bar_limit(&self, series: SeriesId) -> Option<NonZeroUsize> {
        self.bar_limits.get(&series).copied()
    }

    /// Removes up to `count` oldest bars of `series`, returning them oldest first. Studies
    /// restart on the retained window; drawings keep their timestamps and the view keeps its
    /// time position where possible. During replay this removes from the full retained history
    /// and its matching finer-source slots.
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
        let removed = if let Some(replay) = &mut self.replay {
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
                self.pending_from.fill(Some(0));
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
            || self.main_series_present && holds_bars(series, call)
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
    /// [whitespace](Bar::whitespace) bar. Removed series, volume and studies hold no bars.
    pub fn bars(&self, series: SeriesId) -> &[Bar] {
        if series == SeriesId::MAIN && self.main_series_present {
            &self.state.data().bars
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == series)
                .map_or(&[], |item| item.bars.as_slice())
        }
    }

    /// The index in [`bars`](Self::bars) of the bar of `series` opened at `time`, or else of its
    /// nearest neighbour in `direction`. `None` when there is no such bar.
    pub fn bar_index_at(
        &self,
        series: SeriesId,
        time: DateTime<Utc>,
        direction: MismatchDirection,
    ) -> Option<usize> {
        let bars = self.bars(series);
        let at = bars.partition_point(|bar| bar.time < time);
        if bars.get(at).is_some_and(|bar| bar.time == time) {
            return Some(at);
        }
        match direction {
            MismatchDirection::Exact => None,
            MismatchDirection::NearestLeft => at.checked_sub(1),
            MismatchDirection::NearestRight => (at < bars.len()).then_some(at),
        }
    }

    /// The bars of `series` inside `range`, a range of logical indices of the time scale such as
    /// [`visible_logical_range`](Self::visible_logical_range) returns, with the bars before and
    /// after it, for example to request older history before the user scrolls to its start. The range's edges are rounded to whole indices; a bar's logical index is
    /// the position of its open time on the time scale. `None` for a series without bars or a
    /// chart without a time scale.
    pub fn bars_in_logical_range(
        &self,
        series: SeriesId,
        range: LogicalRange,
    ) -> Option<BarsInRange> {
        let bars = self.bars(series);
        let shown = self.state.shown();
        let (first, last) = (bars.first()?, bars.last()?);
        let index = |bar: &Bar| crate::model::index_at(shown, bar.time).unwrap_or(f64::NAN);
        let left = f64::from(range.left).round();
        let right = f64::from(range.right).round();
        shown.first()?;
        let start = bars.partition_point(|bar| index(bar) < left);
        let end = bars.partition_point(|bar| index(bar) <= right).max(start);
        Some(BarsInRange {
            bars: start..end,
            bars_before: if start > 0 {
                start as f64
            } else {
                left - index(first)
            },
            bars_after: if end < bars.len() {
                (bars.len() - end) as f64
            } else {
                index(last) - right
            },
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
    /// `Some` for every present bar series.
    pub fn feed_state(&self, series: SeriesId) -> Option<&FeedState> {
        if series == SeriesId::MAIN && self.main_series_present {
            Some(&self.feed)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == series)
                .map(|item| &item.feed)
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
        let tail = before.tail.as_ref().map(|bar| bar.time);
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
        if !self.bars_changed(before, merged.first_changed)
            && !self.state.shows_elements()
            && let Some(tail) = tail
        {
            // The right edge is the last bar plus the right offset: without this a view scrolled
            // back would move one bar left per appended bar. A bar stamped later in the slot of
            // the last bar replaces it and appends nothing.
            let bars = &self.state.data().bars;
            let tail = clock.bar_open(tail);
            let appended =
                bars.len() - bars.partition_point(|bar| clock.bar_open(bar.time) <= tail);
            let time_scale = self.state.time_scale_mut();
            time_scale.set_right_offset(time_scale.right_offset_precise() - appended as f64);
        }
        if self.enforce_bar_limit(SeriesId::MAIN) > 0 {
            merged.first_changed = Some(0);
        }
        merged
    }

    /// Removes a bar series or study and clears its selection. Removing the main series pauses
    /// its feed and retains the time axis for the remaining series and drawings. Undo of the
    /// [`user_action`](Self::user_action) that removed a bar series restores it with its
    /// markers, price lines, formatter and bar limit, and for the main series the feed follows
    /// [`set_feed_paused`](Self::set_feed_paused) again. Returns `false` for an unknown or
    /// already removed id.
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
    /// next frame, for all changes since the previous one. Returns whether the view followed the
    /// live edge.
    fn bars_changed(&mut self, before: Edge, first_changed: Option<usize>) -> bool {
        if let Some(from) = first_changed {
            for pending in &mut self.pending_from {
                *pending = Some(pending.map_or(from, |pending| pending.min(from)));
            }
        }
        self.follow_live_edge(before)
    }

    fn edge(&self) -> Edge {
        let bars = &self.state.data().bars;
        Edge {
            // Within ~1.5 bars of the right edge. With fix_right_edge the right offset is clamped
            // to <= 0, so near means no farther than 1.5 bars to the left of the edge.
            near_live: self.state.time_scale().is_near_live_edge(),
            len: bars.len(),
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
        #[cfg(feature = "drawings")]
        self.project_drawing_points();
    }

    /// Scrolls to the live edge when the view was near it and a bar was appended, or the last
    /// bar replaced while the options allow shifting on whitespace replacement. Returns whether
    /// it did.
    fn follow_live_edge(&mut self, before: Edge) -> bool {
        let bars = &self.state.data().bars;
        let tail = before.tail.as_ref().zip(bars.last());
        // Appended only if the last bar moved forward in time: older bars loaded in front make
        // the series longer and keep its last bar. They need no change of the right offset
        // either, since the bar indices and the base index both grow by their count.
        let appended =
            bars.len() > before.len && tail.is_some_and(|(old, new)| new.time > old.time);
        // Bitwise, so that the NaN close of an unchanged whitespace bar is unchanged.
        let tail_changed = tail.is_some_and(|(old, new)| {
            old.time != new.time || old.close.to_bits() != new.close.to_bits()
        });
        let options = &self.chart_options.time_scale;
        let shift_for_whitespace = tail_changed
            && !appended
            && options.allow_shift_visible_range_on_whitespace_replacement;
        let follow = options.shift_visible_range_on_new_bar
            && before.near_live
            && (appended || shift_for_whitespace);
        if follow {
            self.state.time_scale_mut().scroll_to_realtime();
            // A pending start index would move the view away from the edge again.
            self.pending_start_idx = None;
        }
        follow
    }
}

/// The bars and the view before the bars change, for [`Chart::bars_changed`].
struct Edge {
    near_live: bool,
    len: usize,
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
mod tests {
    use super::super::test_frames::frame;
    use super::*;
    #[cfg(feature = "drawings")]
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::studies::{Indicator, IndicatorValue, SMA};
    use chrono::{DateTime, TimeDelta, Utc};
    use std::time::Duration;

    /// Bars span 100..110 and the overlay reaches 120: 10% of the pane above and below adds an
    /// eighth of the span on each side, 97.5..122.5. Every line of a study counts and non-finite
    /// values do not; without eligible values, or with a volume profile, the bars alone give
    /// 98.75..111.25. Relative modes use close 110.
    #[test]
    fn autoscale_includes_only_visible_overlay_values() {
        use crate::scales::PriceScaleMode;
        use crate::studies::{CustomIndicator, VolProfileVisible};
        use crate::widget::test_frames::chart;

        for (mode, fitted, bars_only) in [
            (PriceScaleMode::Normal, (97.5, 122.5), (98.75, 111.25)),
            (
                PriceScaleMode::Percentage,
                (-125.0 / 11.0, 125.0 / 11.0),
                (-112.5 / 11.0, 12.5 / 11.0),
            ),
            (
                PriceScaleMode::IndexedTo100,
                (975.0 / 11.0, 1225.0 / 11.0),
                (987.5 / 11.0, 1112.5 / 11.0),
            ),
        ] {
            for case in [
                "single",
                "multiple",
                "excluded",
                "hidden",
                "plot_hidden",
                "interval_hidden",
                "pane",
                "short",
                "profile",
                "independent",
                "no_scale",
                "displaced_future",
                "main_hidden",
                "price_only",
                "price_only_shared",
                "price_only_shared_other",
                "price_only_restored",
                "price_only_excluded",
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[100.0], &[110.0]);
                chart.config_mut().right_axis_scale_mode = mode;
                frame(&ctx, &mut chart, 0.0, Vec::new());
                chart.set_visible_bars(20);
                chart.set_start_idx(40);
                frame(&ctx, &mut chart, 0.02, Vec::new());
                let (start, end) = chart.state.visible_range();
                assert!(start > 0 && end < 100);
                if case == "displaced_future" {
                    chart.set_start_idx(85);
                    frame(&ctx, &mut chart, 0.025, Vec::new());
                    chart.state.time_scale_mut().set_right_offset(8.0);
                    assert!(
                        chart
                            .state
                            .time_scale()
                            .visible_logical_range()
                            .to_strict_range()
                            .1
                            > 104
                    );
                }
                let study = CustomIndicator::new("Overlay", move |bars| {
                    let len = if case == "short" { start } else { bars.len() };
                    (0..len)
                        .map(|i| {
                            if case == "displaced_future" {
                                if i + 1 == bars.len() {
                                    IndicatorValue::Single(120.0)
                                } else {
                                    IndicatorValue::None
                                }
                            } else if i < start {
                                IndicatorValue::Single(-10_000.0)
                            } else if i >= end {
                                IndicatorValue::Single(10_000.0)
                            } else if case == "multiple" {
                                IndicatorValue::Multiple(vec![
                                    105.0,
                                    120.0,
                                    f64::NAN,
                                    f64::INFINITY,
                                    f64::NEG_INFINITY,
                                ])
                            } else {
                                IndicatorValue::Single(120.0)
                            }
                        })
                        .collect()
                })
                .with_line_count(if case == "multiple" { 5 } else { 1 })
                .with_overlay(case != "pane")
                .with_offsets(vec![if case == "displaced_future" { 5 } else { 0 }])
                .with_scale(match case {
                    "independent" => crate::studies::StudyScale::Independent,
                    "no_scale" => crate::studies::StudyScale::None,
                    "pane" => crate::studies::StudyScale::Independent,
                    "price_only_shared" => {
                        crate::studies::StudyScale::Shared(crate::model::SeriesScale::Right)
                    }
                    "price_only_shared_other" => {
                        crate::studies::StudyScale::Shared(crate::model::SeriesScale::Left)
                    }
                    _ => crate::studies::StudyScale::Main,
                });
                let study: Box<dyn Indicator> = if case == "profile" {
                    Box::new(VolProfileVisible::default())
                } else {
                    Box::new(study)
                };
                if case == "interval_hidden" {
                    chart.set_timeframe(crate::model::Timeframe::Min1);
                }
                let id = chart.add_indicator(study);
                if matches!(case, "plot_hidden" | "interval_hidden") {
                    let study = chart.indicator_mut(id).unwrap();
                    let mut settings = study.settings();
                    if case == "plot_hidden" {
                        settings.plots[0].visible = false;
                    } else {
                        settings
                            .hidden_intervals
                            .push(crate::model::Timeframe::Min1);
                    }
                    study.apply_settings(&[], settings).unwrap();
                }
                chart.set_indicator_visible(id, case != "hidden");
                assert!(chart.set_indicator_autoscale(
                    id,
                    !matches!(case, "excluded" | "price_only_excluded")
                ));
                chart.set_main_series_visible(case != "main_hidden");
                if case.starts_with("price_only") {
                    #[cfg(feature = "ui")]
                    {
                        let mut menu = crate::ui::top_toolbar::SettingsMenu::new();
                        menu.settings.scale_price_chart_only = true;
                        chart.update_config(menu.apply_to_config(chart.config().clone()));
                    }
                    #[cfg(not(feature = "ui"))]
                    {
                        chart.config_mut().scale_price_chart_only = true;
                    }
                    if matches!(case, "price_only_restored" | "price_only_excluded") {
                        frame(&ctx, &mut chart, 0.03, Vec::new());
                        chart.config_mut().scale_price_chart_only = false;
                    }
                    assert!(
                        chart
                            .indicators()
                            .find(|(study, _)| *study == id)
                            .unwrap()
                            .1
                            .is_visible()
                    );
                }
                frame(&ctx, &mut chart, 0.04, Vec::new());
                if case == "price_only_shared_other" {
                    let study = chart
                        .indicators()
                        .find(|(study, _)| *study == id)
                        .unwrap()
                        .1;
                    let map =
                        crate::chart::renderers::study_mapping(study, &chart.get_chart_mapping())
                            .price;
                    assert!(map.range().contains(map.transform().to_scale(120.0)));
                }
                let expected = if case == "main_hidden" {
                    let value = crate::scales::PriceTransform { mode, base: 110.0 }.to_scale(120.0);
                    (value - value.abs() * 0.00125, value + value.abs() * 0.00125)
                } else if matches!(
                    case,
                    "single" | "multiple" | "displaced_future" | "price_only_restored"
                ) {
                    fitted
                } else {
                    bars_only
                };
                let range = chart.price_scale_mut().range();
                assert!(
                    (range.min - expected.0).abs() < 1e-10
                        && (range.max - expected.1).abs() < 1e-10,
                    "{mode:?} {case}: {range:?}, expected {expected:?}"
                );
            }
        }
    }

    /// A flat bar at `close`, stamped `seconds` after 10:00.
    fn bar(seconds: i64, close: f64) -> Bar {
        let t =
            "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::seconds(seconds);
        Bar::new(t, close, close, close, close, 1.0)
    }

    /// Streaming keeps the configured live offset, or the same scrolled-back element.
    #[test]
    fn appended_bars_keep_the_view() {
        use crate::model::{ChartType, ChartTypeParams};
        for (name, kind, offset, limit, expected) in [
            (
                "candles at the live edge",
                ChartType::Candles,
                None,
                None,
                [20.0; 3],
            ),
            (
                "renko at the live edge",
                ChartType::Renko,
                None,
                None,
                [20.0; 3],
            ),
            (
                "limited candles at the live edge",
                ChartType::Candles,
                None,
                Some(100),
                [20.0; 3],
            ),
            (
                "limited candles scrolled back",
                ChartType::Candles,
                Some(-30.0),
                Some(100),
                [-31.0, -32.0, -33.0],
            ),
            (
                "limited renko scrolled back",
                ChartType::Renko,
                Some(-30.0),
                Some(100),
                [-32.0, -34.0, -36.0],
            ),
            (
                "renko scrolled back",
                ChartType::Renko,
                Some(-30.0),
                None,
                [-32.0, -34.0, -36.0],
            ),
        ] {
            let options = crate::config::ChartOptions {
                time_scale: crate::config::TimeScaleOptions {
                    right_offset: 20.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            // Every bar after the first adds two bricks of 10. Brick 167 stays at the right edge.
            let mut chart = Chart::new(BarData::from_bars(
                (0..100)
                    .map(|i| bar(i * 60, 120.0 + 20.0 * i as f64))
                    .collect(),
            ))
            .with_chart_options(options);
            chart.set_chart_type(kind);
            chart.set_chart_type_params(ChartTypeParams {
                renko_brick_size: 10.0,
                ..Default::default()
            });
            let ctx = egui::Context::default();
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.set_bar_limit(SeriesId::MAIN, limit.and_then(NonZeroUsize::new));
            if let Some(offset) = offset {
                chart.state.time_scale_mut().set_right_offset(offset);
            }
            let mut got = Vec::new();
            for i in 100..103 {
                chart.update(SeriesId::MAIN, bar(i * 60, 120.0 + 20.0 * i as f64));
                frame(&ctx, &mut chart, i as f64, Vec::new());
                got.push(chart.state.time_scale().right_offset());
            }
            assert_eq!(got, expected, "{name}");
            if limit.is_some() {
                // The first new bar overflows the limit of 100 and evicts down to
                // 100 - 100 / 8 = 88; the other two are kept.
                assert_eq!(
                    chart.bars(SeriesId::MAIN).len(),
                    90,
                    "{name}: retained bars"
                );
            }
        }
    }

    /// Limits and explicit removal operate on retained replay history and independent series.
    /// A limit of 8 evicts down to 8 - 8 / 8 = 7 bars whenever a series holds more:
    /// `set_bar_limit` and `set_data` keep the newest 7 of 9 bars, the first update fills the
    /// limit again and the second evicts down to the 7 bars from 10:04 to 10:10.
    #[test]
    fn bar_limits_cover_replay_and_added_series() {
        use crate::widget::SeriesOptions;
        let initial: Vec<_> = (0..9).map(|i| bar(i * 60, i as f64 + 1.0)).collect();
        let kept: Vec<_> = (4..11).map(|i| bar(i * 60, i as f64 + 1.0)).collect();
        for replay in [false, true] {
            for main in [false, true] {
                let mut chart = Chart::new(BarData::from_bars(initial.clone()));
                let id = if main {
                    SeriesId::MAIN
                } else {
                    chart.add_series(SeriesOptions::default())
                };
                chart.set_data(id, initial.clone());
                if replay {
                    chart.set_replay_time(Some(bar(120, 0.0).time));
                }
                chart.set_bar_limit(id, NonZeroUsize::new(8));
                chart.set_data(id, initial.clone());
                chart.update(id, bar(540, 10.0));
                chart.update(id, bar(600, 11.0));
                if replay {
                    chart.set_replay_time(None);
                }
                assert_eq!(chart.bars(id), kept, "replay {replay}, main {main}");
                assert_eq!(chart.remove_oldest_bars(id, 0), []);
                if replay {
                    chart.set_replay_time(Some(bar(601, 0.0).time));
                }
                assert_eq!(chart.remove_oldest_bars(id, usize::MAX), kept);
                if replay {
                    chart.set_replay_time(None);
                }
                assert!(chart.bars(id).is_empty());
                chart.set_bar_limit(id, None);
                chart.set_data(id, initial.clone());
                assert_eq!(chart.bars(id), initial);
                assert!(chart.bar_limit(id).is_none());
            }
        }
    }

    /// Guards the wiring of every bar update: the merged bars, the studies, the revision that span
    /// caches are keyed on and the count of invalid bars must describe the same series. Studies
    /// updated from a bar after the first change keep stale values, and ones not updated after a
    /// removal keep values for removed bars; a revision bumped on every update rebuilds the caches
    /// each tick, and one kept on an insert or a removal leaves them stale; out-of-order bars must
    /// not be dropped, nor invalid bars drawn; a drawing must stay on its bar when bars are
    /// inserted in front of it.
    #[test]
    #[cfg(feature = "drawings")]
    fn updates_reach_bars_studies_and_revision() {
        enum Action {
            Update(Bar),
            SetData(Vec<Bar>),
            Limit(usize),
            RemoveOldest(usize),
        }
        use Action::{Limit, RemoveOldest, SetData, Update};
        // Five one-minute bars closing at 1 to 5.
        let start: Vec<Bar> = (0..5).map(|i| bar(i * 60, (i + 1) as f64)).collect();
        let mut invalid = bar(300, 6.0);
        invalid.high = 5.0;
        let series = |bars: &[(i64, f64)]| {
            bars.iter()
                .map(|&(t, close)| bar(t, close))
                .collect::<Vec<_>>()
        };
        // Expected bars, then the SMA(2) of their closes, which has no value on the first bar.
        #[rustfmt::skip]
        let cases = [
            ("retained window slides on append", vec![Limit(3), Update(bar(300, 6.0))],
                series(&[(180, 4.0), (240, 5.0), (300, 6.0)]),
                vec![None, Some(4.5), Some(5.5)], true, 0),
            ("oldest bars removed", vec![RemoveOldest(2)],
                series(&[(120, 3.0), (180, 4.0), (240, 5.0)]),
                vec![None, Some(3.5), Some(4.5)], true, 0),
            ("replacement respects limit", vec![Limit(4), SetData(start.clone())],
                start[1..].to_vec(),
                vec![None, Some(2.5), Some(3.5), Some(4.5)], true, 0),
            ("last-bar update", vec![Update(bar(240, 6.0))],
                series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 6.0)]),
                vec![None, Some(1.5), Some(2.5), Some(3.5), Some(5.0)], false, 0),
            ("append", vec![Update(bar(300, 6.0))],
                series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 5.0), (300, 6.0)]),
                vec![None, Some(1.5), Some(2.5), Some(3.5), Some(4.5), Some(5.5)], false, 0),
            ("correction", vec![Update(bar(120, 9.0))],
                series(&[(0, 1.0), (60, 2.0), (120, 9.0), (180, 4.0), (240, 5.0)]),
                vec![None, Some(1.5), Some(5.5), Some(6.5), Some(4.5)], true, 0),
            ("out of order", vec![Update(bar(90, 7.0))],
                series(&[(0, 1.0), (60, 2.0), (90, 7.0), (120, 3.0), (180, 4.0), (240, 5.0)]),
                vec![None, Some(1.5), Some(4.5), Some(5.0), Some(3.5), Some(4.5)], true, 0),
            ("invalid", vec![Update(invalid.clone()), SetData([start.clone(), vec![invalid]].concat())],
                start.clone(),
                vec![None, Some(1.5), Some(2.5), Some(3.5), Some(4.5)], false, 2),
            ("replace", vec![SetData(series(&[(120, 3.0), (0, 1.0), (240, 5.0), (60, 2.0), (180, 4.0), (120, 8.0)]))],
                series(&[(0, 1.0), (60, 2.0), (120, 8.0), (180, 4.0), (240, 5.0)]),
                vec![None, Some(1.5), Some(5.0), Some(6.0), Some(4.5)], true, 0),
            ("replay step", vec![SetData([start.clone(), vec![bar(300, 6.0)]].concat())],
                series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 5.0), (300, 6.0)]),
                vec![None, Some(1.5), Some(2.5), Some(3.5), Some(4.5), Some(5.5)], false, 0),
            ("truncate", vec![SetData(start[..4].to_vec())],
                start[..4].to_vec(),
                vec![None, Some(1.5), Some(2.5), Some(3.5)], true, 0),
            ("sorted with a repeated time", vec![SetData(series(&[(0, 1.0), (60, 2.0), (120, 3.0), (120, 8.0), (180, 4.0)]))],
                series(&[(0, 1.0), (60, 2.0), (120, 8.0), (180, 4.0)]),
                vec![None, Some(1.5), Some(5.0), Some(6.0)], true, 0),
            ("correction, then append", vec![Update(bar(120, 9.0)), Update(bar(300, 6.0))],
                series(&[(0, 1.0), (60, 2.0), (120, 9.0), (180, 4.0), (240, 5.0), (300, 6.0)]),
                vec![None, Some(1.5), Some(5.5), Some(6.5), Some(4.5), Some(5.5)], true, 0),
        ];
        let ctx = egui::Context::default();
        for (name, actions, bars, sma, revised, rejected) in cases {
            let mut chart = Chart::new(BarData::from_bars(start.clone()));
            let id = chart.add_indicator(Box::new(SMA::new(2)));
            // A drawing anchored on the bar at 10:03, the fourth.
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.chart_points.push(ChartPoint::new(3.0, 4.0));
            chart.drawings_mut().add_synced_drawing(drawing).unwrap();
            let revision = chart.state.revision();
            for action in actions {
                match action {
                    Update(bar) => chart.update(SeriesId::MAIN, bar),
                    SetData(bars) => chart.set_data(SeriesId::MAIN, bars),
                    Limit(count) => chart.set_bar_limit(SeriesId::MAIN, NonZeroUsize::new(count)),
                    RemoveOldest(count) => {
                        assert_eq!(
                            chart.remove_oldest_bars(SeriesId::MAIN, count),
                            start[..count]
                        );
                    }
                }
            }
            frame(&ctx, &mut chart, 0.0, Vec::new());
            assert_eq!(chart.bars(SeriesId::MAIN), bars, "{name}");
            let anchor = chart.drawings_mut().drawings[0].chart_points[0].bar_idx as usize;
            assert_eq!(
                chart.bars(SeriesId::MAIN)[anchor].time,
                start[3].time,
                "{name}: drawing"
            );
            let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
            let values: Vec<Option<f64>> = study
                .values()
                .to_rows()
                .iter()
                .map(|value| match value {
                    IndicatorValue::Single(value) => Some(*value),
                    _ => None,
                })
                .collect();
            assert_eq!(values, sma, "{name}: SMA");
            assert_eq!(
                chart.state.revision() != revision,
                revised,
                "{name}: revision"
            );
            let feed = chart.feed_state(SeriesId::MAIN).unwrap();
            assert_eq!(feed.rejected_bars, rejected, "{name}: rejected");
        }
    }

    /// Merges retain anchor times, interpolating gaps and using the most common edge spacing.
    /// After the inserts, 30 s occurs most: -60 / 30 = -2 and 7 + (360 - 240) / 30 = 11.
    #[test]
    #[cfg(feature = "drawings")]
    fn inserted_bars_preserve_drawing_times_including_future_anchors() {
        let start: Vec<Bar> = (0..5).map(|i| bar(i * 60, (i + 1) as f64)).collect();
        let live = |seconds| (bar(seconds, 1.0), Origin::Live);
        #[rustfmt::skip]
        let cases = [
            ("inserts at two positions", vec![live(90), live(100), live(210)],
                vec![-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 6.0], vec![-2.0, 0.0, 1.0, 4.0, 5.0, 7.0, 11.0]),
            ("append", vec![live(300)], vec![3.0, 6.0], vec![3.0, 6.0]),
            ("history in front", vec![(bar(-60, 1.0), Origin::History)], vec![-1.0, 0.0], vec![0.0, 1.0]),
        ];
        for (name, incoming, points, moved) in cases {
            let mut chart = Chart::new(BarData::from_bars(start.clone()));
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = points.iter().map(|&at| ChartPoint::new(at, 1.0)).collect();
            chart.drawings_mut().add_synced_drawing(drawing).unwrap();
            chart.apply(incoming);
            let points = &chart.drawings().drawings[0].chart_points;
            let after: Vec<f32> = points.iter().map(|point| point.bar_idx).collect();
            assert_eq!(after, moved, "{name}");
        }
    }

    /// A handle moves, restyles and removes only its own line, on the main series and on an
    /// added series. Replacing a series' lines or removing the series retires its handles, and a
    /// study takes no line. The moved line's tag and title use their own colours.
    #[test]
    fn price_line_handles_change_only_their_own_line() {
        use crate::model::PriceLine;
        use crate::widget::test_frames::{chart, shapes};
        use egui::{Color32, Shape};

        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0, 120.0]);
        chart.config.show_volume = false;
        let kept = PriceLine::new(105.0, Color32::GOLD);
        let kept_id = chart.add_price_line(SeriesId::MAIN, kept.clone()).unwrap();
        let moved = chart
            .add_price_line(SeriesId::MAIN, PriceLine::new(112.0, Color32::RED))
            .unwrap();
        let (fill, text, title) = (
            Color32::from_rgb(10, 20, 30),
            Color32::from_rgb(200, 210, 220),
            Color32::from_rgb(1, 2, 3),
        );
        let restyled = PriceLine::new(115.0, Color32::BLUE)
            .with_axis_label_colors(fill, text)
            .with_title("Target")
            .with_title_color(title);
        assert!(chart.update_price_line(moved, restyled.clone()));
        assert_eq!(chart.price_line(kept_id), Some(&kept));
        assert_eq!(chart.price_line(moved), Some(&restyled));

        frame(&ctx, &mut chart, 0.0, Vec::new());
        let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
        let y = chart.last_price_map.price_to_y(115.0);
        let tag = painted
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect(tag) if tag.fill == fill => Some(tag.rect),
                _ => None,
            })
            .expect("the moved line's tag");
        assert!((tag.center().y - y).abs() < 1.0);
        assert!(painted.iter().any(|shape| matches!(shape,
            Shape::Text(t) if t.galley.text() == "Target"
                && t.galley.job.sections[0].format.color == title)));
        assert!(painted.iter().any(|shape| matches!(shape,
            Shape::Text(t) if t.galley.text() == "115.00"
                && t.galley.job.sections[0].format.color == text)));

        assert!(chart.remove_price_line(kept_id));
        assert!(!chart.remove_price_line(kept_id));
        assert!(!chart.update_price_line(kept_id, kept.clone()));
        assert_eq!(chart.price_line(moved), Some(&restyled));
        chart.set_price_lines(SeriesId::MAIN, vec![kept.clone()]);
        assert_eq!(chart.price_line(moved), None);

        let series = chart.add_series(crate::widget::SeriesOptions::default());
        let line = chart.add_price_line(series, kept.clone()).unwrap();
        assert!(chart.update_price_line(line, restyled.clone()));
        assert_eq!(chart.price_line(line), Some(&restyled));
        assert!(chart.remove_series(series));
        assert_eq!(chart.price_line(line), None);
        let study = chart.add_indicator(Box::new(SMA::new(3)));
        assert_eq!(chart.add_price_line(study, kept), None);
    }

    /// A price-anchored marker in view widens the fit unless its options say otherwise, and one
    /// out of view never does. Each layer paints the main series' marker and its text over the
    /// main series' candles, the text on a background, and the marker under or over a series
    /// added after it and a price line.
    #[test]
    fn marker_options_fit_and_layer_the_markers() {
        use crate::model::{MarkerOptions, MarkerPos, MarkerShape, MarkerZOrder, PriceLine};
        use crate::widget::test_frames::{chart, shapes};
        use egui::{Color32, Shape};

        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.set_theme_override(Some(theme.clone()));
            chart.config.show_volume = false;
            chart.set_visible_bars(20);
            let bars = chart.bars(SeriesId::MAIN).to_vec();
            let series = chart.add_series(crate::widget::SeriesOptions {
                color: Color32::GREEN,
                ..Default::default()
            });
            chart.set_data(series, bars.clone());
            chart.set_price_lines(
                SeriesId::MAIN,
                vec![
                    PriceLine::new(105.0, Color32::BLUE).with_style(crate::model::LineStyle::Solid),
                ],
            );
            let marker = |time, price| {
                Marker::new(time)
                    .with_shape(MarkerShape::Circle)
                    .with_color(Color32::RED)
                    .with_text("Fill")
                    .at_price(price, MarkerPos::AtPriceMiddle)
            };
            let markers = vec![marker(bars[95].time, 150.0), marker(bars[0].time, 300.0)];
            chart.set_markers(SeriesId::MAIN, markers.clone());
            assert_eq!(chart.markers(SeriesId::MAIN).len(), markers.len());
            assert!(chart.markers(series).is_empty());

            for (auto_scale, z_order) in [
                (false, MarkerZOrder::Normal),
                (true, MarkerZOrder::Normal),
                (true, MarkerZOrder::AboveSeries),
                (true, MarkerZOrder::Top),
            ] {
                chart.set_marker_options(
                    SeriesId::MAIN,
                    MarkerOptions {
                        auto_scale,
                        z_order,
                    },
                );
                frame(&ctx, &mut chart, 0.0, Vec::new());
                let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
                let max = chart.state.price_scale().range().max;
                assert_eq!(max > 150.0, auto_scale, "{max}");
                assert!(max < 300.0, "{max}");
                if !auto_scale {
                    continue;
                }
                let line = |color| {
                    let at = |shape: &Shape| match shape {
                        Shape::LineSegment { stroke, .. } => stroke.color == color,
                        Shape::Path(path) => {
                            path.stroke.color == egui::epaint::ColorMode::Solid(color)
                        }
                        _ => false,
                    };
                    (
                        painted.iter().position(at).unwrap(),
                        painted.iter().rposition(at).unwrap(),
                    )
                };
                let mark = painted
                    .iter()
                    .position(|shape| matches!(shape, Shape::Circle(c) if c.fill == Color32::RED))
                    .unwrap();
                let (series_line, price_line) = (line(Color32::GREEN), line(Color32::BLUE));
                let candles = painted
                    .iter()
                    .rposition(|shape| {
                        matches!(shape, Shape::Rect(r) if r.fill == chart.config().bullish_color
                        && chart.get_rendered_price_rect().contains_rect(r.rect))
                    })
                    .unwrap();
                let text = painted
                    .iter()
                    .position(|shape| matches!(shape, Shape::Text(t) if t.galley.text() == "Fill"))
                    .unwrap();
                assert!(candles < mark && candles + 1 < text, "{z_order:?}");
                assert!(
                    matches!(&painted[text - 1], Shape::Rect(r) if r.fill == theme.chart.bg && r.rect.contains_rect(painted[text].visual_bounding_rect())),
                    "{z_order:?}: the text has a background"
                );
                assert!(
                    match z_order {
                        MarkerZOrder::Normal => mark < series_line.0,
                        MarkerZOrder::AboveSeries => series_line.1 < mark && mark < price_line.0,
                        MarkerZOrder::Top => price_line.1 < mark,
                    },
                    "{z_order:?}: marker {mark}, series {series_line:?}, price line {price_line:?}"
                );
            }
        }
    }

    /// A live update that raises or lowers the close of a stored bar marks its new close in the
    /// rise or the fall colour, replacing the bar's previous mark, on the main and on an added
    /// series. An unchanged close, a new bar and replaced history add no mark, and a mark goes
    /// once its duration has passed.
    #[test]
    fn change_markers_mark_rises_and_falls_of_the_close() {
        use crate::model::ChangeMarkers;
        use crate::widget::test_frames::{chart, shapes};
        use egui::{Color32, Pos2, Shape};

        let (up, down) = (Color32::from_rgb(1, 200, 1), Color32::from_rgb(200, 1, 1));
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config.show_volume = false;
        let options = ChangeMarkers {
            up_color: up,
            down_color: down,
            duration: Duration::from_secs(600),
        };
        chart.set_change_markers(SeriesId::MAIN, Some(options));
        let series = chart.add_series(crate::widget::SeriesOptions::default());
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        chart.set_data(series, bars.clone());
        chart.set_change_markers(series, Some(options));
        let last = bars.last().unwrap().time;
        let bar = |close: f64| Bar::new(last, 100.0, close.max(110.0), 100.0, close, 1.0);
        let marks = |chart: &mut Chart| {
            shapes(&ctx, chart, 0.0, Vec::new())
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::Circle(c) if c.radius == 4.0 && (c.fill == up || c.fill == down) => {
                        Some((c.center, c.fill))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        marks(&mut chart);
        let at = |chart: &Chart, close| {
            Pos2::new(
                chart.get_chart_mapping().idx_to_x(99),
                chart.last_price_map.price_to_y(close),
            )
        };
        for (close, color) in [(108.0, down), (109.0, up), (109.0, up)] {
            chart.update(SeriesId::MAIN, bar(close));
            let painted = marks(&mut chart);
            assert_eq!(painted, vec![(at(&chart, close), color)], "close {close}");
        }
        chart.update(
            SeriesId::MAIN,
            Bar::new(last + TimeDelta::days(1), 1.0, 2.0, 1.0, 2.0, 1.0),
        );
        assert_eq!(marks(&mut chart).len(), 1, "a new bar is not a change");
        chart.update(series, bar(120.0));
        assert_eq!(
            marks(&mut chart).iter().filter(|mark| mark.1 == up).count(),
            2
        );
        chart.set_data(SeriesId::MAIN, bars.clone());
        chart.set_data(series, bars);
        assert!(
            marks(&mut chart).is_empty(),
            "replaced history drops the marks"
        );
        chart.set_change_markers(
            SeriesId::MAIN,
            Some(ChangeMarkers {
                duration: Duration::ZERO,
                ..options
            }),
        );
        chart.update(SeriesId::MAIN, bar(105.0));
        assert!(
            marks(&mut chart).is_empty(),
            "an expired mark is not painted"
        );
    }
}
