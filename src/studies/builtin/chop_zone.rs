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

use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

const EMA_LENGTH: usize = 34;
const RANGE_LENGTH: usize = 30;

/// Chop Zone: unit columns coloured by the angle of the 34-bar EMA.
#[derive(Clone)]
pub struct ChopZone {
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// The angle in whole degrees at each source bar.
    angles: IndicatorValues,
    /// Band colours from the steepest rise to the steepest fall.
    colors: [Color32; 9],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    count: usize,
    prices: std::collections::VecDeque<[f64; 2]>,
    ema: Smoothing,
    previous: f64,
}

impl ChopZone {
    /// Create the Chop Zone; its lengths are fixed by the definition.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            angles: IndicatorValues::new(),
            colors: [
                palette::CYAN,
                palette::DARK_GREEN,
                palette::LIGHT_GREEN,
                palette::DEEP_TEAL,
                palette::YELLOW,
                palette::LIGHT_ORANGE,
                palette::ORANGE,
                palette::PINK,
                palette::DEEP_RED,
            ],
            visible: true,
        }
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

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(0)
    }

    fn name(&self) -> &str {
        "Chop Zone"
    }

    fn desc(&self) -> &str {
        "Chop Zone - Trend and chop by the angle of an EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.angles, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                state.count += 1;
                state.prices.push_back([bar.high, bar.low]);
                if state.prices.len() > RANGE_LENGTH {
                    state.prices.pop_front();
                }
                let ema = state
                    .ema
                    .next(bar.close, EMA_LENGTH, 2.0 / (EMA_LENGTH as f64 + 1.0));
                let previous = std::mem::replace(&mut state.previous, ema);
                if state.count <= RANGE_LENGTH.max(EMA_LENGTH) {
                    return IndicatorValue::None;
                }
                let high = state
                    .prices
                    .iter()
                    .map(|v| v[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = state
                    .prices
                    .iter()
                    .map(|v| v[1])
                    .fold(f64::INFINITY, f64::min);
                let range = high - low;
                if range == 0.0 {
                    return IndicatorValue::None;
                }
                let span = 25.0 / range * low;
                let hlc3 = (bar.high + bar.low + bar.close) / 3.0;
                let y = (previous - ema) / hlc3 * span;
                let angle = y.abs().atan().to_degrees().round();
                IndicatorValue::Single(if y > 0.0 { -angle } else { angle })
            });
        let start = from.min(self.values.len()).min(data.len());
        self.values.truncate(start);
        for i in start..data.len() {
            self.values.push(if self.angles.value(0, i).is_some() {
                IndicatorValue::Single(1.0)
            } else {
                IndicatorValue::None
            });
        }
    }

    fn is_incremental(&self) -> bool {
        true
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
