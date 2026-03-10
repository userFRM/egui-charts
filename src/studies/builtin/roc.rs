//! Rate of Change (ROC) Indicator
//!
//! ROC is a momentum oscillator that measures the percentage change in price
//! between the current price and the price N periods ago.
//!
//! # Interpretation
//! - Positive ROC: Price is higher than N periods ago (bullish)
//! - Negative ROC: Price is lower than N periods ago (bearish)
//! - Zero crossings can signal trend changes
//! - Extreme readings may indicate overbought/oversold conditions
//!
//! # Calculation
//! ROC = ((Close - Close[n]) / Close[n]) * 100
//!
//! # Example
//! ```ignore
//! use egui_charts::RateOfChange;
//!
//! let mut roc = RateOfChange::new(12);
//! roc.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Rate of Change indicator
#[derive(Clone)]
pub struct RateOfChange {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl RateOfChange {
    /// Create a new ROC indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 12 or 25)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.success,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for RateOfChange {
    fn name(&self) -> &str {
        "ROC"
    }

    fn desc(&self) -> &str {
        "Rate of Change - Momentum oscillator (percentage)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() <= self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let prev_close = data[i - self.period].close;
                if prev_close.abs() < 1e-10 {
                    self.values.push(IndicatorValue::Single(0.0));
                } else {
                    let roc = ((data[i].close - prev_close) / prev_close) * 100.0;
                    self.values.push(IndicatorValue::Single(roc));
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
        false // ROC is drawn in separate pane
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
        vec![format!("ROC({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_roc_calculation() {
        let start = Utc::now();
        let bars: Vec<Bar> = (0..20)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0 + i as f64,
                high: 101.0 + i as f64,
                low: 99.0 + i as f64,
                close: 100.0 + i as f64,
                volume: 1000.0,
            })
            .collect();

        let mut roc = RateOfChange::new(5);
        roc.calculate(&bars);

        assert_eq!(roc.values().len(), 20);

        // First 5 values should be None
        for i in 0..5 {
            assert!(matches!(roc.values()[i], IndicatorValue::None));
        }

        // Check ROC at index 5: ((105 - 100) / 100) * 100 = 5%
        if let IndicatorValue::Single(v) = roc.values()[5] {
            assert!((v - 5.0).abs() < 0.01, "Expected ROC of 5%, got {}", v);
        }
    }

    #[test]
    fn test_roc_no_change() {
        let start = Utc::now();
        let bars: Vec<Bar> = (0..20)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0, // All same close price
                volume: 1000.0,
            })
            .collect();

        let mut roc = RateOfChange::new(5);
        roc.calculate(&bars);

        // ROC should be 0 when prices don't change
        for value in roc.values().iter().skip(5) {
            if let IndicatorValue::Single(v) = value {
                assert!(v.abs() < 0.001, "ROC should be 0 for flat prices");
            }
        }
    }

    #[test]
    fn test_roc_negative() {
        let start = Utc::now();
        let bars: Vec<Bar> = (0..20)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 120.0 - i as f64,
                high: 121.0 - i as f64,
                low: 119.0 - i as f64,
                close: 120.0 - i as f64, // Declining prices
                volume: 1000.0,
            })
            .collect();

        let mut roc = RateOfChange::new(5);
        roc.calculate(&bars);

        // ROC should be negative for declining prices
        for value in roc.values().iter().skip(5) {
            if let IndicatorValue::Single(v) = value {
                assert!(*v < 0.0, "ROC should be negative for declining prices");
            }
        }
    }

    #[test]
    fn test_roc_empty_data() {
        let mut roc = RateOfChange::new(12);
        roc.calculate(&[]);
        assert!(roc.values().is_empty());
    }
}
