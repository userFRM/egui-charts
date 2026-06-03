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
//! ```rust,ignore
//! use egui_charts::studies::{EMA, Indicator};
//!
//! let mut ema = EMA::new(12);
//! ema.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Exponential Moving Average indicator.
///
/// Applies an exponential weighting to closing prices so that recent bars
/// have a greater influence. This is an overlay indicator.
#[derive(Clone)]
pub struct EMA {
    period: usize,
    values: Vec<IndicatorValue>,
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
            period,
            values: Vec::new(),
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
    fn name(&self) -> &str {
        "EMA"
    }

    fn desc(&self) -> &str {
        "Exponential Moving Avg - Weighted avg giving more importance to recent prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let multiplier = 2.0 / (self.period as f64 + 1.0);

        // Seed the series with the first close, then smooth from the second bar
        // onward. This keeps values[0] == close[0] and emits one value per bar.
        let mut ema = data[0].close;
        self.values.push(IndicatorValue::Single(ema));

        for bar in data.iter().skip(1) {
            ema = (bar.close - ema) * multiplier + ema;
            self.values.push(IndicatorValue::Single(ema));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
            })
            .collect()
    }

    /// The documented seeding convention: the first output equals the first
    /// close, there is no `None` warmup, and the series is one value per bar.
    #[test]
    fn first_value_equals_first_close() {
        let closes = [10.0, 11.0, 12.0, 13.0, 14.0];
        let bars = bars_from_closes(&closes);

        let mut ema = EMA::new(3);
        ema.calculate(&bars);

        assert_eq!(ema.values().len(), bars.len());
        match ema.values()[0] {
            IndicatorValue::Single(v) => assert!((v - closes[0]).abs() < 1e-12),
            _ => panic!("first EMA value must be the seeded first close"),
        }
    }

    /// The recursive step must match the documented formula exactly from the
    /// second bar onward, proving bar 0 is not re-smoothed.
    #[test]
    fn recursion_matches_formula() {
        let closes = [10.0, 20.0, 30.0];
        let bars = bars_from_closes(&closes);

        let mut ema = EMA::new(4);
        ema.calculate(&bars);

        let mult = 2.0 / (4.0 + 1.0);
        let mut expected = closes[0];
        for (i, &c) in closes.iter().enumerate() {
            if i > 0 {
                expected = (c - expected) * mult + expected;
            }
            match ema.values()[i] {
                IndicatorValue::Single(v) => assert!((v - expected).abs() < 1e-12),
                _ => panic!("bar {i} must carry a value"),
            }
        }
    }

    /// Empty input yields no values rather than an out-of-bounds seed read.
    #[test]
    fn empty_input_yields_empty() {
        let mut ema = EMA::new(12);
        ema.calculate(&[]);
        assert!(ema.values().is_empty());
    }
}
