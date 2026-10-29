//! The order book over time, for liquidity heatmaps: columns of resting size per price row on a
//! fixed time grid. The newest columns keep the grid's cadence; older ones merge in pairs, so the
//! history reaches far back in bounded memory.

use super::FeedError;
use super::depth::Book;
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::VecDeque;
use std::sync::Arc;

/// Resolutions a history keeps: the newest columns, then columns merged from two of them, from
/// four, and so on up to 128.
const TIERS: usize = 8;

/// How a [`DepthHistory`] samples the order book.
///
/// A column holds at most `rows` rows per side of 16 bytes each, and the history at most
/// `columns * 8` columns: about 15 MB with the defaults, which span at least 42 hours.
#[derive(Clone, Debug, PartialEq)]
pub struct DepthHistoryConfig {
    /// Length of a column. The book is sampled at the end of every period of this length counted
    /// from the Unix epoch, and the sample stands for the whole period. At least a millisecond;
    /// one second by default.
    pub cadence: TimeDelta,
    /// Price rows kept per side of each column, those nearest the best bid and ask. 100 by
    /// default.
    pub rows: usize,
    /// Ticks of the instrument's tick size grouped into one price row, their sizes summed. 1 by
    /// default.
    pub ticks_per_row: u32,
    /// Columns kept at each of 8 resolutions. The newest `columns` columns are samples; when there
    /// are more, the two oldest merge into one, and the merged columns overflow into the next
    /// resolution in the same way. At least 2; 600 by default.
    pub columns: usize,
    /// Sample the individual resting orders this filter accepts, summed into rows, instead of
    /// the aggregate prices the feed sends. Needs a feed that sends order events, see
    /// [`Sink::push_order_event`](crate::data::Sink::push_order_event): with aggregate depth only
    /// no column is sampled, and a period in which the filter cannot be applied (see
    /// [`OrderFilterUnavailable`](crate::data::OrderFilterUnavailable)) has no column. `None`
    /// samples the aggregate book.
    pub orders: Option<crate::data::OrderFilter>,
}

impl Default for DepthHistoryConfig {
    fn default() -> Self {
        Self {
            cadence: TimeDelta::seconds(1),
            rows: 100,
            ticks_per_row: 1,
            columns: 600,
            orders: None,
        }
    }
}

/// The resting size at each price row of the book over `[time, time + span)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DepthColumn {
    /// Start of the column, on the cadence grid.
    pub time: DateTime<Utc>,
    /// Length of the column: a whole number of cadences. A sample spans more than one cadence
    /// when the book did not change for longer, and a merged column spans the two it merged.
    pub span: TimeDelta,
    /// Bid rows, best first, as `(row, size)`: the row is the price in integer ticks of the
    /// instrument's tick size divided by `ticks_per_row`, rounded down, and the size is the
    /// resting size of the row's ticks, averaged over the span. A merged column keeps the rows of
    /// both columns, so its first row is the best bid of either.
    pub bids: Vec<(i64, f64)>,
    /// Ask rows, best first, as in [`bids`](Self::bids).
    pub asks: Vec<(i64, f64)>,
}

/// Columns of the order book of one chart, oldest first. Turn it on with
/// `Chart::set_depth_history` and read it with `Chart::depth_history`.
///
/// ```
/// use egui_charts::data::{DepthHistory, DepthHistoryConfig};
///
/// let config = DepthHistoryConfig { ticks_per_row: 4, ..Default::default() };
/// let history = DepthHistory::new(config).unwrap();
/// assert_eq!(history.columns().count(), 0);
/// ```
#[derive(Clone, Debug)]
pub struct DepthHistory {
    config: DepthHistoryConfig,
    /// Column `k` holds columns merged from up to `2^k` samples, oldest first. Every column of a
    /// tier is older than those of the tiers before it.
    /// Shared, so that a heatmap tells a column it drew from a new one by identity instead of
    /// keeping a copy of every column to compare with.
    tiers: Vec<VecDeque<Arc<DepthColumn>>>,
}

