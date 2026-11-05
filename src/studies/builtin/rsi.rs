//! Relative Strength Index (RSI) indicator.
//!
//! RSI is a momentum oscillator that measures the speed and magnitude of
//! price movements on a 0-100 scale. Developed by J. Welles Wilder Jr.,
//! it is one of the most popular technical indicators for identifying
//! overbought and oversold conditions.
//!
//! # Formula
//!
//! ```text
//! RS = Average Gain / Average Loss   (over N periods, using Wilder's smoothing)
//! RSI = 100 - (100 / (1 + RS))
//! ```
//!
//! The initial average gain/loss is a simple mean of the first `N` changes.
//! Subsequent values use Wilder's smoothing:
//! `Avg = (Prev_Avg * (N-1) + Current) / N`.
//!
//! The formula is evaluated as `100 * Gain / (Gain + Loss)`, which is identical but needs no
//! price-unit floor, so scaling the prices does not change the result. A window with gains and
//! no losses is 100, one with losses and no gains is 0, and a fully flat window is the neutral
//! 50.
//!
//! # Interpretation
//!
//! - RSI > 70: overbought -- potential pullback.
//! - RSI < 30: oversold -- potential bounce.
//! - Divergence between RSI and price often precedes reversals.
//! - Default period: 14.
//!
//! This is a **non-overlay** indicator (drawn in its own sub-pane).
//!
//! # RSI-based moving average
//!
//! [`RSI::default`] also draws a 14-bar simple average of the RSI for reading crossovers, whose
//! type (none, SMA or EMA) and length are inputs. [`RSI::new`] starts without it.
//!
//! # Default colour
//!
//! Purple (#7E57C2), with a one-pixel line, and the average yellow (#FDD835).
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{RSI, Indicator, IndicatorValue};
//!
//! let mut rsi = RSI::new(14);
//! rsi.calculate(&bars);
//! // The first `period` values are warmup.
//! assert!(rsi.values().to_rows()[..14].iter().all(|v| matches!(v, IndicatorValue::None)));
//! assert!(matches!(rsi.values().to_rows()[14], IndicatorValue::Single(_)));
//! ```

use super::price_oscillator::{Mean, OscillatorAverage};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Relative Strength Index (RSI) indicator.
///
/// A bounded momentum oscillator (0-100) that compares the magnitude of
/// recent gains to recent losses. Drawn in a separate sub-pane.
#[derive(Clone)]
pub struct RSI {
    period: usize,
    ma_type: MaType,
    ma_length: usize,
    values: IndicatorValues,
    stepper: Stepper<Averaged>,
    color: Color32,
    ma_color: Color32,
    fill_color: Color32,
    visible: bool,
}

/// The moving average of the RSI that the study draws with it.
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaType {
    None,
    SMA,
    EMA,
}

/// The RSI and the average of its values.
#[derive(Clone, Default)]
struct Averaged {
    rsi: State,
    mean: Mean,
}

#[derive(Clone)]
pub(super) struct State {
    previous: Option<f64>,
    usable: usize,
    gain: f64,
    loss: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            previous: None,
            usable: 0,
            gain: -0.0,
            loss: -0.0,
        }
    }
}

/// RSI from Wilder's average gain and loss: 100 with no loss, 0 with no gain, 50 when both are 0.
pub(super) fn from_averages(gain: f64, loss: f64) -> f64 {
    let total = gain + loss;
    if total > 0.0 {
        100.0 * gain / total
    } else {
        50.0
    }
}

impl State {
    pub(super) fn next(&mut self, bar: &Bar, period: usize) -> IndicatorValue {
        if !bar.close.is_finite() {
            return IndicatorValue::None;
        }
        let previous = self.previous.replace(bar.close);
        let i = self.usable;
        self.usable += 1;
        let Some(previous) = previous else {
            return IndicatorValue::None;
        };
        let change = bar.close - previous;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        if i <= period {
            self.gain += gain;
            self.loss += loss;
            if i < period {
                return IndicatorValue::None;
            }
            self.gain /= period as f64;
            self.loss /= period as f64;
        } else {
            self.gain = (self.gain * (period - 1) as f64 + gain) / period as f64;
            self.loss = (self.loss * (period - 1) as f64 + loss) / period as f64;
        }
        IndicatorValue::Single(from_averages(self.gain, self.loss))
    }
}

impl RSI {
    /// Create a new RSI indicator without an RSI-based moving average.
    ///
    /// # Arguments
    /// * `period` -- The lookback period (default convention: 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            ma_type: MaType::None,
            ma_length: 14,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PURPLE,
            ma_color: palette::YELLOW,
            fill_color: palette::PURPLE.gamma_multiply_u8(26),
            visible: true,
        }
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.set_colors(vec![color]);
        self
    }
}

/// Construct with the conventional default parameters: a 14-bar RSI and its 14-bar SMA.
impl Default for RSI {
    fn default() -> Self {
        Self {
            ma_type: MaType::SMA,
            ..Self::new(14)
        }
    }
}

impl Indicator for RSI {
    fn level_fill_color(&self) -> Color32 {
        self.fill_color
    }

    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        ma_type: choice("MA type", [MaType::None, MaType::SMA, MaType::EMA]),
        ma_length: length("MA length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "RSI"
    }

    fn precision(&self) -> Option<usize> {
        Some(2)
    }

