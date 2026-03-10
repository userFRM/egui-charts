use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Correlation of Log Returns with time index over a rolling window (Pearson).
#[derive(Clone)]
pub struct CorrelationLogReturns {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl CorrelationLogReturns {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.deep_purple],
            visible: true,
        }
    }
}

impl Default for CorrelationLogReturns {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for CorrelationLogReturns {
    fn name(&self) -> &str {
        "CorrLogRet"
    }

    fn desc(&self) -> &str {
        "Correlation Log Returns - Pearson correlation of ln(close/prev) vs time index"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // First bar has no log return
        self.values.push(IndicatorValue::None);

        // Compute log returns
        let mut log_returns = Vec::with_capacity(data.len());
        log_returns.push(0.0); // placeholder for index 0
        for i in 1..data.len() {
            if data[i - 1].close > 0.0 && data[i].close > 0.0 {
                log_returns.push((data[i].close / data[i - 1].close).ln());
            } else {
                log_returns.push(0.0);
            }
        }

        for i in 1..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let n = self.period as f64;

            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut sum_xy = 0.0;
            let mut sum_x2 = 0.0;
            let mut sum_y2 = 0.0;

            for (j, idx) in (start..=i).enumerate() {
                let x = j as f64;
                let y = log_returns[idx];
                sum_x += x;
                sum_y += y;
                sum_xy += x * y;
                sum_x2 += x * x;
                sum_y2 += y * y;
            }

            let denom_x = n * sum_x2 - sum_x * sum_x;
            let denom_y = n * sum_y2 - sum_y * sum_y;
            let denom = (denom_x * denom_y).sqrt();

            if denom.abs() < 1e-15 {
                self.values.push(IndicatorValue::Single(0.0));
            } else {
                let corr = (n * sum_xy - sum_x * sum_y) / denom;
                self.values
                    .push(IndicatorValue::Single(corr.clamp(-1.0, 1.0)));
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
        vec![format!("CorrLogRet({})", self.period)]
    }
}
