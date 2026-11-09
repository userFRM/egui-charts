//! Trade volume at integer price levels. No volume or side is inferred from OHLC bars.

use super::FeedError;
use crate::model::{BarClock, OrderSide, Tick};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::ops::RangeBounds;

/// Inclusive thresholds allow a relative error of 1e-9, which covers the rounding of sizes
/// summed trade by trade, and is far below one lot at any realistic volume.
pub(crate) fn meets_threshold(value: f64, threshold: f64) -> bool {
    value >= threshold * (1.0 - 1e-9)
}

/// Executed volume and execution counts at one grouped price, sells on the bid and buys on the ask.
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
    /// Number of sell aggressor executions contributing to this row.
    pub bid_trades: u64,
    /// Number of buy aggressor executions contributing to this row.
    pub ask_trades: u64,
    /// Number of executions whose aggressor is unknown.
    pub unknown_trades: u64,
}

impl FootprintLevel {
    pub(crate) fn absorb(&mut self, next: &Self) {
        self.bid += next.bid;
        self.ask += next.ask;
        self.unknown += next.unknown;
        self.bid_trades += next.bid_trades;
        self.ask_trades += next.ask_trades;
        self.unknown_trades += next.unknown_trades;
    }

    /// Total executions, including those whose aggressor is unknown.
    pub fn trades(&self) -> u64 {
        self.bid_trades + self.ask_trades + self.unknown_trades
    }

    /// Total volume divided by executions; undefined when no execution was counted.
    pub fn average_size(&self) -> Option<f64> {
        let trades = self.trades();
        (trades > 0).then(|| self.volume() / trades as f64)
    }

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

pub use crate::model::NodeSensitivity;

/// One bar's executed volume and intrabar delta path, starting at zero.
#[derive(Clone, Debug, PartialEq)]
pub struct FootprintBar {
    /// Bar open from the supplied clock or trade-bar allocation.
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
        self.absorb_grouped(next, 1);
    }

