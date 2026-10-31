//! Individual resting orders behind the demo's synthetic book, for the instrument whose feed
//! sends order-level depth. Each changed level of the aggregate book becomes order events pushed
//! through the sink, with placement times, and a few large block orders rest beside the levels,
//! so the size tiers, the largest-order profile and the order age filters have something to show.

use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::{OrderEvent, RestingOrder, Sink};
use egui_charts::model::{OrderBook, OrderSide};
use std::collections::{BTreeMap, VecDeque};

/// Orders kept at a price before a size increase grows the newest of them instead.
const ORDERS_PER_LEVEL: usize = 8;

/// Block orders resting at once; a new one beyond this cancels the oldest.
const BLOCKS: usize = 8;

pub(super) struct Orders {
    tick_size: f64,
    random: u64,
    /// Sequence of the last order event, or of the snapshot before the first.
    sequence: u64,
    next_id: u64,
    /// The orders at each (is bid, price in ticks), front of the queue first, each with whether
    /// it is a block order. The other orders at a price sum to the aggregate book's size there.
    levels: BTreeMap<(bool, i64), Vec<(RestingOrder, bool)>>,
    /// Block orders by id and level, oldest first.
    blocks: VecDeque<(u64, (bool, i64))>,
}

impl Orders {
    /// Splits each level of `book`, the aggregate snapshot, into orders of 5 to 30 placed up to
    /// 15 minutes before it.
    pub(super) fn new(book: &OrderBook, tick_size: f64, seed: u64) -> Self {
        let mut orders = Self {
            tick_size,
            // Away from the aggregate book's stream, which starts from the same seed.
            random: seed.rotate_left(32) | 1,
            sequence: 0,
            next_id: 0,
            levels: BTreeMap::new(),
            blocks: VecDeque::new(),
        };
        for (bid, levels) in [(true, &book.bids), (false, &book.asks)] {
            for level in levels {
                let mut queue = Vec::new();
                let mut left = level.size;
                while left > 0.0 {
                    let size = left.min((5 + orders.random() % 26) as f64);
                    left -= size;
                    let placed = book.ts - TimeDelta::seconds((orders.random() % 900) as i64);
                    queue.push((orders.order(bid, level.price, size, placed), false));
                }
                queue.sort_by_key(|(order, _)| order.placed);
                orders
                    .levels
                    .insert((bid, orders.ticks(level.price)), queue);
            }
        }
        orders
    }

    /// The sequence and every resting order, in queue order at each price.
    pub(super) fn snapshot(&self) -> (u64, Vec<RestingOrder>) {
        let orders = self.levels.values().flatten();
        (
            self.sequence,
            orders.map(|(order, _)| order.clone()).collect(),
        )
    }

    /// Pushes the order events that turn the book into `delta`, a change of the aggregate book.
    /// `trade` is the aggressor and the price in ticks when a trade made the change: the levels
    /// it took are executed, oldest order first, rather than cancelled. Between trades a block
    /// order is sometimes placed behind the touch.
    pub(super) fn apply(
        &mut self,
        sink: &Sink,
        delta: &OrderBook,
        trade: Option<(OrderSide, i64)>,
    ) {
        for (bid, levels) in [(true, &delta.bids), (false, &delta.asks)] {
            for level in levels {
                let key = (bid, self.ticks(level.price));
                let executed = match trade {
                    Some((OrderSide::Buy, price)) => !bid && key.1 <= price,
                    Some((OrderSide::Sell, price)) => bid && key.1 >= price,
                    None => false,
                };
                self.set(sink, delta.ts, key, level.price, level.size, executed);
            }
        }
        if trade.is_none() && self.random().is_multiple_of(96) {
            self.place_block(sink, delta.ts);
        }
    }

