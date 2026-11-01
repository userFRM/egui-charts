//! Synchronized depth shared by the detectors, in integer ticks.

use std::collections::BTreeMap;

use crate::data::{OrderQueue, depth::Book};
use crate::model::OrderSide;

/// Bid or ask, and its integer price tick.
pub(super) type Price = (bool, i64);

#[derive(Clone, Default)]
pub(super) struct Level {
    pub size: f64,
    pub largest: Option<(u64, f64)>,
}

#[derive(Default)]
pub(super) struct Depth {
    pub levels: BTreeMap<Price, Level>,
    pub exact: bool,
}

impl Depth {
    pub fn from_book(book: &Book, tick: f64) -> Self {
        Self {
            levels: book
                .levels()
                .into_iter()
                .flatten()
                .map(|(bid, level)| {
                    (
                        (bid, (level.price / tick).round() as i64),
                        Level {
                            size: level.size,
                            largest: None,
                        },
                    )
                })
                .collect(),
            exact: false,
        }
    }

    pub fn from_orders(queue: &OrderQueue, tick: f64) -> Self {
        let mut view = Self {
            exact: true,
            ..Self::default()
        };
        for order in queue.orders() {
            let level = view
                .levels
                .entry(price(order.side, order.price, tick))
                .or_default();
            level.size = (level.size + order.size).min(f64::MAX);
            if level.largest.is_none_or(|(_, size)| order.size > size) {
                level.largest = Some((order.id, order.size));
            }
        }
        view
    }

    pub fn inside(&self, bid: bool) -> Option<i64> {
        let mut side = self.levels.range((bid, i64::MIN)..=(bid, i64::MAX));
        (if bid { side.next_back() } else { side.next() }).map(|(&(.., price), _)| price)
    }

    pub fn size(&self, price: Price) -> f64 {
        self.levels.get(&price).map_or(0.0, |level| level.size)
    }

    pub fn near(&self, price: Price, ticks: u64) -> bool {
        self.inside(price.0)
            .is_some_and(|inside| inside.abs_diff(price.1) <= ticks)
    }
}

pub(super) fn price(side: OrderSide, price: f64, tick: f64) -> Price {
    (side == OrderSide::Buy, (price / tick).round() as i64)
}

pub(super) fn resting(fill: &crate::model::Tick, tick: f64) -> Option<Price> {
    fill.side
        .map(|side| (side == OrderSide::Sell, (fill.price / tick).round() as i64))
}

pub(super) fn side(price: Price) -> OrderSide {
    if price.0 {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}

pub(super) fn beyond(price: Price, traded: i64) -> bool {
    if price.0 {
        traded < price.1
    } else {
        traded > price.1
    }
}
