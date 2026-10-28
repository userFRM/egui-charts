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
//! # Default colour
//!
//! Blue (#2962ff), with a two-pixel line.
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
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
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
    /// Create a new RSI indicator.
    ///
    /// # Arguments
    /// * `period` -- The lookback period (default convention: 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PURPLE,
            visible: true,
        }
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for RSI {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for RSI {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "RSI"
    }

    fn line_style(&self, _line: usize) -> crate::studies::IndicatorLineStyle {
        crate::studies::IndicatorLineStyle {
            width: 2.0,
            ..Default::default()
        }
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
        let period = self.period;
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                state.next(bar, period)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: Some(138.0),
            range: Some((0.0, 100.0)),
            levels: vec![
                (70.0, palette::MUTED, "Overbought"),
                (30.0, palette::MUTED, "Oversold"),
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("RSI({})", self.period)]
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
}