    /// [`absorb`](Self::absorb) with every `group` neighbouring rows of `next` summed into the
    /// row keyed by their quotient.
    pub(crate) fn absorb_grouped(&mut self, next: &FootprintBar, group: i64) {
        let open = self.delta();
        self.min_delta = self.min_delta.min(open + next.min_delta);
        self.max_delta = self.max_delta.max(open + next.max_delta);
        self.bid += next.bid;
        self.ask += next.ask;
        self.unknown += next.unknown;
        for (&level, row) in &next.levels {
            let level = level.div_euclid(group);
            let target = self.levels.entry(level).or_insert_with(|| FootprintLevel {
                level,
                ..Default::default()
            });
            target.absorb(row);
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
    /// A positive side against a zero opposite side is imbalanced. The dominant side needs at
    /// least `min_volume`, inclusive and with the ratio's rounding tolerance; zero keeps every
    /// ratio signal. Ratios below 1, negative minima or non-finite values produce no signals.
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
            a > b && meets_threshold(a, min_volume) && meets_threshold(a / ratio, b)
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

/// Trade aggregation for one instrument, shared by footprints and delta studies.
///
/// Build once for historical trades and call [`push`](Self::push) for live executions.
/// Trades must be chronological, with exchange order preserved for equal timestamps, so
/// delta highs and lows are exact. Replay corrections by rebuilding, rather than silently
/// approximating their intrabar path. The host deduplicates ids (the chart's feed already does).
/// Use [`from_bars`](Self::from_bars) for already aggregated snapshots instead.
#[derive(Clone, Debug)]
pub struct Footprint {
    clock: BarClock,
    step: f64,
    tick_size: f64,
    ticks_per_row: u32,
    bars: BTreeMap<DateTime<Utc>, FootprintBar>,
    trade_bars: super::inbox::TradeBars,
    /// First contributing exchange timestamp, before equal-time opens are disambiguated.
    trade_starts: BTreeMap<DateTime<Utc>, DateTime<Utc>>,
    /// Price rows held across all bars.
    rows: usize,
    last: Option<DateTime<Utc>>,
    min_size: f64,
    max_size: f64,
    max_bars: usize,
    max_rows: usize,
    /// Bars opening up to this time lack executions that were dropped or never received.
    incomplete_through: Option<DateTime<Utc>>,
}

impl Footprint {
    /// Builds a footprint from bars already aggregated by the host, without reconstructing
    /// executions. Volumes, execution counts and delta extremes are kept as supplied.
    ///
    /// Opens must be unique and aligned to the time-based clock; row keys must round-trip
    /// through the price grid. Side totals allow relative rounding error of 1e-9, and delta
    /// bounds allow 1e-9 of the larger side volume. Invalid input returns `FeedError::InvalidData`.
    /// Trade-based clocks and bars with no price rows are rejected.
    ///
    /// The snapshot covers every bar from its first one; an omitted bar had no executions.
    /// Start the snapshot after any gap in the host's coverage. The host supplies matching
    /// OHLC and replaces history for corrections.
    ///
    /// Executions pushed afterwards continue the newest bar and must not already be counted
    /// in it. The host must resume after the last included execution, preserving exchange order
    /// for equal timestamps and deduplicating overlap. A bar's open does not identify that
    /// execution cutoff: only executions before the newest bar's open can be rejected here.
    /// Execution-size filters must be applied before aggregation.
    pub fn from_bars(
        clock: BarClock,
        tick_size: f64,
        ticks_per_row: u32,
        bars: impl IntoIterator<Item = FootprintBar>,
    ) -> Result<Self, FeedError> {
        let mut flow = Self::new(clock, tick_size, ticks_per_row)?;
        if flow.clock.timeframe().is_none_or(|tf| tf.is_trade_based()) {
            return Err(FeedError::InvalidData(format!(
                "footprint aggregate snapshots require a time-based clock, got {}",
                flow.clock
                    .timeframe()
                    .map_or_else(|| "an exact clock".into(), |tf| tf.to_string())
            )));
        }
        let mut trades = 0_u64;
        for bar in bars {
            let invalid =
                |reason| FeedError::InvalidData(format!("footprint bar at {} {reason}", bar.time));
            if flow.clock.bar_open(bar.time) != bar.time {
                return Err(invalid("does not open on the clock".into()));
            }
            if flow.bars.contains_key(&bar.time) {
                return Err(invalid("is supplied more than once".into()));
            }
            if bar.levels.is_empty() {
                return Err(invalid("has no price rows".into()));
            }
            let mut totals = [0.0; 3];
            for (&level, row) in &bar.levels {
                if level != row.level {
                    return Err(invalid(format!(
                        "row {level} has mismatched level {}",
                        row.level
                    )));
                }
                if flow.level_of(level as f64 * flow.step) != Some(level) {
                    return Err(invalid(format!(
                        "row {level} is outside the grid of step {:?}",
                        flow.step
                    )));
                }
                for (i, (side, value, count)) in [
                    ("bid", row.bid, row.bid_trades),
                    ("ask", row.ask, row.ask_trades),
                    ("unknown", row.unknown, row.unknown_trades),
                ]
                .into_iter()
                .enumerate()
                {
                    if !value.is_finite() {
                        return Err(invalid(format!(
                            "row {level} {side} volume must be finite, got {value:?}"
                        )));
                    }
                    if value < 0.0 {
                        return Err(invalid(format!(
                            "row {level} {side} volume must be at least 0, got {value:?}"
                        )));
                    }
                    if value == 0.0 && count > 0 {
                        return Err(invalid(format!(
                            "row {level} {side} count {count} has no volume"
                        )));
                    }
                    if value > 0.0 && count == 0 {
                        return Err(invalid(format!(
                            "row {level} {side} volume {value:?} has no executions"
                        )));
                    }
                    totals[i] += value;
                    trades = trades
                        .checked_add(count)
                        .filter(|total| *total <= u64::MAX / 2)
                        .ok_or_else(|| invalid(format!(
                            "row {level} {side} count {count} takes the snapshot past {} executions",
                            u64::MAX / 2
                        )))?;
                }
                if !row.volume().is_finite() {
                    return Err(invalid(format!(
                        "row {level} volume must be finite, got {:?}",
                        row.volume()
                    )));
                }
                if row.volume() == 0.0 {
                    return Err(invalid(format!("row {level} has no executed volume")));
                }
            }
            for ((side, value), total) in
                [("bid", bar.bid), ("ask", bar.ask), ("unknown", bar.unknown)]
                    .into_iter()
                    .zip(totals)
            {
                if !value.is_finite() {
                    return Err(invalid(format!(
                        "{side} volume must be finite, got {value:?}"
                    )));
                }
                if value < 0.0 {
                    return Err(invalid(format!(
                        "{side} volume must be at least 0, got {value:?}"
                    )));
                }
                if !total.is_finite() {
                    return Err(invalid(format!(
                        "sum of row {side} volumes must be finite, got {total:?}"
                    )));
                }
                if !near(value, total) {
                    return Err(invalid(format!(
                        "{side} volume {value:?} differs from row total {total:?}"
                    )));
                }
            }
            if !bar.volume().is_finite() {
                return Err(invalid(format!(
                    "volume must be finite, got {:?}",
                    bar.volume()
                )));
            }
            for (name, value) in [("min_delta", bar.min_delta), ("max_delta", bar.max_delta)] {
                if !value.is_finite() {
                    return Err(invalid(format!("{name} must be finite, got {value:?}")));
                }
            }
            if bar.min_delta > 0.0 {
                return Err(invalid(format!(
                    "min_delta must be at most 0, got {:?}",
                    bar.min_delta
                )));
            }
            if bar.max_delta < 0.0 {
                return Err(invalid(format!(
                    "max_delta must be at least 0, got {:?}",
                    bar.max_delta
                )));
            }
            let slack = 1e-9 * bar.bid.max(bar.ask);
            let at_least = |value: f64, bound: f64| value >= bound - slack;
            if !at_least(bar.delta(), bar.min_delta) {
                return Err(invalid(format!(
                    "min_delta {:?} exceeds final delta {:?}",
                    bar.min_delta,
                    bar.delta()
                )));
            }
            if !at_least(bar.max_delta, bar.delta()) {
                return Err(invalid(format!(
                    "max_delta {:?} is below final delta {:?}",
                    bar.max_delta,
                    bar.delta()
                )));
            }
            // Visiting both extrema requires their distance in buys or sells.
            if !at_least(bar.bid.max(bar.ask), bar.max_delta - bar.min_delta) {
                return Err(invalid(format!(
                    "delta span {:?} exceeds side volume {:?}",
                    bar.max_delta - bar.min_delta,
                    bar.bid.max(bar.ask)
                )));
            }
            flow.rows += bar.levels.len();
            flow.bars.insert(bar.time, bar);
        }
        flow.last = flow.bars.last_key_value().map(|(time, _)| *time);
        Ok(flow)
    }

    /// Creates an empty aggregation. `tick_size` must be finite and positive, and
    /// `ticks_per_row` nonzero. Prices round to the nearest integer tick, then group
    /// downwards, including negative prices. Grouping never changes trade-side totals.
    pub fn new(clock: BarClock, tick_size: f64, ticks_per_row: u32) -> Result<Self, FeedError> {
        let step = tick_size * ticks_per_row as f64;
        if !tick_size.is_finite() || tick_size <= 0.0 {
            return Err(FeedError::InvalidData(format!(
                "footprint tick_size must be finite and above zero, got {tick_size:?}"
            )));
        }
        if ticks_per_row == 0 {
            return Err(FeedError::InvalidData(
                "footprint ticks_per_row must be at least 1, got 0".into(),
            ));
        }
        if !step.is_finite() {
            return Err(FeedError::InvalidData(format!(
                "footprint price step of tick_size {tick_size:?} times ticks_per_row {ticks_per_row} is not finite"
            )));
        }
        Ok(Self {
            clock,
            step,
            tick_size,
            ticks_per_row,
            bars: BTreeMap::new(),
            trade_bars: Default::default(),
            trade_starts: BTreeMap::new(),
            rows: 0,
            last: None,
            min_size: 0.0,
            max_size: f64::MAX,
            max_bars: usize::MAX,
            max_rows: usize::MAX,
            incomplete_through: None,
        })
    }

    /// Keeps at most `bars` bars and `rows` price rows in all, dropping the oldest bars first.
    /// The newest bar is always kept, whole. Dropped bars are counted as incomplete.
    pub(crate) fn set_budget(&mut self, bars: usize, rows: usize) {
        self.max_bars = bars;
        self.max_rows = rows;
        self.enforce_budget();
    }

    /// Open of the oldest bar whose executions are all held: earlier bars were cut by the
    /// budget or by trades dropped from the host's store, or may lack trades that never
    /// arrived, and must not be shown as complete.
    pub fn complete_from(&self) -> Option<DateTime<Utc>> {
        self.complete_bars(..).next().map(|bar| bar.time)
    }

    /// The bars in `range` from [`complete_from`](Self::complete_from) on. Panics as
    /// [`BTreeMap::range`] does.
    pub(crate) fn complete_bars(
        &self,
        range: impl RangeBounds<DateTime<Utc>>,
    ) -> impl Iterator<Item = &FootprintBar> {
        let cut = self.incomplete_through;
        self.bars
            .range(range)
            .filter(move |(time, _)| cut.is_none_or(|cut| **time > cut))
            .map(|(_, bar)| bar)
    }

    /// Takes out the executions dropped from the host's store, the newest at `through`, and
    /// rebuilds the bar containing it from `held`, the executions still held in order. Earlier
    /// bars go. Delta extrema cannot be recovered by subtraction, so the bar is summed again.
    #[cfg_attr(
        all(not(feature = "widget"), not(test)),
        expect(dead_code, reason = "only the chart widget evicts from its footprint")
    )]
    pub(crate) fn drop_through<'a>(
        &mut self,
        through: DateTime<Utc>,
        held: impl IntoIterator<Item = &'a Tick>,
    ) {
        if self.clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            // Keep the allocation anchor: rebuilding from the retained suffix shifts count bars.
            if let Some(open) = self
                .trade_starts
                .iter()
                .filter(|(_, first)| **first <= through)
                .map(|(open, _)| *open)
                .next_back()
            {
                self.incomplete_through = self.incomplete_through.max(Some(open));
                self.bars.retain(|time, _| *time > open);
                self.trade_starts.retain(|time, _| *time > open);
                self.rows = self.bars.values().map(|bar| bar.levels.len()).sum();
            }
            return;
        }
        let open = self.clock.bar_open(through);
        self.incomplete_through = self.incomplete_through.max(Some(open));
        self.bars = self.bars.split_off(&open);
        if self.bars.contains_key(&open) {
            let mut part = Self {
                clock: self.clock.clone(),
                bars: BTreeMap::new(),
                trade_bars: Default::default(),
                trade_starts: BTreeMap::new(),
                rows: 0,
                last: None,
                ..*self
            };
            for trade in held
                .into_iter()
                .take_while(|trade| self.clock.bar_open(trade.ts) <= open)
            {
                let _ = part.push(trade);
            }
            match part.bars.remove(&open) {
                Some(bar) => self.bars.insert(open, bar),
                None => self.bars.remove(&open),
            };
        }
        self.rows = self.bars.values().map(|bar| bar.levels.len()).sum();
    }

    /// Counts the bar holding `first`, the oldest execution held, and earlier bars as
    /// incomplete: executions before it may exist that never arrived.
    pub(crate) fn received_from(&mut self, first: DateTime<Utc>) {
        let open = self.clock.bar_open(first);
        self.incomplete_through = self.incomplete_through.max(Some(open));
    }

    fn enforce_budget(&mut self) {
        while self.bars.len() > 1 && (self.bars.len() > self.max_bars || self.rows > self.max_rows)
        {
            if let Some((time, bar)) = self.bars.pop_first() {
                self.rows -= bar.levels.len();
                self.trade_starts.remove(&time);
                self.incomplete_through = self.incomplete_through.max(Some(time));
            }
        }
    }

    /// Copies changed sink allocations without replaying or shifting their grouping anchor.
    #[cfg(feature = "widget")]
    pub(crate) fn copy_trade_bars(&mut self, source: &Self, from: Option<DateTime<Utc>>) {
        let Some(from) = from.or_else(|| source.bars.first_key_value().map(|(time, _)| *time))
        else {
            return;
        };
        let removed = self.bars.split_off(&from);
        self.rows -= removed.values().map(|bar| bar.levels.len()).sum::<usize>();
        self.trade_starts.retain(|time, _| *time < from);
        for (time, bar) in source.bars.range(from..) {
            self.rows += bar.levels.len();
            self.bars.insert(*time, bar.clone());
        }
        self.trade_starts.extend(
            source
                .trade_starts
                .range(from..)
                .map(|(time, first)| (*time, *first)),
        );
        self.incomplete_through = self.incomplete_through.max(source.incomplete_through);
        self.enforce_budget();
    }

    /// Restricts executions to this inclusive size range, before aggregation.
    /// Must be called before adding trades. Invalid ranges return an error without changing it.
    pub fn set_size_filter(&mut self, min: f64, max: f64) -> Result<(), FeedError> {
        if self.last.is_some() || self.trade_bars.forming.is_some() {
            return Err(FeedError::InvalidData(
                "footprint size filter must be set before the first trade".into(),
            ));
        }
        if !min.is_finite() || min < 0.0 {
            return Err(FeedError::InvalidData(format!(
                "footprint size filter min must be finite and at least 0, got {min:?}"
            )));
        }
        if !max.is_finite() {
            return Err(FeedError::InvalidData(format!(
                "footprint size filter max must be finite, got {max:?}"
            )));
        }
        if max < min {
            return Err(FeedError::InvalidData(format!(
                "footprint size filter max {max:?} is below min {min:?}"
            )));
        }
        self.min_size = min;
        self.max_size = max;
        Ok(())
    }

    /// Instrument tick size before price-row grouping.
    pub fn tick_size(&self) -> f64 {
        self.tick_size
    }

    /// Height of one grouped price row.
    pub fn price_step(&self) -> f64 {
        self.step
    }

    /// The calendar the bars open by.
    #[cfg(feature = "studies")]
    pub(crate) fn clock(&self) -> &BarClock {
        &self.clock
    }

    /// The level a trade at `price` falls in: the nearest tick, grouped downwards. `None` when
    /// the price is not finite or the level overflows.
    pub fn level_of(&self, price: f64) -> Option<i64> {
        let level = ((price / self.tick_size).round() / self.ticks_per_row as f64).floor();
        (level.is_finite() && level >= i64::MIN as f64 && level < i64::MAX as f64)
            .then_some(level as i64)
    }

    /// Aggregated bars in opening order. Trade-based bars sharing an exchange timestamp
    /// have distinct opens, spaced one nanosecond apart as in `aggregate_trades`.
    pub fn bars(&self) -> &BTreeMap<DateTime<Utc>, FootprintBar> {
        &self.bars
    }

    /// The aggregated bar containing `time`, using the same UTC slots as trade aggregation.
    /// Accepts a bar stamped anywhere inside its time slot, as the feed history contract does.
    /// Trade-based bars require their exact allocated opening time.
    pub fn bar_at(&self, time: DateTime<Utc>) -> Option<&FootprintBar> {
        self.bars.get(&self.clock.bar_open(time))
    }

    /// The executions of the bars that contain `first` through `last`, inclusive, as one bar
    /// opening with the first of them: the exact profile of a span of the chart. Bars before
    /// [`complete_from`](Self::complete_from) are left out, as their executions are not all
    /// held. Empty when no complete bar traded there or `last` precedes `first`.
    /// Counts add the contributing bars, including split executions counted in more than one bar.
    pub fn profile(&self, first: DateTime<Utc>, last: DateTime<Utc>) -> FootprintBar {
        let (first, last) = (self.clock.bar_open(first), self.clock.bar_open(last));
        let mut profile = FootprintBar::empty(first);
        if first <= last {
            for bar in self.complete_bars(first..=last) {
                profile.absorb(bar);
            }
        }
        profile
    }

    /// Adds an execution, returning whether it passed the size filter. Invalid, out-of-order
    /// or overflowing input returns an error and changes nothing. Unknown side stays unknown.
    /// Trade-based intervals use the same grouping anchor and fill splits as `aggregate_trades`;
    /// push every execution, including those outside the size filter. A filter tests the original
    /// execution size before summing its allocated parts. Zero-sized executions add no volume
    /// but count toward tick-bar boundaries.
    pub fn push(&mut self, trade: &Tick) -> Result<bool, FeedError> {
        self.push_allocated(trade, None)
    }

    /// Adds paged executions only to the `history` bars they rebuild, on a trade-based interval.
    /// Bars there are grouped from their first execution, which a page need not start with, so
    /// each candidate is tried against the chart's bars instead: an execution first at the time a
    /// bar opens, grouped on from there. A second bar that opens and sums as grouped, or the page
    /// ending exactly with the first, proves the start; one bar alone could be a fill split by
    /// the bar before it. On volume and delta intervals a second bar proves the start only when
    /// the bar before closes at another price than every execution at the start's time, and the
    /// page's end only when it ends with the last of `history`. The newest proven run is kept, so
    /// its bars have no gap. Older bars get no cells and draw candles. With `before`, adds only
    /// the executions strictly before it.
    #[cfg(feature = "widget")]
    pub(crate) fn push_history(
        &mut self,
        trades: &[&Tick],
        history: &[crate::model::Bar],
        before: Option<DateTime<Utc>>,
    ) {
        let Some(timeframe) = self.clock.timeframe().filter(|tf| tf.is_trade_based()) else {
            return;
        };
        let mut newest = Vec::new();
        let (mut at, mut unproven) = (0, 0);
        while at < trades.len() {
            let start = at;
            at += 1;
            if start > 0 && trades[start - 1].ts == trades[start].ts {
                continue;
            }
            let Ok(first) = history.binary_search_by_key(&trades[start].ts, |bar| bar.time) else {
                continue;
            };
            // A fill split by a boundary opens the next bar with its rest at its own time. Counted
            // whole, it can match every later bar while moving part of each boundary fill, so on
            // volume and delta intervals only the page's end, or a bar before that closes at
            // another price than every execution at the start's time, proves the start.
            let whole = !matches!(
                timeframe,
                crate::model::Timeframe::Volume(_) | crate::model::Timeframe::Delta(_)
            ) || first.checked_sub(1).is_some_and(|before| {
                (trades[start..].iter())
                    .take_while(|trade| trade.ts == trades[start].ts)
                    .all(|trade| !near(trade.price, history[before].close))
            });
            // Without that proof a start counts only when it ends with the page. Inside the bars
            // an earlier such start matched, it repeats that grouping or moves a boundary fill,
            // and neither ends with the page.
            if !whole && start < unproven {
                continue;
            }
            let run = history_run(
                timeframe,
                Some(self.tick_size),
                &trades[start..],
                &history[first..],
                trades[start].size,
                None,
            );
            // The page's end pins the split only with the last bar: an earlier one has executions
            // after the page, and when they sum to the part a grouping moved, it matches as well.
            let ends = run.end.is_some()
                && run.parts.last().map(|part| part.1) == history.last().map(|bar| bar.time);
            if run.proven && (whole || ends) {
                newest = run.parts;
                at = start + run.next.max(1);
            } else if run.proven {
                unproven = unproven.max(start + run.next);
            }
        }
        for (trade, open, size) in newest {
            if before.is_none_or(|before| trade.ts < before) {
                let _ = self.push_part(trade, open, size);
            }
        }
    }

    /// Adds a part already allocated by the live sink, without starting another grouping anchor.
    pub(crate) fn push_part(
        &mut self,
        trade: &Tick,
        open: DateTime<Utc>,
        size: f64,
    ) -> Result<bool, FeedError> {
        self.push_allocated(trade, Some((open, size)))
    }

    fn push_allocated(
        &mut self,
        trade: &Tick,
        part: Option<(DateTime<Utc>, f64)>,
    ) -> Result<bool, FeedError> {
        if !trade.price.is_finite() {
            return Err(FeedError::InvalidData(format!(
                "footprint trade price must be finite, got {:?}",
                trade.price
            )));
        }
        if !trade.size.is_finite() || trade.size < 0.0 {
            return Err(FeedError::InvalidData(format!(
                "footprint trade size must be finite and at least 0, got {:?}",
                trade.size
            )));
        }
        let Some(level) = self.level_of(trade.price) else {
            return Err(FeedError::InvalidData(format!(
                "footprint trade price {:?} is outside the grid of step {:?}",
                trade.price, self.step
            )));
        };
        if let Some(last) = self.last.filter(|last| trade.ts < *last) {
            return Err(FeedError::InvalidData(format!(
                "footprint trade at {} precedes the last trade at {last}",
                trade.ts
            )));
        }
        let mut allocation = self.trade_bars.clone();
        let single = part.unwrap_or((self.clock.bar_open(trade.ts), trade.size));
        let split;
        let parts = if part.is_none()
            && let Some(timeframe) = self.clock.timeframe().filter(|tf| tf.is_trade_based())
        {
            split = allocation
                .push(timeframe, trade, Some(self.tick_size))
                .map_err(|()| {
                    FeedError::InvalidData(
                        "footprint trade does not fold into the bars of the interval".into(),
                    )
                })?
                .into_iter()
                .map(|(bar, size)| (bar.time, size))
                .collect::<Vec<_>>();
            split.as_slice()
        } else {
            std::slice::from_ref(&single)
        };
        let accepted =
            trade.size >= self.min_size && trade.size <= self.max_size && trade.size > 0.0;
        if accepted
            && parts.iter().any(|(time, size)| {
                self.bars.get(time).is_some_and(|bar| {
                    !(bar.volume() + size).is_finite()
                        || bar
                            .levels
                            .get(&level)
                            .is_some_and(|row| !(row.volume() + size).is_finite())
                })
            })
        {
            return Err(FeedError::InvalidData("footprint volume overflow".into()));
        }
        self.trade_bars = allocation;
        if !accepted {
            return Ok(false);
        }
        for &(time, size) in parts {
            self.add_part(trade, level, time, size);
        }
        self.last = Some(trade.ts);
        self.enforce_budget();
        Ok(true)
    }

    fn add_part(&mut self, trade: &Tick, level: i64, time: DateTime<Utc>, size: f64) {
        if self.clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            self.trade_starts.entry(time).or_insert(trade.ts);
        }
        let bar = self
            .bars
            .entry(time)
            .or_insert_with(|| FootprintBar::empty(time));
        let row = bar.levels.entry(level).or_insert_with(|| {
            self.rows += 1;
            FootprintLevel {
                level,
                ..Default::default()
            }
        });
        match trade.side {
            Some(OrderSide::Buy) => {
                row.ask += size;
                row.ask_trades += 1;
                bar.ask += size;
            }
            Some(OrderSide::Sell) => {
                row.bid += size;
                row.bid_trades += 1;
                bar.bid += size;
            }
            _ => {
                row.unknown += size;
                row.unknown_trades += 1;
                bar.unknown += size;
            }
        }
        bar.min_delta = bar.min_delta.min(bar.delta());
        bar.max_delta = bar.max_delta.max(bar.delta());
    }
}

