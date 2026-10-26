//! Weighted Moving Avg (WMA) Indicator
//!
//! WMA assigns greater weight to more recent data points, making it more
//! responsive to recent price changes than SMA.
//!
//! # Formula
//! WMA = (P1 * n + P2 * (n-1) + ... + Pn * 1) / (n * (n+1) / 2)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{WMA, Indicator};
//!
//! let mut wma = WMA::new(14);
//! wma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Weighted Moving Avg indicator
#[derive(Clone)]
pub struct WMA {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl WMA {
    /// Create a new WMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation (typically 14, 20, or 50)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for WMA {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for WMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "WMA"
    }

    fn desc(&self) -> &str {
        "Weighted Moving Avg - Gives more weight to recent prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Weight sum: n*(n+1)/2
        let weight_sum = self.period as f64 * (self.period as f64 + 1.0) / 2.0;

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let mut weighted_sum = 0.0;
                for j in 0..self.period {
                    let weight = (self.period - j) as f64;
                    weighted_sum += data[i - j].close * weight;
                }
                let wma = weighted_sum / weight_sum;
                self.values.push(IndicatorValue::Single(wma));
            }
        }
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

    fn line_count(&self) -> usize {
        1
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
        vec![format!("WMA({})", self.period)]
    }
}
