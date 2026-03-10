//! Kaufman Adaptive Moving Average (KAMA) indicator.
//!
//! KAMA is an adaptive moving average that adjusts its smoothing speed based
//! on the market's efficiency ratio. In trending markets it becomes more
//! responsive; in choppy, ranging markets it slows down to filter noise.
//!
//! # Formula
//!
//! ```text
//! Efficiency Ratio (ER) = |Close - Close[N]| / Sum(|Close[i] - Close[i-1]|, N)
//! Smoothing Constant (SC) = [ER * (fast_sc - slow_sc) + slow_sc]^2
//! KAMA[t] = KAMA[t-1] + SC * (Close[t] - KAMA[t-1])
//! ```
//!
//! # Default parameters
//!
//! `KAMA::new(10)` with fast period 2 and slow period 30.
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{KAMA, Indicator};
//!
//! let mut kama = KAMA::new(10);
//! kama.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Kaufman Adaptive Moving Average indicator.
///
/// Adapts its smoothing speed to market conditions via the efficiency
/// ratio. Overlay indicator drawn on the price chart.
#[derive(Clone)]
pub struct KAMA {
    /// Period for Efficiency Ratio calculation.
    period: usize,
    /// Fastest EMA period (typically 2).
    fast_period: usize,
    /// Slowest EMA period (typically 30).
    slow_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl KAMA {
    /// Create a new KAMA indicator.
    ///
    /// # Arguments
    /// * `period` -- Efficiency ratio lookback period (default: 10).
    ///
    /// Fast/slow periods default to 2 and 30 respectively. Use
    /// [`with_periods`](Self::with_periods) to customise.
    pub fn new(period: usize) -> Self {
        Self {
            period,
            fast_period: 2,
            slow_period: 30,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
            visible: true,
        }
    }

    /// Set custom fast and slow EMA periods.
    pub fn with_periods(mut self, fast: usize, slow: usize) -> Self {
        self.fast_period = fast;
        self.slow_period = slow;
        self
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate the Efficiency Ratio (ER) for a price window.
    ///
    /// ER = |net change| / sum of |bar-to-bar changes|.
    /// Returns a value between 0.0 (no net progress) and 1.0 (perfectly trending).
    fn efficiency_ratio(prices: &[f64]) -> f64 {
        if prices.len() < 2 {
            return 0.0;
        }

        let change = (prices[prices.len() - 1] - prices[0]).abs();

        let mut volatility = 0.0;
        for i in 1..prices.len() {
            volatility += (prices[i] - prices[i - 1]).abs();
        }

        if volatility > 0.0 {
            change / volatility
        } else {
            0.0
        }
    }

    /// Calculate smoothing constant from ER
    fn smoothing_constant(&self, er: f64) -> f64 {
        let fast_sc = 2.0 / (self.fast_period as f64 + 1.0);
        let slow_sc = 2.0 / (self.slow_period as f64 + 1.0);

        // SC = [ER * (FastSC - SlowSC) + SlowSC]^2
        let sc = er * (fast_sc - slow_sc) + slow_sc;
        sc * sc
    }
}

impl Indicator for KAMA {
    fn name(&self) -> &str {
        "KAMA"
    }

    fn desc(&self) -> &str {
        "Kaufman Adaptive Moving Avg - Adjusts smoothing based on market efficiency"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // First values are None
        for _ in 0..self.period - 1 {
            self.values.push(IndicatorValue::None);
        }

        // Initialize KAMA with first valid SMA
        let first_prices: Vec<f64> = data[0..self.period].iter().map(|bar| bar.close).collect();
        let mut kama = first_prices.iter().sum::<f64>() / self.period as f64;
        self.values.push(IndicatorValue::Single(kama));

        // Calculate remaining KAMA values
        for i in self.period..data.len() {
            let prices: Vec<f64> = data[i + 1 - self.period..=i]
                .iter()
                .map(|bar| bar.close)
                .collect();

            let er = Self::efficiency_ratio(&prices);
            let sc = self.smoothing_constant(er);

            let close = data[i].close;
            kama = kama + sc * (close - kama);

            self.values.push(IndicatorValue::Single(kama));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn is_overlay(&self) -> bool {
        true
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
        vec![format!(
            "KAMA({}, {}, {})",
            self.period, self.fast_period, self.slow_period
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_efficiency_ratio_trending() {
        // Perfect uptrend: ER should be 1.0
        let prices = vec![100.0, 101.0, 102.0, 103.0, 104.0];
        let er = KAMA::efficiency_ratio(&prices);
        assert!((er - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_efficiency_ratio_ranging() {
        // Choppy market: ER should be close to 0
        let prices = vec![100.0, 102.0, 100.0, 102.0, 100.0];
        let er = KAMA::efficiency_ratio(&prices);
        // Change = 0, so ER = 0
        assert!((er - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_kama_calculation() {
        let mut kama = KAMA::new(5);

        let data = vec![
            make_bar(100.0),
            make_bar(101.0),
            make_bar(102.0),
            make_bar(103.0),
            make_bar(104.0),
            make_bar(105.0),
            make_bar(106.0),
        ];

        kama.calculate(&data);

        assert_eq!(kama.values.len(), 7);

        // First 4 values should be None
        for i in 0..4 {
            assert!(matches!(kama.values[i], IndicatorValue::None));
        }

        // Fifth value should be present (first KAMA)
        assert!(matches!(kama.values[4], IndicatorValue::Single(_)));
    }

    #[test]
    fn test_smoothing_constant() {
        let kama = KAMA::new(10);

        // ER = 0 (ranging) should give slow SC
        let sc_slow = kama.smoothing_constant(0.0);
        let expected_slow = (2.0_f64 / 31.0).powi(2);
        assert!((sc_slow - expected_slow).abs() < 0.0001);

        // ER = 1 (trending) should give fast SC
        let sc_fast = kama.smoothing_constant(1.0);
        let expected_fast = (2.0_f64 / 3.0).powi(2);
        assert!((sc_fast - expected_fast).abs() < 0.0001);
    }
}
