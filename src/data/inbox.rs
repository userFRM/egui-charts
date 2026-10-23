//! Where feed answers and live updates wait for the chart.
//!
//! Pushes coalesce on arrival: ticks fold into the bar of their slot and a bar replaces the
//! pending bar of its slot, so the inbox holds the bars that really changed, however fast a
//! feed pushes. Trades wait in a store as bounded as the chart's, and depth updates apply to the
//! subscription's book.

use super::depth::{Book, Lost};
use super::merge::{Origin, fold, valid};
use super::trades::{self, Trades};
use super::{FeedError, FeedStatus, SubscriptionId};
use crate::model::{Bar, BarClock, OrderBook, Tick};
use chrono::{DateTime, Utc};
use std::any::Any;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

/// A `Result<T, FeedError>` answering a request for `T`, or a `T` pushed through an event stream.
pub(crate) type Answer = Box<dyn Any + Send>;

/// One chart's inbox, shared with every `Reply` and `Sink` it hands out.
#[derive(Default)]
pub(crate) struct Inbox {
    state: Mutex<State>,
    waker: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

#[derive(Default)]
struct State {
    /// Set by the first push after a drain, so a burst wakes the chart once.
    woken: bool,
    answers: Vec<(u64, Answer)>,
    /// Pushes queued in `answers` per event stream.
    streamed: HashMap<u64, usize>,
    subscriptions: HashMap<SubscriptionId, Live>,
}

struct Live {
    clock: BarClock,
    /// Open of the newest slot pushed, kept across drains: trades of older slots are late.
    forming: Option<DateTime<Utc>>,
    book: Book,
    taken: TakenIds,
    updates: Updates,
}

/// Ids of the trades folded into bars, as many as the trade store holds, the oldest forgotten
/// first: a feed that replays its last trades after a reconnect must not count them twice.
struct TakenIds {
    order: VecDeque<u64>,
    ids: HashSet<u64>,
    capacity: usize,
}

impl TakenIds {
    fn insert(&mut self, id: u64) {
        if self.ids.insert(id) {
            self.order.push_back(id);
            if self.order.len() > self.capacity
                && let Some(oldest) = self.order.pop_front()
            {
                self.ids.remove(&oldest);
            }
        }
    }
}

/// What arrived since the last drain.
pub(crate) struct Drained {
    /// In arrival order, each with the key of its request or event stream.
    pub(crate) answers: Vec<(u64, Answer)>,
    pub(crate) updates: Vec<(SubscriptionId, Updates)>,
}

/// What one subscription pushed since the last drain.
pub(crate) struct Updates {
    /// One per slot, keyed by its open; the last one is the forming bar.
    pub(crate) bars: BTreeMap<DateTime<Utc>, (Bar, Origin)>,
    /// The trades folded into `bars`, in arrival order.
    pub(crate) trades: Trades,
    /// The order book, if it changed.
    pub(crate) depth: Option<OrderBook>,
    /// The book lost its snapshot: call `Datafeed::request_depth_snapshot`.
    pub(crate) resnapshot: bool,
    pub(crate) status: Option<FeedStatus>,
    pub(crate) error: Option<FeedError>,
    /// Trades older than the forming bar, dropped.
    pub(crate) late_ticks: u64,
    /// Trades whose id is among those the subscription remembers, dropped.
    pub(crate) repeated_ticks: u64,
    /// Bars and ticks that failed validation, dropped.
    pub(crate) rejected: u64,
    /// Depth snapshots and deltas that failed validation, dropped.
    pub(crate) rejected_depth: u64,
}

impl Updates {
    fn new(trades: Trades) -> Self {
        Self {
            bars: BTreeMap::new(),
            trades,
            depth: None,
            resnapshot: false,
            status: None,
            error: None,
            late_ticks: 0,
            repeated_ticks: 0,
            rejected: 0,
            rejected_depth: 0,
        }
    }
}

impl Inbox {
    /// Called from the pushing thread by the first push after each drain.
    pub(crate) fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        let _ = self.waker.set(Box::new(waker));
    }