impl DepthHistory {
    /// An empty history, or an error if `config` breaks one of its limits.
    pub fn new(config: DepthHistoryConfig) -> Result<Self, FeedError> {
        if config.cadence < TimeDelta::milliseconds(1)
            || config.rows == 0
            || config.ticks_per_row == 0
            || config.columns < 2
        {
            return Err(FeedError::InvalidData("invalid depth history".into()));
        }
        Ok(Self {
            config,
            tiers: vec![VecDeque::new(); TIERS],
        })
    }

    /// The configuration the history samples with.
    pub fn config(&self) -> &DepthHistoryConfig {
        &self.config
    }

    /// Every column, oldest first. Consecutive columns are adjacent unless the chart lost columns.
    pub fn columns(&self) -> impl DoubleEndedIterator<Item = &DepthColumn> {
        self.shared().map(Arc::as_ref)
    }

    /// The columns of [`columns`](Self::columns), shared. A column is replaced, never changed, so
    /// a clone of the same `Arc` is the same column.
    pub(crate) fn shared(&self) -> impl DoubleEndedIterator<Item = &Arc<DepthColumn>> {
        self.tiers.iter().rev().flatten()
    }

    /// Merges a historical page, preserving every held column on overlap and keeping time gaps.
    /// Columns must use this history's grid and sorted, unique rows, with finite non-negative
    /// sizes and at most `config.rows` per side. Invalid pages leave the history unchanged.
    /// Memory remains bounded by the configured tiers. Returns the number of columns accepted.
    pub fn merge_page(&mut self, mut page: Vec<DepthColumn>) -> Result<usize, FeedError> {
        let cadence = self.config.cadence;
        let valid_side = |side: &[(i64, f64)], bid: bool| {
            side.len() <= self.config.rows
                && side
                    .iter()
                    .all(|(_, size)| size.is_finite() && *size >= 0.0)
                && side.windows(2).all(|w| {
                    if bid {
                        w[0].0 > w[1].0
                    } else {
                        w[0].0 < w[1].0
                    }
                })
        };
        if page.iter().any(|c| {
            start(c.time, cadence) != Some(c.time)
                || c.span <= TimeDelta::zero()
                || c.span
                    .num_nanoseconds()
                    .zip(cadence.num_nanoseconds())
                    .is_none_or(|(span, cadence)| span % cadence != 0)
                || c.time.checked_add_signed(c.span).is_none()
                || !valid_side(&c.bids, true)
                || !valid_side(&c.asks, false)
        }) {
            return Err(FeedError::InvalidData(
                "invalid historical depth columns".into(),
            ));
        }
        page.sort_by_key(|c| c.time);
        let mut held: Vec<_> = self.shared().cloned().collect();
        let mut accepted = Vec::new();
        for column in page {
            let at = held.partition_point(|c| c.time < column.time);
            let overlaps = (at > 0 && held[at - 1].time + held[at - 1].span > column.time)
                || held
                    .get(at)
                    .is_some_and(|c| c.time < column.time + column.span);
            if !overlaps {
                let column = Arc::new(column);
                held.insert(at, Arc::clone(&column));
                accepted.push(column);
            }
        }
        let count = accepted.len();
        if count == 0 {
            return Ok(0);
        }
        for column in accepted {
            self.insert_column(column);
        }
        self.compact();
        Ok(count)
    }

    pub(crate) fn clear(&mut self) {
        self.tiers.iter_mut().for_each(VecDeque::clear);
    }

    /// Live columns supersede overlapping history, including a page answered in the same drain.
    pub(crate) fn push(&mut self, column: impl Into<Arc<DepthColumn>>) {
        let column = column.into();
        if self
            .columns()
            .next_back()
            .is_some_and(|last| last.time + last.span > column.time)
        {
            let end = column.time + column.span;
            for tier in &mut self.tiers {
                tier.retain(|c| c.time >= end || c.time + c.span <= column.time);
            }
            self.insert_column(column);
        } else {
            self.tiers[0].push_back(column);
        }
        self.compact();
    }

