//! Individual resting orders and their queue priority. Aggregated-only feeds need no order data.

use super::FeedError;
use super::depth::{Lost, MAX_LEVELS, key};
use crate::model::{OrderBook, OrderBookLevel, OrderSide, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};

/// One resting order. Snapshot order establishes queue priority at each price.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RestingOrder {
    /// Exchange order id, unique within the subscription's current book.
    pub id: u64,
    /// Buy for bids, sell for asks.
    pub side: OrderSide,
    /// Limit price.
    pub price: f64,
    /// Remaining size, finite and strictly positive.
    pub size: f64,
    /// Host-defined identity of the participant that placed the order, such as a broker or
    /// market maker number, when the feed supplies one. Never inferred.
    #[serde(default)]
    pub participant: Option<u64>,
    /// When the venue placed the order, when the feed supplies it. A price change that costs
    /// priority is still the same order unless the venue says otherwise by sending a new one.
    #[serde(default)]
    pub placed: Option<DateTime<Utc>>,
}

impl RestingOrder {
    /// A resting order with its remaining size and no participant or placement time.
    pub fn new(id: u64, side: OrderSide, price: f64, size: f64) -> Self {
        Self {
            id,
            side,
            price,
            size,
            participant: None,
            placed: None,
        }
    }

    /// The same order, placed by `participant`.
    pub fn with_participant(mut self, participant: u64) -> Self {
        self.participant = Some(participant);
        self
    }

    /// The same order, placed at `time`.
    pub fn with_placed(mut self, time: DateTime<Utc>) -> Self {
        self.placed = Some(time);
        self
    }
}

/// Which resting orders count when they are summed into levels, for ladders, heatmaps and the
/// book size study. Every field narrows the set. Filters are exact: an order whose placement
/// time or participant the feed did not supply is neither guessed at nor dropped for lacking the
/// field it is not filtered on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderFilter {
    /// Orders smaller than this are left out before aggregation. Must be finite and not
    /// negative, else no order counts.
    pub min_size: f64,
    /// Only orders that have rested at least this long at the time of the book, measured from
    /// the placement time the feed supplied. Orders without one cannot be shown to have rested
    /// that long and are left out. A negative age counts as zero.
    pub min_age: Option<TimeDelta>,
    /// Orders of these participants are left out. Orders without a participant stay.
    pub exclude_participants: Vec<u64>,
}

impl OrderFilter {
    /// Whether `order` counts in a book as it stood at `now`.
    pub fn accepts(&self, order: &RestingOrder, now: DateTime<Utc>) -> bool {
        self.min_size.is_finite()
            && self.min_size >= 0.0
            && order.size >= self.min_size
            && self.min_age.is_none_or(|age| {
                order
                    .placed
                    .is_some_and(|placed| now - placed >= age.max(TimeDelta::zero()))
            })
            && order
                .participant
                .is_none_or(|p| !self.exclude_participants.contains(&p))
    }
}

/// Why an order-level view cannot be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OrderFilterUnavailable {
    /// The feed sends aggregate depth only, so no order is known.
    AggregateDepth,
    /// The filter excludes participants but no held order names one.
    Participants,
    /// The filter has a minimum age but no held order has a placement time.
    PlacementTimes,
}

impl std::fmt::Display for OrderFilterUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AggregateDepth => "the feed sends aggregate depth only",
            Self::Participants => "no order names its participant",
            Self::PlacementTimes => "no order has a placement time",
        })
    }
}

impl std::error::Error for OrderFilterUnavailable {}

/// Two sizes that mark an individual resting order as notable and as large.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderSizeTiers {
    /// Orders of at least this size are notable.
    pub notable: f64,
    /// Orders of at least this size are large, whatever `notable` is.
    pub large: f64,
}

/// How an order's size compares with [`OrderSizeTiers`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderSizeTier {
    /// At least the notable size and below the large size.
    Notable,
    /// At least the large size.
    Large,
}

impl OrderSizeTiers {
    /// The tier of an order of `size`, or `None` below both. Sizes that are not numbers have none.
    pub fn tier(&self, size: f64) -> Option<OrderSizeTier> {
        if size >= self.large {
            Some(OrderSizeTier::Large)
        } else if size >= self.notable {
            Some(OrderSizeTier::Notable)
        } else {
            None
        }
    }
}

