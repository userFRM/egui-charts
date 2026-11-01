//! One event clock for host-supplied trades, quotes and depth across instruments.

use super::{
    Datafeed, DepthHistory, DepthPage, DepthRequest, FeedError, HistoryPage, HistoryRequest,
    OrderEvent, RecordingReader, Reply, RestingOrder, Sink, Subscription, SubscriptionId,
};
use crate::model::{Instrument, Level1Quote, OrderBook, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// Recorded market data. Book sequencing is independent of the replay's merge sequence.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MarketEvent {
    /// One raw fill.
    Trade(Tick),
    /// Best bid and ask.
    Quote(Level1Quote),
    /// A periodic aggregated book snapshot, also a depth seek anchor.
    DepthSnapshot {
        /// Exchange sequence number for this book stream.
        sequence: u64,
        /// Aggregated price levels in this snapshot or delta.
        book: OrderBook,
    },
    /// Changed aggregated levels.
    DepthDelta {
        /// Exchange sequence number for this book stream.
        sequence: u64,
        /// Aggregated price levels in this snapshot or delta.
        book: OrderBook,
    },
    /// A periodic per-order snapshot in queue order, also a depth seek anchor.
    OrderSnapshot {
        /// Exchange sequence number for this book stream.
        sequence: u64,
        /// Resting orders in queue order.
        orders: Vec<RestingOrder>,
    },
    /// One sequenced resting order change.
    Order {
        /// Exchange sequence number for this book stream.
        sequence: u64,
        /// Change to one resting order.
        event: OrderEvent,
    },
}

/// One instrument's event at an observation instant. The host supplies a merge sequence to
/// break timestamp ties within a symbol, including ties between trades and book messages.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ReplayEvent {
    /// Instrument routed to the corresponding subscription by the host.
    pub symbol: String,
    /// Observation time used by the shared clock. Payload timestamps must agree.
    pub time: DateTime<Utc>,
    /// Tie breaker within a symbol; independent of exchange book sequence numbers.
    pub sequence: u64,
    /// Market data observed at this instant.
    pub event: MarketEvent,
}

impl ReplayEvent {
    /// An event with an explicit observation time and tie-breaking sequence.
    pub fn new(
        symbol: impl Into<String>,
        time: DateTime<Utc>,
        sequence: u64,
        event: MarketEvent,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            time,
            sequence,
            event,
        }
    }

    /// Pushes into this symbol's sink using recorded market time. Hosts call this outside egui
    /// context locks, exactly as with live feed pushes. Does not perform routing or disk I/O.
    pub fn push_to(&self, sink: &Sink) {
        sink.set_server_time(self.time);
        match &self.event {
            MarketEvent::Trade(trade) => sink.push_tick(trade.clone()),
            MarketEvent::Quote(quote) => sink.push_quote(quote.clone()),
            MarketEvent::DepthSnapshot { sequence, book } => {
                sink.push_depth_snapshot(*sequence, book.clone())
            }
            MarketEvent::DepthDelta { sequence, book } => sink.push_depth_delta(*sequence, book),
            MarketEvent::OrderSnapshot { sequence, orders } => {
                sink.push_order_snapshot(*sequence, self.time, orders.clone())
            }
            MarketEvent::Order { sequence, event } => {
                sink.push_order_event(*sequence, self.time, event.clone())
            }
        }
    }

    fn book_event(&self) -> bool {
        matches!(
            self.event,
            MarketEvent::DepthSnapshot { .. }
                | MarketEvent::DepthDelta { .. }
                | MarketEvent::OrderSnapshot { .. }
                | MarketEvent::Order { .. }
        )
    }
    fn snapshot(&self) -> bool {
        matches!(
            self.event,
            MarketEvent::DepthSnapshot { .. } | MarketEvent::OrderSnapshot { .. }
        )
    }
}

/// A deterministic clock merging all instruments by time, sequence, then symbol. Advancing uses
/// wall-clock elapsed time times the speed, never a bar index. Hosts own the input and scheduling.
/// Seek rebuilds trades from the start and depth from the latest preceding snapshot per symbol.
///
/// ```
/// use egui_charts::data::{MarketEvent, ReplayClock, ReplayEvent};
/// use egui_charts::model::Tick;
/// use std::time::Duration;
/// let time = chrono::Utc::now();
/// let event = ReplayEvent::new("A", time, 0,
///     MarketEvent::Trade(Tick::new(time, 100.0, 2.0, None)));
/// let mut clock = ReplayClock::new(vec![event]).unwrap();
/// clock.set_speed(2.0).unwrap();
/// assert_eq!(clock.advance(Duration::ZERO).unwrap().len(), 1);
/// assert!(clock.is_finished());
/// ```
#[derive(Clone, Debug)]
pub struct ReplayClock {
    events: Arc<[ReplayEvent]>,
    cursor: usize,
    time: DateTime<Utc>,
    speed: f64,
    fractional_nanos: f64,
}

