//! Moving Average Convergence Divergence (MACD) indicator.
//!
//! MACD is a trend-following momentum indicator that shows the
//! relationship between two exponential moving averages of closing
//! prices. It consists of three output lines plotted in a separate
//! sub-pane.
//!
//! # Components
//!
//! 1. **MACD line** = EMA(fast) - EMA(slow)
//! 2. **Signal line** = EMA(MACD line, signal_period)
//! 3. **Histogram** = MACD line - Signal line
//!
//! # Default parameters
//!
//! `MACD::new(12, 26, 9)` -- the classic (12, 26, 9) setting.
//!
//! # Interpretation
//!
//! - MACD crossing above signal: bullish momentum.
//! - MACD crossing below signal: bearish momentum.
//! - Histogram shrinking toward zero: momentum slowing.
//! - Divergence between MACD and price: potential reversal.
//!
//! # Default colours
//!
//! - MACD line: blue (`indicators.macd_line`)
//! - Signal line: orange (`indicators.macd_signal`)
//! - Histogram: green (`extended.success`)
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{MACD, Indicator, IndicatorValue};
//!
//! let mut macd = MACD::new(12, 26, 9);
//! macd.calculate(&bars);
//!
//! if let IndicatorValue::Multiple(vals) = &macd.values()[50] {
//!     let (macd_line, signal, histogram) = (vals[0], vals[1], vals[2]);
//! }
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Moving Average Convergence Divergence (MACD) indicator.
///
/// A three-line momentum oscillator: MACD line, signal line, and histogram.
/// Drawn in a separate sub-pane (`is_overlay = false`).
#[derive(Clone)]
pub struct MACD {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl MACD {
    /// Create a new MACD indicator.
    ///
    /// # Arguments
    /// * `fast_period` -- Period for the fast EMA (default: 12).
    /// * `slow_period` -- Period for the slow EMA (default: 26).
    /// * `signal_period` -- Period for the signal-line EMA (default: 9).
    pub fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            signal_period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.macd_line, // MACD line
                DESIGN_TOKENS.semantic.indicators.macd_signal, // Signal line
                DESIGN_TOKENS.semantic.extended.success,     // Histogram
            ],
            visible: true,
        }
    }
}

impl Indicator for MACD {
    fn name(&self) -> &str {
        "MACD"
    }

    fn desc(&self) -> &str {
        "MACD - Trend-following momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.slow_period {
            return;
        }

        // Calculate fast EMA
        let fast_mult = 2.0 / (self.fast_period as f64 + 1.0);
        let mut fast_ema = data[0].close;
        let mut fast_emas = vec![fast_ema];

        for bar in data.iter().skip(1) {
            fast_ema = (bar.close - fast_ema) * fast_mult + fast_ema;
            fast_emas.push(fast_ema);
        }

        // Calculate slow EMA
        let slow_mult = 2.0 / (self.slow_period as f64 + 1.0);
        let mut slow_ema = data[0].close;
        let mut slow_emas = vec![slow_ema];

        for bar in data.iter().skip(1) {
            slow_ema = (bar.close - slow_ema) * slow_mult + slow_ema;
            slow_emas.push(slow_ema);
        }

        // Calculate MACD line
        let macd_line: Vec<f64> = fast_emas
            .iter()
            .zip(slow_emas.iter())
            .map(|(fast, slow)| fast - slow)
            .collect();

        // Calculate signal line (EMA of MACD)
        if macd_line.len() < self.signal_period {
            return;
        }

        let signal_mult = 2.0 / (self.signal_period as f64 + 1.0);
        let mut signal_ema = macd_line[0];
        let mut signal_line = vec![signal_ema];

        for &macd_val in macd_line.iter().skip(1) {
            signal_ema = (macd_val - signal_ema) * signal_mult + signal_ema;
            signal_line.push(signal_ema);
        }

        // Calculate histogram
        for i in 0..macd_line.len() {
            let histogram = macd_line[i] - signal_line[i];
            self.values.push(IndicatorValue::Multiple(vec![
                macd_line[i],
                signal_line[i],
                histogram,
            ]));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            // If less than 3 colors provided, use the first color for all lines
            self.colors = vec![colors[0], colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        false // MACD is drawn in separate pane
    }

    fn line_cnt(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!(
                "MACD({},{},{})",
                self.fast_period, self.slow_period, self.signal_period
            ),
            "Signal".to_string(),
            "Histogram".to_string(),
        ]
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
}
