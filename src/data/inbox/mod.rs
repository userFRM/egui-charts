//! Where feed answers and live updates wait for the chart.
//!
//! Pushes coalesce on arrival: ticks fold into the bar of their slot and a bar replaces the
//! pending bar of its slot, so the inbox holds the bars that really changed, however fast a
//! feed pushes. Trades wait in a store as bounded as the chart's, and depth updates apply to the
//! subscription's book.

mod sink;

pub use sink::{Sink, aggregate_trades, aggregate_trades_with_tick_size};

use super::depth::Book;
use super::depth_history::{DepthColumn, DepthHistoryConfig, Sampler};
use super::merge::Origin;
use super::trades::Trades;
use super::{FeedError, FeedStatus, OrderQueue, SubscriptionId};
use crate::model::{Bar, BarClock, Level1Quote, OrderBook};
use chrono::{DateTime, TimeDelta, Utc};
use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
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
    symbol: String,
    clock: BarClock,
    /// Open of the newest slot pushed, kept across drains: trades of older slots are late.
    forming: Option<DateTime<Utc>>,
    /// Feed clock minus system clock, as last reported; zero until then.
    offset: TimeDelta,
    count_bar: Option<(Bar, u64)>,
    delta: f64,
    event_closed: bool,
    last_trade: Option<DateTime<Utc>>,
    last_close: Option<f64>,
    empty: bool,
    book: Book,
    orders: Option<OrderQueue>,
    orders_changed: bool,
    tick_size: Option<f64>,
    /// Samples the book for the chart's depth history, when it keeps one.
    sampler: Option<Sampler>,
    study_inputs: Vec<crate::data::OrderFlowInput>,
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

/// What arrived since the last drain.
pub(crate) struct Drained {
    /// In arrival order, each with the key of its request or event stream.
    pub(crate) answers: Vec<(u64, Answer)>,
    pub(crate) updates: Vec<(SubscriptionId, Updates)>,
}

/// What one subscription pushed since the last drain.
pub(crate) struct Updates {
    /// A replay seek discards data from the previous playback position.
    pub(crate) reset: bool,
    /// One per slot, keyed by its open; the last one is the forming bar.
    pub(crate) bars: BTreeMap<DateTime<Utc>, (Bar, Origin)>,
    /// The trades folded into `bars`, in arrival order.
    pub(crate) trades: Trades,
    /// The order book, if it changed.
    pub(crate) depth: Option<OrderBook>,
    pub(crate) orders: Option<OrderQueue>,
    pub(crate) orders_changed: bool,
    /// Columns sampled for the depth history, oldest first, as many as its newest resolution
    /// holds: the oldest make room for new ones.
    pub(crate) depth_columns: VecDeque<DepthColumn>,
    /// Latest top-of-book quote pushed in this frame.
    pub(crate) quote: Option<Level1Quote>,
    /// Latest accepted heartbeat, applied to studies after the queued trades are merged.
    pub(crate) time: Option<DateTime<Utc>>,
    /// The book lost its snapshot: call `Datafeed::request_depth_snapshot`.
    pub(crate) resnapshot: bool,
    pub(crate) symbols_changed: bool,
    pub(crate) status: Option<FeedStatus>,
    pub(crate) error: Option<FeedError>,
    /// Trades older than the last accepted execution or forming bar, dropped.
    pub(crate) late_ticks: u64,
    /// Trades whose id is among those the subscription remembers, dropped.
    pub(crate) repeated_ticks: u64,
    /// Bars and ticks that failed validation or were stamped too far ahead of the feed's clock,
    /// dropped.
    pub(crate) rejected: u64,
    /// Bar slots evicted before a drain, reported as data loss even after another stream error.
    dropped_bars: u64,
    /// Depth snapshots and deltas that failed validation, dropped.
    pub(crate) rejected_depth: u64,
}

impl Updates {
    fn new(trades: Trades) -> Self {
        Self {
            reset: false,
            bars: BTreeMap::new(),
            trades,
            depth: None,
            orders: None,
            orders_changed: false,
            depth_columns: VecDeque::new(),
            quote: None,
            time: None,
            resnapshot: false,
            symbols_changed: false,
            status: None,
            error: None,
            late_ticks: 0,
            repeated_ticks: 0,
            rejected: 0,
            dropped_bars: 0,
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

    /// Samples the book of `id` into depth history columns from its next update, or stops. The
    /// columns sampled before and not yet drained are dropped.
    pub(crate) fn sample_depth(&self, id: SubscriptionId, config: Option<DepthHistoryConfig>) {
        if let Some(live) = self.state().subscriptions.get_mut(&id) {
            live.sampler = config.map(Sampler::new);
            live.updates.depth_columns.clear();
        }
    }

    /// Connects active stream studies before their inputs are coalesced for the frame.
    #[cfg(feature = "widget")]
    pub(crate) fn study_inputs(
        &self,
        id: SubscriptionId,
        inputs: Vec<crate::data::OrderFlowInput>,
    ) {
        if let Some(live) = self.state().subscriptions.get_mut(&id) {
            live.study_inputs = inputs;
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
                if updates.dropped_bars > 0 {
                    updates.rejected += updates.dropped_bars;
                    updates.error = Some(FeedError::InvalidData(format!(
                        "pending bar limit exceeded: {} bars dropped; reload history",
                        updates.dropped_bars,
                    )));
                }
                updates.depth = live.book.take_changed();
                updates.orders_changed = std::mem::take(&mut live.orders_changed);
                if updates.orders_changed {
                    updates.orders = live.orders.clone();
                }
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

impl<T: Send + 'static> std::fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("request", &self.request)
            .field("sent", &self.sent)
            .finish_non_exhaustive()
    }
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

#[cfg(test)]
mod tests;
