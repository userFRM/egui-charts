//! Volatility Stop
//!
//! A stop-and-reverse level after Welles Wilder's volatility system (New Concepts in Technical
//! Trading Systems, 1978): the stop trails the extreme source value of the current trend by a
//! multiple of the average true range, only ever moving in the trend's direction, and the trend
//! reverses when the source crosses it.
//!
//! # Formula
//! With `band = k * ATR(n)` (Wilder's average true range, length 20 and multiplier 2 by
//! default) and the source, close by default:
//! - `max` and `min` are the highest and lowest source values since the last reversal
//! - in an uptrend `stop = max(previous stop, max - band)`,
//!   in a downtrend `stop = min(previous stop, min + band)`
//! - the trend is up while `source >= stop`; on a reversal `max` and `min` restart at the
//!   source and the stop restarts at `source - band` (up) or `source + band` (down)
//!
//! The first stop is the first source value, in an uptrend. Until the ATR has `n` true ranges
//! the band is the bar's true range, without the multiplier. The stop is drawn as crosses in the
//! plot colour, teal by default, in an uptrend and the Downtrend colour, red by default, in a
//! downtrend.
//!
//! The average true range always reads each bar's high, low and previous close. A bar without
//! a source value has no stop and leaves the trend unchanged.
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
use crate::model::{Bar, PriceSource};
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Volatility Stop: a trailing ATR stop that reverses with the trend.
#[derive(Clone, Debug)]
pub struct VolatilityStop {
    length: usize,
    multiplier: f64,
    source: PriceSource,
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// Whether each source bar is in an uptrend, for its colour.
    uptrend: Vec<bool>,
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Debug, Default)]
struct State {
    bar: usize,
    started: bool,
    usable: usize,
    previous: f64,
    atr: f64,
    max: f64,
    min: f64,
    stop: f64,
    downtrend: bool,
}

impl VolatilityStop {
    /// Create the stop with the ATR length (typically 20, at least 2) and the ATR multiplier
    /// (typically 2, clamped into 0.25 to `f64::MAX`, NaN to 0.25).
    pub fn new(length: usize, multiplier: f64) -> Self {
        Self {
            length: length.max(2),
            multiplier: clamp_param(multiplier, 0.25, f64::MAX),
            source: PriceSource::Close,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            uptrend: Vec::new(),
            colors: [
                crate::studies::palette::DEEP_TEAL,
                crate::studies::palette::BRIGHT_RED,
            ],
            visible: true,
        }
    }

