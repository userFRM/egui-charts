//! One event clock for host-supplied trades, quotes and depth across instruments.

use super::{FeedError, OrderEvent, RestingOrder, Sink};
use crate::model::{Level1Quote, OrderBook, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{HashMap, HashSet};
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
    DepthSnapshot { sequence: u64, book: OrderBook },
    /// Changed aggregated levels.
    DepthDelta { sequence: u64, book: OrderBook },
    /// A periodic per-order snapshot in queue order, also a depth seek anchor.
    OrderSnapshot {
        sequence: u64,
        orders: Vec<RestingOrder>,
    },
    /// One sequenced resting order change.
    Order { sequence: u64, event: OrderEvent },
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
    events: Vec<ReplayEvent>,
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
            events,
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

    /// Seeks inclusively to `time`. Before pushing these events, call `Sink::reset_replay` on
    /// every participating sink. Trades rebuild bars from the beginning; book messages before
    /// each symbol's latest snapshot are skipped. A symbol without a snapshot starts empty and
    /// follows normal gap/resync rules. Subsequent `advance` returns only events after the seek.
    pub fn seek(&mut self, time: DateTime<Utc>) -> impl Iterator<Item = ReplayEvent> {
        self.time = time;
        self.fractional_nanos = 0.0;
        self.cursor = self.events.partition_point(|e| e.time <= time);
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
}
