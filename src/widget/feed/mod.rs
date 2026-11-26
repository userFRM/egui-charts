//! A chart driven by a [`Datafeed`]: requests issued from `show`, answers and live pushes applied
//! once per frame.
//!
//! Every feed call but the one in [`FeedLink`]'s `Drop` comes from [`Chart::pump`], which runs
//! outside any egui closure: a feed may answer or push inline, and that runs the waker, which
//! takes the egui context's lock.

pub(super) mod checkpoint;
mod checkpoint_key;
mod footprints;
mod pump;

use super::{Chart, SeriesId};
use crate::chart::overlays::{VolumeBubbles, VolumeBubblesConfig};
use crate::data::inbox::Inbox;
use crate::data::trades::Trades;
use crate::data::{
    Datafeed, DepthHistory, DepthRequest, FeedCapabilities, FeedError, Footprint, FootprintRequest,
    FootprintState, HistoryRequest, Reply, SubscriptionId, TradeRequest,
};
use crate::model::{Bar, BarClock, Instrument, OrderBook, Tick, Timeframe};
use crate::theme::ThemeExt;
use egui::ViewportId;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Trades kept per series for order flow.
pub(crate) const TRADE_CAPACITY: usize = 100_000;
/// Bars a raw-trade footprint keeps, the oldest dropped first.
const FOOTPRINT_BARS: usize = 5_000;
/// Price rows a raw-trade footprint keeps, the oldest bars dropped first.
const FOOTPRINT_ROWS: usize = TRADE_CAPACITY / 2;

/// Bars from the first visible bar to the oldest one below which an older page is asked for.
const SCROLL_BACK_MARGIN: usize = 150;

/// The smallest scroll-back page asked for.
const MIN_SCROLL_BACK_PAGE: usize = 600;

/// A chart's link to its feed.
pub(crate) struct FeedLink {
    pub(super) feed: Arc<dyn Datafeed>,
    inbox: Arc<Inbox>,
    /// Slots of the chart's timeframe, by which history, live bars and ticks merge.
    pub(super) clock: BarClock,
    /// Hours choice of the retained executions: extended and overnight hours.
    extended_hours: bool,
    overnight_hours: bool,
    /// Currency or unit of the retained executions, as `extended_hours` is their hours choice.
    conversion: Option<String>,
    initial_bars: usize,
    /// Whether history is requested adjusted for dividends.
    pub(super) adjusted: bool,
    /// The viewport the chart was last shown in, which the waker wakes. Set by the first `pump`,
    /// which installs the waker, and cleared by `Drop` before it unsubscribes, so the waker does
    /// nothing then.
    wake: Arc<Mutex<Option<ViewportId>>>,
    /// The symbol or timeframe changed: the next `pump` reloads.
    reload_pending: bool,
    /// The pause ended: the next `pump` reloads, keeping the bars, executions and depth history
    /// when nothing else changed.
    resumed: bool,
    /// Open of the last bar a resume dropped; the bars before it are kept. The new subscription
    /// drops older executions, and a latest page that starts after it replaces the kept bars.
    resumed_from: Option<chrono::DateTime<chrono::Utc>>,
    /// On a trade-based interval, the open of the oldest bar this subscription pushed that the
    /// history lacks or holds with other values, which stands for the first live execution while
    /// none is held.
    pushed_from: Option<chrono::DateTime<chrono::Utc>>,
    /// Host-supplied or paged candles cannot resume a trade-built partial candle.
    pub(super) non_trade_bars: bool,
    pub(super) paused: bool,
    /// The pause last set with [`Chart::set_feed_paused`]. `paused` also holds while the main
    /// bars take no data or a replay runs.
    pub(super) host_paused: bool,
    next_key: u64,
    /// Requests not answered yet; an answer under any other key is stale and dropped.
    pending: Vec<(u64, Request, u32)>,
    retries: Vec<(f64, Request, u32)>,
    subscription: Option<SubscriptionId>,
    pub(crate) instrument: Option<Instrument>,
    /// What the feed offers as a whole, once it has answered.
    capabilities: Option<FeedCapabilities>,
    pub(super) trades: Trades,
    /// The held trades as volume bubbles, when the host turned them on.
    pub(super) bubbles: Option<VolumeBubbles>,
    pub(super) bubbles_config: VolumeBubblesConfig,
    /// The trades as a footprint on the instrument's tick, while the chart type or a volume
    /// profile drawing reads them. Raw-only history uses [`FOOTPRINT_BARS`] and
    /// [`FOOTPRINT_ROWS`]; prepared pages outlive those budgets and the raw tape.
    pub(super) footprint: Option<FootprintState>,
    /// Live trade-bar allocations shared with the sink, independent of paged executions.
    footprint_input: Option<(crate::data::OrderFlowInput, u64)>,
    /// The footprint of the trades before the cutoff, while a candle replay runs under a visible
    /// volume profile drawing that reads the trades, rebuilt when the cutoff moves.
    pub(super) replay_footprint: Option<(chrono::DateTime<chrono::Utc>, Option<Footprint>)>,
    order_book: Option<OrderBook>,
    order_queue: Option<crate::data::OrderQueue>,
    /// The order book over time, when the host turned it on.
    pub(super) depth_history: Option<DepthHistory>,
    /// The chart's retention, also applied when a history starts later.
    pub(super) depth_retention: Option<chrono::TimeDelta>,
    depth_complete: bool,
    footprints_complete: bool,
    /// A terminal footprint error stops requests until refresh, independently of older EOF.
    footprints_stopped: bool,
    /// Unavailable spans suppressed only for the current feed context.
    footprint_unavailable: Vec<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    /// Where older history continues after an empty page, by source: bars, trades, depth. The
    /// bar cursor keeps the oldest bar held when the feed named it and lapses once that bar goes.
    bar_cursor: Option<(
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    )>,
    trade_cursor: Option<chrono::DateTime<chrono::Utc>>,
    initial_trades: InitialTrades,
    depth_cursor: Option<chrono::DateTime<chrono::Utc>>,
    /// No older trades remain to be loaded.
    trades_complete: bool,
    pub(super) heatmap: Option<std::cell::RefCell<crate::chart::overlays::LiquidityHeatmap>>,
    heatmap_config: crate::chart::overlays::LiquidityHeatmapConfig,
    /// How the live book is shown by the heatmap's profile and the depth ladder.
    book_view: crate::chart::overlays::OrderBookView,
    /// None keeps the profile following the heatmap for existing setter callers.
    pub(super) book_visible: Option<bool>,
    study_inputs: Vec<(crate::data::OrderFlowInput, u64)>,
    time: Option<chrono::DateTime<chrono::Utc>>,
    /// A permanent history or resolve failure stops paging until the next reload.
    pub(super) paging_stopped: bool,
}

/// Demand may arrive before instrument resolution. Once sent, the normal request lifecycle
/// owns the page (including retries) until the chart subscribes again.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InitialTrades {
    Waiting,
    Wanted,
    Requested,
}

#[derive(Clone)]
enum Request {
    Capabilities,
    Resolve,
    History {
        request: HistoryRequest,
        scroll_back: bool,
    },
    Trades(TradeRequest),
    Footprints(FootprintRequest),
    Depth(DepthRequest),
}

