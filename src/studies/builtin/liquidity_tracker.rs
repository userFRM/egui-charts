//! # Liquidity Tracker
//!
//! Accumulates bar volume at price levels to find heavily traded prices, the
//! bars that trade through them (sweeps) and clusters of them (zones). Bars
//! carry no order book or trade data, so levels stand in for resting
//! liquidity, estimated from bars.

use super::{clamp_param, window_volume};
use crate::model::Bar;
use crate::model::bar::{GrowingGrid, bar_level_keys, level_key};
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::RangeInclusive;

/// Liquidity Tracker, estimated from bars.
///
/// Each bar's volume is spread evenly over the price levels between its low and
/// high. A level holds the volume the last `period` bars spread on it, and is
/// heavy once it holds `volume_threshold` times their average bar volume. The
/// value, plotted in its own pane, is the largest level volume after each bar
/// in multiples of that average. [`sweeps`](Self::sweeps) and
/// [`zones`](Self::zones) give the heavy levels each bar traded through and
/// the clusters of heavy levels after the last bar.
///
/// Levels lie on the configured tick size (see
/// [`with_tick_size`](Self::with_tick_size)), widened to the tick times a
/// power of two once the bars up to the current one span more than 1,000
/// ticks, so they cover at most about 1,000 levels; level, zone and sweep
/// prices are multiples of that grid. Each value and sweep uses only its bar
/// and the bars before it, so appending bars never changes earlier ones. Bars
/// with a non-finite price or an inverted range are not tracked: they add no
/// volume.
#[derive(Clone)]
pub struct LiquidityTracker {
    /// Period for analysis
    period: usize,
    /// Instrument tick size; zero when not configured
    tick_size: f64,
    /// Grid the last calculation keyed levels by: `tick_size`, widened to bound the level count
    grid: f64,
    /// Minimum volume threshold for significant level
    volume_threshold: f64,
    /// Calculated values
    values: Vec<IndicatorValue>,
    /// Color for high liquidity levels
    high_liq_color: Color32,
    /// Color for liquidity sweeps
    sweep_color: Color32,
    /// Visibility
    visible: bool,
    /// Current liquidity map
    liquidity_map: HashMap<i64, LiquidityLevel>,
    /// Detected liquidity zones
    zones: Vec<LiquidityZone>,
    /// Detected sweeps
    sweeps: Vec<LiquiditySweep>,
}

/// Liquidity at a price level
#[derive(Debug, Clone)]
pub struct LiquidityLevel {
    /// Price level
    pub price: f64,
    /// Accumulated volume
    pub volume: f64,
    /// Touch count (how many bars touched this level)
    pub touches: usize,
    /// Last bar index that touched
    pub last_touch: usize,
    /// Is this an untested high/low
    pub is_swing_level: bool,
    /// Side bias (positive = more buys, negative = more sells)
    pub side_bias: f64,
}

/// A zone of concentrated liquidity
#[derive(Debug, Clone)]
pub struct LiquidityZone {
    /// Zone start (lower price)
    pub price_low: f64,
    /// Zone end (higher price)
    pub price_high: f64,
    /// Total volume in zone
    pub total_volume: f64,
    /// Average touches per level
    pub avg_touches: f64,
    /// Zone type
    pub zone_type: LiquidityZoneType,
    /// First bar index
    pub start_idx: usize,
    /// Last bar index
    pub end_idx: usize,
}

/// Type of liquidity zone
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidityZoneType {
    /// Accumulation zone (buying pressure)
    Accumulation,
    /// Distribution zone (selling pressure)
    Distribution,
    /// Neutral/balanced
    Neutral,
    /// Swing high liquidity
    SwingHigh,
    /// Swing low liquidity
    SwingLow,
}

/// The heavy levels one bar traded through on one side of the level of its open
#[derive(Debug, Clone, PartialEq)]
pub struct LiquiditySweep {
    /// Bar index where sweep occurred
    pub bar_idx: usize,
    /// Lowest swept level
    pub price_low: f64,
    /// Highest swept level
    pub price_high: f64,
    /// Volume the swept levels held before the bar
    pub swept_volume: f64,
    /// Side of the open the swept levels lie on
    pub direction: SweepDirection,
    /// Whether the bar closed back beyond the swept levels: above them after
    /// sweeping lows, below them after sweeping highs
    pub is_clean: bool,
}

