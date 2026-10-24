//! The chart's bars, merged by one set of rules for history and live updates, and the studies
//! computed from them.

use super::{Chart, ChartElementId, SeriesId};
use crate::data::FeedState;
use crate::data::merge::{Merged, Origin, merge, valid};
use crate::model::{Bar, BarClock, BarData, Marker, PriceLine};
use crate::studies::Indicator;
use std::collections::BTreeSet;
use web_time::Instant;

impl Chart {
    /// Take the study settings request from a legend button or a double-click in a study pane.
    pub fn take_indicator_settings_request(&mut self) -> Option<SeriesId> {
        self.indicator_settings_request.take()
    }

    /// Replaces the bars of `series`, for history or a replay window. Invalid bars (a non-finite
    /// value, a high or low on the wrong side of the open and close, negative volume) are dropped
    /// and counted in [`feed_state`](Self::feed_state), the rest sorted by time, and of several
    /// bars with one open time, or in one slot of the timeframe for a chart built with a feed, the
    /// last is kept. Drawings retain their timestamps across replacements and interval changes.
    /// The studies are updated from the first bar that changed on the next [`show`](Self::show),
    /// and near the live edge the view follows a new last bar, as with [`update`](Self::update).
    ///
    /// Only [`SeriesId::MAIN`] holds bars; a call with another id is ignored with a warning.
    pub fn set_data(&mut self, series: SeriesId, bars: Vec<Bar>) {
        if !holds_bars(series, "set_data") {
            return;
        }
        let (bars, rejected) = clean(bars, &self.clock());
        self.feed.rejected_bars += rejected;
        self.from_ticks.clear();
        self.pending_box_sizes = self
            .chart_type_params
            .size_methods()
            .map(|method| method != crate::model::BoxSizeMethod::Fixed);
        let before = self.edge();
        self.anchor_drawing_points();
        let first_changed = self.state.set_data(BarData::from_bars(bars));
        self.project_drawing_points();
        self.bars_changed(before, first_changed);
    }

    /// Merges `bar` into the bars of `series` as [`Sink::push_bar`](crate::data::Sink::push_bar)
    /// does, matched by open time, or by the slot of the timeframe for a chart built with a feed:
    /// a bar that matches a stored bar replaces it, a newer one is appended and an older one is
    /// inserted in order. An invalid bar is dropped and counted in
    /// [`feed_state`](Self::feed_state). Drawings move with their bars, studies follow them on the
    /// next [`show`](Self::show), and near the live edge the view follows a new last bar.
    ///
    /// Only [`SeriesId::MAIN`] holds bars; a call with another id is ignored with a warning.
    pub fn update(&mut self, series: SeriesId, bar: Bar) {
        if holds_bars(series, "update") {
            self.apply(vec![(bar, Origin::Live)]);
        }
    }

    /// The bars of `series`, oldest first. Empty for any id but [`SeriesId::MAIN`]: volume and
    /// studies hold no bars.
    pub fn bars(&self, series: SeriesId) -> &[Bar] {
        if series == SeriesId::MAIN {
            &self.state.data().bars
        } else {
            &[]
        }
    }

    /// Replaces the markers drawn on the bars of `series` (trade signals, events).
    ///
    /// Only [`SeriesId::MAIN`] holds bars; a call with another id is ignored with a warning.
    pub fn set_markers(&mut self, series: SeriesId, markers: Vec<Marker>) {
        if holds_bars(series, "set_markers") {
            self.marks = markers;
        }
    }

    /// Replaces the price lines drawn on the scale of `series`.
    ///
    /// Only [`SeriesId::MAIN`] has price lines; another id is ignored with a warning. The price
    /// scale does not fit to them: a line whose price is off the scale or not finite is not drawn.
    pub fn set_price_lines(&mut self, series: SeriesId, lines: Vec<PriceLine>) {
        if holds_bars(series, "set_price_lines") {
            self.price_lines = lines;
        }
    }

