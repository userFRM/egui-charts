//! A chart driven by a [`Datafeed`]: requests issued from `show`, answers and live pushes applied
//! once per frame.
//!
//! Every feed call but the one in [`FeedLink`]'s `Drop` comes from [`Chart::pump`], which runs
//! outside any egui closure: a feed may answer or push inline, and that runs the waker, which
//! takes the egui context's lock.

use super::{Chart, SeriesId};
use crate::chart::overlays::{VolumeBubbles, VolumeBubblesConfig};
use crate::data::inbox::{Answer, Inbox};
use crate::data::merge::Origin;
use crate::data::trades::Trades;
use crate::data::{
    Datafeed, FeedError, FeedState, HistoryPage, HistoryRequest, Reply, Subscription,
    SubscriptionId, TradePage, TradeRequest,
};
use crate::model::{BarClock, Instrument, OrderBook, Tick, Timeframe};
use egui::ViewportId;
use std::sync::{Arc, Mutex, PoisonError};

/// Trades kept per series for order flow.
pub(crate) const TRADE_CAPACITY: usize = 100_000;

/// Bars from the first visible bar to the oldest one below which an older page is asked for.
const SCROLL_BACK_MARGIN: usize = 150;

/// The smallest scroll-back page asked for.
const MIN_SCROLL_BACK_PAGE: usize = 600;

/// A chart's link to its feed.
pub(crate) struct FeedLink {
    feed: Arc<dyn Datafeed>,
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
    next_key: u64,
    /// Requests not answered yet; an answer under any other key is stale and dropped.
    pending: Vec<(u64, Request, u32)>,
    retries: Vec<(f64, Request, u32)>,
    subscription: Option<SubscriptionId>,
    instrument: Option<Instrument>,
    trades: Trades,
    /// The held trades as volume bubbles, when the host turned them on.
    pub(super) bubbles: Option<VolumeBubbles>,
    order_book: Option<OrderBook>,
    /// A permanent history or resolve failure stops paging until the next reload.
    pub(super) paging_stopped: bool,
}

