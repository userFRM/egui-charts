//! Bull Bear Power
//!
//! Alexander Elder's bull and bear power, summed into one histogram: how far the bar's high and
//! low reach, together, above or below the exponential average of the close.
//!
//! # Formula
//! ```text
//! Bull Power = high - EMA(close, length)
//! Bear Power = low - EMA(close, length)
//! Bull Bear Power = Bull Power + Bear Power
//! ```
//! with conventional length 13. The EMA is that of [`ElderRay`]: alpha 2 / (length + 1), seeded
//! with the mean of the first `length` closes, so the first value is on bar `length - 1`.
//!
//! # Presentation
//! Columns, green at or above zero and red below.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{BullBearPower, Indicator};
//!
//! let mut power = BullBearPower::new(13);
//! power.calculate(&bars);
//! assert!(power.values().value(0, 12).is_some());
//! ```

use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Bull Power plus Bear Power.
#[derive(Clone)]
pub struct BullBearPower {
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<Smoothing>,
    positive_color: Color32,
    negative_color: Color32,
    visible: bool,
}

impl BullBearPower {
    /// Create a Bull Bear Power over an EMA of `period` closes (conventionally 13).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            positive_color: palette::TEAL,
            negative_color: palette::RED,
            visible: true,
        }
    }
}

impl Default for BullBearPower {
    fn default() -> Self {
        Self::new(13)
    }
}

impl Indicator for BullBearPower {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX)
    ; {});

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.positive_color,
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        let value = self.values.value(0, bar)?;
        Some(if value < 0.0 {
            self.negative_color
        } else {
            self.positive_color
        })
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "Bull Bear Power"
    }

    fn desc(&self) -> &str {
        "Bull Bear Power - High and low against the EMA of the close"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                let ema = state.next(bar.close, self.period, 2.0 / (self.period as f64 + 1.0));
                let (bull, bear) = (bar.high - ema, bar.low - ema);
                if bull.is_finite() && bear.is_finite() {
                    IndicatorValue::Single(bull + bear)
                } else {
                    IndicatorValue::None
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
        vec![self.positive_color, self.negative_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.positive_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.negative_color = color;
        }
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn line_count(&self) -> usize {
        1
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
        vec![format!("BBP({})", self.period)]
    }
}
