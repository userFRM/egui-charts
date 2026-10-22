//! The trades of one series, kept for order flow.
//!
//! A page adds trades up to the oldest held trade and leaves later ones to the live stream. History
//! and live trades join exactly when both carry ids: a trade whose id is already held is dropped.
//! Without ids, a page only adds trades older than every held trade, and live trades at or before
//! the newest trade a page added are dropped, so trades sharing the boundary instant may be lost
//! but are never counted twice.

use crate::model::Tick;
use chrono::{DateTime, Utc};
use std::collections::{HashSet, VecDeque};

/// Trades in arrival order, at most `capacity` of them: the oldest go first.
pub(crate) struct Trades {
    trades: VecDeque<Tick>,
    capacity: usize,
    /// Time of the newest trade a page added: live trades without an id at or before it repeat it.
    paged_until: Option<DateTime<Utc>>,
}

impl Trades {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            trades: VecDeque::new(),
            capacity,
            paged_until: None,
        }
    }

    /// An empty store of the same capacity, for the next drain.
    pub(crate) fn empty(&self) -> Self {
        Self::new(self.capacity)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Tick> {
        self.trades.iter()
    }

    /// Appends a trade validated by the caller, evicting the oldest beyond capacity.
    pub(crate) fn push(&mut self, trade: Tick) {
        self.trades.push_back(trade);
        if self.trades.len() > self.capacity {
            self.trades.pop_front();
        }
    }

    /// Appends `newer`, the trades pushed since the last drain, without those a page already added.
    pub(crate) fn append(&mut self, newer: Trades) {
        for trade in newer.trades {
            let repeated = match trade.id {
                // A repeat has the same time, so only held trades at or after it can match.
                Some(id) => (self.trades.iter().rev())
                    .take_while(|held| held.ts >= trade.ts)
                    .any(|held| held.id == Some(id)),
                None => self.paged_until.is_some_and(|until| trade.ts <= until),
            };
            if !repeated {
                self.push(trade);
            }
        }
    }

    /// Puts the trades of a history page, oldest first, in front of the held ones, as many as fit,
    /// so a page never evicts newer trades. Returns the number of invalid trades dropped.
    pub(crate) fn prepend(&mut self, mut page: Vec<Tick>) -> u64 {
        let received = page.len();
        page.retain(valid);
        let rejected = (received - page.len()) as u64;
        // Stable, so trades sharing a time keep the feed's order.
        page.sort_by_key(|trade| trade.ts);
        let newest = page.last().map(|trade| trade.ts);
        let oldest = self.trades.iter().map(|trade| trade.ts).min();
        let held: HashSet<u64> = (self.trades.iter())
            .filter(|held| Some(held.ts) <= newest)
            .filter_map(|held| held.id)
            .collect();
        page.retain(|trade| match trade.id {
            Some(id) => !held.contains(&id) && oldest.is_none_or(|oldest| trade.ts <= oldest),
            None => oldest.is_none_or(|oldest| trade.ts < oldest),
        });
        let room = self.capacity.saturating_sub(self.trades.len());
        let mut added = page.into_iter().rev().take(room).peekable();
        if let Some(newest) = added.peek() {
            self.paged_until = self.paged_until.max(Some(newest.ts));
        }
        added.for_each(|trade| self.trades.push_front(trade));
        rejected
    }
}