/// Parts of chart bars: execution, bar open and allocated size.
#[cfg(feature = "widget")]
type Parts<'a> = Vec<(&'a Tick, DateTime<Utc>, f64)>;

/// Paged executions grouped along the chart's bars from one start.
#[cfg(feature = "widget")]
struct HistoryRun<'a> {
    /// The parts of the bars that match.
    parts: Parts<'a>,
    /// Whether they prove the start (see [`Footprint::push_history`]).
    proven: bool,
    /// Index of the first execution of the bar that did not match.
    next: usize,
    /// The grouping after the last execution, when the page ended with a matched bar.
    end: Option<super::inbox::TradeBars>,
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// Groups `trades` from their first execution along `history`, the bars from the one it opens.
/// `first` is the part of that execution in this bar, and `previous` the open of the bar before.
#[cfg(feature = "widget")]
fn history_run<'a>(
    timeframe: crate::model::Timeframe,
    tick_size: Option<f64>,
    trades: &[&'a Tick],
    history: &[crate::model::Bar],
    first: f64,
    previous: Option<DateTime<Utc>>,
) -> HistoryRun<'a> {
    use crate::model::Bar;
    let same = |grouped: &Bar, bar: Option<&Bar>| {
        bar.is_some_and(|bar| {
            grouped.time == bar.time
                && near(grouped.open, bar.open)
                && near(grouped.high, bar.high)
                && near(grouped.low, bar.low)
                && near(grouped.close, bar.close)
                && near(grouped.volume, bar.volume)
        })
    };
    let mut bars = super::inbox::TradeBars::default();
    // A first bar the sink opened right after the bar before opens there again.
    bars.forming = previous;
    let opening = (trades.first()).map(|trade| Tick {
        size: first,
        ..(**trade).clone()
    });
    let mut parts = Vec::new();
    let (mut kept, mut matched, mut opened) = (0, 0, 0);
    let mut forming: Option<Bar> = None;
    'trades: for (index, trade) in trades.iter().enumerate() {
        // Older than the one before it: left out, as `Footprint::push` does.
        let pushed = opening.as_ref().filter(|_| index == 0).unwrap_or(*trade);
        let Ok(allocated) = bars.push(timeframe, pushed, tick_size) else {
            continue;
        };
        for (bar, size) in allocated {
            if let Some(grouped) = forming.as_ref().filter(|grouped| grouped.time != bar.time) {
                if !same(grouped, history.get(matched)) {
                    forming = None;
                    break 'trades;
                }
                matched += 1;
                kept = parts.len();
                opened = index;
            }
            parts.push((*trade, bar.time, size));
            forming = Some(bar);
        }
    }
    // The page ends here, so a last bar that sums as grouped closes no earlier than it should.
    if forming.is_some_and(|grouped| same(&grouped, history.get(matched))) {
        return HistoryRun {
            parts,
            proven: true,
            next: trades.len(),
            end: Some(bars),
        };
    }
    parts.truncate(kept);
    HistoryRun {
        parts,
        proven: matched >= 2,
        next: opened,
        end: None,
    }
}

