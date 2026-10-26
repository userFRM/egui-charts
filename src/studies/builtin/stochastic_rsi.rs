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
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{StochasticRSI, Indicator};
//!
//! let mut stoch_rsi = StochasticRSI::new(14, 14, 3, 3);
//! stoch_rsi.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Stochastic RSI indicator
#[derive(Clone)]
pub struct StochasticRSI {
    rsi_period: usize,
    stoch_period: usize,
    smooth_k: usize,
    smooth_d: usize,
    values: IndicatorValues,
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
            values: IndicatorValues::new(),
            k_color: palette::BLUE,
            d_color: palette::RED,
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

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.is_empty() {
            return values;
        }

        let min_period = self
            .rsi_period
            .saturating_add(self.stoch_period)
            .saturating_add(self.smooth_k)
            .saturating_add(self.smooth_d)
            .saturating_sub(2);
        if data.len() < min_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
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

        let k_line = helpers::sma(&stoch_rsi, self.smooth_k);
        let d_line = helpers::sma(&k_line, self.smooth_d);

        // Build output
        for i in 0..data.len() {
            if k_line[i].is_nan() || d_line[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[
                    k_line[i] * 100.0, // Scale to 0-100
                    d_line[i] * 100.0,
                ]);
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for StochasticRSI {
    fn default() -> Self {
        Self::new(14, 14, 3, 3)
    }
}

impl Indicator for StochasticRSI {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        rsi_period: length("RSI period", 1, usize::MAX),
        stoch_period: length("Stoch period", 1, usize::MAX),
        smooth_k: length("Smooth k", 1, usize::MAX),
        smooth_d: length("Smooth d", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "StochRSI"
    }

    fn desc(&self) -> &str {
        "Stochastic RSI - Stochastic oscillator of RSI"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
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