/// A sequenced change to one resting order. Unknown ids or invalid changes lose the book.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderEvent {
    /// Adds an order at the back of its price queue. Duplicate ids are invalid.
    Add(RestingOrder),
    /// Replaces price or remaining size. A changed price always loses priority; otherwise
    /// `keep_priority` preserves the position only when the venue says it does. Size zero cancels.
    Modify {
        /// Venue identifier of the order to change.
        id: u64,
        /// Replacement limit price, or `None` to retain the current price.
        price: Option<f64>,
        /// Replacement remaining size, or `None` to retain it.
        size: Option<f64>,
        /// Whether the venue preserves this order's queue position after the change.
        keep_priority: bool,
    },
    /// Cancels the remaining order.
    Cancel {
        /// Venue identifier of the order to change.
        id: u64,
    },
    /// Removes `size` from an order, keeping its priority on a partial fill. A known trade is
    /// pushed through the trade path with `resting_order_id` set. Never also push it without a
    /// unique trade id. Size must not exceed the order's remaining size.
    Execute {
        /// Venue identifier of the order to change.
        id: u64,
        /// Executed quantity to remove from the order.
        size: f64,
        /// Execution associated with this fill, when supplied.
        trade: Option<Tick>,
    },
}

/// The per-order book, including queue order. Snapshot and event methods on `Sink` maintain it;
/// `Chart::order_queue` reads it. At most 10,000 orders per side, with bounded resync buffering.
#[derive(Clone, Debug)]
pub struct OrderQueue {
    tick: Option<f64>,
    sequence: Option<u64>,
    requested: bool,
    time: DateTime<Utc>,
    orders: Vec<RestingOrder>,
    held: VecDeque<(u64, DateTime<Utc>, OrderEvent)>,
    executions: VecDeque<Tick>,
}

impl OrderQueue {
    /// An empty book, keying prices by the instrument's positive finite tick size when known.
    pub fn new(tick_size: Option<f64>) -> Self {
        Self {
            tick: tick_size.filter(|t| t.is_finite() && *t > 0.0),
            sequence: None,
            requested: false,
            time: DateTime::default(),
            orders: Vec::new(),
            held: VecDeque::new(),
            executions: VecDeque::new(),
        }
    }

    /// Last applied sequence, or `None` until a usable snapshot or after a gap.
    pub fn sequence(&self) -> Option<u64> {
        self.sequence
    }

    /// All held orders, in queue priority order. A lost book contains none.
    pub fn orders(&self) -> impl Iterator<Item = &RestingOrder> {
        self.orders.iter()
    }

    /// Orders at `price`, first in queue first. Prices share the aggregated book's tick rounding.
    pub fn queue(&self, side: OrderSide, price: f64) -> impl Iterator<Item = &RestingOrder> {
        let price = key(self.tick, price);
        self.orders
            .iter()
            .filter(move |o| o.side == side && key(self.tick, o.price) == price)
    }

    /// Largest resting order at a price, if any. Ties keep the earliest queue priority.
    pub fn largest_order(&self, side: OrderSide, price: f64) -> Option<&RestingOrder> {
        self.queue(side, price)
            .reduce(|a, b| if b.size > a.size { b } else { a })
    }

    /// Trades produced by accepted executions, drained once. Each fill names its resting order.
    /// At most 10,000 pending fills; the oldest make room if the host does not drain them.
    pub fn take_executions(&mut self) -> Vec<Tick> {
        self.executions.drain(..).collect()
    }

    /// Aggregated levels for ladders and heatmaps, counting only orders at least `minimum_size`.
    /// Invalid thresholds return an empty book. Counts and totals refer to the selected orders.
    pub fn aggregate(&self, symbol: impl Into<String>, minimum_size: f64) -> OrderBook {
        let filter = OrderFilter {
            min_size: minimum_size,
            ..OrderFilter::default()
        };
        self.sum(symbol.into(), &filter)
    }