/// Groups the held live executions `trades`, in time order, along `history`, the chart's bars,
/// from the oldest bar whose parts are all held: the executions are the newest part of the sink's
/// stream and the chart's bars hold exactly its parts, so the volume held counts them from the
/// newest. Returns the parts and the grouping after them, or `None` unless they rebuild every bar
/// through the newest.
#[cfg(feature = "widget")]
pub(crate) fn live_run<'a>(
    timeframe: crate::model::Timeframe,
    tick_size: Option<f64>,
    trades: &[&'a Tick],
    history: &[crate::model::Bar],
) -> Option<(Parts<'a>, super::inbox::TradeBars)> {
    let mut left: f64 = trades.iter().map(|trade| trade.size).sum();
    let mut first = history.len();
    while let Some(bar) = first.checked_sub(1).map(|index| &history[index]) {
        if left < bar.volume && !near(left, bar.volume) {
            break;
        }
        left -= bar.volume;
        first -= 1;
    }
    // What is left belongs to the bars before; the next part opens the first bar.
    let mut through = 0.0;
    let start = trades.iter().position(|trade| {
        through += trade.size;
        through - left > 1e-9 * trade.size
    })?;
    let rest = (through - left).min(trades[start].size);
    let previous = first.checked_sub(1).map(|index| history[index].time);
    let run = history_run(
        timeframe,
        tick_size,
        &trades[start..],
        &history[first..],
        rest,
        previous,
    );
    let end = run.end?;
    (run.parts.last()?.1 == history.last()?.time).then_some((run.parts, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe;
    use chrono::TimeDelta;

    fn aggregate_bar(time: DateTime<Utc>) -> FootprintBar {
        FootprintBar {
            time,
            levels: BTreeMap::from([(
                200,
                FootprintLevel {
                    level: 200,
                    bid: 3.0,
                    ask: 8.0,
                    unknown: 2.0,
                    bid_trades: 2,
                    ask_trades: 3,
                    unknown_trades: 1,
                },
            )]),
            bid: 3.0,
            ask: 8.0,
            unknown: 2.0,
            min_delta: -2.0,
            max_delta: 6.0,
        }
    }

    fn ask_bar(time: DateTime<Utc>, ask: f64, ask_trades: u64) -> FootprintBar {
        FootprintBar {
            time,
            levels: BTreeMap::from([(
                200,
                FootprintLevel {
                    level: 200,
                    ask,
                    ask_trades,
                    ..Default::default()
                },
            )]),
            bid: 0.0,
            ask,
            unknown: 0.0,
            min_delta: 0.0,
            max_delta: ask,
        }
    }

    fn aggregate(bars: impl IntoIterator<Item = FootprintBar>) -> Result<Footprint, FeedError> {
        Footprint::from_bars(BarClock::new(Timeframe::Min1), 0.25, 2, bars)
    }

    #[test]
    fn aggregate_bars_preserve_values_and_order() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let first = aggregate_bar(start);
        let mut second = aggregate_bar(start + TimeDelta::minutes(1));
        (second.bid, second.ask, second.unknown) = (4.0, 1.0, 1.0);
        (second.min_delta, second.max_delta) = (-4.0, 0.0);
        let row = second.levels.get_mut(&200).unwrap();
        (row.bid, row.ask, row.unknown) = (4.0, 1.0, 1.0);
        (row.bid_trades, row.ask_trades, row.unknown_trades) = (2, 1, 1);
        let flow = aggregate([second.clone(), first.clone()]).unwrap();
        assert_eq!(
            flow.bars().values().cloned().collect::<Vec<_>>(),
            [first.clone(), second]
        );
        assert_eq!(flow.bar_at(start + TimeDelta::seconds(59)), Some(&first));
        let profile = flow.profile(start, start + TimeDelta::minutes(1));
        assert_eq!(
            (
                profile.bid,
                profile.ask,
                profile.unknown,
                profile.volume(),
                profile.trades()
            ),
            (7.0, 9.0, 3.0, 19.0, 10)
        );
        assert_eq!(
            (profile.delta(), profile.min_delta, profile.max_delta),
            (2.0, -2.0, 6.0)
        );
        assert_eq!(profile.average_size(), Some(1.9));
    }

    #[test]
    fn aggregate_bars_continue_with_new_executions() {
        use OrderSide::{Buy, Sell};
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let trades: Vec<_> = [
            (10, 100.0, 3.0, Some(Sell)),
            (20, 100.25, 8.0, Some(Buy)),
            (60, 100.5, 2.0, None),
            (70, 100.75, 4.0, Some(Sell)),
            (70, 100.5, 3.0, Some(Buy)),
            (80, 101.0, 5.0, None),
            (85, 100.75, 3.0, Some(Sell)),
            (90, 100.25, 7.0, Some(Buy)),
            (180, 101.0, 2.0, Some(Sell)),
        ]
        .into_iter()
        .map(|(second, price, size, side)| {
            Tick::new(start + TimeDelta::seconds(second), price, size, side)
        })
        .collect();
        let fresh = || Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 2).unwrap();
        let mut all = fresh();
        for trade in &trades {
            all.push(trade).unwrap();
        }
        for split in [0, 4, trades.len()] {
            let mut prefix = fresh();
            for trade in &trades[..split] {
                prefix.push(trade).unwrap();
            }
            let mut continued = aggregate(prefix.bars().values().cloned().rev()).unwrap();
            for trade in &trades[split..] {
                continued.push(trade).unwrap();
            }
            assert_eq!(continued.bars(), all.bars(), "split at execution {split}");
            assert_eq!(continued.complete_from(), Some(start));
            assert_eq!(
                continued.profile(start, trades.last().unwrap().ts),
                all.profile(start, trades.last().unwrap().ts)
            );
        }
        let mut imported = aggregate([aggregate_bar(start)]).unwrap();
        let before = imported.bars().clone();
        assert_eq!(
            imported
                .push(&Tick::new(start - TimeDelta::seconds(1), 100.0, 1.0, None))
                .unwrap_err()
                .to_string(),
            "invalid data: footprint trade at 2026-01-05 09:59:59 UTC precedes the last trade at 2026-01-05 10:00:00 UTC"
        );
        assert_eq!(
            imported.set_size_filter(0.0, 1.0).unwrap_err().to_string(),
            "invalid data: footprint size filter must be set before the first trade"
        );
        assert_eq!(imported.bars(), &before);
    }

    #[test]
    fn aggregate_bars_tolerate_rounding() {
        let start = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut decimal = aggregate_bar(start);
        (decimal.bid, decimal.ask, decimal.unknown) = (0.0, 0.3, 0.0);
        (decimal.min_delta, decimal.max_delta) = (0.0, 0.3);
        decimal.levels = [(200, 0.1), (201, 0.2)]
            .into_iter()
            .map(|(level, ask)| {
                (
                    level,
                    FootprintLevel {
                        level,
                        ask,
                        ask_trades: 1,
                        ..Default::default()
                    },
                )
            })
            .collect();
        assert_eq!(
            aggregate([decimal.clone()]).unwrap().bar_at(start),
            Some(&decimal)
        );
        let mut cancellation = aggregate_bar(start);
        (cancellation.bid, cancellation.ask, cancellation.unknown) =
            (50.12345677, 50.12345678, 0.0);
        (cancellation.min_delta, cancellation.max_delta) = (-cancellation.bid, 1e-8);
        let row = cancellation.levels.get_mut(&200).unwrap();
        (row.bid, row.ask, row.unknown) = (cancellation.bid, cancellation.ask, 0.0);
        (row.bid_trades, row.ask_trades, row.unknown_trades) = (1, 1, 0);
        assert!(cancellation.delta() > cancellation.max_delta);
        assert_eq!(
            aggregate([cancellation.clone()]).unwrap().bar_at(start),
            Some(&cancellation)
        );
    }

    #[test]
    fn aggregate_continuation_rejects_overflow_without_mutation() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut rounded = ask_bar(start, f64::MAX, 1);
        rounded.ask *= 1.0 - 5e-10;
        rounded.max_delta = rounded.ask;
        let size = f64::MAX * 4e-10;
        assert!((rounded.volume() + size).is_finite());
        assert!(!(rounded.levels[&200].volume() + size).is_finite());
        let mut flow = aggregate([rounded]).unwrap();
        let before = format!("{flow:?}");
        let trade = Tick::new(
            start + TimeDelta::seconds(1),
            100.0,
            size,
            Some(OrderSide::Buy),
        );
        assert_eq!(
            flow.push(&trade).unwrap_err().to_string(),
            "invalid data: footprint volume overflow"
        );
        assert_eq!(
            format!("{flow:?}"),
            before,
            "rejected execution changes no state"
        );
    }

    #[test]
    fn argument_errors_name_the_argument_and_its_value() {
        use crate::data::{
            DepthHistory, DepthHistoryConfig, MarketEvent, ReplayClock, ReplayEvent,
            aggregate_trades,
        };
        let t0: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let t1 = t0 + TimeDelta::seconds(1);
        let clock = || BarClock::new(Timeframe::Min1);
        let trade = |ts, price, size| Tick::new(ts, price, size, None);
        let base = DepthHistoryConfig::default();
        let depth = |config| DepthHistory::new(config).map(drop);
        let fresh = || Footprint::new(clock(), 1.0, 1).unwrap();
        let mut started = fresh();
        started.push(&trade(t1, 100.0, 1.0)).unwrap();
        let event = |ts| ReplayEvent::new("A", t0, 0, MarketEvent::Trade(trade(ts, 100.0, 1.0)));
        let invalid = |change: fn(&mut FootprintBar)| {
            let mut bar = aggregate_bar(t0);
            change(&mut bar);
            aggregate([bar]).map(drop)
        };
        #[rustfmt::skip]
        let cases = [
            (invalid(|b| b.time += TimeDelta::seconds(1)), "footprint bar at 2026-01-05 10:00:01 UTC does not open on the clock"),
            (aggregate([aggregate_bar(t0), aggregate_bar(t0)]).map(drop), "footprint bar at 2026-01-05 10:00:00 UTC is supplied more than once"),
            (invalid(|b| b.levels.clear()), "footprint bar at 2026-01-05 10:00:00 UTC has no price rows"),
            (invalid(|b| b.levels.get_mut(&200).unwrap().level = 201), "footprint bar at 2026-01-05 10:00:00 UTC row 200 has mismatched level 201"),
            (invalid(|b| { let mut row = b.levels.remove(&200).unwrap(); row.level = i64::MAX; b.levels.insert(row.level, row); }), "footprint bar at 2026-01-05 10:00:00 UTC row 9223372036854775807 is outside the grid of step 0.5"),
            (invalid(|b| { let mut row = b.levels.remove(&200).unwrap(); row.level = (1_i64 << 53) + 1; b.levels.insert(row.level, row); }), "footprint bar at 2026-01-05 10:00:00 UTC row 9007199254740993 is outside the grid of step 0.5"),
            (Footprint::from_bars(clock(), f64::MAX, 1, [aggregate_bar(t0)]).map(drop), "footprint bar at 2026-01-05 10:00:00 UTC row 200 is outside the grid of step 1.7976931348623157e308"),
            (invalid(|b| b.levels.get_mut(&200).unwrap().bid = -1.0), "footprint bar at 2026-01-05 10:00:00 UTC row 200 bid volume must be at least 0, got -1.0"),
            (invalid(|b| b.levels.get_mut(&200).unwrap().ask = f64::NAN), "footprint bar at 2026-01-05 10:00:00 UTC row 200 ask volume must be finite, got NaN"),
            (invalid(|b| b.levels.get_mut(&200).unwrap().unknown_trades = 0), "footprint bar at 2026-01-05 10:00:00 UTC row 200 unknown volume 2.0 has no executions"),
            (invalid(|b| b.levels.get_mut(&200).unwrap().unknown = 0.0), "footprint bar at 2026-01-05 10:00:00 UTC row 200 unknown count 1 has no volume"),
            (invalid(|b| { b.levels.insert(200, FootprintLevel { level: 200, ..Default::default() }); }), "footprint bar at 2026-01-05 10:00:00 UTC row 200 has no executed volume"),
            (invalid(|b| { (b.bid, b.ask, b.unknown) = (f64::MAX * 0.6, f64::MAX * 0.4, 0.0); assert!(b.volume().is_finite()); let row = b.levels.get_mut(&200).unwrap(); (row.bid, row.ask, row.unknown, row.unknown_trades) = (b.bid * (1.0 + 5e-10), b.ask * (1.0 + 5e-10), 0.0, 0); }), "footprint bar at 2026-01-05 10:00:00 UTC row 200 volume must be finite, got inf"),
            (aggregate([ask_bar(t0, 1.0, u64::MAX / 2), ask_bar(t0 + TimeDelta::minutes(1), 1.0, 1)]).map(drop), "footprint bar at 2026-01-05 10:01:00 UTC row 200 ask count 1 takes the snapshot past 9223372036854775807 executions"),
            (invalid(|b| b.bid = -1.0), "footprint bar at 2026-01-05 10:00:00 UTC bid volume must be at least 0, got -1.0"),
            (invalid(|b| b.ask = f64::INFINITY), "footprint bar at 2026-01-05 10:00:00 UTC ask volume must be finite, got inf"),
            (invalid(|b| b.bid = 4.0), "footprint bar at 2026-01-05 10:00:00 UTC bid volume 4.0 differs from row total 3.0"),
            (invalid(|b| b.ask += 1e-6), "footprint bar at 2026-01-05 10:00:00 UTC ask volume 8.000001 differs from row total 8.0"),
            (invalid(|b| { b.levels = [200, 201].into_iter().map(|level| (level, FootprintLevel { level, ask: 1e308, ask_trades: 1, ..Default::default() })).collect(); (b.bid, b.ask, b.unknown) = (0.0, 1e308, 0.0); }), "footprint bar at 2026-01-05 10:00:00 UTC sum of row ask volumes must be finite, got inf"),
            (invalid(|b| { b.levels = BTreeMap::from([(200, FootprintLevel { level: 200, bid: 1e308, bid_trades: 1, ..Default::default() }), (201, FootprintLevel { level: 201, ask: 1e308, ask_trades: 1, ..Default::default() })]); (b.bid, b.ask, b.unknown) = (1e308, 1e308, 0.0); }), "footprint bar at 2026-01-05 10:00:00 UTC volume must be finite, got inf"),
            (invalid(|b| b.min_delta = f64::NAN), "footprint bar at 2026-01-05 10:00:00 UTC min_delta must be finite, got NaN"),
            (invalid(|b| b.max_delta = f64::INFINITY), "footprint bar at 2026-01-05 10:00:00 UTC max_delta must be finite, got inf"),
            (invalid(|b| b.min_delta = 1.0), "footprint bar at 2026-01-05 10:00:00 UTC min_delta must be at most 0, got 1.0"),
            (invalid(|b| b.max_delta = -1.0), "footprint bar at 2026-01-05 10:00:00 UTC max_delta must be at least 0, got -1.0"),
            (invalid(|b| { (b.bid, b.ask, b.min_delta) = (8.0, 3.0, -4.0); let row = b.levels.get_mut(&200).unwrap(); (row.bid, row.ask) = (8.0, 3.0); }), "footprint bar at 2026-01-05 10:00:00 UTC min_delta -4.0 exceeds final delta -5.0"),
            (invalid(|b| b.max_delta = 4.9999999), "footprint bar at 2026-01-05 10:00:00 UTC max_delta 4.9999999 is below final delta 5.0"),
            (invalid(|b| b.min_delta = -4.0), "footprint bar at 2026-01-05 10:00:00 UTC delta span 10.0 exceeds side volume 8.0"),
            (invalid(|b| { b.min_delta = -3.0; b.max_delta = 8.0; }), "footprint bar at 2026-01-05 10:00:00 UTC delta span 11.0 exceeds side volume 8.0"),
            (Footprint::from_bars(BarClock::new("3t".parse().unwrap()), 0.25, 2, []).map(drop), "footprint aggregate snapshots require a time-based clock, got 3t"),

            (depth(DepthHistoryConfig { cadence: TimeDelta::zero(), ..base.clone() }), "depth history cadence must be at least one millisecond, got P0D"),
            (depth(DepthHistoryConfig { rows: 0, ..base.clone() }), "depth history rows must be at least 1, got 0"),
            (depth(DepthHistoryConfig { ticks_per_row: 0, ..base.clone() }), "depth history ticks_per_row must be at least 1, got 0"),
            (depth(DepthHistoryConfig { columns: 1, ..base.clone() }), "depth history columns must be at least 2, got 1"),
            (Footprint::new(clock(), f64::NAN, 1).map(drop), "footprint tick_size must be finite and above zero, got NaN"),
            (Footprint::new(clock(), 1.0, 0).map(drop), "footprint ticks_per_row must be at least 1, got 0"),
            (Footprint::new(clock(), 1e300, u32::MAX).map(drop), "footprint price step of tick_size 1e300 times ticks_per_row 4294967295 is not finite"),
            (started.clone().set_size_filter(0.0, 1.0), "footprint size filter must be set before the first trade"),
            (fresh().set_size_filter(-1.0, 1.0), "footprint size filter min must be finite and at least 0, got -1.0"),
            (fresh().set_size_filter(0.0, f64::NAN), "footprint size filter max must be finite, got NaN"),
            (fresh().set_size_filter(5.0, 1.0), "footprint size filter max 1.0 is below min 5.0"),
            (started.clone().push(&trade(t1, f64::NAN, 1.0)).map(drop), "footprint trade price must be finite, got NaN"),
            (started.clone().push(&trade(t1, 100.0, -1.0)).map(drop), "footprint trade size must be finite and at least 0, got -1.0"),
            (Footprint::new(clock(), 1e-300, u32::MAX).unwrap().push(&trade(t1, 1e6, 1.0)).map(drop), "footprint trade price 1000000.0 is outside the grid of step 4.294967295e-291"),
            (started.clone().push(&trade(t0, 100.0, 1.0)).map(drop), "footprint trade at 2026-01-05 10:00:00 UTC precedes the last trade at 2026-01-05 10:00:01 UTC"),
            (ReplayClock::new(vec![event(t1)]).map(drop), "replay event of \"A\" at 2026-01-05 10:00:00 UTC with sequence 0 is invalid"),
            (ReplayClock::new(vec![event(t0), event(t0)]).map(drop), "replay events of \"A\" at 2026-01-05 10:00:00 UTC share sequence 0"),
            (ReplayClock::new(Vec::new()).unwrap().set_speed(-1.0), "replay speed must be finite and at least 0, got -1.0"),
            (aggregate_trades(&[trade(t0, 100.0, 1.0), trade(t1, f64::NAN, 1.0)], Timeframe::Min1).map(drop), "trade 1 price must be finite, got NaN"),
            (aggregate_trades(&[trade(t0, 100.0, 1.0), trade(t1, 100.0, -1.0)], Timeframe::Min1).map(drop), "trade 1 size must be finite and at least 0, got -1.0"),
            (aggregate_trades(&[trade(t1, 100.0, 1.0), trade(t0, 100.0, 1.0)], Timeframe::Min1).map(drop), "trade 1 at 2026-01-05 10:00:00 UTC is out of time order"),
        ];
        for (result, expected) in cases {
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("invalid data: {expected}")
            );
        }
    }

    /// Hand allocations: count and range bars close after the second fill; a volume bar
    /// takes 3 of the sell of 6, and a delta bar takes 7 of the sell of 8 after a buy of 2.
    #[test]
    fn trade_intervals_keep_exact_fill_allocations_and_equal_timestamp_bars() {
        use OrderSide::{Buy, Sell};
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let cases = [
            (
                "2t",
                vec![(100.0, 2.0, Buy), (101.0, 3.0, Sell), (102.0, 4.0, Buy)],
                vec![(5.0, -1.0, -1.0, 2.0), (4.0, 4.0, 0.0, 4.0)],
            ),
            (
                "5v",
                vec![(100.0, 2.0, Buy), (101.0, 6.0, Sell)],
                vec![(5.0, -1.0, -1.0, 2.0), (3.0, -3.0, -3.0, 0.0)],
            ),
            (
                "2r",
                vec![(100.0, 2.0, Buy), (102.0, 3.0, Sell), (101.0, 4.0, Buy)],
                vec![(5.0, -1.0, -1.0, 2.0), (4.0, 4.0, 0.0, 4.0)],
            ),
            (
                "5delta",
                vec![(100.0, 2.0, Buy), (101.0, 8.0, Sell)],
                vec![(9.0, -5.0, -5.0, 2.0), (1.0, -1.0, -1.0, 0.0)],
            ),
        ];
        for (interval, fills, expected) in cases {
            let clock = BarClock::new(interval.parse().unwrap());
            let mut flow = Footprint::new(clock.clone(), 1.0, 1).unwrap();
            let trades: Vec<_> = fills
                .into_iter()
                .map(|(price, size, side)| Tick::new(start, price, size, Some(side)))
                .collect();
            for trade in &trades {
                flow.push(trade).unwrap();
            }
            let source = super::super::OrderFlowInput::footprint();
            source.set_tick_size(Some(1.0));
            for trade in &trades {
                source.push_trade(&clock, trade);
            }
            assert_eq!(
                source.footprints_from(start).bars,
                *flow.bars(),
                "{interval}: standalone source"
            );
            let got: Vec<_> = flow
                .bars()
                .values()
                .map(|b| (b.volume(), b.delta(), b.min_delta, b.max_delta))
                .collect();
            assert_eq!(got, expected, "{interval}");
            let candles =
                super::super::aggregate_trades_with_tick_size(&trades, clock, Some(1.0)).unwrap();
            for (bar, candle) in flow.bars().values().zip(&candles) {
                assert_eq!(
                    (bar.time, bar.volume()),
                    (candle.time, candle.volume),
                    "{interval}"
                );
                assert_eq!(flow.bar_at(candle.time), Some(bar));
            }
            assert_eq!(flow.bars().len(), candles.len());
            let profile = flow.profile(start, start + TimeDelta::nanoseconds(1));
            assert_eq!(profile.volume(), trades.iter().map(|t| t.size).sum::<f64>());
            // Rebuilds use the new interval rather than the previous bar's timestamp slots.
            let mut minute = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
            for trade in &trades {
                minute.push(trade).unwrap();
            }
            assert_eq!(minute.bars().len(), 1);
            let rebuilt = &minute.bars()[&start];
            let uncounted = |bar: &FootprintBar| {
                let mut bar = bar.clone();
                for row in bar.levels.values_mut() {
                    (row.bid_trades, row.ask_trades, row.unknown_trades) = (0, 0, 0);
                }
                bar
            };
            assert_eq!(uncounted(rebuilt), uncounted(&profile));
            assert_eq!(rebuilt.trades(), trades.len() as u64);
            // A split execution contributes once to each of the two bars it reaches.
            assert_eq!(
                flow.bars()
                    .values()
                    .map(FootprintBar::trades)
                    .collect::<Vec<_>>(),
                [2, 1],
                "{interval}: per-bar executions"
            );
            assert_eq!(profile.trades(), 3);
        }
        for (interval, min_size, fills, expected) in [
            (
                "5v",
                3.0,
                vec![(2.0, Buy), (6.0, Sell)],
                vec![(3.0, -3.0), (3.0, -3.0)],
            ),
            (
                "2t",
                3.0,
                vec![(2.0, Buy), (3.0, Sell), (4.0, Buy)],
                vec![(3.0, -3.0), (4.0, 4.0)],
            ),
            (
                "2t",
                0.0,
                vec![(0.0, Buy), (2.0, Buy), (3.0, Sell)],
                vec![(2.0, 2.0), (3.0, -3.0)],
            ),
        ] {
            let mut flow =
                Footprint::new(BarClock::new(interval.parse().unwrap()), 1.0, 1).unwrap();
            flow.set_size_filter(min_size, 10.0).unwrap();
            for (i, (size, side)) in fills.into_iter().enumerate() {
                flow.push(&Tick::new(start, 100.0, size, Some(side)))
                    .unwrap();
                if i == 0 {
                    assert!(
                        flow.set_size_filter(min_size, 10.0).is_err(),
                        "the grouping anchor already consumed this execution"
                    );
                }
            }
            let got: Vec<_> = flow
                .bars()
                .values()
                .map(|bar| (bar.volume(), bar.delta()))
                .collect();
            assert_eq!(
                got, expected,
                "{interval}: original size filter and zero-size count"
            );
        }
    }

    /// Sizes of 0.1 and 0.2 do not sum exactly to their threshold, so a fill that reaches the
    /// threshold must close the bar whole. A fill that crosses it by more than the rounding is
    /// still split.
    #[test]
    fn fractional_fills_that_reach_the_threshold_close_the_bar() {
        use OrderSide::Buy;
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |seconds: i64| start + TimeDelta::seconds(seconds);
        let fill = |seconds, price, size| Tick::new(at(seconds), price, size, Some(Buy));
        let summary = |flow: &Footprint| {
            flow.bars()
                .values()
                .map(|bar| (bar.trades(), bar.levels.keys().copied().collect::<Vec<_>>()))
                .collect::<Vec<_>>()
        };
        for interval in ["1v", "1delta"] {
            let clock = || BarClock::new(interval.parse().unwrap());
            let mut ten = Footprint::new(clock(), 1.0, 1).unwrap();
            for second in 0..10 {
                ten.push(&fill(second, 100.0, 0.1)).unwrap();
            }
            ten.push(&fill(10, 101.0, 0.1)).unwrap();
            assert_eq!(
                summary(&ten),
                [(10, vec![100]), (1, vec![101])],
                "{interval}: ten buys of 0.1"
            );
            let mut stepped = Footprint::new(clock(), 1.0, 1).unwrap();
            for (second, (price, size)) in [
                (100.0, 0.1),
                (101.0, 0.2),
                (102.0, 0.3),
                (103.0, 0.4),
                (104.0, 0.5),
            ]
            .into_iter()
            .enumerate()
            {
                stepped.push(&fill(second as i64, price, size)).unwrap();
            }
            assert_eq!(
                summary(&stepped),
                [(4, vec![100, 101, 102, 103]), (1, vec![104])],
                "{interval}: sizes of 0.1 to 0.5"
            );
            let mut crossing = Footprint::new(clock(), 1.0, 1).unwrap();
            crossing.push(&fill(0, 100.0, 0.6)).unwrap();
            crossing.push(&fill(1, 101.0, 0.6)).unwrap();
            assert_eq!(
                summary(&crossing),
                [(2, vec![100, 101]), (1, vec![101])],
                "{interval}: a fill crossing by more than the rounding is split"
            );
        }
    }

    /// 2 + 6 + 4 + 8 = 20 from four executions, so the mean is 5. A size filter from 5
    /// upward retains 6 and 8: volume 14, two executions, mean 7.
    #[test]
    fn execution_counts_follow_sides_filters_and_grouping() {
        let start = "2026-01-05T10:00:00Z".parse().unwrap();
        let fills = [
            Tick::new(start, 100.0, 2.0, Some(OrderSide::Buy)),
            Tick::new(start, 100.0, 6.0, Some(OrderSide::Buy)),
            Tick::new(start, 100.0, 4.0, Some(OrderSide::Sell)),
            Tick::new(start, 100.0, 8.0, None),
            Tick::new(start, 100.0, 0.0, None),
        ];
        for (min, max, sides, volume, average) in [
            (0.0, f64::MAX, (1, 2, 1), 20.0, Some(5.0)),
            (5.0, f64::MAX, (0, 1, 1), 14.0, Some(7.0)),
            (2.0, 4.0, (1, 1, 0), 6.0, Some(3.0)),
            (9.0, f64::MAX, (0, 0, 0), 0.0, None),
        ] {
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
            flow.set_size_filter(min, max).unwrap();
            for trade in &fills {
                flow.push(trade).unwrap();
            }
            let bar = flow.profile(start, start);
            let row = bar.levels.get(&100).cloned().unwrap_or_default();
            assert_eq!((row.bid_trades, row.ask_trades, row.unknown_trades), sides);
            assert_eq!((row.volume(), row.average_size()), (volume, average));
            assert_eq!((bar.trades(), bar.average_size()), (row.trades(), average));
        }
        let mut grouped = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 2).unwrap();
        for (price, size, side) in [
            (100.0, 2.0, Some(OrderSide::Sell)),
            (101.0, 6.0, Some(OrderSide::Buy)),
            (101.0, 4.0, None),
        ] {
            grouped.push(&Tick::new(start, price, size, side)).unwrap();
        }
        let bar = &grouped.bars()[&start];
        assert_eq!(bar.levels.len(), 1);
        assert_eq!((bar.trades(), bar.average_size()), (3, Some(4.0)));
        assert_eq!(bar.levels[&50].trades(), 3);
        assert_eq!(FootprintLevel::default().average_size(), None);
        assert_eq!(FootprintBar::empty(start).average_size(), None);
    }

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
        assert_eq!(bar.imbalance(401, 1.5, true, 0.0), (false, true));
        assert_eq!(bar.imbalance(403, 1.5, true, 0.0), (false, true));
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

    /// Dropped executions leave the bar summed again from those held, extrema included, and
    /// the budget drops whole old bars; both leave earlier bars incomplete.
    #[test]
    fn dropped_executions_match_the_held_ones_summed_again() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let sells = [
            Some(OrderSide::Sell),
            Some(OrderSide::Buy),
            Some(OrderSide::Buy),
        ];
        // Running delta of the whole bar: 0, -1, 0, 1, 0, 1, 2; without the first two trades, it
        // starts at zero again: 0, 1, 0, 1, 2.
        let trades: Vec<_> = (0..7)
            .map(|i| {
                Tick::new(
                    start + TimeDelta::seconds(i),
                    100.0 + i as f64,
                    1.0,
                    sells[i as usize % 3],
                )
            })
            .collect();
        let new = || Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        let mut flow = new();
        trades.iter().for_each(|t| _ = flow.push(t));
        flow.drop_through(trades[1].ts, &trades[2..]);
        let mut held = new();
        trades[2..].iter().for_each(|t| _ = held.push(t));
        assert_eq!(flow.bars(), held.bars());
        assert_eq!(flow.bars()[&start].trades(), 5);
        assert_eq!(
            (flow.bars()[&start].min_delta, flow.bars()[&start].max_delta),
            (0.0, 2.0)
        );
        assert_eq!(flow.complete_from(), None);

        let mut budget = new();
        budget.set_budget(2, usize::MAX);
        for minute in 0..3 {
            let at = start + TimeDelta::minutes(minute);
            budget.push(&Tick::new(at, 100.0, 1.0, None)).unwrap();
        }
        assert_eq!(budget.bars().len(), 2);
        assert_eq!(budget.complete_from(), Some(start + TimeDelta::minutes(1)));
    }
}
