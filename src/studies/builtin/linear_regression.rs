use crate::model::Bar;
/// Linear Regression Indicators
/// Includes Linear Regression Line, Slope, and R-Squared
/// Popular for trend analysis
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct LinearRegression {
    /// Lookback period (typically 14)
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl LinearRegression {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for LinearRegression {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for LinearRegression {
    fn name(&self) -> &str {
        "LinReg"
    }

    fn desc(&self) -> &str {
        "Linear Regression Line - Best fit line value"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let n = self.period as f64;

                let mut sum_x = 0.0;
                let mut sum_y = 0.0;
                let mut sum_xy = 0.0;
                let mut sum_x2 = 0.0;

                for (j, bar) in window.iter().enumerate() {
                    let x = j as f64;
                    let y = bar.close;
                    sum_x += x;
                    sum_y += y;
                    sum_xy += x * y;
                    sum_x2 += x * x;
                }

                let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
                let intercept = (sum_y - slope * sum_x) / n;

                // Value at the end of the regression line (current bar)
                let value = intercept + slope * (self.period - 1) as f64;
                self.values.push(IndicatorValue::Single(value));
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
        true // Overlay on price chart
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
        vec![format!("LinReg({})", self.period)]
    }
}

/// Linear Regression Slope
#[derive(Clone)]
pub struct LinearRegressionSlope {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl LinearRegressionSlope {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.warning,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for LinearRegressionSlope {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for LinearRegressionSlope {
    fn name(&self) -> &str {
        "LinReg Slope"
    }

    fn desc(&self) -> &str {
        "Linear Regression Slope - Rate of change"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let n = self.period as f64;

                let mut sum_x = 0.0;
                let mut sum_y = 0.0;
                let mut sum_xy = 0.0;
                let mut sum_x2 = 0.0;

                for (j, bar) in window.iter().enumerate() {
                    let x = j as f64;
                    let y = bar.close;
                    sum_x += x;
                    sum_y += y;
                    sum_xy += x * y;
                    sum_x2 += x * x;
                }

                let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
                self.values.push(IndicatorValue::Single(slope));
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
        vec![format!("Slope({})", self.period)]
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
    fn test_linear_regression() {
        let mut lr = LinearRegression::new(5);

        // Simple uptrend
        let data: Vec<Bar> = (0..10).map(|i| make_bar(100.0 + i as f64 * 2.0)).collect();

        lr.calculate(&data);

        assert_eq!(lr.values.len(), 10);

        // LinReg should be close to the actual price in smooth trend
        if let IndicatorValue::Single(v) = lr.values.last().unwrap() {
            let last_price = data.last().unwrap().close;
            assert!(
                (v - last_price).abs() < 2.0,
                "LinReg {} should be close to price {}",
                v,
                last_price
            );
        }
    }

    #[test]
    fn test_slope_uptrend() {
        let mut slope = LinearRegressionSlope::new(5);

        let data: Vec<Bar> = (0..10).map(|i| make_bar(100.0 + i as f64 * 2.0)).collect();

        slope.calculate(&data);

        // Slope should be positive in uptrend
        if let IndicatorValue::Single(s) = slope.values.last().unwrap() {
            assert!(*s > 0.0, "Slope should be positive in uptrend");
        }
    }
}
