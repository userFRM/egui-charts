//! Borrowed rows of an immutable prepared page.

use super::footprint::{meets_threshold, side_delta};
use super::{FootprintBar, FootprintLevel};
use chrono::{DateTime, Utc};
use std::{collections::BTreeMap, fmt, sync::Arc};

/// Immutable, chronological bars owned by a feed.
///
/// Reads must take constant time and must not perform I/O or allocate. Bars and rows must stay
/// unchanged for the lifetime of the owner, including across clones of its `Arc`. The chart
/// validates them when accepting the page. An out-of-range `index` may panic, as indexing a slice does.
pub trait FootprintSource: Send + Sync {
    /// Number of bars, constant for this owner.
    fn len(&self) -> usize;
    /// Whether the page has no bars.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// The bar at `index`, with rows borrowed from this owner.
    fn bar(&self, index: usize) -> FootprintBarView<'_>;
}

impl FootprintSource for Vec<FootprintBar> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn bar(&self, index: usize) -> FootprintBarView<'_> {
        (&self[index]).into()
    }
}

/// Price rows in strictly ascending order. Slice rows use their own `level` as the key.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum FootprintRows<'a> {
    /// Rows owned by a mutable footprint.
    Tree(&'a BTreeMap<i64, FootprintLevel>),
    /// Rows borrowed from immutable storage owned by the caller.
    Slice(&'a [FootprintLevel]),
}

#[derive(Clone)]
enum Entries<'a> {
    Tree(std::collections::btree_map::Iter<'a, i64, FootprintLevel>),
    Slice(std::slice::Iter<'a, FootprintLevel>),
}
impl<'a> Iterator for Entries<'a> {
    type Item = (&'a i64, &'a FootprintLevel);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Tree(rows) => rows.next(),
            Self::Slice(rows) => rows.next().map(|row| (&row.level, row)),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Tree(rows) => rows.size_hint(),
            Self::Slice(rows) => rows.size_hint(),
        }
    }
}
impl DoubleEndedIterator for Entries<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Tree(rows) => rows.next_back(),
            Self::Slice(rows) => rows.next_back().map(|row| (&row.level, row)),
        }
    }
}
impl ExactSizeIterator for Entries<'_> {}

impl<'a> FootprintRows<'a> {
    /// Rows and their keys in ascending order, without allocating.
    pub fn iter(
        self,
    ) -> impl DoubleEndedIterator<Item = (&'a i64, &'a FootprintLevel)> + ExactSizeIterator + Clone
    {
        match self {
            Self::Tree(rows) => Entries::Tree(rows.iter()),
            Self::Slice(rows) => Entries::Slice(rows.iter()),
        }
    }
    /// Rows in ascending order.
    pub fn values(
        self,
    ) -> impl DoubleEndedIterator<Item = &'a FootprintLevel> + ExactSizeIterator + Clone {
        self.iter().map(|(_, row)| row)
    }
    /// Keys in ascending order.
    pub fn keys(self) -> impl DoubleEndedIterator<Item = &'a i64> + ExactSizeIterator + Clone {
        self.iter().map(|(key, _)| key)
    }
    /// Number of rows.
    pub fn len(self) -> usize {
        match self {
            Self::Tree(rows) => rows.len(),
            Self::Slice(rows) => rows.len(),
        }
    }
    /// Whether no price traded.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    /// Finds a price row, using binary search for a slice.
    pub fn get(self, key: &i64) -> Option<&'a FootprintLevel> {
        match self {
            Self::Tree(rows) => rows.get(key),
            Self::Slice(rows) => rows
                .binary_search_by_key(key, |row| row.level)
                .ok()
                .map(|i| &rows[i]),
        }
    }
    /// Lowest traded price and its row.
    pub fn first_key_value(self) -> Option<(&'a i64, &'a FootprintLevel)> {
        match self {
            Self::Tree(rows) => rows.first_key_value(),
            Self::Slice(rows) => rows.first().map(|row| (&row.level, row)),
        }
    }
    /// Highest traded price and its row.
    pub fn last_key_value(self) -> Option<(&'a i64, &'a FootprintLevel)> {
        match self {
            Self::Tree(rows) => rows.last_key_value(),
            Self::Slice(rows) => rows.last().map(|row| (&row.level, row)),
        }
    }
}
impl PartialEq for FootprintRows<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

