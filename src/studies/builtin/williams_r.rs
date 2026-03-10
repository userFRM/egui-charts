//! Williams %R Indicator
//!
//! Williams %R is a momentum oscillator that measures overbought/oversold levels.
//! It's the inverse of the Fast Stochastic Oscillator, ranging from -100 to 0.
//!
//! # Interpretation
//! - Readings from -20 to 0: Overbought
//! - Readings from -100 to -80: Oversold
//!
//! # Calculation
//! %R = (Highest High - Close) / (Highest High - Lowest Low) * -100
//!
//! # Example
//! ```ignore
//! use egui_charts::WilliamsR;
//!
//! let mut williams = WilliamsR::new(14);
//! williams.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Williams %R indicator
#[derive(Clone)]
pub struct WilliamsR {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl WilliamsR {
    /// Create a new Williams %R indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.pink,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate highest high over a period
    fn highest_high(data: &[Bar], end: usize, period: usize) -> f64 {
        let start = end.saturating_sub(period - 1);
        data[start..=end]
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Calculate lowest low over a period
    fn lowest_low(data: &[Bar], end: usize, period: usize) -> f64 {
        let start = end.saturating_sub(period - 1);
        data[start..=end]
            .iter()
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min)
    }
}

impl Indicator for WilliamsR {
    fn name(&self) -> &str {
        "Williams %R"
    }

    fn desc(&self) -> &str {
        "Williams %R - Momentum oscillator (-100 to 0)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let highest = Self::highest_high(data, i, self.period);
                let lowest = Self::lowest_low(data, i, self.period);
                let range = highest - lowest;

                if range.abs() < 1e-10 {
                    self.values.push(IndicatorValue::Single(-50.0)); // Middle value
                } else {
                    let r = (highest - data[i].close) / range * -100.0;
                    self.values
                        .push(IndicatorValue::Single(r.clamp(-100.0, 0.0)));
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
        false // Williams %R is drawn in separate pane
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
        vec![format!("%R({})", self.period)]
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
                    close: price + 0.5,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_williams_r_range() {
        let bars = create_test_bars();
        let mut williams = WilliamsR::new(14);
        williams.calculate(&bars);

        for value in williams.values() {
            if let IndicatorValue::Single(v) = value {
                assert!(
                    *v >= -100.0 && *v <= 0.0,
                    "Williams %R should be -100 to 0, got {}",
                    v
                );
            }
        }
    }

    #[test]
    fn test_williams_r_at_high() {
        // When close equals highest high, %R should be 0
        let start = Utc::now();
        let bars: Vec<Bar> = (0..20)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 110.0,
                low: 90.0,
                close: 110.0, // At the high
                volume: 1000.0,
            })
            .collect();

        let mut williams = WilliamsR::new(14);
        williams.calculate(&bars);

        if let Some(IndicatorValue::Single(v)) = williams.values().last() {
            assert!((*v - 0.0).abs() < 0.001, "At high, %R should be 0");
        }
    }

    #[test]
    fn test_williams_r_at_low() {
        // When close equals lowest low, %R should be -100
        let start = Utc::now();
        let bars: Vec<Bar> = (0..20)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 110.0,
                low: 90.0,
                close: 90.0, // At the low
                volume: 1000.0,
            })
            .collect();

        let mut williams = WilliamsR::new(14);
        williams.calculate(&bars);

        if let Some(IndicatorValue::Single(v)) = williams.values().last() {
            assert!((*v - (-100.0)).abs() < 0.001, "At low, %R should be -100");
        }
    }

    #[test]
    fn test_williams_r_empty_data() {
        let mut williams = WilliamsR::new(14);
        williams.calculate(&[]);
        assert!(williams.values().is_empty());
    }
}
