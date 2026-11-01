//! Retained history and a shared, exclusive replay cutoff.

use super::price_series::PriceSeries;
use super::{Chart, ChartLayout, SeriesId};
use crate::data::ReplayDriver;
use crate::data::merge::{Origin, fold, merge};
use crate::model::{Bar, BarClock, Timeframe};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub(super) struct ReplayWindow {
    pub(super) time: DateTime<Utc>,
    bars: HashMap<SeriesId, Vec<Bar>>,
    source: Option<(Vec<Bar>, BarClock)>,
    /// The pause last set with `set_feed_paused` before the replay, which its exit sets again.
    feed_paused: bool,
    pub quote: Option<crate::model::Level1Quote>,
    pub session_quote: Option<crate::model::quote::SessionQuote>,
}

pub(super) struct MarketReplay {
    driver: Arc<ReplayDriver>,
    feed: Option<super::feed::FeedLink>,
    window: ReplayWindow,
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
        self.market_replay
            .as_ref()
            .map(|replay| {
                replay
                    .driver
                    .time()
                    .checked_add_signed(chrono::TimeDelta::nanoseconds(1))
                    .unwrap_or(chrono::DateTime::<Utc>::MAX_UTC)
            })
            .or_else(|| self.replay.as_ref().map(|replay| replay.time))
    }

    /// The shared event driver attached with [`Self::set_market_replay`], if any.
    pub fn replay_driver(&self) -> Option<&Arc<ReplayDriver>> {
        self.market_replay.as_ref().map(|replay| &replay.driver)
    }

    /// Replaces the live feed with a shared recording-backed driver, or restores it with `None`.
    /// Retains the live bars, quotes, pause state and feed configuration for exit. Recorded trades
    /// build partial bars through the normal feed path, alongside quotes, orders and depth.
    /// Attach the same driver to every participating chart, then advance it once per frame before
    /// drawing. Its time is inclusive; [`Self::replay_time`] reports the exclusive cutoff one
    /// nanosecond later. An interval change resubscribes at the same shared position.
    pub fn set_market_replay(&mut self, driver: Option<Arc<ReplayDriver>>) {
        if let Some(replay) = self.market_replay.take() {
            self.feed_link = replay.feed;
            self.replay = Some(replay.window);
            self.set_replay_time(None);
        }
        let Some(driver) = driver else { return };
        let history = self
            .feed_link
            .as_ref()
            .and_then(|link| link.depth_history.as_ref())
            .map(|history| {
                crate::data::DepthHistory::new(history.config().clone())
                    .expect("existing depth history configuration")
            });
        let view = self.order_book_view();
        let heatmap_config = self.liquidity_heatmap_config();
        self.set_replay_time(Some(driver.time()));
        let window = self.replay.take().expect("replay window");
        let feed = self.feed_link.take();
        self.attach_feed(driver.clone(), 1);
        self.set_liquidity_heatmap_config(heatmap_config);
        if let (Some(live), Some(replay)) = (&feed, &mut self.feed_link) {
            replay.heatmap = live.heatmap.as_ref().map(|_| Default::default());
        }
        self.set_volume_bubbles(
            feed.as_ref()
                .and_then(|link| link.bubbles.as_ref())
                .map(|bubbles| bubbles.config.clone()),
        );
        self.set_depth_history(history);
        self.set_order_book_view(view);
        self.replace_main_data(Vec::new());
        self.market_replay = Some(MarketReplay {
            driver,
            feed,
            window,
        });
    }

    /// Enters or seeks replay, retaining the full history inside the chart. Only bars completed
    /// by `time` reach rendering, studies, auto-scaling and readouts, including the histories
    /// that studies request for other symbols and intervals. A bar's close is its
    /// `BarClock::next_open`; without a timeframe, timestamps are observation instants and bars
    /// strictly before `time` are available. Supply finer bars with `set_replay_source` to build
    /// the unfinished chart bar. No future OHLC is inferred from a completed coarse bar.
    ///
    /// The main feed is suspended and current quotes and book views are hidden. Use
    /// [`Self::set_market_replay`] to replay recorded trades, quotes and depth together.
    /// `set_data` and `update` retain
    /// host corrections behind the window. `None` exits, restores all retained bars and quotes,
    /// and restores the feed's previous pause state. Hosts own source data and session storage.
    /// With a market driver attached this dispatches a seek; call outside egui context locks,
    /// as with [`ReplayDriver::seek`].
    pub fn set_replay_time(&mut self, time: Option<DateTime<Utc>>) {
        if let Some(replay) = &self.market_replay {
            if let Some(time) = time {
                let inclusive = time
                    .checked_sub_signed(chrono::TimeDelta::nanoseconds(1))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                if replay.driver.time() != inclusive {
                    replay.driver.seek(inclusive);
                }
            } else {
                self.set_market_replay(None);
            }
            return;
        }
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
                self.replay_order_flow(replay.time);
            }
            return;
        };
        let previous = self.replay.as_ref().map(|replay| replay.time);
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
            let feed_paused = self.feed_link.as_ref().is_some_and(|link| link.host_paused);
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
        self.replay_order_flow(previous.map_or(time, |previous| previous.min(time)));
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
    /// Recorded trades replay one at a time as bars of one trade each under a clock of
    /// `Timeframe::Ticks` one, as [`data::aggregate_trades`](crate::data::aggregate_trades) builds
    /// them. A trade is an instant: it falls in the chart bar that holds it and is complete once
    /// the cutoff passes it, so a `ReplayController` over the same bars advances the unfinished bar
    /// trade by trade. `aggregate_trades` stamps trades that share a timestamp one nanosecond
    /// apart, keeping each a step of its own; bars at one time would replace each other. Bars of
    /// more than one trade are refused, since each is stamped at its first trade.
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
        if rejected != 0 || fold_source(&bars, &clock, &self.bar_clock()).is_none() {
            return false;
        }
        if let Some(replay) = &mut self.replay {
            replay.source = (!bars.is_empty()).then_some((bars, clock));
        }
        self.apply_replay_window();
        true
    }

    /// Applies the replay cutoff, or its end, to the order-flow inputs of the studies, which the
    /// paused feed no longer updates, and recalculates those studies from the bar containing
    /// `since`, the earlier of the old and the new cutoff.
    fn replay_order_flow(&mut self, since: DateTime<Utc>) {
        let time = self.replay_time();
        let link = self.feed_link.as_ref();
        let clock = link.map_or_else(|| self.bar_clock(), |link| link.clock.clone());
        let trades = link.map(|link| &link.trades);
        let open = clock.bar_open(since);
        let from = (self.state.data().bars).partition_point(|bar| clock.bar_open(bar.time) < open);
        for (at, study) in self.studies.indicators().iter().enumerate() {
            let inputs = study.order_flow_inputs();
            if inputs.is_empty() {
                continue;
            }
            for input in inputs {
                input.set_replay(&clock, time, trades);
            }
            self.pending_from[at] = Some(self.pending_from[at].map_or(from, |old| old.min(from)));
        }
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
                .map_or(0, |bars| super::series::evicted_bars(bars.len(), limit));
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
                && let Some(partial) = fold_source(
                    completed(source, source_clock, replay.time),
                    source_clock,
                    &clock,
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

    /// Gives `series`, which has left the chart, its retained bars. The window only holds series
    /// in the chart, so the history leaves with the series and is dropped with it.
    pub(super) fn replay_release(&mut self, series: &mut PriceSeries) {
        if let Some(bars) = self
            .replay
            .as_mut()
            .and_then(|replay| replay.bars.remove(&series.id))
        {
            series.bars = bars;
        }
    }

    /// Retains the bars of `series` as it enters the chart, leaving it those the cutoff completed.
    pub(super) fn replay_admit(&mut self, series: &mut PriceSeries) {
        let clock = self
            .timeframe
            .map_or_else(BarClock::exact, |_| self.bar_clock());
        if let Some(replay) = &mut self.replay {
            let bars = std::mem::take(&mut series.bars);
            series.bars = completed(&bars, &clock, replay.time).to_vec();
            replay.bars.insert(series.id, bars);
        }
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

/// The chart bars that `bars` of the `source` clock build under `chart`, or `None` when they do
/// not nest in them. A bar of one trade is an instant and falls in the chart bar that holds it;
/// other sources go through `data::resample`, which refuses the other trade-based intervals,
/// whose bars are stamped at their first trade. Replay folds the completed trades on every step,
/// so they are folded per chart bar, bisecting for its last trade, instead of a clock lookup per
/// trade: a session clock's lookup is not cheap. `bar_open` is ordered, as `completed` assumes.
fn fold_source(bars: &[Bar], source: &BarClock, chart: &BarClock) -> Option<Vec<Bar>> {
    let instants = matches!(source.timeframe(), Some(Timeframe::Ticks(count)) if count.get() == 1)
        && chart
            .timeframe()
            .is_some_and(|timeframe| !timeframe.is_trade_based());
    if !instants {
        return crate::data::resample(bars, source.clone(), chart.clone());
    }
    let mut folded = Vec::new();
    let mut rest = bars;
    while let Some(first) = rest.first() {
        let slot = chart.bar_open(first.time);
        // The first trade is in its own slot, so every pass takes at least one.
        let end = rest
            .partition_point(|bar| chart.bar_open(bar.time) <= slot)
            .max(1);
        // As in `resample`: a source bar's colours and payload are not the chart bar's.
        let mut bar = Bar {
            time: slot,
            colors: None,
            payload: None,
            ..first.clone()
        };
        for trade in &rest[1..end] {
            bar = fold(&bar, trade)?;
        }
        folded.push(bar);
        rest = &rest[end..];
    }
    // shortcut: every step refolds all trades before the cutoff; keep the folded complete chart
    // bars if long trade sources make playback slow.
    Some(folded)
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

    /// An added series brought back by undo shows what it would have shown had it never left:
    /// the bars completed at the cutoff during replay, and all of them after the exit. A cutoff
    /// at minute 2 completes the bars of minutes 0 and 1.
    #[test]
    fn undoing_a_series_removal_across_replay_keeps_its_history_and_the_cutoff() {
        let minutes: Vec<_> = (0..5).map(|i| bar(i, [10.0; 5])).collect();
        for removed_in_replay in [true, false] {
            let mut chart = Chart::new(BarData::from_bars(minutes.clone()));
            chart.set_timeframe(Timeframe::Min1);
            let id = chart.add_series(Default::default());
            chart.set_data(id, minutes.clone());
            if removed_in_replay {
                chart.set_replay_time(Some(time(2)));
            }
            assert!(chart.user_action(|chart| chart.remove_series(id)));
            if removed_in_replay {
                chart.set_replay_time(None);
                assert!(chart.undo());
            } else {
                chart.set_replay_time(Some(time(2)));
                assert!(chart.undo());
                assert_eq!(chart.bars(id), &minutes[..2], "undone during replay");
                chart.set_replay_time(None);
            }
            assert_eq!(
                chart.bars(id),
                minutes,
                "removed_in_replay={removed_in_replay}"
            );
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

    /// Bar replay over recorded trades: one-trade bars build the unfinished chart bar from the
    /// trades before each cursor position, two trades of one timestamp are two steps, a trade in
    /// the next minute starts that bar, and seeking back shows the earlier bar again. The
    /// chart's own minute 1, with its high of 14, never shows. Bars of two trades are refused: the
    /// second opens at 00:01:00.5 and holds the trade of 00:02:00.25.
    #[cfg(feature = "ui")]
    #[test]
    fn replay_advances_trade_by_trade() {
        use crate::model::Tick;
        use crate::ui::replay::{ReplayAction, ReplayController};
        use chrono::TimeDelta;

        let minute_0 = bar(0, [10.0, 12.0, 9.0, 11.0, 5.0]);
        let mut chart = Chart::new(BarData::from_bars(vec![
            minute_0.clone(),
            bar(1, [11.0, 14.0, 10.0, 13.0, 6.0]),
        ]));
        chart.set_timeframe(Timeframe::Min1);
        let at = |ms| time(1) + TimeDelta::milliseconds(ms);
        let trades = [
            Tick::new(at(0), 11.0, 1.0, None),
            Tick::new(at(0), 13.0, 2.0, None),
            Tick::new(at(500), 10.0, 1.0, None),
            Tick::new(at(60_250), 12.0, 3.0, None),
        ];
        let one_trade = BarClock::new("1t".parse().unwrap());
        let source = crate::data::aggregate_trades(&trades, one_trade.clone()).unwrap();
        let mut cursor = ReplayController::new(source.clone(), Vec::new());
        chart.set_replay_time(cursor.cutoff(&one_trade));
        assert!(chart.set_replay_source(source, one_trade.clone()));
        let steps = [
            // Cutoff 00:01:00.000000001: the first trade.
            (
                ReplayAction::None,
                vec![bar(1, [11.0, 11.0, 11.0, 11.0, 1.0])],
            ),
            // The second trade of 00:01:00, stamped one nanosecond later.
            (
                ReplayAction::StepForward,
                vec![bar(1, [11.0, 13.0, 11.0, 13.0, 3.0])],
            ),
            (
                ReplayAction::StepForward,
                vec![bar(1, [11.0, 13.0, 10.0, 10.0, 4.0])],
            ),
            // Minute 1 is complete and built from its trades; minute 2 holds one.
            (
                ReplayAction::StepForward,
                vec![
                    bar(1, [11.0, 13.0, 10.0, 10.0, 4.0]),
                    bar(2, [12.0, 12.0, 12.0, 12.0, 3.0]),
                ],
            ),
            (
                ReplayAction::JumpToBar(1),
                vec![bar(1, [11.0, 13.0, 11.0, 13.0, 3.0])],
            ),
        ];
        for (action, unfinished) in steps {
            cursor.handle_action(action.clone());
            chart.set_replay_time(cursor.cutoff(&one_trade));
            let mut expected = vec![minute_0.clone()];
            expected.extend(unfinished);
            assert_eq!(chart.bars(SeriesId::MAIN), expected, "{action:?}");
        }
        let shown = chart.bars(SeriesId::MAIN).to_vec();
        let two_trades = BarClock::new("2t".parse().unwrap());
        let pairs = crate::data::aggregate_trades(&trades, two_trades.clone()).unwrap();
        assert!(!chart.set_replay_source(pairs, two_trades));
        assert_eq!(chart.bars(SeriesId::MAIN), shown);
    }

    #[test]
    fn market_replay_rebuilds_chart_state_and_restores_the_live_feed() {
        use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
        use crate::model::{Instrument, Tick};
        let mut events: Vec<_> = [(0, 10.0), (1, 12.0), (2, 99.0)]
            .into_iter()
            .enumerate()
            .map(|(sequence, (minute, price))| {
                ReplayEvent::new(
                    "A",
                    time(minute),
                    sequence as u64,
                    MarketEvent::Trade(Tick::new(time(minute), price, 1.0, None)),
                )
            })
            .collect();
        events.push(ReplayEvent::new(
            "A",
            time(0),
            99,
            MarketEvent::DepthSnapshot {
                sequence: 0,
                book: crate::model::OrderBook {
                    ts: time(0),
                    symbol: "A".into(),
                    bids: vec![crate::model::OrderBookLevel {
                        price: 9.0,
                        size: 5.0,
                        order_cnt: None,
                    }],
                    asks: Vec::new(),
                },
            },
        ));
        let load = || {
            Arc::new(
                ReplayDriver::new(
                    events.clone(),
                    vec![Instrument::new("A").with_tick_size(1.0)],
                )
                .unwrap(),
            )
        };
        let live = load();
        live.seek(time(2));
        let mut chart = super::super::ChartBuilder::new()
            .with_feed(live)
            .with_symbol("A")
            .with_timeframe(Timeframe::Min1)
            .build();
        let ctx = egui::Context::default();
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        assert!(chart.order_book(SeriesId::MAIN).is_some());
        chart.set_replay_time(Some(time(1)));
        assert!(
            chart.order_book(SeriesId::MAIN).is_none(),
            "bar-only replay cannot expose a live book"
        );
        chart.set_replay_time(None);
        assert!(chart.order_book(SeriesId::MAIN).is_some());
        chart.set_volume_bubbles(Some(crate::chart::overlays::VolumeBubblesConfig {
            min_size_filter: Some(2.0),
            ..Default::default()
        }));
        let heatmap_config = crate::chart::overlays::LiquidityHeatmapConfig {
            brightness: 0.5,
            lower_cutoff: 2.0,
            full_intensity_size: Some(10.0),
        };
        chart.set_liquidity_heatmap_config(heatmap_config);
        chart.set_liquidity_heatmap(true);
        chart.set_chart_type(crate::model::ChartType::VolumeFootprint);
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        let original = chart.bars(SeriesId::MAIN).to_vec();
        assert_eq!(original.len(), 3);
        let replay = load();
        replay.set_speed(0.0).unwrap();
        replay.seek(time(0));
        chart.set_market_replay(Some(replay.clone()));
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        assert_eq!(chart.bars(SeriesId::MAIN), &original[..1]);
        assert_eq!(chart.liquidity_heatmap_config(), heatmap_config);
        assert_eq!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .bubbles
                .as_ref()
                .map(|bubbles| bubbles.config.min_size_filter),
            Some(Some(2.0)),
            "market replay keeps the configured trade bubbles"
        );
        assert_eq!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .unwrap()
                .bars()
                .len(),
            1,
            "a seek cannot retain future footprint rows"
        );
        replay.seek(time(1));
        chart.pump(&ctx);
        assert_eq!(chart.bars(SeriesId::MAIN), &original[..2]);
        chart.set_timeframe(Timeframe::Min5);
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        assert_eq!(
            chart.bars(SeriesId::MAIN),
            [bar(0, [10.0, 12.0, 10.0, 12.0, 2.0])]
        );
        chart.set_timeframe(Timeframe::Min1);
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        chart.set_market_replay(None);
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        assert_eq!(chart.bars(SeriesId::MAIN), original);
        assert!(chart.replay_time().is_none());
        assert!(chart.replay_driver().is_none());
        assert_eq!(chart.liquidity_heatmap_config(), heatmap_config);

        // A seek must also rebuild the coarser history that ordinary frames accumulated.
        let books = (0..8)
            .map(|minute| {
                ReplayEvent::new(
                    "A",
                    time(minute),
                    minute as u64,
                    MarketEvent::DepthSnapshot {
                        sequence: minute as u64,
                        book: crate::model::OrderBook {
                            ts: time(minute),
                            symbol: "A".into(),
                            bids: vec![crate::model::OrderBookLevel {
                                price: 99.0,
                                size: (minute + 1) as f64,
                                order_cnt: None,
                            }],
                            asks: Vec::new(),
                        },
                    },
                )
            })
            .collect();
        let driver = Arc::new(
            ReplayDriver::new(books, vec![Instrument::new("A").with_tick_size(1.0)]).unwrap(),
        );
        let mut chart = super::super::ChartBuilder::new()
            .with_feed(driver.clone())
            .with_symbol("A")
            .with_timeframe(Timeframe::Min1)
            .build();
        chart.set_depth_history(Some(
            crate::data::DepthHistory::new(crate::data::DepthHistoryConfig {
                columns: 2,
                cadence: chrono::TimeDelta::minutes(1),
                ..Default::default()
            })
            .unwrap(),
        ));
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        driver.advance(std::time::Duration::ZERO).unwrap();
        chart.pump(&ctx);
        for _ in 1..8 {
            driver.advance(std::time::Duration::from_secs(60)).unwrap();
            chart.pump(&ctx);
        }
        let expected: Vec<_> = chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .columns()
            .cloned()
            .collect();
        assert!(expected.len() > 2);
        driver.seek(time(0));
        chart.pump(&ctx);
        driver.seek(time(7));
        chart.pump(&ctx);
        assert_eq!(
            chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .columns()
                .cloned()
                .collect::<Vec<_>>(),
            expected
        );
        let config = chart
            .depth_history(SeriesId::MAIN)
            .unwrap()
            .config()
            .clone();
        chart.set_depth_history(Some(crate::data::DepthHistory::new(config).unwrap()));
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        assert_eq!(
            chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .columns()
                .cloned()
                .collect::<Vec<_>>(),
            expected[expected.len() - 2..],
            "enabling history at a paused cursor must load its latest recording page"
        );
    }
}
