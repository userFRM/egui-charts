//! The order book of one subscription, kept in step with the feed's sequence numbers.

use crate::model::{OrderBook, OrderBookLevel};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, VecDeque};

/// Levels per side, and levels of the deltas held while the book waits for a snapshot. A deeper
/// snapshot is rejected, a delta that makes a side deeper loses the book, and the oldest held
/// deltas make room beyond it, so a feed cannot grow either count without limit.
pub(crate) const MAX_LEVELS: usize = 10_000;

/// One side of the book, keyed by [`key`] of the price.
type Side = BTreeMap<u64, OrderBookLevel>;

/// A book built from a snapshot and the deltas that follow it. Without a snapshot, or after a
/// gap, it is empty until the feed pushes a new snapshot, and holds the deltas pushed meanwhile
/// for that snapshot to apply.
#[derive(Default)]
pub(crate) struct Book {
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
    /// Replaces the book with `book`, numbered `sequence`, and applies the held deltas numbered
    /// above it, so a snapshot fetched while deltas kept arriving heals the book.
    pub(crate) fn snapshot(&mut self, sequence: u64, book: OrderBook) -> Result<(), Lost> {
        if !valid(&book) || book.bids.len().max(book.asks.len()) > MAX_LEVELS {
            return Err(self.lose(true));
        }
        let side = |levels: Vec<OrderBookLevel>| {
            let levels = levels.into_iter().filter(|level| level.size > 0.0);
            levels.map(|level| (key(level.price), level)).collect()
        };
        let held = std::mem::take(&mut self.held);
        *self = Self {
            sequence: Some(sequence),
            requested: false,
            changed: true,
            ts: book.ts,
            symbol: book.symbol,
            bids: side(book.bids),
            asks: side(book.asks),
            ..Self::default()
        };
        let mut applied = Ok(());
        // Every held delta goes through `delta`, so after a gap among them the rest is held again.
        for (sequence, changes) in held {
            applied = applied.and(self.delta(sequence, changes));
        }
        applied
    }

    /// Applies `changes`, the levels that changed with their new size, if `sequence` follows the
    /// last one applied. A replayed delta is ignored; a gap or a delta before any snapshot loses
    /// the book, and the delta is held for the next snapshot.
    pub(crate) fn delta(&mut self, sequence: u64, changes: OrderBook) -> Result<(), Lost> {
        if !valid(&changes) {
            return Err(self.lose(true));
        }
        match self.sequence {
            Some(last) if sequence <= last => return Ok(()),
            Some(last) if last.checked_add(1) == Some(sequence) => {}
            _ => {
                let lost = self.lose(false);
                self.hold(sequence, changes);
                return Err(lost);
            }
        }
        for (side, levels) in [
            (&mut self.bids, changes.bids),
            (&mut self.asks, changes.asks),
        ] {
            for level in levels {
                if level.size > 0.0 {
                    side.insert(key(level.price), level);
                } else {
                    side.remove(&key(level.price));
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
        std::mem::take(&mut self.changed).then(|| OrderBook {
            ts: self.ts,
            symbol: self.symbol.clone(),
            bids: self.bids.values().rev().cloned().collect(),
            asks: self.asks.values().cloned().collect(),
        })
    }

    /// Holds `changes` for the next snapshot. A delta that does not extend the held run starts a
    /// new one, since a run with a hole cannot be applied past it; a replayed one is ignored.
    fn hold(&mut self, sequence: u64, changes: OrderBook) {
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
        self.held_levels += levels(&changes);
        self.held.push_back((sequence, changes));
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

/// Orders finite prices like `f64::total_cmp`, with -0.0 and 0.0 as one price.
fn key(price: f64) -> u64 {
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
                    Delta(sequence, changes) => depth.delta(sequence, changes),
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
