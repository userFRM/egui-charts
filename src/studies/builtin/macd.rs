//! Moving Average Convergence Divergence (MACD) indicator.
//!
//! MACD is a trend-following momentum indicator that shows the
//! relationship between two moving averages of closing prices,
//! exponential by default. It consists of three output lines plotted in
//! a separate sub-pane.
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
//! # Averages
//!
//! Each exponential average starts at its first value. With simple averages the MACD line starts
//! once both have a full window; a simple signal is the mean of the last `signal_period` MACD
//! values, and an exponential one starts at the first MACD value.
//!
//! # Default colours
//!
//! - MACD line: blue ([`palette::BLUE`](crate::studies::palette::BLUE))
//! - Signal line: orange ([`palette::ORANGE`](crate::studies::palette::ORANGE))
//! - Histogram: teal above zero and red below, with lighter tints toward zero.
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

use super::price_oscillator::{Mean, OscillatorAverage};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
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
    /// Formula of the fast and slow averages.
    oscillator_average: OscillatorAverage,
    /// Formula of the signal line.
    signal_average: OscillatorAverage,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    usable: usize,
    /// Bars with a MACD value.
    defined: usize,
    fast: f64,
    slow: f64,
    signal: f64,
    /// The fast, slow and signal simple averages.
    means: [Mean; 3],
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
            oscillator_average: OscillatorAverage::Exponential,
            signal_average: OscillatorAverage::Exponential,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,           // MACD line
                palette::ORANGE,         // Signal line
                palette::HISTOGRAM_TEAL, // Histogram: positive and rising
                palette::LIGHT_TEAL,
                palette::HISTOGRAM_RED,
                palette::LIGHT_RED,
            ],
            visible: true,
        }
    }

    /// Choose the formula of the fast and slow averages and of the signal line; both exponential
    /// by default.
    pub fn with_averages(
        mut self,
        oscillator: OscillatorAverage,
        signal: OscillatorAverage,
    ) -> Self {
        self.oscillator_average = oscillator;
        self.signal_average = signal;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for MACD {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl Indicator for MACD {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        fast_period: length("Fast period", 1, usize::MAX),
        slow_period: length("Slow period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX),
        oscillator_average: choice("Oscillator MA", [OscillatorAverage::Exponential, OscillatorAverage::Simple]),
        signal_average: choice("Signal MA", [OscillatorAverage::Exponential, OscillatorAverage::Simple])
    ; {});

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 2 {
                crate::studies::PlotType::Columns
            } else {
                crate::studies::PlotType::Line
            },
            visible: true,
        }
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        if line != 2 {
            return None;
        }
        histogram_color(&self.values, bar, &self.colors[2..])
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
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
                let (fast, slow) = match self.oscillator_average {
                    OscillatorAverage::Exponential => {
                        if state.usable == 0 {
                            state.fast = bar.close;
                            state.slow = bar.close;
                        } else {
                            state.fast = (bar.close - state.fast) * fast_mult + state.fast;
                            state.slow = (bar.close - state.slow) * slow_mult + state.slow;
                        }
                        (state.fast, state.slow)
                    }
                    OscillatorAverage::Simple => (
                        state.means[0].next(
                            OscillatorAverage::Simple,
                            bar.close,
                            self.fast_period,
                        ),
                        state.means[1].next(
                            OscillatorAverage::Simple,
                            bar.close,
                            self.slow_period,
                        ),
                    ),
                };
                state.usable += 1;
                let macd = fast - slow;
                // A simple average still filling its window.
                if !macd.is_finite() {
                    return IndicatorValue::None;
                }
                let signal = match self.signal_average {
                    OscillatorAverage::Exponential => {
                        state.signal = if state.defined == 0 {
                            macd
                        } else {
                            (macd - state.signal) * signal_mult + state.signal
                        };
                        state.signal
                    }
                    OscillatorAverage::Simple => state.means[2].next(
                        OscillatorAverage::Simple,
                        macd,
                        self.signal_period,
                    ),
                };
                state.defined += 1;
                IndicatorValue::Multiple(vec![macd, signal, macd - signal])
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
            if self.colors.len() < 6 {
                self.colors.truncate(3);
            }
            self.colors.resize(6, self.colors[2]);
        } else if !colors.is_empty() {
            self.colors = vec![colors[0]; 6];
        }
    }

    fn set_color(&mut self, line: usize, color: Color32) {
        match line {
            0 | 1 => self.colors[line] = color,
            2 => self.colors[2..].fill(color),
            _ => {}
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: None,
            levels: vec![(0.0, palette::MUTED.gamma_multiply_u8(128), "Zero Line")],
            fill_between_levels: false,
        }
    }

    fn is_overlay(&self) -> bool {
        false // MACD is drawn in separate pane
    }

    fn line_count(&self) -> usize {
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

/// Column colour of a histogram in output line 2 from `shades`: rising and easing above zero,
/// then falling and recovering below it.
pub(super) fn histogram_color(
    values: &IndicatorValues,
    bar: usize,
    shades: &[Color32],
) -> Option<Color32> {
    let value = values.value(2, bar)?;
    let previous = (0..bar).rev().find_map(|bar| values.value(2, bar));
    let slot = if value >= 0.0 {
        usize::from(!previous.is_some_and(|p| value > p))
    } else {
        2 + usize::from(previous.is_some_and(|p| value > p))
    };
    shades.get(slot).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::ConfiguredStudy;

    #[test]
    fn histogram_palette_survives_edits_and_saved_explicit_colours() {
        let study = MACD {
            values: [1.0, 2.0, 1.5, f64::NAN, -1.0, -2.0, -1.5, 0.0]
                .into_iter()
                .map(|value| IndicatorValue::Multiple(vec![0.0, 0.0, value]))
                .collect(),
            ..MACD::default()
        };
        let expected = [
            Some(palette::LIGHT_TEAL),
            Some(palette::HISTOGRAM_TEAL),
            Some(palette::LIGHT_TEAL),
            None,
            Some(palette::HISTOGRAM_RED),
            Some(palette::HISTOGRAM_RED),
            Some(palette::LIGHT_RED),
            Some(palette::HISTOGRAM_TEAL),
        ];
        let colours = |study: &dyn Indicator| -> Vec<_> {
            (0..8).map(|bar| study.plot_color(2, bar)).collect()
        };
        assert_eq!(colours(&study), expected);
        assert_eq!(study.plot_color(0, 1), None);
        let mut configured = ConfiguredStudy::new(Box::new(study.clone()));
        configured
            .apply_settings(&[], configured.settings())
            .unwrap();
        assert_eq!(colours(&configured), expected);
        let mut settings = configured.settings();
        settings.plots[0].color = Color32::YELLOW;
        configured.apply_settings(&[], settings).unwrap();
        assert_eq!(colours(&configured), expected);
        let snapshot = configured.snapshot_settings();
        configured.set_color(2, Color32::WHITE);
        assert_eq!(configured.plot_color(2, 1), Some(Color32::WHITE));
        configured.restore_settings(&snapshot).unwrap();
        assert_eq!(colours(&configured), expected);
        // A saved single histogram colour stays solid even when it equals the new up colour.
        for color in [palette::UP, palette::HISTOGRAM_TEAL, Color32::YELLOW] {
            let mut legacy = study.clone();
            legacy.set_colors(vec![palette::BLUE, palette::ORANGE, color]);
            assert!(
                (0..8)
                    .filter(|&bar| bar != 3)
                    .all(|bar| legacy.plot_color(2, bar) == Some(color))
            );
            let snapshot = legacy.snapshot_settings();
            configured.restore_settings(&snapshot).unwrap();
            assert_eq!(colours(&configured), colours(&legacy));
        }
    }
}
