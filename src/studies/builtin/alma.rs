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
//! ```ignore
//! use egui_charts::ALMA;
//!
//! let mut alma = ALMA::new(9, 0.85, 6.0);
//! alma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Arnaud Legoux Moving Avg indicator
#[derive(Clone)]
pub struct ALMA {
    period: usize,
    offset: f64,
    sigma: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
    weights: Vec<f64>,
    weight_sum: f64,
}

impl ALMA {
    /// Create a new ALMA indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 9)
    /// * `offset` - Weight offset 0-1 (typically 0.85)
    /// * `sigma` - Gaussian sigma (typically 6.0)
    pub fn new(period: usize, offset: f64, sigma: f64) -> Self {
        let period = period.max(1);
        let offset = offset.clamp(0.0, 1.0);
        let sigma = sigma.max(0.001);

        // Pre-calculate weights
        let m = offset * (period - 1) as f64;
        let s = period as f64 / sigma;

        let mut weights = Vec::with_capacity(period);
        let mut weight_sum = 0.0;

        for i in 0..period {
            let w = (-((i as f64 - m).powi(2)) / (2.0 * s * s)).exp();
            weights.push(w);
            weight_sum += w;
        }

        Self {
            period,
            offset,
            sigma,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_orange, // Deep Orange
            visible: true,
            weights,
            weight_sum,
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

impl Indicator for ALMA {
    fn name(&self) -> &str {
        "ALMA"
    }

    fn desc(&self) -> &str {
        "Arnaud Legoux Moving Avg - Gaussian weighted moving avg"
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
                let mut weighted_sum = 0.0;
                for j in 0..self.period {
                    let idx = i + 1 - self.period + j;
                    weighted_sum += self.weights[j] * data[idx].close;
                }
                let alma = weighted_sum / self.weight_sum;
                self.values.push(IndicatorValue::Single(alma));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
            assert!(matches!(alma.values()[i], IndicatorValue::None));
        }

        // Check valid values exist
        for value in alma.values().iter().skip(8) {
            assert!(matches!(value, IndicatorValue::Single(_)));
        }
    }

    #[test]
    fn test_alma_smoothness() {
        let bars = create_test_bars();
        let mut alma = ALMA::new(9, 0.85, 6.0);
        alma.calculate(&bars);

        // ALMA should track price but be smoothed
        if let Some(IndicatorValue::Single(val)) = alma.values().last() {
            let last_close = bars.last().unwrap().close;
            assert!(
                (*val - last_close).abs() < 10.0,
                "ALMA should be close to price"
            );
        }
    }

    #[test]
    fn test_alma_is_overlay() {
        let alma = ALMA::new(9, 0.85, 6.0);
        assert!(alma.is_overlay());
    }

    #[test]
    fn test_alma_empty_data() {
        let mut alma = ALMA::new(9, 0.85, 6.0);
        alma.calculate(&[]);
        assert!(alma.values().is_empty());
    }
}
