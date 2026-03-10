//! # Liquidity Tracker
//!
//! Tracks where large orders are sitting and identifies liquidity
//! sweeps and accumulation zones.

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;
use std::collections::HashMap;

/// Liquidity Tracker
///
/// Identifies:
/// - High liquidity price levels (accumulated volume)
/// - Liquidity sweeps (price moves through liquidity zones)
/// - Liquidity imbalances
#[derive(Clone)]
pub struct LiquidityTracker {
    /// Period for analysis
    period: usize,
    /// Tick size for price grouping
    tick_size: f64,
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

/// A liquidity sweep event
#[derive(Debug, Clone)]
pub struct LiquiditySweep {
    /// Bar index where sweep occurred
    pub bar_idx: usize,
    /// Price level that was swept
    pub swept_price: f64,
    /// Volume that was swept
    pub swept_volume: f64,
    /// Direction of sweep
    pub direction: SweepDirection,
    /// Was this a clean sweep (price moved through and continued)
    pub is_clean: bool,
}

/// Direction of liquidity sweep
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepDirection {
    /// Sweep through lows (liquidity grab below)
    SweepLows,
    /// Sweep through highs (liquidity grab above)
    SweepHighs,
}

impl LiquidityTracker {
    /// Create a new liquidity tracker
    pub fn new(period: usize) -> Self {
        Self {
            period,
            tick_size: 1.0,
            volume_threshold: 2.0, // Multiple of average
            values: Vec::new(),
            high_liq_color: Color32::from_rgba_unmultiplied(255, 215, 0, 128), // Gold
            sweep_color: Color32::from_rgba_unmultiplied(255, 0, 255, 180),    // Magenta
            visible: true,
            liquidity_map: HashMap::new(),
            zones: Vec::new(),
            sweeps: Vec::new(),
        }
    }

    /// Set tick size
    pub fn with_tick_size(mut self, tick_size: f64) -> Self {
        self.tick_size = tick_size;
        self
    }

    /// Set volume threshold
    pub fn with_volume_threshold(mut self, threshold: f64) -> Self {
        self.volume_threshold = threshold;
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
        (price / self.tick_size).round() as i64
    }

    fn key_to_price(&self, key: i64) -> f64 {
        key as f64 * self.tick_size
    }

    /// Add volume to a price level
    fn add_volume(&mut self, price: f64, volume: f64, bar_idx: usize, is_buy: bool) {
        let key = self.price_to_key(price);
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

    /// Decay old liquidity
    fn decay_old_liquidity(&mut self, current_idx: usize) {
        let decay_threshold = current_idx.saturating_sub(self.period);
        self.liquidity_map
            .retain(|_, level| level.last_touch > decay_threshold);
    }

    /// Detect liquidity sweep
    fn check_sweep(&mut self, bar: &Bar, bar_idx: usize, avg_volume: f64) {
        let threshold = avg_volume * self.volume_threshold;

        // Check for sweep through lows
        let low_key = self.price_to_key(bar.low);
        for (key, level) in &self.liquidity_map {
            if *key <= low_key && level.volume >= threshold {
                // Price swept through this liquidity level
                let is_clean = bar.close > level.price; // Reversed back up

                self.sweeps.push(LiquiditySweep {
                    bar_idx,
                    swept_price: level.price,
                    swept_volume: level.volume,
                    direction: SweepDirection::SweepLows,
                    is_clean,
                });
            }
        }

        // Check for sweep through highs
        let high_key = self.price_to_key(bar.high);
        for (key, level) in &self.liquidity_map {
            if *key >= high_key && level.volume >= threshold {
                let is_clean = bar.close < level.price; // Reversed back down

                self.sweeps.push(LiquiditySweep {
                    bar_idx,
                    swept_price: level.price,
                    swept_volume: level.volume,
                    direction: SweepDirection::SweepHighs,
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

        significant.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

        if significant.is_empty() {
            return;
        }

        // Group adjacent levels into zones
        let mut current_zone_start = 0;
        let zone_gap = self.tick_size * 3.0; // Max gap between levels in a zone

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

impl Indicator for LiquidityTracker {
    fn name(&self) -> &str {
        "Liquidity"
    }

    fn desc(&self) -> &str {
        "Liquidity Tracker - Tracks volume accumulation at price levels"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        self.liquidity_map.clear();
        self.zones.clear();
        self.sweeps.clear();

        if data.is_empty() {
            return;
        }

        // Calculate average volume
        let avg_volume: f64 = data.iter().map(|b| b.volume).sum::<f64>() / data.len() as f64;

        for (idx, bar) in data.iter().enumerate() {
            let is_bullish = bar.close >= bar.open;
            let range = bar.high - bar.low;

            if range > 0.0 {
                // Distribute volume across price levels touched
                let low_key = self.price_to_key(bar.low);
                let high_key = self.price_to_key(bar.high);
                let levels_count = (high_key - low_key + 1) as f64;
                let vol_per_level = bar.volume / levels_count;

                for key in low_key..=high_key {
                    let price = self.key_to_price(key);
                    self.add_volume(price, vol_per_level, idx, is_bullish);
                }
            } else {
                // Doji - all volume at close
                self.add_volume(bar.close, bar.volume, idx, is_bullish);
            }

            // Check for liquidity sweep
            self.check_sweep(bar, idx, avg_volume);

            // Decay old liquidity
            self.decay_old_liquidity(idx);

            // Calculate value for this bar (max liquidity ratio)
            let max_liq = self
                .liquidity_map
                .values()
                .map(|l| l.volume)
                .fold(0.0_f64, f64::max);

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
        true // Liquidity levels shown on main chart
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

    fn create_test_bars() -> Vec<Bar> {
        let ts = Utc::now();
        (0..20)
            .map(|i| {
                let base = 100.0 + (i % 3) as f64; // Oscillate around 100-102
                Bar {
                    time: ts,
                    open: base,
                    high: base + 1.0,
                    low: base - 1.0,
                    close: base + 0.5,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_liquidity_tracking() {
        let mut tracker = LiquidityTracker::new(10).with_tick_size(0.5);

        let bars = create_test_bars();
        tracker.calculate(&bars);

        assert_eq!(tracker.values.len(), bars.len());

        // Should have accumulated liquidity
        assert!(!tracker.liquidity_map.is_empty());
    }

    #[test]
    fn test_liquidity_zones() {
        let mut tracker = LiquidityTracker::new(10)
            .with_tick_size(0.5)
            .with_volume_threshold(1.0); // Lower threshold for test

        let bars = create_test_bars();
        tracker.calculate(&bars);

        // May or may not have zones depending on data distribution
        // Just verify it doesn't crash
        let _ = tracker.zones();
    }

    #[test]
    fn test_price_key_conversion() {
        let tracker = LiquidityTracker::new(10).with_tick_size(0.25);

        let price = 100.5;
        let key = tracker.price_to_key(price);
        let back = tracker.key_to_price(key);

        assert!((back - 100.5).abs() < 0.01);
    }
}