    fn desc(&self) -> &str {
        "Relative Strength Index - Momentum oscillator (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, ma_type, ma_length) = (self.period, self.ma_type, self.ma_length);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let value = state.rsi.next(bar, period);
                let average = match ma_type {
                    MaType::None => return value,
                    MaType::SMA => OscillatorAverage::Simple,
                    MaType::EMA => OscillatorAverage::Exponential,
                };
                match value {
                    IndicatorValue::Single(rsi) => IndicatorValue::Multiple(vec![
                        rsi,
                        state.mean.next(average, rsi, ma_length),
                    ]),
                    other => other,
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    /// The RSI, the average and the band between the levels.
    fn colors(&self) -> Vec<Color32> {
        vec![self.color, self.ma_color, self.fill_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
            self.ma_color = colors.get(1).copied().unwrap_or(self.ma_color);
            self.fill_color = colors.get(2).copied().unwrap_or_else(|| {
                Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), 16)
            });
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: Some(138.0),
            range: Some((0.0, 100.0)),
            levels: vec![
                (70.0, palette::MUTED, "Overbought"),
                (30.0, palette::MUTED, "Oversold"),
                (50.0, palette::MUTED, "Middle"),
            ],
            fill_between_levels: true,
        }
    }

    fn is_overlay(&self) -> bool {
        false // RSI is drawn in separate pane
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

    fn line_count(&self) -> usize {
        if self.ma_type == MaType::None { 1 } else { 2 }
    }

    fn line_names(&self) -> Vec<String> {
        let mut names = vec![format!("RSI({})", self.period)];
        if self.ma_type != MaType::None {
            names.push("RSI-based MA".to_owned());
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    fn rsi_of(closes: &[f64], period: usize) -> Vec<Option<f64>> {
        let mut rsi = RSI::new(period);
        rsi.calculate(&bars_from_closes(closes));
        rsi.values()
            .to_rows()
            .into_iter()
            .map(|v| match v {
                IndicatorValue::Single(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Endpoints and scale: hand values for period 2 are RSI[2] = 100 * 1 / (1 + 0.5) from the
    /// mean gain 1 and loss 0.5, then 100 * 1.5 / (1.5 + 0.25) after Wilder smoothing. Scaling
    /// the prices must not move them; no loss is 100, no gain 0 and a flat window the neutral 50.
    #[test]
    fn endpoints_and_scale_invariance() {
        let ordinary = [10.0, 12.0, 11.0, 13.0];
        for scale in [1.0, 1e-12, 1e12] {
            let closes: Vec<f64> = ordinary.iter().map(|c| c * scale).collect();
            let got = rsi_of(&closes, 2);
            assert_eq!(got[..2], [None, None]);
            assert!((got[2].unwrap() - 200.0 / 3.0).abs() < 1e-9, "{scale}");
            assert!((got[3].unwrap() - 600.0 / 7.0).abs() < 1e-9, "{scale}");
        }
        assert_eq!(rsi_of(&[1e-12, 2e-12, 3e-12, 4e-12], 2)[2], Some(100.0));
        assert_eq!(rsi_of(&[4e-12, 3e-12, 2e-12, 1e-12], 2)[2], Some(0.0));
        assert_eq!(rsi_of(&[5.0, 5.0, 5.0, 5.0], 2)[2..], [Some(50.0); 2]);
    }

    /// RSI(1) of 1, 2, 1, 2, 3 is missing, 100, 0, 100, 100. The two-bar SMA is their mean: 50, 50,
    /// 100. The two-bar EMA starts from that same first mean, then adds 2/3 of each step: 50 + 2/3
    /// * 50, then 250/3 + 2/3 * 50 / 3. The default study is a 14-bar SMA, first defined on bar 27,
    /// the 14th of the RSI values that start on bar 14.
    #[test]
    fn the_average_of_the_rsi_follows_its_inputs() {
        let bars = bars_from_closes(&[1.0, 2.0, 1.0, 2.0, 3.0]);
        for (ma_type, want) in [
            (1, [50.0, 50.0, 100.0]),
            (2, [50.0, 250.0 / 3.0, 850.0 / 9.0]),
        ] {
            let mut study = RSI::default();
            study
                .set_input("period", crate::studies::InputValue::Length(1))
                .unwrap();
            study
                .set_input("ma_length", crate::studies::InputValue::Length(2))
                .unwrap();
            study
                .set_input("ma_type", crate::studies::InputValue::Choice(ma_type))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.values().value(0, 1), Some(100.0));
            assert_eq!(study.plot_value(1, 1), None);
            for (bar, want) in (2..).zip(want) {
                let got = study.values().value(1, bar).unwrap();
                assert!((got - want).abs() < 1e-9, "{ma_type} bar {bar}: {got}");
            }
        }

        let added = crate::studies::IndicatorFactory::new().create("RSI");
        assert_eq!(added.unwrap().line_count(), 2);
        let mut study = RSI::default();
        assert_eq!(study.line_count(), 2);
        assert_eq!(study.line_names()[1], "RSI-based MA");
        assert_eq!(study.plot_style(1).color, palette::YELLOW);
        study.calculate(&bars_from_closes(
            &(0..30).map(|i| f64::from(i % 3)).collect::<Vec<_>>(),
        ));
        assert_eq!(study.plot_value(1, 26), None);
        assert!(study.plot_value(1, 27).is_some());
        study
            .set_input("ma_type", crate::studies::InputValue::Choice(0))
            .unwrap();
        assert_eq!(study.line_count(), 1);
    }
}