/// Direction of liquidity sweep
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepDirection {
    /// Levels below the level of the bar's open, which it traded down through
    SweepLows,
    /// Levels above the level of the bar's open, which it traded up through
    SweepHighs,
}

impl LiquidityTracker {
    /// Create a new liquidity tracker
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            tick_size: 0.0,
            grid: 1.0,
            volume_threshold: 2.0, // Multiple of average
            values: Vec::new(),
            high_liq_color: Color32::from_rgba_unmultiplied(255, 215, 0, 128), // Gold
            sweep_color: Color32::from_rgba_unmultiplied(255, 0, 255, 180),    // Magenta
            visible: true,
            liquidity_map: HashMap::default(),
            zones: Vec::new(),
            sweeps: Vec::new(),
        }
    }

    /// Set the instrument's tick size, which levels are keyed on, clamped into
    /// 0 to `f64::MAX`, NaN to 0. With zero (the default), levels divide the
    /// price span of the bars so far into about 1,000 levels instead, rounded
    /// to a power of two.
    pub fn with_tick_size(mut self, tick_size: f64) -> Self {
        self.tick_size = clamp_param(tick_size, 0.0, f64::MAX);
        self.grid = self.tick_size;
        self
    }

    /// Set the volume threshold, as a multiple of the average bar volume,
    /// clamped into 0 to `f64::MAX`, NaN to 0
    pub fn with_volume_threshold(mut self, threshold: f64) -> Self {
        self.volume_threshold = clamp_param(threshold, 0.0, f64::MAX);
        self
    }

    /// Set colors
    pub fn with_colors(mut self, high_liq: Color32, sweep: Color32) -> Self {
        self.high_liq_color = high_liq;
        self.sweep_color = sweep;
        self
    }

    /// Get liquidity zones
    pub fn zones(&self) -> &[LiquidityZone] {
        &self.zones
    }

    /// Get liquidity sweeps
    pub fn sweeps(&self) -> &[LiquiditySweep] {
        &self.sweeps
    }

    /// Get liquidity at a specific price
    pub fn liquidity_at(&self, price: f64) -> Option<&LiquidityLevel> {
        let key = self.price_to_key(price);
        self.liquidity_map.get(&key)
    }

    /// Get significant liquidity levels
    pub fn significant_levels(&self, avg_volume: f64) -> Vec<&LiquidityLevel> {
        let threshold = avg_volume * self.volume_threshold;
        self.liquidity_map
            .values()
            .filter(|l| l.volume >= threshold)
            .collect()
    }

    fn price_to_key(&self, price: f64) -> i64 {
        level_key(price, self.grid)
    }

    fn key_to_price(&self, key: i64) -> f64 {
        key as f64 * self.grid
    }

    /// Add volume to a price level
    fn add_volume(&mut self, key: i64, volume: f64, bar_idx: usize, is_buy: bool) {
        let price_at_key = self.key_to_price(key);

        let level = self
            .liquidity_map
            .entry(key)
            .or_insert_with(|| LiquidityLevel {
                price: price_at_key,
                volume: 0.0,
                touches: 0,
                last_touch: bar_idx,
                is_swing_level: false,
                side_bias: 0.0,
            });

        level.volume += volume;
        level.touches += 1;
        level.last_touch = bar_idx;
        level.side_bias += if is_buy { volume } else { -volume };
    }

    /// The levels a tracked bar (see the type docs) spreads its volume on, as its
    /// first and last key and the volume per level: its range, or the level of
    /// its close when the range is empty.
    fn spread(&self, bar: &Bar) -> Option<(i64, i64, f64)> {
        let (low, high) = bar_level_keys(bar, self.grid)?;
        if bar.high - bar.low > 0.0 {
            Some((low, high, bar.volume / (high.abs_diff(low) + 1) as f64))
        } else {
            let close = self.price_to_key(bar.close);
            bar.close.is_finite().then_some((close, close, bar.volume))
        }
    }

    /// Add the volume `bar` spreads on its levels.
    fn add_bar(&mut self, bar_idx: usize, bar: &Bar) {
        if let Some((low_key, high_key, volume)) = self.spread(bar) {
            for key in low_key..=high_key {
                self.add_volume(key, volume, bar_idx, bar.close >= bar.open);
            }
        }
    }

    /// Take back the volume `bar` spread on its levels, dropping the levels no
    /// bar touches any more.
    fn remove_bar(&mut self, bar: &Bar) {
        let Some((low_key, high_key, volume)) = self.spread(bar) else {
            return;
        };
        let bias = if bar.close >= bar.open {
            volume
        } else {
            -volume
        };
        for key in low_key..=high_key {
            if let Entry::Occupied(mut entry) = self.liquidity_map.entry(key) {
                let level = entry.get_mut();
                level.touches = level.touches.saturating_sub(1);
                level.volume -= volume;
                level.side_bias -= bias;
                if level.touches == 0 {
                    entry.remove();
                }
            }
        }
    }

    /// Record the heavy levels among `keys` that `bar` traded through, before its
    /// own volume is added: one sweep for those below the level of its open, one
    /// for those above it.
    fn record_sweeps(
        &mut self,
        bar: &Bar,
        bar_idx: usize,
        keys: RangeInclusive<i64>,
        threshold: f64,
    ) {
        if !bar.open.is_finite() {
            return;
        }
        let open_key = self.price_to_key(bar.open);
        // (price_low, price_high, volume) of the swept levels below and above the open
        let mut sides = [(f64::MAX, f64::MIN, 0.0); 2];
        for key in keys {
            let Some(level) = self
                .liquidity_map
                .get(&key)
                .filter(|l| l.volume >= threshold)
            else {
                continue;
            };
            let side = match key.cmp(&open_key) {
                Ordering::Less => &mut sides[0],
                Ordering::Greater => &mut sides[1],
                Ordering::Equal => continue,
            };
            *side = (
                side.0.min(level.price),
                side.1.max(level.price),
                side.2 + level.volume,
            );
        }
        let [lows, highs] = sides;
        for ((price_low, price_high, swept_volume), direction, is_clean) in [
            (lows, SweepDirection::SweepLows, bar.close > lows.1),
            (highs, SweepDirection::SweepHighs, bar.close < highs.0),
        ] {
            if price_low <= price_high {
                self.sweeps.push(LiquiditySweep {
                    bar_idx,
                    price_low,
                    price_high,
                    swept_volume,
                    direction,
                    is_clean,
                });
            }
        }
    }

    /// Identify liquidity zones (clusters of significant levels)
    fn identify_zones(&mut self, avg_volume: f64) {
        self.zones.clear();

        let threshold = avg_volume * self.volume_threshold;
        let mut significant: Vec<_> = self
            .liquidity_map
            .values()
            .filter(|l| l.volume >= threshold)
            .cloned()
            .collect();

        significant.sort_by(|a, b| a.price.total_cmp(&b.price));

        if significant.is_empty() {
            return;
        }

        // Group adjacent levels into zones
        let mut current_zone_start = 0;
        let zone_gap = self.grid * 3.0; // Max gap between levels in a zone

        for i in 1..significant.len() {
            let gap = significant[i].price - significant[i - 1].price;

            if gap > zone_gap {
                // Create zone from current_zone_start to i-1
                self.create_zone(&significant[current_zone_start..i]);
                current_zone_start = i;
            }
        }

        // Create final zone
        self.create_zone(&significant[current_zone_start..]);
    }

    fn create_zone(&mut self, levels: &[LiquidityLevel]) {
        if levels.is_empty() {
            return;
        }

        let price_low = levels.iter().map(|l| l.price).fold(f64::MAX, f64::min);
        let price_high = levels.iter().map(|l| l.price).fold(f64::MIN, f64::max);
        let total_volume: f64 = levels.iter().map(|l| l.volume).sum();
        let avg_touches =
            levels.iter().map(|l| l.touches).sum::<usize>() as f64 / levels.len() as f64;
        let total_bias: f64 = levels.iter().map(|l| l.side_bias).sum();

        let zone_type = if total_bias > total_volume * 0.3 {
            LiquidityZoneType::Accumulation
        } else if total_bias < -total_volume * 0.3 {
            LiquidityZoneType::Distribution
        } else {
            LiquidityZoneType::Neutral
        };

        let start_idx = levels.iter().map(|l| l.last_touch).min().unwrap_or(0);
        let end_idx = levels.iter().map(|l| l.last_touch).max().unwrap_or(0);

        self.zones.push(LiquidityZone {
            price_low,
            price_high,
            total_volume,
            avg_touches,
            zone_type,
            start_idx,
            end_idx,
        });
    }
}