impl ReplayClock {
    /// Sorts events into deterministic replay order. Duplicate time/sequence keys within one
    /// symbol or disagreeing payload timestamps/symbols are invalid; equal keys across symbols
    /// are ordered by symbol. Start at the first event, with no events consumed yet.
    pub fn new(mut events: Vec<ReplayEvent>) -> Result<Self, FeedError> {
        events
            .sort_by(|a, b| (a.time, a.sequence, &a.symbol).cmp(&(b.time, b.sequence, &b.symbol)));
        if events.iter().any(|e| !valid(e))
            || events.windows(2).any(|w| {
                (w[0].time, w[0].sequence, &w[0].symbol) == (w[1].time, w[1].sequence, &w[1].symbol)
            })
        {
            return Err(FeedError::InvalidData(
                "invalid replay event or duplicate merge key".into(),
            ));
        }
        let time = events.first().map_or(DateTime::default(), |e| e.time);
        Ok(Self {
            events: events.into(),
            cursor: 0,
            time,
            speed: 1.0,
            fractional_nanos: 0.0,
        })
    }

    /// Current market time.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }
    /// Playback multiplier; zero pauses the clock.
    pub fn speed(&self) -> f64 {
        self.speed
    }
    /// Sets a finite non-negative multiplier, preserving sub-nanosecond elapsed time.
    pub fn set_speed(&mut self, speed: f64) -> Result<(), FeedError> {
        if !speed.is_finite() || speed < 0.0 {
            return Err(FeedError::InvalidData("invalid replay speed".into()));
        }
        self.speed = speed;
        Ok(())
    }
    /// Whether every recorded event has been consumed.
    pub fn is_finished(&self) -> bool {
        self.cursor == self.events.len()
    }

    /// Advances by real elapsed time at the selected speed, returning newly due events once,
    /// including events at the new time. A paused clock consumes nothing. Overflow is an error
    /// without changing the cursor or time. Sub-nanosecond progress is retained between calls.
    pub fn advance(&mut self, elapsed: Duration) -> Result<&[ReplayEvent], FeedError> {
        if self.speed == 0.0 {
            return Ok(&[]);
        }
        let nanos = elapsed.as_secs_f64() * self.speed * 1e9 + self.fractional_nanos;
        if !nanos.is_finite() || nanos >= i64::MAX as f64 {
            return Err(FeedError::InvalidData(
                "replay elapsed time overflow".into(),
            ));
        }
        let time = self
            .time
            .checked_add_signed(TimeDelta::nanoseconds(nanos as i64))
            .ok_or_else(|| FeedError::InvalidData("replay clock overflow".into()))?;
        self.fractional_nanos = nanos.fract();
        self.time = time;
        let start = self.cursor;
        self.cursor = self.events.partition_point(|e| e.time <= time);
        Ok(&self.events[start..self.cursor])
    }

    fn position(&mut self, time: DateTime<Utc>) {
        self.time = time;
        self.fractional_nanos = 0.0;
        self.cursor = self.events.partition_point(|event| event.time <= time);
    }

    /// Seeks inclusively to `time`. Before pushing these events, call `Sink::reset_replay` on
    /// every participating sink. Trades rebuild bars from the beginning; book messages before
    /// each symbol's latest snapshot are skipped. A symbol without a snapshot starts empty and
    /// follows normal gap/resync rules. Subsequent `advance` returns only events after the seek.
    pub fn seek(&mut self, time: DateTime<Utc>) -> impl Iterator<Item = ReplayEvent> {
        self.position(time);
        let events = &self.events[..self.cursor];
        let mut anchors = HashMap::new();
        for (at, event) in events.iter().enumerate().filter(|(_, e)| e.snapshot()) {
            anchors.insert(event.symbol.as_str(), at);
        }
        events.iter().enumerate().filter_map(move |(at, event)| {
            if event.book_event()
                && anchors
                    .get(event.symbol.as_str())
                    .is_some_and(|anchor| at < *anchor)
            {
                if let MarketEvent::Order {
                    event:
                        OrderEvent::Execute {
                            id,
                            trade: Some(trade),
                            ..
                        },
                    ..
                } = &event.event
                {
                    let trade = trade.clone().with_resting_order_id(*id);
                    return Some(ReplayEvent::new(
                        &event.symbol,
                        event.time,
                        event.sequence,
                        MarketEvent::Trade(trade),
                    ));
                }
                return None;
            }
            Some(event.clone())
        })
    }
}

