//! A chart driven by a [`Datafeed`]: requests issued from `show`, answers and live pushes applied
//! once per frame.
//!
//! Every feed call but the one in [`FeedLink`]'s `Drop` comes from [`Chart::pump`], which runs
//! outside any egui closure: a feed may answer or push inline, and that runs the waker, which
//! takes the egui context's lock.

mod pump;

use super::{Chart, SeriesId};
use crate::chart::overlays::{VolumeBubbles, VolumeBubblesConfig};
use crate::data::inbox::Inbox;
use crate::data::trades::Trades;
use crate::data::{
    Datafeed, DepthHistory, DepthRequest, FeedCapabilities, FeedError, Footprint, HistoryRequest,
    Reply, SubscriptionId, TradeRequest,
};
use crate::model::{BarClock, Instrument, OrderBook, Tick, Timeframe};
use crate::theme::ThemeExt;
use egui::ViewportId;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Trades kept per series for order flow.
pub(crate) const TRADE_CAPACITY: usize = 100_000;
/// Bars a feed-backed footprint keeps, the oldest dropped first.
const FOOTPRINT_BARS: usize = 5_000;
/// Price rows a feed-backed footprint keeps in all its bars, the oldest bars dropped first.
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
    initial_bars: usize,
    /// The viewport the chart was last shown in, which the waker wakes. Set by the first `pump`,
    /// which installs the waker, and cleared by `Drop` before it unsubscribes, so the waker does
    /// nothing then.
    wake: Arc<Mutex<Option<ViewportId>>>,
    /// The symbol or timeframe changed: the next `pump` reloads.
    reload_pending: bool,
    pub(super) paused: bool,
    /// The pause last set with [`Chart::set_feed_paused`]. `paused` also holds while the main
    /// series is removed or a replay runs.
    pub(super) host_paused: bool,
    next_key: u64,
    /// Requests not answered yet; an answer under any other key is stale and dropped.
    pending: Vec<(u64, Request, u32)>,
    retries: Vec<(f64, Request, u32)>,
    subscription: Option<SubscriptionId>,
    instrument: Option<Instrument>,
    /// What the feed offers as a whole, once it has answered.
    capabilities: Option<FeedCapabilities>,
    pub(super) trades: Trades,
    /// The held trades as volume bubbles, when the host turned them on.
    pub(super) bubbles: Option<VolumeBubbles>,
    /// The trades as a footprint on the instrument's tick, while the chart type or a volume
    /// profile drawing reads them, within [`FOOTPRINT_BARS`] and [`FOOTPRINT_ROWS`].
    pub(super) footprint: Option<Footprint>,
    /// The footprint of the trades before the replay cutoff, while a replay runs under a visible
    /// volume profile drawing, rebuilt when the cutoff moves.
    pub(super) replay_footprint: Option<(chrono::DateTime<chrono::Utc>, Option<Footprint>)>,
    order_book: Option<OrderBook>,
    order_queue: Option<crate::data::OrderQueue>,
    /// The order book over time, when the host turned it on.
    pub(super) depth_history: Option<DepthHistory>,
    depth_complete: bool,
    /// Where older history continues after an empty page, by source: bars, trades, depth. The
    /// bar cursor keeps the oldest bar held when the feed named it and lapses once that bar goes.
    bar_cursor: Option<(
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    )>,
    trade_cursor: Option<chrono::DateTime<chrono::Utc>>,
    depth_cursor: Option<chrono::DateTime<chrono::Utc>>,
    /// No older trades remain to be loaded.
    trades_complete: bool,
    /// A trade page reported that nothing exists before its oldest trade, or came back empty
    /// without `continue_before`: no trade before the oldest held one was missed.
    trades_from_start: bool,
    pub(super) heatmap: Option<std::cell::RefCell<crate::chart::overlays::LiquidityHeatmap>>,
    heatmap_config: crate::chart::overlays::LiquidityHeatmapConfig,
    /// How the live book is shown by the heatmap's profile and the depth ladder.
    book_view: crate::chart::overlays::OrderBookView,
    study_inputs: Vec<(crate::data::OrderFlowInput, u64)>,
    time: Option<chrono::DateTime<chrono::Utc>>,
    /// A permanent history or resolve failure stops paging until the next reload.
    pub(super) paging_stopped: bool,
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
    fn rebuild_bubbles(&mut self) {
        if let Some(bubbles) = self.bubbles.take() {
            self.bubbles = Some(self.bubbles(bubbles.config));
        }
        if self.footprint.is_some() {
            self.footprint = self.footprint(None);
        }
    }

    /// A paused link for an offscreen image: it holds the visuals the feed built, the footprint,
    /// bubbles, order book and depth, and never talks to the feed or the live link's inbox.
    pub(super) fn frozen(&self) -> Self {
        Self {
            feed: Arc::clone(&self.feed),
            inbox: Arc::default(),
            clock: self.clock.clone(),
            initial_bars: self.initial_bars,
            wake: Arc::default(),
            reload_pending: false,
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
            footprint: self
                .footprint
                .is_some()
                .then(|| self.footprint(None))
                .flatten(),
            replay_footprint: self.replay_footprint.clone(),
            order_book: self.order_book.clone(),
            order_queue: self.order_queue.clone(),
            depth_history: self.depth_history.clone(),
            depth_complete: self.depth_complete,
            bar_cursor: None,
            trade_cursor: None,
            depth_cursor: None,
            trades_complete: false,
            trades_from_start: false,
            heatmap: self.heatmap.is_some().then(Default::default),
            heatmap_config: self.heatmap_config,
            book_view: self.book_view.clone(),
            study_inputs: Vec::new(),
            time: self.time,
            paging_stopped: true,
        }
    }

    /// The held trades as a footprint, or `None` without the instrument's tick size. A trade
    /// older than the one before it is left out, as it is when it arrives live. The bar of the
    /// oldest held trade is incomplete until a trade page reports the start of trade history.
    /// With `before`, includes only executions strictly before that replay cutoff.
    pub(super) fn footprint(
        &self,
        before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Option<Footprint> {
        let tick_size = self.instrument.as_ref()?.tick_size?;
        let mut flow = Footprint::new(self.clock.clone(), tick_size, 1).ok()?;
        flow.set_budget(FOOTPRINT_BARS, FOOTPRINT_ROWS);
        let trades = || {
            self.trades
                .iter()
                .take_while(|trade| before.is_none_or(|time| trade.ts < time))
        };
        for trade in trades() {
            let _ = flow.push(trade);
        }
        if let Some(lost) = self.trades.lost_until() {
            flow.drop_through(lost, trades());
        }
        if let Some(first) = self.unconfirmed_start() {
            flow.received_from(first);
        }
        Some(flow)
    }

    /// The oldest held trade, where the footprint starts, while executions before it may
    /// exist that never arrived: no trade page reported the start of trade history and the
    /// store lost none. A loss is marked exactly by `drop_through` instead.
    fn unconfirmed_start(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.trades_from_start || self.trades.lost_until().is_some() {
            return None;
        }
        self.trades.iter().next().map(|trade| trade.ts)
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
    /// Drives the chart from `feed` from the next [`show`](Self::show) on.
    pub(crate) fn attach_feed(&mut self, feed: Arc<dyn Datafeed>, initial_bars: usize) {
        self.set_timeframe(self.timeframe.unwrap_or_default());
        let timeframe = self.timeframe.unwrap_or_default();
        self.feed_link = Some(FeedLink {
            feed,
            inbox: Arc::default(),
            clock: BarClock::new(timeframe),
            initial_bars,
            wake: Arc::default(),
            reload_pending: true,
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
            replay_footprint: None,
            order_book: None,
            order_queue: None,
            depth_history: None,
            depth_complete: false,
            bar_cursor: None,
            trade_cursor: None,
            depth_cursor: None,
            trades_complete: false,
            trades_from_start: false,
            heatmap: None,
            heatmap_config: Default::default(),
            book_view: Default::default(),
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
    /// Resuming reloads the current symbol and interval, including any trades retained by the
    /// host. Bars supplied while paused are matched by exact opening time.
    /// The feed also stays paused while the main series is removed, and follows this setting
    /// again once undo restores the series.
    pub fn set_feed_paused(&mut self, paused: bool) {
        if let Some(link) = &mut self.feed_link {
            link.host_paused = paused;
        }
        let paused = paused || !self.main_series_present || self.replay.is_some();
        if paused {
            self.study_data.pause();
        }
        if let Some(link) = &mut self.feed_link
            && link.paused != paused
        {
            link.paused = paused;
            link.reload_pending = true;
            if paused {
                if let Some(id) = link.subscription.take() {
                    link.inbox.unsubscribe(id);
                    link.feed.unsubscribe(id);
                }
                link.pending.clear();
            } else {
                self.study_data.clear_feed();
            }
        }
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
            self.host_events
                .push(super::ChartEvent::SymbolChanged(symbol.clone()));
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
                next.0.set_active_tool(self.drawings.active_tool);
                if let Some(id) = self.drawings.active_custom_tool() {
                    let _ = next.0.set_custom_tool(id);
                }
                next.0.stay_in_drawing_mode = self.drawings.stay_in_drawing_mode;
                next.0.eraser_mode = self.drawings.eraser_mode;
                next.0.curr_timeframe = self.drawings.curr_timeframe.clone();
                std::mem::swap(&mut self.drawings, &mut next.0);
                std::mem::swap(&mut self.marks, &mut next.1);
                self.attach_pending_drawings();
                if !next.0.drawings.is_empty()
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
            if !keep_history {
                self.history.clear();
            }
            self.history.edits += 1;
            self.state.price_scale_mut().reset();
            self.state.clear_zoom_history();
            self.pending_scroll_time = None;
            self.legend_instrument = None;
            self.currency_choice = None;
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
            self.state.price_scale_mut().set_price_precision(None);
            self.state.price_scale_mut().set_tick_size(None);
        }
        self.symbol = symbol;
    }

    /// Drops the cached drawings, markers and storage reader of an inactive symbol.
    /// Returns `false` for the current symbol or while local drawing edits await a storage
    /// read or write acknowledgement. Otherwise returns `true`, also for an uncached symbol.
    /// Stored documents are left in host storage and read again on the next visit. Without
    /// storage, forgotten drawings and markers are lost, including pending sync notifications.
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
            self.symbol_drawings.remove(symbol);
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
            self.host_events
                .push(super::ChartEvent::TimeframeChanged(timeframe));
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
    /// A changed choice reloads history and replaces the live subscription on the next frame.
    /// Without a feed the host supplies the selected bars with `set_data`.
    pub fn set_extended_hours(&mut self, extended_hours: bool) {
        if self.extended_hours != extended_hours {
            self.extended_hours = extended_hours;
            self.study_data.clear_feed();
            if let Some(link) = &mut self.feed_link {
                link.reload_pending = true;
            }
        }
    }

    /// Asks the feed for data in a currency or unit the host chose, such as `"EUR"` or `"oz"`,
    /// or in the instrument's own with `None`. The library converts nothing: the feed does, and
    /// labels the resolved instrument's currency and unit, which the legend and scale headers
    /// show. A changed choice drops the bars, trades and depth held, ignores the replies to the
    /// previous choice and reloads, resolving through [`Datafeed::resolve_converted`] and
    /// carrying the choice in every history, trade and depth request and subscription,
    /// including those of studies on other symbols. A feed that cannot convert answers
    /// [`FeedError::NotSupported`] and the chart shows no data. A feed attached later uses it.
    pub fn set_conversion(&mut self, conversion: Option<String>) {
        if self.conversion != conversion {
            self.conversion = conversion;
            self.study_data.clear_feed();
            if let Some(link) = &mut self.feed_link {
                link.reload_pending = true;
                // The headers must not keep the previous choice's currency while loading.
                link.instrument = None;
            }
        }
    }

    /// The currency or unit chosen with [`set_conversion`](Self::set_conversion).
    pub fn conversion(&self) -> Option<&str> {
        self.conversion.as_deref()
    }

    /// Reloads the current feed key on the next frame, retaining drawings and chart history.
    /// Use after changing host-side feed settings or to retry a permanent request failure.
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

    /// The trades of `series` the feed sent, oldest first, at most 100,000. Empty for any id but
    /// [`SeriesId::MAIN`] and without a feed.
    pub fn trades(&self, series: SeriesId) -> impl Iterator<Item = &Tick> {
        let link = self.feed_link.as_ref().filter(|_| series == SeriesId::MAIN);
        link.into_iter().flat_map(|link| link.trades.iter())
    }

    /// Combines the retained fills with explicit aggressor ids, in first-fill order. At most
    /// 100,000 entries; fills without an id stay separate. Rebuilds the view on request, including
    /// fills arriving in separate pages or frames. Per-price consumers keep using `trades`.
    pub fn combined_trades(&self, series: SeriesId) -> Vec<crate::data::CombinedTrade> {
        crate::data::combine_trades(self.trades(series))
    }

    /// Draws the trades the feed sends as bubbles, grouped and coloured as the config says,
    /// from the next frame, or stops with `None`. By default, nearby bars and price levels merge
    /// when crowded, and diameters adapt to the available screen space. A chart without a feed
    /// holds no trades and draws none, and Renko, Kagi, line break, point and figure and range
    /// charts draw none, since their x axis is not time. Sums every held trade again: call it
    /// when the settings change, not every frame.
    pub fn set_volume_bubbles(&mut self, config: Option<VolumeBubblesConfig>) {
        if let Some(link) = &mut self.feed_link {
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
    /// It needs the instrument's tick size, and keeps no columns without one or without a feed.
    /// Columns follow the times of the depth updates: an update stamped more than a minute after
    /// the feed's time ([`Sink::set_server_time`](crate::data::Sink::set_server_time)) samples
    /// nothing, so a wrong time cannot hold the history back.
    /// The chart takes the columns sampled since its last frame when it is drawn: while it is not,
    /// at most [`columns`](crate::data::DepthHistoryConfig::columns) of them wait, the oldest
    /// making room for new ones.
    pub fn set_depth_history(&mut self, history: Option<DepthHistory>) {
        if let Some(link) = &mut self.feed_link {
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

    /// Shows resting liquidity behind the price series. A feed and instrument tick size are
    /// required. Enabling starts the default depth history if none is configured; disabling
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
        let (Some(heatmap), Some(history), Some(tick)) = (
            &link.heatmap,
            &link.depth_history,
            self.instrument().and_then(|i| i.tick_size),
        ) else {
            return;
        };
        let mut heatmap = heatmap.borrow_mut();
        heatmap.config = link.heatmap_config;
        heatmap.render(
            painter,
            mapping,
            history,
            tick * history.config().ticks_per_row as f64,
            |time| self.time_to_index(time).and_then(|i| self.index_to_x(i)),
            &link.book_view,
            [self.config.bullish_color, self.config.bearish_color],
        );
        let shown = crate::chart::overlays::render_book_profile(
            painter,
            mapping,
            link.order_book.as_ref(),
            link.order_queue.as_ref(),
            tick,
            &link.book_view,
            [self.config.bullish_color, self.config.bearish_color],
        );
        // A heatmap of individual orders has no columns without them either.
        let unavailable = shown.err().or_else(|| {
            (history.config().orders.is_some() && link.order_queue.is_none())
                .then_some(crate::data::OrderFilterUnavailable::AggregateDepth)
        });
        if let Some(reason) = unavailable {
            painter.text(
                mapping.rect.right_top() + egui::vec2(-8.0, 8.0),
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
            );
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
    pub fn bar_clock(&self) -> BarClock {
        let clock = BarClock::new(self.timeframe.unwrap_or_default());
        let clock = match self.display_timezone() {
            crate::scales::TimezoneMode::Local => clock.with_local_timezone(),
            zone => clock.with_timezone(zone.to_tz().unwrap_or(crate::model::TimezoneId::utc())),
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
            Some((zone, session)) => clock.with_session(zone, session, self.extended_hours()),
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

#[cfg(test)]
mod tests;