#[derive(Clone)]
enum Request {
    Resolve,
    History {
        request: HistoryRequest,
        scroll_back: bool,
    },
    Trades,
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
            next_key: 0,
            pending: Vec::new(),
            retries: Vec::new(),
            subscription: None,
            instrument: None,
            trades: Trades::new(TRADE_CAPACITY),
            bubbles: None,
            order_book: None,
            paging_stopped: false,
        });
    }

    /// Suspends feed requests and unsubscribes while the host supplies replay bars by hand.
    /// Resuming reloads the current symbol and interval, including any trades retained by the
    /// host. Bars supplied while paused are matched by exact opening time.
    /// A removed main series stays paused.
    pub fn set_feed_paused(&mut self, paused: bool) {
        let paused = paused || !self.main_series_present || self.replay.is_some();
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
            }
        }
    }

    /// Shows `symbol` from the next [`show`](Self::show). With a feed, a changed symbol clears
    /// the series and its studies, then resolves, loads and subscribes it, in one load per
    /// `show` however many setters ran; an unchanged one does nothing, also after an error.
    /// Drawings and markers are retained separately for each symbol. A changed symbol clears
    /// chart undo history. Without a feed replace the bars with
    /// [`set_data`](Self::set_data).
    pub fn set_symbol(&mut self, symbol: impl Into<String>) {
        let symbol = symbol.into();
        if symbol != self.symbol {
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
            self.anchor_drawing_points();
            self.drawings.cancel_curr_drawing();
            self.drawings.end_drag_handle();
            self.drawings.deselect();
            let mut next = self.symbol_drawings.remove(&symbol).unwrap_or_default();
            next.0.options = self.drawings.options.clone();
            next.0.tool_defaults = self.drawings.tool_defaults.clone();
            next.0.active_tool = self.drawings.active_tool;
            next.0.stay_in_drawing_mode = self.drawings.stay_in_drawing_mode;
            next.0.eraser_mode = self.drawings.eraser_mode;
            next.0.curr_timeframe = self.drawings.curr_timeframe.clone();
            std::mem::swap(&mut self.drawings, &mut next.0);
            std::mem::swap(&mut self.marks, &mut next.1);
            self.symbol_drawings.insert(self.symbol.clone(), next);
            self.history = Default::default();
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
            self.events.clear();
            self.time_scale_events.clear();
            self.event_click = None;
            self.axis_hover = None;
            self.state.price_scale_mut().set_price_precision(None);
        }
        self.symbol = symbol;
    }

    /// Shows bars of `timeframe` from the next [`show`](Self::show), by the rules of
    /// [`set_symbol`](Self::set_symbol). Without a feed it sets the legend label and the
    /// session breaks only. It is also the interval the drawings' per-interval visibility is
    /// checked against.
    pub fn set_timeframe(&mut self, timeframe: Timeframe) {
        if self.timeframe != Some(timeframe) {
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
        self.drawings.curr_timeframe = timeframe.as_str().into_owned();
        if changed {
            self.apply_replay_window();
        }
    }

    /// Requests regular hours (`false`, the default) or regular and extended hours (`true`).
    /// A changed choice reloads history and replaces the live subscription on the next frame.
    /// Without a feed the host supplies the selected bars with `set_data`.
    pub fn set_extended_hours(&mut self, extended_hours: bool) {
        if self.extended_hours != extended_hours {
            self.extended_hours = extended_hours;
            if let Some(link) = &mut self.feed_link {
                link.reload_pending = true;
            }
        }
    }

    /// Reloads the current feed key on the next frame, retaining drawings and chart history.
    /// Use after changing host-side feed settings or to retry a permanent request failure.
    pub fn reload_feed(&mut self) {
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

    /// Draws the trades the feed sends as bubbles, one per bar and price level, from the next
    /// frame, or stops with `None`. A chart without a feed holds no trades and draws none, and
    /// Renko, Kagi, line break, point and figure and range charts draw none, since their x axis
    /// is not time. Sums every held trade again: call it when the settings change, not every
    /// frame.
    pub fn set_volume_bubbles(&mut self, config: Option<VolumeBubblesConfig>) {
        if let Some(link) = &mut self.feed_link {
            link.bubbles = config.map(|config| link.bubbles(config));
        }
    }

    /// The order book of `series` the feed sent, if it holds one.
    pub fn order_book(&self, series: SeriesId) -> Option<&OrderBook> {
        let link = self
            .feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN)?;
        link.order_book.as_ref()
    }

    /// Calendar boundaries selected by the chart, including the resolved exchange or local zone.
    pub fn bar_clock(&self) -> BarClock {
        let clock = BarClock::new(self.timeframe.unwrap_or_default());
        match self.display_timezone() {
            crate::scales::TimezoneMode::Local => clock.with_local_timezone(),
            zone => clock.with_timezone(zone.to_tz().unwrap_or(chrono_tz::UTC)),
        }
    }

    /// The clock bars merge by: the feed's, or exact times without a feed.
    pub(super) fn clock(&self) -> BarClock {
        self.feed_link
            .as_ref()
            .filter(|link| !link.paused)
            .map_or_else(BarClock::exact, |link| link.clock.clone())
    }

    /// Issues what the feed must be asked and applies what it sent since the last call. Called
    /// first in [`show`](Self::show); does nothing without a feed.
    pub(crate) fn pump(&mut self, ctx: &egui::Context) {
        if self.feed_link.as_ref().is_some_and(|link| link.paused) {
            return;
        }
        let clock = self.bar_clock();
        let Some(mut link) = self.feed_link.take() else {
            return;
        };
        if link.clock != clock {
            link.reload_pending = true;
        }
        if link.reload_pending {
            self.reload(&mut link, clock);
        }
        // The chart's own viewport, also after it moved to another: off the UI thread
        // `request_repaint` wakes the root one.
        if lock(&link.wake).replace(ctx.viewport_id()).is_none() {
            let (ctx, wake) = (ctx.clone(), Arc::clone(&link.wake));
            link.inbox.set_waker(move || {
                let viewport = *lock(&wake);
                if let Some(viewport) = viewport {
                    ctx.request_repaint_of(viewport);
                }
            });
        }

        let now = ctx.input(|input| input.time);
        let drained = link.inbox.drain();
        for (key, answer) in drained.answers {
            let Some(at) = link
                .pending
                .iter()
                .position(|(pending, _, _)| *pending == key)
            else {
                continue;
            };
            let (_, request, failures) = link.pending.swap_remove(at);
            match request.clone() {
                Request::Resolve => match result::<Instrument>(answer) {
                    Ok(instrument) => self.subscribe(&mut link, instrument),
                    Err(error) => self.request_failed(&mut link, request, failures, error, now),
                },
                Request::History { scroll_back, .. } => match result::<HistoryPage>(answer) {
                    Ok(page) => self.apply_page(&link, page, scroll_back),
                    Err(error) => self.request_failed(&mut link, request, failures, error, now),
                },
                Request::Trades => match result::<TradePage>(answer) {
                    Ok(page) => {
                        let bubbles = &mut link.bubbles;
                        let fold = &mut |trade: &Tick, added| {
                            bubbles.iter_mut().for_each(|b| b.fold(trade, added));
                        };
                        self.feed.rejected_bars += link.trades.prepend(page.trades, fold);
                    }
                    Err(error) => log::debug!("no trade history for {}: {error}", self.symbol),
                },
            }
        }

        let mut resnapshot = false;
        for (id, updates) in drained.updates {
            if link.subscription != Some(id) {
                continue;
            }
            if !updates.bars.is_empty() {
                let incoming = updates.bars.into_values().collect();
                let merged = self.apply_by(&link.clock, incoming);
                self.feed.late_ticks += merged.late_ticks;
            }
            if let Some(quote) = updates.quote
                && self.quote.as_ref().is_none_or(|old| quote.ts >= old.ts)
            {
                self.set_quote(Some(quote));
            }
            let feed = &mut self.feed;
            feed.status = updates.status.or(feed.status);
            feed.symbol_catalog_revision = feed
                .symbol_catalog_revision
                .wrapping_add(u64::from(updates.symbols_changed));
            feed.last_error = updates.error.or(feed.last_error.take());
            feed.late_ticks += updates.late_ticks;
            feed.repeated_ticks += updates.repeated_ticks;
            feed.rejected_bars += updates.rejected;
            feed.rejected_depth += updates.rejected_depth;
            let bubbles = &mut link.bubbles;
            let fold = &mut |trade: &Tick, added| {
                bubbles.iter_mut().for_each(|b| b.fold(trade, added));
            };
            link.trades.append(updates.trades, fold);
            link.order_book = updates.depth.or(link.order_book.take());
            resnapshot |= updates.resnapshot;
        }
        if resnapshot && let Some(id) = link.subscription {
            link.feed.request_depth_snapshot(id);
        }

        let mut at = 0;
        while at < link.retries.len() {
            if link.retries[at].0 > now {
                at += 1;
                continue;
            }
            let (_, request, failures) = link.retries.swap_remove(at);
            let feed = Arc::clone(&link.feed);
            match request.clone() {
                Request::Resolve => {
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    feed.resolve(&self.symbol, reply);
                }
                Request::History {
                    request: history, ..
                } => {
                    self.feed.loading_history = true;
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    feed.history(history, reply);
                }
                Request::Trades => unreachable!(),
            }
        }
        if let Some(deadline) = link.retries.iter().map(|r| r.0).min_by(f64::total_cmp) {
            ctx.request_repaint_after(
                std::time::Duration::try_from_secs_f64((deadline - now).max(0.0))
                    .unwrap_or(std::time::Duration::MAX),
            );
        }
        self.scroll_back(&mut link);
        self.feed_link = Some(link);
    }

    /// Drops what the chart held for the old symbol or timeframe and asks for the new one.
    fn reload(&mut self, link: &mut FeedLink, clock: BarClock) {
        link.reload_pending = false;
        if let Some(id) = link.subscription.take() {
            link.inbox.unsubscribe(id);
            link.feed.unsubscribe(id);
        }
        link.pending.clear();
        link.retries.clear();
        self.legend_instrument = link
            .instrument
            .take()
            .filter(|instrument| instrument.symbol == self.symbol);
        self.state.price_scale_mut().set_price_precision(None);
        link.trades = Trades::new(TRADE_CAPACITY);
        link.order_book = None;
        self.set_quote(None);
        link.paging_stopped = false;
        let timeframe = self.timeframe.unwrap_or_default();
        link.clock = clock;
        link.rebuild_bubbles();
        // Timestamp anchors survive the empty interval between history requests.
        self.replace_main_data(Vec::new());
        self.feed = FeedState::default();
        if self.symbol.is_empty() {
            return;
        }
        self.feed.loading_history = true;
        let reply = link.reply(Request::Resolve);
        link.feed.resolve(&self.symbol, reply);
        let request = HistoryRequest {
            symbol: self.symbol.clone(),
            timeframe,
            clock: link.clock.clone(),
            before: None,
            count: link.initial_bars,
            extended_hours: self.extended_hours,
        };
        let reply = link.reply(Request::History {
            request: request.clone(),
            scroll_back: false,
        });
        link.feed.history(request, reply);
    }

    /// Subscribes to live updates once the symbol resolved, and asks for the recent trades.
    fn subscribe(&mut self, link: &mut FeedLink, instrument: Instrument) {
        self.state
            .price_scale_mut()
            .set_price_precision(crate::scales::instrument_price_precision(&instrument));
        link.instrument = Some(instrument);
        link.rebuild_bubbles();
        let sink = link
            .inbox
            .subscribe(self.symbol.clone(), link.clock.clone(), TRADE_CAPACITY);
        link.subscription = Some(sink.id);
        let subscription = Subscription {
            id: sink.id,
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.unwrap_or_default(),
            clock: link.clock.clone(),
            extended_hours: self.extended_hours,
            sink,
        };
        link.feed.subscribe(subscription);
        let request = TradeRequest {
            symbol: self.symbol.clone(),
            before: None,
            count: TRADE_CAPACITY,
        };
        let reply = link.reply(Request::Trades);
        link.feed.trades(request, reply);
    }

    /// Merges a page of history and ends scroll-back when the feed has nothing older.
    fn apply_page(&mut self, link: &FeedLink, page: HistoryPage, scroll_back: bool) {
        let oldest = self.bars(SeriesId::MAIN).first().map(|bar| bar.time);
        let empty = page.bars.is_empty();
        let incoming = page.bars.into_iter().map(|bar| (bar, Origin::History));
        self.apply_by(&link.clock, incoming.collect());
        let older = self.bars(SeriesId::MAIN).first().map(|bar| bar.time) < oldest;
        // A feed that ignores `before` answers the newest bars again, and would be asked again
        // on every frame.
        let stale = scroll_back && !empty && !older;
        if stale {
            log::warn!(
                "the feed of {} sent no bar older than the oldest one held; scroll-back ended",
                self.symbol
            );
        }
        self.feed.loading_history = false;
        self.feed.history_complete |= page.end_of_history || empty || stale;
        self.data_loaded(SeriesId::MAIN);
    }

    /// Retries transient failures, retaining the exact page boundary and independent attempts.
    fn request_failed(
        &mut self,
        link: &mut FeedLink,
        request: Request,
        failures: u32,
        error: FeedError,
        now: f64,
    ) {
        log::warn!("feed request for {} failed: {error}", self.symbol);
        if matches!(request, Request::History { .. }) {
            self.feed.loading_history = false;
        }
        if error.is_recoverable() {
            let backoff = std::time::Duration::from_secs(1 << failures.min(6))
                .min(std::time::Duration::from_secs(60));
            let delay = match &error {
                FeedError::RateLimited {
                    retry_after: Some(after),
                } => backoff.max(*after),
                _ => backoff,
            };
            link.retries.push((
                now + delay.as_secs_f64(),
                request,
                failures.saturating_add(1),
            ));
        } else {
            link.paging_stopped = true;
        }
        self.feed.last_error = Some(error);
    }

    /// Asks for the page before the oldest bar once the view nears it.
    fn scroll_back(&mut self, link: &mut FeedLink) {
        let feed = &self.feed;
        if feed.loading_history
            || feed.history_complete
            || link
                .retries
                .iter()
                .any(|(_, request, _)| matches!(request, Request::History { .. }))
            || link.paging_stopped
            || !self.older_bars_drawn
        {
            return;
        }
        let Some(oldest) = self.bars(SeriesId::MAIN).first() else {
            return;
        };
        if self.symbol.is_empty() || self.state.visible_range().0 >= SCROLL_BACK_MARGIN {
            return;
        }
        // Stored bars keep the feed's stamp, any instant inside the slot.
        let before = link.clock.bar_open(oldest.time);
        let request = HistoryRequest {
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.unwrap_or_default(),
            clock: link.clock.clone(),
            before: Some(before),
            count: link.initial_bars.max(MIN_SCROLL_BACK_PAGE),
            extended_hours: self.extended_hours,
        };
        self.feed.loading_history = true;
        let reply = link.reply(Request::History {
            request: request.clone(),
            scroll_back: true,
        });
        link.feed.history(request, reply);
    }
}

