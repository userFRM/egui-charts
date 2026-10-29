//! Retained history and a shared, exclusive replay cutoff.

use super::{Chart, ChartLayout, SeriesId};
use crate::data::merge::{Origin, merge};
use crate::model::{Bar, BarClock};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};

pub(super) struct ReplayWindow {
    pub(super) time: DateTime<Utc>,
    bars: HashMap<SeriesId, Vec<Bar>>,
    source: Option<(Vec<Bar>, BarClock)>,
    feed_paused: bool,
    pub quote: Option<crate::model::Level1Quote>,
    pub session_quote: Option<crate::model::quote::SessionQuote>,
}

impl ReplayWindow {
    pub(super) fn remove_oldest_bars(
        &mut self,
        id: SeriesId,
        count: usize,
        clock: &BarClock,
    ) -> Vec<Bar> {
        let bars = self.bars.entry(id).or_default();
        let count = count.min(bars.len());
        if id == SeriesId::MAIN
            && count > 0
            && let Some((source, _)) = &mut self.source
        {
            let through = clock.bar_open(bars[count - 1].time);
            let end = source.partition_point(|bar| clock.bar_open(bar.time) <= through);
            source.drain(..end);
            super::series::shrink_bars(source);
        }
        let removed = bars.drain(..count).collect();
        super::series::shrink_bars(bars);
        removed
    }
}

impl Chart {
    /// The exclusive replay cutoff, or `None` when showing live data.
    pub fn replay_time(&self) -> Option<DateTime<Utc>> {
        self.replay.as_ref().map(|replay| replay.time)
    }

    /// Enters or seeks replay, retaining the full history inside the chart. Only bars completed
    /// by `time` reach rendering, studies, auto-scaling and readouts, including the histories
    /// that studies request for other symbols and intervals. A bar's close is its
    /// `BarClock::next_open`; without a timeframe, timestamps are observation instants and bars
    /// strictly before `time` are available. Supply finer bars with `set_replay_source` to build
    /// the unfinished chart bar. No future OHLC is inferred from a completed coarse bar.
    ///
    /// The main feed is suspended and current quotes are hidden. `set_data` and `update` retain
    /// host corrections behind the window. `None` exits, restores all retained bars and quotes,
    /// and restores the feed's previous pause state. Hosts own source data and session storage.
    pub fn set_replay_time(&mut self, time: Option<DateTime<Utc>>) {
        let Some(time) = time else {
            if let Some(replay) = self.replay.take() {
                for (id, bars) in replay.bars {
                    if id == SeriesId::MAIN {
                        self.replace_main_data(bars);
                    } else if let Some(item) = self.price_series.iter_mut().find(|s| s.id == id) {
                        item.bars = bars;
                        item.revision = crate::model::next_revision();
                    }
                }
                self.quote = replay.quote;
                self.session_quote = replay.session_quote;
                self.set_feed_paused(replay.feed_paused);
                self.invalidate_study_data();
            }
            return;
        };
        let rewind = self.replay.as_ref().is_none_or(|replay| time < replay.time);
        if let Some(replay) = &mut self.replay {
            if replay.time == time {
                return;
            }
            replay.time = time;
        } else {
            let mut bars: HashMap<_, _> = self
                .price_series
                .iter()
                .map(|s| (s.id, s.bars.clone()))
                .collect();
            bars.insert(SeriesId::MAIN, self.state.data().bars.clone());
            let feed_paused = self.feed_link.as_ref().is_some_and(|link| link.paused);
            self.replay = Some(ReplayWindow {
                time,
                bars,
                source: None,
                feed_paused,
                quote: self.quote.take(),
                session_quote: self.session_quote.take(),
            });
            self.set_feed_paused(true);
        }
        self.apply_replay_window();
        // Studies on other histories see the bars completed by the new cursor.
        self.invalidate_study_data();
        if rewind {
            // A cached value at the cursor may have used the candle's later high or close.
            for study in self.studies.indicators_mut() {
                study.update(&[], 0);
            }
            self.pending_from.fill(Some(0));
            for study in self.element_studies.studies.indicators_mut() {
                study.update(&[], 0);
            }
            self.element_studies.pending_from.fill(Some(0));
        }
    }

