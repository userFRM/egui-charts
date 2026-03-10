//! Standard Deviation Indicator
//!
//! Standard Deviation measures price volatility by calculating the dispersion
//! of prices from their moving avg.
//!
//! # Formula
//! StdDev = sqrt(Σ(price - SMA)² / period)
//!
//! # Interpretation
//! - High StdDev: High volatility, larger price movements
//! - Low StdDev: Low volatility, consolidation period
//! - Often used as component of Bollinger Bands
//!
//! # Example
//! ```ignore
//! use egui_charts::StandardDeviation;
//!
//! let mut std_dev = StandardDeviation::new(20);
//! std_dev.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Standard Deviation indicator
#[derive(Clone)]
pub struct StandardDeviation {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl StandardDeviation {
    /// Create a new Standard Deviation indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.favorite_gold,
            visible: true,
        }
    }

    /// Create with default param (20)
    pub fn default_params() -> Self {
        Self::new(20)
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for StandardDeviation {
    fn name(&self) -> &str {
        "StdDev"
    }

    fn desc(&self) -> &str {
        "Standard Deviation - Measures price volatility"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;

                // Calculate mean (SMA)
                let sum: f64 = data[start..=i].iter().map(|b| b.close).sum();
                let mean = sum / self.period as f64;

                // Calculate variance
                let variance: f64 = data[start..=i]
                    .iter()
                    .map(|b| (b.close - mean).powi(2))
                    .sum::<f64>()
                    / self.period as f64;

                // Standard deviation is square root of variance
                let std_dev = variance.sqrt();
                self.values.push(IndicatorValue::Single(std_dev));
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
        false // StdDev is typically in a separate pane
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
        vec![format!("StdDev({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..30)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.3).sin() * 10.0;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 2.0,
                    low: price - 2.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    fn create_flat_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..30)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
            })
            .collect()
    }

    #[test]
    fn test_std_dev_calculation() {
        let bars = create_test_bars();
        let mut std_dev = StandardDeviation::new(20);
        std_dev.calculate(&bars);

        assert_eq!(std_dev.values().len(), bars.len());

        // First period-1 values should be None
        for i in 0..19 {
            assert!(matches!(std_dev.values()[i], IndicatorValue::None));
        }

        // Remaining should have values
        for value in std_dev.values().iter().skip(19) {
            assert!(matches!(value, IndicatorValue::Single(_)));
        }
    }

    #[test]
    fn test_std_dev_positive() {
        let bars = create_test_bars();
        let mut std_dev = StandardDeviation::new(20);
        std_dev.calculate(&bars);

        for value in std_dev.values() {
            if let IndicatorValue::Single(val) = value {
                assert!(*val >= 0.0, "StdDev should be non-negative");
            }
        }
    }

    #[test]
    fn test_std_dev_flat_prices() {
        let bars = create_flat_bars();
        let mut std_dev = StandardDeviation::new(20);
        std_dev.calculate(&bars);

        // StdDev of flat prices should be 0
        for value in std_dev.values().iter().skip(19) {
            if let IndicatorValue::Single(val) = value {
                assert!(
                    val.abs() < 0.001,
                    "StdDev should be ~0 for flat prices, got {}",
                    val
                );
            }
        }
    }

    #[test]
    fn test_std_dev_is_not_overlay() {
        let std_dev = StandardDeviation::new(20);
        assert!(!std_dev.is_overlay());
    }

    #[test]
    fn test_std_dev_empty_data() {
        let mut std_dev = StandardDeviation::new(20);
        std_dev.calculate(&[]);
        assert!(std_dev.values().is_empty());
    }
}