/// A recording-backed feed and playback driver for every symbol in one session.
///
/// Attach the same `Arc<ReplayDriver>` to each chart with `ChartBuilder::with_feed`. Call
/// [`advance`](Self::advance) outside egui context locks before drawing, even while paused, to
/// publish the shared time to quiet symbols. Zero speed pauses; seeking is inclusive. Hosts own
/// capture, storage and scheduling. Bar aggregation, book gaps and trade deduplication use the
/// live sink's rules. A new subscription reconstructs the current position, including when its
/// interval changes. No history request can return events beyond the playback position.
///
/// ```
/// use egui_charts::data::{MarketEvent, RecordingWriter, ReplayDriver, ReplayEvent};
/// use egui_charts::model::{Instrument, Tick};
/// use std::time::Duration;
/// let time = chrono::Utc::now();
/// let mut capture = RecordingWriter::new(Vec::new())?;
/// capture.write(&ReplayEvent::new("A", time, 0,
///     MarketEvent::Trade(Tick::new(time, 100.0, 2.0, None))))?;
/// let bytes = capture.finish()?;
/// let replay = ReplayDriver::load(bytes.as_slice(), vec![Instrument::new("A")])?;
/// replay.advance(Duration::ZERO)?;
/// replay.set_speed(0.0)?;
/// replay.seek(time);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ReplayDriver {
    state: Mutex<Playback>,
    // Serialize pushes without blocking readouts or unsubscribe while a waker waits on egui.
    dispatch: Mutex<()>,
}

struct Playback {
    clock: ReplayClock,
    instruments: HashMap<String, Instrument>,
    subscriptions: HashMap<SubscriptionId, (String, Sink)>,
}

impl ReplayDriver {
    /// Validates a complete recording and its host-supplied instrument metadata. Every recorded
    /// symbol must have exactly one instrument; its tick size is needed for price-based bars.
    /// Playback starts at the first event, with none consumed, at normal speed.
    pub fn new(events: Vec<ReplayEvent>, instruments: Vec<Instrument>) -> Result<Self, FeedError> {
        let count = instruments.len();
        let instruments: HashMap<_, _> = instruments
            .into_iter()
            .map(|instrument| (instrument.symbol.clone(), instrument))
            .collect();
        if count != instruments.len()
            || events
                .iter()
                .any(|event| !instruments.contains_key(&event.symbol))
        {
            return Err(FeedError::InvalidData(
                "missing or duplicate replay instrument".into(),
            ));
        }
        Ok(Self {
            state: Mutex::new(Playback {
                clock: ReplayClock::new(events)?,
                instruments,
                subscriptions: HashMap::new(),
            }),
            dispatch: Mutex::new(()),
        })
    }

    /// Loads the same stream that [`super::RecordingWriter`] writes. Read errors retain their
    /// original kind; malformed recordings and duplicate merge keys report `InvalidData`.
    pub fn load(reader: impl Read, instruments: Vec<Instrument>) -> io::Result<Self> {
        let mut reader = RecordingReader::new(reader)?;
        let mut events = Vec::new();
        while let Some(event) = reader.read()? {
            events.push(event);
        }
        Self::new(events, instruments)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn state(&self) -> MutexGuard<'_, Playback> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The inclusive playback position shared by all subscriptions.
    pub fn time(&self) -> DateTime<Utc> {
        self.state().clock.time()
    }

    /// First and last recorded observation instants, for transport bounds; `None` when empty.
    pub fn time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let state = self.state();
        Some((
            state.clock.events.first()?.time,
            state.clock.events.last()?.time,
        ))
    }

    /// Playback multiplier; zero pauses.
    pub fn speed(&self) -> f64 {
        self.state().clock.speed()
    }

    /// Sets a finite, non-negative playback multiplier.
    pub fn set_speed(&self, speed: f64) -> Result<(), FeedError> {
        self.state().clock.set_speed(speed)
    }

    /// Whether every event has been published.
    pub fn is_finished(&self) -> bool {
        self.state().clock.is_finished()
    }

    /// Advances once by wall-clock elapsed time and routes due events by symbol. Publishes the
    /// resulting time to every sink, including quiet symbols and paused frames. Clock errors
    /// leave the position and subscriptions unchanged.
    pub fn advance(&self, elapsed: Duration) -> Result<(), FeedError> {
        let _dispatch = self.dispatch.lock().unwrap_or_else(PoisonError::into_inner);
        let (events, start, end, time, sinks) = {
            let mut state = self.state();
            let count = state.clock.advance(elapsed)?.len();
            let end = state.clock.cursor;
            (
                Arc::clone(&state.clock.events),
                end - count,
                end,
                state.clock.time(),
                state.sinks(),
            )
        };
        for event in &events[start..end] {
            for (symbol, sink) in &sinks {
                if *symbol == event.symbol {
                    event.push_to(sink);
                }
            }
        }
        publish_time(time, &sinks);
        Ok(())
    }

