//! Relative Volatility Index (RVI), Donald Dorsey (1993).
//!
//! Built like the RSI, but it accumulates the standard deviation of the close instead of the size
//! of the price change: volatility on rising bars against volatility on falling bars.
//!
//! # Formula
//!
//! ```text
//! sd   = population standard deviation of the close over `length`
//! up   = EMA(close > previous close ? sd : 0, 14)
//! down = EMA(close > previous close ? 0 : sd, 14)
//! RVI  = 100 * up / (up + down)
//! ```
//!
//! An unchanged close counts as a falling bar. Both averages start at their first input, on the
//! first bar with a full window and a previous close (bar `length - 1`, bar 1 for a length of 1).
//! A bar where both averages are zero has no value. Length defaults to 10; the smoothing length is
//! fixed at 14. Levels at 80, 50 and 20 with the band between 80 and 20 filled.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, RelativeVolatilityIndex};
//!
//! let mut rvi = RelativeVolatilityIndex::default();
//! rvi.calculate(&bars);
//! let value = rvi.values().value(0, 30).unwrap();
//! assert!((0.0..=100.0).contains(&value));
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Length of the exponential averages of the up and down volatility.
const SMOOTHING: f64 = 14.0;

/// Relative Volatility Index, a 0 to 100 oscillator in a separate pane.
#[derive(Clone)]
pub struct RelativeVolatilityIndex {
    length: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl RelativeVolatilityIndex {
    /// Create an RVI over a standard deviation of `length` closes (10 by convention).
    pub fn new(length: usize) -> Self {
        Self {
            length: length.max(1),
            values: IndicatorValues::new(),
            color: palette::DEEP_PURPLE,
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let alpha = 2.0 / (SMOOTHING + 1.0);
        let mut averages: Option<(f64, f64)> = None;
        let mut values = IndicatorValues::new();
        for (i, bar) in data.iter().enumerate() {
            let (Some(start), Some(previous)) =
                ((i + 1).checked_sub(self.length), i.checked_sub(1))
            else {
                values.push(IndicatorValue::None);
                continue;
            };
            let window = &data[start..=i];
            let mean = window.iter().map(|b| b.close).sum::<f64>() / self.length as f64;
            let variance =
                window.iter().map(|b| (b.close - mean).powi(2)).sum::<f64>() / self.length as f64;
            let sd = variance.sqrt();
            let (up, down) = if bar.close > data[previous].close {
                (sd, 0.0)
            } else {
                (0.0, sd)
            };
            let (up, down) = match averages {
                Some((u, d)) => (u + (up - u) * alpha, d + (down - d) * alpha),
                None => (up, down),
            };
            averages = Some((up, down));
            values.push(if up + down > 0.0 {
                IndicatorValue::Single(100.0 * up / (up + down))
            } else {
                IndicatorValue::None
            });
        }
        values
    }
}

impl Default for RelativeVolatilityIndex {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for RelativeVolatilityIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        length: length("Length", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Relative Volatility Index"
    }

    fn desc(&self) -> &str {
        "Relative Volatility Index - Direction of volatility (0-100)"
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

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = palette::GRAY;
        crate::studies::StudyPane {
            height: None,
            range: Some((0.0, 100.0)),
            levels: vec![
                (80.0, gray, "Upper Band"),
                (50.0, gray, "Middle Band"),
                (20.0, gray, "Lower Band"),
            ],
            fill_between_levels: true,
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
        vec![format!("RVI({})", self.length)]
    }
}