    /// Supplies authoritative finer history for the main series during replay. Its completed
    /// bars build the partial chart bar using first open, maximum high, minimum low, last close
    /// and summed volume. Earlier chart history stays available. The host must supply all finer
    /// bars in the covered chart slots; missing bars are treated as periods without trades.
    ///
    /// Returns `false` without changes outside replay, without a chart timeframe, for invalid
    /// bars, or when the source intervals do not nest in the chart interval. Input is cleaned
    /// and sorted as by `set_data`. An empty source clears it. Other bar series use completed
    /// chart bars. Change playback's cursor interval to the supplied source interval in the host.
    pub fn set_replay_source(&mut self, bars: Vec<Bar>, clock: BarClock) -> bool {
        if self.replay.is_none() || self.timeframe.is_none() {
            return false;
        }
        let (bars, rejected) = super::series::clean(bars, &clock);
        if rejected != 0 || crate::data::resample(&bars, clock.clone(), self.bar_clock()).is_none()
        {
            return false;
        }
        if let Some(replay) = &mut self.replay {
            replay.source = (!bars.is_empty()).then_some((bars, clock));
        }
        self.apply_replay_window();
        true
    }

    pub(super) fn apply_replay_window(&mut self) {
        let Some(mut replay) = self.replay.take() else {
            return;
        };
        let clock = self
            .timeframe
            .map_or_else(BarClock::exact, |_| self.bar_clock());
        for (&id, &limit) in &self.bar_limits {
            let count = replay
                .bars
                .get(&id)
                .map_or(0, |bars| bars.len().saturating_sub(limit.get()));
            if count > 0 {
                replay.remove_oldest_bars(id, count, &clock);
                if id == SeriesId::MAIN {
                    self.feed.history_complete = false;
                }
            }
        }
        if let Some(bars) = replay.bars.get(&SeriesId::MAIN) {
            let mut visible = completed(bars, &clock, replay.time).to_vec();
            if let Some((source, source_clock)) = &replay.source
                && let Some(first) = source.first()
                && let Some(last) = source.last()
                && let Some(partial) = crate::data::resample(
                    completed(source, source_clock, replay.time),
                    source_clock.clone(),
                    clock.clone(),
                )
            {
                let start = clock.bar_open(first.time);
                let end = clock.bar_open(last.time);
                visible.retain(|bar| {
                    let slot = clock.bar_open(bar.time);
                    slot < start || slot > end
                });
                merge(
                    &mut visible,
                    &mut BTreeMap::new(),
                    &clock,
                    partial.into_iter().map(|bar| (bar, Origin::Live)).collect(),
                );
            }
            self.replace_main_data(visible);
        }
        for item in &mut self.price_series {
            if let Some(bars) = replay.bars.get(&item.id) {
                item.bars = completed(bars, &clock, replay.time).to_vec();
                item.revision = crate::model::next_revision();
            }
        }
        self.replay = Some(replay);
    }

    pub(super) fn replay_set_data(&mut self, id: SeriesId, bars: Vec<Bar>) {
        let clock = self.clock();
        let Some(replay) = &mut self.replay else {
            return;
        };
        let (bars, rejected) = super::series::clean(bars, &clock);
        replay.bars.insert(id, bars);
        self.count_replay_rejections(id, rejected);
        self.apply_replay_window();
    }

    /// Merges `bar` into the retained bars of `id`; returns whether they changed.
    pub(super) fn replay_update(&mut self, id: SeriesId, bar: Bar) -> bool {
        let clock = self.clock();
        let Some(replay) = &mut self.replay else {
            return false;
        };
        let merged = merge(
            replay.bars.entry(id).or_default(),
            &mut BTreeMap::new(),
            &clock,
            vec![(bar, Origin::Live)],
        );
        self.count_replay_rejections(id, merged.rejected);
        self.apply_replay_window();
        merged.first_changed.is_some()
    }

    /// Removes the last `count` retained bars of `id` and returns them.
    pub(super) fn replay_pop(&mut self, id: SeriesId, count: usize) -> Vec<Bar> {
        let Some(replay) = &mut self.replay else {
            return Vec::new();
        };
        let bars = replay.bars.entry(id).or_default();
        let removed = bars.split_off(bars.len().saturating_sub(count));
        self.apply_replay_window();
        removed
    }