    /// What the chart knows about the feed of `series`, such as the bars dropped as invalid.
    /// `Some` for [`SeriesId::MAIN`].
    pub fn feed_state(&self, series: SeriesId) -> Option<&FeedState> {
        (series == SeriesId::MAIN).then_some(&self.feed)
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
        self.anchor_drawing_points();
        let from_ticks = &mut self.from_ticks;
        let mut merged = Merged::default();
        self.state.edit_bars(|bars| {
            merged = merge(bars, from_ticks, clock, incoming);
            merged.first_changed
        });
        self.feed.rejected_bars += merged.rejected;
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
            time_scale.set_right_offset(time_scale.right_offset() - appended as f32);
        }
        merged
    }

    /// Adds a study, computed at once from the bars, which [`show`](Self::show) draws: on the
    /// main pane if [`Indicator::is_overlay`], otherwise in a pane below it. Returns the study's
    /// id, which stays valid when other studies are removed.
    pub fn add_indicator(&mut self, mut indicator: Box<dyn Indicator>) -> SeriesId {
        if !indicator.has_settings() {
            indicator = Box::new(crate::studies::ConfiguredStudy::new(indicator));
        }
        if let Some(timeframe) = self.timeframe() {
            indicator.set_timeframe(timeframe);
        }
        indicator.set_instrument(self.instrument());
        indicator.calculate(&self.state.data().bars);
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.record_study(id);
        self.studies.add(indicator);
        self.study_ids.push(id);
        self.study_autoscale.push(true);
        self.study_inputs.push(String::new());
        self.pending_from.push(None);
        self.next_refresh.push(f64::NEG_INFINITY);
        id
    }

    /// Set the input caption displayed beside a study in the legend.
    pub fn set_indicator_inputs(&mut self, id: SeriesId, inputs: impl Into<String>) -> bool {
        let Some(index) = self.study_ids.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        self.record_study(id);
        self.study_inputs[index] = inputs.into();
        true
    }

    /// The host-supplied input caption of a study, if its id is still present.
    pub fn indicator_inputs(&self, id: SeriesId) -> Option<&str> {
        self.study_ids
            .iter()
            .position(|candidate| *candidate == id)
            .map(|index| self.study_inputs[index].as_str())
    }

    /// Removes the study `id` and clears the selection if it is that study. Returns `false` for
    /// [`SeriesId::MAIN`] and any id that is not a study of the chart.
    pub fn remove_series(&mut self, id: SeriesId) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.record_study(id);
        #[cfg(feature = "studies")]
        if self.volume_study == Some(id) {
            self.volume_study = None;
            self.config.show_volume = false;
            self.volume_setting = false;
        }
        self.studies.remove_indicator(at);
        self.study_ids.remove(at);
        self.study_autoscale.remove(at);
        self.study_inputs.remove(at);
        self.pending_from.remove(at);
        self.next_refresh.remove(at);
        if self.selection.selected_id() == Some(ChartElementId::Indicator(id)) {
            self.selection.deselect();
        }
        true
    }

    /// Shows or hides the study `id`; a hidden study keeps its values. Returns `false` for an id
    /// that is not a study of the chart.
    pub fn set_indicator_visible(&mut self, id: SeriesId, visible: bool) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.record_study(id);
        self.studies.indicators_mut()[at].set_visible(visible);
        true
    }

    /// Show or hide the main price series, its volume and its legend without unloading its bars.
    /// Studies and drawings remain visible and continue to receive data.
    pub fn set_main_series_visible(&mut self, visible: bool) {
        self.main_series_visible = visible;
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

    /// Includes or excludes study `id` from the main price auto-scale, without hiding it.
    /// Defaults to `true`; only visible overlay studies contribute. Returns `false` for an id
    /// that is not a study of the chart.
    pub fn set_indicator_autoscale(&mut self, id: SeriesId, autoscale: bool) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.record_study(id);
        self.study_autoscale[at] = autoscale;
        true
    }

    /// The chart's studies with their ids, in the order they were added. Their values reflect the
    /// bars as of the last [`show`](Self::show), or of the call to
    /// [`add_indicator`](Self::add_indicator). A study that is not updated incrementally can lag
    /// the bars by its wait between full recomputes, at least 0.25 s; after the bars changed from
    /// the first one on, as when `set_data` loads other bars or older bars are loaded in front, it
    /// has no values until it is recomputed.
    pub fn indicators(&self) -> impl Iterator<Item = (SeriesId, &dyn Indicator)> {
        let studies = self.studies.indicators().iter().map(|study| study.as_ref());
        self.study_ids.iter().copied().zip(studies)
    }

    /// The study `id`, to change its settings. Its values reflect the bars as of the last
    /// [`show`](Self::show); the next one recomputes it in full.
    pub fn indicator_mut(&mut self, id: SeriesId) -> Option<&mut (dyn Indicator + 'static)> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        self.record_study(id);
        self.pending_from[at] = Some(0);
        self.next_refresh[at] = f64::NEG_INFINITY;
        Some(self.studies.indicators_mut()[at].as_mut())
    }

    /// Updates every study with a pending change, as of `now` in seconds. An incremental study
    /// whose change starts at its last bar or later is updated at once. Any other change
    /// recomputes the study in full, for at most one study per call: of the studies whose change
    /// starts at the first bar or whose `next_refresh` has passed, the one whose `next_refresh`
    /// is earliest. Its next full recompute then waits, from the end of this one,
    /// the longer of [`REFRESH_INTERVAL`] and four times what this one took. Until its turn, a
    /// study whose change starts at the first bar has no values, since its values belong to
    /// other bars. Returns how long until the next waiting study is due, zero if one is due now.
    pub(super) fn refresh_studies(&mut self, now: f64) -> Option<f64> {
        let instrument = self.instrument().cloned();
        let bars = &self.state.data().bars;
        let studies = self.studies.indicators_mut();
        let mut due = None::<usize>;
        for (at, study) in studies.iter_mut().enumerate() {
            if study.set_instrument(instrument.as_ref()) {
                self.pending_from[at] = Some(0);
            }
            let Some(from) = self.pending_from[at] else {
                continue;
            };
            let len = study.values().len();
            if study.is_incremental() && len > 0 && from + 1 >= len && bars.len() >= len {
                study.update(bars, from);
                self.pending_from[at] = None;
            } else if (from == 0 || now >= self.next_refresh[at])
                && due.is_none_or(|first| self.next_refresh[at] < self.next_refresh[first])
            {
                due = Some(at);
            }
        }
        if let Some(at) = due {
            let started = Instant::now();
            let from = self.pending_from[at].take().unwrap_or(0);
            studies[at].update(bars, from);
            let cost = started.elapsed().as_secs_f64();
            self.next_refresh[at] = now + cost + REFRESH_INTERVAL.max(4.0 * cost);
        }
        let mut wait = None::<f64>;
        for (at, study) in studies.iter_mut().enumerate() {
            let Some(from) = self.pending_from[at] else {
                continue;
            };
            let left = if from == 0 {
                if !study.values().is_empty() {
                    study.update(&[], 0);
                }
                0.0
            } else {
                (self.next_refresh[at] - now).max(0.0)
            };
            wait = Some(wait.map_or(left, |wait| wait.min(left)));
        }
        wait
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
        self.anchor_drawing_points();
        self.chart_type_params
            .resolve_sizes(&self.state.data().bars, &mut self.pending_box_sizes);
        let built = crate::chart::rendering::candles::price_based::elements(
            self.chart_type,
            &self.chart_type_params,
            &self.state.data().bars,
            self.state.revision(),
        );
        self.older_bars_drawn = built.as_ref().is_none_or(|(_, trimmed)| !trimmed);
        let follow = self.chart_options.time_scale.shift_visible_range_on_new_bar;
        self.state
            .set_elements(built.map(|(elements, _)| elements), follow);
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
        let tail_changed =
            tail.is_some_and(|(old, new)| old.time != new.time || old.close != new.close);
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

/// Least time in seconds between two full recomputes of a study while bars stream; a study
/// whose recompute took longer than a quarter of it waits four times that instead.
const REFRESH_INTERVAL: f64 = 0.25;

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
    let merged = merge(&mut clean, &mut BTreeSet::new(), clock, incoming);
    (clean, merged.rejected)
}

