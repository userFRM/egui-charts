//! # Iceberg Order Detector
//!
//! Detects hidden large orders that refresh at the same price level.
//! Icebergs are characterized by repeated fills at the same price.

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Iceberg Order Detector
///
/// Identifies potential iceberg orders by detecting:
/// - High volume at specific price levels
/// - Repeated touches of the same price
/// - Large cumulative volume relative to visible size
#[derive(Clone)]
pub struct IcebergDetector {
    /// Period for lookback analysis
    period: usize,
    /// Minimum volume threshold (as multiple of average)
    volume_threshold: f64,
    /// Minimum touches at price level to flag
    min_touches: usize,
    /// Calculated values
    values: Vec<IndicatorValue>,
    /// Color for buy iceberg markers
    buy_color: Color32,
    /// Color for sell iceberg markers
    sell_color: Color32,
    /// Visibility
    visible: bool,
    /// Detected icebergs
    detections: Vec<IcebergDetection>,
}

/// A detected iceberg order
#[derive(Debug, Clone)]
pub struct IcebergDetection {
    /// Bar index where detected
    pub bar_idx: usize,
    /// Price level
    pub price: f64,
    /// Cumulative volume at this level
    pub volume: f64,
    /// Number of touches
    pub touches: usize,
    /// Side (true = buy/bid, false = sell/ask)
    pub is_buy_side: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

impl IcebergDetector {
    /// Create a new iceberg detector
    pub fn new(period: usize) -> Self {
        Self {
            period,
            volume_threshold: 2.0,
            min_touches: 3,
            values: Vec::new(),
            buy_color: DESIGN_TOKENS.semantic.chart.bullish,
            sell_color: DESIGN_TOKENS.semantic.chart.bearish,
            visible: true,
            detections: Vec::new(),
        }
    }

    /// Set volume threshold
    pub fn with_volume_threshold(mut self, threshold: f64) -> Self {
        self.volume_threshold = threshold;
        self
    }

    /// Set minimum touches
    pub fn with_min_touches(mut self, touches: usize) -> Self {
        self.min_touches = touches;
        self
    }

    /// Set colors
    pub fn with_colors(mut self, buy: Color32, sell: Color32) -> Self {
        self.buy_color = buy;
        self.sell_color = sell;
        self
    }

    /// Get detected icebergs
    pub fn detections(&self) -> &[IcebergDetection] {
        &self.detections
    }

    /// Analyze price level activity
    #[allow(dead_code)]
    fn analyze_price_level(
        &self,
        bars: &[Bar],
        start_idx: usize,
        price: f64,
        tick_size: f64,
    ) -> Option<(usize, f64, bool)> {
        let end_idx = (start_idx + self.period).min(bars.len());
        let mut touches = 0;
        let mut total_volume = 0.0;
        let mut buy_volume = 0.0;
        let mut sell_volume = 0.0;

        for i in start_idx..end_idx {
            let bar = &bars[i];
            let bar_low = bar.low;
            let bar_high = bar.high;

            // Check if bar touched this price level
            if bar_low - tick_size <= price && bar_high + tick_size >= price {
                touches += 1;

                // Estimate volume at this level (simplified)
                let bar_range = bar.high - bar.low;
                if bar_range > 0.0 {
                    let level_volume = bar.volume / (bar_range / tick_size).max(1.0);
                    total_volume += level_volume;

                    // Estimate side based on close
                    if bar.close >= bar.open {
                        buy_volume += level_volume;
                    } else {
                        sell_volume += level_volume;
                    }
                }
            }
        }

        if touches >= self.min_touches {
            let is_buy = buy_volume > sell_volume;
            Some((touches, total_volume, is_buy))
        } else {
            None
        }
    }
}

impl Indicator for IcebergDetector {
    fn name(&self) -> &str {
        "Iceberg"
    }

