//! Stochastic RSI Indicator
//!
//! Stochastic RSI applies the Stochastic formula to RSI values instead of
//! price, creating a more sensitive oscillator for identifying overbought
//! and oversold conditions.
//!
//! # Formula
//! StochRSI = (RSI - Lowest RSI) / (Highest RSI - Lowest RSI)
//! %K = SMA(StochRSI, smoothK)
//! %D = SMA(%K, smoothD)
//!
//! # Interpretation
//! - Above 0.8: Overbought
//! - Below 0.2: Oversold
//! - %K crossing %D: Trading signals
//!
//! # Example
//! ```ignore
//! use egui_charts::StochasticRSI;
//!
//! let mut stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
//! stoch_rsi.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Stochastic RSI indicator
#[derive(Clone)]
pub struct StochasticRSI {
    rsi_period: usize,
    stoch_period: usize,
    smooth_k: usize,
    smooth_d: usize,
    values: Vec<IndicatorValue>,
    k_color: Color32,
    d_color: Color32,
    visible: bool,
}

impl StochasticRSI {
    /// Create a new Stochastic RSI indicator
    ///
    /// # Arguments
    /// * `rsi_period` - RSI calculation period (typically 14)
    /// * `stoch_period` - Stochastic lookback period (typically 14)
    /// * `smooth_k` - %K smoothing period (typically 3)
    /// * `smooth_d` - %D smoothing period (typically 3)
    pub fn new(rsi_period: usize, stoch_period: usize, smooth_k: usize, smooth_d: usize) -> Self {
        Self {
            rsi_period: rsi_period.max(1),
            stoch_period: stoch_period.max(1),
            smooth_k: smooth_k.max(1),
            smooth_d: smooth_d.max(1),
            values: Vec::new(),
            k_color: DESIGN_TOKENS.semantic.extended.info,
            d_color: DESIGN_TOKENS.semantic.extended.error,
            visible: true,
        }
    }

    /// Create with default params (14, 14, 3, 3)
    pub fn default_params() -> Self {
        Self::new(14, 14, 3, 3)
    }

    /// Set colors for %K and %D lines
    pub fn with_colors(mut self, k_color: Color32, d_color: Color32) -> Self {
        self.k_color = k_color;
        self.d_color = d_color;
        self
    }

    /// Calculate RSI values
    fn calculate_rsi(data: &[Bar], period: usize) -> Vec<f64> {
        let mut rsi = vec![f64::NAN; data.len()];
        if data.len() < period + 1 {
            return rsi;
        }

        let mut gains = vec![0.0; data.len()];
        let mut losses = vec![0.0; data.len()];

        // Calculate gains and losses
        for i in 1..data.len() {
            let change = data[i].close - data[i - 1].close;
            if change > 0.0 {
                gains[i] = change;
            } else {
                losses[i] = -change;
            }
        }

        // First avg
        let mut avg_gain: f64 = gains[1..=period].iter().sum::<f64>() / period as f64;
        let mut avg_loss: f64 = losses[1..=period].iter().sum::<f64>() / period as f64;

        if avg_loss.abs() < 1e-10 {
            rsi[period] = 100.0;
        } else {
            let rs = avg_gain / avg_loss;
            rsi[period] = 100.0 - (100.0 / (1.0 + rs));
        }

        // Subsequent values using Wilder's smoothing
        for i in (period + 1)..data.len() {
            avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;

            if avg_loss.abs() < 1e-10 {
                rsi[i] = 100.0;
            } else {
                let rs = avg_gain / avg_loss;
                rsi[i] = 100.0 - (100.0 / (1.0 + rs));
            }
        }

        rsi
    }

    /// Calculate SMA for a slice
    fn sma(data: &[f64], start: usize, period: usize) -> f64 {
        if start + 1 < period {
            return f64::NAN;
        }
        let begin = start + 1 - period;
        let sum: f64 = data[begin..=start].iter().filter(|x| !x.is_nan()).sum();
        let count = data[begin..=start].iter().filter(|x| !x.is_nan()).count();
        if count == period {
            sum / period as f64
        } else {
            f64::NAN
        }
    }
}

impl Indicator for StochasticRSI {
    fn name(&self) -> &str {
        "StochRSI"
    }

    fn desc(&self) -> &str {
        "Stochastic RSI - Stochastic oscillator of RSI"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let min_period = self.rsi_period + self.stoch_period + self.smooth_k + self.smooth_d;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Step 1: Calculate RSI
        let rsi = Self::calculate_rsi(data, self.rsi_period);

        // Step 2: Calculate Stochastic of RSI
        let mut stoch_rsi = vec![f64::NAN; data.len()];
        for i in (self.rsi_period + self.stoch_period - 1)..data.len() {
            let start = i + 1 - self.stoch_period;
            let mut min_rsi = f64::INFINITY;
            let mut max_rsi = f64::NEG_INFINITY;

            for j in start..=i {
                if !rsi[j].is_nan() {
                    min_rsi = min_rsi.min(rsi[j]);
                    max_rsi = max_rsi.max(rsi[j]);
                }
            }

            if (max_rsi - min_rsi).abs() < 1e-10 {
                stoch_rsi[i] = 0.5; // Middle when range is zero
            } else {
                stoch_rsi[i] = (rsi[i] - min_rsi) / (max_rsi - min_rsi);
            }
        }

        // Step 3: Calculate %K (SMA of StochRSI)
        let mut k_line = vec![f64::NAN; data.len()];
        for i in 0..data.len() {
            k_line[i] = Self::sma(&stoch_rsi, i, self.smooth_k);
        }

        // Step 4: Calculate %D (SMA of %K)
        let mut d_line = vec![f64::NAN; data.len()];
        for i in 0..data.len() {
            d_line[i] = Self::sma(&k_line, i, self.smooth_d);
        }

        // Build output
        for i in 0..data.len() {
            if k_line[i].is_nan() || d_line[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Multiple(vec![
                    k_line[i] * 100.0, // Scale to 0-100
                    d_line[i] * 100.0,
                ]));
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
        false // StochRSI is a separate oscillator pane
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
            format!(
                "%K({}, {}, {})",
                self.rsi_period, self.stoch_period, self.smooth_k
            ),
            format!("%D({})", self.smooth_d),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..60)
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
    fn test_stoch_rsi_calculation() {
        let bars = create_test_bars();
        let mut stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
        stoch_rsi.calculate(&bars);

        assert_eq!(stoch_rsi.values().len(), bars.len());

        // Check that we eventually get valid values
        let valid_cnt = stoch_rsi
            .values()
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Multiple(_)))
            .count();
        assert!(valid_cnt > 0, "Should have some valid values");
    }

    #[test]
    fn test_stoch_rsi_range() {
        let bars = create_test_bars();
        let mut stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
        stoch_rsi.calculate(&bars);

        for value in stoch_rsi.values() {
            if let IndicatorValue::Multiple(vals) = value {
                assert!(vals[0] >= 0.0 && vals[0] <= 100.0, "%K should be 0-100");
                assert!(vals[1] >= 0.0 && vals[1] <= 100.0, "%D should be 0-100");
            }
        }
    }

    #[test]
    fn test_stoch_rsi_line_cnt() {
        let stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
        assert_eq!(stoch_rsi.line_cnt(), 2);
    }

    #[test]
    fn test_stoch_rsi_is_not_overlay() {
        let stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
        assert!(!stoch_rsi.is_overlay());
    }

    #[test]
    fn test_stoch_rsi_empty_data() {
        let mut stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
        stoch_rsi.calculate(&[]);
        assert!(stoch_rsi.values().is_empty());
    }
}
