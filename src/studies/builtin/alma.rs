//! Arnaud Legoux Moving Avg (ALMA) Indicator
//!
//! ALMA uses a Gaussian distribution to weight prices, providing a smooth
//! moving avg with reduced lag. The offset param controls the
//! weight distribution, and sigma controls the smoothness.
//!
//! # Formula
//! ALMA = Σ(weight[i] * price[i]) / Σ(weight[i])
//! Where weight[i] = exp(-((i - offset * (period - 1))^2) / (2 * sigma^2 * period^2))
//!
//! # Params
//! - period: Lookback period (typically 9)
//! - offset: Weight offset 0-1 (typically 0.85, higher = more weight to recent)
//! - sigma: Gaussian sigma (typically 6, higher = smoother)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ALMA, Indicator};
//!
//! let mut alma = ALMA::new(9, 0.85, 6.0);
//! alma.calculate(&bars);
//! ```

use super::clamp_param;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Arnaud Legoux Moving Avg indicator
#[derive(Clone)]
pub struct ALMA {
    period: usize,
    offset: f64,
    sigma: f64,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl ALMA {
    /// Create a new ALMA indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 9)
    /// * `offset` - Weight offset (typically 0.85), clamped into 0 to 1, NaN to 0
    /// * `sigma` - Gaussian sigma (typically 6.0), clamped into 0.001 to 100, NaN
    ///   to 0.001; a larger sigma narrows the weights until they all underflow to zero
    pub fn new(period: usize, offset: f64, sigma: f64) -> Self {
        let period = period.max(1);
        let offset = clamp_param(offset, 0.0, 1.0);
        let sigma = clamp_param(sigma, 0.001, 100.0);

        Self {
            period,
            offset,
            sigma,
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_orange, // Deep Orange
            visible: true,
        }
    }

    /// Create with default params (9, 0.85, 6.0)
    pub fn default_params() -> Self {
        Self::new(9, 0.85, 6.0)
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ALMA {
    fn default() -> Self {
        Self::new(9, 0.85, 6.0)
    }
}

impl Indicator for ALMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        offset: number("Offset", 0.0, 1.0),
        sigma: number("Sigma", 0.001, 100.0)
    ; {});

    fn name(&self) -> &str {
        "ALMA"
    }

    fn desc(&self) -> &str {
        "Arnaud Legoux Moving Avg - Gaussian weighted moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            self.values.resize(data.len(), IndicatorValue::None);
            return;
        }

        // Gaussian weights, computed here so their size is bounded by the data
        let m = self.offset * (self.period - 1) as f64;
        let s = self.period as f64 / self.sigma;
        let weights: Vec<f64> = (0..self.period)
            .map(|i| (-((i as f64 - m).powi(2)) / (2.0 * s * s)).exp())
            .collect();
        let weight_sum: f64 = weights.iter().sum();

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let mut weighted_sum = 0.0;
                for j in 0..self.period {
                    let idx = i + 1 - self.period + j;
                    weighted_sum += weights[j] * data[idx].close;
                }
                let alma = weighted_sum / weight_sum;
                self.values.push(IndicatorValue::Single(alma));
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
        vec![format!(
            "ALMA({}, {}, {})",
            self.period, self.offset, self.sigma
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..30)
            .map(|i| {
                let price = 100.0 + i as f64;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                    colors: None,
                    payload: None,
                }
            })
            .collect()
    }

    #[test]
    fn test_alma_calculation() {
        let bars = create_test_bars();
        let mut alma = ALMA::new(9, 0.85, 6.0);
        alma.calculate(&bars);

        assert_eq!(alma.values().len(), bars.len());

        // First period-1 values should be None
        for i in 0..8 {
            assert!(matches!(alma.values().to_rows()[i], IndicatorValue::None));
        }

        // Check valid values exist
        for value in alma.values().to_rows().iter().skip(8) {
            assert!(matches!(value, IndicatorValue::Single(_)));
        }
    }
}