impl FeedLink {
    pub(super) fn server_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.inbox.server_time(self.subscription?)
    }

    /// A reply for `request`, under a new key.
    fn reply<T: Send + 'static>(&mut self, request: Request) -> Reply<T> {
        self.next_key += 1;
        self.pending.push((self.next_key, request, 0));
        self.inbox.reply(self.next_key)
    }

    /// Sums the held trades again, after the clock, the instrument or the trades changed.
    /// `history` holds the chart's bars.
    fn rebuild_bubbles(&mut self, history: &[Bar]) {
        if let Some(bubbles) = self.bubbles.take() {
            self.bubbles = Some(self.bubbles(bubbles.config));
        }
        if let Some(flow) = self
            .footprint
            .as_mut()
            .filter(|flow| flow.live.prepared_before().is_some())
        {
            flow.live.rebuild_prepared(&self.trades);
        } else if self.footprint.is_some() {
            self.footprint = self.footprint(None, history).map(FootprintState::new);
        }
    }

    fn rebuild_paged_trade_rate(&self, input: &crate::data::OrderFlowInput, history: &[Bar]) {
        if let Some(tf) = self.clock.timeframe().filter(|tf| tf.is_trade_based())
            && self.trades.from_start
            && self.pushed_from.is_none()
            && self.trades.live().next().is_none()
            && self.trades.unheld_live().is_none()
            && self.trades.lost_until().is_none()
            && self.trades.retained_until().is_none()
            && self.trades.left_out().is_empty()
        {
            let tick_size = self.instrument.as_ref().and_then(|i| i.tick_size);
            input.rebuild_paged_trade_rate(tf, tick_size, &self.trades, history);
        }
    }

    /// Rebuilds a study's order-flow `input` from prepared history and held trades along the
    /// chart's bars, at the instrument's tick size and latest time, as connecting
    /// it does. Like `OrderFlowInput::replace_trades`, it keeps quote and book sources, and on a
    /// trade-based interval a source the sink has allocated to, as they are.
    pub(super) fn rebuild_study_input(
        &mut self,
        input: &crate::data::OrderFlowInput,
        history: &[Bar],
    ) {
        if input.is_footprint()
            && let Some(flow) = &mut self.footprint
        {
            flow.share_prepared();
        }
        input.set_tick_size(self.instrument.as_ref().and_then(|i| i.tick_size));
        input.replace_trades(&self.clock, &self.trades, history, self.footprint.as_ref());
        self.rebuild_paged_trade_rate(input, history);
        if let Some(now) = self.time {
            input.advance_time(&self.clock, now);
        }
    }

    /// A paused link for an offscreen image: it holds the visuals the feed built, the footprint,
    /// bubbles, order book and depth, and never talks to the feed or the live link's inbox.
    pub(super) fn frozen(&self) -> Self {
        Self {
            feed: Arc::clone(&self.feed),
            inbox: Arc::default(),
            clock: self.clock.clone(),
            extended_hours: self.extended_hours,
            overnight_hours: self.overnight_hours,
            conversion: self.conversion.clone(),
            initial_bars: self.initial_bars,
            adjusted: self.adjusted,
            wake: Arc::default(),
            reload_pending: false,
            resumed: false,
            resumed_from: None,
            pushed_from: None,
            non_trade_bars: self.non_trade_bars,
            paused: true,
            host_paused: true,
            next_key: 0,
            pending: Vec::new(),
            retries: Vec::new(),
            subscription: None,
            instrument: self.instrument.clone(),
            capabilities: self.capabilities.clone(),
            trades: Trades::new(TRADE_CAPACITY),
            bubbles: self
                .bubbles
                .as_ref()
                .map(|bubbles| self.bubbles(bubbles.config.clone())),
            footprint: self.footprint.clone(),
            footprint_input: None,
            replay_footprint: self.replay_footprint.clone(),
            order_book: self.order_book.clone(),
            order_queue: self.order_queue.clone(),
            depth_history: self.depth_history.clone(),
            depth_retention: self.depth_retention,
            depth_complete: self.depth_complete,
            footprints_complete: true,
            footprints_stopped: true,
            footprint_unavailable: Vec::new(),
            bar_cursor: None,
            trade_cursor: None,
            initial_trades: InitialTrades::Waiting,
            depth_cursor: None,
            trades_complete: false,
            heatmap: self.heatmap.is_some().then(Default::default),
            heatmap_config: self.heatmap_config,
            book_view: self.book_view.clone(),
            book_visible: self.book_visible,
            bubbles_config: self.bubbles_config.clone(),
            study_inputs: Vec::new(),
            time: self.time,
            paging_stopped: true,
        }
    }

    /// The held trades as a footprint, or `None` without the instrument's tick size. The held
    /// trades are taken in time order, as a stream can stamp its first trades before held ones.
    /// The bar of the oldest held trade is incomplete until a trade page reports the start of
    /// trade history.
    /// On a trade-based interval, paged trades fill only the bars of `history`, the chart's, that
    /// open before the live executions, held ones or those the store kept apart at the first
    /// instant a page covers while the chart holds a bar at or before the oldest of them, or
    /// without them before the oldest pushed bar the history lacks or holds with other values,
    /// and that they rebuild ([`Footprint::push_history`]). No grouping takes in an instant at
    /// which the held executions may lack some, as a page left them out or no page reached the
    /// first instant of a stream, whatever subscriptions changed since: the newest executions
    /// between such instants that prove a bar are kept. With `before`, includes only executions
    /// strictly before that replay cutoff.
    /// On a trade-based interval, a cutoff after the stream's first held execution gives `None`
    /// once the store has lost executions: the stream's bars cannot then be summed again.
    pub(super) fn footprint(
        &self,
        before: Option<chrono::DateTime<chrono::Utc>>,
        history: &[Bar],
    ) -> Option<Footprint> {
        let tick_size = self.instrument.as_ref()?.tick_size?;
        let mut flow = Footprint::new(self.clock.clone(), tick_size, 1).ok()?;
        flow.set_budget(FOOTPRINT_BARS, FOOTPRINT_ROWS);
        // Replay cannot repair a sealed partial slot with later raw history.
        let excluded = self
            .footprint
            .as_ref()
            .map(FootprintState::view)
            .filter(|source| !source.partial_slots().is_empty());
        if let Some(source) = excluded {
            for &open in source.partial_slots() {
                flow.add_missing(
                    self.clock.bar_start(open),
                    self.clock
                        .next_open(open)
                        .map(|next| self.clock.bar_start(next))
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC),
                );
            }
        }
        let trade_based = self.clock.timeframe().is_some_and(|tf| tf.is_trade_based());
        let held = || {
            self.trades
                .iter()
                .take(if trade_based {
                    self.trades.iter().count() - self.trades.live().count()
                } else {
                    usize::MAX
                })
                .filter(|trade| {
                    excluded
                        .is_none_or(|source| !source.partial_at(self.clock.instant_open(trade.ts)))
                })
        };
        let trades = || held().take_while(|trade| before.is_none_or(|time| trade.ts < time));
        // Paged executions have their own anchor; the sink supplies live allocations below.
        let lost = self.trades.lost_until();
        if trade_based && !history.is_empty() {
            // An execution at the loss's time may have lost the one that opened its bar.
            let paged: Vec<_> = held()
                .filter(|trade| lost.is_none_or(|lost| trade.ts > lost))
                .collect();
            // Bars from the first live execution, or pushed bar, on hold executions the page
            // lacks, so it cannot end with them.
            let live = (self.trades.live().next().map(|trade| trade.ts)).or(self.pushed_from);
            let mut paged_bars =
                history.partition_point(|bar| live.is_none_or(|live| bar.time < live));
            // The page's executions at the first live one's time were left out, so the bar before
            // may lack some of its own: it proves nothing.
            if live.is_some_and(|live| self.trades.left_out_at() == Some(live)) {
                paged_bars = paged_bars.saturating_sub(1);
            }
            // Until a trade page fills the time after the retained executions, some that traded
            // after them may be missing: their end is not the page's and proves nothing.
            let ended = self.trades.retained_until().is_none();
            // The sink opened the bars from the oldest live execution the store kept apart, where
            // the page may hold executions the stream never sent, so the page fills none of them.
            // The sink's allocations fill them, as `OrderFlowInput::chart_run` rebuilds them,
            // unless the store lost an execution at or after the open of the first: then the run
            // starts at the oldest bar after the loss whose executions are all held, and without
            // one `OrderFlowInput::replace_trades` waits for the sink's next allocations. A chart
            // that starts after it holds no bar with the page's executions of that instant.
            let stream_from = (self.trades.unheld_live())
                .and_then(|(_, kept)| kept.first())
                .map(|trade| trade.ts)
                .filter(|at| history.first().is_some_and(|bar| bar.time <= *at));
            // The held executions may lack some of an instant a page left out, or of the first
            // instant of a stream no page reached, whatever subscriptions changed since: no
            // grouping takes in such an instant between the oldest paged execution and the first
            // live one. The executions after the newest are tried first, then those between each
            // pair of older ones, and the first to prove a bar are kept. Before such an instant
            // the bar that opens last may hold executions of it and proves nothing, nor does the
            // end of those executions.
            let oldest = paged.iter().map(|trade| trade.ts).min();
            // Until a page fills the time after the executions the first of several subscription
            // changes retained, the first held instant after them, that of a later stream,
            // follows a time that may lack all of its executions.
            let pending = (self.trades.first_retained_until()).and_then(|until| {
                (paged.iter().map(|trade| trade.ts))
                    .filter(|at| *at > until)
                    .min()
            });
            let splits: BTreeSet<_> = (self.trades.left_out().iter().copied())
                .chain(pending)
                .filter(|at| oldest < Some(*at) && live.is_none_or(|live| *at < live))
                .collect();
            let mut end = None;
            for start in (splits.into_iter().rev().map(Some)).chain([None]) {
                let part: Vec<_> = (paged.iter().copied())
                    .filter(|trade| start.is_none_or(|at| trade.ts > at))
                    .filter(|trade| end.is_none_or(|at| trade.ts < at))
                    .collect();
                let bars = end.map_or(paged_bars, |at| {
                    (history.partition_point(|bar| bar.time < at))
                        .saturating_sub(1)
                        .min(paged_bars)
                });
                let ended = ended && end.is_none();
                flow.push_history(&part, &history[..bars], before, ended, stream_from);
                if !flow.bars().is_empty() {
                    break;
                }
                end = start;
            }
        } else if !trade_based || lost.is_none() || self.trades.live().next().is_none() {
            // A stream can stamp its first trades before held ones: take them in time order.
            let mut ordered: Vec<_> = held()
                .filter(|trade| before.is_none_or(|time| trade.ts < time))
                .collect();
            ordered.sort_by_key(|trade| trade.ts);
            for trade in ordered {
                let _ = flow.push(trade);
            }
        }
        if let Some(lost) = lost {
            flow.drop_through(lost, trades());
        }
        let first = self.unconfirmed_start().or_else(|| {
            // Without bars to match, paged trades group from the oldest held one. Matched ones
            // start after any loss, and live bars carry their own marks.
            let unsure = !self.trades.from_start || lost.is_some();
            if trade_based && history.is_empty() && unsure {
                self.trades.iter().next().map(|trade| trade.ts)
            } else {
                None
            }
        });
        if let Some(first) = first {
            flow.received_from(first);
        }
        if let Some(cutoff) = before.filter(|_| trade_based) {
            // A cutoff can fall inside a sink allocation. Rebuild from every streamed execution:
            // the held live ones, those kept apart at the first instant a page covers and the
            // page's later copies. Their anchor is known only while the whole stream is held.
            if self.trades.lost_until().is_none() {
                let (unheld_at, unheld) = self.trades.unheld_live().unzip();
                let copies = unheld_at
                    .into_iter()
                    .flat_map(|at| held().filter(move |trade| trade.ts > at));
                let unheld = by_time(copies, unheld.into_iter().flatten());
                let stream = by_time(unheld, self.trades.live());
                let mut live = Footprint::new(self.clock.clone(), tick_size, 1).ok()?;
                for trade in stream.take_while(|trade| trade.ts < cutoff) {
                    let _ = live.push(trade);
                }
                flow.copy_trade_bars((&live).into(), None);
            } else if (self.trades.unheld_live().and_then(|(_, kept)| kept.first()))
                .into_iter()
                .chain(self.trades.live().next())
                .any(|trade| trade.ts < cutoff)
            {
                // Without the stream's bars a span could count the page's volume alone as complete.
                return None;
            }
        } else if let Some((input, _)) = &self.footprint_input {
            input.copy_footprint(&mut flow, u64::MAX);
            if let Some(lost) = self.trades.lost_until() {
                flow.drop_through(lost, trades());
            }
        }
        Some(flow)
    }

    /// The oldest held trade, where the footprint starts, while executions before it may
    /// exist that never arrived: no trade page reported the start of trade history and the
    /// store lost none. A loss is marked exactly by `drop_through` instead. Paged trades on a
    /// trade-based interval need no mark: they fill only bars they were proven to rebuild.
    fn unconfirmed_start(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.trades.from_start || self.trades.lost_until().is_some() {
            return None;
        }
        let oldest = self.trades.iter().next()?;
        let paged = self.clock.timeframe().is_some_and(|tf| tf.is_trade_based())
            && !self
                .trades
                .live()
                .next()
                .is_some_and(|live| std::ptr::eq(live, oldest));
        (!paged).then_some(oldest.ts)
    }

    fn bubbles(&self, config: VolumeBubblesConfig) -> VolumeBubbles {
        let tick_size = self.instrument.as_ref().and_then(|i| i.tick_size);
        VolumeBubbles::new(config, self.clock.clone(), tick_size, self.trades.iter())
    }
}