    /// The levels of the orders `filter` accepts, as the book stood at its last update: what a
    /// ladder, heatmap and book size study built on the same filter show. Fails when the held
    /// orders carry none of the metadata a filter field needs, rather than showing the unfiltered
    /// book as if it were filtered. An empty book is never unavailable.
    pub fn filtered(
        &self,
        symbol: impl Into<String>,
        filter: &OrderFilter,
    ) -> Result<OrderBook, OrderFilterUnavailable> {
        if !self.orders.is_empty() {
            if !filter.exclude_participants.is_empty()
                && self.orders.iter().all(|o| o.participant.is_none())
            {
                return Err(OrderFilterUnavailable::Participants);
            }
            if filter.min_age.is_some() && self.orders.iter().all(|o| o.placed.is_none()) {
                return Err(OrderFilterUnavailable::PlacementTimes);
            }
        }
        Ok(self.sum(symbol.into(), filter))
    }

    /// The largest order `filter` accepts at each price of each side, bids from the highest
    /// price down and then asks from the lowest up. Ties keep the earliest queue priority.
    pub fn largest_orders(&self, filter: &OrderFilter) -> Vec<&RestingOrder> {
        let mut largest: BTreeMap<(bool, u64), &RestingOrder> = BTreeMap::new();
        for order in self.orders.iter().filter(|o| filter.accepts(o, self.time)) {
            let bid = order.side == OrderSide::Buy;
            let level = largest.entry((!bid, key(self.tick, order.price)));
            level
                .and_modify(|held| {
                    if order.size > held.size {
                        *held = order;
                    }
                })
                .or_insert(order);
        }
        let (mut bids, asks): (Vec<_>, Vec<_>) =
            largest.into_iter().partition(|((ask, _), _)| !ask);
        bids.reverse();
        bids.into_iter()
            .chain(asks)
            .map(|(_, order)| order)
            .collect()
    }

    /// The levels of [`largest_orders`](Self::largest_orders), one order each: what a depth
    /// history of individual orders records next to the sums of [`filtered`](Self::filtered).
    pub(crate) fn largest_levels(&self, filter: &OrderFilter) -> OrderBook {
        let largest = self.largest_orders(filter);
        let levels = |side| {
            largest
                .iter()
                .filter(|o| o.side == side)
                .map(|o| OrderBookLevel {
                    price: o.price,
                    size: o.size,
                    order_cnt: Some(1),
                })
                .collect()
        };
        OrderBook {
            ts: self.time,
            symbol: String::new(),
            bids: levels(OrderSide::Buy),
            asks: levels(OrderSide::Sell),
        }
    }

    fn sum(&self, symbol: String, filter: &OrderFilter) -> OrderBook {
        let mut bids: BTreeMap<u64, OrderBookLevel> = BTreeMap::new();
        let mut asks: BTreeMap<u64, OrderBookLevel> = BTreeMap::new();
        for order in self.orders.iter().filter(|o| filter.accepts(o, self.time)) {
            let side = if order.side == OrderSide::Buy {
                &mut bids
            } else {
                &mut asks
            };
            let level = side
                .entry(key(self.tick, order.price))
                .or_insert(OrderBookLevel {
                    price: order.price,
                    size: 0.0,
                    order_cnt: Some(0),
                });
            level.size = (level.size + order.size).min(f64::MAX);
            level.order_cnt = level.order_cnt.map(|n| n + 1);
        }
        OrderBook {
            ts: self.time,
            symbol,
            bids: bids.into_values().rev().collect(),
            asks: asks.into_values().collect(),
        }
    }

    /// Replaces all orders, then applies buffered events above `sequence`. A malformed snapshot
    /// clears the book. Errors leave it empty and ready for a replacement snapshot.
    pub fn snapshot(
        &mut self,
        sequence: u64,
        time: DateTime<Utc>,
        orders: Vec<RestingOrder>,
    ) -> Result<(), FeedError> {
        self.set_snapshot(sequence, time, orders)
            .map_err(|_| invalid())
    }

    /// Applies exactly the next sequence. Repeated sequences are ignored. A gap clears the book
    /// and buffers at most 10,000 following events for the next snapshot.
    pub fn apply(
        &mut self,
        sequence: u64,
        time: DateTime<Utc>,
        event: OrderEvent,
    ) -> Result<(), FeedError> {
        self.event(sequence, time, event).map_err(|_| invalid())
    }

