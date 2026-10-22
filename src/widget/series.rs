//! The chart's bars: history and live updates, merged by one set of rules.

use super::{Chart, SeriesId};
use crate::data::FeedState;
use crate::data::merge::{Merged, Origin, merge, valid};
use crate::model::{Bar, BarClock, BarData};
use std::collections::BTreeSet;

impl Chart {
    /// Replaces the bars of `series`, for history or a replay window. Invalid bars (a non-finite
    /// value, a high or low on the wrong side of the open and close, negative volume) are dropped
    /// and counted in [`feed_state`](Self::feed_state), the rest sorted by time, and of several
    /// bars with one open time the last is kept. Near the live edge the view follows a new last
    /// bar, as with [`update`](Self::update).
    ///
    /// Only [`SeriesId::MAIN`] holds bars; a call with another id is ignored with a warning.
    pub fn set_data(&mut self, series: SeriesId, bars: Vec<Bar>) {
        if !holds_bars(series, "set_data") {
            return;
        }
        let (bars, rejected) = clean(bars);
        self.feed.rejected_bars += rejected;
        self.from_ticks.clear();
        let edge = self.edge();
        self.state.set_data(BarData::from_bars(bars));
        self.follow_live_edge(edge);
    }

    /// Merges `bar` into the bars of `series` as [`Sink::push_bar`](crate::data::Sink::push_bar)
    /// does, matched by exact open time: a bar with the open time of a stored bar replaces it, a
    /// newer one is appended and an older one is inserted in order. An invalid bar is dropped and
    /// counted in [`feed_state`](Self::feed_state). Near the live edge the view follows a new last
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

    /// What the chart knows about the feed of `series`, such as the bars dropped as invalid.
    /// `Some` for [`SeriesId::MAIN`].
    pub fn feed_state(&self, series: SeriesId) -> Option<&FeedState> {
        (series == SeriesId::MAIN).then_some(&self.feed)
    }

    /// Merges `incoming` into the main bars by the rules of each bar's origin and counts the
    /// invalid ones in the feed state.
    pub(crate) fn apply(&mut self, incoming: Vec<(Bar, Origin)>) -> Merged {
        let edge = self.edge();
        let (from_ticks, clock) = (&mut self.from_ticks, BarClock::exact());
        let mut merged = Merged::default();
        self.state.edit_bars(|bars| {
            merged = merge(bars, from_ticks, &clock, incoming);
            merged.first_changed
        });
        self.feed.rejected_bars += merged.rejected;
        self.follow_live_edge(edge);
        merged
    }

    fn edge(&self) -> Edge {
        let bars = &self.state.data().bars;
        Edge {
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

/// The view before the bars change, for [`Chart::follow_live_edge`].
struct Edge {
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

#[cfg(test)]
mod tests {
    use super::super::test_frames::frame;
    use super::*;
    use chrono::{DateTime, TimeDelta, Utc};

    /// A flat bar at `close`, stamped `seconds` after 10:00.
    fn bar(seconds: i64, close: f64) -> Bar {
        let t =
            "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::seconds(seconds);
        Bar::new(t, close, close, close, close, 1.0)
    }

    /// Guards the wiring of every bar update: the merged bars, the revision that span caches are
    /// keyed on and the count of invalid bars must describe the same series. A revision bumped on
    /// every update rebuilds the caches each tick; one kept on an insert or a removal leaves them
    /// stale; out-of-order bars must not be dropped, nor invalid bars drawn.
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
        #[rustfmt::skip]
        let cases = [
            ("last-bar update", Update(bar(240, 6.0)),
                series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 6.0)]), false, 0),
            ("append", Update(bar(300, 6.0)),
                series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 5.0), (300, 6.0)]), false, 0),
            ("correction", Update(bar(120, 9.0)),
                series(&[(0, 1.0), (60, 2.0), (120, 9.0), (180, 4.0), (240, 5.0)]), true, 0),
            ("out of order", Update(bar(90, 7.0)),
                series(&[(0, 1.0), (60, 2.0), (90, 7.0), (120, 3.0), (180, 4.0), (240, 5.0)]), true, 0),
            ("invalid", Update(invalid),
                start.clone(), false, 1),
            ("replace", SetData(series(&[(120, 3.0), (0, 1.0), (240, 5.0), (60, 2.0), (180, 4.0), (120, 8.0)])),
                series(&[(0, 1.0), (60, 2.0), (120, 8.0), (180, 4.0), (240, 5.0)]), true, 0),
            ("replay step", SetData([start.clone(), vec![bar(300, 6.0)]].concat()),
                series(&[(0, 1.0), (60, 2.0), (120, 3.0), (180, 4.0), (240, 5.0), (300, 6.0)]), false, 0),
            ("truncate", SetData(start[..4].to_vec()),
                start[..4].to_vec(), true, 0),
        ];
        let ctx = egui::Context::default();
        for (name, action, bars, revised, rejected) in cases {
            let mut chart = Chart::new(BarData::from_bars(start.clone()));
            let revision = chart.state.revision();
            match action {
                Update(bar) => chart.update(SeriesId::MAIN, bar),
                SetData(bars) => chart.set_data(SeriesId::MAIN, bars),
            }
            frame(&ctx, &mut chart, 0.0, Vec::new());
            assert_eq!(chart.bars(SeriesId::MAIN), bars, "{name}");
            assert_eq!(
                chart.state.revision() != revision,
                revised,
                "{name}: revision"
            );
            let feed = chart.feed_state(SeriesId::MAIN).unwrap();
            assert_eq!(feed.rejected_bars, rejected, "{name}: rejected");
        }
    }
}
