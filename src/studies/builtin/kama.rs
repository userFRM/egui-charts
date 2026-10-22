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
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
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
            period: period.max(1),
            fast_period: 2,
            slow_period: 30,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
            visible: true,
        }
    }

    /// Set custom fast and slow EMA periods.
    pub fn with_periods(mut self, fast: usize, slow: usize) -> Self {
        self.fast_period = fast.max(1);
        self.slow_period = slow.max(1);
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

/// Construct with the conventional default parameters.
impl Default for KAMA {
    fn default() -> Self {
        Self::new(10)
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

        // N + 1 closes give the N price changes the efficiency ratio sums.
        for i in self.period..data.len() {
            let prices: Vec<f64> = data[i - self.period..=i]
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

    /// Expected values from Kaufman's definition (n price changes per
    /// efficiency ratio), seeded with the simple average of the first n
    /// closes; fast 2, slow 30.
    #[test]
    fn calculate_matches_kaufman_reference() {
        // (name, period, closes, expected KAMA per bar)
        type Case = (&'static str, usize, &'static [f64], &'static [Option<f64>]);
        let cases: &[Case] = &[
            (
                "choppy",
                4,
                &[10.0, 11.0, 10.5, 12.0, 11.0, 13.0, 12.5, 14.0, 13.0, 13.5],
                &[
                    None,
                    None,
                    None,
                    Some(10.875),
                    Some(10.88078101514626),
                    Some(11.078408170242067),
                    Some(11.210978306235772),
                    Some(11.47106771973126),
                    Some(11.613647858472564),
                    Some(11.656395577253855),
                ],
            ),
            (
                "trend",
                3,
                &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                &[
                    None,
                    None,
                    Some(2.0),
                    Some(2.8888888888888884),
                    Some(3.8271604938271597),
                    Some(4.792866941015088),
                ],
            ),
            (
                "flat",
                3,
                &[7.0; 5],
                &[None, None, Some(7.0), Some(7.0), Some(7.0)],
            ),
            ("short", 5, &[1.0, 2.0, 3.0], &[None, None, None]),
        ];

        for (name, period, closes, expected) in cases {
            let data: Vec<Bar> = closes.iter().map(|&c| make_bar(c)).collect();
            let mut kama = KAMA::new(*period);
            kama.calculate(&data);
            assert_eq!(kama.values.len(), expected.len(), "{name}");
            for (i, (got, want)) in kama.values.iter().zip(expected.iter()).enumerate() {
                match (got, want) {
                    (IndicatorValue::None, None) => {}
                    (IndicatorValue::Single(g), Some(w)) => {
                        assert!((g - w).abs() < 1e-12, "{name}[{i}]: got {g}, want {w}")
                    }
                    _ => panic!("{name}[{i}]: got {got:?}, want {want:?}"),
                }
            }
        }
    }
}
