//! A chart driven by a [`Datafeed`]: requests issued from `show`, answers and live pushes applied
//! once per frame.
//!
//! Every feed call but the one in [`FeedLink`]'s `Drop` comes from [`Chart::pump`], which runs
//! outside any egui closure: a feed may answer or push inline, and that runs the waker, which
//! takes the egui context's lock.

use super::{Chart, SeriesId};
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
    next_key: u64,
    /// Requests not answered yet; an answer under any other key is stale and dropped.
    pending: Vec<(u64, Request)>,
    subscription: Option<SubscriptionId>,
    instrument: Option<Instrument>,
    trades: Trades,
    order_book: Option<OrderBook>,
    /// A history or resolve request failed: no automatic request until the next reload.
    paging_stopped: bool,
}

enum Request {
    Resolve,
    History { scroll_back: bool },
    Trades,
}

impl FeedLink {
    /// A reply for `request`, under a new key.
    fn reply<T: Send + 'static>(&mut self, request: Request) -> Reply<T> {
        self.next_key += 1;
        self.pending.push((self.next_key, request));
        self.inbox.reply(self.next_key)
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
        let timeframe = self.timeframe.unwrap_or_default();
        self.timeframe = Some(timeframe);
        self.feed_link = Some(FeedLink {
            feed,
            inbox: Arc::default(),
            clock: BarClock::new(timeframe),
            initial_bars,
            wake: Arc::default(),
            reload_pending: true,
            next_key: 0,
            pending: Vec::new(),
            subscription: None,
            instrument: None,
            trades: Trades::new(TRADE_CAPACITY),
            order_book: None,
            paging_stopped: false,
        });
    }

    /// Shows `symbol` from the next [`show`](Self::show). With a feed, a changed symbol clears
    /// the series and its studies, then resolves, loads and subscribes it, in one load per
    /// `show` however many setters ran; an unchanged one does nothing, also after an error.
    /// Without a feed it only sets the label: replace the bars with
    /// [`set_data`](Self::set_data).
    pub fn set_symbol(&mut self, symbol: impl Into<String>) {
        let symbol = symbol.into();
        if symbol != self.symbol
            && let Some(link) = &mut self.feed_link
        {
            link.reload_pending = true;
        }
        self.symbol = symbol;
    }

    /// Shows bars of `timeframe` from the next [`show`](Self::show), by the rules of
    /// [`set_symbol`](Self::set_symbol). Without a feed it sets the legend label and the
    /// session breaks only.
    pub fn set_timeframe(&mut self, timeframe: Timeframe) {
        if self.timeframe != Some(timeframe)
            && let Some(link) = &mut self.feed_link
        {
            link.reload_pending = true;
        }
        self.timeframe = Some(timeframe);
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

    /// The feed's description of the symbol, once it answered.
    pub fn instrument(&self) -> Option<&Instrument> {
        self.feed_link.as_ref()?.instrument.as_ref()
    }

    /// The trades of `series` the feed sent, oldest first, at most 100,000. Empty for any id but
    /// [`SeriesId::MAIN`] and without a feed.
    pub fn trades(&self, series: SeriesId) -> impl Iterator<Item = &Tick> {
        let link = self.feed_link.as_ref().filter(|_| series == SeriesId::MAIN);
        link.into_iter().flat_map(|link| link.trades.iter())
    }

    /// The order book of `series` the feed sent, if it holds one.
    pub fn order_book(&self, series: SeriesId) -> Option<&OrderBook> {
        let link = self
            .feed_link
            .as_ref()
            .filter(|_| series == SeriesId::MAIN)?;
        link.order_book.as_ref()
    }

    /// The clock bars merge by: the feed's, or exact times without a feed.
    pub(super) fn clock(&self) -> BarClock {
        self.feed_link
            .as_ref()
            .map_or_else(BarClock::exact, |link| link.clock.clone())
    }

    /// Issues what the feed must be asked and applies what it sent since the last call. Called
    /// first in [`show`](Self::show); does nothing without a feed.
    pub(crate) fn pump(&mut self, ctx: &egui::Context) {
        let Some(mut link) = self.feed_link.take() else {
            return;
        };
        if link.reload_pending {
            self.reload(&mut link);
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

        let drained = link.inbox.drain();
        for (key, answer) in drained.answers {
            let Some(at) = link.pending.iter().position(|(pending, _)| *pending == key) else {
                continue;
            };
            match link.pending.swap_remove(at).1 {
                Request::Resolve => match result::<Instrument>(answer) {
                    Ok(instrument) => self.subscribe(&mut link, instrument),
                    Err(error) => self.stop(&mut link, error),
                },
                Request::History { scroll_back } => match result::<HistoryPage>(answer) {
                    Ok(page) => self.apply_page(&link, page, scroll_back),
                    Err(error) => self.stop(&mut link, error),
                },
                Request::Trades => match result::<TradePage>(answer) {
                    Ok(page) => self.feed.rejected_bars += link.trades.prepend(page.trades),
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
            let feed = &mut self.feed;
            feed.status = updates.status.or(feed.status);
            feed.last_error = updates.error.or(feed.last_error.take());
            feed.late_ticks += updates.late_ticks;
            feed.repeated_ticks += updates.repeated_ticks;
            feed.rejected_bars += updates.rejected;
            feed.rejected_depth += updates.rejected_depth;
            link.trades.append(updates.trades);
            link.order_book = updates.depth.or(link.order_book.take());
            resnapshot |= updates.resnapshot;
        }
        if resnapshot && let Some(id) = link.subscription {
            link.feed.request_depth_snapshot(id);
        }

        self.scroll_back(&mut link);
        self.feed_link = Some(link);
    }

    /// Drops what the chart held for the old symbol or timeframe and asks for the new one.
    fn reload(&mut self, link: &mut FeedLink) {
        link.reload_pending = false;
        if let Some(id) = link.subscription.take() {
            link.inbox.unsubscribe(id);
            link.feed.unsubscribe(id);
        }
        link.pending.clear();
        link.instrument = None;
        link.trades = Trades::new(TRADE_CAPACITY);
        link.order_book = None;
        link.paging_stopped = false;
        let timeframe = self.timeframe.unwrap_or_default();
        link.clock = BarClock::new(timeframe);
        // Drawings stay: they belong to the chart, not to the bars of one symbol.
        self.set_data(SeriesId::MAIN, Vec::new());
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
            before: None,
            count: link.initial_bars,
        };
        let reply = link.reply(Request::History { scroll_back: false });
        link.feed.history(request, reply);
    }

    /// Subscribes to live updates once the symbol resolved, and asks for the recent trades.
    fn subscribe(&mut self, link: &mut FeedLink, instrument: Instrument) {
        link.instrument = Some(instrument);
        let sink = link.inbox.subscribe(link.clock.clone(), TRADE_CAPACITY);
        link.subscription = Some(sink.id);
        let subscription = Subscription {
            id: sink.id,
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.unwrap_or_default(),
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
    }

    /// Reports `error` and stops automatic requests until the symbol or timeframe changes.
    fn stop(&mut self, link: &mut FeedLink, error: FeedError) {
        log::warn!("feed request for {} failed: {error}", self.symbol);
        self.feed.last_error = Some(error);
        self.feed.loading_history = false;
        link.paging_stopped = true;
    }

    /// Asks for the page before the oldest bar once the view nears it.
    fn scroll_back(&mut self, link: &mut FeedLink) {
        let feed = &self.feed;
        if feed.loading_history || feed.history_complete || link.paging_stopped {
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
            before: Some(before),
            count: link.initial_bars.max(MIN_SCROLL_BACK_PAGE),
        };
        self.feed.loading_history = true;
        let reply = link.reply(Request::History { scroll_back: true });
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

    #[derive(Debug, PartialEq)]
    enum Call {
        Resolve(String),
        History(HistoryRequest),
        Subscribe(SubscriptionId, String, Timeframe),
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
                sink,
            } = subscription;
            let call = Call::Subscribe(id, symbol, timeframe);
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
            before,
            count: 1000,
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

    /// Guards the feed contract of the chart: nothing blocks or calls the feed outside `show`,
    /// each request is issued once, answers and pushes reach the bars, a push from another thread
    /// wakes the UI once per burst, order flow is asked for, scroll-back ends on every end
    /// signal, an error stops automatic requests until the symbol or timeframe changes, a change
    /// releases the old subscription and ignores whatever comes for it, and a dropped chart
    /// unsubscribes without waking itself.
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

        // b. The page reaches the bars.
        feed.page().send(page(bars(1000), false));
        frames.run(&mut chart);
        assert_eq!(chart.bars(main), bars(1000), "b");
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
        let subscribe = Call::Subscribe(id, "A".into(), Timeframe::Min1);
        assert_eq!(feed.calls(), [subscribe, trades], "c");
        assert_eq!(chart.instrument(), Some(&Instrument::new("A")), "c");
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

        // e. A failed page stops scroll-back, also through a setter called every frame with the
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
        feed.page().send(Err(FeedError::Network("reset".into())));
        frames.run(&mut chart);
        let error = Some(FeedError::Network("reset".into()));
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
        assert_eq!(chart.feed_state(main).unwrap().last_error, error, "e: live");
        chart.set_start_idx(0);
        frames.run(&mut chart);
        frames.run(&mut chart);
        let calls = feed.calls();
        let older = history("A", Timeframe::Min1, Some(bars(1)[0].time));
        assert_eq!(calls.last(), Some(&older), "e: a live error stops nothing");

        // f. A change of symbol and timeframe between two frames is one load at the next frame,
        // which releases the old subscription; what arrives for the old one changes nothing.
        let feed = Arc::new(Script::default());
        let mut chart = fed(&feed, Some("A"));
        frames.run(&mut chart);
        feed.resolve().send(Ok(Instrument::new("A")));
        frames.run(&mut chart);
        let (old_sink, old_page) = (feed.sink(), feed.page());
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
        old_sink.push_bar(bars(1)[0].clone());
        old_page.send(page(bars(10), false));
        frames.run(&mut chart);
        assert_eq!(chart.bars(main), [], "f: nothing of the old key");
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
    }
}
