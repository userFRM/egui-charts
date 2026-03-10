use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Standard Error Bands - upper/middle (linear regression)/lower bands
/// where upper = LinReg + multiplier * StdError, lower = LinReg - multiplier * StdError.
#[derive(Clone)]
pub struct StandardErrorBands {
    period: usize,
    multiplier: f64,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl StandardErrorBands {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.max(2),
            multiplier,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.bb_upper,
                DESIGN_TOKENS.semantic.indicators.bb_middle,
                DESIGN_TOKENS.semantic.indicators.bb_lower,
            ],
            visible: true,
        }
    }

    /// Compute linear regression slope, intercept, and standard error.
    fn linreg_with_se(prices: &[f64]) -> (f64, f64, f64) {
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
            let mean = sum_y / n;
            return (0.0, mean, 0.0);
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / n;

        let sum_sq_err: f64 = prices
            .iter()
            .enumerate()
            .map(|(j, &y)| {
                let predicted = intercept + slope * j as f64;
                (y - predicted).powi(2)
            })
            .sum();

        let se = if n > 2.0 {
            (sum_sq_err / (n - 2.0)).sqrt()
        } else {
            0.0
        };

        (slope, intercept, se)
    }
}

impl Default for StandardErrorBands {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for StandardErrorBands {
    fn name(&self) -> &str {
        "StdErrBands"
    }

    fn desc(&self) -> &str {
        "Standard Error Bands - Linear regression with standard error envelope"
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
            let (slope, intercept, se) = Self::linreg_with_se(&prices);

            // Middle = linear regression value at end of window
            let middle = intercept + slope * (self.period - 1) as f64;
            let upper = middle + self.multiplier * se;
            let lower = middle - self.multiplier * se;

            self.values
                .push(IndicatorValue::Multiple(vec![upper, middle, lower]));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            self.colors = vec![colors[0], colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        3
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
        vec![
            format!("SE Upper({}, {})", self.period, self.multiplier),
            format!("SE Middle({})", self.period),
            format!("SE Lower({}, {})", self.period, self.multiplier),
        ]
    }
}
