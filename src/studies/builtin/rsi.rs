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
//! Purple (from the design-token `indicators.rsi`).
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{RSI, Indicator};
//!
//! let mut rsi = RSI::new(14);
//! rsi.calculate(&bars);
//! // First `period` values are IndicatorValue::None (warmup)
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Relative Strength Index (RSI) indicator.
///
/// A bounded momentum oscillator (0-100) that compares the magnitude of
/// recent gains to recent losses. Drawn in a separate sub-pane.
#[derive(Clone)]
pub struct RSI {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl RSI {
    /// Create a new RSI indicator.
    ///
    /// # Arguments
    /// * `period` -- The lookback period (default convention: 14).
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.indicators.rsi, // Purple
            visible: true,
        }
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for RSI {
    fn name(&self) -> &str {
        "RSI"
    }

    fn desc(&self) -> &str {
        "Relative Strength Index - Momentum oscillator (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            return;
        }

        let mut gains = Vec::new();
        let mut losses = Vec::new();

        // Calculate price changes
        for i in 1..data.len() {
            let change = data[i].close - data[i - 1].close;
            gains.push(if change > 0.0 { change } else { 0.0 });
            losses.push(if change < 0.0 { -change } else { 0.0 });
        }

        // Calculate initial avg gain/loss
        let mut avg_gain = gains[..self.period].iter().sum::<f64>() / self.period as f64;
        let mut avg_loss = losses[..self.period].iter().sum::<f64>() / self.period as f64;

        // First RSI value
        self.values.push(IndicatorValue::None);
        for _ in 0..self.period {
            self.values.push(IndicatorValue::None);
        }

        let rs = avg_gain / avg_loss.max(1e-10);
        let rsi = 100.0 - (100.0 / (1.0 + rs));
        self.values.push(IndicatorValue::Single(rsi));

        // Calculate remaining RSI values using smoothing
        for i in self.period..gains.len() {
            avg_gain = (avg_gain * (self.period - 1) as f64 + gains[i]) / self.period as f64;
            avg_loss = (avg_loss * (self.period - 1) as f64 + losses[i]) / self.period as f64;

            let rs = avg_gain / avg_loss.max(1e-10);
            let rsi = 100.0 - (100.0 / (1.0 + rs));
            self.values.push(IndicatorValue::Single(rsi));
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
