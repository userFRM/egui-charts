//! Linear Regression Indicators
//! Includes Linear Regression Line, Slope, and R-Squared
//! Popular for trend analysis

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Linear regression fitted to closing prices over a rolling window.
#[derive(Clone)]
pub struct LinearRegression {
    /// Lookback period (typically 14)
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl LinearRegression {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// Set the plot color.
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

impl LinearRegression {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
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
                values.push(IndicatorValue::Single(value));
            }
        }
        values
    }
}

impl Indicator for LinearRegression {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "LinReg"
    }

    fn desc(&self) -> &str {
        "Linear Regression Line - Best fit line value"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            |_data, from| from.saturating_sub(self.period - 1),
            |data| self.compute(data),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
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
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl LinearRegressionSlope {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::ORANGE,
            visible: true,
        }
    }

    /// Set the plot color.
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

impl LinearRegressionSlope {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
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
                values.push(IndicatorValue::Single(slope));
            }
        }
        values
    }
}

impl Indicator for LinearRegressionSlope {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "LinReg Slope"
    }

    fn desc(&self) -> &str {
        "Linear Regression Slope - Rate of change"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            |_data, from| from.saturating_sub(self.period - 1),
            |data| self.compute(data),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
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
