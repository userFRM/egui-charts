//! The chart's bars, merged by one set of rules for history and live updates, and the studies
//! computed from them.

use super::{Chart, ChartElementId, SeriesId};
use crate::data::FeedState;
use crate::data::merge::{Merged, Origin, merge, valid};
use crate::model::{Bar, BarClock, BarData, Marker};
use crate::studies::Indicator;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;

impl Chart {
    /// Replaces the bars of `series`, for history or a replay window. Invalid bars (a non-finite
    /// value, a high or low on the wrong side of the open and close, negative volume) are dropped
    /// and counted in [`feed_state`](Self::feed_state), the rest sorted by time, and of several
    /// bars with one open time the last is kept. If the new bars still hold the former first bar,
    /// the drawings move with it, so older history can be loaded in front of the bars. The
    /// studies are updated from the first bar that changed on the next [`show`](Self::show), and
    /// near the live edge the view follows a new last bar, as with [`update`](Self::update).
    ///
    /// Only [`SeriesId::MAIN`] holds bars; a call with another id is ignored with a warning.
    pub fn set_data(&mut self, series: SeriesId, bars: Vec<Bar>) {
        if !holds_bars(series, "set_data") {
            return;
        }
        let (bars, rejected) = clean(bars);
        self.feed.rejected_bars += rejected;
        self.from_ticks.clear();
        let before = self.edge();
        let prepended = before
            .first
            .filter(|first| bars.binary_search_by_key(first, |bar| bar.time).is_ok())
            .map_or(0, |first| bars.partition_point(|bar| bar.time < first));
        let first_changed = self.state.set_data(BarData::from_bars(bars));
        self.drawings
            .shift_bar_indices(f32::NEG_INFINITY, prepended as f32);
        self.bars_changed(before, first_changed);
    }

    /// Merges `bar` into the bars of `series` as [`Sink::push_bar`](crate::data::Sink::push_bar)
    /// does, matched by exact open time: a bar with the open time of a stored bar replaces it, a
    /// newer one is appended and an older one is inserted in order. An invalid bar is dropped and
    /// counted in [`feed_state`](Self::feed_state). Drawings move with their bars, studies follow
    /// them on the next [`show`](Self::show), and near the live edge the view follows a new last
    /// bar.
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

    /// What the chart knows about the feed of `series`, such as the bars dropped as invalid.
    /// `Some` for [`SeriesId::MAIN`].
    pub fn feed_state(&self, series: SeriesId) -> Option<&FeedState> {
        (series == SeriesId::MAIN).then_some(&self.feed)
    }

    /// Merges `incoming` into the main bars by the rules of each bar's origin and counts the
    /// invalid ones in the feed state.
    pub(crate) fn apply(&mut self, incoming: Vec<(Bar, Origin)>) -> Merged {
        let before = self.edge();
        let (from_ticks, clock) = (&mut self.from_ticks, BarClock::exact());
        let mut merged = Merged::default();
        self.state.edit_bars(|bars| {
            merged = merge(bars, from_ticks, &clock, incoming);
            merged.first_changed
        });
        self.feed.rejected_bars += merged.rejected;
        // Drawings hold bar indices: a bar inserted in front of a stored bar moves the points from
        // that bar on, and one in front of the first bar also the points left of it.
        for run in merged.inserted_before.chunk_by(|a, b| a == b).rev() {
            let from = if run[0] == 0 {
                f32::NEG_INFINITY
            } else {
                run[0] as f32
            };
            self.drawings.shift_bar_indices(from, run.len() as f32);
        }
        self.bars_changed(before, merged.first_changed);
        merged
    }

    /// Adds a study, computed at once from the bars, which [`show`](Self::show) draws: on the
    /// main pane if [`Indicator::is_overlay`], otherwise in a pane below it. Returns the study's
    /// id, which stays valid when other studies are removed.
    pub fn add_indicator(&mut self, mut indicator: Box<dyn Indicator>) -> SeriesId {
        indicator.calculate(&self.state.data().bars);
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.studies.add(indicator);
        self.study_ids.push(id);
        self.pending_from.push(None);
        id
    }

