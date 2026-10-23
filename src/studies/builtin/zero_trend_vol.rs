use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Zero-mean close-to-close volatility, without subtracting the mean log return.
///
/// `sqrt(mean(ln(close / previous_close)^2)) * sqrt(252) * 100`.
/// Annualisation assumes daily bars and 252 trading days per year (see #672).
/// A full `period` of returns from positive closes is required before a value exists.
#[derive(Clone)]
pub struct ZeroTrendVol {
    period: usize,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl ZeroTrendVol {
    /// Create a zero-trend study over `period` log returns (at least 1).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(103, 58, 183)], // Deep purple
            visible: true,
        }
    }
}

impl Default for ZeroTrendVol {
    fn default() -> Self {
        Self::new(20)
    }
}

impl ZeroTrendVol {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let returns: Vec<_> = data
            .windows(2)
            .map(|bars| {
                if bars[0].close > 0.0 && bars[1].close > 0.0 {
                    (bars[1].close / bars[0].close).ln()
                } else {
                    f64::NAN
                }
            })
            .collect();
        (0..data.len())
            .map(|i| {
                if i < self.period {
                    return IndicatorValue::None;
                }
                let window = &returns[i - self.period..i];
                if window.iter().any(|r| !r.is_finite()) {
                    return IndicatorValue::None;
                }
                let mean_squared = window.iter().map(|r| r * r).sum::<f64>() / self.period as f64;
                IndicatorValue::Single(mean_squared.sqrt() * 252.0_f64.sqrt() * 100.0)
            })
            .collect()
    }
}

impl Indicator for ZeroTrendVol {
    fn name(&self) -> &str {
        "Zero-Trend Vol"
    }

    fn desc(&self) -> &str {
        "Zero-Trend Close-to-Close Volatility - Annualised percentage assuming daily bars"
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
        if !colors.is_empty() {
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("Zero-Trend Vol({})", self.period)]
    }
}