/// Construct with the conventional default parameters.
impl Default for LiquidityTracker {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for LiquidityTracker {
    fn name(&self) -> &str {
        "Liquidity"
    }

    fn desc(&self) -> &str {
        "Liquidity Tracker - Volume accumulated at price levels, estimated from bars"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        self.liquidity_map.clear();
        self.zones.clear();
        self.sweeps.clear();

        let mut grid = GrowingGrid::new(self.tick_size);
        let mut avg_volume = 0.0;
        let mut volume = 0.0;
        for (idx, bar) in data.iter().enumerate() {
            // The levels hold the `period` bars before this one, keyed again when the grid changes
            let regrid = grid.push(bar);
            self.grid = grid.grid();
            if regrid {
                self.liquidity_map.clear();
                for i in idx.saturating_sub(self.period)..idx {
                    self.add_bar(i, &data[i]);
                }
            }
            volume = window_volume(data, idx, self.period, volume);
            avg_volume = volume / (idx + 1).min(self.period) as f64;

            if bar.high - bar.low > 0.0
                && let Some((low_key, high_key, _)) = self.spread(bar)
            {
                let threshold = avg_volume * self.volume_threshold;
                self.record_sweeps(bar, idx, low_key..=high_key, threshold);
            }
            if let Some(leaving) = idx.checked_sub(self.period) {
                self.remove_bar(&data[leaving]);
            }
            self.add_bar(idx, bar);

            // The value for this bar: the largest level volume over the last `period` bars
            let max_liq = self
                .liquidity_map
                .values()
                .fold(0.0_f64, |max, level| max.max(level.volume));

            let liquidity_ratio = if avg_volume > 0.0 {
                max_liq / avg_volume
            } else {
                0.0
            };

            self.values.push(IndicatorValue::Single(liquidity_ratio));
        }

        // Identify liquidity zones
        self.identify_zones(avg_volume);
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.high_liq_color, self.sweep_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.high_liq_color = colors[0];
        }
        if colors.len() > 1 {
            self.sweep_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Liquidity".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// A level holds the volume of the last `period` bars only, so the ratio
    /// stops growing once the window is full while price stays in a range.
    #[test]
    fn level_volume_is_limited_to_the_window() {
        let bar = Bar {
            time: Utc::now(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 30.0,
        };
        let mut tracker = LiquidityTracker::new(5).with_tick_size(1.0);
        tracker.calculate(&vec![bar; 12]);
        // Each bar puts 10 on each of 99, 100 and 101; the average bar volume is 30
        for (i, value) in tracker.values().iter().enumerate() {
            let expected = (i + 1).min(5) as f64 * 10.0 / 30.0;
            assert!(
                matches!(value, IndicatorValue::Single(v) if (v - expected).abs() < 1e-9),
                "bar {i}: {value:?}, expected {expected}"
            );
        }
        assert_eq!(tracker.values().len(), 12);
    }

    /// A bar sweeps the heavy levels it traded through, one record per side.
    #[test]
    fn sweeps_are_the_heavy_levels_inside_the_bar() {
        let bar = |open, high, low, close, volume| Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume,
        };
        let bars = [
            // 100 at each of 100..=104
            bar(100.0, 104.0, 100.0, 102.0, 500.0),
            // Trades 98..=106 from an open of 102 and closes at 103.5
            bar(102.0, 106.0, 98.0, 103.5, 900.0),
            // Opens at its low inside level 100 and rallies to 103
            bar(100.4, 103.0, 100.4, 103.0, 700.0),
        ];
        // Average volume 700, so levels of 70 or more are heavy
        let mut tracker = LiquidityTracker::new(10)
            .with_tick_size(1.0)
            .with_volume_threshold(0.1);
        tracker.calculate(&bars);
        // 100..=104 held 100 each before bar 1 and 200 each before bar 2. Bar 1
        // opens at level 102; bar 2 opens in level 100, so it sweeps only 101..=103.
        let sweep =
            |bar_idx, price_low, price_high, swept_volume, direction, is_clean| LiquiditySweep {
                bar_idx,
                price_low,
                price_high,
                swept_volume,
                direction,
                is_clean,
            };
        let expected = [
            sweep(1, 100.0, 101.0, 200.0, SweepDirection::SweepLows, true),
            sweep(1, 103.0, 104.0, 200.0, SweepDirection::SweepHighs, false),
            sweep(2, 101.0, 103.0, 600.0, SweepDirection::SweepHighs, false),
        ];
        assert_eq!(tracker.sweeps(), expected);
    }
}
