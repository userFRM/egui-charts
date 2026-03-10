//! Weighted Moving Avg (WMA) Indicator
//!
//! WMA assigns greater weight to more recent data points, making it more
//! responsive to recent price changes than SMA.
//!
//! # Formula
//! WMA = (P1 * n + P2 * (n-1) + ... + Pn * 1) / (n * (n+1) / 2)
//!
//! # Example
//! ```ignore
//! use egui_charts::WMA;
//!
//! let mut wma = WMA::new(14);
//! wma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Weighted Moving Avg indicator
#[derive(Clone)]
pub struct WMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl WMA {
    /// Create a new WMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation (typically 14, 20, or 50)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for WMA {
    fn name(&self) -> &str {
        "WMA"
    }

    fn desc(&self) -> &str {
        "Weighted Moving Avg - Gives more weight to recent prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Weight sum: n*(n+1)/2
        let weight_sum = (self.period * (self.period + 1)) / 2;

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let mut weighted_sum = 0.0;
                for j in 0..self.period {
                    let weight = (self.period - j) as f64;
                    weighted_sum += data[i - j].close * weight;
                }
                let wma = weighted_sum / weight_sum as f64;
                self.values.push(IndicatorValue::Single(wma));
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
        true
    }

    fn line_cnt(&self) -> usize {
        1
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
        vec![format!("WMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..20)
            .map(|i| {
                let price = 100.0 + i as f64;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_wma_calculation() {
        let bars = create_test_bars();
        let mut wma = WMA::new(5);
        wma.calculate(&bars);

        assert_eq!(wma.values().len(), bars.len());

        // First 4 values should be None
        for i in 0..4 {
            assert!(matches!(wma.values()[i], IndicatorValue::None));
        }

        // Check that values exist after warmup
        for value in wma.values().iter().skip(4) {
            if let IndicatorValue::Single(v) = value {
                assert!(*v > 0.0);
            } else {
                panic!("Expected Single value");
            }
        }
    }

    #[test]
    fn test_wma_more_responsive() {
        // WMA should be closer to recent prices than SMA
        let bars = create_test_bars();
        let mut wma = WMA::new(5);
        wma.calculate(&bars);

        if let IndicatorValue::Single(wma_val) = wma.values().last().unwrap() {
            // In an uptrend, WMA should be higher than simple midpoint
            let last_close = bars.last().unwrap().close;
            let first_of_window = bars[bars.len() - 5].close;
            let midpoint = (last_close + first_of_window) / 2.0;
            assert!(
                *wma_val > midpoint,
                "WMA should be biased toward recent prices"
            );
        }
    }

    #[test]
    fn test_wma_is_overlay() {
        let wma = WMA::new(14);
        assert!(wma.is_overlay());
    }

    #[test]
    fn test_wma_empty_data() {
        let mut wma = WMA::new(14);
        wma.calculate(&[]);
        assert!(wma.values().is_empty());
    }
}
