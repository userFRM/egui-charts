//! Avg True Range (ATR) Indicator
//!
//! ATR measures market volatility by decomposing the entire range of an asset price.
//! It's commonly used for position sizing and stop-loss placement.
//!
//! # Calculation
//! True Range = max(high - low, |high - prev_close|, |low - prev_close|)
//! ATR = Exponential moving avg of True Range over N periods
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ATR, Indicator};
//!
//! let mut atr = ATR::new(14);
//! atr.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Avg True Range indicator
#[derive(Clone)]
pub struct ATR {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ATR {
    /// Create a new ATR indicator with the specified period
    ///
    /// # Arguments
    /// * `period` - The lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate True Range for a single bar
    #[inline]
    pub fn true_range(bar: &Bar, prev_close: f64) -> f64 {
        let hl = bar.high - bar.low;
        let hc = (bar.high - prev_close).abs();
        let lc = (bar.low - prev_close).abs();
        hl.max(hc).max(lc)
    }
}

/// Construct with the conventional default parameters.
impl Default for ATR {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ATR {
    fn name(&self) -> &str {
        "ATR"
    }

    fn desc(&self) -> &str {
        "Avg True Range - Volatility indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Calculate True Range for all bars
        let mut true_ranges = Vec::with_capacity(data.len());
        true_ranges.push(data[0].high - data[0].low); // First TR is just high - low

        for i in 1..data.len() {
            let tr = Self::true_range(&data[i], data[i - 1].close);
            true_ranges.push(tr);
        }

        // Calculate ATR using Wilder's smoothing method
        let multiplier = 1.0 / self.period as f64;

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else if i == self.period - 1 {
                // Initial ATR is simple avg
                let sum: f64 = true_ranges[..=i].iter().sum();
                let atr = sum / self.period as f64;
                self.values.push(IndicatorValue::Single(atr));
            } else {
                // Subsequent ATR uses smoothing
                if let IndicatorValue::Single(prev_atr) = self.values[i - 1] {
                    let atr = prev_atr + multiplier * (true_ranges[i] - prev_atr);
                    self.values.push(IndicatorValue::Single(atr));
                } else {
                    self.values.push(IndicatorValue::None);
                }
            }
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
        false // ATR is drawn in separate pane
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
        vec![format!("ATR({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_true_range_calculation() {
        let bar = Bar {
            time: Utc::now(),
            open: 100.0,
            high: 105.0,
            low: 98.0,
            close: 103.0,
            volume: 1000.0,
        };

        // Case 1: Normal range (high - low is largest)
        let tr = ATR::true_range(&bar, 100.0);
        assert!((tr - 7.0).abs() < 0.001); // high - low = 105 - 98 = 7

        // Case 2: Gap up (high - prev_close is largest)
        let tr = ATR::true_range(&bar, 95.0);
        assert!((tr - 10.0).abs() < 0.001); // high - prev = 105 - 95 = 10

        // Case 3: Gap down (|low - prev_close| is largest)
        let tr = ATR::true_range(&bar, 110.0);
        assert!((tr - 12.0).abs() < 0.001); // |low - prev| = |98 - 110| = 12
    }

    /// A bar's ATR does not depend on whether a later bar exists: with length 1 the first bar has its range.
    #[test]
    fn length_one_has_a_value_on_a_single_bar() {
        let bar = |high, low, close| Bar {
            time: Utc::now(),
            open: close,
            high,
            low,
            close,
            volume: 1000.0,
        };
        let first = bar(102.0, 98.0, 100.0);
        // Ranges by hand: 102 - 98 = 4; for the second bar max(103 - 99, |103 - 100|, |99 - 100|) = 4.
        let cases: [(Vec<Bar>, Vec<f64>); 2] = [
            (vec![first.clone()], vec![4.0]),
            (vec![first, bar(103.0, 99.0, 101.0)], vec![4.0, 4.0]),
        ];
        for (data, expected) in cases {
            let mut atr = ATR::new(1);
            atr.calculate(&data);
            assert_eq!(atr.values().len(), expected.len());
            for (got, want) in atr.values().iter().zip(expected) {
                assert!(
                    matches!(got, IndicatorValue::Single(v) if (v - want).abs() < 1e-12),
                    "{got:?} != {want}"
                );
            }
        }
    }
}