    fn update_prices(&mut self, data: &[Bar], from: usize, source: Option<&[f64]>) {
        let (length, multiplier) = (self.length, self.multiplier);
        let mut trends = Vec::new();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.bar;
                state.bar += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|p| p.is_finite()) {
                    trends.push(false);
                    return IndicatorValue::None;
                }
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
                let price = source.map_or_else(
                    || self.source.compute(bar.open, bar.high, bar.low, bar.close),
                    |source| source.get(index).copied().unwrap_or(f64::NAN),
                );
                if !price.is_finite() {
                    trends.push(false);
                    return IndicatorValue::None;
                }
                if !state.started {
                    state.started = true;
                    (state.max, state.min, state.stop) = (price, price, price);
                } else {
                    state.max = state.max.max(price);
                    state.min = state.min.min(price);
                    state.stop = if state.downtrend {
                        state.stop.min(state.min + band)
                    } else {
                        state.stop.max(state.max - band)
                    };
                }
                let downtrend = price < state.stop;
                if downtrend != state.downtrend {
                    state.downtrend = downtrend;
                    (state.max, state.min) = (price, price);
                    state.stop = if downtrend {
                        price + band
                    } else {
                        price - band
                    };
                }
                trends.push(!state.downtrend);
                IndicatorValue::Single(state.stop)
            });
        self.uptrend.truncate(data.len() - trends.len());
        self.uptrend.extend(trends);
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
        length: length("Length", 2, usize::MAX),
        multiplier: number("Multiplier", 0.25, f64::MAX, 0.25),
        source: choice("Source", PriceSource::all())
    ; {});

    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(self.source))
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[0],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Crosses,
            visible: true,
        }
    }

    fn draws_only_plots(&self) -> bool {
        true
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        self.values.value(0, bar)?;
        let up = *self.uptrend.get(bar)?;
        Some(self.colors[usize::from(!up)])
    }

    fn plot_variants(&self, _line: usize) -> Vec<crate::studies::PlotVariant> {
        vec![crate::studies::PlotVariant::new(
            "Downtrend",
            self.colors[1],
        )]
    }

    fn plot_variant(&self, _line: usize, bar: usize) -> Option<usize> {
        self.values.value(0, bar)?;
        (!*self.uptrend.get(bar)?).then_some(0)
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
        self.update_prices(data, from, None);
    }

    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        self.update_prices(data, from, Some(source));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PriceSource;
    use crate::studies::{ConfiguredStudy, InputValue, StudySource};

    /// ATR(2) reads true ranges 2, 2, 1.5, 2.7, 1.5, 2.4, hence bands 2, 2, 1.75,
    /// 2.225, 1.8625, 2.13125 at multiplier 1. High as source changes only the trail.
    #[test]
    fn catalogue_source_moves_the_stop_while_the_range_reads_the_bars() {
        let bars: Vec<_> = [
            (10.0, 11.0, 9.0, 10.5),
            (10.5, 12.0, 10.0, 11.5),
            (11.5, 12.5, 11.0, 12.0),
            (12.0, 12.2, 9.5, 9.8),
            (9.8, 10.5, 9.0, 9.2),
            (9.2, 11.5, 9.1, 11.2),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c))| {
            Bar::new(
                chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i as i64),
                o,
                h,
                l,
                c,
                1.0,
            )
        })
        .collect();
        let close = [
            (10.5, false),
            (10.5, false),
            (10.5, false),
            (12.025, true),
            (11.0625, true),
            (9.06875, false),
        ];
        let high = [
            (11.0, false),
            (11.0, false),
            (11.0, false),
            (11.0, false),
            (12.3625, true),
            (12.3625, true),
        ];
        for mode in 0..4 {
            let mut study: Box<dyn Indicator> = Box::new(VolatilityStop::new(2, 1.0));
            if mode == 1 {
                study.set_input("source", InputValue::Choice(1)).unwrap();
            } else if mode == 2 {
                study = Box::new(ConfiguredStudy::new(study));
                study
                    .set_input(
                        "source",
                        InputValue::Source(StudySource::Price(PriceSource::High)),
                    )
                    .unwrap();
            }
            let column = [f64::NAN, 12.0, f64::NAN, 12.2, 10.5, 11.5];
            let check = |study: &dyn Indicator, bar: usize| {
                if mode == 3 && (bar == 0 || bar == 2) {
                    assert_eq!(study.values().value(0, bar), None);
                    assert_eq!(study.plot_variant(0, bar), None);
                } else {
                    let (want, down) = if mode == 0 {
                        close[bar]
                    } else if mode == 3 && bar < 4 {
                        (12.0, false)
                    } else {
                        high[bar]
                    };
                    let got = study.values().value(0, bar).unwrap();
                    assert!(
                        (got - want).abs() < 1e-12,
                        "mode {mode}, bar {bar}: {got} != {want}"
                    );
                    assert_eq!(
                        study.plot_variant(0, bar),
                        down.then_some(0),
                        "mode {mode}, bar {bar}"
                    );
                }
            };
            if mode == 3 {
                study.update_source(&bars, 0, &column);
            } else {
                study.calculate(&bars);
            }
            for bar in 0..bars.len() {
                check(study.as_ref(), bar);
            }
            for end in 1..=bars.len() {
                if mode == 3 {
                    study.update_source(&bars[..end], end - 1, &column);
                } else if end == 1 {
                    study.calculate(&bars[..end]);
                } else {
                    study.update(&bars[..end], end - 1);
                }
                check(study.as_ref(), end - 1);
            }
            // A correction replays the same source and trend state from a checkpoint.
            if mode == 3 {
                study.update_source(&bars, 2, &column);
            } else {
                study.update(&bars, 2);
            }
            for bar in 0..bars.len() {
                check(study.as_ref(), bar);
            }
        }
    }
}
