//! Parabolic SAR (Stop and Reverse) Indicator
//!
//! Developed by J. Welles Wilder Jr., the Parabolic SAR provides potential
//! entry and exit points by plotting a trailing stop-and-reverse point.
//!
//! # Formula
//! SARn+1 = SARn + AF * (EP - SARn)
//! Where:
//! - AF = Acceleration Factor (starts at initial_af, increases by af_step up to max_af)
//! - EP = Extreme Point (highest high in uptrend, lowest low in downtrend)
//!
//! # Interpretation
//! - SAR below price: Uptrend (SAR acts as support/trailing stop)
//! - SAR above price: Downtrend (SAR acts as resistance/trailing stop)
//! - When price crosses SAR: Trend reversal signal
//!
//! The SAR is output as two lines, the uptrend SAR and the downtrend SAR; each bar has a
//! value on exactly one of them, the other is NaN.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ParabolicSAR, Indicator};
//!
//! let mut psar = ParabolicSAR::new(0.02, 0.02, 0.2);
//! psar.calculate(&bars);
//! ```

use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Parabolic SAR indicator
#[derive(Clone)]
pub struct ParabolicSAR {
    initial_af: f64,
    af_step: f64,
    max_af: f64,
    values: Vec<IndicatorValue>,
    bullish_color: Color32,
    bearish_color: Color32,
    visible: bool,
}

impl ParabolicSAR {
    /// Create a new Parabolic SAR indicator
    ///
    /// # Arguments
    /// * `initial_af` - Starting acceleration factor (typically 0.02)
    /// * `af_step` - AF increment (typically 0.02)
    /// * `max_af` - Max AF (typically 0.2)
    ///
    /// `initial_af` and `af_step` are clamped into 0.001 to 1, NaN to 0.001, and
    /// `max_af` into `initial_af` to 1, NaN to `initial_af`.
    pub fn new(initial_af: f64, af_step: f64, max_af: f64) -> Self {
        let initial_af = clamp_param(initial_af, 0.001, 1.0);
        Self {
            initial_af,
            af_step: clamp_param(af_step, 0.001, 1.0),
            max_af: clamp_param(max_af, initial_af, 1.0),
            values: Vec::new(),
            bullish_color: DESIGN_TOKENS.semantic.extended.bullish,
            bearish_color: DESIGN_TOKENS.semantic.extended.bearish,
            visible: true,
        }
    }

    /// Create with default params (0.02, 0.02, 0.2)
    pub fn default_params() -> Self {
        Self::new(0.02, 0.02, 0.2)
    }

    /// Set colors for bullish and bearish SAR points
    pub fn with_colors(mut self, bullish: Color32, bearish: Color32) -> Self {
        self.bullish_color = bullish;
        self.bearish_color = bearish;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ParabolicSAR {
    fn default() -> Self {
        Self::new(0.02, 0.02, 0.2)
    }
}

impl ParabolicSAR {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();
        if data.len() < 2 {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        let mut sar = vec![0.0; data.len()];
        let mut trend = vec![1i32; data.len()]; // 1 = uptrend, -1 = downtrend
        let mut ep = vec![0.0; data.len()]; // Extreme point
        let mut af = vec![self.initial_af; data.len()];

        // Initialize - determine initial trend from first two bars
        if data[1].close >= data[0].close {
            // Start in uptrend
            trend[0] = 1;
            trend[1] = 1;
            sar[0] = data[0].low;
            sar[1] = data[0].low;
            ep[0] = data[0].high;
            ep[1] = data[1].high.max(ep[0]);
        } else {
            // Start in downtrend
            trend[0] = -1;
            trend[1] = -1;
            sar[0] = data[0].high;
            sar[1] = data[0].high;
            ep[0] = data[0].low;
            ep[1] = data[1].low.min(ep[0]);
        }

        for i in 2..data.len() {
            let prev_trend = trend[i - 1];
            let prev_sar = sar[i - 1];
            let prev_ep = ep[i - 1];
            let prev_af = af[i - 1];

            // Calculate potential next SAR
            let mut next_sar = prev_sar + prev_af * (prev_ep - prev_sar);

            // Check for reversal
            let reversal = if prev_trend == 1 {
                // In uptrend, reversal if price falls below SAR
                // Also ensure SAR doesn't go above prior two lows
                next_sar = next_sar.min(data[i - 1].low).min(data[i - 2].low);
                data[i].low < next_sar
            } else {
                // In downtrend, reversal if price rises above SAR
                // Also ensure SAR doesn't go below prior two highs
                next_sar = next_sar.max(data[i - 1].high).max(data[i - 2].high);
                data[i].high > next_sar
            };

            if reversal {
                // Trend reversal
                trend[i] = -prev_trend;
                sar[i] = prev_ep;
                ep[i] = if trend[i] == 1 {
                    data[i].high
                } else {
                    data[i].low
                };
                af[i] = self.initial_af;
            } else {
                // Continue trend
                trend[i] = prev_trend;
                sar[i] = next_sar;

                // Update EP and AF
                if trend[i] == 1 {
                    if data[i].high > prev_ep {
                        ep[i] = data[i].high;
                        af[i] = (prev_af + self.af_step).min(self.max_af);
                    } else {
                        ep[i] = prev_ep;
                        af[i] = prev_af;
                    }
                } else if data[i].low < prev_ep {
                    ep[i] = data[i].low;
                    af[i] = (prev_af + self.af_step).min(self.max_af);
                } else {
                    ep[i] = prev_ep;
                    af[i] = prev_af;
                }
            }
        }

        // Build output: [uptrend SAR, downtrend SAR]
        for i in 0..data.len() {
            if i == 0 {
                values.push(IndicatorValue::None);
            } else if trend[i] == 1 {
                values.push(IndicatorValue::Multiple(vec![sar[i], f64::NAN]));
            } else {
                values.push(IndicatorValue::Multiple(vec![f64::NAN, sar[i]]));
            }
        }

        values
    }
}

impl Indicator for ParabolicSAR {
    fn name(&self) -> &str {
        "PSAR"
    }

    fn desc(&self) -> &str {
        "Parabolic SAR - Trend following stop-and-reverse"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.bullish_color, self.bearish_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.bullish_color = colors[0];
        }
        if colors.len() > 1 {
            self.bearish_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        true // PSAR is drawn on main chart
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
        vec!["Up Trend".to_string(), "Down Trend".to_string()]
    }
}
