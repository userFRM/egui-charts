//! Retained history and a shared, exclusive replay cutoff.

use super::events::same_bar;
use super::layout::MAX_CHARTS;
use super::series::PriceSeries;
use super::{Chart, ChartLayout, SeriesId};
use crate::data::ReplayDriver;
use crate::data::merge::{Origin, fold, merge};
use crate::model::{Bar, BarClock, Timeframe};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

pub(super) struct ReplayWindow {
    pub(super) time: DateTime<Utc>,
    pub(super) bars: HashMap<SeriesId, Vec<Bar>>,
    source: Option<(Vec<Bar>, BarClock)>,
    /// The pause last set with `set_feed_paused` before the replay, which its exit sets again.
    pub(super) feed_paused: bool,
    /// The live ATR and percentage sizes in Renko, Kagi, Range, Point and Figure order, with
    /// whether each waited for history, which the exit gives back. `None` for a fixed size, and
    /// for one armed by its parameters or by new main history during the session, which
    /// resolves from the restored history instead.
    pub(super) live_sizes: [Option<(f64, bool)>; 4],
    pub quote: Option<crate::model::Level1Quote>,
    pub session_quote: Option<crate::model::quote::SessionQuote>,
}

pub(super) struct MarketReplay {
    driver: Arc<ReplayDriver>,
    pub(super) feed: Option<super::feed::FeedLink>,
    pub(super) window: ReplayWindow,
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
            && let Some((source, source_clock)) = &mut self.source
        {
            let through = clock.bar_open(bars[count - 1].time);
            let end = match trading_dates(source, source_clock, clock) {
                // Source bars that join no chart bar before the first date kept leave too.
                Some(runs) => (runs.iter().find(|(slot, _)| *slot > through))
                    .map_or(source.len(), |(_, run)| run.start),
                None => source.partition_point(|bar| clock.bar_open(bar.time) <= through),
            };
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
    /// Retains the live bars, quotes, pause state and feed configuration for exit, which gives
    /// back the ATR and percentage sizes as [`Self::set_replay_time`] describes. Recorded trades
    /// build partial bars through the normal feed path, alongside quotes, orders and depth.
    /// Analyst targets come from the driver, which answers none, and from the live feed on exit.
    /// Attach the same driver to every participating chart, then advance it once per frame before
    /// drawing. Its time is inclusive; [`Self::replay_time`] reports the exclusive cutoff one
    /// nanosecond later. An interval change resubscribes at the same shared position. On exit,
    /// the order-flow inputs of the studies are rebuilt from prepared history and executions
    /// the live feed kept, also while it stays paused; without a feed they are emptied, and a
    /// host that fills them itself pushes its events again.
    ///
    /// Other series that the host supplies are windowed as in [`Self::set_replay_time`]: they
    /// show only the bars completed by [`Self::replay_time`] and follow seeks, playback,
    /// interval changes and bar limits. `set_data`, `update` and `pop_bars` change their retained
    /// history, which `None` restores. For the main series they change only the retained
    /// history, since the driver builds the bars it shows; series the feed supplies show what
    /// the driver sends.
    pub fn set_market_replay(&mut self, driver: Option<Arc<ReplayDriver>>) {
        if let Some(replay) = self.market_replay.take() {
            self.feed_link = replay.feed;
            // The targets held are the driver's: ask the live feed again.
            self.reload_price_targets();
            self.replay = Some(replay.window);
            self.set_replay_time(None);
            // The studies' order-flow inputs hold what the driver delivered, and a paused feed
            // reloads nothing: rebuild them from the restored feed, or empty them without one.
            // Clearing first also drops the recorded quotes and books and the driver's
            // allocations to trade-based bars, which a rebuild keeps.
            for study in self.studies.indicators() {
                for input in study.order_flow_inputs() {
                    input.clear();
                    if let Some(link) = &mut self.feed_link {
                        link.rebuild_study_input(input, &self.state.data().bars);
                    }
                }
            }
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
        let entry = driver.time();
        self.set_replay_time(Some(entry));
        let window = self.replay.take().expect("replay window");
        let feed = self.feed_link.take();
        self.attach_feed(driver.clone(), 1);
        self.set_liquidity_heatmap_config(heatmap_config);
        if let (Some(live), Some(replay)) = (&feed, &mut self.feed_link) {
            replay.depth_retention = live.depth_retention;
            replay.heatmap = live.heatmap.as_ref().map(|_| Default::default());
            replay.book_visible = live.book_visible;
            replay.bubbles_config = live.bubbles_config.clone();
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
        // The driver delivers no execution past its cursor, so lift the cutoff the entry set.
        self.replay_order_flow(entry);
        // The driver's cutoff is one nanosecond after the time the entry windowed at.
        self.apply_replay_window();
    }

    /// Windows the host series again once the market replay driver moved.
    pub(super) fn follow_market_replay(&mut self) {
        if self
            .market_replay
            .as_ref()
            .is_some_and(|replay| Some(replay.window.time) != self.replay_time())
        {
            self.apply_replay_window();
        }
    }

    /// The retained history of a candle or market replay.
    pub(super) fn replay_window(&self) -> Option<&ReplayWindow> {
        (self.replay.as_ref()).or(self.market_replay.as_ref().map(|replay| &replay.window))
    }

    pub(super) fn replay_window_mut(&mut self) -> Option<&mut ReplayWindow> {
        (self.replay.as_mut()).or(self.market_replay.as_mut().map(|replay| &mut replay.window))
    }

    /// Whether the replay window decides the bars `series` shows: every series in candle
    /// replay, and in market replay the series other than the main one that no feed supplies.
    pub(super) fn replay_windows(&self, series: SeriesId) -> bool {
        self.replay.is_some()
            || self.market_replay.is_some() && series != SeriesId::MAIN && !self.is_fed(series)
    }

    /// Whether host bars for `series` go to the retained replay history: those of the windowed
    /// series and, in market replay, those of the main series.
    pub(super) fn replay_retains(&self, series: SeriesId) -> bool {
        self.replay_windows(series) || self.market_replay.is_some() && series == SeriesId::MAIN
    }

    /// Enters or seeks replay, retaining the full history inside the chart. Only bars completed
    /// by `time` reach rendering, studies, auto-scaling and readouts, including the histories
    /// that studies request for other symbols and intervals. A bar closes at its next open on
    /// [`Self::bar_clock`], and a daily, weekly or monthly bar of an instrument with a session
    /// when its last trading window closes, including the extended and overnight hours
    /// selected by [`Self::set_extended_hours`] and [`Self::set_overnight_hours`]; without a timeframe or on numeric and custom
    /// horizontal axes, keys strictly before `time` are available. Supply finer bars with
    /// `set_replay_source` to build the unfinished chart bar. No future OHLC is inferred from a
    /// completed coarse bar. ATR and percentage sizes resolve from the bars before the cursor when
    /// the session starts and after a step back, and stay while the cursor moves forward.
    ///
    /// The main feed is suspended and current quotes and book views are hidden. Use
    /// [`Self::set_market_replay`] to replay recorded trades, quotes and depth together.
    /// `set_data` and `update` retain
    /// host corrections behind the window. `None` exits, restores all retained bars and quotes
    /// and the sizes from before the session, and restores the feed's previous pause state; a
    /// feed that resumes keeps the restored bars and depth history as [`Self::set_feed_paused`]
    /// describes. Sizes whose parameters were set during the session, and all of them once its
    /// main history was replaced, resolve from the restored history instead. Hosts own source
    /// data and session storage.
    /// With a market driver attached this dispatches a seek; call outside egui context locks,
    /// as with [`ReplayDriver::seek`]. The rebuilt bars arrive on the next frame, and the view
    /// follows them as it follows playback: near the live edge it follows the cursor, and a view
    /// scrolled back keeps the bars it shows in place.
    pub fn set_replay_time(&mut self, time: Option<DateTime<Utc>>) {
        if let Some(replay) = &self.market_replay {
            if let Some(time) = time {
                let inclusive = time
                    .checked_sub_signed(chrono::TimeDelta::nanoseconds(1))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                if replay.driver.time() != inclusive {
                    replay.driver.seek(inclusive);
                }
                self.apply_replay_window();
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
                // The sizes come back with the bars, as the session found them.
                let sizes = self.chart_type_params.sizes_mut().into_iter();
                for ((size, pending), live) in sizes
                    .zip(&mut self.pending_box_sizes)
                    .zip(replay.live_sizes)
                {
                    if let Some(live) = live {
                        (*size, *pending) = live;
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
            let methods = self.chart_type_params.size_methods();
            let sizes = self.chart_type_params.sizes_mut().map(|size| *size);
            let pending = self.pending_box_sizes;
            let live_sizes = std::array::from_fn(|i| {
                (methods[i] != crate::model::BoxSizeMethod::Fixed).then_some((sizes[i], pending[i]))
            });
            self.replay = Some(ReplayWindow {
                time,
                bars,
                source: None,
                feed_paused,
                live_sizes,
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
            // No size may come from a bar after the cursor: they resolve from the bars before
            // it, as at the start of the session.
            self.arm_box_sizes();
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
    /// A source bar joins the chart bar that holds its open on [`bar_clock`](Self::bar_clock).
    /// On a daily or longer chart whose instrument has a session, intraday bars and trades join
    /// the bar of their trading date instead, as
    /// [`data::resample_with_session`](crate::data::resample_with_session) dates it: a bar
    /// belongs to the selected window that holds its first instant inside the selected hours,
    /// and to that window's date, which starts at the date's earliest window. Post-market hours
    /// after 00:00 UTC and a session that opens the evening before stay with their date. Bars
    /// that reach no selected window join no chart bar, and the source replaces the chart bars
    /// of the dates it covers.
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

    /// Applies the candle replay's cutoff, or its end, to the order-flow inputs of the studies,
    /// which the paused feed no longer updates, and recalculates those studies from the bar
    /// containing `since`, the earlier of the old and the new cutoff. A market replay's feed
    /// needs no cutoff, since its driver delivers no execution past its cursor.
    fn replay_order_flow(&mut self, since: DateTime<Utc>) {
        let time = self.replay.as_ref().map(|replay| replay.time);
        let link = self.feed_link.as_ref();
        let clock = link.map_or_else(|| self.bar_clock(), |link| link.clock.clone());
        let trades = link.map(|link| &link.trades);
        let open = clock.instant_open(since);
        let from = (self.state.data().bars).partition_point(|bar| clock.bar_open(bar.time) < open);
        // The window keeps the bars after the cutoff aside; the executions are grouped along all.
        let history = (self.replay.as_ref())
            .and_then(|replay| replay.bars.get(&SeriesId::MAIN))
            .map_or(&[][..], Vec::as_slice);
        for (at, study) in self.studies.indicators().iter().enumerate() {
            let inputs = study.order_flow_inputs();
            if inputs.is_empty() {
                continue;
            }
            for input in inputs {
                input.set_replay(&clock, time, trades, history);
            }
            self.pending_from[at] = Some(self.pending_from[at].map_or(from, |old| old.min(from)));
        }
    }

    pub(super) fn apply_replay_window(&mut self) {
        let Some(time) = self.replay_time() else {
            return;
        };
        let clock = self
            .calendar_timeframe()
            .map_or_else(BarClock::exact, |_| self.bar_clock());
        let market = self.market_replay.is_some();
        let main_live = self.main_bars_live();
        let Some(replay) =
            (self.replay.as_mut()).or(self.market_replay.as_mut().map(|replay| &mut replay.window))
        else {
            return;
        };
        replay.time = time;
        // A removed main series keeps its limit unread, as in `Chart::bar_limit`.
        let limits = (self.bar_limits.iter()).filter(|(id, _)| **id != SeriesId::MAIN || main_live);
        for (&id, &limit) in limits {
            let count = replay
                .bars
                .get(&id)
                .map_or(0, |bars| super::series::evicted_bars(bars.len(), limit));
            if count > 0 {
                replay.remove_oldest_bars(id, count, &clock);
                if id == SeriesId::MAIN && !market {
                    self.feed.history_complete = false;
                }
            }
        }
        // In market replay a fed series shows what the driver sends.
        for item in (self.price_series.iter_mut()).filter(|item| !(market && item.fed)) {
            if let Some(bars) = replay.bars.get(&item.id) {
                let visible = completed(bars, &clock, time);
                // Market playback runs this every frame; an unchanged window keeps the revision.
                if item.bars.len() != visible.len()
                    || !item.bars.iter().zip(visible).all(|(a, b)| same_bar(a, b))
                {
                    item.bars = visible.to_vec();
                    item.revision = crate::model::next_revision();
                    if let Some(marks) = &mut item.change_marks {
                        // Windowing keeps timestamps; host bars can share an interval.
                        marks.retain_bars(&item.bars, &BarClock::exact());
                    }
                }
            }
        }
        if market {
            return;
        }
        let Some(bars) = replay.bars.get(&SeriesId::MAIN) else {
            return;
        };
        let mut visible = completed(bars, &clock, time).to_vec();
        if let Some((source, source_clock)) = &replay.source
            && let Some((start, end)) = covered(source, source_clock, &clock)
            && let Some(partial) =
                fold_source(completed(source, source_clock, time), source_clock, &clock)
        {
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
        // A step forward keeps the sizes resolved for the session, as bars streaming in do.
        let (replay, pending) = (self.replay.take(), self.pending_box_sizes);
        self.replace_main_data(visible);
        (self.replay, self.pending_box_sizes) = (replay, pending);
    }

    pub(super) fn replay_set_data(&mut self, id: SeriesId, bars: Vec<Bar>) {
        let clock = self.clock();
        let Some(replay) = self.replay_window_mut() else {
            return;
        };
        let (bars, rejected) = super::series::clean(bars, &clock);
        replay.bars.insert(id, bars);
        if id == SeriesId::MAIN {
            // The exit resolves the sizes from the new history.
            replay.live_sizes = [None; 4];
        }
        self.count_replay_rejections(id, rejected);
        // In bar replay new main history resolves the sizes again from the bars before the
        // cursor, as `set_data` does outside replay. In market replay it changes no bar on show.
        if id == SeriesId::MAIN && self.market_replay.is_none() {
            self.arm_box_sizes();
        }
        self.apply_replay_window();
    }

    /// Resolves the sizes derived from history again, from the bars before the cursor.
    fn arm_box_sizes(&mut self) {
        self.pending_box_sizes = (self.chart_type_params.size_methods())
            .map(|method| method != crate::model::BoxSizeMethod::Fixed);
    }

    /// Merges `bar` into the retained bars of `id`; returns whether they changed.
    pub(super) fn replay_update(&mut self, id: SeriesId, bar: Bar) -> bool {
        let clock = self.clock();
        let Some(replay) = self.replay_window_mut() else {
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
        let Some(replay) = self.replay_window_mut() else {
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
            .replay_window_mut()
            .and_then(|replay| replay.bars.remove(&series.id))
        {
            series.bars = bars;
        }
    }

    /// Retains the bars of `series` as it enters the chart, leaving it those the cutoff completed.
    pub(super) fn replay_admit(&mut self, series: &mut PriceSeries) {
        let clock = self
            .calendar_timeframe()
            .map_or_else(BarClock::exact, |_| self.bar_clock());
        let time = self.replay_time();
        if let (Some(time), Some(replay)) = (time, self.replay_window_mut()) {
            let bars = std::mem::take(&mut series.bars);
            series.bars = completed(&bars, &clock, time).to_vec();
            if let Some(marks) = &mut series.change_marks {
                marks.retain_bars(&series.bars, &BarClock::exact());
            }
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
        bar_close(clock, clock.bar_open(bar.time)).map_or(bar.time < time, |end| end <= time)
    });
    &bars[..end]
}

/// When the bar of `clock` opening at `open` closes in bar replay: at its next open, or for a
/// daily, multi-day, weekly or monthly bar of a clock with an exchange session when its last
/// trading window closes. A bar without a trading window closes at its next open.
/// `ReplayController::cutoff` and `seek_cutoff` close bars here too.
pub(crate) fn bar_close(clock: &BarClock, open: DateTime<Utc>) -> Option<DateTime<Utc>> {
    (clock.timeframe())
        .filter(|timeframe| {
            clock.has_session() && timeframe.duration_ms() >= Timeframe::Day1.duration_ms()
        })
        .and_then(|timeframe| clock.sessions().bar_session_close(open, timeframe, true))
        .or_else(|| clock.next_open(open))
}

/// The chart bars that `bars` of the `source` clock build under `chart`, or `None` when they do
/// not nest in them. On a daily or longer chart with a session, intraday bars and trades build
/// the bars of their trading dates, as `trading_dates` places them. Elsewhere a bar of one
/// trade is an instant and falls in the chart bar that holds it; other sources go through
/// `data::resample`, which refuses the other trade-based intervals, whose bars are stamped at
/// their first trade. Replay folds the completed trades on every step, so they are folded per
/// chart bar, bisecting for its last trade, instead of a clock lookup per trade: a session
/// clock's lookup is not cheap. `bar_open` is ordered, as `completed` assumes.
fn fold_source(bars: &[Bar], source: &BarClock, chart: &BarClock) -> Option<Vec<Bar>> {
    if let Some(runs) = trading_dates(bars, source, chart) {
        let mut folded: Vec<Bar> = Vec::new();
        for (slot, run) in runs {
            for bar in &bars[run] {
                match folded.last_mut() {
                    Some(last) if last.time == slot => *last = fold(last, bar)?,
                    _ => folded.push(Bar {
                        time: slot,
                        colors: None,
                        payload: None,
                        ..bar.clone()
                    }),
                }
            }
        }
        return Some(folded);
    }
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

/// The opens of the first and last chart bars that `bars` of the `source` clock join under
/// `chart`, `None` when they join none.
fn covered(
    bars: &[Bar],
    source: &BarClock,
    chart: &BarClock,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    match trading_dates(bars, source, chart) {
        Some(runs) => Some((runs.first()?.0, runs.last()?.0)),
        None => Some((
            chart.bar_open(bars.first()?.time),
            chart.bar_open(bars.last()?.time),
        )),
    }
}

/// The chart bars that intraday bars or trades of the `source` clock join on a daily or longer
/// `chart` with a session, by trading date as `data::resample_with_session` dates them. A bar
/// joins the selected window that holds its first instant inside the selected hours, which for
/// a bar that opens before a window and ends inside it is the window's open, and the chart bar
/// of that window's trading date, which starts at the date's earliest window. Bars that reach
/// no window join no chart bar. Returns one entry per window holding bars, with the chart bar's
/// open and the range of those bars, in order, and `None` for other charts and sources, which
/// `bar_open` places. The lookups follow the windows, not the bars.
fn trading_dates(
    bars: &[Bar],
    source: &BarClock,
    chart: &BarClock,
) -> Option<Vec<(DateTime<Utc>, Range<usize>)>> {
    let timeframe = source.timeframe()?;
    let intraday = match timeframe {
        Timeframe::Ticks(count) => count.get() == 1,
        _ => !timeframe.is_trade_based() && timeframe.duration_ms() < Timeframe::Day1.duration_ms(),
    };
    let daily = (chart.timeframe())
        .is_some_and(|interval| interval.duration_ms() >= Timeframe::Day1.duration_ms());
    if !(intraday && daily && chart.has_session()) {
        return None;
    }
    // On an intraday interval an instant takes the date of the last session open at or before it.
    let sessions = chart.sessions().with_timeframe(Timeframe::Min1);
    // Zero for a trade, which is an instant.
    let span = timeframe.duration();
    let mut runs = Vec::new();
    let mut at = 0;
    while let Some(bar) = bars.get(at) {
        // The window holding the bar's open or the next one; none within a year ends the walk.
        let Some(open) = sessions.open_at_or_after(bar.time) else {
            break;
        };
        let Some(close) = sessions.session_close(open, true) else {
            break;
        };
        // Bars that end by the window's open hold none of its trades.
        let start = at
            + bars[at..].partition_point(|bar| {
                bar.time < open
                    && (bar.time.checked_add_signed(span)).is_some_and(|end| end <= open)
            });
        let end = start + bars[start..].partition_point(|bar| bar.time < close);
        if start < end
            && let Some(slot) = chart.open_on_date(sessions.trading_date(open))
        {
            runs.push((slot, start..end));
        }
        // The bar at `at` either reaches the window, which closes after it opens, or ends before
        // it, so each pass moves past at least one bar.
        at = end;
    }
    Some(runs)
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
        if source >= charts.len().min(MAX_CHARTS) {
            return;
        }
        for (index, chart) in charts.iter_mut().take(MAX_CHARTS).enumerate() {
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
    /// at minute 2 completes the bars of minutes 0 and 1; on a custom axis, cutoff key 3
    /// admits keys 0, 1 and 2 regardless of the displayed calendar interval.
    #[test]
    fn custom_key_replay_cutoff_ignores_the_timeframe() {
        let scale = Arc::new(crate::scales::CustomHorizontalScale::new(0..6_u8, |key| {
            key.to_string()
        }));
        for (timeframe, custom, removed_in_replay) in [
            (Some(Timeframe::Min1), false),
            (None, true),
            (Some(Timeframe::Day1), true),
            (Some(Timeframe::Month1), true),
        ]
        .into_iter()
        .flat_map(|(timeframe, custom)| [true, false].map(|removed| (timeframe, custom, removed)))
        {
            let minutes: Vec<_> = (0..5)
                .map(|i| {
                    if custom {
                        scale.value(&i, 10.0).unwrap()
                    } else {
                        bar(i.into(), [10.0; 5])
                    }
                })
                .collect();
            let cutoff = if custom {
                scale.time(&3).unwrap()
            } else {
                time(2)
            };
            let count = if custom { 3 } else { 2 };
            let mut chart = Chart::new(BarData::from_bars(minutes.clone()));
            if custom {
                chart.set_horizontal_scale(Some(scale.clone()));
            }
            if let Some(timeframe) = timeframe {
                chart.set_timeframe(timeframe);
            }
            let id = chart.add_series(Default::default());
            chart.set_data(id, minutes.clone());
            if removed_in_replay {
                chart.set_replay_time(Some(cutoff));
                assert_eq!(chart.bars(SeriesId::MAIN), &minutes[..count]);
                assert_eq!(chart.bars(id), &minutes[..count]);
            }
            assert!(chart.user_action(|chart| chart.remove_series(id)));
            if removed_in_replay {
                chart.set_replay_time(None);
                assert!(chart.undo());
            } else {
                chart.set_replay_time(Some(cutoff));
                assert_eq!(chart.bars(SeriesId::MAIN), &minutes[..count]);
                assert!(chart.undo());
                assert_eq!(chart.bars(id), &minutes[..count], "undone during replay");
                chart.set_replay_time(Some(minutes[4].time));
                assert_eq!(chart.bars(SeriesId::MAIN), &minutes[..4]);
                assert_eq!(chart.bars(id), &minutes[..4]);
                chart.set_replay_time(Some(cutoff));
                assert_eq!(chart.bars(id), &minutes[..count]);
                chart.set_replay_time(None);
            }
            assert_eq!(
                chart.bars(id),
                minutes,
                "removed_in_replay={removed_in_replay}"
            );
        }
    }

    /// A chart's extended hours choice selects the driver's executions at the same position.
    #[test]
    fn switching_extended_hours_rebuilds_market_replay_at_the_position() {
        use crate::data::{MarketEvent, ReplayEvent};
        use crate::model::{Instrument, Session, SessionWindow, Tick};
        use chrono::{NaiveTime, Weekday};
        let hm = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let days = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];
        let mut instrument = Instrument::new("A").with_tick_size(1.0);
        instrument.session = Some(
            Session::new([SessionWindow::new(days, hm(9, 30), hm(16, 0))])
                .with_extended_windows([SessionWindow::new(days, hm(4, 0), hm(9, 30))]),
        );
        // Friday 1970-01-02 in UTC, the instrument's default zone: 08:00 and 09:31.
        let at = |h: i64, m: i64| {
            chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(1440 + 60 * h + m)
        };
        let events = [(at(8, 0), 50.0), (at(9, 31), 100.0)]
            .into_iter()
            .enumerate()
            .map(|(id, (time, price))| {
                let fill = Tick::new(time, price, 1.0, None).with_id(id as u64);
                ReplayEvent::new("A", time, id as u64, MarketEvent::Trade(fill))
            })
            .collect();
        let driver = Arc::new(ReplayDriver::new(events, vec![instrument]).unwrap());
        driver.set_speed(0.0).unwrap();
        driver.seek(at(9, 31));
        let mut chart = Chart::new(BarData::new());
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min1);
        chart.set_market_replay(Some(driver.clone()));
        let ctx = egui::Context::default();
        for (extended, expected) in [
            (false, vec![at(9, 31)]),
            (true, vec![at(8, 0), at(9, 31)]),
            (false, vec![at(9, 31)]),
        ] {
            chart.set_extended_hours(extended);
            for _ in 0..4 {
                chart.pump(&ctx);
            }
            let executed: Vec<_> = (chart.bars(SeriesId::MAIN).iter())
                .filter(|bar| bar.volume > 0.0)
                .map(|bar| bar.time)
                .collect();
            assert_eq!(executed, expected, "extended hours {extended}");
            assert_eq!(driver.time(), at(9, 31));
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

    /// A session resolves ATR and percentage sizes from the bars before the cursor when it starts
    /// and after a step back, or once the cursor passes enough bars, and keeps them while the
    /// cursor moves forward, as bars streaming in do. Parameters set and main history supplied
    /// during the session resolve from the bars before the cursor. The end gives back the live
    /// sizes, or resolves from the restored history those set or supplied during the session.
    /// ATR(14) of 14 true ranges of 2 is 2, a 15th of max(16, 15, 1) = 16 makes it
    /// (13 * 2 + 16) / 14 = 3 and one of max(30, 29, 1) = 30 makes it (13 * 2 + 30) / 14 = 4; 1%
    /// of the closes 100, 110 and 120 is 1, 1.1 and 1.2.
    #[test]
    fn replay_steps_keep_history_derived_sizes() {
        use crate::model::{BoxSizeMethod, ChartType, ChartTypeParams};
        let mut chart = Chart::new(BarData::from_bars(
            (0..14)
                .map(|i| bar(i, [100.0, 101.0, 99.0, 100.0, 1.0]))
                .collect(),
        ));
        chart.set_timeframe(Timeframe::Min1);
        chart.set_chart_type(ChartType::Renko);
        chart.set_chart_type_params(ChartTypeParams {
            renko_size_method: BoxSizeMethod::Atr {
                period: 14,
                multiplier: 1.0,
            },
            kagi_size_method: BoxSizeMethod::Percentage(1.0),
            ..Default::default()
        });
        // The live sizes stay [2, 1] through the streamed bar.
        chart.update(SeriesId::MAIN, bar(14, [100.0, 115.0, 99.0, 110.0, 1.0]));
        let mut other: Vec<_> = chart.bars(SeriesId::MAIN).to_vec();
        other[14] = bar(14, [100.0, 129.0, 99.0, 120.0, 1.0]);
        #[derive(Debug)]
        enum Step {
            At(i64),
            End,
            Params,
            History,
        }
        use Step::*;
        for (step, want) in [
            (At(5), [0.0, 1.0]),
            (At(14), [2.0, 1.0]),
            (At(15), [2.0, 1.0]),
            (End, [2.0, 1.0]),
            (At(15), [3.0, 1.1]),
            (At(14), [2.0, 1.0]),
            (At(15), [2.0, 1.0]),
            (Params, [3.0, 1.1]),
            (At(14), [2.0, 1.0]),
            (End, [3.0, 1.1]),
            (At(15), [3.0, 1.1]),
            (History, [4.0, 1.2]),
            (End, [4.0, 1.2]),
        ] {
            match step {
                At(minute) => chart.set_replay_time(Some(time(minute))),
                End => chart.set_replay_time(None),
                Params => chart.set_chart_type_params(chart.chart_type_params().clone()),
                History => assert!(chart.set_data(SeriesId::MAIN, other.clone())),
            }
            chart.show_elements();
            let params = chart.chart_type_params();
            let got = [params.renko_brick_size, params.kagi_reversal];
            assert!(
                got.iter()
                    .zip(want)
                    .all(|(got, want)| (got - want).abs() < 1e-12),
                "{step:?}: {got:?}"
            );
        }
    }

    /// During market replay the host's main history goes to the retained bars while the driver
    /// keeps building the bars on show, so the sizes the session resolved stay. 1% of the close of
    /// 100 at entry is 1, and playback's trade at 200 keeps it, as bars streaming in do.
    #[test]
    fn market_replay_host_history_keeps_the_session_sizes() {
        use crate::data::{MarketEvent, ReplayEvent};
        use crate::model::{BoxSizeMethod, ChartType, ChartTypeParams, Instrument, Tick};
        let events = (0..7)
            .map(|minute| {
                let price = if minute < 6 { 100.0 } else { 200.0 };
                let fill = Tick::new(time(minute), price, 1.0, None);
                ReplayEvent::new("A", time(minute), minute as u64, MarketEvent::Trade(fill))
            })
            .collect();
        let driver = Arc::new(
            ReplayDriver::new(events, vec![Instrument::new("A").with_tick_size(1.0)]).unwrap(),
        );
        driver.set_speed(0.0).unwrap();
        driver.seek(time(5));
        let mut chart = Chart::new(BarData::new());
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min1);
        chart.set_chart_type(ChartType::Renko);
        chart.set_chart_type_params(ChartTypeParams {
            renko_size_method: BoxSizeMethod::Percentage(1.0),
            ..Default::default()
        });
        chart.set_market_replay(Some(driver.clone()));
        let ctx = egui::Context::default();
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        chart.show_elements();
        let entry = chart.chart_type_params().renko_brick_size;
        driver.set_speed(1.0).unwrap();
        driver.advance(std::time::Duration::from_secs(60)).unwrap();
        chart.pump(&ctx);
        chart.show_elements();
        assert_eq!(
            chart.bars(SeriesId::MAIN).last().map(|bar| bar.close),
            Some(200.0),
            "playback shows the trade at 00:06"
        );
        let played = chart.chart_type_params().renko_brick_size;
        chart.set_data(
            SeriesId::MAIN,
            (0..5).map(|minute| bar(minute, [100.0; 5])).collect(),
        );
        chart.show_elements();
        let after = chart.chart_type_params().renko_brick_size;
        assert_eq!([entry, played, after], [1.0; 3]);
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

    /// With an exchange session, a finer replay source forms the bar of each source bar's
    /// trading date, which opens at the earliest window of that date, and replaces only the
    /// dates it covers. In winter the New York post-market hour from 19:00 to 20:00 opens at
    /// 00:00 UTC of the next date, and the Chicago session of Tuesday 6 February opens at 23:00
    /// UTC on Monday. In the break before it, the hour from 16:00 joins no bar, and a 4-hour bar
    /// from 16:00 holds the session's first hours. Evicting the dates before the source's keeps
    /// all of its bars. Bar replay completes the bar of 5 February when its last window closes,
    /// at 01:00 UTC on 6 February in New York and at 22:00 UTC on 5 February in Chicago, and a
    /// replay controller over the daily bars on the chart's clock seeks and cuts off there too.
    /// Daily bars carry volume 100 and source bars volume 1.
    #[cfg(all(feature = "timezones", feature = "ui"))]
    #[test]
    fn replay_folds_post_market_bars_into_their_trading_date() {
        use crate::model::{Instrument, Session, SessionWindow, TimezoneId};
        use crate::scales::TimezoneMode;
        use crate::ui::replay::ReplayController;
        use chrono::Weekday::{Fri, Mon, Thu, Tue, Wed};
        use chrono::{Duration, NaiveTime, TimeZone};
        let at = |day, hour| Utc.with_ymd_and_hms(2024, 2, day, hour, 0, 0).unwrap();
        let hm = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        // `count` bars of volume 1, `step` hours apart from `first`.
        let bars = |first: DateTime<Utc>, step: i64, count: i64| -> Vec<Bar> {
            (0..count)
                .map(|i| {
                    let time = first + Duration::hours(step * i);
                    Bar::new(time, 100.0, 110.0, 100.0, 110.0, 1.0)
                })
                .collect()
        };
        let four_hours =
            |zone: &TimezoneId| BarClock::new(Timeframe::Hour4).with_timezone(zone.clone());
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let new_york = TimezoneId::named("America/New_York").unwrap();
        let chicago = TimezoneId::named("America/Chicago").unwrap();
        let daily: Vec<_> = [1, 2, 5, 6, 7]
            .into_iter()
            .map(|day| Bar::new(at(day, 0), 100.0, 110.0, 100.0, 110.0, 100.0))
            .collect();
        let dated = |days: &[(u32, f64)]| -> Vec<_> {
            (days.iter())
                .map(|&(day, volume)| (format!("Feb {day:02}, 2024"), volume))
                .collect()
        };
        // The close of 5 February and, at the cursor, each source with the dates and volumes
        // the chart shows once it is set.
        for (zone, session, close, cursor, sources, evict) in [
            (
                new_york.clone(),
                Session::new([SessionWindow::new(weekdays, hm(9, 30), hm(16, 0))])
                    .with_extended_windows([
                        SessionWindow::new(weekdays, hm(4, 0), hm(9, 30)),
                        SessionWindow::new(weekdays, hm(16, 0), hm(20, 0)),
                    ]),
                // 20:00 on Monday 5 February in New York, when its post-market closes.
                at(6, 1),
                // 21:00 on Monday 5 February in New York, an hour after its post-market closed.
                at(6, 2),
                vec![
                    // The 16 hours from 04:00; the last, 19:00 to 20:00, opens at 00:00 UTC.
                    (
                        bars(at(5, 9), 1, 16),
                        BarClock::new(Timeframe::Hour1),
                        vec![(1, 100.0), (2, 100.0), (5, 16.0)],
                    ),
                    // The four 4-hour bars from 04:00; the last ends at 01:00 UTC.
                    (
                        bars(at(5, 9), 4, 4),
                        four_hours(&new_york),
                        vec![(1, 100.0), (2, 100.0), (5, 4.0)],
                    ),
                ],
                2,
            ),
            (
                chicago.clone(),
                Session::new([SessionWindow::new(weekdays, hm(17, 0), hm(16, 0))]),
                // 16:00 on Monday 5 February in Chicago, when its session closes.
                at(5, 22),
                // 09:00 on Tuesday 6 February in Chicago, 16 hours into its session.
                at(6, 15),
                vec![
                    // The hour from 16:00 on Monday, in the break, and the 23 of the session.
                    (
                        bars(at(5, 22), 1, 24),
                        BarClock::new(Timeframe::Hour1),
                        vec![(1, 100.0), (2, 100.0), (5, 100.0), (6, 16.0)],
                    ),
                    // The six 4-hour bars from 16:00 on Monday; four have ended.
                    (
                        bars(at(5, 22), 4, 6),
                        four_hours(&chicago),
                        vec![(1, 100.0), (2, 100.0), (5, 100.0), (6, 4.0)],
                    ),
                ],
                3,
            ),
        ] {
            let mut chart = Chart::new(BarData::from_bars(daily.clone()));
            chart.set_timeframe(Timeframe::Day1);
            chart.chart_options.time_scale.timezone = TimezoneMode::Timezone(zone.clone());
            let instrument = Instrument::new("XYZ").with_time_zone(zone.clone());
            chart.set_instrument(Some(instrument.with_session(session)));
            chart.set_extended_hours(true);
            let shown = |chart: &Chart| -> Vec<_> {
                (chart.bars(SeriesId::MAIN).iter())
                    .map(|bar| (chart.format_time(bar.time), bar.volume))
                    .collect()
            };
            let clock = chart.bar_clock();
            let mut controller = ReplayController::new(daily.clone(), Vec::new());
            for (time, days) in [
                (
                    close - Duration::nanoseconds(1),
                    vec![(1, 100.0), (2, 100.0)],
                ),
                (close, vec![(1, 100.0), (2, 100.0), (5, 100.0)]),
            ] {
                chart.set_replay_time(Some(time));
                assert!(controller.seek_cutoff(time, &clock));
                assert_eq!(
                    (shown(&chart), controller.curr_ts().copied()),
                    (dated(&days), days.last().map(|&(day, _)| at(day, 0))),
                    "{zone:?}"
                );
            }
            assert_eq!(controller.cutoff(&clock), Some(close), "{zone:?}");
            chart.set_replay_time(Some(cursor));
            let mut expected = Vec::new();
            for (source, clock, days) in sources {
                expected = dated(&days);
                let accepted = chart.set_replay_source(source, clock);
                assert_eq!(
                    (accepted, shown(&chart)),
                    (true, expected.clone()),
                    "{zone:?}"
                );
            }
            chart.remove_oldest_bars(SeriesId::MAIN, evict);
            assert_eq!(shown(&chart), expected[expected.len() - 1..], "{zone:?}");
        }
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
                .owned()
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

    /// Host series follow the shared cursor as they do in candle replay, and host bars that
    /// arrive during market replay wait in the retained history, which the exit restores. The
    /// exclusive cutoffs one nanosecond after 00:05, 00:02 and 00:08 complete 5, 2 and 8 bars.
    #[test]
    fn market_replay_windows_host_series_at_the_cursor() {
        use crate::data::{MarketEvent, ReplayEvent};
        use crate::model::{Instrument, Tick};
        let events = (0..10)
            .map(|minute| {
                ReplayEvent::new(
                    "A",
                    time(minute),
                    minute as u64,
                    MarketEvent::Trade(Tick::new(time(minute), 10.0, 1.0, None)),
                )
            })
            .collect();
        let driver = Arc::new(
            ReplayDriver::new(events, vec![Instrument::new("A").with_tick_size(1.0)]).unwrap(),
        );
        let minutes: Vec<_> = (0..10).map(|m| bar(m, [1.0; 5])).collect();
        let mut chart = Chart::new(BarData::from_bars(minutes.clone()));
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min1);
        let comparison = chart.add_series(Default::default());
        chart.set_data(comparison, minutes.clone());
        let removed = chart.add_series(Default::default());
        chart.set_data(removed, minutes.clone());
        let ctx = egui::Context::default();
        driver.set_speed(0.0).unwrap();
        driver.seek(time(5));
        chart.set_market_replay(Some(driver.clone()));
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        assert_eq!(chart.bars(comparison), &minutes[..5], "entry");
        let added = chart.add_series(Default::default());
        chart.set_data(added, minutes.clone());
        assert_eq!(
            chart.bars(added),
            &minutes[..5],
            "added during market replay"
        );
        assert!(chart.user_action(|chart| chart.remove_series(removed)));
        let live = bar(10, [2.0; 5]);
        chart.update(SeriesId::MAIN, live.clone());
        assert_eq!(
            chart.bars(SeriesId::MAIN).last().map(|bar| bar.time),
            Some(time(5)),
            "a host bar past the cursor stays hidden"
        );
        driver.seek(time(2));
        chart.pump(&ctx);
        assert_eq!(chart.bars(comparison), &minutes[..2], "seek back");
        driver.set_speed(1.0).unwrap();
        driver.advance(std::time::Duration::from_secs(360)).unwrap();
        chart.pump(&ctx);
        assert_eq!(chart.bars(comparison), &minutes[..8], "playback");
        chart.set_market_replay(None);
        assert_eq!(chart.bars(SeriesId::MAIN).last(), Some(&live), "exit");
        assert_eq!(chart.bars(comparison), minutes);
        assert_eq!(chart.bars(added), minutes);
        assert!(chart.undo());
        assert_eq!(chart.bars(removed), minutes, "undone after exit");
    }

    /// Playback that completes no host bar leaves the host series as it was, also when it holds
    /// a whitespace bar, whose NaN values never compare equal: its primitives see no change.
    #[test]
    fn market_playback_keeps_an_unchanged_host_window() {
        use crate::chart::primitives::Primitive;
        use crate::data::{MarketEvent, ReplayEvent};
        use crate::model::{Instrument, Tick};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Changes(Arc<AtomicUsize>);
        impl Primitive for Changes {
            fn data_changed(&mut self, _: &[Bar]) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let trade = MarketEvent::Trade(Tick::new(time(0), 10.0, 1.0, None));
        let driver = Arc::new(
            ReplayDriver::new(
                vec![ReplayEvent::new("A", time(0), 0, trade)],
                vec![Instrument::new("A").with_tick_size(1.0)],
            )
            .unwrap(),
        );
        let mut chart = Chart::new(BarData::new());
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min1);
        let comparison = chart.add_series(Default::default());
        let mut minutes: Vec<_> = (0..10).map(|m| bar(m, [1.0; 5])).collect();
        minutes[1] = Bar::whitespace(time(1));
        chart.set_data(comparison, minutes);
        let changes = Arc::new(AtomicUsize::new(0));
        chart.attach_primitive(comparison, Changes(changes.clone()));
        driver.set_speed(0.0).unwrap();
        driver.seek(time(5));
        chart.set_market_replay(Some(driver.clone()));
        let ctx = egui::Context::default();
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let seen = changes.load(Ordering::Relaxed);
        driver.set_speed(1.0).unwrap();
        for second in 1..3 {
            // 00:05:10 and 00:05:20 complete no bar.
            driver.advance(std::time::Duration::from_secs(10)).unwrap();
            crate::widget::test_frames::frame(&ctx, &mut chart, f64::from(second), vec![]);
        }
        assert_eq!(chart.bars(comparison).len(), 5, "with the whitespace bar");
        assert_eq!(changes.load(Ordering::Relaxed), seen, "no bar completed");
        // 00:06 completes the 00:05 bar.
        driver.advance(std::time::Duration::from_secs(40)).unwrap();
        crate::widget::test_frames::frame(&ctx, &mut chart, 3.0, vec![]);
        assert_eq!(chart.bars(comparison).len(), 6);
        assert_eq!(changes.load(Ordering::Relaxed), seen + 1);
    }

    /// A market replay seek leaves the view where playback to the same position does. With
    /// minutes 0..=69 shown at a right offset of -30, minute 39 is at the right edge; with the
    /// last bar at minute m it stays there at 39 - m, and a seek that no longer holds it stops
    /// with the oldest bar in view. A view near the live edge follows the cursor to its default
    /// margin of 2.5 bars. A candle replay seek agrees.
    #[test]
    fn market_replay_seek_keeps_a_scrolled_back_view_on_its_bars() {
        use crate::data::{MarketEvent, ReplayEvent};
        use crate::model::{Instrument, Tick};
        use crate::widget::test_frames::frame;
        let ns = chrono::TimeDelta::nanoseconds(1);
        let ctx = egui::Context::default();
        let market = |offset: f32| {
            let events = (0..100)
                .map(|minute| {
                    let fill = Tick::new(time(minute), 10.0, 1.0, None).with_id(minute as u64);
                    ReplayEvent::new("A", time(minute), minute as u64, MarketEvent::Trade(fill))
                })
                .collect();
            let instrument = Instrument::new("A").with_tick_size(1.0);
            let driver = Arc::new(ReplayDriver::new(events, vec![instrument]).unwrap());
            driver.set_speed(0.0).unwrap();
            driver.seek(time(69));
            let mut chart = Chart::new(BarData::new());
            chart.set_symbol("A");
            chart.set_timeframe(Timeframe::Min1);
            chart.set_market_replay(Some(driver.clone()));
            for _ in 0..4 {
                chart.pump(&ctx);
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            assert_eq!(chart.bars(SeriesId::MAIN).len(), 70, "entry");
            chart.scroll_to_position(offset, false);
            (chart, driver)
        };
        let settle = |chart: &mut Chart| {
            for _ in 0..4 {
                chart.pump(&ctx);
            }
            frame(&ctx, chart, 1.0, vec![]);
            let last = chart.bars(SeriesId::MAIN).last().unwrap().time;
            (last, chart.state.time_scale().right_offset())
        };
        let mut rows = Vec::new();
        let (mut chart, driver) = market(-30.0);
        driver.set_speed(1.0).unwrap();
        driver.advance(std::time::Duration::from_secs(600)).unwrap();
        rows.push(("playback to 79", settle(&mut chart)));
        for (name, offset, minute) in [
            ("seek to 79", -30.0, 79),
            ("seek to 59", -30.0, 59),
            ("seek to 20", -30.0, 20),
            ("near the live edge, seek to 79", 0.0, 79),
        ] {
            let (mut chart, _driver) = market(offset);
            chart.set_replay_time(Some(time(minute) + ns));
            rows.push((name, settle(&mut chart)));
        }
        let minutes: Vec<_> = (0..100).map(|m| bar(m, [10.0; 5])).collect();
        let mut chart = Chart::new(BarData::from_bars(minutes));
        chart.set_timeframe(Timeframe::Min1);
        chart.set_replay_time(Some(time(70)));
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart.scroll_to_position(-30.0, false);
        chart.set_replay_time(Some(time(80)));
        rows.push(("candle seek to 79", settle(&mut chart)));
        assert_eq!(
            rows,
            [
                ("playback to 79", (time(79), -40.0)),
                ("seek to 79", (time(79), -40.0)),
                ("seek to 59", (time(59), -20.0)),
                ("seek to 20", (time(20), 0.0)),
                ("near the live edge, seek to 79", (time(79), 2.5)),
                ("candle seek to 79", (time(79), -40.0)),
            ]
        );
    }
}