    /// Reconstructs every active subscription through `time`, clearing all later state first.
    /// Replays book history from the beginning as well as trades: starting at the latest book
    /// snapshot would lose earlier heatmap columns. No later snapshot or execution is exposed.
    /// Call outside egui context locks, as with live pushes.
    pub fn seek(&self, time: DateTime<Utc>) {
        let _dispatch = self.dispatch.lock().unwrap_or_else(PoisonError::into_inner);
        let (events, end, sinks) = {
            let mut state = self.state();
            state.clock.position(time);
            (
                Arc::clone(&state.clock.events),
                state.clock.cursor,
                state.sinks(),
            )
        };
        for (symbol, sink) in &sinks {
            rebuild(&events[..end], symbol, sink, time);
        }
        publish_time(time, &sinks);
    }
}

impl Playback {
    fn sinks(&mut self) -> Vec<(String, Sink)> {
        self.subscriptions.retain(|_, (_, sink)| sink.is_active());
        self.subscriptions.values().cloned().collect()
    }
}

fn rebuild(events: &[ReplayEvent], symbol: &str, sink: &Sink, time: DateTime<Utc>) {
    sink.reset_replay(time);
    for event in events {
        if event.symbol == symbol {
            event.push_to(sink);
        }
    }
}

fn publish_time(time: DateTime<Utc>, sinks: &[(String, Sink)]) {
    for (_, sink) in sinks {
        sink.set_server_time(time);
        sink.advance_time(time);
    }
}

impl Datafeed for ReplayDriver {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        let instrument = self
            .state()
            .instruments
            .get(symbol)
            .cloned()
            .ok_or(FeedError::NotSupported);
        reply.send(instrument);
    }

    fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
        // The subscription rebuilds all bars through the cursor, including its partial bar.
        reply.send(Ok(HistoryPage::new(Vec::new(), true)));
    }

    fn subscribe(&self, subscription: Subscription) {
        let _dispatch = self.dispatch.lock().unwrap_or_else(PoisonError::into_inner);
        let position = {
            let mut state = self.state();
            if !state.instruments.contains_key(&subscription.symbol)
                || subscription.conversion.is_some()
            {
                None
            } else {
                state.subscriptions.insert(
                    subscription.id,
                    (subscription.symbol.clone(), subscription.sink.clone()),
                );
                Some((
                    Arc::clone(&state.clock.events),
                    state.clock.cursor,
                    state.clock.time(),
                ))
            }
        };
        let Some((events, end, time)) = position else {
            subscription.sink.report_error(FeedError::NotSupported);
            return;
        };
        rebuild(
            &events[..end],
            &subscription.symbol,
            &subscription.sink,
            time,
        );
        publish_time(time, &[(subscription.symbol, subscription.sink)]);
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        self.state().subscriptions.remove(&id);
    }

    fn depth_history(&self, request: DepthRequest, reply: Reply<DepthPage>) {
        let _dispatch = self.dispatch.lock().unwrap_or_else(PoisonError::into_inner);
        if request.conversion.is_some()
            || !request.tick_size.is_finite()
            || request.tick_size <= 0.0
        {
            reply.send(Err(FeedError::NotSupported));
            return;
        }
        if let Err(error) = DepthHistory::new(request.config.clone()) {
            reply.send(Err(error));
            return;
        }
        let (events, end, time) = {
            let state = self.state();
            let time = state.clock.time();
            let horizon = request
                .before
                .and_then(|before| before.checked_add_signed(request.config.cadence))
                .map_or(time, |before| before.min(time));
            let end = state
                .clock
                .events
                .partition_point(|event| event.time <= horizon)
                .min(state.clock.cursor);
            (Arc::clone(&state.clock.events), end, horizon)
        };
        let inbox = Arc::new(super::inbox::Inbox::default());
        let sink = inbox.subscribe(
            request.symbol.clone(),
            crate::model::Timeframe::Min1.into(),
            0,
            Some(request.tick_size),
        );
        inbox.sample_depth(sink.id, Some(request.config));
        sink.reset_replay(time);
        for event in &events[..end] {
            if event.symbol == request.symbol && event.book_event() {
                event.push_to(&sink);
            }
        }
        sink.set_server_time(time);
        sink.advance_time(time);
        let (_, updates) = inbox
            .drain()
            .updates
            .pop()
            .expect("replay depth subscription");
        let history = updates.replay_depth.expect("replay depth history");
        let mut columns: Vec<_> = history
            .columns()
            .filter(|column| {
                request.before.is_none_or(|before| column.time < before)
                    && request.since.is_none_or(|since| column.time >= since)
            })
            .cloned()
            .collect();
        let start = columns.len().saturating_sub(request.count);
        reply.send(Ok(DepthPage::new(columns.split_off(start), start == 0)));
    }
}

