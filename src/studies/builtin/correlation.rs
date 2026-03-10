use crate::model::Bar;
/// Correlation Coefficient
/// Measures correlation between price and time (trend strength)
/// Can also be used for price correlation between assets
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct CorrelationCoefficient {
    /// Lookback period (typically 20)
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl CorrelationCoefficient {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple, // Deep Purple
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for CorrelationCoefficient {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for CorrelationCoefficient {
    fn name(&self) -> &str {
        "Correlation"
    }

    fn desc(&self) -> &str {
        "Correlation Coefficient - Measures trend strength"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate correlation between price and time index
        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // X = time index (0, 1, 2, ..., period-1)
                // Y = close prices
                let n = self.period as f64;
                let mut sum_x = 0.0;
                let mut sum_y = 0.0;
                let mut sum_xy = 0.0;
                let mut sum_x2 = 0.0;
                let mut sum_y2 = 0.0;

                for (j, bar) in window.iter().enumerate() {
                    let x = j as f64;
                    let y = bar.close;
                    sum_x += x;
                    sum_y += y;
                    sum_xy += x * y;
                    sum_x2 += x * x;
                    sum_y2 += y * y;
                }

                // Pearson correlation coefficient
                let numerator = n * sum_xy - sum_x * sum_y;
                let denominator =
                    ((n * sum_x2 - sum_x * sum_x) * (n * sum_y2 - sum_y * sum_y)).sqrt();

                if denominator == 0.0 {
                    self.values.push(IndicatorValue::Single(0.0));
                } else {
                    let r = numerator / denominator;
                    self.values.push(IndicatorValue::Single(r));
                }
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
        vec![format!("Corr({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_perfect_uptrend() {
        let mut corr = CorrelationCoefficient::new(5);

        // Perfect linear uptrend
        let data: Vec<Bar> = (0..10).map(|i| make_bar(100.0 + i as f64)).collect();

        corr.calculate(&data);

        // Correlation should be close to 1.0 in perfect uptrend
        if let IndicatorValue::Single(r) = corr.values.last().unwrap() {
            assert!(*r > 0.99, "Correlation should be ~1.0, got {}", r);
        }
    }

    #[test]
    fn test_perfect_downtrend() {
        let mut corr = CorrelationCoefficient::new(5);

        // Perfect linear downtrend
        let data: Vec<Bar> = (0..10).map(|i| make_bar(100.0 - i as f64)).collect();

        corr.calculate(&data);

        // Correlation should be close to -1.0 in perfect downtrend
        if let IndicatorValue::Single(r) = corr.values.last().unwrap() {
            assert!(*r < -0.99, "Correlation should be ~-1.0, got {}", r);
        }
    }
}
