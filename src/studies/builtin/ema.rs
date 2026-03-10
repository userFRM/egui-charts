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
//! The first EMA value is seeded with the first closing price.
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
        let mut ema = data[0].close;

        for bar in data {
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