    fn desc(&self) -> &str {
        "Iceberg Order Detector - Identifies hidden large orders"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        self.detections.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate average volume for threshold
        let avg_volume: f64 = data.iter().map(|b| b.volume).sum::<f64>() / data.len() as f64;
        let volume_threshold = avg_volume * self.volume_threshold;

        // Estimate tick size from price data
        let tick_size = estimate_tick_size(data);

        // Track price levels and their touches
        use std::collections::HashMap;
        let mut price_levels: HashMap<i64, (usize, f64, usize, bool)> = HashMap::new();

        for (idx, bar) in data.iter().enumerate() {
            // Quantize prices to tick size
            let low_key = (bar.low / tick_size).round() as i64;
            let high_key = (bar.high / tick_size).round() as i64;

            let is_bullish = bar.close >= bar.open;

            // Track all price levels in this bar
            for key in low_key..=high_key {
                let entry = price_levels.entry(key).or_insert((0, 0.0, idx, is_bullish));
                entry.0 += 1; // touches
                entry.1 += bar.volume / (high_key - low_key + 1) as f64; // volume per level
            }

            // Check for iceberg detection
            let close_key = (bar.close / tick_size).round() as i64;
            if let Some((touches, vol, _, is_buy)) = price_levels.get(&close_key) {
                if *touches >= self.min_touches && *vol >= volume_threshold {
                    let price = close_key as f64 * tick_size;
                    let confidence = (*touches as f64 / (self.min_touches * 2) as f64).min(1.0)
                        * (*vol / (volume_threshold * 2.0)).min(1.0);

                    self.detections.push(IcebergDetection {
                        bar_idx: idx,
                        price,
                        volume: *vol,
                        touches: *touches,
                        is_buy_side: *is_buy,
                        confidence,
                    });

                    // Mark with confidence score
                    self.values.push(IndicatorValue::Single(confidence));
                } else {
                    self.values.push(IndicatorValue::None);
                }
            } else {
                self.values.push(IndicatorValue::None);
            }

            // Decay old price level data (sliding window)
            if idx >= self.period {
                let old_bar = &data[idx - self.period];
                let old_low_key = (old_bar.low / tick_size).round() as i64;
                let old_high_key = (old_bar.high / tick_size).round() as i64;

                for key in old_low_key..=old_high_key {
                    if let Some(entry) = price_levels.get_mut(&key) {
                        entry.0 = entry.0.saturating_sub(1);
                        if entry.0 == 0 {
                            price_levels.remove(&key);
                        }
                    }
                }
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.buy_color, self.sell_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.buy_color = colors[0];
        }
        if colors.len() > 1 {
            self.sell_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        true // Icebergs are marked on the main chart
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
        vec!["Iceberg".to_string()]
    }
}

/// Estimate tick size from price data
fn estimate_tick_size(bars: &[Bar]) -> f64 {
    if bars.is_empty() {
        return 0.01;
    }

    // Find the smallest price difference
    let mut min_diff = f64::MAX;
    for i in 1..bars.len().min(100) {
        let diffs = [
            (bars[i].high - bars[i - 1].high).abs(),
            (bars[i].low - bars[i - 1].low).abs(),
            (bars[i].close - bars[i - 1].close).abs(),
        ];

        for diff in diffs {
            if diff > 0.0001 && diff < min_diff {
                min_diff = diff;
            }
        }
    }

    // Round to common tick sizes
    if min_diff < 0.001 {
        0.0001
    } else if min_diff < 0.01 {
        0.001
    } else if min_diff < 0.1 {
        0.01
    } else if min_diff < 0.5 {
        0.25
    } else if min_diff < 1.0 {
        0.5
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_iceberg_bars() -> Vec<Bar> {
        let ts = Utc::now();
        let mut bars = Vec::new();

        // Create bars that repeatedly touch 100.0 (iceberg pattern)
        for i in 0..20 {
            let base = if i % 2 == 0 { 99.0 } else { 100.0 };
            bars.push(Bar {
                time: ts,
                open: base + 0.5,
                high: 101.0, // All touch 100.0 level
                low: 99.0,   // All touch 100.0 level
                close: 100.0,
                volume: 1000.0,
            });
        }

        bars
    }

    #[test]
    fn test_iceberg_detection() {
        let mut detector = IcebergDetector::new(10).with_min_touches(3);
        let bars = create_iceberg_bars();

        detector.calculate(&bars);

        // Should detect icebergs due to repeated touches at 100.0
        assert!(!detector.detections.is_empty());
    }

    #[test]
    fn test_tick_size_estimation() {
        let ts = Utc::now();
        let bars: Vec<Bar> = (0..10)
            .map(|i| Bar {
                time: ts,
                open: 100.0 + i as f64 * 0.25,
                high: 100.5 + i as f64 * 0.25,
                low: 99.5 + i as f64 * 0.25,
                close: 100.25 + i as f64 * 0.25,
                volume: 1000.0,
            })
            .collect();

        let tick = estimate_tick_size(&bars);
        assert!(tick <= 0.5); // Should detect ~0.25 tick size
    }
}
