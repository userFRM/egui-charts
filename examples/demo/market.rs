//! A synthetic limit order book that follows the trades of a price process: each trade takes the
//! liquidity in its way, liquidity providers quote again around the new last price, and orders
//! are added and cancelled near the best bid and ask between trades. Every change is numbered
//! as a depth delta, so a feed can push the book through the same contract as an exchange's.
//!
//! The benches include this file to measure a busy instrument; it uses only the library's API.

use chrono::{DateTime, Utc};
use egui_charts::model::{Level1Quote, OrderBook, OrderBookLevel, TradeSide};
use std::collections::BTreeMap;

/// Price levels quoted on each side.
const DEPTH: i64 = 200;

pub struct Market {
    symbol: String,
    tick_size: f64,
    random: u64,
    /// Sequence of the last snapshot or delta.
    sequence: u64,
    /// Resting size per price in ticks.
    bids: BTreeMap<i64, f64>,
    asks: BTreeMap<i64, f64>,
    /// The levels the last event changed, reused for every delta.
    delta: OrderBook,
}

impl Market {
    /// A book quoted one tick around `last`, a price in ticks of `tick_size`.
    pub fn new(symbol: &str, tick_size: f64, last: i64, seed: u64) -> Self {
        let mut market = Self {
            symbol: symbol.into(),
            tick_size,
            random: seed | 1,
            sequence: 0,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            delta: OrderBook {
                ts: DateTime::default(),
                symbol: symbol.into(),
                bids: Vec::new(),
                asks: Vec::new(),
            },
        };
        for distance in 0..DEPTH {
            let bid = market.resting(distance);
            market.bids.insert(last - 1 - distance, bid);
            let ask = market.resting(distance);
            market.asks.insert(last + 1 + distance, ask);
        }
        market
    }

    /// The whole book and its sequence, for a new subscription or a resync.
    pub fn snapshot(&self, ts: DateTime<Utc>) -> (u64, OrderBook) {
        let level = |(&price, &size): (&i64, &f64)| self.level(price, size);
        let book = OrderBook {
            ts,
            symbol: self.symbol.clone(),
            bids: self.bids.iter().rev().map(level).collect(),
            asks: self.asks.iter().map(level).collect(),
        };
        (self.sequence, book)
    }

    /// The best bid and ask. A side without levels quotes nothing at size 0.
    pub fn quote(&self, ts: DateTime<Utc>) -> Level1Quote {
        let bid = self.bids.last_key_value().map(|(&p, &s)| (p, s));
        let ask = self.asks.first_key_value().map(|(&p, &s)| (p, s));
        let price = |level: Option<(i64, f64)>| level.map_or(f64::NAN, |(p, _)| self.price(p));
        Level1Quote {
            ts,
            symbol: self.symbol.clone(),
            bid: price(bid),
            bid_size: bid.map_or(0.0, |(_, s)| s),
            ask: price(ask),
            ask_size: ask.map_or(0.0, |(_, s)| s),
        }
    }

    /// A trade of `size` at `price` ticks, aggressed by `side`. A buy takes every ask below the
    /// price and `size` from the ask at it, and the bids at or above it are gone; a sell does the
    /// same the other way. The book then quotes again one tick from the price on the passive
    /// side and beyond it on the side taken, if nothing is left there. Returns the delta.
    pub fn trade(
        &mut self,
        ts: DateTime<Utc>,
        price: i64,
        side: TradeSide,
        size: f64,
    ) -> (u64, &OrderBook) {
        self.begin(ts);
        // +1 when the asks are taken, -1 when the bids are.
        let (taken, direction) = match side {
            TradeSide::Buy => (false, 1),
            TradeSide::Sell => (true, -1),
        };
        let swept = self.levels(taken, |level| (level - price) * direction < 0);
        swept
            .into_iter()
            .for_each(|level| self.set(taken, level, 0.0));
        let left = self
            .side(taken)
            .get(&price)
            .map_or(0.0, |resting| resting - size);
        self.set(taken, price, left.max(0.0));
        if self.levels(taken, |_| true).is_empty() {
            let size = self.resting(0);
            self.set(taken, price + direction, size);
        }
        let crossed = self.levels(!taken, |level| (level - price) * direction >= 0);
        crossed
            .into_iter()
            .for_each(|level| self.set(!taken, level, 0.0));
        if !self.side(!taken).contains_key(&(price - direction)) {
            let size = self.resting(0);
            self.set(!taken, price - direction, size);
        }
        self.trim();
        self.end()
    }