impl Drop for FeedLink {
    fn drop(&mut self) {
        *lock(&self.wake) = None;
        if let Some(id) = self.subscription.take() {
            self.inbox.unsubscribe(id);
            self.feed.unsubscribe(id);
        }
    }
}

impl Chart {
    /// Installs or replaces this chart's feed, loading from the next [`show`](Self::show).
    ///
    /// The same `Arc` allocation is a no-op, also during market replay, where it is compared
    /// with the saved live feed rather than the replay driver. To reload changed settings in
    /// the same adapter, leave replay and call [`reload_feed`](Self::reload_feed).
    ///
    /// A different feed immediately retires this chart's subscriptions and clears feed-owned
    /// bars, study histories, order flow, quotes and metadata, even while paused. No new feed
    /// requests run here. Other charts and a shared replay driver are not changed.
    ///
    /// Keeps chart configuration, drawings, host-supplied auxiliary series and
    /// [`set_study_data`](Self::set_study_data) overrides. The configured initial page size
    /// stays; first attachment uses the builder default of 1,000 bars. Ends replay locally and
    /// restores the pre-replay host pause choice. Clears undo/redo, whose retained series could
    /// otherwise restore old-source bars; the source change is not an undoable action.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        let configured = self
            .market_replay
            .as_ref()
            .map_or(self.feed_link.as_ref(), |replay| replay.feed.as_ref());
        if configured.is_some_and(|link| Arc::ptr_eq(&link.feed, &feed)) {
            return;
        }

