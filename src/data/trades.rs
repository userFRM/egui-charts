//! The trades of one series, kept for order flow.
//!
//! A page adds trades up to the oldest held trade and leaves later ones to the live stream. A trade
//! whose id is already held is dropped, whatever its time, so history and live trades join exactly
//! when both carry ids. Otherwise time decides: a page adds a trade at the instant of the oldest
//! held trade only if it and every held trade at that instant carry ids, and a live trade is
//! dropped if a page added a trade at or after it and the two do not both carry ids. Trades
//! sharing the boundary instant may be lost but are never counted twice.

use crate::model::{OrderSide, Tick, next_revision};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet, VecDeque};

/// Size taken from one resting order, or from an unidentified one, at one price.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConsumedOrder {
    /// Exchange id of the resting order; fills without one are listed apart, as their orders
    /// cannot be told from each other.
    pub resting_order_id: Option<u64>,
    /// Price of the fills.
    pub price: f64,
    /// Total retained size filled, saturating at the largest finite value on overflow.
    pub size: f64,
}

/// Fills of one explicitly identified aggressive order, or one fill without an order id.
/// Only retained fills count: eviction from the raw store also removes them from this view.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CombinedTrade {
    /// Explicit aggressor order id; unknown fills are never grouped together.
    pub aggressor_order_id: Option<u64>,
    /// Time of the first retained fill.
    pub first_time: DateTime<Utc>,
    /// Time of the last retained fill.
    pub last_time: DateTime<Utc>,
    /// Price of the first retained fill.
    pub first_price: f64,
    /// Price of the last retained fill.
    pub last_price: f64,
    /// Volume-weighted price; the first price for zero total size.
    pub price: f64,
    /// Total retained size, saturating at the largest finite value on overflow.
    pub size: f64,
    /// Number of retained fills.
    pub fills: usize,
    /// Aggressor side, unknown if fills disagree.
    pub side: Option<OrderSide>,
    /// The resting orders the retained fills consumed, in fill order; consecutive fills of
    /// one identified order at one price are one entry. Their sizes add up to
    /// [`size`](Self::size).
    pub consumed: Vec<ConsumedOrder>,
    /// Whether the order is fully reconstructed: its aggressor id is explicit and every
    /// retained fill names its resting order. Otherwise `consumed` lacks evidence.
    pub complete: bool,
}

/// Combines explicit aggressor ids, in order of their first fill. Unknown ids stay separate.
/// Use raw fills for per-price volume. This allocates at most one entry per input fill and
/// ignores invalid fills. A bounded input gives a bounded combined view.
pub fn combine_trades<'a>(trades: impl IntoIterator<Item = &'a Tick>) -> Vec<CombinedTrade> {
    let mut indices: HashMap<u64, usize> = HashMap::new();
    let mut combined: Vec<CombinedTrade> = Vec::new();
    for fill in trades.into_iter().filter(|t| valid(t)) {
        if let Some(&at) = fill
            .aggressor_order_id
            .as_ref()
            .and_then(|id| indices.get(id))
        {
            let trade = &mut combined[at];
            let scale = trade.size.max(fill.size);
            if scale > 0.0 {
                let weight = (fill.size / scale) / (trade.size / scale + fill.size / scale);
                trade.price += (fill.price - trade.price) * weight;
            }
            trade.size = (trade.size + fill.size).min(f64::MAX);
            trade.last_time = fill.ts;
            trade.last_price = fill.price;
            trade.fills += 1;
            if trade.side != fill.side {
                trade.side = None;
            }
            trade.consume(fill);
        } else {
            if let Some(id) = fill.aggressor_order_id {
                indices.insert(id, combined.len());
            }
            let mut trade = CombinedTrade {
                aggressor_order_id: fill.aggressor_order_id,
                first_time: fill.ts,
                last_time: fill.ts,
                first_price: fill.price,
                last_price: fill.price,
                price: fill.price,
                size: fill.size,
                fills: 1,
                side: fill.side,
                consumed: Vec::new(),
                complete: fill.aggressor_order_id.is_some(),
            };
            trade.consume(fill);
            combined.push(trade);
        }
    }
    combined
}

