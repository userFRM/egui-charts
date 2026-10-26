//! Where feed answers and live updates wait for the chart.
//!
//! Pushes coalesce on arrival: ticks fold into the bar of their slot and a bar replaces the
//! pending bar of its slot, so the inbox holds the bars that really changed, however fast a
//! feed pushes. Trades wait in a store as bounded as the chart's, and depth updates apply to the
//! subscription's book.

use super::depth::{Book, Lost};
use super::depth_history::{DepthColumn, DepthHistoryConfig, Sampler};
use super::merge::{Origin, fold, valid};
use super::trades::{self, Trades};
use super::{FeedError, FeedStatus, SubscriptionId};
use crate::model::{Bar, BarClock, Level1Quote, OrderBook, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::any::Any;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

/// A `Result<T, FeedError>` answering a request for `T`, or a `T` pushed through an event stream.
pub(crate) type Answer = Box<dyn Any + Send>;

/// Rebuilds bars from chronological executions using the live sink's aggregation rules.
/// Count intervals start at the first accepted trade in `trades`; retain that grouping anchor
/// when joining history to live data. Duplicate ids are ignored. Invalid or late trades and
/// overflowing volumes return an error. Periods without executions are omitted.
///
/// ```
/// use egui_charts::data::aggregate_trades;
/// use egui_charts::model::Tick;
/// use chrono::Utc;
/// let trades = vec![Tick::new(Utc::now(), 100.0, 2.0, None)];
/// let bars = aggregate_trades(&trades, "10t".parse::<egui_charts::model::Timeframe>().unwrap()).unwrap();
/// assert_eq!(bars[0].volume, 2.0);
/// ```
pub fn aggregate_trades(
    trades: &[Tick],
    clock: impl Into<BarClock>,
) -> Result<Vec<Bar>, FeedError> {
    let Some(last) = trades.last() else {
        return Ok(Vec::new());
    };
    let inbox = Arc::new(Inbox::default());
    let sink = inbox.subscribe(String::new(), clock.into(), trades.len(), None);
    sink.set_server_time(last.ts);
    for trade in trades {
        sink.push_tick(trade.clone());
    }
    let (_, updates) = inbox.drain().updates.pop().expect("active subscription");
    if updates.rejected > 0 || updates.late_ticks > 0 {
        return Err(FeedError::InvalidData(
            "invalid or unordered executions".into(),
        ));
    }
    Ok(updates.bars.into_values().map(|(bar, _)| bar).collect())
}

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
    symbol: String,
    clock: BarClock,
    /// Open of the newest slot pushed, kept across drains: trades of older slots are late.
    forming: Option<DateTime<Utc>>,
    /// Feed clock minus system clock, as last reported; zero until then.
    offset: TimeDelta,
    count_bar: Option<(Bar, u64)>,
    last_trade: Option<DateTime<Utc>>,
    last_close: Option<f64>,
    empty: bool,
    book: Book,
    /// Samples the book for the chart's depth history, when it keeps one.
    sampler: Option<Sampler>,
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
    /// Columns sampled for the depth history, oldest first, as many as its newest resolution
    /// holds: the oldest make room for new ones.
    pub(crate) depth_columns: VecDeque<DepthColumn>,
    /// Latest top-of-book quote pushed in this frame.
    pub(crate) quote: Option<Level1Quote>,
    /// The book lost its snapshot: call `Datafeed::request_depth_snapshot`.
    pub(crate) resnapshot: bool,
    pub(crate) symbols_changed: bool,
    pub(crate) status: Option<FeedStatus>,
    pub(crate) error: Option<FeedError>,
    /// Trades older than the forming bar, dropped.
    pub(crate) late_ticks: u64,
    /// Trades whose id is among those the subscription remembers, dropped.
    pub(crate) repeated_ticks: u64,
    /// Bars and ticks that failed validation or were stamped too far ahead of the feed's clock,
    /// dropped.
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
            depth_columns: VecDeque::new(),
            quote: None,
            resnapshot: false,
            symbols_changed: false,
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
    /// Current time in a subscription's server clock, including between pushes.
    #[cfg(feature = "widget")]
    pub(crate) fn server_time(&self, id: SubscriptionId) -> Option<DateTime<Utc>> {
        Utc::now().checked_add_signed(self.state().subscriptions.get(&id)?.offset)
    }

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
    /// `trade_capacity`, and whose book keys prices by integer ticks of `tick_size`.
    pub(crate) fn subscribe(
        self: &Arc<Self>,
        symbol: String,
        clock: BarClock,
        trade_capacity: usize,
        tick_size: Option<f64>,
    ) -> Sink {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = SubscriptionId(NEXT.fetch_add(1, Ordering::Relaxed));
        let live = Live {
            symbol,
            clock,
            forming: None,
            offset: TimeDelta::zero(),
            count_bar: None,
            last_trade: None,
            last_close: None,
            empty: false,
            book: Book::new(tick_size),
            sampler: None,
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

    /// Samples the book of `id` into depth history columns from its next update, or stops. The
    /// columns sampled before and not yet drained are dropped.
    pub(crate) fn sample_depth(&self, id: SubscriptionId, config: Option<DepthHistoryConfig>) {
        if let Some(live) = self.state().subscriptions.get_mut(&id) {
            live.sampler = config.map(Sampler::new);
            live.updates.depth_columns.clear();
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
///         sink.push_depth_delta(8, &book(vec![], vec![level(100.0, 0.0)]));
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
    /// of a day or longer is stamped inside its calendar period, as [`HistoryPage::bars`] describes. A
    /// bar stamped too far ahead of the feed's current time is rejected, as
    /// [`Sink::set_server_time`] describes.
    ///
    /// [`HistoryPage::bars`]: super::HistoryPage::bars
    pub fn push_bar(&self, bar: Bar) {
        let system = Utc::now();
        self.update(|live| {
            let slot = live.clock.bar_open(bar.time);
            if !valid(&bar) || live.ahead(slot, system) {
                live.updates.rejected += 1;
                return;
            }
            if matches!(
                live.clock.timeframe(),
                Some(crate::model::Timeframe::Ticks(_))
            ) && live.forming.is_none_or(|last| slot >= last)
            {
                live.count_bar = None;
                live.last_trade = Some(slot);
            }
            if live.forming.is_none_or(|last| slot >= last) {
                live.last_close = Some(bar.close);
                live.empty = false;
            }
            live.forming = live.forming.max(Some(slot));
            live.updates.bars.insert(slot, (bar, Origin::Live));
        });
    }

    /// A trade, folded into the bar of its slot and kept for order flow, the oldest trades making
    /// room for new ones. Its size is added to that bar's volume, also when the bar came from a
    /// history page. A feed must therefore not push trades that its history already counts; a feed
    /// that cannot tell pushes bars instead. Trades older than the forming bar are dropped, and so is
    /// a trade whose id is among those of the last trades the subscription folded, as many as its
    /// trade store holds. A trade stamped too far ahead of the feed's current time is rejected, as
    /// [`Sink::set_server_time`] describes.
    ///
    /// Count intervals start at the first accepted execution of the subscription. Replay the
    /// retained executions on resubscription to rebuild a partial bar. Count bars use the first
    /// trade's time; if that would duplicate a bar time, it advances by one nanosecond. Original
    /// execution times remain unchanged. A pushed count bar must be complete: the next trade
    /// starts a fresh bar. History pages should contain complete count bars only.
    pub fn push_tick(&self, tick: Tick) {
        let system = Utc::now();
        self.update(|live| {
            if let Some(crate::model::Timeframe::Ticks(count)) = live.clock.timeframe() {
                if !trades::valid(&tick) || live.ahead(tick.ts, system) {
                    live.updates.rejected += 1;
                } else if tick.id.is_some_and(|id| live.taken.ids.contains(&id)) {
                    live.updates.repeated_ticks += 1;
                } else if live.last_trade.is_some_and(|last| tick.ts < last) {
                    live.updates.late_ticks += 1;
                } else {
                    let starts = live
                        .count_bar
                        .as_ref()
                        .is_none_or(|(_, n)| *n == count.get());
                    let open = if starts {
                        match live.forming.filter(|last| *last >= tick.ts) {
                            Some(last) => {
                                match last.checked_add_signed(TimeDelta::nanoseconds(1)) {
                                    Some(time) => time,
                                    None => {
                                        live.updates.rejected += 1;
                                        return;
                                    }
                                }
                            }
                            None => tick.ts,
                        }
                    } else {
                        live.count_bar.as_ref().unwrap().0.time
                    };
                    let trade = Bar::new(
                        open, tick.price, tick.price, tick.price, tick.price, tick.size,
                    );
                    let (bar, n) = if starts {
                        (trade, 1)
                    } else {
                        let (old, n) = live.count_bar.as_ref().unwrap();
                        let Some(bar) = fold(old, &trade) else {
                            live.updates.rejected += 1;
                            return;
                        };
                        (bar, n + 1)
                    };
                    live.forming = Some(open);
                    live.last_trade = Some(tick.ts);
                    live.count_bar = Some((bar.clone(), n));
                    // A full snapshot survives drains without adding the same trades twice.
                    live.updates.bars.insert(open, (bar, Origin::Live));
                    if let Some(id) = tick.id {
                        live.taken.insert(id);
                    }
                    live.updates.trades.push(tick);
                }
                return;
            }
            let (open, price) = (live.clock.bar_open(tick.ts), tick.price);
            let ticks = Bar::new(open, price, price, price, price, tick.size);
            let ahead = live.ahead(open, system);
            let updates = &mut live.updates;
            if !trades::valid(&tick) || ahead {
                updates.rejected += 1;
            } else if live.forming.is_some_and(|forming| open < forming) {
                updates.late_ticks += 1;
            } else if tick.id.is_some_and(|id| live.taken.ids.contains(&id)) {
                updates.repeated_ticks += 1;
            } else {
                let replace_empty = live.empty && live.forming == Some(open);
                match updates.bars.entry(open) {
                    Entry::Occupied(mut pending) if replace_empty => {
                        pending.insert((ticks, Origin::Live));
                    }
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
                        slot.insert((
                            ticks,
                            if replace_empty {
                                Origin::Live
                            } else {
                                Origin::Ticks(1)
                            },
                        ));
                    }
                }
                live.last_close = Some(price);
                live.empty = false;
                live.forming = Some(open);
                if let Some(id) = tick.id {
                    live.taken.insert(id);
                }
                updates.trades.push(tick);
            }
        });
    }

    /// Advances a time-based stream through `now`, opening an empty forming candle at the
    /// current clock boundary when there has been no execution. Empty candles carry the last
    /// close with zero volume; their first trade sets the real open, high and low. Count bars
    /// never advance without trades. Call from a trusted feed heartbeat, after seeding a bar or
    /// execution; invalid or future clock values obey the same rule as trade timestamps.
    pub fn advance_time(&self, now: DateTime<Utc>) {
        let system = Utc::now();
        self.inbox.push(|state| {
            let Some(live) = state.subscriptions.get_mut(&self.id) else {
                return false;
            };
            if live
                .clock
                .timeframe()
                .is_none_or(|tf| tf.duration_ms() <= 0)
            {
                return false;
            }
            let open = live.clock.bar_open(now);
            let Some(close) = live.last_close else {
                return false;
            };
            if live.forming.is_none_or(|last| open <= last) || live.ahead(open, system) {
                return false;
            }
            live.updates.bars.insert(
                open,
                (
                    Bar::new(open, close, close, close, close, 0.0),
                    Origin::Ticks(0),
                ),
            );
            live.forming = Some(open);
            live.empty = true;
            true
        });
    }

    /// The whole order book, numbered `sequence`, at most 10,000 levels per side: a deeper book is
    /// rejected. It replaces the book; the next delta carries `sequence + 1`. Levels are keyed by
    /// the nearest integer tick of the instrument's [`tick_size`](crate::model::Instrument::tick_size),
    /// when it has one, so prices that differ by a rounding error are one level and the latest
    /// size pushed for it holds. A crossed book is kept as pushed.
    pub fn push_depth_snapshot(&self, sequence: u64, book: OrderBook) {
        self.update(|live| live.depth(book.ts, |depth| depth.snapshot(sequence, book)));
    }

    /// The levels that changed since the update numbered `sequence - 1`, each with its new size; a
    /// size of 0 removes the level. The `symbol` of `changes` is ignored. Taken by reference, so a
    /// feed can clear and refill one `OrderBook` for every delta instead of allocating its levels
    /// per message: the book itself allocates nothing for a delta once its sides reached their
    /// depth. A delta numbered at or
    /// below the last one applied is ignored. After a gap, before the first snapshot, or when a
    /// side would exceed 10,000 levels, the book is cleared and the chart calls
    /// [`Datafeed::request_depth_snapshot`] once; a snapshot pushed before the chart sees the loss
    /// cancels the call. Keep pushing deltas while the snapshot is fetched: those pushed while the
    /// book waits are held, the oldest dropped beyond 10,000 levels in all, and the snapshot
    /// applies the ones numbered above its own sequence.
    ///
    /// [`Datafeed::request_depth_snapshot`]: super::Datafeed::request_depth_snapshot
    pub fn push_depth_delta(&self, sequence: u64, changes: &OrderBook) {
        self.update(|live| live.depth(changes.ts, |depth| depth.delta(sequence, changes)));
    }

    /// Latest best bid and ask. Bursts coalesce to the newest timestamp (last arrival on a tie).
    /// Non-finite prices and negative or non-finite sizes are rejected without replacing the
    /// pending quote. A crossed quote, its bid above its ask, is kept, as the top of a crossed
    /// book (see [`OrderBook::is_crossed`]). Times obey the feed-clock rule of [`Self::set_server_time`].
    /// The subscription supplies the instrument identity; the
    /// sink ignores a quote whose symbol differs from its subscribed symbol.
    pub fn push_quote(&self, quote: Level1Quote) {
        let system = Utc::now();
        self.update(|live| {
            if quote.symbol != live.symbol {
                return;
            }
            if !quote.bid.is_finite()
                || !quote.ask.is_finite()
                || !quote.bid_size.is_finite()
                || !quote.ask_size.is_finite()
                || quote.bid_size < 0.0
                || quote.ask_size < 0.0
                || live.ahead(live.clock.bar_open(quote.ts), system)
            {
                live.updates.error =
                    Some(FeedError::InvalidData("invalid best bid/ask quote".into()));
            } else if live
                .updates
                .quote
                .as_ref()
                .is_none_or(|old| quote.ts >= old.ts)
            {
                live.updates.quote = Some(quote);
            }
        });
    }

    /// The feed's current time, from a clock other than the times it pushes, such as its server's
    /// heartbeat or time message: reporting the time of the trade or bar about to be pushed would
    /// let a wrong time through. The time runs on with the system clock until the next call; until
    /// the first, the system clock stands in.
    ///
    /// A bar or trade is rejected when its slot opens more than one slot after the slot of this
    /// time and more than a minute after this time, or more than two weeks for bars of a day or
    /// longer, which a feed stamps on their trading date. A time in the wrong unit, such as
    /// milliseconds read as seconds, therefore cannot make every later trade late, while a system
    /// clock a few seconds slow rejects nothing. A replay or simulation calls this before its first
    /// push, whenever its clock jumps, and at least once per bar while its clock runs at another
    /// speed.
    pub fn set_server_time(&self, now: DateTime<Utc>) {
        let offset = now - Utc::now();
        // Not a push: the chart shows nothing new, so it is not woken.
        if let Some(live) = self.inbox.state().subscriptions.get_mut(&self.id) {
            live.offset = offset;
        }
    }

    /// The connection status. The latest one wins.
    pub fn set_status(&self, status: FeedStatus) {
        self.update(|live| live.updates.status = Some(status));
    }

    /// An error of the live stream. The latest one wins.
    pub fn report_error(&self, error: FeedError) {
        self.update(|live| live.updates.error = Some(error));
    }

    /// Notifies the host that its symbol search catalog should be refreshed. Coalesces within
    /// a frame; the chart exposes arrivals through `FeedState::symbol_catalog_revision`.
    pub fn symbols_changed(&self) {
        self.update(|live| live.updates.symbols_changed = true);
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
    /// Whether the slot opening at `open` is too far ahead of the feed's current time, `system`
    /// being the system clock's, by the rule of [`Sink::set_server_time`].
    fn ahead(&self, open: DateTime<Utc>, system: DateTime<Utc>) -> bool {
        let Some(now) = system.checked_add_signed(self.offset) else {
            return false;
        };
        // A system clock may lag the exchange by seconds, and a bar of a day or longer is stamped
        // on its trading date, which can be days ahead. A clock of unknown timeframe gets the
        // longer allowance.
        let intraday = self
            .clock
            .timeframe()
            .is_some_and(|tf| tf.duration_ms() < TimeDelta::days(1).num_milliseconds());
        let allowance = if intraday {
            TimeDelta::minutes(1)
        } else {
            TimeDelta::weeks(2)
        };
        let beyond = now
            .checked_add_signed(allowance)
            .is_some_and(|limit| open > limit);
        // The slot before `open`'s is later than now's: `open` is two or more slots ahead.
        beyond
            && open
                .checked_sub_signed(TimeDelta::milliseconds(1))
                .is_some_and(|before| self.clock.bar_open(before) > self.clock.bar_open(now))
    }

    /// Applies a depth update stamped `ts`, sampling the book as it stood before when the update
    /// opens a new period of the depth history. An update stamped more than a minute after the
    /// feed's time samples nothing: it would hold the open period, and every later column, until
    /// that time passed.
    fn depth(&mut self, ts: DateTime<Utc>, apply: impl FnOnce(&mut Book) -> Result<(), Lost>) {
        if let Some(sampler) = &mut self.sampler
            && (Utc::now().checked_add_signed(self.offset + TimeDelta::minutes(1)))
                .is_some_and(|limit| ts <= limit)
            && let Some(column) = sampler.sample(ts, &self.book)
        {
            let columns = &mut self.updates.depth_columns;
            if columns.len() == sampler.config.columns {
                columns.pop_front();
            }
            columns.push_back(column);
        }
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
        inbox.subscribe("ES".into(), BarClock::new(Timeframe::Min1), 100, None)
    }

    /// Count bars must keep their progress across drains, count only accepted trades, and
    /// separate bars even when the exchange stamps several executions at the same instant.
    #[test]
    fn count_bars_follow_accepted_trades_across_frames() {
        for count in [1, 10, 100, 1000] {
            let timeframe = format!("{count}t").parse().unwrap();
            let inbox = Arc::new(Inbox::default());
            let clock = BarClock::new(timeframe);
            let sink = inbox.subscribe("ES".into(), clock.clone(), 10_000, None);
            let mut bars = BTreeMap::new();
            let mut merged = Vec::new();
            let mut from_ticks = std::collections::BTreeSet::new();
            for i in 0..(count * 2 + 1) {
                let trade =
                    tick(at(0, 5), 100.0 + (i % 3) as f64, (i % 4 + 1) as f64).with_id(i as u64);
                sink.push_tick(trade.clone());
                sink.push_tick(trade); // Replays must not consume a count.
                sink.push_tick(tick(at(0, 5), f64::NAN, 1.0));
                sink.advance_time(at(2, 0)); // A clock heartbeat never consumes a count.
                let (_, updates) = inbox.drain().updates.pop().unwrap();
                super::super::merge::merge(
                    &mut merged,
                    &mut from_ticks,
                    &clock,
                    updates.bars.values().cloned().collect(),
                );
                bars.extend(updates.bars.into_iter().map(|(t, (bar, _))| (t, bar)));
                assert_eq!(updates.repeated_ticks, 1);
                assert_eq!(updates.rejected, 1);
            }
            assert_eq!(merged, bars.values().cloned().collect::<Vec<_>>());
            assert_eq!(bars.len(), 3, "{count}");
            for (group, bar) in bars.values().enumerate() {
                let start = group * count;
                let end = (start + count).min(count * 2 + 1);
                let prices: Vec<_> = (start..end).map(|i| 100.0 + (i % 3) as f64).collect();
                assert_eq!(
                    [bar.open, bar.high, bar.low, bar.close, bar.volume],
                    [
                        prices[0],
                        prices.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        prices.iter().copied().fold(f64::INFINITY, f64::min),
                        *prices.last().unwrap(),
                        (start..end).map(|i| (i % 4 + 1) as f64).sum()
                    ],
                    "{count}, group {group}",
                );
            }
        }
    }

    /// The first trade on a boundary belongs to the next bar, and a quiet interval opens
    /// without trades. Its first execution replaces the carried close, even across drains.
    #[test]
    fn clock_boundaries_and_empty_intervals() {
        for timeframe in [Timeframe::Sec1, Timeframe::Min1, "45s".parse().unwrap()] {
            let clock = BarClock::new(timeframe);
            let inbox = Arc::new(Inbox::default());
            let sink = inbox.subscribe("ES".into(), clock.clone(), 100, None);
            let start = at(0, 0);
            let boundary = start + timeframe.duration();
            sink.push_tick(tick(start, 100.0, 2.0));
            sink.push_tick(tick(boundary - TimeDelta::milliseconds(1), 103.0, 3.0));
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let first = &updates.bars[&start].0;
            assert_eq!(
                [first.open, first.high, first.low, first.close, first.volume],
                [100.0, 103.0, 100.0, 103.0, 5.0]
            );
            sink.advance_time(boundary);
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            assert_eq!(
                updates.bars[&boundary].0,
                Bar::new(boundary, 103.0, 103.0, 103.0, 103.0, 0.0)
            );
            sink.push_tick(tick(boundary, 99.0, 4.0));
            sink.push_tick(tick(boundary + TimeDelta::milliseconds(1), 101.0, 5.0));
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            assert_eq!(
                updates.bars[&boundary],
                (
                    Bar::new(boundary, 99.0, 101.0, 99.0, 101.0, 9.0),
                    Origin::Live
                )
            );
        }
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
        sink.push_depth_delta(6, &book(f64::NAN)); // invalid: the book is lost
        let quote = |second, bid| Level1Quote {
            ts: at(1, second),
            symbol: "ES".into(),
            bid,
            ask: 101.0,
            bid_size: 2.0,
            ask_size: 3.0,
        };
        sink.push_quote(quote(0, 99.0));
        sink.push_quote(quote(1, 100.0));
        sink.push_quote(quote(0, 98.0)); // older must not replace the latest
        sink.push_quote(quote(1, 102.0)); // crossed, as in an auction: kept
        sink.push_quote(quote(2, f64::NAN)); // invalid must not replace it either
        let mut future = quote(2, 100.0);
        future.ts = DateTime::from_timestamp(1_710_496_805_000, 0).unwrap();
        sink.push_quote(future); // a bad time must not make every later quote stale
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
        assert_eq!(
            updates.quote.as_ref().map(|q| (q.ts, q.bid)),
            Some((at(1, 1), 102.0))
        );
        assert_eq!(updates.status, Some(FeedStatus::Live));
        assert_eq!(updates.error, Some(FeedError::Network("timeout".into())));

        sink.push_depth_snapshot(7, book(98.0));
        sink.push_depth_delta(9, &book(97.0)); // a gap
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
        let sink = inbox.subscribe("ES".into(), BarClock::new(Timeframe::Min1), 2, None);
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

    /// Guards the forming bar against a push stamped far ahead, such as a millisecond time read as
    /// seconds: it must be rejected without making the later trades late. A push within the bound
    /// must be kept: the trades of a feed clock running ahead of the system clock, a trade on short
    /// bars from a feed whose clock is seconds ahead, a daily bar stamped on a trading date days
    /// ahead.
    #[test]
    fn a_push_far_ahead_of_the_feed_clock_is_rejected() {
        let inbox = Arc::new(Inbox::default());
        let sink = subscribe(&inbox);
        let far = DateTime::from_timestamp(1_710_496_805_000, 0).unwrap();
        sink.push_tick(tick(at(0, 5), 100.0, 1.0));
        sink.push_tick(tick(far, 101.0, 1.0));
        sink.push_bar(Bar::new(far, 101.0, 101.0, 101.0, 101.0, 1.0));
        sink.push_tick(tick(at(0, 6), 102.0, 1.0));
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert_eq!(
            Vec::from_iter(updates.bars.into_values()),
            [(bar(0, [100.0, 102.0, 100.0, 102.0, 2.0]), Origin::Ticks(2))]
        );
        let prices = Vec::from_iter(updates.trades.iter().map(|trade| trade.price));
        assert_eq!(prices, [100.0, 102.0]);
        assert_eq!((updates.rejected, updates.late_ticks), (2, 0));

        let time = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
        // (timeframe, feed clock, pushes bars rather than trades, pushed times, kept slots, rejected)
        #[rustfmt::skip]
        let cases = [
            // 00:02 is two slots and 90 s ahead.
            (Timeframe::Min1, "2100-01-01T00:00:30Z", false,
                &["2100-01-01T00:00:40Z", "2100-01-01T00:01:59Z", "2100-01-01T00:02:00Z"][..],
                &["2100-01-01T00:00:00Z", "2100-01-01T00:01:00Z"][..], 1),
            // Five slots but seconds ahead is kept; 61 s ahead is rejected.
            (Timeframe::Sec1, "2100-01-01T00:00:00Z", false,
                &["2100-01-01T00:00:05Z", "2100-01-01T00:01:01Z"],
                &["2100-01-01T00:00:05Z"], 1),
            // A trading date three days ahead is kept; fifteen days ahead is rejected.
            (Timeframe::Day1, "2100-01-01T12:00:00Z", true,
                &["2100-01-04T00:00:00Z", "2100-01-16T00:00:00Z"],
                &["2100-01-04T00:00:00Z"], 1),
        ];
        for (timeframe, now, bars, pushes, kept, rejected) in cases {
            let inbox = Arc::new(Inbox::default());
            let sink = inbox.subscribe("ES".into(), BarClock::new(timeframe), 100, None);
            sink.set_server_time(time(now));
            for &push in pushes {
                let ts = time(push);
                if bars {
                    sink.push_bar(Bar::new(ts, 1.0, 1.0, 1.0, 1.0, 1.0));
                } else {
                    sink.push_tick(tick(ts, 1.0, 1.0));
                }
            }
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let kept = Vec::from_iter(kept.iter().map(|&slot| time(slot)));
            assert_eq!(
                (Vec::from_iter(updates.bars.into_keys()), updates.rejected),
                (kept, rejected),
                "{timeframe:?}"
            );
        }
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

    /// Guards the depth history samples: the book keys prices by integer ticks (a level removed
    /// at a price computed with a rounding error must leave, or the history shows liquidity that
    /// left), rows group ticks downwards, sum their sizes and keep the nearest `rows`, and a
    /// period that ends gives one column of the book as it stood before the update that opened
    /// the next period, spanning every period it did not change in. An update stamped before the
    /// open period, or ahead of the feed's time, samples nothing.
    #[test]
    fn depth_columns_sample_the_book_at_the_end_of_each_period() {
        let inbox = Arc::new(Inbox::default());
        let clock = BarClock::new(Timeframe::Min1);
        let sink = inbox.subscribe("ES".into(), clock, 100, Some(0.1));
        let config = DepthHistoryConfig {
            cadence: TimeDelta::seconds(1),
            rows: 2,
            ticks_per_row: 2,
            columns: 10,
        };
        inbox.sample_depth(sink.id, Some(config));
        let level = |price, size| OrderBookLevel {
            price,
            size,
            order_cnt: None,
        };
        let book = |second, bids, asks| OrderBook {
            ts: at(0, second),
            symbol: "ES".into(),
            bids,
            asks,
        };
        let bids = vec![
            level(0.3, 1.0),
            level(0.2, 2.0),
            level(-0.1, 4.0),
            level(-0.3, 8.0),
        ];
        sink.push_depth_snapshot(1, book(0, bids, vec![level(0.5, 3.0)]));
        sink.push_depth_delta(2, &book(0, vec![level(0.1 + 0.2, 0.0)], vec![]));
        let bids = vec![level(0.2, 5.0), level(0.3, 1.0)];
        sink.push_depth_delta(3, &book(3, bids, vec![]));
        sink.push_depth_delta(4, &book(2, vec![], vec![level(0.5, 0.0)]));
        let mut future = book(4, vec![], vec![]);
        future.ts = Utc::now() + TimeDelta::days(1);
        sink.push_depth_delta(5, &future);
        sink.push_depth_delta(6, &book(4, vec![], vec![]));

        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let column = |second, span, bids: &[(i64, f64)], asks: &[(i64, f64)]| DepthColumn {
            time: at(0, second),
            span: TimeDelta::seconds(span),
            bids: bids.to_vec(),
            asks: asks.to_vec(),
        };
        assert_eq!(
            Vec::from(updates.depth_columns),
            [
                column(0, 3, &[(1, 2.0), (-1, 4.0)], &[(2, 3.0)]),
                column(3, 1, &[(1, 6.0), (-1, 4.0)], &[]),
            ]
        );
        sink.push_depth_delta(7, &book(5, vec![], vec![]));
        inbox.sample_depth(sink.id, Some(DepthHistoryConfig::default()));
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        assert!(
            updates.depth_columns.is_empty(),
            "new settings discard pending old rows"
        );
    }
}
