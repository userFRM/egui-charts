//! Chande Kroll Stop
//!
//! Tushar Chande and Stanley Kroll (The New Technical Trader, 1994) place trailing stops a
//! multiple of the average true range beyond the recent extremes, then take the extreme of
//! those preliminary stops so that a stop only moves after the market has.
//!
//! # Formula
//! - first high stop = highest(high, p) - x * ATR(p)
//! - first low stop = lowest(low, p) + x * ATR(p)
//! - stop short = highest(first high stop, q)
//! - stop long = lowest(first low stop, q)
//!
//! ATR is Wilder's average true range. The defaults are p = 10, x = 1 and q = 9, so the
//! first stops appear on bar p + q - 2, counting from 0. The short stop is drawn red, the long stop green.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ChandeKrollStop, Indicator};
//!
//! let mut stops = ChandeKrollStop::new(10, 1.0, 9);
//! stops.calculate(&bars);
//! ```

use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Chande Kroll Stop: a short (upper) and a long (lower) trailing stop.
#[derive(Clone)]
pub struct ChandeKrollStop {
    atr_length: usize,
    multiplier: f64,
    stop_length: usize,
    values: IndicatorValues,
    atr: ATR,
    stepper: Stepper<State>,
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    index: usize,
    count: usize,
    prices: std::collections::VecDeque<[f64; 2]>,
    stops: std::collections::VecDeque<[f64; 2]>,
}

impl ChandeKrollStop {
    /// Create the stops from the ATR and extreme length `p` (typically 10), the ATR multiplier
    /// `x` (typically 1, clamped into 0 to `f64::MAX`, NaN to 0) and the stop length `q`
    /// (typically 9).
    pub fn new(atr_length: usize, multiplier: f64, stop_length: usize) -> Self {
        Self {
            atr_length: atr_length.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            stop_length: stop_length.max(1),
            values: IndicatorValues::new(),
            atr: ATR::new(atr_length),
            stepper: Stepper::default(),
            colors: [palette::ORANGE, palette::BLUE],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for ChandeKrollStop {
    fn default() -> Self {
        Self::new(10, 1.0, 9)
    }
}

impl Indicator for ChandeKrollStop {
    crate::studies::inputs::study_inputs!(self;
        atr_length: length("ATR length", 1, usize::MAX),
        multiplier: number("ATR coefficient", 0.0, f64::MAX),
        stop_length: length("Stop length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Chande Kroll Stop"
    }

    fn desc(&self) -> &str {
        "Chande Kroll Stop - ATR trailing stops beyond recent extremes"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.atr = ATR::new(self.atr_length);
        }
        self.atr.update(data, from);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.index;
                state.index += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                state.count += 1;
                state.prices.push_back([bar.high, bar.low]);
                if state.prices.len() > self.atr_length {
                    state.prices.pop_front();
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
                let band = self
                    .atr
                    .values()
                    .value(0, index)
                    .map(|atr| self.multiplier * atr);
                state.stops.push_back([
                    band.map_or(f64::NAN, |band| high - band),
                    band.map_or(f64::NAN, |band| low + band),
                ]);
                if state.stops.len() > self.stop_length {
                    state.stops.pop_front();
                }
                if state.count - 1 < (self.atr_length - 1).saturating_add(self.stop_length - 1) {
                    return IndicatorValue::None;
                }
                let short = state
                    .stops
                    .iter()
                    .map(|v| v[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                let long = state
                    .stops
                    .iter()
                    .map(|v| v[1])
                    .fold(f64::INFINITY, f64::min);
                IndicatorValue::Multiple(vec![short, long])
            });
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
        vec!["Stop Short".into(), "Stop Long".into()]
    }
}
