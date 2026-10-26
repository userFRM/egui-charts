//! Ulcer Index
//!
//! Peter Martin's Ulcer Index measures downside risk: the root mean square of the percentage
//! drawdowns of the close from its highest value over the lookback. It rises with both the depth
//! and the duration of declines, and ignores rises.
//!
//! # Formula
//! ```text
//! highest  = highest close over the last `length` bars
//! drawdown = 100 * (close - highest) / highest
//! Ulcer Index = sqrt(SMA(drawdown^2, length))
//! ```
//! with conventional length 14. A drawdown needs a complete window, so the first value is on
//! bar `2 * length - 2`.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, UlcerIndex};
//!
//! let mut ulcer = UlcerIndex::new(14);
//! ulcer.calculate(&bars);
//! assert!(ulcer.values().value(0, 25).is_none());
//! assert!(ulcer.values().value(0, 26).is_some());
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{rolling_max, rolling_mean};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Root mean square drawdown from the rolling highest close.
#[derive(Clone)]
pub struct UlcerIndex {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl UlcerIndex {
    /// Create an Ulcer Index over `period` bars (conventionally 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let highest = rolling_max(data.iter().map(|b| b.close), self.period);
        let squares: Vec<f64> = data
            .iter()
            .zip(highest)
            .enumerate()
            .map(|(i, (bar, highest))| {
                if i + 1 < self.period {
                    return f64::NAN;
                }
                let drawdown = 100.0 * (bar.close - highest) / highest;
                drawdown * drawdown
            })
            .collect();
        rolling_mean(&squares, self.period)
            .into_iter()
            .map(|mean| IndicatorValue::Single(mean.sqrt()))
            .collect()
    }
}

impl Default for UlcerIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for UlcerIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Ulcer Index"
    }

    fn desc(&self) -> &str {
        "Ulcer Index - Depth and duration of drawdowns from the recent high"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("Ulcer Index({})", self.period)]
    }
}
