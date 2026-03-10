//! Simple Moving Average (SMA) indicator.
//!
//! The SMA is the most fundamental moving average, computed as the
//! unweighted arithmetic mean of the closing prices over the last `N`
//! periods. It is an overlay indicator drawn directly on the price chart.
//!
//! # Formula
//!
//! ```text
//! SMA(t) = (Close[t] + Close[t-1] + ... + Close[t-N+1]) / N
//! ```
//!
//! # Interpretation
//!
//! - Price above SMA: bullish bias.
//! - Price below SMA: bearish bias.
//! - Popular periods: 20 (short-term), 50 (medium-term), 200 (long-term).
//! - SMA crossovers (e.g. 50/200 "golden cross") are widely-used signals.
//!
//! # Default colour
//!
//! Orange (from the design-token `indicators.ma`).
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{SMA, Indicator};
//!
//! let mut sma = SMA::new(20);
//! sma.calculate(&bars);
//! // First 19 values are IndicatorValue::None (warmup)
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Simple Moving Average indicator.
///
/// Computes the arithmetic mean of closing prices over a rolling window of
/// `period` bars. This is an overlay indicator (drawn on the price chart).
#[derive(Clone)]
pub struct SMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl SMA {
    /// Create a new SMA indicator.
    ///
    /// # Arguments
    /// * `period` -- The number of bars in the averaging window (e.g. 20, 50, 200).
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.indicators.ma, // Orange for MA
            visible: true,
        }
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for SMA {
    fn name(&self) -> &str {
        "SMA"
    }

    fn desc(&self) -> &str {
        "Simple Moving Avg - Avg price over N periods"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;
                let sum: f64 = data[start..=i].iter().map(|bar| bar.close).sum();
                let sma = sum / self.period as f64;
                self.values.push(IndicatorValue::Single(sma));
            }
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
        vec![format!("SMA({})", self.period)]
    }
}