    /// Removes the study `id` and clears the selection if it is that study. Returns `false` for
    /// [`SeriesId::MAIN`] and any id that is not a study of the chart.
    pub fn remove_series(&mut self, id: SeriesId) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.studies.remove_indicator(at);
        self.study_ids.remove(at);
        self.pending_from.remove(at);
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
        self.studies.indicators_mut()[at].set_visible(visible);
        true
    }

    /// The chart's studies with their ids, in the order they were added. Their values reflect the
    /// bars as of the last [`show`](Self::show), or of the call to
    /// [`add_indicator`](Self::add_indicator).
    pub fn indicators(&self) -> impl Iterator<Item = (SeriesId, &dyn Indicator)> {
        let studies = self.studies.indicators().iter().map(|study| study.as_ref());
        self.study_ids.iter().copied().zip(studies)
    }

    /// The study `id`, to change its settings. Its values reflect the bars as of the last
    /// [`show`](Self::show); the next one recomputes it in full.
    pub fn indicator_mut(&mut self, id: SeriesId) -> Option<&mut (dyn Indicator + 'static)> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        self.pending_from[at] = Some(0);
        Some(self.studies.indicators_mut()[at].as_mut())
    }

    /// Updates every study with a pending change from the first bar it must be updated from.
    pub(super) fn refresh_studies(&mut self) {
        let bars = &self.state.data().bars;
        let studies = self.studies.indicators_mut().iter_mut();
        for (study, from) in studies.zip(&mut self.pending_from) {
            if let Some(from) = from.take() {
                study.update(bars, from);
            }
        }
    }

    /// Keeps studies and the view in step after the main bars changed. `first_changed` is the
    /// index of the first bar inserted, changed or removed. The studies are updated once on the
    /// next frame, for all changes since the previous one.
    fn bars_changed(&mut self, before: Edge, first_changed: Option<usize>) {
        if let Some(from) = first_changed {
            for pending in &mut self.pending_from {
                *pending = Some(pending.map_or(from, |pending| pending.min(from)));
            }
        }
        self.follow_live_edge(before);
    }

    fn edge(&self) -> Edge {
        let bars = &self.state.data().bars;
        Edge {
            first: bars.first().map(|bar| bar.time),
            // Within ~1.5 bars of the right edge. With fix_right_edge the right offset is clamped
            // to <= 0, so near means no farther than 1.5 bars to the left of the edge.
            near_live: self.state.time_scale().right_offset() >= -1.5,
            len: bars.len(),
            tail: bars.last().cloned(),
        }
    }

    /// Scrolls to the live edge when the view was near it and a bar was appended, or the last
    /// bar replaced while the options allow shifting on whitespace replacement.
    fn follow_live_edge(&mut self, before: Edge) {
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
        if options.shift_visible_range_on_new_bar
            && before.near_live
            && (appended || shift_for_whitespace)
        {
            self.state.time_scale_mut().scroll_to_realtime();
            // A pending start index would move the view away from the edge again.
            self.pending_start_idx = None;
        }
    }
}

/// The bars and the view before the bars change, for [`Chart::bars_changed`].
struct Edge {
    first: Option<DateTime<Utc>>,
    near_live: bool,
    len: usize,
    tail: Option<Bar>,
}

/// `bars` without the invalid ones, sorted by time, with the last of several bars of one open
/// time; and the number of invalid bars dropped.
pub(super) fn clean(bars: Vec<Bar>) -> (Vec<Bar>, u64) {
    // Bars already in order pass as they are, without the copy through the merge.
    if bars.windows(2).all(|pair| pair[0].time < pair[1].time) && bars.iter().all(valid) {
        return (bars, 0);
    }
    let mut clean = Vec::with_capacity(bars.len());
    let incoming = bars.into_iter().map(|bar| (bar, Origin::History)).collect();
    let merged = merge(
        &mut clean,
        &mut BTreeSet::new(),
        &BarClock::exact(),
        incoming,
    );
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
    use crate::studies::{IndicatorValue, SMA};
    use chrono::TimeDelta;

    /// A flat bar at `close`, stamped `seconds` after 10:00.
    fn bar(seconds: i64, close: f64) -> Bar {
        let t =
            "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::seconds(seconds);
        Bar::new(t, close, close, close, close, 1.0)
    }

    /// A chart built with a right offset of 20 bars keeps it while it follows the live edge
    /// through appended bars.
    #[test]
    fn appended_bars_keep_the_configured_right_offset() {
        let options = crate::config::ChartOptions {
            time_scale: crate::config::TimeScaleOptions {
                right_offset: 20.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, 100.0)).collect(),
        ))
        .with_chart_options(options);
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, Vec::new());
        for i in 100..103 {
            chart.update(SeriesId::MAIN, bar(i * 60, 101.0));
            assert_eq!(chart.state.time_scale().right_offset(), 20.0, "bar {i}");
            frame(&ctx, &mut chart, i as f64, Vec::new());
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

    /// Guards the drawing shift of a merge: a point moves by the number of bars inserted in front
    /// of its bar or at it, every point does for bars in front of the first bar, and none does
    /// for an appended bar. Points right of the last bar must not creep on each new bar.
    #[test]
    fn inserted_bars_move_the_drawings_after_them() {
        let start: Vec<Bar> = (0..5).map(|i| bar(i * 60, (i + 1) as f64)).collect();
        let live = |seconds| (bar(seconds, 1.0), Origin::Live);
        #[rustfmt::skip]
        let cases = [
            ("inserts at two positions", vec![live(90), live(100), live(210)],
                vec![-1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 6.0], vec![-1.0, 0.0, 1.0, 4.0, 5.0, 7.0, 9.0]),
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