/// A finite price and a finite, non-negative size.
pub(super) fn valid(trade: &Tick) -> bool {
    trade.price.is_finite() && trade.size.is_finite() && trade.size >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    /// A trade `second` seconds after 10:00, told apart by its price.
    fn trade(second: i64, price: f64) -> Tick {
        let ts = "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        Tick::new(ts + TimeDelta::seconds(second), price, 1.0, None)
    }

    enum Step {
        /// Trades pushed between two drains.
        Live(Vec<Tick>),
        Page(Vec<Tick>),
    }
    use Step::{Live, Page};

    /// Guards the memory bound and the join of history and live trades: a store that never evicts
    /// grows with the trade rate, a page that evicts newer trades erases the live tape, a trade
    /// counted twice or lost at the join skews every footprint and cumulative delta after a
    /// reload, sorting a page must keep trades sharing a time in their order, and an invalid page
    /// trade poisons every sum.
    #[test]
    fn the_store_is_bounded_and_joins_pages_without_repeats() {
        let id = |trade: Tick, id| trade.with_id(id);
        #[rustfmt::skip]
        let cases = [
            ("the oldest trades make room", 3,
                vec![Live(vec![trade(10, 10.0), trade(11, 11.0), trade(12, 12.0),
                    trade(13, 13.0)])],
                vec![11.0, 12.0, 13.0], 0),
            ("a page fills free room in front, without invalid or repeated trades", 4,
                vec![Live(vec![trade(10, 10.0), trade(11, 11.0), trade(12, 12.0)]),
                    Page(vec![trade(9, 9.0), trade(8, f64::NAN), trade(11, 99.0), trade(7, 7.0)])],
                vec![9.0, 10.0, 11.0, 12.0], 1),
            ("a full store takes nothing from a page", 2,
                vec![Live(vec![trade(10, 10.0), trade(11, 11.0)]), Page(vec![trade(1, 1.0)])],
                vec![10.0, 11.0], 0),
            ("with ids, a page keeps a trade at the instant of the oldest live trade", 10,
                vec![Live(vec![id(trade(5, 3.0), 3), id(trade(5, 4.0), 4)]),
                    Page(vec![id(trade(4, 1.0), 1), id(trade(5, 2.0), 2), id(trade(5, 3.0), 3),
                        id(trade(5, 4.0), 4)])],
                vec![1.0, 2.0, 3.0, 4.0], 0),
            ("without ids, a page drops trades at the instant of the oldest live trade", 10,
                vec![Live(vec![trade(5, 3.0), trade(5, 4.0)]),
                    Page(vec![trade(4, 1.0), trade(5, 2.0), trade(5, 3.0), trade(5, 4.0)])],
                vec![1.0, 3.0, 4.0], 0),
            ("with ids, live trades a page already added are dropped", 10,
                vec![Page(vec![id(trade(1, 1.0), 1), id(trade(2, 2.0), 2)]),
                    Live(vec![id(trade(2, 2.0), 2), id(trade(2, 2.5), 5), id(trade(3, 3.0), 3)])],
                vec![1.0, 2.0, 2.5, 3.0], 0),
            ("without ids, live trades at or before the newest page trade are dropped", 10,
                vec![Page(vec![trade(1, 1.0), trade(2, 2.0)]),
                    Live(vec![trade(2, 2.0), trade(3, 3.0)])],
                vec![1.0, 2.0, 3.0], 0),
            ("with ids, page trades after the oldest live trade are left to live", 10,
                vec![Live(vec![id(trade(5, 3.0), 3)]),
                    Page(vec![id(trade(4, 1.0), 1), id(trade(5, 3.0), 3), id(trade(6, 4.0), 4),
                        id(trade(7, 5.0), 5)]),
                    Live(vec![id(trade(6, 4.0), 4), id(trade(7, 5.0), 5)])],
                vec![1.0, 3.0, 4.0, 5.0], 0),
            ("a page is sorted by time, keeping the order of trades sharing a time", 10,
                vec![Page(vec![trade(5, 4.0), trade(5, 3.0), trade(4, 2.0)])],
                vec![2.0, 4.0, 3.0], 0),
        ];
        for (name, capacity, steps, prices, rejected) in cases {
            let mut store = Trades::new(capacity);
            let mut dropped = 0;
            for step in steps {
                match step {
                    Live(trades) => {
                        let mut pending = store.empty();
                        trades.into_iter().for_each(|trade| pending.push(trade));
                        store.append(pending);
                    }
                    Page(page) => dropped += store.prepend(page),
                }
            }
            let held = Vec::from_iter(store.iter().map(|trade| trade.price));
            assert_eq!((held, dropped), (prices, rejected), "{name}");
        }
    }
}
