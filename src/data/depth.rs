//! The order book of one subscription, kept in step with the feed's sequence numbers.

use crate::model::{OrderBook, OrderBookLevel};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Levels per side, and levels of the deltas held while the book waits for a snapshot. A deeper
/// snapshot is rejected, a delta that makes a side deeper loses the book, and the oldest held
/// deltas make room beyond it, so a feed cannot grow either count without limit.
pub(crate) const MAX_LEVELS: usize = 10_000;

/// One side of the book, ascending by key with the best level last, so that the frequent changes
/// near the touch shift few levels and reuse the side's capacity instead of allocating. Bids are
/// keyed by [`key`] of their price and asks by its complement, which orders them the other
/// way.
type Side = Vec<(u64, OrderBookLevel)>;

/// A book built from a snapshot and the deltas that follow it. Without a snapshot, or after a
/// gap, it is empty until the feed pushes a new snapshot, and holds the deltas pushed meanwhile
/// for that snapshot to apply.
#[derive(Default)]
pub(crate) struct Book {
    /// The instrument's tick size, positive and finite: prices are keyed by the nearest integer
    /// tick, so prices a feed computed with rounding errors land on one level. `None` keys them
    /// exactly.
    tick: Option<f64>,
    /// Sequence of the last snapshot or delta applied. `None` while the book has no snapshot.
    sequence: Option<u64>,
    /// A snapshot was asked for since the book lost its snapshot.
    requested: bool,
    /// Changed since the last `take_changed`.
    changed: bool,
    ts: DateTime<Utc>,
    symbol: String,
    bids: Side,
    asks: Side,
    /// Deltas pushed while the book has no snapshot: one run of consecutive sequence numbers.
    held: VecDeque<(u64, OrderBook)>,
    /// Levels of the held deltas, an empty delta counting as one so a stream of them is bounded.
    held_levels: usize,
}

/// The book was cleared and waits for a snapshot.
#[derive(Debug, PartialEq)]
pub(crate) struct Lost {
    /// The update failed validation.
    pub(crate) invalid: bool,
    /// Ask the feed for a snapshot: the first loss since the last snapshot applied, so a feed that
    /// keeps sending unusable snapshots is not asked again and again.
    pub(crate) request: bool,
}

impl Book {
    /// An empty book of an instrument of tick size `tick`, if it is positive and finite.
    pub(crate) fn new(tick: Option<f64>) -> Self {
        let tick = tick.filter(|tick| tick.is_finite() && *tick > 0.0);
        Self {
            tick,
            ..Self::default()
        }
    }

    /// Replaces the book with `book`, numbered `sequence`, and applies the held deltas numbered
    /// above it, so a snapshot fetched while deltas kept arriving heals the book.
    pub(crate) fn snapshot(&mut self, sequence: u64, book: OrderBook) -> Result<(), Lost> {
        if !valid(&book) || book.bids.len().max(book.asks.len()) > MAX_LEVELS {
            return Err(self.lose(true));
        }
        for (side, levels, order) in [
            (&mut self.bids, book.bids, 0),
            (&mut self.asks, book.asks, u64::MAX),
        ] {
            side.clear();
            // Reversed so that, after the stable sort, the first of equal keys is the last pushed.
            let levels = levels.into_iter().rev().filter(|level| level.size > 0.0);
            side.extend(levels.map(|level| (key(self.tick, level.price) ^ order, level)));
            side.sort_by_key(|(key, _)| *key);
            side.dedup_by_key(|(key, _)| *key);
        }
        let held = std::mem::take(&mut self.held);
        (self.sequence, self.requested, self.changed) = (Some(sequence), false, true);
        (self.ts, self.symbol, self.held_levels) = (book.ts, book.symbol, 0);
        let mut applied = Ok(());
        // Every held delta goes through `delta`, so after a gap among them the rest is held again.
        for (sequence, changes) in held {
            applied = applied.and(self.delta(sequence, &changes));
        }
        applied
    }

