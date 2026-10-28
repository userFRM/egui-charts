//! Trade volume at integer price levels. No volume or side is inferred from OHLC bars.

use super::FeedError;
use crate::model::{BarClock, OrderSide, Tick};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Executed volume at one grouped price, with sells on the bid and buys on the ask.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootprintLevel {
    /// Integer price level, in multiples of [`Footprint::price_step`].
    pub level: i64,
    /// Sell aggressor volume.
    pub bid: f64,
    /// Buy aggressor volume.
    pub ask: f64,
    /// Volume whose aggressor is unknown. Never assigned to either side.
    pub unknown: f64,
}

impl FootprintLevel {
    /// Buy volume minus sell volume.
    pub fn delta(&self) -> f64 {
        self.ask - self.bid
    }

    /// Total executed volume, including unclassified trades.
    pub fn volume(&self) -> f64 {
        self.bid + self.ask + self.unknown
    }

    /// Ask divided by bid. Undefined when the bid is zero.
    pub fn ratio(&self) -> Option<f64> {
        (self.bid > 0.0).then(|| self.ask / self.bid)
    }
}

/// How [`TradeProfile::nodes`](crate::studies::TradeProfile::nodes) chooses volume nodes.
///
/// The default compares each price row with its immediate neighbours, which is the most
/// sensitive setting. Raising `smoothing` or `prominence` leaves only the broader nodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeSensitivity {
    /// Rows on each side of a row that are averaged with it before comparing; zero compares
    /// the rows as traded. Untraded rows count as zero volume.
    pub smoothing: u32,
    /// Smallest rise of a peak above, or drop of a valley below, the higher respectively
    /// lower of its neighbouring plateaus, as a share of the largest smoothed volume. NaN
    /// counts as zero and values are clamped to 0..=1.
    pub prominence: f64,
}

/// One bar's executed volume and intrabar delta path, starting at zero.
#[derive(Clone, Debug, PartialEq)]
pub struct FootprintBar {
    /// Bar open, obtained from the supplied [`BarClock`].
    pub time: DateTime<Utc>,
    /// Traded levels in ascending price order; empty prices allocate nothing.
    pub levels: BTreeMap<i64, FootprintLevel>,
    /// Total sell aggressor volume.
    pub bid: f64,
    /// Total buy aggressor volume.
    pub ask: f64,
    /// Total unclassified volume.
    pub unknown: f64,
    /// Minimum running delta, including the initial zero.
    pub min_delta: f64,
    /// Maximum running delta, including the initial zero.
    pub max_delta: f64,
}

impl FootprintBar {
    /// A bar with no executions, opening at `time`.
    pub(crate) fn empty(time: DateTime<Utc>) -> Self {
        Self {
            time,
            levels: BTreeMap::new(),
            bid: 0.0,
            ask: 0.0,
            unknown: 0.0,
            min_delta: 0.0,
            max_delta: 0.0,
        }
    }

    /// Adds the executions of the next bar in time, extending the running delta path.
    pub(crate) fn absorb(&mut self, next: &FootprintBar) {
        let open = self.delta();
        self.min_delta = self.min_delta.min(open + next.min_delta);
        self.max_delta = self.max_delta.max(open + next.max_delta);
        self.bid += next.bid;
        self.ask += next.ask;
        self.unknown += next.unknown;
        for (&level, row) in &next.levels {
            let target = self.levels.entry(level).or_insert_with(|| FootprintLevel {
                level,
                ..Default::default()
            });
            target.bid += row.bid;
            target.ask += row.ask;
            target.unknown += row.unknown;
        }
    }

    /// Buy volume minus sell volume.
    pub fn delta(&self) -> f64 {
        self.ask - self.bid
    }

