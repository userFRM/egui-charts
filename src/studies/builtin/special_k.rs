//! Pring's Special K, Martin Pring.
//!
//! A weighted sum of simple moving averages of the rate of change over short, intermediate and long
//! lookbacks, built to peak and trough with the primary trend.
//!
//! # Formula
//!
//! ```text
//! ROC(n)    = (close / close[n] - 1) * 100
//! Special K = sum of weight * SMA(ROC(roc length), sma length) over
//!             (10, 10, 1) (15, 10, 2) (20, 10, 3) (30, 15, 4)
//!             (40, 50, 1) (65, 65, 2) (75, 75, 3) (100, 100, 4)
//!             (195, 130, 1) (265, 130, 2) (390, 130, 3) (530, 195, 4)
//! Signal    = SMA(SMA(Special K, signal length 1), signal length 2)
//! ```
//!
//! The components are fixed by the definition; the two signal lengths default to 100. The longest
//! component needs 725 bars, so the Special K starts on bar 724 and the signal 198 bars later by
//! default. A rate of change from a zero close has no value and leaves its averages without one.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..930).map(|i| { let p = 100.0 + (i as f64 * 0.03).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, PringsSpecialK};
//!
//! let mut special_k = PringsSpecialK::default();
//! special_k.calculate(&bars);
//! assert!(special_k.values().value(0, 723).is_none());
//! assert!(special_k.values().value(0, 724).is_some());
//! assert!(special_k.values().value(1, 922).is_some());
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::rolling_mean;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// (rate of change length, SMA length, weight) of each component.
const COMPONENTS: [(usize, usize, f64); 12] = [
    (10, 10, 1.0),
    (15, 10, 2.0),
    (20, 10, 3.0),
    (30, 15, 4.0),
    (40, 50, 1.0),
    (65, 65, 2.0),
    (75, 75, 3.0),
    (100, 100, 4.0),
    (195, 130, 1.0),
    (265, 130, 2.0),
    (390, 130, 3.0),
    (530, 195, 4.0),
];

/// Pring's Special K and its doubly smoothed signal line, in a separate pane.
#[derive(Clone)]
pub struct PringsSpecialK {
    signal_lengths: [usize; 2],
    values: IndicatorValues,
    /// Colors: [Special K, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl PringsSpecialK {
    /// Create a Special K with the lengths of its two signal smoothings (100 and 100 by convention).
    pub fn new(signal_length_1: usize, signal_length_2: usize) -> Self {
        Self {
            signal_lengths: [signal_length_1.max(1), signal_length_2.max(1)],
            values: IndicatorValues::new(),
            colors: vec![palette::BLUE, palette::RED],
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();
        let mut special_k = vec![0.0; closes.len()];
        for (roc_length, sma_length, weight) in COMPONENTS {
            let roc: Vec<f64> = (0..closes.len())
                .map(|i| match i.checked_sub(roc_length) {
                    Some(back) if closes[back] != 0.0 => (closes[i] / closes[back] - 1.0) * 100.0,
                    _ => f64::NAN,
                })
                .collect();
            for (sum, average) in special_k.iter_mut().zip(rolling_mean(&roc, sma_length)) {
                *sum += weight * average;
            }
        }
        let signal = rolling_mean(
            &rolling_mean(&special_k, self.signal_lengths[0]),
            self.signal_lengths[1],
        );
        let mut values = IndicatorValues::new();
        for (value, signal) in special_k.into_iter().zip(signal) {
            if value.is_finite() {
                values.push_lines(&[value, signal]);
            } else {
                values.push(IndicatorValue::None);
            }
        }
        values
    }
}

impl Default for PringsSpecialK {
    fn default() -> Self {
        Self::new(100, 100)
    }
}

impl Indicator for PringsSpecialK {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        signal_lengths[0]: length("Signal length 1", 1, usize::MAX),
        signal_lengths[1]: length("Signal length 2", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Pring's Special K"
    }

    fn desc(&self) -> &str {
        "Pring's Special K - Weighted sum of smoothed rates of change across cycles"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 2 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        false
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

    fn line_cnt(&self) -> usize {
        2
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            "Special K".to_string(),
            format!(
                "Signal({},{})",
                self.signal_lengths[0], self.signal_lengths[1]
            ),
        ]
    }
}