    /// Applies `changes`, the levels that changed with their new size, if `sequence` follows the
    /// last one applied. A replayed delta is ignored; a gap or a delta before any snapshot loses
    /// the book, and the delta is held for the next snapshot.
    pub(crate) fn delta(&mut self, sequence: u64, changes: &OrderBook) -> Result<(), Lost> {
        // A replay is never applied, so its contents cannot lose the book.
        if self.sequence.is_some_and(|last| sequence <= last) {
            return Ok(());
        }
        if !valid(changes) {
            return Err(self.lose(true));
        }
        match self.sequence {
            Some(last) if last.checked_add(1) == Some(sequence) => {}
            _ => {
                let lost = self.lose(false);
                self.hold(sequence, changes);
                return Err(lost);
            }
        }
        for (side, levels, order) in [
            (&mut self.bids, &changes.bids, 0),
            (&mut self.asks, &changes.asks, u64::MAX),
        ] {
            for level in levels {
                let key = key(self.tick, level.price) ^ order;
                match side.binary_search_by_key(&key, |(key, _)| *key) {
                    Ok(at) if level.size > 0.0 => side[at].1 = level.clone(),
                    Ok(at) => _ = side.remove(at),
                    Err(at) if level.size > 0.0 => side.insert(at, (key, level.clone())),
                    Err(_) => {}
                }
            }
        }
        if self.bids.len().max(self.asks.len()) > MAX_LEVELS {
            return Err(self.lose(true));
        }
        (self.sequence, self.ts, self.changed) = (Some(sequence), changes.ts, true);
        Ok(())
    }

    /// The book, bids best first and asks best first, if it changed since the last call.
    pub(crate) fn take_changed(&mut self) -> Option<OrderBook> {
        let levels = |side: &Side| side.iter().rev().map(|(_, level)| level.clone()).collect();
        std::mem::take(&mut self.changed).then(|| OrderBook {
            ts: self.ts,
            symbol: self.symbol.clone(),
            bids: levels(&self.bids),
            asks: levels(&self.asks),
        })
    }

    /// Synchronized levels, best first on each side, without copying the book.
    pub(crate) fn levels(&self) -> Option<impl Iterator<Item = (bool, &OrderBookLevel)>> {
        self.sequence?;
        Some(
            self.bids
                .iter()
                .rev()
                .map(|(_, level)| (true, level))
                .chain(self.asks.iter().rev().map(|(_, level)| (false, level))),
        )
    }

    /// The best `rows` price rows of each side, bids then asks, each best first, as the row of
    /// `ticks_per_row` integer ticks the levels fall in, rounding down, and the sum of their
    /// sizes. `None` for a book without a tick size.
    pub(crate) fn rows(&self, ticks_per_row: u32, rows: usize) -> Option<[Vec<(i64, f64)>; 2]> {
        self.tick?;
        let group = |side: &Side, order: u64| {
            let mut grouped: Vec<(i64, f64)> = Vec::with_capacity(rows.min(side.len()));
            for (key, level) in side.iter().rev() {
                let row = ((key ^ order ^ 1 << 63) as i64).div_euclid(i64::from(ticks_per_row));
                let full = grouped.len() == rows;
                match grouped.last_mut() {
                    Some((last, size)) if *last == row => {
                        *size = (*size + level.size).min(f64::MAX)
                    }
                    _ if full => break,
                    _ => grouped.push((row, level.size)),
                }
            }
            grouped
        };
        Some([group(&self.bids, 0), group(&self.asks, u64::MAX)])
    }

    /// Holds `changes` for the next snapshot. A delta that does not extend the held run starts a
    /// new one, since a run with a hole cannot be applied past it; a replayed one is ignored.
    fn hold(&mut self, sequence: u64, changes: &OrderBook) {
        if let (Some(&(first, _)), Some(&(last, _))) = (self.held.front(), self.held.back()) {
            if (first..=last).contains(&sequence) {
                return;
            }
            if last.checked_add(1) != Some(sequence) {
                self.held.clear();
                self.held_levels = 0;
            }
        }
        let levels = |changes: &OrderBook| (changes.bids.len() + changes.asks.len()).max(1);
        self.held_levels += levels(changes);
        self.held.push_back((sequence, changes.clone()));
        while self.held_levels > MAX_LEVELS
            && let Some((_, oldest)) = self.held.pop_front()
        {
            self.held_levels -= levels(&oldest);
        }
    }