impl CombinedTrade {
    /// Adds the resting order that `fill` consumed to the breakdown.
    fn consume(&mut self, fill: &Tick) {
        self.complete &= fill.resting_order_id.is_some();
        match self.consumed.last_mut().filter(|c| {
            fill.resting_order_id.is_some()
                && c.resting_order_id == fill.resting_order_id
                && c.price == fill.price
        }) {
            Some(order) => order.size = (order.size + fill.size).min(f64::MAX),
            None => self.consumed.push(ConsumedOrder {
                resting_order_id: fill.resting_order_id,
                price: fill.price,
                size: fill.size,
            }),
        }
    }
}

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
    /// Time of the newest trade dropped to stay within `capacity`: evidence before it is incomplete.
    lost_until: Option<DateTime<Utc>>,
    /// Changes whenever a trade is added or evicted.
    revision: u64,
}

impl Trades {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            trades: VecDeque::new(),
            capacity,
            ids: HashSet::new(),
            paged_until: None,
            paged_without_id_until: None,
            lost_until: None,
            revision: next_revision(),
        }
    }

    /// Identifies the held trades: it differs after any trade was added or evicted.
    #[cfg_attr(
        not(feature = "widget"),
        expect(dead_code, reason = "only the chart widget consumes a feed")
    )]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Time of the newest trade dropped to stay within capacity, evicted or left out of a page.
    #[cfg_attr(
        not(feature = "widget"),
        expect(dead_code, reason = "only the chart widget consumes a feed")
    )]
    pub(crate) fn lost_until(&self) -> Option<DateTime<Utc>> {
        self.lost_until
    }

    /// An empty store of the same capacity, for the next drain.
    pub(crate) fn empty(&self) -> Self {
        Self::new(self.capacity)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Tick> {
        self.trades.iter()
    }

    /// Appends a trade validated by the caller unless its id is held, evicting the oldest beyond
    /// capacity. Repeated ids can complete missing order ids without counting the fill again.
    pub(crate) fn push(&mut self, trade: Tick) {
        self.insert(trade, &mut |_, _| ());
    }

    /// [`push`](Self::push), calling `changed` with the trade if it is added (`true`) and with
    /// the trade it evicts (`false`).
    fn insert(&mut self, trade: Tick, changed: &mut impl FnMut(&Tick, bool)) {
        if trade.id.is_some_and(|id| !self.ids.insert(id)) {
            self.annotate(&trade);
            return;
        }
        self.revision = next_revision();
        changed(&trade, true);
        self.trades.push_back(trade);
        if self.trades.len() > self.capacity
            && let Some(evicted) = self.trades.pop_front()
        {
            if let Some(id) = evicted.id {
                self.ids.remove(&id);
            }
            self.lost_until = self.lost_until.max(Some(evicted.ts));
            changed(&evicted, false);
        }
    }

    /// A tape print may precede the order execution or recorded fill that identifies its orders.
    fn annotate(&mut self, trade: &Tick) {
        if (trade.aggressor_order_id.is_some() || trade.resting_order_id.is_some())
            && let Some(id) = trade.id.filter(|id| self.ids.contains(id))
            && let Some(held) = self.trades.iter_mut().rev().find(|t| t.id == Some(id))
        {
            held.aggressor_order_id = held.aggressor_order_id.or(trade.aggressor_order_id);
            held.resting_order_id = held.resting_order_id.or(trade.resting_order_id);
        }
    }

    /// Appends `newer`, the trades pushed since the last drain, without those a page already
    /// added. Calls `changed` with each trade added (`true`) or evicted (`false`).
    pub(crate) fn append(&mut self, newer: Trades, changed: &mut impl FnMut(&Tick, bool)) {
        self.lost_until = self.lost_until.max(newer.lost_until);
        for trade in newer.trades {
            let paged_until = match trade.id {
                Some(_) => self.paged_without_id_until,
                None => self.paged_until,
            };
            if trade.id.is_some_and(|id| self.ids.contains(&id))
                || paged_until.is_none_or(|until| trade.ts > until)
            {
                self.insert(trade, changed);
            }
        }
    }

    /// Puts the trades of a history page, oldest first, in front of the held ones, as many as fit,
    /// so a page never evicts newer trades. Calls `changed` with each trade added. Returns the
    /// number of invalid trades dropped.
    pub(crate) fn prepend(
        &mut self,
        mut page: Vec<Tick>,
        changed: &mut impl FnMut(&Tick, bool),
    ) -> u64 {
        let received = page.len();
        page.retain(valid);
        let rejected = (received - page.len()) as u64;
        for trade in &page {
            self.annotate(trade);
        }
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
        let mut room = self.capacity.saturating_sub(self.trades.len());
        let mut added = Vec::new();
        for trade in page.into_iter().rev() {
            if trade.id.is_some_and(|id| self.ids.contains(&id)) {
                continue;
            }
            if room == 0 {
                // The newest trade left out; the older ones are lost with it.
                self.lost_until = self.lost_until.max(Some(trade.ts));
                break;
            }
            room -= 1;
            if let Some(id) = trade.id {
                self.ids.insert(id);
            }
            added.push(trade);
        }
        for trade in added {
            self.paged_until = self.paged_until.max(Some(trade.ts));
            if trade.id.is_none() {
                self.paged_without_id_until = self.paged_without_id_until.max(Some(trade.ts));
            }
            self.revision = next_revision();
            changed(&trade, true);
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

    /// Group only explicit aggressor ids, across pages and drains, without moving price volume.
    #[test]
    fn combines_only_fills_of_the_same_aggressor() {
        let fill = |second, price, size, order| {
            let mut t = trade(second, price);
            t.size = size;
            t.aggressor_order_id = order;
            t
        };
        #[rustfmt::skip]
        let cases = [
            ("empty input", 10, vec![], vec![]),
            ("zero total size keeps the first price", 10,
                vec![Live(vec![fill(1, 100.0, 0.0, Some(7)), fill(2, 101.0, 0.0, Some(7))])],
                vec![(0.0, 100.0, 101.0, 100.0, 2)]),
            ("invalid fills do not combine", 10,
                vec![Live(vec![fill(1, f64::NAN, 1.0, Some(7)), fill(2, 100.0, f64::NAN, Some(7)),
                    fill(3, 100.0, -1.0, Some(7)), fill(4, 100.0, 1.0, Some(7))])],
                vec![(1.0, 100.0, 100.0, 100.0, 1)]),
            ("three prices", 10, vec![Live(vec![fill(1, 100.0, 2.0, Some(7)),
                fill(2, 101.0, 3.0, Some(7)), fill(3, 102.0, 5.0, Some(7))])],
                vec![(10.0, 100.0, 102.0, 101.3, 3)]),
            ("interleaved ids", 10, vec![Live(vec![fill(1, 100.0, 2.0, Some(7)),
                fill(2, 90.0, 1.0, Some(8)), fill(3, 102.0, 2.0, Some(7))])],
                vec![(4.0, 100.0, 102.0, 101.0, 2), (1.0, 90.0, 90.0, 90.0, 1)]),
            ("unknown ids stay separate", 10, vec![Live(vec![fill(1, 100.0, 2.0, None),
                fill(2, 100.0, 3.0, None)])],
                vec![(2.0, 100.0, 100.0, 100.0, 1), (3.0, 100.0, 100.0, 100.0, 1)]),
            ("history and live fills", 10, vec![Page(vec![fill(1, 100.0, 2.0, Some(7))]),
                Live(vec![fill(2, 102.0, 2.0, Some(7))])],
                vec![(4.0, 100.0, 102.0, 101.0, 2)]),
            ("separate drains", 10, vec![Live(vec![fill(1, 100.0, 2.0, Some(7))]),
                Live(vec![fill(2, 102.0, 2.0, Some(7))])],
                vec![(4.0, 100.0, 102.0, 101.0, 2)]),
            ("a page completes order ids on retained fills", 10,
                vec![Live(vec![fill(1, 100.0, 2.0, None).with_id(1), fill(2, 102.0, 2.0, None).with_id(2)]),
                    Page(vec![fill(1, 100.0, 2.0, Some(7)).with_id(1), fill(2, 102.0, 2.0, Some(7)).with_id(2)])],
                vec![(4.0, 100.0, 102.0, 101.0, 2)]),
            ("only retained fills count", 2, vec![Live(vec![fill(1, 100.0, 2.0, Some(7)),
                fill(2, 101.0, 3.0, Some(7))]), Live(vec![fill(3, 102.0, 1.0, Some(7))])],
                vec![(4.0, 101.0, 102.0, 101.25, 2)]),
        ];
        for (name, capacity, steps, expected) in cases {
            let mut store = Trades::new(capacity);
            for step in steps {
                match step {
                    Live(trades) => {
                        let mut pending = store.empty();
                        trades.into_iter().for_each(|t| pending.push(t));
                        store.append(pending, &mut |_, _| ());
                    }
                    Page(page) => {
                        store.prepend(page, &mut |_, _| ());
                    }
                }
            }
            let combined = combine_trades(store.iter());
            let held: Vec<_> = combined
                .iter()
                .map(|t| (t.size, t.first_price, t.last_price, t.price, t.fills))
                .collect();
            assert_eq!(held.len(), expected.len(), "{name}");
            for (actual, expected) in held.iter().zip(expected) {
                assert_eq!(
                    (actual.0, actual.1, actual.2, actual.4),
                    (expected.0, expected.1, expected.2, expected.4),
                    "{name}"
                );
                assert!((actual.3 - expected.3).abs() < 1e-10, "{name}: VWAP");
            }
        }
        // All fills at one price have exactly that VWAP, including a bar's body edge.
        let fills = [
            fill(1, 99.99, 1.0, Some(7)),
            fill(2, 99.99, 2.0, Some(7)),
            fill(3, 99.99, 3.0, Some(7)),
        ];
        assert_eq!(combine_trades(&fills)[0].price, 99.99);
    }

    /// The breakdown lists the resting orders the retained fills consumed and adds up to the
    /// combined size, also after eviction; it is complete only when every fill names its
    /// resting order.
    #[test]
    fn the_breakdown_sums_to_the_retained_fills_and_reports_missing_evidence() {
        let fill = |second, price, size, resting| {
            let mut t = trade(second, price).with_aggressor_order_id(7);
            t.size = size;
            t.resting_order_id = resting;
            t
        };
        let fills = || {
            vec![
                fill(1, 100.0, 2.0, Some(1)),
                fill(2, 100.0, 1.0, Some(1)),
                fill(3, 101.0, 4.0, Some(2)),
                fill(4, 101.0, 3.0, None),
            ]
        };
        for (capacity, want, complete) in [
            (
                10,
                vec![
                    (Some(1), 100.0, 3.0),
                    (Some(2), 101.0, 4.0),
                    (None, 101.0, 3.0),
                ],
                false,
            ),
            // The oldest two fills are evicted.
            (2, vec![(Some(2), 101.0, 4.0), (None, 101.0, 3.0)], false),
        ] {
            let mut store = Trades::new(capacity);
            fills().into_iter().for_each(|t| store.push(t));
            let combined = combine_trades(store.iter());
            let trade = &combined[0];
            let got: Vec<_> = trade
                .consumed
                .iter()
                .map(|c| (c.resting_order_id, c.price, c.size))
                .collect();
            assert_eq!(
                (got, trade.complete),
                (want, complete),
                "capacity {capacity}"
            );
            assert_eq!(
                trade.consumed.iter().map(|c| c.size).sum::<f64>(),
                trade.size
            );
        }
        let named: Vec<_> = fills().into_iter().take(3).collect();
        assert!(combine_trades(&named)[0].complete);
    }

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
                        store.append(pending, &mut |_, _| ());
                    }
                    Page(page) => dropped += store.prepend(page, &mut |_, _| ()),
                }
            }
            let held = Vec::from_iter(store.iter().map(|trade| trade.price));
            assert_eq!((held, dropped), (prices, rejected), "{name}");
            let ids = store.iter().filter(|trade| trade.id.is_some()).count();
            assert_eq!(store.ids.len(), ids, "{name}: held ids");
        }
    }
}
