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

use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Parabolic SAR indicator
#[derive(Clone)]
pub struct ParabolicSAR {
    initial_af: f64,
    af_step: f64,
    max_af: f64,
    values: IndicatorValues,
    stepper: Stepper<State>,
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
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            bullish_color: palette::BLUE,
            bearish_color: palette::BLUE,
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

#[derive(Clone, Default)]
struct State {
    previous: Option<[f64; 3]>,
    before_previous: Option<[f64; 2]>,
    sar: f64,
    ep: f64,
    af: f64,
    up: bool,
}

impl Indicator for ParabolicSAR {
    crate::studies::inputs::study_inputs!(self;
        initial_af: number("Initial af", 0.001, 1.0),
        af_step: number("Af step", 0.001, 1.0),
        max_af: number("Max af", 0.001, 1.0)
    ; { self.max_af = self.max_af.max(self.initial_af); });

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors()[line],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Crosses,
            visible: true,
        }
    }

    fn name(&self) -> &str {
        "PSAR"
    }

    fn desc(&self) -> &str {
        "Parabolic SAR - Trend following stop-and-reverse"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() || !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let current = [bar.high, bar.low, bar.close];
                let Some(previous) = state.previous.replace(current) else {
                    return IndicatorValue::None;
                };
                if let Some(before) = state.before_previous {
                    let mut next = state.sar + state.af * (state.ep - state.sar);
                    let reversal = if state.up {
                        next = next.min(previous[1]).min(before[1]);
                        bar.low < next
                    } else {
                        next = next.max(previous[0]).max(before[0]);
                        bar.high > next
                    };
                    if reversal {
                        state.up = !state.up;
                        state.sar = state.ep;
                        state.ep = if state.up { bar.high } else { bar.low };
                        state.af = self.initial_af;
                    } else {
                        state.sar = next;
                        if (state.up && bar.high > state.ep) || (!state.up && bar.low < state.ep) {
                            state.ep = if state.up { bar.high } else { bar.low };
                            state.af = (state.af + self.af_step).min(self.max_af);
                        }
                    }
                } else {
                    state.up = bar.close >= previous[2];
                    state.sar = if state.up { previous[1] } else { previous[0] };
                    state.ep = if state.up {
                        bar.high.max(previous[0])
                    } else {
                        bar.low.min(previous[1])
                    };
                    state.af = self.initial_af;
                }
                state.before_previous = Some([previous[0], previous[1]]);
                IndicatorValue::Multiple(if state.up {
                    vec![state.sar, f64::NAN]
                } else {
                    vec![f64::NAN, state.sar]
                })
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
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