    pub(crate) fn set_snapshot(
        &mut self,
        sequence: u64,
        time: DateTime<Utc>,
        orders: Vec<RestingOrder>,
    ) -> Result<(), Lost> {
        let mut ids = HashSet::new();
        let bids = orders.iter().filter(|o| o.side == OrderSide::Buy).count();
        if bids > MAX_LEVELS
            || orders.len() - bids > MAX_LEVELS
            || orders.iter().any(|o| !valid(o) || !ids.insert(o.id))
        {
            return Err(self.lose(true));
        }
        self.orders = orders;
        self.sequence = Some(sequence);
        self.requested = false;
        self.time = time;
        let held = std::mem::take(&mut self.held);
        let mut result = Ok(());
        for (sequence, time, event) in held {
            result = result.and(self.event(sequence, time, event));
        }
        result
    }

    pub(crate) fn event(
        &mut self,
        sequence: u64,
        time: DateTime<Utc>,
        event: OrderEvent,
    ) -> Result<(), Lost> {
        if self.sequence.is_some_and(|last| sequence <= last) {
            return Ok(());
        }
        if self.sequence.and_then(|s| s.checked_add(1)) != Some(sequence) {
            if self.held.iter().any(|(held, _, _)| *held == sequence) {
                return Err(Lost {
                    invalid: false,
                    request: false,
                });
            }
            let lost = self.lose(false);
            if self
                .held
                .back()
                .is_some_and(|(s, _, _)| s.checked_add(1) != Some(sequence))
            {
                self.held.clear();
            }
            self.held.push_back((sequence, time, event));
            if self.held.len() > MAX_LEVELS {
                self.held.pop_front();
            }
            return Err(lost);
        }
        if self.change(event).is_err() {
            return Err(self.lose(true));
        }
        self.sequence = Some(sequence);
        self.time = time;
        Ok(())
    }

    fn change(&mut self, event: OrderEvent) -> Result<(), FeedError> {
        match event {
            OrderEvent::Add(order) => {
                if !valid(&order)
                    || self.orders.iter().any(|o| o.id == order.id)
                    || self.orders.iter().filter(|o| o.side == order.side).count() == MAX_LEVELS
                {
                    return Err(invalid());
                }
                self.orders.push(order);
            }
            OrderEvent::Modify {
                id,
                price,
                size,
                keep_priority,
            } => {
                let at = self
                    .orders
                    .iter()
                    .position(|o| o.id == id)
                    .ok_or_else(invalid)?;
                let mut order = self.orders[at].clone();
                let old_price = key(self.tick, order.price);
                if let Some(price) = price {
                    order.price = price;
                }
                if let Some(size) = size {
                    order.size = size;
                }
                if !order.price.is_finite() || !order.size.is_finite() || order.size < 0.0 {
                    return Err(invalid());
                }
                if order.size == 0.0 {
                    self.orders.remove(at);
                } else if !keep_priority || old_price != key(self.tick, order.price) {
                    self.orders.remove(at);
                    self.orders.push(order);
                } else {
                    self.orders[at] = order;
                }
            }
            OrderEvent::Cancel { id } => {
                let at = self
                    .orders
                    .iter()
                    .position(|o| o.id == id)
                    .ok_or_else(invalid)?;
                self.orders.remove(at);
            }
            OrderEvent::Execute { id, size, trade } => {
                let at = self
                    .orders
                    .iter()
                    .position(|o| o.id == id)
                    .ok_or_else(invalid)?;
                let order = &mut self.orders[at];
                if !size.is_finite()
                    || size <= 0.0
                    || size > order.size
                    || trade.as_ref().is_some_and(|t| {
                        !super::trades::valid(t)
                            || t.size != size
                            || key(self.tick, t.price) != key(self.tick, order.price)
                    })
                {
                    return Err(invalid());
                }
                if let Some(mut trade) = trade {
                    trade.resting_order_id = Some(id);
                    if self.executions.len() == MAX_LEVELS {
                        self.executions.pop_front();
                    }
                    self.executions.push_back(trade);
                }
                order.size -= size;
                if order.size == 0.0 {
                    self.orders.remove(at);
                }
            }
        }
        Ok(())
    }