        let (mut previous, window) = if let Some(replay) = self.market_replay.take() {
            // Drop only this chart's driver subscription, without seeking or stopping it.
            drop(self.feed_link.take());
            (replay.feed, Some(replay.window))
        } else {
            (self.feed_link.take(), self.replay.take())
        };
        let host_paused = window.as_ref().map_or_else(
            || previous.as_ref().is_some_and(|link| link.host_paused),
            |window| window.feed_paused,
        );
        if let Some(window) = window {
            // Restore only host-owned auxiliary history, never the old main/feed histories.
            for (id, bars) in window.bars {
                if id != SeriesId::MAIN
                    && let Some(item) = self.price_series.iter_mut().find(|item| item.id == id)
                    && !item.fed
                {
                    item.bars = bars;
                    item.revision = crate::model::next_revision();
                }
            }
            for (size, live) in self
                .chart_type_params
                .sizes_mut()
                .into_iter()
                .zip(window.live_sizes)
            {
                if let Some((live, _)) = live {
                    *size = live;
                }
            }
        }

        self.legend_instrument = None;
        self.currency_choice = None;
        self.unit_choice = None;
        self.bar_close_time = None;
        self.server_time_offset = None;
        self.set_quote(None);
        self.quote_action = None;
        self.session_quote = None;
        self.state.price_scale_mut().set_instrument_precision(None);
        self.state.price_scale_mut().set_instrument_ticks(None);
        let initial_bars = previous.as_ref().map_or(1000, |link| link.initial_bars);
        self.attach_feed(feed, initial_bars);
        if let (Some(old), Some(new)) = (&mut previous, &mut self.feed_link) {
            new.adjusted = old.adjusted;
            new.bubbles_config = std::mem::take(&mut old.bubbles_config);
            new.bubbles = old
                .bubbles
                .take()
                .map(|bubbles| new.bubbles(bubbles.config));
            new.depth_retention = old.depth_retention;
            new.depth_history = old.depth_history.take();
            new.depth_history.iter_mut().for_each(DepthHistory::clear);
            new.heatmap = old.heatmap.take().map(|_| Default::default());
            new.heatmap_config = old.heatmap_config;
            new.book_view = std::mem::take(&mut old.book_view);
            new.book_visible = old.book_visible;
        }
        drop(previous);

