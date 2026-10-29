//! Keltner Channels Indicator
//!
//! Keltner Channels are volatility-based bands set above and below an EMA.
//! The bands are based on ATR (Avg True Range).
//!
//! # Components
//! - Middle Line: EMA of close price
//! - Upper Band: EMA + (ATR multiplier * ATR)
//! - Lower Band: EMA - (ATR multiplier * ATR)
//!
//! # Interpretation
//! - Price above upper band: Strong uptrend or overbought
//! - Price below lower band: Strong downtrend or oversold
//! - Channel squeeze (narrow bands): Low volatility, potential breakout
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{KeltnerChannels, Indicator};
//!
//! let mut keltner = KeltnerChannels::new(20, 10, 2.0);
//! keltner.calculate(&bars);
//! ```

use super::clamp_param;
use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Keltner Channels indicator
#[derive(Clone)]
pub struct KeltnerChannels {
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
    values: IndicatorValues,
    stepper: Stepper<State>,
    upper_color: Color32,
    middle_color: Color32,
    lower_color: Color32,
    fill_color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<f64>,
    ema: Smoothing,
    atr: f64,
    count: usize,
}

impl KeltnerChannels {
    /// Create a new Keltner Channels indicator
    ///
    /// # Arguments
    /// * `ema_period` - EMA period for middle line (typically 20)
    /// * `atr_period` - ATR period (typically 10)
    /// * `multiplier` - ATR multiplier (typically 2.0), clamped into 0 to `f64::MAX`, NaN to 0
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period: ema_period.max(1),
            atr_period: atr_period.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            upper_color: palette::BLUE,
            middle_color: palette::BLUE,
            lower_color: palette::BLUE,
            fill_color: palette::LIGHT_BLUE.gamma_multiply_u8(13),
            visible: true,
        }
    }

    /// Create with default params (20, 10, 2.0)
    pub fn default_params() -> Self {
        Self::new(20, 10, 2.0)
    }

    /// Set colors for upper, middle, and lower bands
    pub fn with_colors(mut self, upper: Color32, middle: Color32, lower: Color32) -> Self {
        self.upper_color = upper;
        self.middle_color = middle;
        self.lower_color = lower;
        self.fill_color = Color32::from_rgba_unmultiplied(upper.r(), upper.g(), upper.b(), 16);
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for KeltnerChannels {
    fn default() -> Self {
        Self::new(20, 10, 2.0)
    }
}

impl Indicator for KeltnerChannels {
    crate::studies::inputs::study_inputs!(self;
        ema_period: length("EMA period", 1, usize::MAX),
        atr_period: length("ATR period", 1, usize::MAX),
        multiplier: number("Multiplier", 0.0, f64::MAX)
    ; {});

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(1),
            second: crate::studies::FillBoundary::Plot(2),
            color: self.fill_color,
            inverted_color: None,
        }]
    }

    fn name(&self) -> &str {
        "Keltner"
    }

    fn desc(&self) -> &str {
        "Keltner Channels - Volatility-based envelope"
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
                let tr = state
                    .previous
                    .map_or(bar.high - bar.low, |close| ATR::true_range(bar, close));
                state.previous = Some(bar.close);
                let middle = state.ema.next(
                    bar.close,
                    self.ema_period,
                    2.0 / (self.ema_period as f64 + 1.0),
                );
                state.count += 1;
                if state.count <= self.atr_period {
                    state.atr += tr;
                    if state.count == self.atr_period {
                        state.atr /= self.atr_period as f64;
                    }
                } else {
                    state.atr += (1.0 / self.atr_period as f64) * (tr - state.atr);
                }
                if state.count < self.ema_period.max(self.atr_period) {
                    return IndicatorValue::None;
                }
                let width = self.multiplier * state.atr;
                IndicatorValue::Multiple(vec![middle, middle + width, middle - width])
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![
            self.middle_color,
            self.upper_color,
            self.lower_color,
            self.fill_color,
        ]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.middle_color = colors[0];
        }
        if colors.len() > 1 {
            self.upper_color = colors[1];
        }
        if colors.len() > 2 {
            self.lower_color = colors[2];
        }
        if let Some(&color) = colors.get(3) {
            self.fill_color = color;
        } else if colors.len() > 1 {
            let color = self.upper_color;
            self.fill_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 16);
        }
    }

    fn is_overlay(&self) -> bool {
        true // Keltner is drawn on main chart
    }

    fn line_count(&self) -> usize {
        3 // Middle, Upper, Lower
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
            format!("EMA({})", self.ema_period),
            format!("Upper({}x ATR)", self.multiplier),
            format!("Lower({}x ATR)", self.multiplier),
        ]
    }
}
