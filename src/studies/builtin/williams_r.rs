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
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{WilliamsR, Indicator};
//!
//! let mut williams = WilliamsR::new(14);
//! williams.calculate(&bars);
//! ```

use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Williams %R indicator
#[derive(Clone)]
pub struct WilliamsR {
    period: usize,
    smoothing: SmoothingType,
    smoothing_length: usize,
    smoothing_deviation: f64,
    values: IndicatorValues,
    stepper: Stepper<Rolling>,
    color: Color32,
    smoothing_colors: [Color32; 3],
    visible: bool,
}

/// The highs and lows of the last `period` bars with finite prices, and the average of %R.
#[derive(Clone, Default)]
struct Rolling {
    window: VecDeque<(f64, f64)>,
    smoothed: Smoothed,
}

impl WilliamsR {
    /// Create a new Williams %R indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            smoothing: SmoothingType::None,
            smoothing_length: 14,
            smoothing_deviation: 2.0,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PURPLE,
            smoothing_colors: SMOOTHING_COLORS,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for WilliamsR {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for WilliamsR {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smoothing: choice("Smoothing", SmoothingType::ALL),
        smoothing_length: length("Smoothing length", 1, usize::MAX)
            if smoothing in SmoothingType::WITH_LENGTH,
        smoothing_deviation: number("Band deviation", 0.0, f64::MAX)
            if smoothing in SmoothingType::WITH_BANDS
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "Williams %R"
    }

    fn desc(&self) -> &str {
        "Williams %R - Momentum oscillator (-100 to 0)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, kind, length, deviation) = (
            self.period,
            self.smoothing,
            self.smoothing_length,
            self.smoothing_deviation,
        );
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !(bar.high.is_finite() && bar.low.is_finite() && bar.close.is_finite()) {
                    return IndicatorValue::None;
                }
                state.window.push_back((bar.high, bar.low));
                if state.window.len() > period {
                    state.window.pop_front();
                }
                if state.window.len() < period {
                    return IndicatorValue::None;
                }
                let highest = state.window.iter().fold(f64::MIN, |max, w| max.max(w.0));
                let lowest = state.window.iter().fold(f64::MAX, |min, w| min.min(w.1));
                let range = highest - lowest;
                let r = if range == 0.0 {
                    -50.0 // Middle value
                } else {
                    ((highest - bar.close) / range * -100.0).clamp(-100.0, 0.0)
                };
                state.smoothed.next(kind, length, deviation, r, bar.volume)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        let [average, upper, lower] = self.smoothing_colors;
        vec![self.color, average, upper, lower]
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        self.smoothing.fills()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
            for (slot, color) in self.smoothing_colors.iter_mut().zip(&colors[1..]) {
                *slot = *color;
            }
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
        let mut names = vec![format!("%R({})", self.period)];
        names.extend(self.smoothing.line_names("%R MA"));
        names
    }
}