    /// A reply that delivers its answer here under `request`.
    pub(crate) fn reply<T: Send + 'static>(self: &Arc<Self>, request: u64) -> Reply<T> {
        Reply {
            inbox: Arc::clone(self),
            request,
            sent: false,
            answer: PhantomData,
        }
    }

    /// A sink for a new subscription, whose ticks fold into bars of `clock` and wait in a store of
    /// `trade_capacity`.
    pub(crate) fn subscribe(self: &Arc<Self>, clock: BarClock, trade_capacity: usize) -> Sink {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = SubscriptionId(NEXT.fetch_add(1, Ordering::Relaxed));
        let live = Live {
            clock,
            forming: None,
            book: Book::default(),
            taken: TakenIds {
                order: VecDeque::new(),
                ids: HashSet::new(),
                capacity: trade_capacity,
            },
            updates: Updates::new(Trades::new(trade_capacity)),
        };
        self.state().subscriptions.insert(id, live);
        Sink {
            inbox: Arc::clone(self),
            id,
        }
    }

    /// Discards what `id` pushed and ignores whatever it pushes from now on.
    pub(crate) fn unsubscribe(&self, id: SubscriptionId) {
        self.state().subscriptions.remove(&id);
    }

    /// Takes everything that arrived since the last drain.
    pub(crate) fn drain(&self) -> Drained {
        let mut state = self.state();
        state.woken = false;
        let answers = std::mem::take(&mut state.answers);
        state.streamed.clear();
        let updates = state
            .subscriptions
            .iter_mut()
            .map(|(id, live)| {
                let next = Updates::new(live.updates.trades.empty());
                let mut updates = std::mem::replace(&mut live.updates, next);
                updates.depth = live.book.take_changed();
                (*id, updates)
            })
            .collect();
        Drained { answers, updates }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // A panic on one feed thread must not take every other producer and the chart with it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies `push` under the lock, then wakes the chart if it is the first push since a drain.
    fn push(&self, push: impl FnOnce(&mut State) -> bool) {
        let wake = {
            let mut state = self.state();
            push(&mut state) && !std::mem::replace(&mut state.woken, true)
        };
        if wake && let Some(waker) = self.waker.get() {
            waker();
        }
    }

    fn answer<T: Send + 'static>(&self, request: u64, result: Result<T, FeedError>) {
        self.push(|state| {
            state.answers.push((request, Box::new(result)));
            true
        });
    }

    /// Queues `event` under `key` while `active` holds, up to 10,000 per drain. A push over the
    /// bound is dropped and clears `active`: the stream lagged.
    pub(crate) fn push_answer(&self, key: u64, event: Answer, active: &AtomicBool) {
        self.push(|state| {
            if !active.load(Ordering::Relaxed) {
                return false;
            }
            let queued = state.streamed.entry(key).or_default();
            if *queued == STREAM_BOUND {
                active.store(false, Ordering::Relaxed);
                return false;
            }
            *queued += 1;
            state.answers.push((key, event));
            true
        });
    }
}

/// Pushes one event stream may queue between two drains.
const STREAM_BOUND: usize = 10_000;

/// The answer to one request, usable from any thread. Dropping it unanswered reports
/// [`FeedError::Unanswered`], so a lost request never leaves the chart loading forever.
#[must_use = "answer the request with Reply::send"]
pub struct Reply<T: Send + 'static> {
    inbox: Arc<Inbox>,
    request: u64,
    sent: bool,
    answer: PhantomData<fn(T)>,
}

impl<T: Send + 'static> Reply<T> {
    /// Delivers `result` to the chart.
    pub fn send(mut self, result: Result<T, FeedError>) {
        self.sent = true;
        self.inbox.answer(self.request, result);
    }
}

impl<T: Send + 'static> Drop for Reply<T> {
    fn drop(&mut self) {
        if !self.sent {
            self.inbox
                .answer(self.request, Err::<T, _>(FeedError::Unanswered));
        }
    }
}

/// Live updates for one subscription. Cheap to clone and usable from any thread; pushes never
/// wait for the chart to catch up.
///
/// ```
/// use chrono::Utc;
/// use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
/// use egui_charts::model::{OrderBook, OrderBookLevel, Tick, TradeSide};
///
/// struct Exchange;
///
/// impl Datafeed for Exchange {
///     fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
///         reply.send(Ok(HistoryPage::default()));
///     }
///
///     /// A real feed pushes from its connection's thread or task.
///     fn subscribe(&self, subscription: Subscription) {
///         let sink = subscription.sink;
///         let level = |price, size| OrderBookLevel { price, size, order_cnt: None };
///         let symbol = subscription.symbol;
///         let book = |bids, asks| OrderBook { ts: Utc::now(), symbol: symbol.clone(), bids, asks };
///
///         sink.push_depth_snapshot(7, book(vec![level(99.75, 3.0)], vec![level(100.0, 2.0)]));
///         let side = Some(TradeSide::Buy);
///         sink.push_tick(Tick::new(Utc::now(), 100.0, 2.0, side).with_id(41_207));
///         // The buy took the whole offer at 100.
///         sink.push_depth_delta(8, book(vec![], vec![level(100.0, 0.0)]));
///     }
/// }
/// ```
#[derive(Clone)]
pub struct Sink {
    inbox: Arc<Inbox>,
    pub(crate) id: SubscriptionId,
}

