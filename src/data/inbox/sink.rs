//! The producer side of the inbox: the sink a feed pushes through and the folding of ticks, trades
//! and books into what the chart drains.

use super::{Inbox, Live, TakenIds, Updates};
use crate::data::depth::{Book, Lost};
use crate::data::depth_history::Sampler;
use crate::data::merge::{Origin, fold, valid};
use crate::data::trades::{self, Trades};
use crate::data::{FeedError, FeedStatus, OrderEvent, OrderQueue, RestingOrder, SubscriptionId};
use crate::model::{Bar, BarClock, Level1Quote, OrderBook, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::btree_map::Entry;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum undrained bar slots per live subscription, independent of raw trade retention.
const MAX_PENDING_BARS: usize = 100_000;

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
    aggregate_trades_with_tick_size(trades, clock, None)
}

/// Rebuilds trade bars as `aggregate_trades` does, with an instrument tick size for range bars.
/// The tick size must be positive and finite for a range interval.
pub fn aggregate_trades_with_tick_size(
    trades: &[Tick],
    clock: impl Into<BarClock>,
    tick_size: Option<f64>,
) -> Result<Vec<Bar>, FeedError> {
    let Some(last) = trades.last() else {
        return Ok(Vec::new());
    };
    let inbox = Arc::new(Inbox::default());
    let sink = inbox.subscribe(String::new(), clock.into(), trades.len(), tick_size);
    sink.set_server_time(last.ts);
    {
        // A finite historical batch owns its output; only live pushes need the pending limit.
        let mut state = inbox.state();
        let live = state
            .subscriptions
            .get_mut(&sink.id)
            .expect("active subscription");
        let system = Utc::now();
        for trade in trades {
            live.tick(trade.clone(), system);
        }
    }
    let (_, updates) = inbox.drain().updates.pop().expect("active subscription");
    if updates.rejected > 0 || updates.late_ticks > 0 {
        return Err(FeedError::InvalidData(
            "invalid or unordered executions".into(),
        ));
    }
    Ok(updates.bars.into_values().map(|(bar, _)| bar).collect())
}

impl Inbox {
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
            delta: 0.0,
            event_closed: false,
            last_trade: None,
            last_close: None,
            empty: false,
            book: Book::new(tick_size),
            orders: None,
            orders_changed: false,
            tick_size,
            sampler: None,
            study_inputs: Vec::new(),
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
}

/// Live updates for one subscription. Cheap to clone and usable from any thread; pushes never
/// wait for the chart to catch up.
///
/// ```
/// use chrono::Utc;
/// use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
/// use egui_charts::model::{OrderBook, OrderBookLevel, Tick, OrderSide};
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
///         let side = Some(OrderSide::Buy);
///         sink.push_tick(Tick::new(Utc::now(), 100.0, 2.0, side).with_id(41_207));
///         // The buy took the whole offer at 100.
///         sink.push_depth_delta(8, &book(vec![], vec![level(100.0, 0.0)]));
///     }
/// }
/// ```
///
/// Pending bars are capped at 100,000 slots independently of raw trades. Overflow discards
/// the oldest slots and reports `FeedError::InvalidData` on the next drain; the host reloads
/// history to recover the missing bars.
#[derive(Clone)]
pub struct Sink {
    inbox: Arc<Inbox>,
    pub(crate) id: SubscriptionId,
}

impl Sink {
    /// Starts a new replay position on this subscription. The next chart drain clears prior
    /// bars, trades, depth, quote and sampled history before applying subsequent pushes. Resets
    /// trade ids and partial event bars too, so rewinding cannot double-count or retain future
    /// state. Hosts coordinate outstanding history replies and call this on every symbol's sink
    /// before dispatching `ReplayClock::seek`. Keep using the same subscription and sampler grid.
    pub fn reset_replay(&self, time: DateTime<Utc>) {
        self.update(|live| {
            live.forming = None;
            live.count_bar = None;
            live.delta = 0.0;
            live.event_closed = false;
            live.last_trade = None;
            live.last_close = None;
            live.empty = false;
            live.offset = time - Utc::now();
            live.book = Book::new(live.tick_size);
            live.orders = None;
            live.orders_changed = true;
            live.sampler = live.sampler.take().map(|s| Sampler::new(s.config));
            live.taken.order.clear();
            live.taken.ids.clear();
            live.updates = Updates::new(live.updates.trades.empty());
            live.updates.reset = true;
            for input in &live.study_inputs {
                input.clear();
            }
        });
    }