    fn count_replay_rejections(&mut self, id: SeriesId, rejected: u64) {
        if id == SeriesId::MAIN {
            self.feed.rejected_bars += rejected;
        } else if let Some(item) = self.price_series.iter_mut().find(|s| s.id == id) {
            item.feed.rejected_bars += rejected;
        }
    }
}

pub(super) fn completed<'a>(bars: &'a [Bar], clock: &BarClock, time: DateTime<Utc>) -> &'a [Bar] {
    let end = bars.partition_point(|bar| {
        clock
            .next_open(clock.bar_open(bar.time))
            .map_or(bar.time < time, |end| end <= time)
    });
    &bars[..end]
}

impl ChartLayout {
    /// Sets one replay cutoff on the source slot's synchronization group, including hidden
    /// slots. Other groups keep their own sessions. Call for every cursor move and with `None`
    /// to exit; each chart windows its own host-supplied history at its own interval.
    pub fn set_replay_time(
        &self,
        charts: &mut [&mut Chart],
        source: usize,
        time: Option<DateTime<Utc>>,
    ) {
        if source >= charts.len().min(16) {
            return;
        }
        for (index, chart) in charts.iter_mut().take(16).enumerate() {
            if self.sync_group(index) == self.sync_group(source) {
                chart.set_replay_time(time);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BarData, Timeframe};

    fn bar(minute: i64, values: [f64; 5]) -> Bar {
        let [open, high, low, close, volume] = values;
        Bar::new(time(minute), open, high, low, close, volume)
    }

    fn time(minute: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_704_067_200 + minute * 60, 0).unwrap()
    }

    /// Completion cutoffs, partial OHLCV, corrections, new/removed comparisons and group
    /// isolation all use the same retained history. The final minute's spike must stay hidden.
    #[test]
    fn replay_windows_every_series_and_restores_retained_history() {
        let minutes = vec![
            bar(0, [10.0, 12.0, 9.0, 11.0, 2.0]),
            bar(1, [11.0, 15.0, 10.0, 14.0, 3.0]),
            bar(2, [14.0, 16.0, 8.0, 9.0, 4.0]),
            bar(3, [9.0, 13.0, 7.0, 12.0, 5.0]),
            bar(4, [12.0, 100.0, 1.0, 90.0, 6.0]),
        ];
        let coarse = vec![bar(0, [10.0, 100.0, 1.0, 90.0, 20.0])];
        let mut a = Chart::new(BarData::from_bars(coarse.clone()));
        a.set_timeframe(Timeframe::Min5);
        let comparison = a.add_series(Default::default());
        a.set_data(comparison, coarse.clone());
        let mut b = Chart::new(BarData::from_bars(minutes.clone()));
        b.set_timeframe(Timeframe::Min1);
        let mut isolated = Chart::new(BarData::from_bars(minutes.clone()));
        isolated.set_timeframe(Timeframe::Min1);
        let mut layout = ChartLayout::default();
        layout.set_sync_group(2, Some("other".into()));
        layout.set_replay_time(&mut [&mut a, &mut b, &mut isolated], 0, Some(time(2)));
        assert!(
            a.bars(SeriesId::MAIN).is_empty(),
            "unfinished coarse bar must be hidden"
        );
        assert!(a.bars(comparison).is_empty());
        assert_eq!(b.bars(SeriesId::MAIN), &minutes[..2]);
        assert_eq!(isolated.bars(SeriesId::MAIN), minutes);
        assert!(a.set_replay_source(minutes.clone(), BarClock::new(Timeframe::Min1)));
        // First open 10; max(12,15)=15; min(9,10)=9; last close 14; volume 2+3=5.
        assert_eq!(
            a.bars(SeriesId::MAIN),
            [bar(0, [10.0, 15.0, 9.0, 14.0, 5.0])]
        );
        let prior = a.bars(SeriesId::MAIN).to_vec();
        assert!(!a.set_replay_source(minutes.clone(), BarClock::new(Timeframe::Min15)));
        assert_eq!(a.bars(SeriesId::MAIN), prior);
        let mut invalid = minutes.clone();
        invalid[0].volume = -1.0;
        assert!(!a.set_replay_source(invalid, BarClock::new(Timeframe::Min1)));
        for (cutoff, expected) in [
            (4, bar(0, [10.0, 16.0, 7.0, 12.0, 14.0])),
            (1, bar(0, [10.0, 12.0, 9.0, 11.0, 2.0])),
            (5, coarse[0].clone()),
        ] {
            a.set_replay_time(Some(time(cutoff)));
            assert_eq!(a.bars(SeriesId::MAIN), [expected]);
        }
        a.set_replay_time(Some(time(2)));
        let added = a.add_series(Default::default());
        a.set_data(added, coarse.clone());
        assert!(a.bars(added).is_empty());
        a.remove_series(comparison);
        let correction = bar(5, [90.0, 95.0, 80.0, 85.0, 7.0]);
        a.update(SeriesId::MAIN, correction.clone());
        a.update(added, correction.clone());
        assert_eq!(a.bars(SeriesId::MAIN), prior);
        a.set_replay_time(Some(time(10)));
        assert_eq!(
            a.bars(SeriesId::MAIN),
            [coarse[0].clone(), correction.clone()],
            "completed chart history beyond the finer source must remain available"
        );
        a.set_replay_time(None);
        assert_eq!(
            a.bars(SeriesId::MAIN),
            [coarse[0].clone(), correction.clone()]
        );
        assert_eq!(a.bars(added), [coarse[0].clone(), correction]);
        assert!(
            a.bars(comparison).is_empty(),
            "exit cannot resurrect a removed series"
        );
        b.set_timeframe(Timeframe::Min5);
        assert!(
            b.bars(SeriesId::MAIN).is_empty(),
            "a paused cutoff must follow interval changes"
        );
        b.set_timeframe(Timeframe::Min1);
        assert_eq!(b.bars(SeriesId::MAIN), &minutes[..2]);
        b.set_data(SeriesId::MAIN, minutes[..3].to_vec());
        assert_eq!(b.bars(SeriesId::MAIN), &minutes[..2]);
        b.set_replay_time(None);
        assert_eq!(b.bars(SeriesId::MAIN), &minutes[..3]);

        for limit in [false, true] {
            let mut chart = Chart::new(BarData::from_bars(a.bars(SeriesId::MAIN).to_vec()));
            chart.set_timeframe(Timeframe::Min5);
            chart.set_replay_time(Some(time(2)));
            assert!(chart.set_replay_source(minutes.clone(), BarClock::new(Timeframe::Min1)));
            assert_eq!(chart.bars(SeriesId::MAIN).len(), 1);
            if limit {
                chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(1));
            } else {
                assert_eq!(chart.remove_oldest_bars(SeriesId::MAIN, 1), coarse);
            }
            assert!(
                chart.bars(SeriesId::MAIN).is_empty(),
                "finer replay source must not rebuild evicted bars; limit={limit}"
            );
            chart.set_replay_time(Some(time(10)));
            assert_eq!(chart.bars(SeriesId::MAIN), &a.bars(SeriesId::MAIN)[1..]);
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn rewind_clears_future_study_values_before_rendering() {
        let mut chart = Chart::new(BarData::from_bars(
            (0..8).map(|i| bar(i, [10.0 + i as f64; 5])).collect(),
        ));
        chart.set_timeframe(Timeframe::Min1);
        chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
        chart.add_indicator(Box::new(crate::studies::EMA::new(2)));
        chart.set_replay_time(Some(time(4)));
        assert!(
            chart
                .indicators()
                .all(|(_, study)| study.values().is_empty())
        );
        chart.refresh_studies(1.0);
        chart.refresh_studies(2.0);
        assert_eq!(
            chart.indicators().next().unwrap().1.values().line(0)[3],
            12.5
        );
        chart.set_replay_time(Some(time(2)));
        assert!(
            chart
                .indicators()
                .all(|(_, study)| study.values().is_empty())
        );
        chart.refresh_studies(3.0);
        chart.refresh_studies(4.0);
        assert_eq!(
            chart.indicators().next().unwrap().1.values().line(0)[1],
            10.5
        );
    }
}