    fn lose(&mut self, invalid: bool) -> Lost {
        if self.sequence.take().is_some() {
            self.bids.clear();
            self.asks.clear();
            self.changed = true;
        }
        let request = !std::mem::replace(&mut self.requested, true);
        Lost { invalid, request }
    }
}

/// Finite prices, finite and non-negative sizes.
fn valid(book: &OrderBook) -> bool {
    (book.bids.iter().chain(&book.asks))
        .all(|level| level.price.is_finite() && level.size.is_finite() && level.size >= 0.0)
}

/// Orders finite prices: by their nearest integer multiple of `tick`, as an `i64` with its sign
/// bit flipped, or like `f64::total_cmp` without a tick, with -0.0 and 0.0 as one price.
fn key(tick: Option<f64>, price: f64) -> u64 {
    if let Some(tick) = tick {
        return (price / tick).round() as i64 as u64 ^ 1 << 63;
    }
    let bits = (price + 0.0).to_bits();
    if bits >> 63 == 0 {
        bits | 1 << 63
    } else {
        !bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::{once, repeat_with};

    fn book(bids: &[(f64, f64)], asks: &[(f64, f64)]) -> OrderBook {
        let levels = |levels: &[(f64, f64)]| {
            let level = |&(price, size)| OrderBookLevel {
                price,
                size,
                order_cnt: None,
            };
            levels.iter().map(level).collect()
        };
        OrderBook {
            ts: DateTime::default(),
            symbol: "ES".into(),
            bids: levels(bids),
            asks: levels(asks),
        }
    }

    enum Push {
        Snapshot(u64, OrderBook),
        Delta(u64, OrderBook),
    }
    use Push::{Delta, Snapshot};

    /// Guards the depth sequence: a missed delta must clear the book and ask for one snapshot (a
    /// book that keeps going after a gap shows sizes that no longer exist), a size of 0 must remove
    /// its level (or the book grows with every price ever quoted), a replayed delta must not apply
    /// twice, hostile values and books deeper than the limit must not enter the book, a feed must
    /// be asked once per loss, not on every unusable snapshot, and deltas pushed while the book
    /// waits must apply after a snapshot older than them (or a snapshot fetched while the stream
    /// runs never heals the book) without growing without limit.
    #[test]
    fn deltas_follow_their_snapshot_and_a_gap_asks_for_a_new_one() {
        let lost = |invalid, request| Err(Lost { invalid, request });
        let initial = || Snapshot(10, book(&[(99.0, 1.0), (98.0, 2.0)], &[(100.0, 3.0)]));
        let applied = book(&[(99.0, 5.0), (-1.0, 1.0)], &[(100.0, 3.0), (101.0, 4.0)]);
        let deep = |levels: usize| book(&Vec::from_iter((0..levels).map(|i| (i as f64, 1.0))), &[]);
        #[rustfmt::skip]
        let cases = [
            ("deltas apply in sequence; size 0 removes; a replay is ignored",
                vec![initial(),
                    Delta(11, book(&[(99.0, 5.0), (98.0, 0.0), (-1.0, 1.0)], &[])),
                    Delta(12, book(&[], &[(101.0, 4.0), (102.0, 0.0)])),
                    Delta(11, book(&[(99.0, 7.0)], &[]))],
                vec![Ok(()), Ok(()), Ok(()), Ok(())], Some(applied)),
            ("a gap clears the book and asks once",
                vec![initial(), Delta(12, book(&[(99.0, 5.0)], &[])),
                    Delta(13, book(&[(99.0, 6.0)], &[]))],
                vec![Ok(()), lost(false, true), lost(false, false)], Some(book(&[], &[]))),
            ("a delta before any snapshot asks once",
                vec![Delta(1, book(&[(99.0, 5.0)], &[])), Delta(2, book(&[(99.0, 6.0)], &[]))],
                vec![lost(false, true), lost(false, false)], None),
            ("a snapshot answers the request and deltas resume",
                vec![Delta(1, book(&[], &[])), initial(), Delta(11, book(&[], &[(100.0, 0.0)]))],
                vec![lost(false, true), Ok(()), Ok(())],
                Some(book(&[(99.0, 1.0), (98.0, 2.0)], &[]))),
            ("an invalid delta is rejected and loses the book",
                vec![initial(), Delta(11, book(&[(f64::NAN, 1.0)], &[]))],
                vec![Ok(()), lost(true, true)], Some(book(&[], &[]))),
            ("a replayed invalid delta is ignored",
                vec![initial(), Delta(11, book(&[(99.0, 5.0)], &[])),
                    Delta(11, book(&[(f64::NAN, 1.0)], &[]))],
                vec![Ok(()), Ok(()), Ok(())],
                Some(book(&[(99.0, 5.0), (98.0, 2.0)], &[(100.0, 3.0)]))),
            ("an invalid snapshot is rejected and asks once per loss",
                vec![Snapshot(10, book(&[], &[(100.0, -1.0)])), Delta(11, book(&[], &[])),
                    Snapshot(12, book(&[], &[(100.0, -1.0)]))],
                vec![lost(true, true), lost(false, false), lost(true, false)], None),
            ("a snapshot deeper than the limit is rejected",
                vec![Snapshot(10, deep(MAX_LEVELS + 1))],
                vec![lost(true, true)], None),
            ("a delta that makes a side deeper than the limit loses the book",
                vec![Snapshot(10, deep(MAX_LEVELS)), Delta(11, book(&[(-1.0, 1.0)], &[]))],
                vec![Ok(()), lost(true, true)], Some(book(&[], &[]))),
            ("deltas pushed while waiting apply after an older snapshot",
                vec![initial(), Delta(12, book(&[(99.0, 5.0)], &[])),
                    Delta(13, book(&[(97.0, 9.0)], &[])), Delta(14, book(&[(98.0, 2.0)], &[])),
                    Snapshot(12, book(&[(99.0, 1.0)], &[(100.0, 3.0)])),
                    Delta(15, book(&[], &[(101.0, 4.0)]))],
                vec![Ok(()), lost(false, true), lost(false, false), lost(false, false), Ok(()),
                    Ok(())],
                Some(book(&[(99.0, 1.0), (98.0, 2.0), (97.0, 9.0)],
                    &[(100.0, 3.0), (101.0, 4.0)]))),
            ("held deltas are bounded by their levels, the oldest dropped first",
                (1..=MAX_LEVELS as u64 / 2 + 1)
                    .map(|sequence| Delta(sequence, book(&[(99.0, 1.0)], &[(100.0, 1.0)])))
                    .chain([Snapshot(0, book(&[], &[]))]).collect(),
                once(lost(false, true))
                    .chain(repeat_with(|| lost(false, false)).take(MAX_LEVELS / 2))
                    .chain([lost(false, true)]).collect(),
                Some(book(&[], &[]))),
        ];
        for (name, pushes, outcomes, expected) in cases {
            let mut depth = Book::default();
            let got: Vec<_> = pushes
                .into_iter()
                .map(|push| match push {
                    Snapshot(sequence, book) => depth.snapshot(sequence, book),
                    Delta(sequence, changes) => depth.delta(sequence, &changes),
                })
                .collect();
            assert_eq!(got, outcomes, "{name}");
            let prices = |book: OrderBook| -> Vec<_> {
                let side = |levels: Vec<OrderBookLevel>| {
                    Vec::from_iter(levels.into_iter().map(|level| (level.price, level.size)))
                };
                vec![side(book.bids), side(book.asks)]
            };
            assert_eq!(
                depth.take_changed().map(prices),
                expected.map(prices),
                "{name}"
            );
        }
    }
}