impl Sink {
    /// A bar of the subscribed timeframe, stamped with its open or any instant inside it:
    /// replaces the bar it shares a slot with, appends a newer one, corrects an older one. A bar
    /// of a day or longer is stamped inside its UTC period, as [`HistoryPage::bars`] describes.
    ///
    /// [`HistoryPage::bars`]: super::HistoryPage::bars
    pub fn push_bar(&self, bar: Bar) {
        self.update(|live| {
            if !valid(&bar) {
                live.updates.rejected += 1;
                return;
            }
            let slot = live.clock.bar_open(bar.time);
            live.forming = live.forming.max(Some(slot));
            live.updates.bars.insert(slot, (bar, Origin::Live));
        });
    }

    /// A trade, folded into the bar of its slot and kept for order flow, the oldest trades making
    /// room for new ones. Its size is added to that bar's volume, also when the bar came from a
    /// history page. A feed must therefore not push trades that its history already counts; a feed
    /// that cannot tell pushes bars instead. Trades older than the forming bar are dropped, and so is
    /// a trade whose id is among those of the last trades the subscription folded, as many as its
    /// trade store holds.
    pub fn push_tick(&self, tick: Tick) {
        self.update(|live| {
            let (open, price) = (live.clock.bar_open(tick.ts), tick.price);
            let ticks = Bar::new(open, price, price, price, price, tick.size);
            let updates = &mut live.updates;
            if !trades::valid(&tick) {
                updates.rejected += 1;
            } else if live.forming.is_some_and(|forming| open < forming) {
                updates.late_ticks += 1;
            } else if tick.id.is_some_and(|id| live.taken.ids.contains(&id)) {
                updates.repeated_ticks += 1;
            } else {
                live.forming = Some(open);
                match updates.bars.entry(open) {
                    Entry::Occupied(mut pending) => {
                        let (bar, origin) = pending.get_mut();
                        let Some(folded) = fold(bar, &ticks) else {
                            updates.rejected += 1;
                            return;
                        };
                        *bar = folded;
                        if let Origin::Ticks(trades) = origin {
                            *trades += 1;
                        }
                    }
                    Entry::Vacant(slot) => {
                        slot.insert((ticks, Origin::Ticks(1)));
                    }
                }
                if let Some(id) = tick.id {
                    live.taken.insert(id);
                }
                updates.trades.push(tick);
            }
        });
    }

    /// The whole order book, numbered `sequence`, at most 10,000 levels per side: a deeper book is
    /// rejected. It replaces the book; the next delta carries `sequence + 1`.
    pub fn push_depth_snapshot(&self, sequence: u64, book: OrderBook) {
        self.update(|live| live.depth(|depth| depth.snapshot(sequence, book)));
    }

    /// The levels that changed since the update numbered `sequence - 1`, each with its new size; a
    /// size of 0 removes the level. The `symbol` of `changes` is ignored. A delta numbered at or
    /// below the last one applied is ignored. After a gap, before the first snapshot, or when a
    /// side would exceed 10,000 levels, the book is cleared and the chart calls
    /// [`Datafeed::request_depth_snapshot`] once; a snapshot pushed before the chart sees the loss
    /// cancels the call. Keep pushing deltas while the snapshot is fetched: those pushed while the
    /// book waits are held, the oldest dropped beyond 10,000 levels in all, and the snapshot
    /// applies the ones numbered above its own sequence.
    ///
    /// [`Datafeed::request_depth_snapshot`]: super::Datafeed::request_depth_snapshot
    pub fn push_depth_delta(&self, sequence: u64, changes: OrderBook) {
        self.update(|live| live.depth(|depth| depth.delta(sequence, changes)));
    }

    /// The connection status. The latest one wins.
    pub fn set_status(&self, status: FeedStatus) {
        self.update(|live| live.updates.status = Some(status));
    }

    /// An error of the live stream. The latest one wins.
    pub fn report_error(&self, error: FeedError) {
        self.update(|live| live.updates.error = Some(error));
    }

    /// False once the subscription ended. Pushes are ignored from then on.
    pub fn is_active(&self) -> bool {
        self.inbox.state().subscriptions.contains_key(&self.id)
    }

    fn update(&self, update: impl FnOnce(&mut Live)) {
        self.inbox
            .push(|state| match state.subscriptions.get_mut(&self.id) {
                Some(live) => {
                    update(live);
                    true
                }
                None => false,
            });
    }
}