    /// Brings the orders at `key` other than blocks to `size`: new size joins the back of the
    /// queue, executions take the oldest orders and cancels the newest, and a level that is gone
    /// takes its blocks with it.
    fn set(
        &mut self,
        sink: &Sink,
        ts: DateTime<Utc>,
        key: (bool, i64),
        price: f64,
        size: f64,
        executed: bool,
    ) {
        let mut queue = self.levels.remove(&key).unwrap_or_default();
        let base: f64 = queue
            .iter()
            .filter(|(_, block)| !block)
            .map(|(o, _)| o.size)
            .sum();
        let mut events = Vec::new();
        if size <= 0.0 {
            for (order, block) in queue.drain(..) {
                if block {
                    self.blocks.retain(|&(id, _)| id != order.id);
                }
                events.push(if executed {
                    OrderEvent::Execute {
                        id: order.id,
                        size: order.size,
                        trade: None,
                    }
                } else {
                    OrderEvent::Cancel { id: order.id }
                });
            }
        } else if size > base {
            let count = queue.iter().filter(|(_, block)| !block).count();
            let order = if count < ORDERS_PER_LEVEL {
                let order = self.order(key.0, price, size - base, ts);
                events.push(OrderEvent::Add(order.clone()));
                order
            } else {
                // An increase loses priority, so the order moves to the back.
                let at = queue.iter().rposition(|(_, block)| !block);
                let (mut order, _) = queue.remove(at.expect("a full level has orders"));
                order.size += size - base;
                events.push(OrderEvent::Modify {
                    id: order.id,
                    price: None,
                    size: Some(order.size),
                    keep_priority: false,
                });
                order
            };
            queue.push((order, false));
        } else {
            let mut cut = base - size;
            let len = queue.len();
            for i in 0..len {
                let (order, block) = &mut queue[if executed { i } else { len - 1 - i }];
                if *block || cut <= 0.0 {
                    continue;
                }
                let take = cut.min(order.size);
                cut -= take;
                order.size -= take;
                events.push(match (executed, order.size > 0.0) {
                    (true, _) => OrderEvent::Execute {
                        id: order.id,
                        size: take,
                        trade: None,
                    },
                    (false, true) => OrderEvent::Modify {
                        id: order.id,
                        price: None,
                        size: Some(order.size),
                        keep_priority: true,
                    },
                    (false, false) => OrderEvent::Cancel { id: order.id },
                });
            }
            queue.retain(|(order, _)| order.size > 0.0);
        }
        if !queue.is_empty() {
            self.levels.insert(key, queue);
        }
        for event in events {
            self.push(sink, ts, event);
        }
    }

    /// Rests a block of 100 to 399 at a quoted level 10 to 60 ticks behind one side's touch,
    /// cancelling the oldest block beyond [`BLOCKS`]. The synthetic price moves up to 5 ticks a
    /// trade, so a block nearer the touch is swept within seconds. Only at a quoted level, so the
    /// aggregate book removing that level removes the block too and the book cannot cross.
    fn place_block(&mut self, sink: &Sink, ts: DateTime<Utc>) {
        let bid = self.random().is_multiple_of(2);
        let distance = 10 + (self.random() % 51) as i64;
        let size = (100 + self.random() % 300) as f64;
        // Asks sort before bids, so the best ask is the first key and the best bid the last.
        let touch = if bid {
            self.levels.keys().next_back()
        } else {
            self.levels.keys().next()
        };
        let Some(&(side, touch)) = touch.filter(|(side, _)| *side == bid) else {
            return;
        };
        let key = (
            side,
            if bid {
                touch - distance
            } else {
                touch + distance
            },
        );
        let Some(price) = self.levels.get(&key).map(|queue| queue[0].0.price) else {
            return;
        };
        let order = self.order(bid, price, size, ts);
        self.blocks.push_back((order.id, key));
        if let Some(queue) = self.levels.get_mut(&key) {
            queue.push((order.clone(), true));
        }
        self.push(sink, ts, OrderEvent::Add(order));
        if self.blocks.len() > BLOCKS
            && let Some((id, key)) = self.blocks.pop_front()
        {
            if let Some(queue) = self.levels.get_mut(&key) {
                queue.retain(|(order, _)| order.id != id);
            }
            self.push(sink, ts, OrderEvent::Cancel { id });
        }
    }

    fn push(&mut self, sink: &Sink, ts: DateTime<Utc>, event: OrderEvent) {
        self.sequence += 1;
        sink.push_order_event(self.sequence, ts, event);
    }

    fn order(&mut self, bid: bool, price: f64, size: f64, placed: DateTime<Utc>) -> RestingOrder {
        self.next_id += 1;
        let side = if bid { OrderSide::Buy } else { OrderSide::Sell };
        RestingOrder::new(self.next_id, side, price, size).with_placed(placed)
    }

    fn ticks(&self, price: f64) -> i64 {
        (price / self.tick_size).round() as i64
    }

    fn random(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }
}