        // All old sinks are inactive before shared inputs lose their source context.
        let clock = self.bar_clock();
        for study in self.studies.indicators_mut() {
            for input in study.order_flow_inputs() {
                input.clear();
                input.set_tick_size(None);
                input.set_replay(&clock, None, None, &[]);
            }
            study.update(&[], 0);
        }
        self.pending_from.fill(Some(0));
        self.next_refresh.fill(f64::NEG_INFINITY);
        self.stale.fill(false);
        self.element_studies = Default::default();
        self.execution_ranges = Default::default();
        self.replace_main_data(Vec::new());
        self.feed = Default::default();
        for series in self.price_series.iter_mut().filter(|series| series.fed) {
            series.feed = Default::default();
        }
        self.invalidate_study_data();
        self.state.set_elements(None, false);
        self.refresh_time_scale_content();
        #[cfg(feature = "drawings")]
        self.project_drawing_points();
        self.set_feed_paused(host_paused);
        self.history.clear();
        #[cfg(feature = "drawings")]
        self.drawings
            .swap_history(&mut crate::drawings::services::HistoryService::new());
    }

    /// Drives the chart from `feed` from the next [`show`](Self::show) on.
    pub(crate) fn attach_feed(&mut self, feed: Arc<dyn Datafeed>, initial_bars: usize) {
        self.set_timeframe(self.timeframe.unwrap_or_default());
        // The previous feed's histories, and the bars filled from them, are another request's,
        // as are the analyst targets it answered.
        self.study_data.clear_feed();
        self.reload_price_targets();
        let timeframe = self.timeframe.unwrap_or_default();
        self.readout_revision = crate::model::next_revision();
        self.feed_link = Some(FeedLink {
            feed,
            inbox: Arc::default(),
            clock: BarClock::new(timeframe),
            extended_hours: self.extended_hours,
            overnight_hours: self.overnight_hours,
            conversion: self.conversion.clone(),
            initial_bars,
            adjusted: false,
            wake: Arc::default(),
            reload_pending: true,
            resumed: false,
            resumed_from: None,
            pushed_from: None,
            non_trade_bars: false,
            paused: false,
            host_paused: false,
            next_key: 0,
            pending: Vec::new(),
            retries: Vec::new(),
            subscription: None,
            instrument: None,
            capabilities: None,
            trades: Trades::new(TRADE_CAPACITY),
            bubbles: None,
            footprint: None,
            footprint_input: None,
            replay_footprint: None,
            order_book: None,
            order_queue: None,
            depth_history: None,
            depth_retention: None,
            depth_complete: false,
            footprints_complete: false,
            footprints_stopped: false,
            footprint_unavailable: Vec::new(),
            bar_cursor: None,
            trade_cursor: None,
            initial_trades: InitialTrades::Waiting,
            depth_cursor: None,
            trades_complete: false,
            heatmap: None,
            heatmap_config: Default::default(),
            book_view: Default::default(),
            book_visible: None,
            bubbles_config: Default::default(),
            study_inputs: Vec::new(),
            time: None,
            paging_stopped: false,
        });
    }

    /// What the feed offers as a whole, once it has answered [`Datafeed::capabilities`]. Pass it
    /// to the interval and symbol selectors before a symbol resolves. `None` without a feed or
    /// before the answer.
    pub fn feed_capabilities(&self) -> Option<&FeedCapabilities> {
        self.feed_link.as_ref()?.capabilities.as_ref()
    }

    /// Suspends feed requests and unsubscribes while the host supplies replay bars by hand.
    /// Resuming with the same symbol, interval, hours and conversion keeps the bars, executions
    /// and depth history the chart holds and loads what the pause missed. The latest page and the
    /// new subscription rebuild the last bar and add the newer ones, executions a feed replays on
    /// resubscribing are dropped for the bars kept, and so are the bars it pushes again for them
    /// until the chart holds a newer one. A latest page that does not reach the kept bars replaces
    /// them. After any other change, on trade-based intervals and when the pause came before the
    /// first page arrived, resuming reloads the current symbol and interval. Bars supplied while
    /// paused are matched by exact opening time.
    /// The feed also stays paused while the main series is removed, and follows this setting
    /// again once undo restores the series. On a chart without the original pane it keeps
    /// following this setting, because the removed series' bars still supply the studies.
    pub fn set_feed_paused(&mut self, paused: bool) {
        if let Some(link) = &mut self.feed_link {
            link.host_paused = paused;
        }
        let paused = paused || !self.main_bars_live() || self.replay.is_some();
        if paused {
            self.study_data.pause();
        }
        if let Some(link) = &mut self.feed_link
            && link.paused != paused
        {
            link.paused = paused;
            if paused {
                // The bars of a load the pause interrupted are incomplete: resuming reloads them.
                let first_page = |request: &Request| {
                    matches!(
                        request,
                        Request::History {
                            scroll_back: false,
                            ..
                        }
                    )
                };
                link.reload_pending |= link.pending.iter().any(|(_, r, _)| first_page(r))
                    || link.retries.iter().any(|(_, r, _)| first_page(r));
                if let Some(id) = link.subscription.take() {
                    link.inbox.unsubscribe(id);
                    link.feed.unsubscribe(id);
                }
                link.pending.clear();
            } else {
                link.resumed = true;
                self.study_data.clear_feed();
            }
        }
    }

    /// Applies the host's pause again after the main series or the original pane came or went.
    pub(super) fn follow_feed_pause(&mut self) {
        let paused = self.feed_link.as_ref().is_some_and(|link| link.host_paused);
        self.set_feed_paused(paused);
    }

    /// Shows `symbol` from the next [`show`](Self::show). With a feed, a changed symbol clears
    /// the series and its studies, then resolves, loads and subscribes it, in one load per
    /// `show` however many setters ran; an unchanged one does nothing, also after an error.
    /// Drawings and markers are retained separately for each symbol. A changed symbol clears
    /// zoom history and returns its price scale to autoscaling. It also clears chart undo
    /// history, unless the host makes it inside [`Self::user_action`]: then the navigation is
    /// an undo step that restores the symbol, interval and viewport, and the steps before it
    /// stay with their own symbol. Without a feed replace the bars with
    /// [`set_data`](Self::set_data).
    pub fn set_symbol(&mut self, symbol: impl Into<String>) {
        let keep_history = self.history.recording();
        self.change_symbol(symbol.into(), keep_history);
    }

    /// Shows `symbol` from the next frame, keeping chart undo history when asked to.
    pub(super) fn change_symbol(&mut self, symbol: String, keep_history: bool) {
        if symbol != self.symbol {
            self.clear_source_signals();
            self.reload_price_targets();
            self.set_replay_time(None);
        }
        if symbol != self.symbol
            && let Some(link) = &mut self.feed_link
        {
            link.reload_pending = true;
            link.instrument = None;
        }
        if symbol != self.symbol {
            #[cfg(feature = "drawings")]
            self.anchor_drawing_points();
            #[cfg(feature = "drawings")]
            {
                self.save_stored_drawings();
                self.drawings.cancel_curr_drawing();
                self.drawings.end_drag_handle();
                self.drawings.deselect();
                self.detach_pending_drawings();
                let mut next = self.symbol_drawings.remove(&symbol).unwrap_or_default();
                next.0.set_sync_enabled(self.drawings.sync_enabled);
                next.0.registry = self.drawings.registry.clone();
                next.0.options = self.drawings.options.clone();
                next.0.tool_defaults = self.drawings.tool_defaults.clone();
                next.0.custom_tool_defaults = self.drawings.custom_tool_defaults.clone();
                next.0.set_active_tool(self.drawings.active_tool);
                if let Some(id) = self.drawings.active_custom_tool() {
                    let _ = next.0.set_custom_tool(id);
                }
                next.0.stay_in_drawing_mode = self.drawings.stay_in_drawing_mode;
                next.0.eraser_mode = self.drawings.eraser_mode;
                next.0.set_hidden(self.drawings.is_hidden());
                next.0.shared_filter = self.drawings.shared_filter;
                next.0.set_locked(self.drawings.is_locked());
                next.0.curr_timeframe = self.drawings.curr_timeframe.clone();
                self.flush_change_events();
                next.0.events.clear();
                next.0.events_dropped = 0;
                std::mem::swap(&mut self.drawings, &mut next.0);
                std::mem::swap(&mut self.marks, &mut next.1);
                self.attach_pending_drawings();
                // A pane remembered for a study removed while shared drawings were hidden
                // places those drawings on charts that mirror this one when they return.
                if !next.0.drawings.is_empty()
                    || !next.0.removed_panes.is_empty()
                    || !next.1.is_empty()
                    || !next
                        .0
                        .pending_sync
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .is_empty()
                {
                    self.symbol_drawings.insert(self.symbol.clone(), next);
                }
            }
            #[cfg(not(feature = "drawings"))]
            {
                let mut next_marks = self.symbol_markers.remove(&symbol).unwrap_or_default();
                std::mem::swap(&mut self.marks, &mut next_marks);
                if !next_marks.is_empty() {
                    self.symbol_markers.insert(self.symbol.clone(), next_marks);
                }
            }
            self.push_event(super::ChartEvent::SymbolChanged(symbol.clone()));
            if !keep_history {
                self.history.clear();
            }
            self.history.edits += 1;
            self.state.price_scale_mut().reset();
            self.state.clear_zoom_history();
            self.pending_scroll_time = None;
            self.legend_instrument = None;
            self.currency_choice = None;
            self.unit_choice = None;
            self.bar_close_time = None;
            self.server_time_offset = None;
            self.legend_badge = None;
            self.legend_statuses.clear();
            self.quote = None;
            self.quote_action = None;
            self.session_quote = None;
            self.events = Default::default();
            self.time_scale_events = Default::default();
            self.event_click = None;
            self.axis_hover = None;
            self.state.price_scale_mut().set_instrument_precision(None);
            self.state.price_scale_mut().set_instrument_ticks(None);
        }
        self.symbol = symbol;
    }

    /// Drops the cached drawings, markers and storage reader of an inactive symbol.
    /// Returns `false` for the current symbol or while local drawing edits await a storage
    /// read or write acknowledgement. Otherwise returns `true`, also for an uncached symbol.
    /// Stored documents are left in host storage and read again on the next visit. Without
    /// storage, forgotten drawings and markers are lost, including pending sync notifications;
    /// with it, so are the drawings kept on the chart or in the layout, which storage does not
    /// hold.
    pub fn forget_symbol(&mut self, symbol: &str) -> bool {
        if symbol == self.symbol {
            return false;
        }
        #[cfg(feature = "drawings")]
        {
            if let Some(store) = &mut self.drawing_store
                && !store.forget(symbol)
            {
                return false;
            }
            if self.symbol_drawings.remove(symbol).is_some() {
                self.drawing_resets += 1;
            }
            self.symbol_drawings.shrink_to_fit();
        }
        #[cfg(not(feature = "drawings"))]
        {
            self.symbol_markers.remove(symbol);
            self.symbol_markers.shrink_to_fit();
        }
        true
    }

    /// Shows bars of `timeframe` from the next [`show`](Self::show), by the rules of
    /// [`set_symbol`](Self::set_symbol). Without a feed it sets the legend label and the
    /// session breaks only. It is also the interval the drawings' per-interval visibility is
    /// checked against.
    pub fn set_timeframe(&mut self, timeframe: Timeframe) {
        if self.timeframe != Some(timeframe) {
            self.clear_source_signals();
            self.push_event(super::ChartEvent::TimeframeChanged(timeframe));
            self.bar_close_time = None;
        }
        if self.timeframe != Some(timeframe)
            && let Some(link) = &mut self.feed_link
        {
            link.reload_pending = true;
        }
        let changed = self.timeframe != Some(timeframe);
        if changed {
            self.pending_scroll_time = None;
        }
        self.timeframe = Some(timeframe);
        for study in self.studies.indicators_mut() {
            study.set_timeframe(timeframe);
        }
        #[cfg(feature = "drawings")]
        {
            self.drawings.curr_timeframe = timeframe.as_str().into_owned();
        }
        if changed {
            self.pending_from.fill(Some(0));
            self.next_refresh.fill(f64::NEG_INFINITY);
            self.element_studies.dirty = true;
            self.apply_replay_window();
        }
    }

    /// Requests regular hours (`false`, the default) or regular and extended hours (`true`).
    /// Turning extended hours off also turns [overnight hours](Self::set_overnight_hours) off.
    /// A changed choice reloads history and replaces the live subscription on the next frame.
    /// Without a feed the host supplies the selected bars with `set_data`.
    pub fn set_extended_hours(&mut self, extended_hours: bool) {
        self.set_hours(extended_hours, extended_hours && self.overnight_hours);
    }

    /// Requests all hours, including the instrument's overnight windows (`true`), or leaves
    /// overnight hours out (`false`, the default). Turning this on also turns
    /// [extended hours](Self::set_extended_hours) on. A changed choice reloads history and
    /// replaces the live subscription on the next frame. Without a feed the host supplies the
    /// selected bars with `set_data`.
    pub fn set_overnight_hours(&mut self, overnight_hours: bool) {
        self.set_hours(self.extended_hours || overnight_hours, overnight_hours);
    }

    fn set_hours(&mut self, extended_hours: bool, overnight_hours: bool) {
        if (self.extended_hours, self.overnight_hours) != (extended_hours, overnight_hours) {
            self.extended_hours = extended_hours;
            self.overnight_hours = overnight_hours;
            self.study_data.clear_feed();
            if let Some(link) = &mut self.feed_link {
                link.reload_pending = true;
            }
        }
    }

    /// Asks the feed for data in a currency or unit the host chose, such as `"EUR"` or `"oz"`,
    /// or in the instrument's own with `None`. The library converts nothing: the feed does, and
    /// labels the resolved instrument's currency and unit, which the legend and scale headers
    /// show. A changed choice drops the bars, trades, depth and analyst targets held, ignores the
    /// replies to the previous choice and reloads, resolving through
    /// [`Datafeed::resolve_converted`], asking for targets through
    /// [`Datafeed::price_targets_converted`] and carrying the choice in every history, trade,
    /// footprint and depth request and subscription, including those of studies on other
    /// symbols. A feed that cannot convert answers [`FeedError::NotSupported`] and the chart shows
    /// no data. A feed attached later uses it.
    pub fn set_conversion(&mut self, conversion: Option<String>) {
        if self.conversion != conversion {
            self.conversion = conversion;
            self.study_data.clear_feed();
            self.reload_price_targets();
            if let Some(link) = &mut self.feed_link {
                link.reload_pending = true;
                // The headers must not keep the previous choice's currency while loading.
                link.instrument = None;
                self.readout_revision = crate::model::next_revision();
            }
        }
    }

    /// The currency or unit chosen with [`set_conversion`](Self::set_conversion).
    pub fn conversion(&self) -> Option<&str> {
        self.conversion.as_deref()
    }

    /// Requests history adjusted for dividends (`true`) or as traded (`false`, the default), when
    /// the feed supports it; comparisons are requested the same way. A changed choice reloads
    /// history and replaces the live subscription on the next frame. Without a feed the host
    /// supplies the selected bars with `set_data`.
    pub fn set_dividend_adjusted(&mut self, adjusted: bool) {
        if let Some(link) = &mut self.feed_link
            && link.adjusted != adjusted
        {
            link.adjusted = adjusted;
            // Adjusted rows belong to another context even when the symbol and clock match.
            link.footprint = None;
            link.reload_pending = true;
            self.study_data.clear_feed();
        }
    }

    /// Whether history is requested adjusted for dividends.
    pub fn dividend_adjusted(&self) -> bool {
        self.feed_link.as_ref().is_some_and(|link| link.adjusted)
    }

    /// Reloads the current feed key on the next frame, retaining drawings and chart history.
    /// Use after changing host-side feed settings or to retry a permanent request failure.
    /// A feed asks for the same reload of the chart's series through
    /// [`Sink::reload_history`](crate::data::Sink::reload_history).
    pub fn reload_feed(&mut self) {
        self.study_data.clear_feed();
        if let Some(link) = &mut self.feed_link {
            link.reload_pending = true;
        }
    }

    /// Whether history and subscriptions request extended hours.
    pub fn extended_hours(&self) -> bool {
        self.extended_hours
    }

    /// Whether history and subscriptions request overnight hours, with extended hours.
    pub fn overnight_hours(&self) -> bool {
        self.overnight_hours
    }

    /// The symbol the chart shows.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// The timeframe the chart shows. A chart built with a feed always has one; `None` only for
    /// a chart made with [`Chart::new`] until [`set_timeframe`](Self::set_timeframe) is called.
    pub fn timeframe(&self) -> Option<Timeframe> {
        self.timeframe
    }

    /// The feed's description of the symbol, or metadata supplied with `set_instrument`.
    pub fn instrument(&self) -> Option<&Instrument> {
        self.feed_link
            .as_ref()
            .and_then(|link| link.instrument.as_ref())
            .or(self.legend_instrument.as_ref())
    }

    /// Defers the first historical trade page on a time chart until something reads executions.
    /// Set this before showing a sealed recording whose candles and prepared footprint pages
    /// cover the selected history. By default the chart asks on subscription so callers of
    /// [`Self::trades`] need not declare interest. Changing the policy does not discard trades
    /// or cancel an outstanding page. It survives [`Self::set_feed`].
    ///
    /// Before drawing the order ticket's DOM tab, call [`Self::request_trade_history`]:
    /// `TradingPanel::show_ticket` borrows the chart immutably and cannot request the trades
    /// for its ladder profile.
    ///
    /// Live subscriptions, trade-based intervals and market replay still work as before.
    /// Candle replay uses the trades already held when the host pauses the feed: if its partial
    /// bars need executions, request the tape and let it arrive before entering replay.
    pub fn set_trade_history_on_demand(&mut self, on_demand: bool) {
        self.trade_history_on_demand = on_demand;
    }

    /// Signals that a host reader (for example Time & Sales) needs this binding's historical
    /// trades. May be called each frame, including before the instrument resolves. The next
    /// feed pump with an active subscription sends the usual latest page once, also when a
    /// study needs it in the same frame. Does nothing without a feed.
    pub fn request_trade_history(&mut self) {
        if let Some(link) = &mut self.feed_link
            && link.initial_trades == InitialTrades::Waiting
        {
            link.initial_trades = InitialTrades::Wanted;
        }
    }

    /// The trades of `series` the feed sent, oldest first, at most 100,000. Empty for any id but
    /// [`SeriesId::MAIN`] and without a feed.
    pub fn trades(&self, series: SeriesId) -> impl Iterator<Item = &Tick> {
        let link = self.feed_link.as_ref().filter(|_| series == SeriesId::MAIN);
        link.into_iter().flat_map(|link| link.trades.iter())
    }

    /// Identifies the retained tape behind [`Self::trades`], or `None` without a feed or for
    /// another series than [`SeriesId::MAIN`]. Compare for equality: replacing the tape,
    /// including on a feed reset, changes the value; adding or evicting trades does not.
    pub fn trade_generation(&self, series: SeriesId) -> Option<u64> {
        self.feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN)
            .map(|link| link.trades.generation)
    }

    /// The last history page revision and the index in [`Self::trades`] where newly streamed
    /// executions begin. If the revision changed, refresh older rows without alerting on them.
    /// Start `live_cursor` at zero for each [`Self::trade_generation`]; this call advances it
    /// through the current stream, even when history and live executions arrived together.
    /// Returns `None` without a feed or for a series other than [`SeriesId::MAIN`].
    pub fn trade_progress(
        &self,
        series: SeriesId,
        live_cursor: &mut usize,
    ) -> Option<(u64, usize)> {
        self.feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN)
            .map(|link| link.trades.progress(live_cursor))
    }

    /// Combines the retained fills with explicit aggressor ids, in first-fill order. At most
    /// 100,000 entries; fills without an id stay separate. Rebuilds the view on request, including
    /// fills arriving in separate pages or frames. Per-price consumers keep using `trades`.
    pub fn combined_trades(&self, series: SeriesId) -> Vec<crate::data::CombinedTrade> {
        crate::data::combine_trades(self.trades(series))
    }

    /// Draws the trades the feed sends as bubbles, grouped and coloured as the config says,
    /// from the next frame, or stops with `None`. By default, at most the 20 largest visible
    /// trades that stand out draw, one bubble each. A chart without a feed holds no trades and
    /// draws none, and Renko, Kagi, line break, point and figure and range charts draw none, since
    /// their x axis is not time. Sums every held trade again: call it when the settings change,
    /// not every frame.
    pub fn set_volume_bubbles(&mut self, config: Option<VolumeBubblesConfig>) {
        if let Some(link) = &mut self.feed_link {
            if let Some(config) = &config {
                link.bubbles_config = config.clone();
            }
            link.bubbles = config.map(|config| link.bubbles(config));
        }
    }

    /// The order book of `series` the feed sent, if it holds one. Unavailable during candle-only
    /// replay; attach a market replay driver for recorded book state.
    pub fn order_book(&self, series: SeriesId) -> Option<&OrderBook> {
        let link = self
            .feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN && self.replay.is_none())?;
        link.order_book.as_ref()
    }

    /// Individual resting orders and their queue priority, when the feed sends order-level data.
    /// Use `OrderQueue::aggregate` with a minimum size for filtered ladders or heatmaps, and
    /// `largest_order` for the largest resting order at a level. Aggregated-only feeds return
    /// `None`; a lost order book has no orders until a replacement snapshot arrives. Candle-only
    /// replay hides the live queue.
    pub fn order_queue(&self, series: SeriesId) -> Option<&crate::data::OrderQueue> {
        self.feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN && self.replay.is_none())?
            .order_queue
            .as_ref()
    }

    /// How the heatmap's profile and the depth ladder show the live book: which individual orders
    /// count, what a bar measures and which orders are highlighted. Its size tiers also mark the
    /// heatmap's cells whose largest recorded order reaches one; the orders the heatmap records are
    /// those of [`DepthHistoryConfig::orders`](crate::data::DepthHistoryConfig::orders). Applies
    /// from the next frame. The profile key names its measure and maximum across all supplied
    /// levels, including offscreen prices, after filtering; hovering the strip shows the row's
    /// exact price, side and size. Set [`OrderBookView::quantity_unit`](crate::chart::overlays::OrderBookView::quantity_unit)
    /// to describe the feed's quantities; without it the key says "feed units".
    /// A view that needs individual orders on an aggregate feed draws an explicit notice in the
    /// pane and an empty ladder, rather than aggregate sizes.
    pub fn set_order_book_view(&mut self, view: crate::chart::overlays::OrderBookView) {
        if let Some(link) = &mut self.feed_link {
            link.book_view = view;
        }
    }

    /// Show the live book profile at the right edge of the pane, independently of the heatmap.
    /// Until explicitly set, the profile follows the heatmap for existing setter callers.
    /// Requires a feed and the instrument's tick size; does nothing without a feed.
    pub fn set_order_book_visible(&mut self, visible: bool) {
        if let Some(link) = &mut self.feed_link {
            link.book_visible = Some(visible);
        }
    }

    /// The view set by [`set_order_book_view`](Self::set_order_book_view), the default of the
    /// aggregate book without a feed.
    pub fn order_book_view(&self) -> crate::chart::overlays::OrderBookView {
        self.feed_link
            .as_ref()
            .map(|link| link.book_view.clone())
            .unwrap_or_default()
    }

    /// Loads recorded depth from the feed and keeps `history` of its live book, or stops
    /// with `None`. The history starts empty and is cleared when the symbol or interval changes.
    /// A history without its own [`retention`](crate::data::DepthHistoryConfig::retention)
    /// takes the chart's [`set_depth_retention`](Self::set_depth_retention); one with its own
    /// replaces that setting. It needs the instrument's tick size, and keeps no columns without
    /// one or without a feed.
    /// Columns follow the times of the depth updates: an update stamped more than a minute after
    /// the feed's time ([`Sink::set_server_time`](crate::data::Sink::set_server_time)) samples
    /// nothing, so a wrong time cannot hold the history back.
    /// The chart takes the columns sampled since its last frame when it is drawn: while it is not,
    /// at most [`columns`](crate::data::DepthHistoryConfig::columns) of them wait, or 600 under a
    /// retention, the oldest making room for new ones.
    pub fn set_depth_history(&mut self, mut history: Option<DepthHistory>) {
        if let Some(link) = &mut self.feed_link {
            if let Some(history) = &mut history {
                if history.config().retention.is_none() && link.depth_retention.is_some() {
                    history.set_retention(link.depth_retention);
                }
                link.depth_retention = history.config().retention;
            }
            if let Some(id) = link.subscription {
                let config = history.as_ref().map(|history| history.config().clone());
                link.inbox.sample_depth(id, config);
            }
            link.depth_history = history;
            link.depth_complete = false;
            link.depth_cursor = None;
            link.pending
                .retain(|(_, r, _)| !matches!(r, Request::Depth(_)));
            link.retries
                .retain(|(_, r, _)| !matches!(r, Request::Depth(_)));
        }
    }

    /// Keeps the depth history for `retention`, choosing tier capacity with
    /// [`DepthHistoryConfig::with_retention`](crate::data::DepthHistoryConfig::with_retention).
    /// Columns outside it leave immediately, and paging stops at its start. `None` restores
    /// the full default history of 600 columns per tier. Also applies to a history started later
    /// by [`set_liquidity_heatmap`](Self::set_liquidity_heatmap); a history supplied with its own
    /// retention replaces the setting. A non-positive duration acts as `None`. Does nothing
    /// without a feed.
    pub fn set_depth_retention(&mut self, retention: Option<chrono::TimeDelta>) {
        let retention = retention.filter(|r| *r > chrono::TimeDelta::zero());
        let Some(link) = &mut self.feed_link else {
            return;
        };
        link.depth_retention = retention;
        if let Some(mut history) = link
            .depth_history
            .take_if(|h| h.config().retention != retention)
        {
            history.set_retention(retention);
            self.set_depth_history(Some(history));
        }
    }

    /// Empties held depth columns and heatmap textures, keeping their settings, as the Order
    /// flow tab's Release memory button requests. Live depth and historical pages within the
    /// retention fill the history again. Does nothing without a feed or a depth history.
    pub fn clear_depth_history(&mut self) {
        let Some(link) = &mut self.feed_link else {
            return;
        };
        let Some(history) = &mut link.depth_history else {
            return;
        };
        history.clear();
        link.heatmap = link.heatmap.take().map(|_| Default::default());
        link.depth_complete = false;
        link.depth_cursor = None;
        link.pending
            .retain(|(_, r, _)| !matches!(r, Request::Depth(_)));
        link.retries
            .retain(|(_, r, _)| !matches!(r, Request::Depth(_)));
    }

    /// Shows resting liquidity behind the price series. A feed and instrument tick size are
    /// required. Enabling starts the default depth history with the retention of
    /// [`set_depth_retention`](Self::set_depth_retention) if none is configured; disabling
    /// releases textures and retains that history and the appearance settings. Re-enabling an
    /// enabled heatmap keeps its textures. Trade bubbles and best bid/ask lines can be
    /// enabled independently through the existing chart options.
    pub fn set_liquidity_heatmap(&mut self, enabled: bool) {
        if enabled
            && self
                .feed_link
                .as_ref()
                .is_none_or(|link| link.depth_history.is_none())
        {
            self.set_depth_history(Some(
                DepthHistory::new(Default::default()).expect("default depth history"),
            ));
        }
        if let Some(link) = &mut self.feed_link {
            if enabled {
                link.heatmap.get_or_insert_with(Default::default);
            } else {
                link.heatmap = None;
            }
        }
    }

    /// Sets the heatmap's cell brightness and resting-size range from the next frame. A changed
    /// appearance redraws the held columns once; an unchanged effective setting uploads nothing.
    /// Retained while disabled and in exported images. Does nothing without a feed; enable the
    /// heatmap separately with [`set_liquidity_heatmap`](Self::set_liquidity_heatmap).
    pub fn set_liquidity_heatmap_config(
        &mut self,
        config: crate::chart::overlays::LiquidityHeatmapConfig,
    ) {
        if let Some(link) = &mut self.feed_link {
            link.heatmap_config = config;
        }
    }

    /// The heatmap appearance supplied by the host, or its default without a feed.
    pub fn liquidity_heatmap_config(&self) -> crate::chart::overlays::LiquidityHeatmapConfig {
        self.feed_link
            .as_ref()
            .map(|link| link.heatmap_config)
            .unwrap_or_default()
    }

    pub(super) fn paint_liquidity(
        &self,
        painter: &egui::Painter,
        mapping: &crate::chart::coords::ChartMapping,
    ) {
        if self.replay.is_some() {
            return;
        }
        let Some(link) = &self.feed_link else { return };
        let Some(tick) = self.instrument().and_then(|i| i.tick_size) else {
            return;
        };
        // Why the heatmap's history and the book profile cannot filter, each reason said once.
        let mut unavailable = Vec::new();
        if let (Some(heatmap), Some(history)) = (&link.heatmap, &link.depth_history) {
            let mut heatmap = heatmap.borrow_mut();
            heatmap.config = link.heatmap_config;
            heatmap.background = Some(self.config.background_color);
            heatmap.render(
                painter,
                mapping,
                history,
                tick * history.config().ticks_per_row as f64,
                |time| self.time_to_index(time).and_then(|i| self.index_to_x(i)),
                &link.book_view,
                [self.config.bullish_color, self.config.bearish_color],
            );
            if let Some(filter) = &history.config().orders {
                let applies = link.order_queue.as_ref().map_or(
                    Err(crate::data::OrderFilterUnavailable::AggregateDepth),
                    |queue| queue.check_filter(filter),
                );
                unavailable.extend(applies.err());
            }
        }
        if link.book_visible.unwrap_or(link.heatmap.is_some())
            && let Err(reason) = crate::chart::overlays::render_book_profile(
                painter,
                mapping,
                link.order_book.as_ref(),
                link.order_queue.as_ref(),
                tick,
                &link.book_view,
                [self.config.bullish_color, self.config.bearish_color],
            )
            && !unavailable.contains(&reason)
        {
            unavailable.push(reason);
        }
        // One line per reason, the heatmap's first, each below the last.
        let mut top = mapping.rect.top() + 8.0;
        for reason in unavailable {
            top = painter
                .text(
                    egui::pos2(mapping.rect.right() - 8.0, top),
                    egui::Align2::RIGHT_TOP,
                    crate::localization::format_text(
                        painter.ctx(),
                        "Order-level depth unavailable: {0}",
                        &[(
                            "0",
                            crate::localization::text(painter.ctx(), reason.to_string()),
                        )],
                    ),
                    egui::FontId::proportional(11.0),
                    painter.ctx().active_theme().ui.warning,
                )
                .bottom();
        }
    }

    /// The order book history of `series`, if [`set_depth_history`](Self::set_depth_history)
    /// turned it on. Candle-only replay hides the live history.
    pub fn depth_history(&self, series: SeriesId) -> Option<&DepthHistory> {
        let link = self
            .feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN && self.replay.is_none())?;
        link.depth_history.as_ref()
    }

    /// Calendar boundaries selected by the chart, including the resolved exchange or local zone.
    /// Its days are those that date the bars: bars of a day or longer given without a feed run
    /// on UTC days under a display zone behind UTC unless they sit at its midnight, as
    /// [`format_time`](Self::format_time) reads them.
    pub fn bar_clock(&self) -> BarClock {
        let clock = BarClock::new(self.timeframe.unwrap_or_default());
        let clock = match self.calendar_timezone() {
            Some(zone) => clock.with_timezone(zone),
            None => clock.with_local_timezone(),
        };
        match self.instrument().and_then(|instrument| {
            Some((
                instrument
                    .time_zone
                    .clone()
                    .unwrap_or_else(crate::model::TimezoneId::utc),
                instrument.session.clone()?,
            ))
        }) {
            Some((zone, session)) => clock.with_session_hours(
                zone,
                session,
                self.extended_hours(),
                self.overnight_hours(),
            ),
            None => clock,
        }
    }

    /// The clock bars merge by: the feed's, or exact times without a feed.
    pub(super) fn clock(&self) -> BarClock {
        self.feed_link
            .as_ref()
            .filter(|link| !link.paused)
            .map_or_else(BarClock::exact, |link| link.clock.clone())
    }
}