    /// A bar of the subscribed timeframe, stamped with its open or any instant inside it:
    /// replaces the bar it shares a slot with, appends a newer one, corrects an older one. A bar
    /// of a day or longer is stamped inside its calendar period, as [`HistoryPage::bars`] describes. A
    /// bar stamped too far ahead of the feed's current time is rejected, as
    /// [`Sink::set_server_time`] describes.
    ///
    /// [`HistoryPage::bars`]: crate::data::HistoryPage::bars
    pub fn push_bar(&self, bar: Bar) {
        let system = Utc::now();
        self.update(|live| {
            let slot = live.clock.bar_open(bar.time);
            if !valid(&bar) || live.ahead(slot, system) {
                live.updates.rejected += 1;
                return;
            }
            if live.clock.timeframe().is_some_and(|tf| tf.is_trade_based())
                && live.forming.is_none_or(|last| slot >= last)
            {
                live.count_bar = None;
                live.event_closed = false;
                live.delta = 0.0;
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
    /// that cannot tell pushes bars instead. Trades older than the last accepted execution or
    /// forming bar are dropped, and so is a trade whose id is among those of the last trades the
    /// subscription folded, as many as its trade store holds. A trade stamped too far ahead of
    /// the feed's current time is rejected, as [`Sink::set_server_time`] describes.
    /// Repeated ids can complete missing aggressor and resting order ids without adding volume.
    ///
    /// Trade-based intervals start at the first accepted execution of the subscription. Replay the
    /// retained executions on resubscription to rebuild a partial bar. Count bars use the first
    /// trade's time; if that would duplicate a bar time, it advances by one nanosecond. Original
    /// execution times remain unchanged. Volume and delta fills split exactly at the threshold;
    /// range bars need the instrument tick size and close at the first real fill reaching it.
    /// A fill producing more than 100,000 bars is rejected atomically. A pushed trade-based bar must be complete: the next trade
    /// starts a fresh bar. History pages should contain complete trade-based bars only.
    pub fn push_tick(&self, tick: Tick) {
        let system = Utc::now();
        self.update(|live| live.tick(tick, system));
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
            if live.ahead(live.clock.bar_open(now), system) {
                return false;
            }
            live.updates.time = live.updates.time.max(Some(now));
            let open = live.clock.bar_open(now);
            let Some(close) = live.last_close else {
                return !live.study_inputs.is_empty();
            };
            if live.forming.is_none_or(|last| open <= last) {
                return !live.study_inputs.is_empty();
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
            live.bound_bars();
            true
        });
    }

    /// The whole order book, numbered `sequence`, at most 10,000 levels per side: a deeper book is
    /// rejected. It replaces the book; the next delta carries `sequence + 1`. Levels are keyed by
    /// the nearest integer tick of the instrument's [`tick_size`](crate::model::Instrument::tick_size),
    /// when it has one, so prices that differ by a rounding error are one level and the latest
    /// size pushed for it holds. A crossed book is kept as pushed.
    pub fn push_depth_snapshot(&self, sequence: u64, book: OrderBook) {
        self.update(|live| {
            live.orders = None;
            live.orders_changed = true;
            live.depth(book.ts, |depth| depth.snapshot(sequence, book));
        });
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
    /// [`Datafeed::request_depth_snapshot`]: crate::data::Datafeed::request_depth_snapshot
    pub fn push_depth_delta(&self, sequence: u64, changes: &OrderBook) {
        self.update(|live| {
            live.orders = None;
            live.orders_changed = true;
            live.depth(changes.ts, |depth| depth.delta(sequence, changes));
        });
    }

    /// Replaces the per-order book, at most 10,000 orders per side, listed in queue order.
    /// The next event has `sequence + 1`; events buffered during resync above the snapshot's
    /// sequence apply afterwards. Gaps and invalid events clear both books and ask the feed for
    /// a new snapshot once through `Datafeed::request_depth_snapshot`. Use either order events
    /// or aggregated deltas for a subscription; an aggregated push discards the order queues.
    pub fn push_order_snapshot(
        &self,
        sequence: u64,
        time: DateTime<Utc>,
        orders: Vec<RestingOrder>,
    ) {
        self.update(|live| {
            live.sample_book(time);
            let book = live
                .orders
                .get_or_insert_with(|| OrderQueue::new(live.tick_size));
            let result = book.set_snapshot(sequence, time, orders);
            let trades = book.take_executions();
            live.sync_orders(sequence, time, result);
            for trade in trades {
                live.tick(trade, Utc::now());
            }
        });
    }

    /// Applies one order event with consecutive sequencing. Repeated sequences are ignored;
    /// while waiting for a snapshot at most 10,000 consecutive events are kept. Unknown cancels,
    /// duplicate adds, invalid sizes and oversized executions lose the book. An execution's
    /// optional trade enters the raw trade store and carries the resting order id it filled.
    pub fn push_order_event(&self, sequence: u64, time: DateTime<Utc>, event: OrderEvent) {
        self.update(|live| {
            live.sample_book(time);
            let book = live
                .orders
                .get_or_insert_with(|| OrderQueue::new(live.tick_size));
            let result = book.event(sequence, time, event);
            let trades = book.take_executions();
            live.sync_orders(sequence, time, result);
            for trade in trades {
                live.tick(trade, Utc::now());
            }
        });
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
                if let Some(tick) = live.tick_size {
                    for input in &live.study_inputs {
                        input.push_quote(&live.clock, &quote, tick);
                    }
                }
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

    pub(super) fn update(&self, update: impl FnOnce(&mut Live)) {
        self.inbox
            .push(|state| match state.subscriptions.get_mut(&self.id) {
                Some(live) => {
                    update(live);
                    live.bound_bars();
                    true
                }
                None => false,
            });
    }
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

impl Live {
    fn bound_bars(&mut self) {
        while self.updates.bars.len() > MAX_PENDING_BARS {
            self.updates.bars.pop_first();
            self.updates.dropped_bars += 1;
        }
    }

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

    fn tick(&mut self, tick: Tick, system: DateTime<Utc>) {
        let live = self;

        if live.clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            if !trades::valid(&tick) || live.ahead(tick.ts, system) {
                live.updates.rejected += 1;
            } else if tick.id.is_some_and(|id| live.taken.ids.contains(&id)) {
                live.updates.repeated_ticks += 1;
                if tick.aggressor_order_id.is_some() || tick.resting_order_id.is_some() {
                    live.updates.trades.push(tick);
                }
            } else if live.last_trade.is_some_and(|last| tick.ts < last) {
                live.updates.late_ticks += 1;
            } else if live.fold_trade_bar(&tick).is_err() {
                live.updates.rejected += 1;
            } else {
                live.last_trade = Some(tick.ts);
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
        } else if tick.id.is_some_and(|id| live.taken.ids.contains(&id)) {
            updates.repeated_ticks += 1;
            if tick.aggressor_order_id.is_some() || tick.resting_order_id.is_some() {
                updates.trades.push(tick);
            }
        } else if live.last_trade.is_some_and(|last| tick.ts < last)
            || live.forming.is_some_and(|forming| open < forming)
        {
            updates.late_ticks += 1;
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
            live.last_trade = Some(tick.ts);
            live.last_close = Some(price);
            live.empty = false;
            live.forming = Some(open);
            if let Some(id) = tick.id {
                live.taken.insert(id);
            }
            updates.trades.push(tick);
        }
    }

    /// Folds exact volume and delta thresholds atomically; raw fills stay intact for order flow.
    fn fold_trade_bar(&mut self, tick: &Tick) -> Result<(), ()> {
        use crate::model::{OrderSide, Timeframe};
        let timeframe = self.clock.timeframe().ok_or(())?;
        let mut forming = self.forming;
        let mut current = self.count_bar.clone();
        let (mut delta, mut closed) = (self.delta, self.event_closed);
        let mut remaining = tick.size;
        let mut bars = Vec::new();
        loop {
            if bars.len() == 100_000 {
                return Err(());
            }
            let starts = closed || current.is_none();
            let (old, count) = if starts {
                let time = match forming.filter(|last| *last >= tick.ts) {
                    Some(last) => last
                        .checked_add_signed(TimeDelta::nanoseconds(1))
                        .ok_or(())?,
                    None => tick.ts,
                };
                delta = 0.0;
                (
                    Bar::new(time, tick.price, tick.price, tick.price, tick.price, 0.0),
                    0,
                )
            } else {
                current.clone().ok_or(())?
            };
            let size = match timeframe {
                Timeframe::Volume(limit) => remaining.min(limit.get() as f64 - old.volume),
                Timeframe::Delta(limit) => match tick.side {
                    Some(OrderSide::Buy) => remaining.min(limit.get() as f64 - delta),
                    Some(OrderSide::Sell) => remaining.min(limit.get() as f64 + delta),
                    None => remaining,
                },
                _ => remaining,
            };
            let fill = Bar::new(
                old.time, tick.price, tick.price, tick.price, tick.price, size,
            );
            let bar = fold(&old, &fill).ok_or(())?;
            delta += match tick.side {
                Some(OrderSide::Buy) => size,
                Some(OrderSide::Sell) => -size,
                None => 0.0,
            };
            closed = match timeframe {
                Timeframe::Ticks(limit) => count + 1 >= limit.get(),
                Timeframe::Volume(limit) => bar.volume >= limit.get() as f64,
                Timeframe::Delta(limit) => delta.abs() >= limit.get() as f64,
                Timeframe::Range(ticks) => {
                    let tick_size = self
                        .tick_size
                        .filter(|s| s.is_finite() && *s > 0.0)
                        .ok_or(())?;
                    let range = ticks.get() as f64 * tick_size;
                    if !range.is_finite() {
                        return Err(());
                    }
                    (bar.high / tick_size).round() - (bar.low / tick_size).round()
                        >= ticks.get() as f64
                }
                _ => return Err(()),
            };
            forming = Some(bar.time);
            current = Some((bar.clone(), count + 1));
            bars.push(bar);
            let left = remaining - size;
            if left <= 0.0 {
                break;
            }
            if left == remaining {
                return Err(());
            }
            remaining = left;
        }
        self.forming = forming;
        self.count_bar = current;
        self.delta = delta;
        self.event_closed = closed;
        for bar in bars {
            self.updates.bars.insert(bar.time, (bar, Origin::Live));
        }
        Ok(())
    }

    /// Applies a depth update stamped `ts`, sampling the book as it stood before when the update
    /// opens a new period of the depth history. An update stamped more than a minute after the
    /// feed's time samples nothing: it would hold the open period, and every later column, until
    /// that time passed.
    fn sample_book(&mut self, ts: DateTime<Utc>) {
        let Some(sampler) = &mut self.sampler else {
            return;
        };
        if !(Utc::now().checked_add_signed(self.offset + TimeDelta::minutes(1)))
            .is_some_and(|limit| ts <= limit)
        {
            return;
        }
        // The orders a filtered history samples, summed as they stood before this update, only
        // when the update opens a period.
        let mut filtered = Book::new(self.tick_size);
        let book = match &sampler.config.orders {
            Some(filter) if sampler.due(ts) => {
                let levels = self
                    .orders
                    .as_ref()
                    .and_then(|queue| queue.filtered(&self.symbol, filter).ok());
                let Some(levels) = levels else {
                    sampler.lose();
                    return;
                };
                let _ = filtered.snapshot(0, levels);
                &filtered
            }
            _ => &self.book,
        };
        if let Some(column) = sampler.sample(ts, book) {
            let columns = &mut self.updates.depth_columns;
            if columns.len() == sampler.config.columns {
                columns.pop_front();
            }
            columns.push_back(column);
        }
    }

    fn sync_orders(&mut self, sequence: u64, time: DateTime<Utc>, result: Result<(), Lost>) {
        self.orders_changed = true;
        let levels = self
            .orders
            .as_ref()
            .expect("order book")
            .aggregate(&self.symbol, 0.0);
        let sequence = self
            .orders
            .as_ref()
            .and_then(OrderQueue::sequence)
            .unwrap_or(sequence);
        let derived = self.book.snapshot(sequence, levels);
        self.depth_result(time, result.and(derived));
    }

    fn depth(&mut self, ts: DateTime<Utc>, apply: impl FnOnce(&mut Book) -> Result<(), Lost>) {
        self.sample_book(ts);
        let result = apply(&mut self.book);
        self.depth_result(ts, result);
    }

    fn depth_result(&mut self, ts: DateTime<Utc>, result: Result<(), Lost>) {
        let lost = result.is_err();
        match result {
            // The book holds a snapshot newer than any loss, so a pending request is moot.
            Ok(()) => self.updates.resnapshot = false,
            Err(lost) => {
                self.updates.rejected_depth += u64::from(lost.invalid);
                self.updates.resnapshot |= lost.request;
            }
        }
        if (Utc::now().checked_add_signed(self.offset + TimeDelta::minutes(1)))
            .is_some_and(|limit| ts <= limit)
        {
            for input in &self.study_inputs {
                match input.order_filter() {
                    None => {
                        input.book_levels(&self.clock, ts, self.book.levels().filter(|_| !lost))
                    }
                    Some(filter) => {
                        let book = (self.orders.as_ref())
                            .and_then(|queue| queue.filtered(&self.symbol, &filter).ok())
                            .filter(|_| !lost);
                        input.push_book(&self.clock, ts, book.as_ref());
                    }
                }
            }
        }
    }
}
