//! Chop Zone
//!
//! The Chop Zone colours a column on every bar by the angle of a 34-bar EMA of the close, so
//! steep slopes (trends) and flat stretches (chop) stand out at a glance.
//!
//! # Formula
//! - span = 25 / (highest high - lowest low over 30 bars) * lowest low
//! - y = (EMA(close, 34)[1] - EMA(close, 34)) / hlc3 * span
//! - angle = atan(|y|) in degrees, rounded to a whole degree, negative when the EMA fell (y > 0)
//!
//! The EMA starts with the simple mean of its first 34 closes. Every column has the value 1;
//! the colour carries the reading:
//!
//! | angle | colour |
//! |---|---|
//! | 5 or more | turquoise |
//! | 3.57 to 5 | dark green |
//! | 2.14 to 3.57 | pale green |
//! | 0.71 to 2.14 | lime |
//! | between -0.71 and 0.71 | yellow |
//! | -2.14 to -0.71 | light orange |
//! | -3.57 to -2.14 | orange |
//! | -5 to -3.57 | red |
//! | -5 or less | dark red |
//!
//! A bar whose 30-bar range is zero has no value.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ChopZone, Indicator};
//!
//! let mut zone = ChopZone::new();
//! zone.calculate(&bars);
//! assert!(zone.plot_color(0, 59).is_some());
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::stepper::{rolling_max, rolling_min};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

const EMA_LENGTH: usize = 34;
const RANGE_LENGTH: usize = 30;

/// Chop Zone: unit columns coloured by the angle of the 34-bar EMA.
#[derive(Clone)]
pub struct ChopZone {
    values: IndicatorValues,
    /// The angle in whole degrees at each source bar.
    angles: IndicatorValues,
    /// Band colours from the steepest rise to the steepest fall.
    colors: [Color32; 9],
    visible: bool,
}

impl ChopZone {
    /// Create the Chop Zone; its lengths are fixed by the definition.
    pub fn new() -> Self {
        let palette = &DESIGN_TOKENS.semantic.extended;
        Self {
            values: IndicatorValues::new(),
            angles: IndicatorValues::new(),
            colors: [
                palette.cyan,
                palette.success_dark,
                palette.success_light,
                palette.teal,
                palette.caution,
                palette.favorite_gold,
                palette.warning,
                palette.pink,
                palette.error_darker,
            ],
            visible: true,
        }
    }

    /// The angles over `data`, every bar of which has a finite high, low and close.
    fn compute(data: &[Bar]) -> IndicatorValues {
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();
        let ema = helpers::ema_sma(&closes, EMA_LENGTH);
        let highest = rolling_max(data.iter().map(|b| b.high), RANGE_LENGTH);
        let lowest = rolling_min(data.iter().map(|b| b.low), RANGE_LENGTH);
        (0..data.len())
            .map(|i| {
                let range = highest[i] - lowest[i];
                if i < RANGE_LENGTH.max(EMA_LENGTH) || range == 0.0 {
                    return IndicatorValue::None;
                }
                let span = 25.0 / range * lowest[i];
                let hlc3 = (data[i].high + data[i].low + data[i].close) / 3.0;
                let y = (ema[i - 1] - ema[i]) / hlc3 * span;
                let angle = y.abs().atan().to_degrees().round();
                IndicatorValue::Single(if y > 0.0 { -angle } else { angle })
            })
            .collect()
    }
}

/// Construct with the conventional definition.
impl Default for ChopZone {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ChopZone {
    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[0],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        let angle = self.angles.value(0, bar)?;
        let band = match angle {
            a if a >= 5.0 => 0,
            a if a >= 3.57 => 1,
            a if a >= 2.14 => 2,
            a if a >= 0.71 => 3,
            a if a <= -5.0 => 8,
            a if a <= -3.57 => 7,
            a if a <= -2.14 => 6,
            a if a <= -0.71 => 5,
            _ => 4,
        };
        Some(self.colors[band])
    }

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Fixed(0)
    }

    fn name(&self) -> &str {
        "Chop Zone"
    }

    fn desc(&self) -> &str {
        "Chop Zone - Trend and chop by the angle of an EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.angles = over_finite_bars(
            data,
            |b| [b.high, b.low, b.close].iter().all(|p| p.is_finite()),
            Self::compute,
        );
        self.values = (0..data.len())
            .map(|bar| match self.angles.value(0, bar) {
                Some(_) => IndicatorValue::Single(1.0),
                None => IndicatorValue::None,
            })
            .collect();
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }

    fn is_overlay(&self) -> bool {
        false
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
        vec!["Chop Zone".into()]
    }
}
