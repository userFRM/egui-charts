//! Stochastic Oscillator Indicator
//!
//! The Stochastic Oscillator is a momentum indicator comparing the closing price
//! to the range of prices over a specified period. It generates overbought/oversold
//! signals when %K crosses above 80 or below 20.
//!
//! # Components
//! - %K (Fast): Main oscillator line
//! - %D (Slow): Signal line (SMA of %K)
//!
//! # Calculation
//! %K = 100 * (Close - Lowest Low) / (Highest High - Lowest Low)
//! %D = SMA(%K, d_period)
//!
//! # Example
//! ```ignore
//! use egui_charts::Stochastic;
//!
//! let mut stoch = Stochastic::new(14, 3, 3);
//! stoch.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Stochastic Oscillator indicator
#[derive(Clone)]
pub struct Stochastic {
    k_period: usize,
    k_smooth: usize,
    d_period: usize,
    values: Vec<IndicatorValue>,
    k_color: Color32,
    d_color: Color32,
    visible: bool,
}

impl Stochastic {
    /// Create a new Stochastic Oscillator
    ///
    /// # Arguments
    /// * `k_period` - Lookback period for %K (typically 14)
    /// * `k_smooth` - Smoothing period for %K (typically 3)
    /// * `d_period` - Period for %D signal line (typically 3)
    pub fn new(k_period: usize, k_smooth: usize, d_period: usize) -> Self {
        Self {
            k_period: k_period.max(1),
            k_smooth: k_smooth.max(1),
            d_period: d_period.max(1),
            values: Vec::new(),
            k_color: DESIGN_TOKENS.semantic.extended.info, // Blue
            d_color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    /// Create with default params (14, 3, 3)
    pub fn default_params() -> Self {
        Self::new(14, 3, 3)
    }

    /// Set colors for %K and %D lines
    pub fn with_colors(mut self, k_color: Color32, d_color: Color32) -> Self {
        self.k_color = k_color;
        self.d_color = d_color;
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

    /// Calculate SMA of a slice
    fn sma(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }
}

impl Indicator for Stochastic {
    fn name(&self) -> &str {
        "Stochastic"
    }

    fn desc(&self) -> &str {
        "Stochastic Oscillator - Momentum oscillator (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.k_period + self.k_smooth - 1;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate raw %K (Fast Stochastic)
        let mut raw_k = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i < self.k_period - 1 {
                raw_k.push(f64::NAN);
            } else {
                let highest = Self::highest_high(data, i, self.k_period);
                let lowest = Self::lowest_low(data, i, self.k_period);
                let range = highest - lowest;

                if range.abs() < 1e-10 {
                    raw_k.push(50.0); // Middle value when no range
                } else {
                    let k = 100.0 * (data[i].close - lowest) / range;
                    raw_k.push(k.clamp(0.0, 100.0));
                }
            }
        }

        // Calculate smoothed %K (Slow Stochastic %K)
        let mut smooth_k = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i < self.k_period + self.k_smooth - 2 || raw_k[i].is_nan() {
                smooth_k.push(f64::NAN);
            } else {
                let start = i - self.k_smooth + 1;
                let valid_values: Vec<f64> = raw_k[start..=i]
                    .iter()
                    .filter(|v| !v.is_nan())
                    .copied()
                    .collect();

                if valid_values.len() >= self.k_smooth {
                    smooth_k.push(Self::sma(&valid_values));
                } else {
                    smooth_k.push(f64::NAN);
                }
            }
        }

        // Calculate %D (SMA of smoothed %K)
        let mut d_values = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i < self.k_period + self.k_smooth + self.d_period - 3 || smooth_k[i].is_nan() {
                d_values.push(f64::NAN);
            } else {
                let start = i - self.d_period + 1;
                let valid_values: Vec<f64> = smooth_k[start..=i]
                    .iter()
                    .filter(|v| !v.is_nan())
                    .copied()
                    .collect();

                if valid_values.len() >= self.d_period {
                    d_values.push(Self::sma(&valid_values));
                } else {
                    d_values.push(f64::NAN);
                }
            }
        }

        // Build output values
        for i in 0..data.len() {
            if smooth_k[i].is_nan() || d_values[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![smooth_k[i], d_values[i]]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.k_color, self.d_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.k_color = colors[0];
        }
        if colors.len() > 1 {
            self.d_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false // Stochastic is drawn in separate pane
    }

    fn line_cnt(&self) -> usize {
        2 // %K and %D
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
        vec![
            format!("%K({},{},{})", self.k_period, self.k_smooth, self.d_period),
            format!("%D"),
        ]
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
                    close: price + (i as f64 * 0.1).sin(),
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_stochastic_range() {
        let bars = create_test_bars();
        let mut stoch = Stochastic::new(14, 3, 3);
        stoch.calculate(&bars);

        for value in stoch.values() {
            if let IndicatorValue::Multiple(vals) = value {
                for v in vals {
                    assert!(*v >= 0.0 && *v <= 100.0, "Stochastic should be 0-100");
                }
            }
        }
    }

    #[test]
    fn test_stochastic_output_format() {
        let bars = create_test_bars();
        let mut stoch = Stochastic::new(14, 3, 3);
        stoch.calculate(&bars);

        for value in stoch.values() {
            match value {
                IndicatorValue::Multiple(vals) => {
                    assert_eq!(vals.len(), 2, "Should have %K and %D");
                }
                IndicatorValue::None => {} // Expected for early bars
                _ => panic!("Unexpected value type"),
            }
        }
    }

    #[test]
    fn test_stochastic_line_cnt() {
        let stoch = Stochastic::new(14, 3, 3);
        assert_eq!(stoch.line_cnt(), 2);
    }

    #[test]
    fn test_stochastic_empty_data() {
        let mut stoch = Stochastic::new(14, 3, 3);
        stoch.calculate(&[]);
        assert!(stoch.values().is_empty());
    }
}