pub(super) fn valid(event: &ReplayEvent) -> bool {
    match &event.event {
        MarketEvent::Trade(t) => t.ts == event.time && super::trades::valid(t),
        MarketEvent::Quote(q) => {
            q.ts == event.time
                && q.symbol == event.symbol
                && [q.bid, q.ask, q.bid_size, q.ask_size]
                    .iter()
                    .all(|v| v.is_finite())
                && q.bid_size >= 0.0
                && q.ask_size >= 0.0
        }
        MarketEvent::DepthSnapshot { book, .. } | MarketEvent::DepthDelta { book, .. } => {
            book.ts == event.time
                && book.symbol == event.symbol
                && book.bids.len().max(book.asks.len()) <= super::depth::MAX_LEVELS
                && book
                    .bids
                    .iter()
                    .chain(&book.asks)
                    .all(|l| l.price.is_finite() && l.size.is_finite() && l.size >= 0.0)
        }
        MarketEvent::OrderSnapshot { orders, .. } => {
            let bids = orders
                .iter()
                .filter(|o| o.side == crate::model::OrderSide::Buy)
                .count();
            let mut ids = HashSet::new();
            bids <= super::depth::MAX_LEVELS
                && orders.len() - bids <= super::depth::MAX_LEVELS
                && orders.iter().all(|o| valid_order(o) && ids.insert(o.id))
        }
        MarketEvent::Order { event: order, .. } => match order {
            OrderEvent::Add(order) => valid_order(order),
            OrderEvent::Modify { price, size, .. } => {
                price.is_none_or(f64::is_finite) && size.is_none_or(|s| s.is_finite() && s >= 0.0)
            }
            OrderEvent::Cancel { .. } => true,
            OrderEvent::Execute { size, trade, .. } => {
                size.is_finite()
                    && *size > 0.0
                    && trade.as_ref().is_none_or(|t| {
                        t.ts == event.time && super::trades::valid(t) && t.size == *size
                    })
            }
        },
    }
}

fn valid_order(order: &RestingOrder) -> bool {
    order.price.is_finite() && order.size.is_finite() && order.size > 0.0
}

#[cfg(test)]
mod tests {
    use super::super::inbox::Inbox;
    use super::*;
    use crate::model::{OrderBookLevel, OrderSide, Timeframe};
    use std::sync::Arc;

    fn at(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(s, 0).unwrap()
    }
    fn trade(symbol: &str, s: i64, seq: u64, price: f64) -> ReplayEvent {
        ReplayEvent::new(
            symbol,
            at(s),
            seq,
            MarketEvent::Trade(Tick::new(at(s), price, 1.0, Some(OrderSide::Buy)).with_id(seq)),
        )
    }
    fn snapshot(s: i64, seq: u64, size: f64) -> ReplayEvent {
        ReplayEvent::new(
            "A",
            at(s),
            seq,
            MarketEvent::DepthSnapshot {
                sequence: seq,
                book: OrderBook {
                    ts: at(s),
                    symbol: "A".into(),
                    bids: vec![OrderBookLevel {
                        price: 99.0,
                        size,
                        order_cnt: None,
                    }],
                    asks: vec![],
                },
            },
        )
    }

