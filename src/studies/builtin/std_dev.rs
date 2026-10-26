//! Standard Deviation Indicator
//!
//! Standard Deviation measures price volatility by calculating the dispersion
//! of prices from their moving avg.
//!
//! # Formula
//! StdDev = sqrt(Σ(price - SMA)² / period)
//!
//! # Interpretation
//! - High StdDev: High volatility, larger price movements
//! - Low StdDev: Low volatility, consolidation period
//! - Often used as component of Bollinger Bands
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{StandardDeviation, Indicator};
//!
//! let mut std_dev = StandardDeviation::new(20);
//! std_dev.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Standard Deviation indicator
#[derive(Clone)]
pub struct StandardDeviation {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl StandardDeviation {
    /// Create a new Standard Deviation indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.favorite_gold,
            visible: true,
        }
    }

    /// Create with default param (20)
    pub fn default_params() -> Self {
        Self::new(20)
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for StandardDeviation {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for StandardDeviation {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "StdDev"
    }

    fn desc(&self) -> &str {
        "Standard Deviation - Measures price volatility"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;

                // Calculate mean (SMA)
                let sum: f64 = data[start..=i].iter().map(|b| b.close).sum();
                let mean = sum / self.period as f64;

                // Calculate variance
                let variance: f64 = data[start..=i]
                    .iter()
                    .map(|b| (b.close - mean).powi(2))
                    .sum::<f64>()
                    / self.period as f64;

                // Standard deviation is square root of variance
                let std_dev = variance.sqrt();
                self.values.push(IndicatorValue::Single(std_dev));
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
        false // StdDev is typically in a separate pane
    }

    fn line_cnt(&self) -> usize {
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
        vec![format!("StdDev({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_std_dev_flat_prices() {
        // 1234.56 is inexact in binary, so a one-pass variance goes slightly negative here.
        let start = Utc::now();
        let bars: Vec<Bar> = (0..30)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 1234.56,
                high: 1234.56,
                low: 1234.56,
                close: 1234.56,
                volume: 1000.0,
                colors: None,
                payload: None,
            })
            .collect();

        let mut std_dev = StandardDeviation::new(20);
        std_dev.calculate(&bars);

        for value in std_dev.values().to_rows().iter().skip(19) {
            assert!(
                matches!(value, IndicatorValue::Single(v) if v.abs() < 1e-9),
                "a window without deviation reads 0, got {value:?}"
            );
        }
    }
}
