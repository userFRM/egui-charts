//! The trades of one series, kept for order flow.
//!
//! A page adds trades up to the oldest held trade and leaves later ones to the live stream. A trade
//! whose id is already held is dropped, whatever its time, so history and live trades join exactly
//! when both carry ids. Otherwise time decides: a page adds a trade at the instant of the oldest
//! held trade only if it and every held trade at that instant carry ids, and a live trade is
//! dropped if a page added a trade at or after it and the two do not both carry ids. Trades
//! sharing the boundary instant may be lost but are never counted twice.

use crate::model::Tick;
use chrono::{DateTime, Utc};
use std::collections::{HashSet, VecDeque};

/// Trades in arrival order, at most `capacity` of them: the oldest go first.
pub(crate) struct Trades {
    trades: VecDeque<Tick>,
    capacity: usize,
    /// Ids of the held trades.
    ids: HashSet<u64>,
    /// Time of the newest trade a page added: live trades without an id at or before it repeat it.
    paged_until: Option<DateTime<Utc>>,
    /// Time of the newest trade without an id a page added: any live trade at or before it may
    /// repeat it.
    paged_without_id_until: Option<DateTime<Utc>>,
}

impl Trades {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            trades: VecDeque::new(),
            capacity,
            ids: HashSet::new(),
            paged_until: None,
            paged_without_id_until: None,
        }
    }

    /// An empty store of the same capacity, for the next drain.
    pub(crate) fn empty(&self) -> Self {
        Self::new(self.capacity)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Tick> {
        self.trades.iter()
    }

    /// Appends a trade validated by the caller unless its id is held, evicting the oldest beyond
    /// capacity.
    pub(crate) fn push(&mut self, trade: Tick) {
        if trade.id.is_some_and(|id| !self.ids.insert(id)) {
            return;
        }
        self.trades.push_back(trade);
        if self.trades.len() > self.capacity
            && let Some(id) = self.trades.pop_front().and_then(|evicted| evicted.id)
        {
            self.ids.remove(&id);
        }
    }

    /// Appends `newer`, the trades pushed since the last drain, without those a page already added.
    pub(crate) fn append(&mut self, newer: Trades) {
        for trade in newer.trades {
            let paged_until = match trade.id {
                Some(_) => self.paged_without_id_until,
                None => self.paged_until,
            };
            if paged_until.is_none_or(|until| trade.ts > until) {
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
        if let Some(oldest) = self.trades.iter().map(|trade| trade.ts).min() {
            let ids_at_oldest = (self.trades.iter())
                .filter(|held| held.ts == oldest)
                .all(|held| held.id.is_some());
            page.retain(|trade| {
                trade.ts < oldest || (trade.ts == oldest && trade.id.is_some() && ids_at_oldest)
            });
        }
        let room = self.capacity.saturating_sub(self.trades.len());
        let added = (page.into_iter().rev())
            .filter(|trade| trade.id.is_none_or(|id| self.ids.insert(id)))
            .take(room);
        for trade in added {
            self.paged_until = self.paged_until.max(Some(trade.ts));
            if trade.id.is_none() {
                self.paged_without_id_until = self.paged_without_id_until.max(Some(trade.ts));
            }
            self.trades.push_front(trade);
        }
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
        let later = |mut trade: Tick, micros| {
            trade.ts += TimeDelta::microseconds(micros);
            trade
        };
        #[rustfmt::skip]
        let cases = [
            ("the oldest trades make room", 3,
                vec![Live(vec![id(trade(10, 10.0), 10), trade(11, 11.0), trade(12, 12.0)]),
                    Live(vec![trade(13, 13.0)])],
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
            ("with ids, a replayed live trade is dropped wherever the held one sits", 10,
                vec![Page(vec![id(trade(8, 8.0), 8), id(trade(9, 9.0), 9)]),
                    Live(vec![id(trade(7, 7.0), 7), id(trade(8, 8.0), 8), id(trade(9, 9.0), 9)])],
                vec![8.0, 9.0, 7.0], 0),
            ("with ids, a live trade stamped later than the page's copy is dropped", 10,
                vec![Page(vec![id(trade(5, 5.0), 5)]),
                    Live(vec![id(later(trade(5, 5.0), 400), 5)])],
                vec![5.0], 0),
            ("with ids, a page trade stamped earlier than the live copy is dropped", 10,
                vec![Live(vec![id(later(trade(5, 5.0), 400), 5)]),
                    Page(vec![id(trade(4, 4.0), 4), id(trade(5, 5.0), 5)])],
                vec![4.0, 5.0], 0),
            ("a page without ids drops live trades with ids at or before its newest trade", 10,
                vec![Page(vec![trade(1, 1.0), trade(2, 2.0)]),
                    Live(vec![id(trade(2, 2.0), 2), id(trade(3, 3.0), 3)])],
                vec![1.0, 2.0, 3.0], 0),
            ("a page with ids drops trades at the instant of a held trade without one", 10,
                vec![Live(vec![trade(5, 5.0)]),
                    Page(vec![id(trade(4, 4.0), 4), id(trade(5, 5.0), 5)])],
                vec![4.0, 5.0], 0),
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
            let ids = store.iter().filter(|trade| trade.id.is_some()).count();
            assert_eq!(store.ids.len(), ids, "{name}: held ids");
        }
    }
}
