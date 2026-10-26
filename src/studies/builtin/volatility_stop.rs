//! Volatility Stop
//!
//! A stop-and-reverse level after Welles Wilder's volatility system (New Concepts in Technical
//! Trading Systems, 1978): the stop trails the extreme source value of the current trend by a
//! multiple of the average true range, only ever moving in the trend's direction, and the trend
//! reverses when the source crosses it.
//!
//! # Formula
//! With `band = k * ATR(n)` (Wilder's average true range, length 20 and multiplier 2 by
//! default) and the source the close:
//! - `max` and `min` are the highest and lowest source values since the last reversal
//! - in an uptrend `stop = max(previous stop, max - band)`,
//!   in a downtrend `stop = min(previous stop, min + band)`
//! - the trend is up while `source >= stop`; on a reversal `max` and `min` restart at the
//!   source and the stop restarts at `source - band` (up) or `source + band` (down)
//!
//! The first stop is the first source value, in an uptrend. Until the ATR has `n` true ranges
//! the band is the bar's true range, without the multiplier. The stop is drawn as crosses,
//! green in an uptrend and red in a downtrend.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, VolatilityStop};
//!
//! let mut stop = VolatilityStop::new(20, 2.0);
//! stop.calculate(&bars);
//! ```

use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Volatility Stop: a trailing ATR stop that reverses with the trend.
#[derive(Clone)]
pub struct VolatilityStop {
    length: usize,
    multiplier: f64,
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// Whether each source bar is in an uptrend, for its colour.
    uptrend: Vec<bool>,
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    usable: usize,
    previous: f64,
    atr: f64,
    max: f64,
    min: f64,
    stop: f64,
    downtrend: bool,
}

impl VolatilityStop {
    /// Create the stop with the ATR length (typically 20) and the ATR multiplier (typically 2,
    /// clamped into 0 to `f64::MAX`, NaN to 0).
    pub fn new(length: usize, multiplier: f64) -> Self {
        Self {
            length: length.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            uptrend: Vec::new(),
            colors: [palette::TEAL, palette::RED],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for VolatilityStop {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for VolatilityStop {
    crate::studies::inputs::study_inputs!(self;
        length: length("Length", 1, usize::MAX),
        multiplier: number("Multiplier", 0.0, f64::MAX)
    ; {});

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[0],
            width: 1.0,
            pattern: crate::studies::PlotPattern::Solid,
            plot_type: crate::studies::PlotType::Crosses,
            visible: true,
        }
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        self.values.value(0, bar)?;
        let up = *self.uptrend.get(bar)?;
        Some(self.colors[usize::from(!up)])
    }

    fn name(&self) -> &str {
        "Volatility Stop"
    }

    fn desc(&self) -> &str {
        "Volatility Stop - ATR trailing stop and reverse"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (length, multiplier) = (self.length, self.multiplier);
        let mut trends = Vec::new();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|p| p.is_finite()) {
                    trends.push(false);
                    return IndicatorValue::None;
                }
                let source = bar.close;
                let first = state.usable == 0;
                let tr = if first {
                    bar.high - bar.low
                } else {
                    ATR::true_range(bar, state.previous)
                };
                state.previous = bar.close;
                state.usable += 1;
                if state.usable <= length {
                    state.atr += tr;
                    if state.usable == length {
                        state.atr /= length as f64;
                    }
                } else {
                    state.atr += (tr - state.atr) / length as f64;
                }
                let band = if state.usable >= length {
                    multiplier * state.atr
                } else {
                    tr
                };
                if first {
                    (state.max, state.min, state.stop) = (source, source, source);
                } else {
                    state.max = state.max.max(source);
                    state.min = state.min.min(source);
                    state.stop = if state.downtrend {
                        state.stop.min(state.min + band)
                    } else {
                        state.stop.max(state.max - band)
                    };
                }
                let downtrend = source < state.stop;
                if downtrend != state.downtrend {
                    state.downtrend = downtrend;
                    (state.max, state.min) = (source, source);
                    state.stop = if downtrend {
                        source + band
                    } else {
                        source - band
                    };
                }
                trends.push(!state.downtrend);
                IndicatorValue::Single(state.stop)
            });
        self.uptrend.truncate(data.len() - trends.len());
        self.uptrend.extend(trends);
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
        vec!["Volatility Stop".into()]
    }
}