    fn lose(&mut self, invalid: bool) -> Lost {
        self.sequence = None;
        self.orders.clear();
        Lost {
            invalid,
            request: !std::mem::replace(&mut self.requested, true),
        }
    }
}

fn valid(order: &RestingOrder) -> bool {
    order.price.is_finite() && order.size.is_finite() && order.size > 0.0
}
fn invalid() -> FeedError {
    FeedError::InvalidData("invalid or unsequenced order book".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use OrderSide::{Buy, Sell};

    #[test]
    fn orders_preserve_priority_and_derive_levels() {
        let orders = vec![
            RestingOrder::new(1, Buy, 100.0, 3.0),
            RestingOrder::new(2, Buy, 100.0, 5.0),
            RestingOrder::new(3, Sell, 101.0, 7.0),
        ];
        #[rustfmt::skip]
        let cases = [
            ("partial execution", 8, OrderEvent::Execute { id: 2, size: 2.0, trade: None }, vec![(100.0, 6.0, 2)], vec![1, 2], false),
            ("price change", 8, OrderEvent::Modify { id: 1, price: Some(99.0), size: None, keep_priority: true }, vec![(100.0, 5.0, 1), (99.0, 3.0, 1)], vec![2], false),
            ("keeps priority", 8, OrderEvent::Modify { id: 1, price: None, size: Some(2.0), keep_priority: true }, vec![(100.0, 7.0, 2)], vec![1, 2], false),
            ("loses priority", 8, OrderEvent::Modify { id: 1, price: None, size: Some(4.0), keep_priority: false }, vec![(100.0, 9.0, 2)], vec![2, 1], false),
            ("invalid zero-size modify", 8, OrderEvent::Modify { id: 1, price: Some(f64::NAN), size: Some(0.0), keep_priority: true }, vec![], vec![], true),
            ("gap", 9, OrderEvent::Cancel { id: 1 }, vec![], vec![], true),
            ("unknown cancel", 8, OrderEvent::Cancel { id: 99 }, vec![], vec![], true),
            ("oversized execution", 8, OrderEvent::Execute { id: 1, size: 4.0, trade: None }, vec![], vec![], true),
        ];
        for (name, seq, event, expected, queue, lost) in cases {
            let mut book = OrderQueue::new(Some(0.25));
            book.snapshot(7, DateTime::default(), orders.clone())
                .unwrap();
            assert_eq!(
                book.apply(seq, DateTime::default(), event).is_err(),
                lost,
                "{name}"
            );
            let levels = book.aggregate("A", 0.0);
            let bids: Vec<_> = levels
                .bids
                .iter()
                .map(|l| (l.price, l.size, l.order_cnt.unwrap()))
                .collect();
            assert_eq!(bids, expected, "{name}");
            assert_eq!(
                book.queue(Buy, 100.0).map(|o| o.id).collect::<Vec<_>>(),
                queue,
                "{name}"
            );
            if !lost {
                assert_eq!(levels.asks[0].size, 7.0);
            }
        }
        let mut book = OrderQueue::new(None);
        book.snapshot(7, DateTime::default(), orders.clone())
            .unwrap();
        assert_eq!(book.aggregate("A", 4.0).bids[0].size, 5.0);
        assert_eq!(book.largest_order(Buy, 100.0).unwrap().id, 2);
        assert!(
            book.apply(9, DateTime::default(), OrderEvent::Cancel { id: 1 })
                .is_err()
        );
        book.snapshot(8, DateTime::default(), orders).unwrap();
        assert_eq!(
            book.queue(Buy, 100.0).map(|o| o.id).collect::<Vec<_>>(),
            [2]
        );
        let full: Vec<_> = (0..MAX_LEVELS)
            .map(|i| RestingOrder::new(i as u64, Buy, 100.0, 1.0))
            .collect();
        book.snapshot(10, DateTime::default(), full).unwrap();
        assert!(
            book.apply(
                11,
                DateTime::default(),
                OrderEvent::Add(RestingOrder::new(10_001, Buy, 99.0, 1.0))
            )
            .is_err()
        );
        assert_eq!(book.orders().count(), 0);
    }
}
