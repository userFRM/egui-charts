use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Standard Error of the linear regression of close prices over a rolling window.
#[derive(Clone)]
pub struct StandardError {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl StandardError {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.teal],
            visible: true,
        }
    }

    /// Compute linear regression slope and intercept for a window of close prices.
    fn linreg(prices: &[f64]) -> (f64, f64) {
        let n = prices.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2 = 0.0;

        for (j, &y) in prices.iter().enumerate() {
            let x = j as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
        }

        let denom = n * sum_x2 - sum_x * sum_x;
        if denom.abs() < 1e-15 {
            return (0.0, sum_y / n);
        }
        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / n;
        (slope, intercept)
    }
}

impl Default for StandardError {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for StandardError {
    fn name(&self) -> &str {
        "StdErr"
    }

    fn desc(&self) -> &str {
        "Standard Error - Standard error of linear regression of close prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let prices: Vec<f64> = data[start..=i].iter().map(|b| b.close).collect();
            let (slope, intercept) = Self::linreg(&prices);

            let n = self.period as f64;
            let sum_sq_err: f64 = prices
                .iter()
                .enumerate()
                .map(|(j, &y)| {
                    let predicted = intercept + slope * j as f64;
                    (y - predicted).powi(2)
                })
                .sum();

            // Standard error = sqrt(sum_sq_err / (n - 2))
            let se = if n > 2.0 {
                (sum_sq_err / (n - 2.0)).sqrt()
            } else {
                0.0
            };

            self.values.push(IndicatorValue::Single(se));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!("StdErr({})", self.period)]
    }
}