    /// An order added at, or cancelled from, a level near the best bid or ask. Returns the delta.
    pub fn churn(&mut self, ts: DateTime<Utc>) -> (u64, &OrderBook) {
        self.begin(ts);
        let bid = self.random().is_multiple_of(2);
        // Geometric distance from the touch: most activity is within a few ticks of it.
        let mut distance = 0;
        while distance < DEPTH - 1 && self.random() % 10 < 7 {
            distance += 1;
        }
        let touch = match bid {
            true => self.bids.last_key_value().map(|(&p, _)| p),
            false => self.asks.first_key_value().map(|(&p, _)| p),
        };
        if let Some(touch) = touch {
            let level = if bid {
                touch - distance
            } else {
                touch + distance
            };
            let resting = self.side(bid).get(&level).copied().unwrap_or(0.0);
            let change = (1 + self.random() % 40) as f64;
            let size = if self.random() % 10 < 6 {
                resting + change
            } else {
                (resting - change).max(0.0)
            };
            // The touch stays quoted, so a cancel there only reduces it.
            let size = if distance == 0 { size.max(1.0) } else { size };
            self.set(bid, level, size);
        }
        self.trim();
        self.end()
    }

    fn begin(&mut self, ts: DateTime<Utc>) {
        self.delta.ts = ts;
        self.delta.bids.clear();
        self.delta.asks.clear();
    }

    fn end(&mut self) -> (u64, &OrderBook) {
        self.sequence += 1;
        (self.sequence, &self.delta)
    }

    /// The prices of one side that satisfy `filter`.
    fn levels(&mut self, bid: bool, filter: impl Fn(i64) -> bool) -> Vec<i64> {
        self.side(bid)
            .keys()
            .copied()
            .filter(|&level| filter(level))
            .collect()
    }

    fn side(&mut self, bid: bool) -> &mut BTreeMap<i64, f64> {
        if bid { &mut self.bids } else { &mut self.asks }
    }

    /// Sets the size at `price` ticks and records it in the delta; 0 removes the level.
    fn set(&mut self, bid: bool, price: i64, size: f64) {
        if size > 0.0 {
            self.side(bid).insert(price, size);
        } else {
            self.side(bid).remove(&price);
        }
        let level = self.level(price, size);
        if bid {
            self.delta.bids.push(level);
        } else {
            self.delta.asks.push(level);
        }
    }

    /// Removes the levels furthest from the touch beyond the quoted depth.
    fn trim(&mut self) {
        while self.bids.len() as i64 > DEPTH {
            let (&price, _) = self.bids.first_key_value().unwrap();
            self.set(true, price, 0.0);
        }
        while self.asks.len() as i64 > DEPTH {
            let (&price, _) = self.asks.last_key_value().unwrap();
            self.set(false, price, 0.0);
        }
    }

    /// A resting size `distance` ticks from the touch: larger further away, with noise.
    fn resting(&mut self, distance: i64) -> f64 {
        let base = 20.0 + 4.0 * (distance.min(20) as f64);
        (base * (0.5 + (self.random() % 1000) as f64 / 1000.0)).round()
    }

    fn level(&self, price: i64, size: f64) -> OrderBookLevel {
        OrderBookLevel {
            price: self.price(price),
            size,
            order_cnt: None,
        }
    }

    fn price(&self, ticks: i64) -> f64 {
        ticks as f64 * self.tick_size
    }

    fn random(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }
}