/// Describes `symbol` as the feed does for `conversion`, or as it is without one.
pub(super) fn resolve(
    feed: &dyn Datafeed,
    symbol: &str,
    conversion: Option<&str>,
    reply: Reply<Instrument>,
) {
    match conversion {
        Some(conversion) => feed.resolve_converted(symbol, conversion, reply),
        None => feed.resolve(symbol, reply),
    }
}

/// How long to wait before retrying a request that has failed `failures` times: doubling from a
/// second up to a minute, and no sooner than the feed asked.
pub(super) fn retry_delay(failures: u32, error: &FeedError) -> Duration {
    let backoff = Duration::from_secs(1 << failures.min(6)).min(Duration::from_secs(60));
    match error {
        FeedError::RateLimited {
            retry_after: Some(after),
        } => backoff.max(*after),
        _ => backoff,
    }
}

/// `wake`, also after a panic while it was held.
fn lock(wake: &Mutex<Option<ViewportId>>) -> std::sync::MutexGuard<'_, Option<ViewportId>> {
    wake.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Merges ordered trades by time, keeping `a` before `b` at equal times, as a stable sort would.
fn by_time<'a>(
    a: impl Iterator<Item = &'a Tick>,
    b: impl Iterator<Item = &'a Tick>,
) -> impl Iterator<Item = &'a Tick> {
    let (mut a, mut b) = (a.peekable(), b.peekable());
    std::iter::from_fn(move || match (a.peek(), b.peek()) {
        (Some(x), Some(y)) if y.ts < x.ts => b.next(),
        (Some(_), _) => a.next(),
        (None, _) => b.next(),
    })
}

#[cfg(test)]
mod tests;