    /// Total executed volume.
    pub fn volume(&self) -> f64 {
        self.bid + self.ask + self.unknown
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
    /// Ties add both sides. `share` is clamped to 0..=1, NaN to zero. Gaps hold zero volume.
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
    /// A positive side against a zero opposite side is imbalanced. Ratios below 1 or
    /// non-finite ratios produce no signals.
    pub fn imbalance(&self, level: i64, ratio: f64, diagonal: bool) -> (bool, bool) {
        let Some(row) = self.levels.get(&level) else {
            return (false, false);
        };
        if !ratio.is_finite() || ratio < 1.0 {
            return (false, false);
        }
        let dominant = |a: f64, b: f64| a > b && a / ratio >= b;
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

/// Trade aggregation for one instrument, shared by footprints and delta studies.
///
/// Build once for historical trades and call [`push`](Self::push) for live executions.
/// Trades must be chronological, with exchange order preserved for equal timestamps, so
/// delta highs and lows are exact. Replay corrections by rebuilding, rather than silently
/// approximating their intrabar path. The host deduplicates ids (the chart's feed already does).
#[derive(Clone, Debug)]
pub struct Footprint {
    clock: BarClock,
    step: f64,
    tick_size: f64,
    ticks_per_row: u32,
    bars: BTreeMap<DateTime<Utc>, FootprintBar>,
    last: Option<DateTime<Utc>>,
    min_size: f64,
    max_size: f64,
}

impl Footprint {
    /// Creates an empty aggregation. `tick_size` must be finite and positive, and
    /// `ticks_per_row` nonzero. Prices round to the nearest integer tick, then group
    /// downwards, including negative prices. Grouping never changes trade-side totals.
    pub fn new(clock: BarClock, tick_size: f64, ticks_per_row: u32) -> Result<Self, FeedError> {
        let step = tick_size * ticks_per_row as f64;
        if !tick_size.is_finite() || tick_size <= 0.0 || ticks_per_row == 0 || !step.is_finite() {
            return Err(FeedError::InvalidData(
                "invalid footprint price step".into(),
            ));
        }
        Ok(Self {
            clock,
            step,
            tick_size,
            ticks_per_row,
            bars: BTreeMap::new(),
            last: None,
            min_size: 0.0,
            max_size: f64::MAX,
        })
    }

    /// Restricts executions to this inclusive size range, before aggregation.
    /// Must be called before adding trades. Invalid ranges return an error without changing it.
    pub fn set_size_filter(&mut self, min: f64, max: f64) -> Result<(), FeedError> {
        if self.last.is_some() || !min.is_finite() || !max.is_finite() || min < 0.0 || max < min {
            return Err(FeedError::InvalidData(
                "invalid or late footprint size filter".into(),
            ));
        }
        self.min_size = min;
        self.max_size = max;
        Ok(())
    }

    /// Height of one grouped price row.
    pub fn price_step(&self) -> f64 {
        self.step
    }

    /// The level a trade at `price` falls in: the nearest tick, grouped downwards. `None` when
    /// the price is not finite or the level overflows.
    pub fn level_of(&self, price: f64) -> Option<i64> {
        let level = ((price / self.tick_size).round() / self.ticks_per_row as f64).floor();
        (level.is_finite() && level >= i64::MIN as f64 && level < i64::MAX as f64)
            .then_some(level as i64)
    }

    /// Aggregated bars in chronological order.
    pub fn bars(&self) -> &BTreeMap<DateTime<Utc>, FootprintBar> {
        &self.bars
    }

    /// The aggregated bar containing `time`, using the same UTC slots as trade aggregation.
    /// Accepts a bar stamped anywhere inside its slot, as the feed history contract does.
    pub fn bar_at(&self, time: DateTime<Utc>) -> Option<&FootprintBar> {
        self.bars.get(&self.clock.bar_open(time))
    }

    /// The executions of the bars that contain `first` through `last`, inclusive, as one bar
    /// opening with the first of them: the exact profile of a span of the chart. Empty when
    /// nothing traded there or `last` precedes `first`.
    pub fn profile(&self, first: DateTime<Utc>, last: DateTime<Utc>) -> FootprintBar {
        let (first, last) = (self.clock.bar_open(first), self.clock.bar_open(last));
        let mut profile = FootprintBar::empty(first);
        if first <= last {
            for bar in self.bars.range(first..=last).map(|(_, bar)| bar) {
                profile.absorb(bar);
            }
        }
        profile
    }

    /// Adds an execution, returning whether it passed the size filter. Invalid, out-of-order
    /// or overflowing input returns an error and changes nothing. Unknown side stays unknown.
    /// Zero-sized executions are ignored.
    pub fn push(&mut self, trade: &Tick) -> Result<bool, FeedError> {
        let Some(level) = self.level_of(trade.price).filter(|_| {
            super::trades::valid(trade) && self.last.is_none_or(|last| trade.ts >= last)
        }) else {
            return Err(FeedError::InvalidData(
                "invalid or out-of-order footprint trade".into(),
            ));
        };
        if trade.size < self.min_size || trade.size > self.max_size || trade.size == 0.0 {
            return Ok(false);
        }
        let time = self.clock.bar_open(trade.ts);
        if self
            .bars
            .get(&time)
            .is_some_and(|bar| !(bar.volume() + trade.size).is_finite())
        {
            return Err(FeedError::InvalidData("footprint volume overflow".into()));
        }
        let bar = self.bars.entry(time).or_insert_with(|| FootprintBar {
            time,
            levels: BTreeMap::new(),
            bid: 0.0,
            ask: 0.0,
            unknown: 0.0,
            min_delta: 0.0,
            max_delta: 0.0,
        });
        let row = bar.levels.entry(level).or_insert_with(|| FootprintLevel {
            level,
            ..Default::default()
        });
        match trade.side {
            Some(OrderSide::Buy) => {
                row.ask += trade.size;
                bar.ask += trade.size;
            }
            Some(OrderSide::Sell) => {
                row.bid += trade.size;
                bar.bid += trade.size;
            }
            _ => {
                row.unknown += trade.size;
                bar.unknown += trade.size;
            }
        }
        bar.min_delta = bar.min_delta.min(bar.delta());
        bar.max_delta = bar.max_delta.max(bar.delta());
        self.last = Some(trade.ts);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe;
    use chrono::TimeDelta;

    #[test]
    fn trades_keep_side_path_grouping_and_value_area() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        // Running delta: 0, -3, 5, 3, 3, 7. Total volume 24, bid 5, ask 12, unknown 7.
        for (i, price, size, side) in [
            (0, 100.0, 3.0, Some(OrderSide::Sell)),
            (1, 100.25, 8.0, Some(OrderSide::Buy)),
            (2, 100.0, 2.0, Some(OrderSide::Sell)),
            (3, 100.5, 7.0, None),
            (4, 100.75, 4.0, Some(OrderSide::Buy)),
        ] {
            flow.push(&Tick::new(start + TimeDelta::seconds(i), price, size, side))
                .unwrap();
        }
        let bar = &flow.bars()[&start];
        assert_eq!(
            (
                bar.bid,
                bar.ask,
                bar.volume(),
                bar.delta(),
                bar.min_delta,
                bar.max_delta
            ),
            (5.0, 12.0, 24.0, 7.0, -3.0, 7.0)
        );
        assert_eq!(flow.bar_at(start + TimeDelta::seconds(59)), Some(bar));
        assert_eq!(bar.poc(), Some(401));
        // POC 8, choose upper neighbour 7 over lower 5, then lower 5: 20 >= 16.8.
        assert_eq!(bar.value_area(0.7), Some((400, 402)));
        assert_eq!(bar.imbalance(401, 1.5, true), (false, true));
        assert_eq!(bar.imbalance(403, 1.5, true), (false, true));
        assert_eq!(bar.delta_percent(), Some(7.0 / 24.0 * 100.0));
        let snapshot = flow.bars().clone();
        for trade in [
            Tick::new(start, 100.0, 1.0, None),
            Tick::new(start + TimeDelta::seconds(5), f64::NAN, 1.0, None),
        ] {
            assert!(flow.push(&trade).is_err());
            assert_eq!(flow.bars(), &snapshot);
        }
        // A negative price groups down, and decimal ticks do not fall into the row below.
        let mut grouped = Footprint::new(BarClock::new(Timeframe::Min1), 0.1, 3).unwrap();
        for price in [-0.1, 0.3, 0.6] {
            grouped.push(&Tick::new(start, price, 1.0, None)).unwrap();
        }
        assert_eq!(
            grouped.bars()[&start]
                .levels
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [-1, 1, 2]
        );
        // The row at 110 is not adjacent to the POC at 100. First take the 4 below it.
        let mut sparse = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, size) in [(99.0, 4.0), (100.0, 10.0), (110.0, 9.0)] {
            sparse.push(&Tick::new(start, price, size, None)).unwrap();
        }
        assert_eq!(sparse.bars()[&start].value_area(0.6), Some((99, 100)));
    }
}