    #[test]
    fn shared_clock_orders_events_scales_elapsed_time_and_seeks_from_snapshots() {
        let events = vec![
            trade("B", 2, 3, 20.0),
            snapshot(0, 0, 5.0),
            trade("A", 1, 2, 11.0),
            trade("A", 1, 1, 10.0),
            snapshot(2, 4, 7.0),
            trade("A", 3, 5, 12.0),
        ];
        let mut clock = ReplayClock::new(events).unwrap();
        clock.set_speed(2.0).unwrap();
        let due: Vec<_> = clock
            .advance(Duration::from_millis(500))
            .unwrap()
            .iter()
            .map(|e| (e.symbol.as_str(), e.sequence))
            .collect();
        assert_eq!(due, [("A", 0), ("A", 1), ("A", 2)]);
        clock.set_speed(0.0).unwrap();
        assert!(clock.advance(Duration::from_secs(10)).unwrap().is_empty());
        assert_eq!(clock.time(), at(1));
        clock.set_speed(1.0).unwrap();
        let inbox = Arc::new(Inbox::default());
        let a = inbox.subscribe("A".into(), Timeframe::Min1.into(), 100, Some(1.0));
        let b = inbox.subscribe("B".into(), Timeframe::Min1.into(), 100, Some(1.0));
        for cutoff in [3, 1, 2, -1, 3] {
            a.reset_replay(at(cutoff));
            b.reset_replay(at(cutoff));
            let due: Vec<_> = clock.seek(at(cutoff)).collect();
            if cutoff >= 2 {
                assert!(!due.iter().any(|e| e.sequence == 0));
            }
            for event in due {
                event.push_to(if event.symbol == "A" { &a } else { &b });
            }
            let updates = inbox.drain().updates;
            let a = &updates.iter().find(|(id, _)| *id == a.id).unwrap().1;
            let b = &updates.iter().find(|(id, _)| *id == b.id).unwrap().1;
            assert!(a.reset && b.reset);
            assert_eq!(
                a.trades.iter().count(),
                if cutoff >= 3 {
                    3
                } else if cutoff >= 1 {
                    2
                } else {
                    0
                }
            );
            assert_eq!(b.trades.iter().count(), usize::from(cutoff >= 2));
            if cutoff >= 0 {
                assert_eq!(
                    a.depth.as_ref().unwrap().bids[0].size,
                    if cutoff >= 2 { 7.0 } else { 5.0 }
                );
            }
        }
        assert!(clock.is_finished());
        assert!(clock.set_speed(f64::NAN).is_err());
        let previous = clock.time();
        assert!(clock.advance(Duration::MAX).is_err());
        assert_eq!(clock.time(), previous);
        let order = RestingOrder::new(9, OrderSide::Sell, 100.0, 4.0);
        let mut orders = ReplayClock::new(vec![
            ReplayEvent::new(
                "A",
                at(0),
                0,
                MarketEvent::OrderSnapshot {
                    sequence: 0,
                    orders: vec![order.clone()],
                },
            ),
            ReplayEvent::new(
                "A",
                at(1),
                1,
                MarketEvent::Order {
                    sequence: 1,
                    event: OrderEvent::Execute {
                        id: 9,
                        size: 2.0,
                        trade: Some(Tick::new(at(1), 100.0, 2.0, Some(OrderSide::Buy)).with_id(55)),
                    },
                },
            ),
            ReplayEvent::new(
                "A",
                at(2),
                2,
                MarketEvent::OrderSnapshot {
                    sequence: 2,
                    orders: vec![RestingOrder { size: 2.0, ..order }],
                },
            ),
        ])
        .unwrap();
        a.reset_replay(at(2));
        for event in orders.seek(at(2)) {
            event.push_to(&a);
        }
        let (_, updates) = inbox
            .drain()
            .updates
            .into_iter()
            .find(|(id, _)| *id == a.id)
            .unwrap();
        let fills: Vec<_> = updates.trades.iter().collect();
        assert_eq!((fills.len(), fills[0].resting_order_id), (1, Some(9)));
        assert_eq!(updates.orders.unwrap().orders().next().unwrap().size, 2.0);
        let mut fractional = ReplayClock::new(vec![trade("A", 0, 0, 1.0)]).unwrap();
        fractional.set_speed(0.5).unwrap();
        fractional.advance(Duration::from_nanos(1)).unwrap();
        assert_eq!(fractional.time(), at(0));
        fractional.advance(Duration::from_nanos(1)).unwrap();
        assert_eq!(fractional.time(), at(0) + TimeDelta::nanoseconds(1));
    }