    /// Places a column by its span, constrained by the ages of the held tiers. Scroll-back
    /// columns join the oldest occupied tier, leaving the newer resolutions intact.
    fn insert_column(&mut self, column: Arc<DepthColumn>) {
        let tier = if let Some(k) = self.tiers.iter().rposition(|tier| !tier.is_empty())
            && column.time < self.tiers[k][0].time
        {
            k
        } else {
            let resolution = (0..TIERS - 1)
                .find(|&k| {
                    self.config
                        .cadence
                        .checked_mul(1 << k)
                        .is_none_or(|span| column.span <= span)
                })
                .unwrap_or(TIERS - 1);
            let lower = self
                .tiers
                .iter()
                .rposition(|tier| tier.back().is_some_and(|c| c.time > column.time))
                .unwrap_or(0);
            let upper = self
                .tiers
                .iter()
                .position(|tier| tier.front().is_some_and(|c| c.time < column.time))
                .unwrap_or(TIERS - 1);
            resolution.clamp(lower, upper)
        };
        let at = self.tiers[tier].partition_point(|c| c.time < column.time);
        self.tiers[tier].insert(at, column);
    }

    /// Promotes only overflow, keeping columns in all other tiers at their held resolution.
    fn compact(&mut self) {
        for tier in 0..TIERS {
            while self.tiers[tier].len() > self.config.columns {
                let older = self.tiers[tier].pop_front().expect("a full tier");
                // A column lost between the two would be averaged over the time it does not cover.
                let merged = match self.tiers[tier].front() {
                    Some(newer) if older.time + older.span == newer.time => {
                        let newer = self.tiers[tier].pop_front().expect("a newer column");
                        Arc::new(merge(&older, &newer, self.config.rows))
                    }
                    _ => older,
                };
                if let Some(next) = self.tiers.get_mut(tier + 1) {
                    next.push_back(merged);
                }
            }
        }
    }
}

/// Samples a book into columns on the grid of a [`DepthHistoryConfig`].
pub(crate) struct Sampler {
    pub(crate) config: DepthHistoryConfig,
    /// Start of the period of the last update, whose sample is not taken yet.
    open: Option<DateTime<Utc>>,
}

impl Sampler {
    pub(crate) fn new(config: DepthHistoryConfig) -> Self {
        Self { config, open: None }
    }

    /// Whether an update stamped `ts` opens a period after the open one, so that `sample` reads
    /// the book.
    pub(crate) fn due(&self, ts: DateTime<Utc>) -> bool {
        start(ts, self.config.cadence)
            .zip(self.open)
            .is_some_and(|(period, open)| period > open)
    }

    /// The book cannot be sampled: the period that was open has no column, nor does the time until
    /// the next sample.
    pub(crate) fn lose(&mut self) {
        self.open = None;
    }

    /// Called before `book` applies an update stamped `ts`. When the update opens a later period,
    /// returns the book as it stood since the end of the open period's last update: one column
    /// from the open period to the update's. An update stamped before the open period's start
    /// samples nothing. `None` also for a book without a tick size.
    pub(crate) fn sample(&mut self, ts: DateTime<Utc>, book: &Book) -> Option<DepthColumn> {
        let period = start(ts, self.config.cadence)?;
        let open = self.open.replace(period.max(self.open.unwrap_or(period)))?;
        if period <= open {
            return None;
        }
        let [bids, asks] = book.rows(self.config.ticks_per_row, self.config.rows)?;
        Some(DepthColumn {
            time: open,
            span: period - open,
            bids,
            asks,
        })
    }
}

/// Start of the period of `cadence` that holds `ts`, counted from the Unix epoch.
fn start(ts: DateTime<Utc>, cadence: TimeDelta) -> Option<DateTime<Utc>> {
    let (ts, cadence) = (ts.timestamp_nanos_opt()?, cadence.num_nanoseconds()?);
    Some(DateTime::from_timestamp_nanos(
        ts.checked_sub(ts.rem_euclid(cadence))?,
    ))
}

/// One column spanning `older` and `newer`, adjacent, each row's size averaged over their spans.
fn merge(older: &DepthColumn, newer: &DepthColumn, rows: usize) -> DepthColumn {
    let span = older.span + newer.span;
    let weight = |part: TimeDelta| part.as_seconds_f64() / span.as_seconds_f64();
    let (a, b) = (weight(older.span), weight(newer.span));
    DepthColumn {
        time: older.time,
        span,
        bids: merge_side(&older.bids, &newer.bids, (a, b), rows, |row| -row),
        asks: merge_side(&older.asks, &newer.asks, (a, b), rows, |row| row),
    }
}

