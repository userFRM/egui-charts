use crate::model::Bar;
/// Relative Vigor Index (RVI)
/// Measures conviction of price movement
/// Uses the relationship between close-open vs high-low
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct RelativeVigorIndex {
    /// Period for smoothing (typically 10)
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [RVI, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl RelativeVigorIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - RVI
                DESIGN_TOKENS.semantic.extended.error,   // Red - Signal
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    fn swma(data: &[f64]) -> Vec<f64> {
        // Symmetrically weighted moving avg (weights: 1, 2, 2, 1)
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < 3 {
                result.push(f64::NAN);
            } else {
                let swma = (data[i - 3] + 2.0 * data[i - 2] + 2.0 * data[i - 1] + data[i]) / 6.0;
                result.push(swma);
            }
        }

        result
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let valid: Vec<f64> = data[i + 1 - period..=i]
                    .iter()
                    .filter(|&&x| !x.is_nan())
                    .copied()
                    .collect();

                if valid.is_empty() {
                    result.push(f64::NAN);
                } else {
                    result.push(valid.iter().sum::<f64>() / valid.len() as f64);
                }
            }
        }

        result
    }
}

impl Default for RelativeVigorIndex {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for RelativeVigorIndex {
    fn name(&self) -> &str {
        "RVI"
    }

    fn desc(&self) -> &str {
        "Relative Vigor Index - Price movement conviction"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 4 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate (Close - Open) for numerator
        let close_open: Vec<f64> = data.iter().map(|b| b.close - b.open).collect();

        // Calculate (High - Low) for denominator
        let high_low: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();

        // Apply SWMA smoothing
        let smoothed_num = Self::swma(&close_open);
        let smoothed_den = Self::swma(&high_low);

        // Calculate running sums over period
        let num_sum = Self::calculate_sma(&smoothed_num, self.period);
        let den_sum = Self::calculate_sma(&smoothed_den, self.period);

        // Calculate RVI
        let mut rvi = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if num_sum[i].is_nan() || den_sum[i].is_nan() || den_sum[i] == 0.0 {
                rvi.push(f64::NAN);
            } else {
                rvi.push(num_sum[i] / den_sum[i]);
            }
        }

        // Calculate signal line (SWMA of RVI)
        let signal = Self::swma(&rvi);

        // Store results
        for i in 0..data.len() {
            if rvi[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else if signal[i].is_nan() {
                self.values.push(IndicatorValue::Single(rvi[i]));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![rvi[i], signal[i]]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("RVI({})", self.period), "Signal".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_rvi_calculation() {
        let mut rvi = RelativeVigorIndex::new(5);

        let data: Vec<Bar> = (0..20)
            .map(|i| {
                make_bar(
                    100.0 + i as f64,
                    102.0 + i as f64,
                    98.0 + i as f64,
                    101.0 + i as f64,
                )
            })
            .collect();

        rvi.calculate(&data);

        assert_eq!(rvi.values.len(), 20);

        // Should have valid values
        let valid_cnt = rvi
            .values
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
