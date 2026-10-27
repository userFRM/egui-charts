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

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
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
    stepper: Stepper<State>,
    k_color: Color32,
    d_color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<f64>,
    count: usize,
    gain: f64,
    loss: f64,
    rsi: std::collections::VecDeque<f64>,
    k: std::collections::VecDeque<f64>,
    d: std::collections::VecDeque<f64>,
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
            stepper: Stepper::default(),
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let Some(previous) = state.previous.replace(bar.close) else {
                    return IndicatorValue::None;
                };
                let change = bar.close - previous;
                state.count += 1;
                let gain = change.max(0.0);
                let loss = (-change).max(0.0);
                if state.count <= self.rsi_period {
                    state.gain += gain;
                    state.loss += loss;
                    if state.count < self.rsi_period {
                        return IndicatorValue::None;
                    }
                    state.gain /= self.rsi_period as f64;
                    state.loss /= self.rsi_period as f64;
                } else {
                    state.gain =
                        (state.gain * (self.rsi_period - 1) as f64 + gain) / self.rsi_period as f64;
                    state.loss =
                        (state.loss * (self.rsi_period - 1) as f64 + loss) / self.rsi_period as f64;
                }
                let rsi = if state.loss.abs() < 1e-10 {
                    100.0
                } else {
                    100.0 - 100.0 / (1.0 + state.gain / state.loss)
                };
                state.rsi.push_back(rsi);
                if state.rsi.len() > self.stoch_period {
                    state.rsi.pop_front();
                }
                if state.rsi.len() < self.stoch_period {
                    return IndicatorValue::None;
                }
                let low = state.rsi.iter().copied().fold(f64::INFINITY, f64::min);
                let high = state.rsi.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let stoch = if (high - low).abs() < 1e-10 {
                    0.5
                } else {
                    (rsi - low) / (high - low)
                };
                state.k.push_back(stoch);
                if state.k.len() > self.smooth_k {
                    state.k.pop_front();
                }
                if state.k.len() < self.smooth_k {
                    return IndicatorValue::None;
                }
                let k = state.k.iter().sum::<f64>() / self.smooth_k as f64;
                state.d.push_back(k);
                if state.d.len() > self.smooth_d {
                    state.d.pop_front();
                }
                if state.d.len() < self.smooth_d {
                    return IndicatorValue::None;
                }
                let d = state.d.iter().sum::<f64>() / self.smooth_d as f64;
                if k.is_nan() || d.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![k * 100.0, d * 100.0])
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
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

    fn line_count(&self) -> usize {
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