/// Whether `series` holds bars, warning about a call that names another. Until compare series
/// exist only [`SeriesId::MAIN`] does.
fn holds_bars(series: SeriesId, call: &str) -> bool {
    if series != SeriesId::MAIN {
        log::warn!("Chart::{call} ignored: {series:?} holds no bars, only SeriesId::MAIN does");
    }
    series == SeriesId::MAIN
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::super::test_frames::frame;
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::studies::{IndicatorValue, RSI, SMA};
    use chrono::{DateTime, TimeDelta, Utc};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Bars span 100..110 and the overlay reaches 120: 10% margins give 98..122. Every line of a
    /// study counts and non-finite values do not; without eligible values, or with a volume
    /// profile, the bars alone give 99..111. Relative modes use close 110.
    #[test]
    fn autoscale_includes_only_visible_overlay_values() {
        use crate::model::ChartType;
        use crate::scales::PriceScaleMode;
        use crate::studies::{CustomIndicator, VolProfileVisible};
        use crate::widget::test_frames::chart;

        for (mode, fitted, bars_only) in [
            (PriceScaleMode::Normal, (98.0, 122.0), (99.0, 111.0)),
            (
                PriceScaleMode::Percentage,
                (-120.0 / 11.0, 120.0 / 11.0),
                (-10.0, 10.0 / 11.0),
            ),
            (
                PriceScaleMode::IndexedTo100,
                (980.0 / 11.0, 1220.0 / 11.0),
                (90.0, 1110.0 / 11.0),
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
                "renko",
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[100.0], &[110.0]);
                if case == "renko" {
                    chart.set_chart_type(ChartType::Renko);
                }
                chart.price_scale_mut().options_mut().mode = mode;
                frame(&ctx, &mut chart, 0.0, Vec::new());
                chart.set_visible_bars(20);
                chart.set_start_idx(40);
                frame(&ctx, &mut chart, 0.02, Vec::new());
                let (start, end) = chart.state.visible_range();
                assert!(start > 0 && end < 100);
                let study = CustomIndicator::new("Overlay", move |bars| {
                    let len = if case == "short" { start } else { bars.len() };
                    (0..len)
                        .map(|i| {
                            if i < start {
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
                .with_line_cnt(if case == "multiple" { 5 } else { 1 })
                .with_overlay(case != "pane");
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
                assert!(chart.set_indicator_autoscale(id, case != "excluded"));
                frame(&ctx, &mut chart, 0.04, Vec::new());
                let expected = if matches!(case, "single" | "multiple") {
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
        for (name, kind, offset, expected) in [
            (
                "candles at the live edge",
                ChartType::Candles,
                None,
                [20.0; 3],
            ),
            ("renko at the live edge", ChartType::Renko, None, [20.0; 3]),
            (
                "renko scrolled back",
                ChartType::Renko,
                Some(-30.0),
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
    fn updates_reach_bars_studies_and_revision() {
        enum Action {
            Update(Bar),
            SetData(Vec<Bar>),
        }
        use Action::{SetData, Update};
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
            chart.drawings_mut().add_synced_drawing(drawing);
            let revision = chart.state.revision();
            for action in actions {
                match action {
                    Update(bar) => chart.update(SeriesId::MAIN, bar),
                    SetData(bars) => chart.set_data(SeriesId::MAIN, bars),
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

    /// RSI, counting the calls that recompute it.
    struct Counted(RSI, Arc<AtomicUsize>, Duration);

    impl Indicator for Counted {
        fn name(&self) -> &str {
            self.0.name()
        }
        fn calculate(&mut self, data: &[Bar]) {
            if !data.is_empty() {
                self.1.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(self.2);
            }
            self.0.calculate(data);
        }
        fn update(&mut self, data: &[Bar], from: usize) {
            if !data.is_empty() {
                self.1.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(self.2);
            }
            self.0.update(data, from);
        }
        fn values(&self) -> &crate::studies::IndicatorValues {
            self.0.values()
        }
        fn colors(&self) -> Vec<egui::Color32> {
            self.0.colors()
        }
        fn set_colors(&mut self, colors: Vec<egui::Color32>) {
            self.0.set_colors(colors);
        }
        fn is_overlay(&self) -> bool {
            self.0.is_overlay()
        }
        fn set_visible(&mut self, visible: bool) {
            self.0.set_visible(visible);
        }
        fn clone_box(&self) -> Box<dyn Indicator> {
            Box::new(Counted(self.0.clone(), Arc::clone(&self.1), self.2))
        }
    }

    /// A replaced series gets one full recompute per frame, even while every wait is still
    /// running; waiting studies have no stale values and request another frame until the last
    /// study is current.
    #[test]
    fn full_recomputes_run_one_per_frame() {
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, i as f64)).collect(),
        ));
        let calls: Vec<_> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
        for counter in &calls {
            chart.add_indicator(Box::new(Counted(
                RSI::new(14),
                counter.clone(),
                Duration::ZERO,
            )));
            counter.store(0, Ordering::Relaxed);
        }
        // The second round replaces the bars while the waits of the first are still running.
        for (round, close) in [(0, 200.0), (1, 300.0)] {
            if round == 1 {
                // Model unequal measured costs without depending on the runner clock.
                chart.next_refresh.copy_from_slice(&[3.0, 2.0, 1.0]);
            }
            for counter in &calls {
                counter.store(0, Ordering::Relaxed);
            }
            chart.set_data(
                SeriesId::MAIN,
                (0..100)
                    .map(|i| bar(i * 60, close + (i * 7 % 13) as f64))
                    .collect(),
            );
            for k in 1..=3 {
                let now = if round == 0 { k as f64 } else { 3.0 } / 60.0;
                let wait = chart.refresh_studies(now);
                let counts: Vec<_> = calls
                    .iter()
                    .map(|counter| counter.load(Ordering::Relaxed))
                    .collect();
                assert_eq!(counts.iter().sum::<usize>(), k, "round {round}, call {k}");
                for (j, (_, study)) in chart.indicators().enumerate() {
                    assert!(counts[j] <= 1, "round {round}, call {k}, study {j}");
                    assert_eq!(
                        study.values().is_empty(),
                        counts[j] == 0,
                        "round {round}, call {k}, study {j}"
                    );
                }
                assert_eq!(wait, if k < 3 { Some(0.0) } else { None });
            }
        }
        let mut fresh = RSI::new(14);
        fresh.calculate(chart.bars(SeriesId::MAIN));
        for (_, study) in chart.indicators() {
            assert_eq!(
                format!("{:?}", study.values()),
                format!("{:?}", fresh.values())
            );
        }
    }

    /// The real cost advances the frame clock: re-arming from the frame start would make
    /// every frame due again when a recompute takes longer than the minimum wait.
    #[test]
    fn a_slow_full_recompute_does_not_take_every_frame() {
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, i as f64)).collect(),
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        chart.add_indicator(Box::new(Counted(
            RSI::new(14),
            calls.clone(),
            Duration::from_millis(300),
        )));
        calls.store(0, Ordering::Relaxed);
        let mut time = 0.0;
        for i in 0..10 {
            chart.update(SeriesId::MAIN, bar(99 * 60, 50.0 + i as f64));
            let started = Instant::now();
            chart.refresh_studies(time);
            time += 1.0 / 60.0 + started.elapsed().as_secs_f64();
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    /// Bars set every frame, as a replay does: each study is recomputed in turn every 0.25 s.
    /// Resetting the waits on `set_data` gave the first study every frame and the others none.
    #[test]
    fn set_data_every_frame_recomputes_each_study_in_turn() {
        let mut bars: Vec<Bar> = (0..100).map(|i| bar(i * 60, i as f64)).collect();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        let calls: Vec<_> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
        for counter in &calls {
            chart.add_indicator(Box::new(Counted(
                RSI::new(14),
                counter.clone(),
                Duration::ZERO,
            )));
            counter.store(0, Ordering::Relaxed);
        }
        for k in 0..120 {
            bars.push(bar((100 + k) * 60, 50.0 + k as f64));
            chart.set_data(SeriesId::MAIN, bars.clone());
            chart.refresh_studies(k as f64 / 60.0);
        }
        // Two seconds of frames at 60 Hz, a wait of 0.25 s: eight recomputes each.
        let calls: Vec<_> = calls.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        assert_eq!(calls, [8, 8, 8]);
    }

    /// Guards the cost and the freshness of studies while bars stream: a study that does not
    /// update incrementally is refreshed at most four times a second and once more after the
    /// stream stops, an incremental one on every frame, and both end equal to a fresh
    /// calculation. A refresh on every frame, a pending change lost after the last update, or
    /// the wait applied to an incremental study fails it.
    #[test]
    fn slow_studies_refresh_four_times_a_second_and_end_current() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, i as f64)).collect(),
        ));
        let sma = chart.add_indicator(Box::new(SMA::new(5)));
        let calls = Arc::new(AtomicUsize::new(0));
        let rsi = chart.add_indicator(Box::new(Counted(
            RSI::new(14),
            Arc::clone(&calls),
            Duration::ZERO,
        )));
        let values = |chart: &Chart, id| {
            let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
            format!("{:?}", study.values())
        };
        let fresh = |chart: &Chart, mut study: Box<dyn Indicator>| {
            study.calculate(chart.bars(SeriesId::MAIN));
            format!("{:?}", study.values())
        };
        frame(&ctx, &mut chart, 0.0, Vec::new());
        calls.store(0, Ordering::Relaxed);
        let mut time = 0.0;
        for i in 0..60 {
            time += 1.0 / 60.0;
            // Alternately a new close of the last bar and a new bar.
            chart.update(
                SeriesId::MAIN,
                bar((100 + i / 2) * 60, 50.0 + (i * 7 % 13) as f64),
            );
            frame(&ctx, &mut chart, time, Vec::new());
            assert_eq!(
                values(&chart, sma),
                fresh(&chart, Box::new(SMA::new(5))),
                "SMA at {i}"
            );
        }
        assert!(
            calls.load(Ordering::Relaxed) <= 4,
            "{calls:?} refreshes while streaming"
        );
        let streamed = calls.load(Ordering::Relaxed);
        let stop = time;
        while time < stop + 0.25 {
            time += 1.0 / 60.0;
            frame(&ctx, &mut chart, time, Vec::new());
        }
        assert_eq!(
            calls.load(Ordering::Relaxed),
            streamed + 1,
            "one refresh after the stream"
        );
        assert_eq!(
            values(&chart, rsi),
            fresh(&chart, Box::new(RSI::new(14))),
            "RSI"
        );
    }

    /// Merges retain anchor times, interpolating gaps and extrapolating whitespace from the
    /// nearest pair. The 360 s future anchor becomes 7 + (360 - 240) / 30 = index 11.
    #[test]
    fn inserted_bars_preserve_drawing_times_including_future_anchors() {
        let start: Vec<Bar> = (0..5).map(|i| bar(i * 60, (i + 1) as f64)).collect();
        let live = |seconds| (bar(seconds, 1.0), Origin::Live);
        #[rustfmt::skip]
        let cases = [
            ("inserts at two positions", vec![live(90), live(100), live(210)],
                vec![-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 6.0], vec![-1.0, 0.0, 1.0, 4.0, 5.0, 7.0, 11.0]),
            ("append", vec![live(300)], vec![3.0, 6.0], vec![3.0, 6.0]),
            ("history in front", vec![(bar(-60, 1.0), Origin::History)], vec![-1.0, 0.0], vec![0.0, 1.0]),
        ];
        for (name, incoming, points, moved) in cases {
            let mut chart = Chart::new(BarData::from_bars(start.clone()));
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = points.iter().map(|&at| ChartPoint::new(at, 1.0)).collect();
            chart.drawings_mut().add_synced_drawing(drawing);
            chart.apply(incoming);
            let points = &chart.drawings().drawings[0].chart_points;
            let after: Vec<f32> = points.iter().map(|point| point.bar_idx).collect();
            assert_eq!(after, moved, "{name}");
        }
    }
}
