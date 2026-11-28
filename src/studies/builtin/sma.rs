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
//! Blue from [`palette::BLUE`](crate::studies::palette::BLUE).
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{SMA, Indicator, IndicatorValue};
//!
//! let mut sma = SMA::new(20);
//! sma.calculate(&bars);
//! // The first 19 values are warmup.
//! assert!(sma.values().to_rows()[..19].iter().all(|v| matches!(v, IndicatorValue::None)));
//! assert!(matches!(sma.values().to_rows()[19], IndicatorValue::Single(_)));
//! ```

use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Simple Moving Average indicator.
///
/// Computes the arithmetic mean of closing prices over a rolling window of
/// `period` bars. This is an overlay indicator (drawn on the price chart).
#[derive(Clone, Debug)]
pub struct SMA {
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(VecDeque<f64>, Smoothed)>,
    color: Color32,
    smoothing: SmoothingType,
    smoothing_length: usize,
    smoothing_deviation: f64,
    smoothing_colors: [Color32; 3],
    visible: bool,
}

impl SMA {
    /// Create a new SMA indicator.
    ///
    /// # Arguments
    /// * `period` -- The number of bars in the averaging window (e.g. 20, 50, 200).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::BLUE,
            smoothing: SmoothingType::None,
            smoothing_length: 14,
            smoothing_deviation: 2.0,
            smoothing_colors: SMOOTHING_COLORS,
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
impl Default for SMA {
    fn default() -> Self {
        Self::new(9)
    }
}

impl Indicator for SMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smoothing: choice("Smoothing", SmoothingType::ALL),
        smoothing_length: length("Smoothing length", 1, usize::MAX)
            if smoothing in SmoothingType::WITH_LENGTH,
        smoothing_deviation: number("Band deviation", 0.0, f64::MAX)
            if smoothing in SmoothingType::WITH_BANDS
    ; {});

    fn name(&self) -> &str {
        "SMA"
    }

    fn desc(&self) -> &str {
        "Simple Moving Average - Avg price over N periods"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, kind, length, deviation) = (
            self.period,
            self.smoothing,
            self.smoothing_length,
            self.smoothing_deviation,
        );
        self.stepper
            .run(data, from, &mut self.values, |(window, smoothed), bar| {
                window.push_back(bar.close);
                if window.len() > period {
                    window.pop_front();
                }
                if window.len() < period {
                    return IndicatorValue::None;
                }
                // Sum each window in source order to avoid accumulated rounding drift.
                let mean = window.iter().sum::<f64>() / period as f64;
                if !mean.is_finite() {
                    return IndicatorValue::None;
                }
                smoothed.next(kind, length, deviation, mean, bar.volume)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        let [average, upper, lower] = self.smoothing_colors;
        vec![self.color, average, upper, lower]
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        self.smoothing.fills()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
            for (slot, color) in self.smoothing_colors.iter_mut().zip(&colors[1..]) {
                *slot = *color;
            }
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
        let mut names = vec![format!("SMA({})", self.period)];
        names.extend(self.smoothing.line_names("MA-based MA"));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    /// Closes 11 to 17 give a 5-bar SMA of 13, 14 and 15, the means of 11 to 15, 12 to 16
    /// and 13 to 17.
    #[test]
    fn rolling_means_of_five_closes() {
        let start = Utc::now();
        let bars: Vec<Bar> = (11..=17)
            .enumerate()
            .map(|(i, c)| {
                let c = f64::from(c);
                Bar {
                    time: start + Duration::minutes(i as i64),
                    open: c,
                    high: c,
                    low: c,
                    close: c,
                    volume: 1.0,
                    colors: None,
                    payload: None,
                }
            })
            .collect();

        let mut sma = SMA::new(5);
        sma.calculate(&bars);

        let got: Vec<Option<f64>> = sma
            .values()
            .to_rows()
            .iter()
            .map(|v| match v {
                IndicatorValue::Single(x) => Some(*x),
                _ => None,
            })
            .collect();
        assert_eq!(
            got,
            [None, None, None, None, Some(13.0), Some(14.0), Some(15.0)]
        );
    }
}
