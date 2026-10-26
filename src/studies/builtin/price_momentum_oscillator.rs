//! Price Momentum Oscillator (PMO), Carl Swenlin (DecisionPoint).
//!
//! A doubly smoothed one-bar rate of change, with an exponential signal line.
//!
//! # Formula
//!
//! ```text
//! ROC1   = (close / previous close - 1) * 100
//! csf(x, n): prev + (x - prev) * 2 / n
//! PMO    = csf(10 * csf(ROC1, length 1), length 2)
//! Signal = EMA(PMO, signal length)
//! ```
//!
//! The custom smoothing uses the factor `2 / n` instead of the usual `2 / (n + 1)`, so its lengths
//! are at least 2: at 1 the factor of 2 overshoots and the error grows. Each smoothing,
//! the signal EMA included, starts at the mean of its first `n` inputs. The rate of change starts on
//! bar 1, so the PMO starts on bar `length 1 + length 2 - 1` and the signal `signal length - 1` bars
//! later. A bar following a zero close has no rate of change, and the smoothing carries over it.
//! Defaults: 35, 20 and 10.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..80).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, PriceMomentumOscillator};
//!
//! let mut pmo = PriceMomentumOscillator::default();
//! pmo.calculate(&bars);
//! assert!(pmo.values().value(0, 54).is_some());
//! assert!(pmo.values().value(1, 63).is_some());
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Price Momentum Oscillator: PMO and signal lines in a separate pane.
#[derive(Clone)]
pub struct PriceMomentumOscillator {
    first_length: usize,
    second_length: usize,
    signal_length: usize,
    values: IndicatorValues,
    /// Colors: [PMO, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

/// Recursive smoothing `prev + (x - prev) * alpha`, starting at the mean of the first `length` inputs.
struct Smoothing {
    length: usize,
    alpha: f64,
    seen: usize,
    value: f64,
}

impl Smoothing {
    fn new(length: usize, alpha: f64) -> Self {
        Self {
            length,
            alpha,
            seen: 0,
            value: 0.0,
        }
    }

    fn push(&mut self, x: f64) -> Option<f64> {
        if self.seen < self.length {
            self.seen += 1;
            self.value += (x - self.value) / self.seen as f64;
            return (self.seen == self.length).then_some(self.value);
        }
        self.value += (x - self.value) * self.alpha;
        Some(self.value)
    }
}

impl PriceMomentumOscillator {
    /// Create a PMO with the two smoothing lengths and the signal length (35, 20 and 10 by convention).
    pub fn new(first_length: usize, second_length: usize, signal_length: usize) -> Self {
        Self {
            first_length: first_length.max(2),
            second_length: second_length.max(2),
            signal_length: signal_length.max(1),
            values: IndicatorValues::new(),
            colors: vec![palette::BLUE, palette::ORANGE],
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let mut first = Smoothing::new(self.first_length, 2.0 / self.first_length as f64);
        let mut second = Smoothing::new(self.second_length, 2.0 / self.second_length as f64);
        let mut signal =
            Smoothing::new(self.signal_length, 2.0 / (self.signal_length as f64 + 1.0));
        let mut previous: Option<f64> = None;
        for bar in data {
            let roc = previous.map(|previous| (bar.close / previous - 1.0) * 100.0);
            previous = Some(bar.close);
            let pmo = roc
                .filter(|roc| roc.is_finite())
                .and_then(|roc| first.push(roc))
                .and_then(|smoothed| second.push(10.0 * smoothed));
            match pmo {
                Some(pmo) => {
                    values.push_lines(&[pmo, signal.push(pmo).unwrap_or(f64::NAN)]);
                }
                None => values.push(IndicatorValue::None),
            }
        }
        values
    }
}

impl Default for PriceMomentumOscillator {
    fn default() -> Self {
        Self::new(35, 20, 10)
    }
}

impl Indicator for PriceMomentumOscillator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        first_length: length("Length 1", 2, usize::MAX),
        second_length: length("Length 2", 2, usize::MAX),
        signal_length: length("Signal length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Price Momentum Oscillator"
    }

    fn desc(&self) -> &str {
        "Price Momentum Oscillator - Doubly smoothed rate of change with a signal line"
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

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: None,
            levels: vec![(0.0, palette::GRAY, "Zero Line")],
            fill_between_levels: false,
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

    fn line_count(&self) -> usize {
        2
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("PMO({},{})", self.first_length, self.second_length),
            format!("Signal({})", self.signal_length),
        ]
    }
}
