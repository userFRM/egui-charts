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
//! Each exponential average starts from the simple average of its first values. The MACD line
//! starts once both averages have a value, on bar `max(fast, slow) - 1`. The signal, simple or
//! exponential, averages only MACD values, so it and the histogram start `signal - 1` bars later
//! (bar 33 with 12, 26 and 9). A bar with a non-finite close has no value and is skipped by the
//! averages.
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
#[derive(Clone, Debug)]
pub struct MACD {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    /// Formula of the fast and slow averages.
    oscillator_average: OscillatorAverage,
    /// Formula of the signal line.
    signal_average: OscillatorAverage,
    values: IndicatorValues,
    stepper: Stepper<[Mean; 3]>,
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
        self.stepper
            .run(data, from, &mut self.values, |means, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let average = self.oscillator_average;
                let macd = means[0].next(average, bar.close, self.fast_period)
                    - means[1].next(average, bar.close, self.slow_period);
                // Either average still filling its window.
                if !macd.is_finite() {
                    return IndicatorValue::None;
                }
                let signal = means[2].next(self.signal_average, macd, self.signal_period);
                IndicatorValue::Multiple(vec![macd, signal, macd - signal])
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn alert_conditions(&self) -> Vec<String> {
        ["Crosses above signal line", "Crosses below signal line"]
            .map(str::to_owned)
            .into()
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        let previous = bar.checked_sub(1)?;
        let macd = self.values.value(0, bar)?;
        let signal = self.values.value(1, bar)?;
        let before = self.values.value(0, previous)?;
        let before_signal = self.values.value(1, previous)?;
        match condition {
            0 => Some(before < before_signal && macd >= signal),
            1 => Some(before > before_signal && macd <= signal),
            _ => None,
        }
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

    /// The histogram, then the MACD and signal lines.
    fn legend_lines(&self) -> Vec<usize> {
        vec![2, 0, 1]
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
