//! Avg Directional Index (ADX) Indicator
//!
//! ADX measures trend strength regardless of direction. It's derived from the
//! Directional Movement Index (DMI) system which includes +DI and -DI.
//!
//! # Components
//! - ADX: Trend strength (0-100)
//! - +DI: Positive directional indicator
//! - -DI: Negative directional indicator
//!
//! # Interpretation
//! - ADX > 25: Strong trend
//! - ADX < 20: Weak or no trend
//! - +DI > -DI: Bullish trend
//! - -DI > +DI: Bearish trend
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ADX, Indicator};
//!
//! let mut adx = ADX::new(14);
//! adx.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Avg Directional Index indicator
#[derive(Clone)]
pub struct ADX {
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    adx_color: Color32,
    plus_di_color: Color32,
    minus_di_color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<[f64; 3]>,
    count: usize,
    plus: f64,
    minus: f64,
    tr: f64,
    dx_count: usize,
    adx: f64,
}

impl ADX {
    /// Create a new ADX indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            adx_color: palette::RED,
            plus_di_color: palette::BLUE,
            minus_di_color: palette::ORANGE,
            visible: true,
        }
    }

    /// Set colors for all three lines
    pub fn with_colors(mut self, adx: Color32, plus_di: Color32, minus_di: Color32) -> Self {
        self.adx_color = adx;
        self.plus_di_color = plus_di;
        self.minus_di_color = minus_di;
        self
    }

    /// Wilder's smoothing
    #[inline]
    fn wilders_smooth(prev: f64, current: f64, period: usize) -> f64 {
        prev - (prev / period as f64) + current
    }
}

/// Construct with the conventional default parameters.
impl Default for ADX {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ADX {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "ADX"
    }

    fn desc(&self) -> &str {
        "Avg Directional Index - Trend strength indicator"
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
                let Some(previous) = state.previous.replace([bar.high, bar.low, bar.close]) else {
                    return IndicatorValue::None;
                };
                state.count += 1;
                let up = bar.high - previous[0];
                let down = previous[1] - bar.low;
                let plus = if up > down && up > 0.0 { up } else { 0.0 };
                let minus = if down > up && down > 0.0 { down } else { 0.0 };
                let tr = ATR::true_range(bar, previous[2]);
                if state.count <= self.period {
                    state.plus += plus;
                    state.minus += minus;
                    state.tr += tr;
                    if state.count < self.period {
                        return IndicatorValue::None;
                    }
                } else {
                    state.plus = Self::wilders_smooth(state.plus, plus, self.period);
                    state.minus = Self::wilders_smooth(state.minus, minus, self.period);
                    state.tr = Self::wilders_smooth(state.tr, tr, self.period);
                }
                let (plus, minus) = if state.tr != 0.0 {
                    (
                        100.0 * state.plus / state.tr,
                        100.0 * state.minus / state.tr,
                    )
                } else {
                    (0.0, 0.0)
                };
                let sum = plus + minus;
                let dx = if sum.abs() > 1e-10 {
                    100.0 * (plus - minus).abs() / sum
                } else {
                    0.0
                };
                if state.dx_count < self.period {
                    state.dx_count += 1;
                    state.adx += dx;
                    if state.dx_count < self.period {
                        return IndicatorValue::Multiple(vec![f64::NAN, plus, minus]);
                    }
                    state.adx /= self.period as f64;
                } else {
                    state.adx = (state.adx * (self.period - 1) as f64 + dx) / self.period as f64;
                }
                IndicatorValue::Multiple(vec![state.adx, plus, minus])
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.adx_color, self.plus_di_color, self.minus_di_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.adx_color = colors[0];
        }
        if colors.len() > 1 {
            self.plus_di_color = colors[1];
        }
        if colors.len() > 2 {
            self.minus_di_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        false // ADX is drawn in separate pane
    }

    fn line_count(&self) -> usize {
        3 // ADX, +DI, -DI
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
            format!("ADX({})", self.period),
            "+DI".to_string(),
            "-DI".to_string(),
        ]
    }
}