    #[test]
    fn recording_driver_routes_two_symbols_and_rewinds_bars_quotes_books_and_heatmap() {
        use super::super::{DepthHistoryConfig, RecordingWriter};
        let order = RestingOrder::new(9, OrderSide::Sell, 20.0, 6.0)
            .with_participant(42)
            .with_placed(at(-30));
        let quote = |symbol: &str, time, sequence, bid| {
            ReplayEvent::new(
                symbol,
                at(time),
                sequence,
                MarketEvent::Quote(Level1Quote {
                    ts: at(time),
                    symbol: symbol.into(),
                    bid,
                    ask: bid + 1.0,
                    bid_size: 3.0,
                    ask_size: 4.0,
                }),
            )
        };
        let delta = |time, sequence, size| {
            let mut event = snapshot(time, sequence, size);
            if let MarketEvent::DepthSnapshot { sequence, book } = event.event {
                event.event = MarketEvent::DepthDelta { sequence, book };
            }
            event
        };
        let events = vec![
            snapshot(0, 0, 5.0),
            ReplayEvent::new(
                "B",
                at(0),
                0,
                MarketEvent::OrderSnapshot {
                    sequence: 0,
                    orders: vec![order.clone()],
                },
            ),
            trade("A", 0, 10, 10.0),
            trade("B", 0, 10, 20.0),
            delta(1, 1, 7.0),
            ReplayEvent::new(
                "B",
                at(1),
                1,
                MarketEvent::Order {
                    sequence: 1,
                    event: OrderEvent::Execute {
                        id: 9,
                        size: 2.0,
                        trade: Some(Tick::new(at(1), 20.0, 2.0, Some(OrderSide::Buy)).with_id(55)),
                    },
                },
            ),
            quote("A", 1, 20, 9.0),
            quote("B", 1, 20, 19.0),
            delta(2, 2, 8.0),
            trade("A", 2, 11, 12.0),
            ReplayEvent::new(
                "B",
                at(2),
                2,
                MarketEvent::Order {
                    sequence: 2,
                    event: OrderEvent::Modify {
                        id: 9,
                        price: None,
                        size: Some(3.0),
                        keep_priority: true,
                    },
                },
            ),
            snapshot(3, 3, 99.0),
            ReplayEvent::new(
                "B",
                at(3),
                3,
                MarketEvent::OrderSnapshot {
                    sequence: 3,
                    orders: vec![RestingOrder {
                        size: 99.0,
                        ..order
                    }],
                },
            ),
            quote("A", 3, 20, 98.0),
            trade("A", 4, 12, 99.0),
        ];
        let mut writer = RecordingWriter::new(Vec::new()).unwrap();
        writer.capture(events.iter().cloned().map(Ok)).unwrap();
        let bytes = writer.finish().unwrap();
        let load = || {
            ReplayDriver::load(
                bytes.as_slice(),
                vec![
                    Instrument::new("A").with_tick_size(1.0),
                    Instrument::new("B").with_tick_size(1.0),
                ],
            )
            .unwrap()
        };
        let connect = |driver: &ReplayDriver| {
            let inbox = Arc::new(Inbox::default());
            let mut sinks = Vec::new();
            for symbol in ["A", "B"] {
                let sink = inbox.subscribe(symbol.into(), Timeframe::Min1.into(), 100, Some(1.0));
                inbox.sample_depth(sink.id, Some(DepthHistoryConfig::default()));
                driver.subscribe(Subscription {
                    id: sink.id,
                    symbol: symbol.into(),
                    timeframe: Timeframe::Min1,
                    clock: Timeframe::Min1.into(),
                    extended_hours: true,
                    conversion: None,
                    sink: sink.clone(),
                });
                sinks.push(sink);
            }
            inbox.drain();
            (inbox, sinks)
        };
        let state = |inbox: &Inbox, sinks: &[Sink]| {
            let mut updates = inbox.drain().updates;
            sinks
                .iter()
                .map(|sink| {
                    let (_, update) = updates
                        .swap_remove(updates.iter().position(|(id, _)| *id == sink.id).unwrap());
                    assert_eq!(
                        (update.rejected, update.rejected_depth, update.late_ticks),
                        (0, 0, 0)
                    );
                    (
                        update
                            .bars
                            .into_values()
                            .map(|(bar, _)| bar)
                            .collect::<Vec<_>>(),
                        update.trades.iter().cloned().collect::<Vec<_>>(),
                        update.quote,
                        update.depth,
                        update
                            .orders
                            .map(|queue| queue.orders().cloned().collect::<Vec<_>>()),
                        update
                            .replay_depth
                            .map(|history| history.columns().cloned().collect())
                            .unwrap_or(update.depth_columns),
                        update.time,
                    )
                })
                .collect::<Vec<_>>()
        };
        let fresh = load();
        let (fresh_inbox, fresh_sinks) = connect(&fresh);
        fresh.set_speed(2.0).unwrap();
        fresh.advance(Duration::from_secs(1)).unwrap(); // 1 wall second * 2 = market second 2.
        let expected = state(&fresh_inbox, &fresh_sinks);
        assert_eq!(
            expected[0].0,
            [crate::model::Bar::new(at(0), 10.0, 12.0, 10.0, 12.0, 2.0)]
        );
        assert_eq!(
            expected[1].0,
            [crate::model::Bar::new(at(0), 20.0, 20.0, 20.0, 20.0, 3.0)]
        );
        assert_eq!(expected[0].3.as_ref().unwrap().bids[0].size, 8.0);
        assert_eq!(expected[1].4.as_ref().unwrap()[0].size, 3.0);
        assert_eq!(expected[1].4.as_ref().unwrap()[0].placed, Some(at(-30)));
        assert_eq!(
            expected[0]
                .5
                .iter()
                .map(|column| column.bids[0].1)
                .collect::<Vec<_>>(),
            [5.0, 7.0]
        );
        assert_eq!(
            expected[1]
                .5
                .iter()
                .map(|column| column.asks[0].1)
                .collect::<Vec<_>>(),
            [6.0, 4.0]
        );
        let replay = load();
        let (inbox, sinks) = connect(&replay);
        let fresh_later = load();
        let (later_inbox, later_sinks) = connect(&fresh_later);
        fresh_later.advance(Duration::from_secs(4)).unwrap();
        let later = state(&later_inbox, &later_sinks);
        assert_eq!(later[1].5.back().unwrap().time, at(3));
        assert_eq!(
            later[1].5.back().unwrap().asks[0].1,
            99.0,
            "the shared clock must sample a quiet symbol's standing book"
        );
        replay.seek(at(4));
        assert_eq!(
            state(&inbox, &sinks),
            later,
            "seek must retain heatmap history before the latest snapshots"
        );
        replay.set_speed(0.0).unwrap();
        replay.seek(at(2)); // Cross both snapshots backward; none of their size 99 may survive.
        assert_eq!(state(&inbox, &sinks), expected);
        replay.advance(Duration::from_secs(20)).unwrap();
        assert_eq!(replay.time(), at(2));
        assert!(
            state(&inbox, &sinks)
                .iter()
                .all(|update| update.6 == Some(at(2)))
        );
        replay.set_speed(0.5).unwrap();
        replay.advance(Duration::from_secs(1)).unwrap();
        let quiet = at(2) + TimeDelta::milliseconds(500);
        assert_eq!(replay.time(), quiet);
        assert!(
            state(&inbox, &sinks)
                .iter()
                .all(|update| update.6 == Some(quiet))
        );
        replay.seek(at(-1));
        assert!(
            state(&inbox, &sinks)
                .iter()
                .all(|update| update.0.is_empty()
                    && update.1.is_empty()
                    && update.2.is_none()
                    && update.3.is_none()
                    && update.4.is_none()
                    && update.5.is_empty())
        );

        let mut capture = RecordingWriter::new(Vec::new()).unwrap();
        assert_eq!(
            capture
                .capture([
                    Ok(events[0].clone()),
                    Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "capture stream failed"
                    )),
                ])
                .unwrap_err()
                .kind(),
            io::ErrorKind::ConnectionReset
        );
        assert!(
            RecordingReader::new(capture.finish().unwrap().as_slice())
                .unwrap()
                .read()
                .unwrap()
                .is_some()
        );

        struct FailedRead;
        impl Read for FailedRead {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "capture stream failed",
                ))
            }
        }
        assert_eq!(
            ReplayDriver::load(bytes.as_slice().chain(FailedRead), Vec::new())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::ConnectionReset
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn readouts_and_unsubscribe_do_not_wait_for_playback_wakers() {
        use std::sync::mpsc;
        let replay = Arc::new(
            ReplayDriver::new(vec![trade("A", 0, 0, 10.0)], vec![Instrument::new("A")]).unwrap(),
        );
        let inbox = Arc::new(Inbox::default());
        let sink = inbox.subscribe("A".into(), Timeframe::Min1.into(), 100, None);
        let id = sink.id;
        replay.subscribe(Subscription {
            id,
            symbol: "A".into(),
            timeframe: Timeframe::Min1,
            clock: Timeframe::Min1.into(),
            extended_hours: true,
            conversion: None,
            sink,
        });
        inbox.drain();
        let (woken, waking) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let resume = Mutex::new(resume);
        inbox.set_waker(move || {
            woken.send(()).unwrap();
            resume.lock().unwrap().recv().unwrap();
        });
        let producer = replay.clone();
        let pushing = std::thread::spawn(move || producer.advance(Duration::ZERO).unwrap());
        waking.recv_timeout(Duration::from_secs(5)).unwrap();
        let (finished, finish) = mpsc::channel();
        let removing = std::thread::spawn(move || {
            let time = replay.time();
            replay.unsubscribe(id);
            finished.send(time).unwrap();
        });
        let result = finish.recv_timeout(Duration::from_secs(5));
        release.send(()).unwrap();
        pushing.join().unwrap();
        removing.join().unwrap();
        assert_eq!(
            result.unwrap(),
            at(0),
            "a UI readout or drop must not wait on its own waker"
        );
    }
}