/// The result an answer carries; the key it came under names its type.
fn result<T: 'static>(answer: Answer) -> Result<T, FeedError> {
    match answer.downcast::<Result<T, FeedError>>() {
        Ok(result) => *result,
        Err(_) => Err(FeedError::Other("answer of another request".into())),
    }
}

/// `wake`, also after a panic while it was held.
fn lock(wake: &Mutex<Option<ViewportId>>) -> std::sync::MutexGuard<'_, Option<ViewportId>> {
    wake.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::super::test_frames::frame;
    use super::*;
    use crate::data::Sink;
    use crate::model::{Bar, BarData};
    use crate::widget::ChartBuilder;
    use chrono::{DateTime, TimeDelta, Utc};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Resolve(String),
        History(HistoryRequest),
        Subscribe(SubscriptionId, String, Timeframe, bool),
        Unsubscribe(SubscriptionId),
        Trades(TradeRequest),
        Snapshot(SubscriptionId),
    }

    /// A feed that records every call and keeps the replies and sinks for the test to answer.
    #[derive(Default)]
    struct Script {
        calls: Mutex<Vec<Call>>,
        resolves: Mutex<Vec<Reply<Instrument>>>,
        pages: Mutex<Vec<Reply<HistoryPage>>>,
        trades: Mutex<Vec<Reply<TradePage>>>,
        sinks: Mutex<Vec<Sink>>,
        /// `unsubscribe` drops the replies it keeps.
        drop_replies: AtomicBool,
    }

    impl Script {
        fn calls(&self) -> Vec<Call> {
            std::mem::take(&mut *self.calls.lock().unwrap())
        }
        fn resolve(&self) -> Reply<Instrument> {
            self.resolves.lock().unwrap().remove(0)
        }
        fn page(&self) -> Reply<HistoryPage> {
            self.pages.lock().unwrap().remove(0)
        }
        fn sink(&self) -> Sink {
            self.sinks.lock().unwrap().last().unwrap().clone()
        }
    }

    impl Datafeed for Script {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Resolve(symbol.into()));
            self.resolves.lock().unwrap().push(reply);
        }
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            self.calls.lock().unwrap().push(Call::History(request));
            self.pages.lock().unwrap().push(reply);
        }
        fn subscribe(&self, subscription: Subscription) {
            let Subscription {
                id,
                symbol,
                timeframe,
                extended_hours,
                sink,
                ..
            } = subscription;
            let call = Call::Subscribe(id, symbol, timeframe, extended_hours);
            self.calls.lock().unwrap().push(call);
            self.sinks.lock().unwrap().push(sink);
        }
        fn unsubscribe(&self, id: SubscriptionId) {
            self.calls.lock().unwrap().push(Call::Unsubscribe(id));
            if self.drop_replies.load(Ordering::Relaxed) {
                self.resolves.lock().unwrap().clear();
                self.pages.lock().unwrap().clear();
                self.trades.lock().unwrap().clear();
            }
        }
        fn trades(&self, request: TradeRequest, reply: Reply<TradePage>) {
            self.calls.lock().unwrap().push(Call::Trades(request));
            self.trades.lock().unwrap().push(reply);
        }
        fn request_depth_snapshot(&self, id: SubscriptionId) {
            self.calls.lock().unwrap().push(Call::Snapshot(id));
        }
    }

    /// `count` one-minute bars from 00:00 on 15 March 2024, closing at 100 plus their index.
    fn bars(count: usize) -> Vec<Bar> {
        let start = "2024-03-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        (0..count)
            .map(|i| {
                let close = 100.0 + i as f64;
                let time = start + TimeDelta::minutes(i as i64);
                Bar::new(time, close, close, close, close, 1.0)
            })
            .collect()
    }

    fn page(bars: Vec<Bar>, end_of_history: bool) -> Result<HistoryPage, FeedError> {
        Ok(HistoryPage {
            bars,
            end_of_history,
        })
    }

    fn history(symbol: &str, timeframe: Timeframe, before: Option<DateTime<Utc>>) -> Call {
        Call::History(HistoryRequest {
            symbol: symbol.into(),
            timeframe,
            clock: BarClock::new(timeframe),
            before,
            count: 1000,
            extended_hours: false,
        })
    }

    /// Frames at 60 Hz, each a sixtieth of a second after the last.
    struct Frames {
        ctx: egui::Context,
        time: f64,
    }

    impl Frames {
        fn new() -> Self {
            Self {
                ctx: egui::Context::default(),
                time: 0.0,
            }
        }
        fn run(&mut self, chart: &mut Chart) {
            self.time += 1.0 / 60.0;
            frame(&self.ctx, chart, self.time, Vec::new());
        }
    }

    fn fed(feed: &Arc<Script>, symbol: Option<&str>) -> Chart {
        let builder = ChartBuilder::new().with_feed(feed.clone());
        match symbol {
            Some(symbol) => builder.with_symbol(symbol).build(),
            None => builder.build(),
        }
    }

    #[test]
    fn extended_hours_reload_history_paging_and_subscription_together() {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        let mut previous = None;
        for extended in [false, true, false] {
            chart.set_extended_hours(extended);
            assert_eq!(chart.extended_hours(), extended);
            frames.run(&mut chart);
            let calls = feed.calls();
            if let Some(previous) = previous {
                assert!(calls.contains(&Call::Unsubscribe(previous)));
            }
            assert!(calls.iter().any(|call| matches!(call, Call::History(request) if request.before.is_none() && request.extended_hours == extended)));
            feed.page().send(page(bars(1000), false));
            feed.resolve().send(Ok(Instrument::new("A")));
            frames.run(&mut chart);
            let id = feed.sink().id;
            let mut calls = feed.calls();
            assert!(calls.contains(&Call::Subscribe(id, "A".into(), Timeframe::Min1, extended)));
            chart.set_extended_hours(extended);
            frames.run(&mut chart);
            assert!(
                feed.calls().is_empty(),
                "an unchanged choice must keep the stream"
            );
            chart.set_start_idx(0);
            frames.run(&mut chart);
            frames.run(&mut chart);
            calls.extend(feed.calls());
            assert!(calls.iter().any(|call| matches!(call, Call::History(request) if request.before.is_some() && request.extended_hours == extended)), "{extended}: {calls:?}");
            feed.page().send(page(vec![], true));
            frames.run(&mut chart);
            previous = Some(id);
        }
    }

    /// Resolving an exchange zone must realign history, live aggregation and the countdown
    /// once, then keep the subscription stable through ordinary repaints.
    #[test]
    fn calendar_reload_uses_the_resolved_exchange_zone() {
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        chart.chart_options.time_scale.timezone =
            crate::scales::TimezoneMode::Exchange(chrono_tz::UTC);
        chart.set_timeframe(Timeframe::Hour4);
        let mut frames = Frames::new();
        let zone = crate::model::TimezoneId::named("America/New_York").unwrap();
        let expected = BarClock::new(Timeframe::Hour4).with_timezone(chrono_tz::America::New_York);
        frames.run(&mut chart);
        feed.page().send(page(Vec::new(), true));
        feed.resolve()
            .send(Ok(Instrument::new("A").with_time_zone(zone.clone())));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(chart.bar_clock(), expected);
        assert!(
            feed.calls()
                .iter()
                .any(|call| matches!(call, Call::History(request) if request.clock == expected))
        );
        feed.page().send(page(Vec::new(), true));
        feed.resolve()
            .send(Ok(Instrument::new("A").with_time_zone(zone)));
        frames.run(&mut chart);
        let sink = feed.sink();
        let time = "2024-03-10T07:30:00Z".parse().unwrap();
        sink.push_tick(Tick::new(time, 100.0, 2.0, None));
        frames.run(&mut chart);
        assert_eq!(chart.bars(SeriesId::MAIN)[0].time, expected.bar_open(time));
        assert_eq!(chart.feed_link.as_ref().unwrap().clock, expected);
        feed.calls();
        frames.run(&mut chart);
        assert!(feed.calls().is_empty());
    }

    #[test]
    fn scroll_to_time_shows_the_nearest_bar_and_loads_older_history() {
        let all = bars(3000);
        // Exact match, nearer right/left neighbour, tie (earlier wins), and both ends.
        for (seconds, expected) in [
            (30000, 500),
            (30040, 501),
            (30020, 500),
            (30030, 500),
            (-60, 0),
            (180000, 2999),
        ] {
            let mut chart = Chart::new(BarData::from_bars(all.clone()));
            let mut frames = Frames::new();
            chart.set_visible_bars(100);
            frames.run(&mut chart);
            let spacing = chart.state.time_scale().bar_spacing();
            chart.enable_tracking_mode();
            chart.scroll_to_time(all[0].time + TimeDelta::seconds(seconds));
            frames.run(&mut chart);
            let range = chart.state.time_scale().visible_logical_range();
            assert!(
                ((range.left + range.right) / 2.0 - expected as f32).abs() < 0.01,
                "{seconds}: {range:?}"
            );
            assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
            assert!(!chart.is_tracking_mode_active());
        }
        let mut empty = Chart::new(BarData::new());
        empty.scroll_to_time(all[0].time);
        Frames::new().run(&mut empty);

        for (target, expected, exhaust) in [
            (all[300].time, 300, false),
            (all[0].time - TimeDelta::days(1), 0, true),
        ] {
            let feed = Arc::new(Script::default());
            let mut chart = fed(&feed, Some("A"));
            let mut frames = Frames::new();
            // Navigation requested even before the first page remains pending.
            chart.scroll_to_time(target);
            frames.run(&mut chart);
            feed.calls();
            feed.page().send(page(all[2000..].to_vec(), false));
            frames.run(&mut chart);
            frames.run(&mut chart);
            assert_eq!(
                feed.calls(),
                [history("A", Timeframe::Min1, Some(all[2000].time))]
            );
            feed.page().send(page(all[1000..2000].to_vec(), false));
            frames.run(&mut chart);
            frames.run(&mut chart);
            assert_eq!(
                feed.calls(),
                [history("A", Timeframe::Min1, Some(all[1000].time))]
            );
            feed.page().send(page(all[..1000].to_vec(), exhaust));
            frames.run(&mut chart);
            frames.run(&mut chart);
            let range = chart.state.time_scale().visible_logical_range();
            assert!(
                ((range.left + range.right) / 2.0 - expected as f32).abs() < 0.01,
                "{range:?}"
            );
            assert!(feed.calls().is_empty());
        }

        // Moving the view while an older page is outstanding cancels the target.
        fn play(chart: &mut Chart, frames: &mut Frames, events: Vec<egui::Event>) {
            for event in events {
                frames.time += 1.0 / 60.0;
                frame(&frames.ctx, chart, frames.time, vec![event]);
            }
        }
        fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        }
        let drag = |chart: &mut Chart, frames: &mut Frames| {
            let start = chart.last_rendered_price_rect.center();
            let mut events = vec![egui::Event::PointerMoved(start), button(start, true)];
            events.extend(
                (1..=10)
                    .map(|i| egui::Event::PointerMoved(start - egui::vec2(40.0 * i as f32, 0.0))),
            );
            events.push(button(start - egui::vec2(400.0, 0.0), false));
            play(chart, frames, events);
        };
        let home_key = |chart: &mut Chart, frames: &mut Frames| {
            let centre = chart.last_rendered_price_rect.center();
            let home = egui::Event::Key {
                key: egui::Key::Home,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let click = [egui::Event::PointerMoved(centre), button(centre, true)];
            play(
                chart,
                frames,
                [&click[..], &[button(centre, false), home]].concat(),
            );
        };
        let box_zoom = |chart: &mut Chart, _: &mut Frames| {
            let rect = chart.last_rendered_price_rect;
            let start = rect.right_top() + egui::vec2(-100.0, 10.0);
            let end = rect.right_bottom() - egui::vec2(10.0, 10.0);
            assert!(chart.execute_box_zoom(start, end, rect, rect.width()));
        };
        let moves: [fn(&mut Chart, &mut Frames); 4] = [
            |chart, _| chart.set_start_idx(600),
            drag,
            home_key,
            box_zoom,
        ];
        for (row, user_move) in moves.into_iter().enumerate() {
            let feed = Arc::new(Script::default());
            let mut chart = fed(&feed, Some("A"));
            let mut frames = Frames::new();
            frames.run(&mut chart);
            feed.page().send(page(all[2000..].to_vec(), false));
            frames.run(&mut chart);
            chart.scroll_to_time(all[300].time);
            frames.run(&mut chart);
            frames.run(&mut chart);
            user_move(&mut chart, &mut frames);
            for _ in 0..5 {
                frames.run(&mut chart);
            }
            let range = chart.state.time_scale().visible_logical_range();
            assert!((range.left + range.right) / 2.0 > 25.0, "{row}: {range:?}");
        }
    }

    #[test]
    fn transient_requests_back_off_and_keep_the_failed_page_boundary() {
        for (error, delay) in [
            (FeedError::Network("offline".into()), 1.0),
            (FeedError::Unanswered, 1.0),
            (
                FeedError::RateLimited {
                    retry_after: Some(std::time::Duration::from_secs(7)),
                },
                7.0,
            ),
            (FeedError::RateLimited { retry_after: None }, 1.0),
        ] {
            let feed = Arc::new(Script::default());
            let mut chart = fed(&feed, Some("A"));
            let mut frames = Frames::new();
            frames.run(&mut chart);
            feed.page().send(page(bars(1000), false));
            feed.resolve().send(Ok(Instrument::new("A")));
            frames.run(&mut chart);
            chart.set_start_idx(0);
            frames.run(&mut chart);
            frames.run(&mut chart);
            feed.calls();
            let expected = history("A", Timeframe::Min1, Some(bars(1)[0].time));
            feed.page().send(Err(error.clone()));
            frames.run(&mut chart);
            let failed_at = frames.time;
            frames.time = failed_at + delay - 0.1;
            frames.run(&mut chart);
            assert!(feed.calls().is_empty(), "early retry: {error}");
            frames.time = failed_at + delay;
            frames.run(&mut chart);
            assert_eq!(
                feed.calls(),
                std::slice::from_ref(&expected),
                "missing retry: {error}"
            );
            feed.page()
                .send(Err(FeedError::Network("still offline".into())));
            frames.run(&mut chart);
            let second = frames.time;
            frames.time = second + 1.5;
            frames.run(&mut chart);
            assert!(feed.calls().is_empty(), "backoff did not double");
            frames.time = second + 2.0;
            frames.run(&mut chart);
            assert_eq!(feed.calls(), [expected]);
            feed.page().send(page(bars(1000), true));
            frames.run(&mut chart);
            assert!(chart.feed_state(SeriesId::MAIN).unwrap().history_complete);
        }
        // A failed resolve retries independently and a changed key cancels its deadline.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.calls();
        feed.resolve()
            .send(Err(FeedError::Network("offline".into())));
        feed.page().send(page(bars(10), true));
        frames.run(&mut chart);
        frames.time += 1.0;
        frames.run(&mut chart);
        assert_eq!(feed.calls(), [Call::Resolve("A".into())]);
        feed.resolve()
            .send(Err(FeedError::Network("offline".into())));
        frames.run(&mut chart);
        chart.set_symbol("B");
        frames.run(&mut chart);
        feed.calls();
        frames.time += 60.0;
        frames.run(&mut chart);
        assert!(feed.calls().is_empty(), "old symbol retry survived");
        // A host can use an arbitrarily long rate limit, including Duration::MAX.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        let mut frames = Frames::new();
        frames.run(&mut chart);
        feed.calls();
        feed.page().send(Err(FeedError::RateLimited {
            retry_after: Some(std::time::Duration::MAX),
        }));
        frames.run(&mut chart);
        frames.time += 3600.0;
        frames.run(&mut chart);
        assert!(feed.calls().is_empty(), "long rate limit resumed early");
    }

    /// Guards the feed contract of the chart: nothing blocks or calls the feed outside `show`,
    /// each request is issued once, answers and pushes reach the bars, a push from another thread
    /// wakes the UI once per burst, order flow is asked for and reaches the volume bubbles,
    /// scroll-back ends on every end signal, a permanent error stops requests until the symbol
    /// or timeframe changes, a change releases the old subscription and ignores whatever comes
    /// for it, and a dropped chart unsubscribes without waking itself.
    #[test]
    fn feed_lifecycle() {
        let main = SeriesId::MAIN;
        let mut frames = Frames::new();

        // a. `build` calls nothing; the first frames ask once for the instrument and a page.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        assert_eq!(feed.calls(), [], "a: build");
        frames.run(&mut chart);
        frames.run(&mut chart);
        let first_page = history("A", Timeframe::Min1, None);
        assert_eq!(feed.calls(), [Call::Resolve("A".into()), first_page], "a");

        assert!(
            !chart
                .drain_events()
                .any(|event| matches!(event, super::super::ChartEvent::DataLoaded { .. })),
            "starting history must not report a completed load"
        );
        // b. The page reaches the bars and notifies the host once.
        feed.page().send(page(bars(1000), false));
        frames.run(&mut chart);
        assert_eq!(chart.bars(main), bars(1000), "b");
        assert_eq!(
            chart
                .drain_events()
                .filter(|event| matches!(event, super::super::ChartEvent::DataLoaded { .. }))
                .collect::<Vec<_>>(),
            [super::super::ChartEvent::DataLoaded {
                series: main,
                bars: 1000
            }]
        );
        assert!(!chart.feed_state(main).unwrap().loading_history, "b");

        // c. The instrument subscribes and asks for trades once; a burst from another thread
        // wakes the root viewport once and reaches the forming bar; a lost book asks for one
        // snapshot.
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        let id = feed.sink().id;
        let trades = Call::Trades(TradeRequest {
            symbol: "A".into(),
            before: None,
            count: TRADE_CAPACITY,
        });
        let subscribe = Call::Subscribe(id, "A".into(), Timeframe::Min1, false);
        assert_eq!(feed.calls(), [subscribe, trades], "c");
        assert_eq!(chart.instrument(), Some(&Instrument::new("A")), "c");
        feed.sink().symbols_changed();
        feed.sink().symbols_changed();
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(
            chart.feed_state(main).unwrap().symbol_catalog_revision,
            1,
            "c: catalog notification coalesces"
        );
        chart.set_volume_bubbles(Some(VolumeBubblesConfig::default()));
        let woken = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&woken);
        frames
            .ctx
            .set_repaint_observer(move |info| seen.lock().unwrap().push(info.viewport_id));
        let forming = bars(1001).pop().unwrap();
        let sink = feed.sink();
        let ts = forming.time + TimeDelta::seconds(30);
        std::thread::spawn(move || {
            // The last trade repeats an id: it is counted, not folded.
            for id in (0..1000).chain([999]) {
                sink.push_tick(Tick::new(ts, 7.0, 1.0, None).with_id(id));
            }
        })
        .join()
        .unwrap();
        assert_eq!(*woken.lock().unwrap(), [ViewportId::ROOT], "c: one wake");
        frames.run(&mut chart);
        let ticks = Bar::new(forming.time, 7.0, 7.0, 7.0, 7.0, 1000.0);
        assert_eq!(chart.bars(main).last(), Some(&ticks), "c: forming bar");
        assert_eq!(chart.trades(main).count(), 1000, "c: trades");
        let bubbles = chart
            .feed_link
            .as_ref()
            .and_then(|link| link.bubbles.as_ref());
        let shown = bubbles.unwrap().between(forming.time, forming.time);
        let shown = Vec::from_iter(shown.map(|bubble| (bubble.price, bubble.volume)));
        assert_eq!(shown, [(7.0, 1000.0)], "c: the trades reach the bubbles");
        assert_eq!(chart.feed_state(main).unwrap().repeated_ticks, 1, "c");
        let empty = OrderBook {
            ts,
            symbol: "A".into(),
            bids: Vec::new(),
            asks: Vec::new(),
        };
        feed.sink().push_depth_delta(5, empty);
        frames.run(&mut chart);
        assert_eq!(feed.calls(), [Call::Snapshot(id)], "c: snapshot");
        frames.run(&mut chart);
        assert_eq!(feed.calls(), [], "c: one snapshot");

        let quote = |time, symbol: &str, bid| crate::model::Level1Quote {
            ts: time,
            symbol: symbol.into(),
            bid,
            ask: 8.0,
            bid_size: 1.0,
            ask_size: 2.0,
        };
        feed.sink().push_quote(quote(ts, "A", 6.0));
        feed.sink()
            .push_quote(quote(ts + TimeDelta::seconds(1), "B", 4.0));
        frames.run(&mut chart);
        assert_eq!(
            chart.quote.as_ref().map(|q| q.bid),
            Some(6.0),
            "c: quote reaches legend"
        );
        feed.sink()
            .push_quote(quote(ts - TimeDelta::seconds(1), "A", 5.0));
        frames.run(&mut chart);
        assert_eq!(
            chart.quote.as_ref().map(|q| q.bid),
            Some(6.0),
            "c: older frame quote ignored"
        );
        feed.sink()
            .push_quote(quote(ts + TimeDelta::seconds(1), "B", 4.0));
        frames.run(&mut chart);
        assert_eq!(
            chart.quote.as_ref().map(|q| q.bid),
            Some(6.0),
            "c: other symbol ignored"
        );

        // h. Bars appended while the view is scrolled back leave the visible bars where they are,
        // and so do updates of the forming bar stamped later inside its slot.
        chart.state.time_scale_mut().set_right_offset(-50.0);
        frames.run(&mut chart);
        let first_visible = |chart: &Chart| chart.bars(main)[chart.state.visible_range().0].time;
        let shown = first_visible(&chart);
        for bar in bars(1004).drain(1001..) {
            feed.sink().push_bar(bar);
        }
        frames.run(&mut chart);
        assert_eq!(chart.bars(main).len(), 1004, "h");
        assert_eq!(first_visible(&chart), shown, "h: the view moved");
        let mut forming = bars(1004).pop().unwrap();
        for seconds in [10, 20] {
            forming.time += TimeDelta::seconds(seconds);
            feed.sink().push_bar(forming.clone());
            frames.run(&mut chart);
        }
        assert_eq!(chart.bars(main).len(), 1004, "h: forming");
        assert_eq!(first_visible(&chart), shown, "h: forming bar");

        // d. Near the oldest bar the chart asks once for the page before it; a page with nothing
        // older ends scroll-back.
        chart.set_start_idx(0);
        for _ in 0..5 {
            frames.run(&mut chart);
        }
        let oldest = chart.bars(main)[0].time;
        let older = history("A", Timeframe::Min1, Some(oldest));
        assert_eq!(feed.calls(), [older], "d");
        feed.page().send(page(bars(1000)[400..].to_vec(), false));
        for _ in 0..5 {
            frames.run(&mut chart);
        }
        assert_eq!(feed.calls(), [], "d: a repeated page ends scroll-back");
        assert!(chart.feed_state(main).unwrap().history_complete, "d");

        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.page().send(page(bars(1000), true));
        frames.run(&mut chart);
        chart.set_start_idx(0);
        for _ in 0..5 {
            frames.run(&mut chart);
        }
        assert_eq!(feed.calls().len(), 2, "d: end of history");

        // e. A permanently failed page stops scroll-back, also through a setter called every frame with the
        // value it has; a live error does not.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.page().send(page(bars(1000), false));
        frames.run(&mut chart);
        chart.set_start_idx(0);
        frames.run(&mut chart);
        frames.run(&mut chart);
        feed.calls();
        feed.page()
            .send(Err(FeedError::InvalidData("reset".into())));
        frames.run(&mut chart);
        let error = Some(FeedError::InvalidData("reset".into()));
        assert_eq!(chart.feed_state(main).unwrap().last_error, error, "e");
        for _ in 0..10 {
            chart.set_timeframe(Timeframe::Min1);
            frames.run(&mut chart);
        }
        assert_eq!(feed.calls(), [], "e: no retry");
        assert_eq!(chart.bars(main), bars(1000), "e: bars kept");

        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        feed.page().send(page(bars(1000), false));
        frames.run(&mut chart);
        feed.sink().report_error(FeedError::Network("reset".into()));
        frames.run(&mut chart);
        assert_eq!(
            chart.feed_state(main).unwrap().last_error,
            Some(FeedError::Network("reset".into())),
            "e: live"
        );
        chart.set_start_idx(0);
        frames.run(&mut chart);
        frames.run(&mut chart);
        let calls = feed.calls();
        let older = history("A", Timeframe::Min1, Some(bars(1)[0].time));
        assert_eq!(calls.last(), Some(&older), "e: a live error stops nothing");

        // f. A change of symbol and timeframe between two frames is one load at the next frame,
        // which releases the old subscription and drops its trades and bubbles; what arrives for
        // the old one changes nothing.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_price_decimals(3)));
        frames.run(&mut chart);
        let precision = |chart: &Chart| {
            chart
                .state
                .price_scale()
                .map(crate::scales::PriceTransform::default(), 0.0, 100.0)
                .price_precision()
        };
        assert_eq!(precision(&chart), Some(3), "f: supplied precision");
        let (old_sink, old_page) = (feed.sink(), feed.page());
        chart.set_volume_bubbles(Some(VolumeBubblesConfig::default()));
        old_sink.push_tick(Tick::new(Utc::now(), 7.0, 1.0, None));
        frames.run(&mut chart);
        assert_eq!(chart.trades(main).count(), 1, "f: a trade of the old key");
        feed.calls();
        chart.set_symbol("B");
        chart.set_timeframe(Timeframe::Hour1);
        assert_eq!(feed.calls(), [], "f: no call from a setter");
        frames.run(&mut chart);
        let load = [
            Call::Unsubscribe(old_sink.id),
            Call::Resolve("B".into()),
            history("B", Timeframe::Hour1, None),
        ];
        assert_eq!(feed.calls(), load, "f");
        assert!(!old_sink.is_active(), "f: old sink");
        chart.drain_events().for_each(drop);
        old_sink.push_bar(bars(1)[0].clone());
        old_page.send(page(bars(10), false));
        frames.run(&mut chart);
        assert_eq!(chart.bars(main), [], "f: nothing of the old key");
        assert!(
            !chart
                .drain_events()
                .any(|event| matches!(event, super::super::ChartEvent::DataLoaded { .. }))
        );
        assert_eq!(precision(&chart), None, "f: old precision cleared");
        let bubbles = chart
            .feed_link
            .as_ref()
            .and_then(|link| link.bubbles.as_ref());
        let all = bubbles
            .unwrap()
            .between(DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC);
        assert_eq!(all.count(), 0, "f: no bubble of the old key");
        for _ in 0..3 {
            chart.set_timeframe(Timeframe::Hour1);
            frames.run(&mut chart);
        }
        assert_eq!(feed.calls(), [], "f: an unchanged value reloads nothing");

        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        chart.set_symbol("B");
        frames.run(&mut chart);
        let calls = feed.calls();
        assert!(
            !calls.iter().any(|call| matches!(call, Call::Subscribe(..))),
            "f: a late resolve subscribes nothing: {calls:?}"
        );

        // g. Without a symbol the feed is never called. Bars the host sets keep one per slot.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, None);
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert_eq!(feed.calls(), [], "g");
        let mut set = bars(2);
        let mut later = set[0].clone();
        later.time += TimeDelta::seconds(20);
        set.insert(1, later.clone());
        chart.set_data(main, set);
        assert_eq!(chart.bars(main), [later, bars(2)[1].clone()], "g: slot");

        // i. A chart dropped inside an input handler, while the feed drops its replies in
        // `unsubscribe`, unsubscribes and returns.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        let sink = feed.sink();
        feed.calls();
        feed.drop_replies.store(true, Ordering::Relaxed);
        frames.ctx.input(move |_| drop(chart));
        assert_eq!(feed.calls(), [Call::Unsubscribe(sink.id)], "i");
        assert!(!sink.is_active(), "i: sink");

        // j. Labelling a chart without a feed keeps its bars.
        let mut chart = Chart::new(BarData::from_bars(bars(10)));
        chart.set_symbol("X");
        chart.set_timeframe(Timeframe::Hour1);
        frames.run(&mut chart);
        assert_eq!(chart.bars(main), bars(10), "j");

        // k. Scroll-back counts elements; older history adds none to a trimmed build.
        for (size, asks) in [(1.0, true), (1.0 / 16.0, false)] {
            let feed = Arc::new(Script::default());
            let mut chart = fed(&feed, Some("A"));
            chart.set_chart_type(crate::model::ChartType::Renko);
            chart.set_chart_type_params(crate::model::ChartTypeParams {
                renko_brick_size: size,
                ..Default::default()
            });
            frames.run(&mut chart);
            feed.page().send(page(bars(1000), false));
            frames.run(&mut chart);
            feed.calls();
            chart.set_start_idx(0);
            for _ in 0..5 {
                frames.run(&mut chart);
            }
            let older = history("A", Timeframe::Min1, Some(bars(1)[0].time));
            assert_eq!(feed.calls().contains(&older), asks, "k: brick {size}");
        }
        // Removing the main series keeps its reference history even when playback resumes.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.page().send(page(bars(1000), true));
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        let id = feed.sink().id;
        feed.calls();
        assert!(chart.remove_series(main));
        assert_eq!(feed.calls(), [Call::Unsubscribe(id)]);
        chart.set_feed_paused(false);
        frames.run(&mut chart);
        assert!(
            feed.calls().is_empty(),
            "removed main cannot restart its feed"
        );
        assert_eq!(chart.state.data().bars, bars(1000));
    }
}
