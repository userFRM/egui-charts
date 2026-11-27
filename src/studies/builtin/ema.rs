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
//! # First value
//!
//! The first value is on the bar of the `period`-th finite close and equals the simple average
//! of the first `period` finite closes; earlier bars have no value. Smoothing starts on the
//! next bar.
//!
//! # Interpretation
//!
//! - Faster to react to price changes than SMA of the same period.
//! - Common periods: 12 and 26 (used in MACD), 9 (short-term), 50/200.
//! - EMA crossovers are popular entry/exit signals.
//!
//! # Default colour
//!
//! Blue from [`palette::BLUE`](crate::studies::palette::BLUE).
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

use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Exponential Moving Average indicator.
///
/// Applies an exponential weighting to closing prices so that recent bars
/// have a greater influence. This is an overlay indicator.
#[derive(Clone, Debug)]
pub struct EMA {
    period: usize,
    values: IndicatorValues,
    /// The exponential average of the finite closes stepped so far.
    stepper: Stepper<Smoothing>,
    color: Color32,
    visible: bool,
}

impl EMA {
    #[cfg(feature = "widget")]
    pub(crate) fn replay_period(&self) -> usize {
        self.period
    }

    /// Create a new EMA indicator.
    ///
    /// # Arguments
    /// * `period` -- The smoothing period (e.g. 12, 26, 50).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::BLUE, // Blue
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
        Self::new(9)
    }
}

impl Indicator for EMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "EMA"
    }

    fn desc(&self) -> &str {
        "Exponential Moving Average - Weighted avg giving more importance to recent prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, alpha) = (self.period, 2.0 / (self.period as f64 + 1.0));
        // A bar without a finite close has no value and leaves the average as it was.
        self.stepper.run(data, from, &mut self.values, |ema, bar| {
            IndicatorValue::Single(ema.next(bar.close, period, alpha))
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

    /// EMA(4) seeds with the mean of its first four finite closes on the bar of the fourth;
    /// the NaN close neither counts nor has a value.
    #[test]
    fn seeds_with_the_mean_of_its_first_finite_closes() {
        let closes = [22.27, 22.19, f64::NAN, 22.08, 22.17, 22.18, 22.13];
        let expected = [
            None,
            None,
            None,
            None,
            Some(22.1775),
            Some(22.1785),
            Some(22.1591),
        ];
        let bars = bars_from_closes(&closes);
        let mut ema = EMA::new(4);
        ema.calculate(&bars);

        assert_eq!(ema.values().len(), expected.len());
        for (i, expected) in expected.into_iter().enumerate() {
            let actual = ema.values().value(0, i);
            match (actual, expected) {
                (Some(actual), Some(expected)) => assert!(
                    (actual - expected).abs() < 1e-9,
                    "bar {i}: {actual} != {expected}"
                ),
                _ => assert_eq!(actual, expected, "bar {i}"),
            }
        }
    }
}
