//! Commodity Channel Index (CCI) Indicator
//!
//! CCI measures the current price level relative to an avg price level over
//! a given period. It oscillates above and below zero, helping identify cyclical
//! trends in a commodity or stock.
//!
//! # Interpretation
//! - Readings above +100: Overbought, potential reversal
//! - Readings below -100: Oversold, potential reversal
//! - Zero crossings can signal trend changes
//!
//! # Calculation
//! Typical Price (TP) = (High + Low + Close) / 3
//! SMA = Simple Moving Avg of TP
//! Mean Deviation = Avg of |TP - SMA|
//! CCI = (TP - SMA) / (0.015 * Mean Deviation)
//!
//! # Example
//! ```ignore
//! use egui_charts::CCI;
//!
//! let mut cci = CCI::new(20);
//! cci.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Commodity Channel Index indicator
#[derive(Clone)]
pub struct CCI {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl CCI {
    /// Create a new CCI indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
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

    /// Calculate typical price (TP) for a bar
    #[inline]
    fn typical_price(bar: &Bar) -> f64 {
        (bar.high + bar.low + bar.close) / 3.0
    }
}

impl Indicator for CCI {
    fn name(&self) -> &str {
        "CCI"
    }

    fn desc(&self) -> &str {
        "Commodity Channel Index - Cyclical trend indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate typical prices
        let tp: Vec<f64> = data.iter().map(Self::typical_price).collect();

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                // Calculate SMA of typical price
                let start = i + 1 - self.period;
                let sma: f64 = tp[start..=i].iter().sum::<f64>() / self.period as f64;

                // Calculate mean deviation
                let mean_dev: f64 =
                    tp[start..=i].iter().map(|p| (p - sma).abs()).sum::<f64>() / self.period as f64;

                // Calculate CCI
                if mean_dev.abs() < 1e-10 {
                    self.values.push(IndicatorValue::Single(0.0));
                } else {
                    // Lambert's constant 0.015 ensures ~70-80% of values fall between -100 and +100
                    let cci = (tp[i] - sma) / (0.015 * mean_dev);
                    self.values.push(IndicatorValue::Single(cci));
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
        false // CCI is drawn in separate pane
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
        vec![format!("CCI({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.2).sin() * 10.0;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 2.0,
                    low: price - 2.0,
                    close: price + 0.5,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_cci_calculation() {
        let bars = create_test_bars();
        let mut cci = CCI::new(20);
        cci.calculate(&bars);

        assert_eq!(cci.values().len(), bars.len());

        // First period-1 values should be None
        for i in 0..19 {
            assert!(matches!(cci.values()[i], IndicatorValue::None));
        }

        // Remaining values should be valid
        for value in cci.values().iter().skip(19) {
            assert!(matches!(value, IndicatorValue::Single(_)));
        }
    }

    #[test]
    fn test_cci_typical_price() {
        let bar = Bar {
            time: Utc::now(),
            open: 100.0,
            high: 110.0,
            low: 90.0,
            close: 105.0,
            volume: 1000.0,
        };

        let tp = CCI::typical_price(&bar);
        // (110 + 90 + 105) / 3 = 101.67
        assert!((tp - 101.666666).abs() < 0.001);
    }

    #[test]
    fn test_cci_flat_prices() {
        // With flat prices, CCI should be around 0
        let start = Utc::now();
        let bars: Vec<Bar> = (0..30)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
            })
            .collect();

        let mut cci = CCI::new(20);
        cci.calculate(&bars);

        for value in cci.values().iter().skip(19) {
            if let IndicatorValue::Single(v) = value {
                assert!(v.abs() < 0.001, "CCI should be ~0 for flat prices");
            }
        }
    }

    #[test]
    fn test_cci_empty_data() {
        let mut cci = CCI::new(20);
        cci.calculate(&[]);
        assert!(cci.values().is_empty());
    }
}
