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
//! ```ignore
//! use egui_charts::ATR;
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

impl Indicator for ATR {
    fn name(&self) -> &str {
        "ATR"
    }

    fn desc(&self) -> &str {
        "Avg True Range - Volatility indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
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
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        vec![
            Bar {
                time: start,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(5),
                open: 101.0,
                high: 104.0,
                low: 100.0,
                close: 103.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(10),
                open: 103.0,
                high: 105.0,
                low: 102.0,
                close: 104.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(15),
                open: 104.0,
                high: 106.0,
                low: 103.0,
                close: 105.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(20),
                open: 105.0,
                high: 107.0,
                low: 104.0,
                close: 106.0,
                volume: 1000.0,
            },
        ]
    }

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

    #[test]
    fn test_atr_calculation() {
        let bars = create_test_bars();
        let mut atr = ATR::new(3);
        atr.calculate(&bars);

        assert_eq!(atr.values().len(), 5);

        // First 2 values should be None (period - 1)
        assert!(matches!(atr.values()[0], IndicatorValue::None));
        assert!(matches!(atr.values()[1], IndicatorValue::None));

        // Remaining values should be valid
        assert!(matches!(atr.values()[2], IndicatorValue::Single(_)));
        assert!(matches!(atr.values()[3], IndicatorValue::Single(_)));
        assert!(matches!(atr.values()[4], IndicatorValue::Single(_)));
    }

    #[test]
    fn test_atr_positive() {
        let bars = create_test_bars();
        let mut atr = ATR::new(3);
        atr.calculate(&bars);

        for value in atr.values() {
            if let IndicatorValue::Single(v) = value {
                assert!(*v > 0.0, "ATR should always be positive");
            }
        }
    }

    #[test]
    fn test_atr_empty_data() {
        let mut atr = ATR::new(14);
        atr.calculate(&[]);
        assert!(atr.values().is_empty());
    }
}