/// One bar's totals and delta extrema, with borrowed price rows.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct FootprintBarView<'a> {
    /// Clock-aligned opening time, matching the OHLC bar.
    pub time: DateTime<Utc>,
    /// Executed price rows in ascending order.
    pub levels: FootprintRows<'a>,
    /// Sell aggressor volume.
    pub bid: f64,
    /// Buy aggressor volume.
    pub ask: f64,
    /// Unclassified volume, kept separate from bid and ask.
    pub unknown: f64,
    /// Lowest running delta, including the initial zero.
    pub min_delta: f64,
    /// Highest running delta, including the initial zero.
    pub max_delta: f64,
}

impl<'a> From<&'a FootprintBar> for FootprintBarView<'a> {
    fn from(bar: &'a FootprintBar) -> Self {
        Self {
            time: bar.time,
            levels: FootprintRows::Tree(&bar.levels),
            bid: bar.bid,
            ask: bar.ask,
            unknown: bar.unknown,
            min_delta: bar.min_delta,
            max_delta: bar.max_delta,
        }
    }
}

impl<'a> FootprintBarView<'a> {
    /// Borrows `levels` at `time`, with zero totals and delta extrema. Fill these from the
    /// prepared bar; the page checks their consistency when it is accepted.
    pub fn new(time: DateTime<Utc>, levels: FootprintRows<'a>) -> Self {
        Self {
            time,
            levels,
            bid: 0.0,
            ask: 0.0,
            unknown: 0.0,
            min_delta: 0.0,
            max_delta: 0.0,
        }
    }
    /// Buy volume minus sell volume; zero for sides equal up to the rounding of their sums.
    pub fn delta(&self) -> f64 {
        side_delta(self.ask, self.bid)
    }

    /// Total executed volume.
    pub fn volume(&self) -> f64 {
        self.bid + self.ask + self.unknown
    }

    /// Executions summed across price rows, including unknown sides. An execution split
    /// across trade-based bars counts once in each bar it contributes to.
    pub fn trades(&self) -> u64 {
        self.levels.values().map(FootprintLevel::trades).sum()
    }

    /// Total volume divided by executions; undefined when no execution was counted.
    pub fn average_size(&self) -> Option<f64> {
        let trades = self.trades();
        (trades > 0).then(|| self.volume() / trades as f64)
    }

    /// Delta as a percentage of all executed volume; undefined for zero volume.
    pub fn delta_percent(&self) -> Option<f64> {
        (self.volume() > 0.0).then(|| self.delta() / self.volume() * 100.0)
    }

    /// Highest-volume level. A tie takes the lower price.
    pub fn poc(&self) -> Option<i64> {
        self.levels
            .values()
            .max_by(|a, b| {
                a.volume()
                    .total_cmp(&b.volume())
                    .then_with(|| b.level.cmp(&a.level))
            })
            .map(|level| level.level)
    }