impl Live {
    fn depth(&mut self, apply: impl FnOnce(&mut Book) -> Result<(), Lost>) {
        match apply(&mut self.book) {
            // The book holds a snapshot newer than any loss, so a pending request is moot.
            Ok(()) => self.updates.resnapshot = false,
            Err(lost) => {
                self.updates.rejected_depth += u64::from(lost.invalid);
                self.updates.resnapshot |= lost.request;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::HistoryPage;
    use crate::model::{OrderBookLevel, Timeframe};
    use chrono::{DateTime, TimeDelta, Utc};
    use std::sync::atomic::AtomicUsize;

    /// `minute` and `second` after 10:00.
    fn at(minute: i64, second: i64) -> DateTime<Utc> {
        "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap()
            + TimeDelta::minutes(minute)
            + TimeDelta::seconds(second)
    }

    fn tick(ts: DateTime<Utc>, price: f64, size: f64) -> Tick {
        Tick::new(ts, price, size, None)
    }

    /// The one-minute bar opening `minute` minutes after 10:00.
    fn bar(minute: i64, [open, high, low, close, volume]: [f64; 5]) -> Bar {
        Bar::new(at(minute, 0), open, high, low, close, volume)
    }

    fn subscribe(inbox: &Arc<Inbox>) -> Sink {
        inbox.subscribe(BarClock::new(Timeframe::Min1), 100)
    }

    /// Guards coalescing: folding must keep the highs and lows of every trade, a pushed bar must
    /// replace the forming bar, late and invalid trades must be dropped from the bars and the trade
    /// store alike, a lost book must reach the chart as one snapshot request unless the feed healed
    /// it first, and the latest status and error must win.
    #[test]
    fn pushes_coalesce_into_bars() {
        let inbox = Arc::new(Inbox::default());
        let sink = subscribe(&inbox);
        sink.push_tick(tick(at(0, 5), 100.0, 1.0));
        sink.push_tick(tick(at(0, 20), 103.0, 2.0));
        sink.push_tick(tick(at(0, 40), 99.0, 1.0));
        sink.push_tick(tick(at(0, 50), f64::NAN, 1.0)); // invalid
        sink.push_tick(tick(at(1, 10), 101.0, 3.0)); // opens the 10:01 bar
        sink.push_tick(tick(at(0, 59), 104.0, 1.0)); // late
        sink.push_bar(bar(1, [101.0, 102.0, 100.0, 101.5, 50.0])); // replaces the forming bar
        sink.push_tick(tick(at(1, 30), 105.0, 1.0));
        sink.push_bar(bar(-2, [98.0, 99.0, 97.0, 98.5, 40.0])); // older than everything pending
        let book = |price| OrderBook {
            ts: at(1, 30),
            symbol: "ES".into(),
            bids: vec![OrderBookLevel {
                price,
                size: 1.0,
                order_cnt: None,
            }],
            asks: Vec::new(),
        };
        sink.push_depth_snapshot(5, book(99.0));
        sink.push_depth_delta(6, book(f64::NAN)); // invalid: the book is lost
        sink.set_status(FeedStatus::Connecting);
        sink.set_status(FeedStatus::Live);
        sink.report_error(FeedError::Network("reset".into()));
        sink.report_error(FeedError::Network("timeout".into()));

        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(
            Vec::from_iter(updates.bars.into_values()),
            [
                (bar(-2, [98.0, 99.0, 97.0, 98.5, 40.0]), Origin::Live),
                (bar(0, [100.0, 103.0, 99.0, 99.0, 4.0]), Origin::Ticks(3)),
                (bar(1, [101.0, 105.0, 100.0, 105.0, 51.0]), Origin::Live),
            ]
        );
        let prices = Vec::from_iter(updates.trades.iter().map(|trade| trade.price));
        assert_eq!(prices, [100.0, 103.0, 99.0, 101.0, 105.0]);
        let depth = updates.depth.map(|book| book.bids.len());
        assert_eq!(
            (depth, updates.resnapshot, updates.rejected_depth),
            (Some(0), true, 1)
        );
        assert_eq!((updates.late_ticks, updates.rejected), (1, 1));
        assert_eq!(updates.status, Some(FeedStatus::Live));
        assert_eq!(updates.error, Some(FeedError::Network("timeout".into())));

        sink.push_depth_snapshot(7, book(98.0));
        sink.push_depth_delta(9, book(97.0)); // a gap
        sink.push_depth_snapshot(10, book(96.0)); // healed before the chart saw the gap
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let depth = updates.depth.map(|book| book.bids[0].price);
        assert_eq!((depth, updates.resnapshot), (Some(96.0), false));
    }

    /// Guards the memory bound and the repaint rate: a queue of raw updates or an unbounded trade
    /// store grows with the update rate, a wake per push floods the UI, and a waker that a drain
    /// does not re-arm stalls the chart.
    #[test]
    fn a_burst_holds_one_bar_and_wakes_once() {
        let inbox = Arc::new(Inbox::default());
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        inbox.set_waker(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let sink = subscribe(&inbox);
        inbox.drain(); // the burst lands in the store a drain left behind
        let producer = sink.clone();
        std::thread::spawn(move || {
            for i in 0..10_000 {
                let price = 100.0 + (i % 7) as f64;
                producer.push_tick(tick(at(0, 0) + TimeDelta::milliseconds(5 * i), price, 1.0));
            }
        })
        .join()
        .unwrap();
        assert_eq!(wakes.load(Ordering::Relaxed), 1);

        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(updates.trades.iter().count(), 100);
        let bar = bar(0, [100.0, 106.0, 100.0, 103.0, 10_000.0]);
        assert_eq!(
            Vec::from_iter(updates.bars.into_values()),
            [(bar, Origin::Ticks(10_000))]
        );
        sink.push_tick(tick(at(0, 59), 100.0, 1.0));
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
    }

    /// Guards the trade id contract: a feed replaying its last trades after a reconnect must not
    /// count them twice in a bar, within one drain or across drains.
    #[test]
    fn a_trade_id_is_folded_once() {
        let inbox = Arc::new(Inbox::default());
        let sink = subscribe(&inbox);
        let trade = tick(at(0, 5), 100.0, 1.0).with_id(7);
        sink.push_tick(trade.clone());
        sink.push_tick(trade.clone());
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(
            Vec::from_iter(updates.bars.into_values()),
            [(bar(0, [100.0, 100.0, 100.0, 100.0, 1.0]), Origin::Ticks(1))]
        );
        assert_eq!(updates.trades.iter().count(), 1);
        assert_eq!(updates.repeated_ticks, 1);

        sink.push_tick(trade);
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(updates.bars, BTreeMap::new());
        assert_eq!(updates.trades.iter().count(), 0);
        assert_eq!(updates.repeated_ticks, 1);

        // A store of two trades remembers the last two ids: 1 is forgotten once 3 is folded.
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe(BarClock::new(Timeframe::Min1), 2);
        for (second, id) in [(1, 1), (2, 2), (3, 3), (4, 1), (5, 3)] {
            sink.push_tick(tick(at(0, second), 100.0, 1.0).with_id(id));
        }
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(
            Vec::from_iter(updates.bars.into_values()),
            [(bar(0, [100.0, 100.0, 100.0, 100.0, 4.0]), Origin::Ticks(4))]
        );
        assert_eq!(updates.repeated_ticks, 1);
    }

    /// Guards against updates of a replaced subscription (an older symbol or timeframe) leaking
    /// into the series that replaced it.
    #[test]
    fn an_ended_subscription_is_ignored() {
        let inbox = Arc::new(Inbox::default());
        let old = subscribe(&inbox);
        old.push_tick(tick(at(0, 5), 100.0, 1.0));
        let new = subscribe(&inbox);
        inbox.unsubscribe(old.id);
        old.push_tick(tick(at(0, 10), 101.0, 1.0));
        new.push_tick(tick(at(0, 10), 200.0, 1.0));
        assert!(!old.is_active());
        assert!(new.is_active());

        let drained = inbox.drain().updates;
        let ids: Vec<_> = drained.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [new.id]);
        let bar = bar(0, [200.0, 200.0, 200.0, 200.0, 1.0]);
        assert_eq!(
            Vec::from_iter(drained[0].1.bars.values().cloned()),
            [(bar, Origin::Ticks(1))]
        );
    }

    /// Guards the one-shot contract: a lost request must not leave the chart loading forever,
    /// and an answered one must not also report `Unanswered` when dropped.
    #[test]
    fn a_reply_answers_once_and_a_dropped_one_reports_unanswered() {
        let inbox = Arc::new(Inbox::default());
        let page = HistoryPage::default();
        inbox.reply(1).send(Ok(page.clone()));
        let lost = inbox.reply::<HistoryPage>(2);
        std::thread::spawn(move || drop(lost)).join().unwrap();

        let answers: Vec<_> = inbox
            .drain()
            .answers
            .into_iter()
            .map(|(request, answer)| {
                let answer = answer.downcast::<Result<HistoryPage, FeedError>>();
                (request, *answer.unwrap())
            })
            .collect();
        assert_eq!(answers, [(1, Ok(page)), (2, Err(FeedError::Unanswered))]);
    }
}
