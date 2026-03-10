use crate::model::Bar;
/// Hamming Weighted Moving Average
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct HammingMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl HammingMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.indicators.ma,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Compute Hamming window weights for the given period
    fn hamming_weights(period: usize) -> Vec<f64> {
        let n = period as f64;
        (0..period)
            .map(|i| 0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1.0)).cos())
            .collect()
    }
}

impl Default for HammingMA {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for HammingMA {
    fn name(&self) -> &str {
        "Hamming MA"
    }

    fn desc(&self) -> &str {
        "Hamming Weighted Moving Average - Hamming window function weighting"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period || self.period < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        let weights = Self::hamming_weights(self.period);
        let weight_sum: f64 = weights.iter().sum();

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;
                let weighted_sum: f64 = data[start..=i]
                    .iter()
                    .zip(weights.iter())
                    .map(|(bar, w)| bar.close * w)
                    .sum();
                let hma = weighted_sum / weight_sum;
                self.values.push(IndicatorValue::Single(hma));
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
        vec![format!("Hamming({})", self.period)]
    }
}