    /// Contiguous value area grown from the POC toward the larger adjacent volume.
    /// Ties add one row on each side. `share` is clamped to 0..=1, NaN to zero.
    /// Gaps hold zero volume.
    /// Returns inclusive lower and upper level keys; a zero-volume bar has no area.
    pub fn value_area(&self, share: f64) -> Option<(i64, i64)> {
        if self.volume() <= 0.0 {
            return None;
        }
        let rows: Vec<_> = self.levels.values().collect();
        let poc = self.poc()?;
        let mut low = rows.partition_point(|row| row.level < poc);
        let mut high = low;
        let mut volume = rows[low].volume();
        let target = self.volume()
            * if share.is_nan() {
                0.0
            } else {
                share.clamp(0.0, 1.0)
            };
        let mut low_key = rows[low].level;
        let mut high_key = low_key;
        while volume < target && (low > 0 || high + 1 < rows.len()) {
            let below = low.checked_sub(1).map(|i| rows[i]);
            let above = rows.get(high + 1).copied();
            let below_gap = below.map(|row| low_key as i128 - row.level as i128);
            let above_gap = above.map(|row| row.level as i128 - high_key as i128);
            // Skip empty spans in one step, without comparing across untraded prices.
            let gap = below_gap.into_iter().chain(above_gap).min().unwrap_or(1);
            if gap > 1 {
                if below.is_some() {
                    low_key = (low_key as i128 - gap + 1) as i64;
                }
                if above.is_some() {
                    high_key = (high_key as i128 + gap - 1) as i64;
                }
            }
            let bid_volume = below.map(|row| {
                if row.level as i128 + 1 == low_key as i128 {
                    row.volume()
                } else {
                    0.0
                }
            });
            let ask_volume = above.map(|row| {
                if row.level as i128 - 1 == high_key as i128 {
                    row.volume()
                } else {
                    0.0
                }
            });
            if below.is_some() && (above.is_none() || bid_volume >= ask_volume) {
                low_key -= 1;
                if below.is_some_and(|row| row.level == low_key) {
                    low -= 1;
                    volume += rows[low].volume();
                }
            }
            if above.is_some() && (below.is_none() || ask_volume >= bid_volume) {
                high_key += 1;
                if above.is_some_and(|row| row.level == high_key) {
                    high += 1;
                    volume += rows[high].volume();
                }
            }
        }
        Some((low_key, high_key))
    }

    /// (bid, ask) dominance at `level`, comparing horizontally or to the adjacent price.
    /// Missing adjacent prices and the outer diagonals do not signal an imbalance.
    /// A positive side against a zero opposite side is imbalanced; sides equal within the ratio's
    /// rounding tolerance are not, at any ratio. The dominant side needs at least `min_volume`,
    /// inclusive and with the ratio's rounding tolerance; zero keeps every ratio signal. Ratios
    /// below 1, negative minima or non-finite values produce no signals.
    pub fn imbalance(
        &self,
        level: i64,
        ratio: f64,
        diagonal: bool,
        min_volume: f64,
    ) -> (bool, bool) {
        let Some(row) = self.levels.get(&level) else {
            return (false, false);
        };
        if !ratio.is_finite() || ratio < 1.0 || !min_volume.is_finite() || min_volume < 0.0 {
            return (false, false);
        }
        let dominant = |a: f64, b: f64| {
            !meets_threshold(b, a)
                && meets_threshold(a, min_volume)
                && meets_threshold(a / ratio, b)
        };
        if !diagonal {
            return (dominant(row.bid, row.ask), dominant(row.ask, row.bid));
        }
        let bid = level
            .checked_add(1)
            .and_then(|key| self.levels.get(&key))
            .is_some_and(|above| dominant(row.bid, above.ask));
        let ask = level
            .checked_sub(1)
            .and_then(|key| self.levels.get(&key))
            .is_some_and(|below| dominant(row.ask, below.bid));
        (bid, ask)
    }
}

/// Prepared bars moved into a page, or borrowed from an immutable shared owner.
///
/// Shared payloads compare by owner identity; owned payloads compare their bar values.
#[derive(Clone)]
#[non_exhaustive]
pub enum FootprintBars {
    /// Existing owned bars, accepted in any order.
    Owned(Vec<FootprintBar>),
    /// Immutable bars in strictly ascending time order.
    Shared(Arc<dyn FootprintSource>),
}

impl FootprintBars {
    /// Number of bars in the page.
    pub fn len(&self) -> usize {
        self.source().len()
    }
    /// Whether the page contains no bars.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Bar views in the payload's order, without copying rows.
    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = FootprintBarView<'_>> + ExactSizeIterator + Clone {
        (0..self.len()).map(|index| self.source().bar(index))
    }
    pub(crate) fn source(&self) -> &dyn FootprintSource {
        match self {
            Self::Owned(bars) => bars,
            Self::Shared(source) => source.as_ref(),
        }
    }
}
impl fmt::Debug for FootprintBars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owned(bars) => f.debug_tuple("Owned").field(bars).finish(),
            Self::Shared(source) => f
                .debug_struct("Shared")
                .field("owner", &Arc::as_ptr(source))
                .field("bars", &source.len())
                .finish(),
        }
    }
}
impl PartialEq for FootprintBars {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Owned(a), Self::Owned(b)) => a == b,
            (Self::Shared(a), Self::Shared(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
