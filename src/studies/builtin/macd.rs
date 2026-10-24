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
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{MACD, Indicator, IndicatorValue};
//!
//! let mut macd = MACD::new(12, 26, 9);
//! macd.calculate(&bars);
//!
//! if let IndicatorValue::Multiple(vals) = &macd.values().to_rows()[50] {
//!     let (macd_line, signal, histogram) = (vals[0], vals[1], vals[2]);
//! }
//! ```

use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
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
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    usable: usize,
    fast: f64,
    slow: f64,
    signal: f64,
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
            fast_period: fast_period.max(1),
            slow_period: slow_period.max(1),
            signal_period: signal_period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.macd_line, // MACD line
                DESIGN_TOKENS.semantic.indicators.macd_signal, // Signal line
                DESIGN_TOKENS.semantic.extended.success,     // Histogram
            ],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for MACD {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl Indicator for MACD {
    crate::studies::inputs::study_inputs!(self;
        fast_period: length("Fast period", 1, usize::MAX),
        slow_period: length("Slow period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line],
            width: 1.0,
            pattern: crate::studies::PlotPattern::Solid,
            plot_type: if line == 2 {
                crate::studies::PlotType::Columns
            } else {
                crate::studies::PlotType::Line
            },
            visible: true,
        }
    }

    fn name(&self) -> &str {
        "MACD"
    }

    fn desc(&self) -> &str {
        "MACD - Trend-following momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let threshold = self.slow_period.max(self.signal_period);
        // Crossing the whole-series threshold reveals values on earlier bars too.
        let from = if self.stepper.state().usable < threshold {
            0
        } else {
            from
        };
        let fast_mult = 2.0 / (self.fast_period as f64 + 1.0);
        let slow_mult = 2.0 / (self.slow_period as f64 + 1.0);
        let signal_mult = 2.0 / (self.signal_period as f64 + 1.0);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                if state.usable == 0 {
                    state.fast = bar.close;
                    state.slow = bar.close;
                } else {
                    state.fast = (bar.close - state.fast) * fast_mult + state.fast;
                    state.slow = (bar.close - state.slow) * slow_mult + state.slow;
                }
                let macd = state.fast - state.slow;
                state.signal = if state.usable == 0 {
                    macd
                } else {
                    (macd - state.signal) * signal_mult + state.signal
                };
                state.usable += 1;
                IndicatorValue::Multiple(vec![macd, state.signal, macd - state.signal])
            });
        if self.stepper.state().usable < threshold {
            self.values.fill(IndicatorValue::None);
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
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