/// Merges two sides, best first, into their best `rows` rows. `order` ranks a row: lower is
/// better.
fn merge_side(
    older: &[(i64, f64)],
    newer: &[(i64, f64)],
    (a, b): (f64, f64),
    rows: usize,
    order: fn(i128) -> i128,
) -> Vec<(i64, f64)> {
    let mut merged = Vec::with_capacity(rows.min(older.len() + newer.len()));
    let (mut older, mut newer) = (older.iter().peekable(), newer.iter().peekable());
    while merged.len() < rows {
        let rank = |row: Option<&&(i64, f64)>| row.map(|(row, _)| order(*row as i128));
        let row = match (rank(older.peek()), rank(newer.peek())) {
            (None, None) => break,
            (Some(x), Some(y)) if x == y => {
                let ((row, x), (_, y)) = (older.next().unwrap(), newer.next().unwrap());
                (*row, x * a + y * b)
            }
            (Some(x), y) if y.is_none_or(|y| x < y) => {
                let (row, x) = older.next().unwrap();
                (*row, x * a)
            }
            _ => {
                let (row, y) = newer.next().unwrap();
                (*row, y * b)
            }
        };
        merged.push(row);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(second, 0).unwrap()
    }

    fn column(second: i64, span: i64, bid: f64) -> DepthColumn {
        DepthColumn {
            time: at(second),
            span: TimeDelta::seconds(span),
            bids: vec![(100, bid)],
            asks: Vec::new(),
        }
    }

    /// Guards the decimation: a full tier must merge its two oldest columns into one spanning
    /// both, with each row's size averaged over their spans (rows missing on one side count as
    /// empty, the union stays best first and is cut to `rows`), and the history must stay bounded
    /// with columns oldest first. Columns that are not adjacent must not be averaged together.
    #[test]
    fn full_tiers_merge_adjacent_columns_by_time_weight() {
        // Spans of a cadence that is not a whole number of milliseconds.
        let older = DepthColumn {
            time: at(0),
            span: TimeDelta::microseconds(500),
            bids: vec![(101, 4.0), (99, 2.0)],
            asks: vec![(102, 6.0)],
        };
        let newer = DepthColumn {
            time: at(0) + TimeDelta::microseconds(500),
            span: TimeDelta::microseconds(1_500),
            bids: vec![(100, 8.0), (99, 6.0)],
            asks: vec![(102, 2.0), (103, 4.0)],
        };
        // 101: 4 * 1/4; 100: 8 * 3/4; 99: (2 * 1 + 6 * 3) / 4; cut to two rows.
        assert_eq!(
            merge(&older, &newer, 2),
            DepthColumn {
                time: at(0),
                span: TimeDelta::milliseconds(2),
                bids: vec![(101, 1.0), (100, 6.0)],
                asks: vec![(102, 3.0), (103, 3.0)],
            }
        );

        let mut history = DepthHistory::new(config_columns(2)).unwrap();
        for second in 0..1_000 {
            history.push(column(second, 1, second as f64));
        }
        let spans = Vec::from_iter(history.columns().map(|c| c.span.num_seconds()));
        assert!(history.columns().count() <= 2 * TIERS, "{spans:?}");
        let times = Vec::from_iter(history.columns().map(|c| c.time));
        assert!(times.is_sorted(), "{times:?}");
        // The newest columns are samples and the oldest merged from 128 of them.
        assert_eq!((spans.first(), spans.last()), (Some(&128), Some(&1)));
        let first = history.columns().next().unwrap();
        let mean = (first.time.timestamp()..first.time.timestamp() + 128).sum::<i64>() / 128;
        assert_eq!(first.bids, [(100, mean as f64 + 0.5)]);

        let mut history = DepthHistory::new(config_columns(2)).unwrap();
        for (second, bid) in [(0, 1.0), (5, 3.0), (6, 5.0)] {
            history.push(column(second, 1, bid));
        }
        let held = Vec::from_iter(history.columns().map(|c| (c.time, c.span, c.bids[0].1)));
        let one = TimeDelta::seconds(1);
        assert_eq!(
            held,
            [(at(0), one, 1.0), (at(5), one, 3.0), (at(6), one, 5.0)]
        );
    }

    #[test]
    fn historical_pages_preserve_live_columns_and_gaps() {
        #[rustfmt::skip]
        let cases = [
            ("overlap", vec![column(1, 1, 1.0), column(2, 1, 99.0)],
                vec![(1, 1.0), (2, 2.0), (3, 3.0)]),
            ("gap", vec![column(-4, 1, 4.0)], vec![(-4, 4.0), (2, 2.0), (3, 3.0)]),
            ("empty", vec![], vec![(2, 2.0), (3, 3.0)]),
        ];
        for (name, page, expected) in cases {
            let mut history = DepthHistory::new(config_columns(10)).unwrap();
            history.push(column(2, 1, 2.0));
            history.push(column(3, 1, 3.0));
            history.merge_page(page).unwrap();
            let held: Vec<_> = history
                .columns()
                .map(|c| (c.time.timestamp(), c.bids[0].1))
                .collect();
            assert_eq!(held, expected, "{name}");
            history.push(column(3, 1, 30.0));
            assert_eq!(history.columns().filter(|c| c.time == at(3)).count(), 1);
            assert_eq!(history.columns().next_back().unwrap().bids, [(100, 30.0)]);
            let before: Vec<_> = history.columns().cloned().collect();
            assert!(history.merge_page(vec![column(1, -1, 1.0)]).is_err());
            let edge = DepthColumn {
                time: DateTime::from_timestamp_nanos(i64::MIN),
                ..column(1, 1, 1.0)
            };
            assert!(history.merge_page(vec![edge]).is_err());
            assert_eq!(history.columns().cloned().collect::<Vec<_>>(), before);
        }
    }

    /// Paging and live corrections must leave unrelated, already merged columns at their
    /// resolution, including when a page fills a gap inside the held history.
    #[test]
    fn history_updates_preserve_held_resolution() {
        let mut history = DepthHistory::new(config_columns(2)).unwrap();
        for second in 0..40 {
            history.push(column(second, 1, second as f64));
        }
        let before = history.tiers.clone();
        for page in [vec![], vec![column(0, 40, 99.0)]] {
            assert_eq!(history.merge_page(page).unwrap(), 0);
            assert_eq!(history.tiers, before);
        }
        for page in 1..=5 {
            let older = (-8 * page..-8 * (page - 1))
                .map(|s| column(s, 1, s.unsigned_abs() as f64))
                .collect();
            assert_eq!(history.merge_page(older).unwrap(), 8);
            for held in before.iter().flatten() {
                assert!(history.shared().any(|c| Arc::ptr_eq(c, held)));
            }
        }
        history.push(column(39, 1, 99.0));
        for held in before.iter().flatten().filter(|c| c.time < at(39)) {
            assert!(history.shared().any(|c| Arc::ptr_eq(c, held)));
        }
        let paged = history;

        // A coarse column followed by samples, with room for a page at each resolution.
        let mut history = DepthHistory::new(config_columns(3)).unwrap();
        history.merge_page(vec![column(0, 8, 4.0)]).unwrap();
        history.push(column(20, 1, 20.0));
        let held: Vec<_> = history.shared().cloned().collect();
        assert_eq!(
            history
                .merge_page(vec![column(8, 4, 8.0), column(16, 1, 16.0)])
                .unwrap(),
            2
        );
        history.push(column(12, 4, 12.0));
        for c in held {
            assert!(history.shared().any(|h| Arc::ptr_eq(h, &c)));
        }
        assert_eq!(
            history
                .columns()
                .map(|c| (c.time.timestamp(), c.span.num_seconds()))
                .collect::<Vec<_>>(),
            [(0, 8), (8, 4), (12, 4), (16, 1), (20, 1)]
        );
        for history in [&paged, &history] {
            assert!(
                history
                    .tiers
                    .iter()
                    .all(|tier| tier.len() <= history.config.columns)
            );
            let columns: Vec<_> = history.columns().collect();
            assert!(
                columns
                    .windows(2)
                    .all(|w| w[0].time + w[0].span <= w[1].time)
            );
        }
    }

    fn config_columns(columns: usize) -> DepthHistoryConfig {
        DepthHistoryConfig {
            columns,
            ..Default::default()
        }
    }
}
