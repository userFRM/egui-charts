//! Exponential Moving Average (EMA) indicator.
//!
//! The EMA gives exponentially more weight to recent prices, making it more
//! responsive to new information than the SMA. It is one of the most widely
//! used technical indicators and serves as a building block for many others
//! (MACD, Bollinger Bands, Keltner Channels, etc.).
//!
//! # Formula
//!
//! ```text
//! multiplier = 2 / (period + 1)
//! EMA(t) = (Close[t] - EMA[t-1]) * multiplier + EMA[t-1]
//! ```
//!
//! # Seeding convention
//!
//! The first output value equals the first closing price (`values[0] ==
//! data[0].close`); recursive smoothing begins at the second bar. There is no
//! `None` warmup. This matches the seeding used by the EMAs inside [`MACD`],
//! so the standalone indicator and the composite agree bar-for-bar.
//!
//! # Interpretation
//!
//! - Faster to react to price changes than SMA of the same period.
//! - Common periods: 12 and 26 (used in MACD), 9 (short-term), 50/200.
//! - EMA crossovers are popular entry/exit signals.
//!
//! # Default colour
//!
//! Blue (from the design-token `indicators.ema`).
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{EMA, Indicator};
//!
//! let mut ema = EMA::new(12);
//! ema.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Exponential Moving Average indicator.
///
/// Applies an exponential weighting to closing prices so that recent bars
/// have a greater influence. This is an overlay indicator.
#[derive(Clone)]
pub struct EMA {
    period: usize,
    values: IndicatorValues,
    /// The EMA of the bars stepped so far, `None` before the first bar.
    stepper: Stepper<Option<f64>>,
    color: Color32,
    visible: bool,
}

impl EMA {
    /// Create a new EMA indicator.
    ///
    /// # Arguments
    /// * `period` -- The smoothing period (e.g. 12, 26, 50).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: DESIGN_TOKENS.semantic.indicators.ema, // Blue
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
impl Default for EMA {
    fn default() -> Self {
        Self::new(12)
    }
}

impl Indicator for EMA {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "EMA"
    }

    fn desc(&self) -> &str {
        "Exponential Moving Avg - Weighted avg giving more importance to recent prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let multiplier = 2.0 / (self.period as f64 + 1.0);
        self.stepper.run(data, from, &mut self.values, |ema, bar| {
            // A bar without a finite close has no value and leaves the average as it was.
            if !bar.close.is_finite() {
                return IndicatorValue::None;
            }
            // Seed with the first close, then smooth from the second bar onward.
            let next = match *ema {
                None => bar.close,
                Some(prev) => (bar.close - prev) * multiplier + prev,
            };
            *ema = Some(next);
            IndicatorValue::Single(next)
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

    fn is_overlay(&self) -> bool {
        true
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
        vec![format!("EMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        let start = Utc::now();
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar {
                time: start + Duration::minutes(i as i64),
                open: c,
                high: c,
                low: c,
                close: c,
                volume: 1.0,
                colors: None,
                payload: None,
            })
            .collect()
    }

    /// Reference values from pandas 3.0.2, `Series.ewm(span=4, adjust=False).mean()`,
    /// which seeds with the first value and applies `alpha = 2 / (span + 1)`.
    #[test]
    fn matches_pandas_reference() {
        let closes = [
            22.27, 22.19, 22.08, 22.17, 22.18, 22.13, 22.23, 22.43, 22.24, 22.29,
        ];
        let expected = [
            22.27,
            22.238,
            22.1748,
            22.17288,
            22.175728,
            22.1574368,
            22.18646208,
            22.283877248,
            22.2663263488,
            22.27579580928,
        ];
        let bars = bars_from_closes(&closes);

        let mut ema = EMA::new(4);
        ema.calculate(&bars);

        assert_eq!(ema.values().len(), expected.len());
        for (i, (v, e)) in ema.values().to_rows().iter().zip(expected).enumerate() {
            match v {
                IndicatorValue::Single(v) => assert!((v - e).abs() < 1e-9, "bar {i}: {v} != {e}"),
                _ => panic!("bar {i} must carry a value"),
            }
        }
    }
}
