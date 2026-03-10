//! Least Squares Moving Average (LSMA) indicator.
//!
//! Also known as the *Linear Regression Line* or *Moving Linear Regression*,
//! LSMA fits an ordinary least-squares line through the last `N` closing
//! prices and reports the endpoint value. An optional `offset` parameter
//! projects the line forward (or backward) by a specified number of bars.
//!
//! # Formula
//!
//! For each window of `period` prices, compute the best-fit line
//! `y = m*x + b` and evaluate it at the last position (+offset).
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{LSMA, Indicator};
//!
//! let mut lsma = LSMA::new(25);
//! lsma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Least Squares Moving Average (Linear Regression Line) indicator.
///
/// Fits a least-squares regression line to the most recent `period` prices
/// and reports the end-point value. Overlay indicator.
#[derive(Clone)]
pub struct LSMA {
    period: usize,
    offset: i32, // Future projection offset (can be negative for past)
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl LSMA {
    /// Create a new LSMA indicator.
    ///
    /// # Arguments
    /// * `period` -- Number of bars in the regression window (e.g. 25).
    pub fn new(period: usize) -> Self {
        Self {
            period,
            offset: 0,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.favorite_gold, // Amber
            visible: true,
        }
    }

    /// Set the forward (positive) or backward (negative) projection offset in bars.
    pub fn with_offset(mut self, offset: i32) -> Self {
        self.offset = offset;
        self
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Compute the least-squares regression value for a price window,
    /// evaluated at the last index plus `offset`.
    fn linear_regression(prices: &[f64], offset: i32) -> f64 {
        let n = prices.len() as f64;
        if n == 0.0 {
            return 0.0;
        }

        // Calculate sums for linear regression
        // y = mx + b where x is the bar index
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for (i, price) in prices.iter().enumerate() {
            let x = i as f64;
            sum_x += x;
            sum_y += price;
            sum_xy += x * price;
            sum_xx += x * x;
        }

        // Calculate slope (m) and intercept (b)
        let denominator = n * sum_xx - sum_x * sum_x;
        if denominator.abs() < 1e-10 {
            return sum_y / n; // Return avg if no slope
        }

        let m = (n * sum_xy - sum_x * sum_y) / denominator;
        let b = (sum_y - m * sum_x) / n;

        // Calculate value at the last point + offset
        let x_target = (prices.len() - 1) as f64 + offset as f64;
        m * x_target + b
    }
}

impl Indicator for LSMA {
    fn name(&self) -> &str {
        "LSMA"
    }

    fn desc(&self) -> &str {
        "Least Squares Moving Avg - Linear regression line"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let prices: Vec<f64> = data[i + 1 - self.period..=i]
                    .iter()
                    .map(|bar| bar.close)
                    .collect();

                let lsma = Self::linear_regression(&prices, self.offset);
                self.values.push(IndicatorValue::Single(lsma));
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
        if self.offset != 0 {
            vec![format!("LSMA({}, {})", self.period, self.offset)]
        } else {
            vec![format!("LSMA({})", self.period)]
        }
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
    fn test_linear_regression_flat() {
        // Flat prices should return the same value
        let prices = vec![100.0, 100.0, 100.0, 100.0, 100.0];
        let result = LSMA::linear_regression(&prices, 0);
        assert!((result - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_linear_regression_uptrend() {
        // Perfect uptrend: each bar +1
        let prices = vec![100.0, 101.0, 102.0, 103.0, 104.0];
        let result = LSMA::linear_regression(&prices, 0);
        // Should be close to 104.0 (last point on the line)
        assert!((result - 104.0).abs() < 0.001);
    }

    #[test]
    fn test_linear_regression_with_offset() {
        // Perfect uptrend, project 1 bar forward
        let prices = vec![100.0, 101.0, 102.0, 103.0, 104.0];
        let result = LSMA::linear_regression(&prices, 1);
        // Should be close to 105.0 (next point on the line)
        assert!((result - 105.0).abs() < 0.001);
    }

    #[test]
    fn test_lsma_calculation() {
        let mut lsma = LSMA::new(3);

        let data = vec![
            make_bar(100.0),
            make_bar(101.0),
            make_bar(102.0),
            make_bar(103.0),
        ];

        lsma.calculate(&data);

        assert_eq!(lsma.values.len(), 4);
        assert!(matches!(lsma.values[0], IndicatorValue::None));
        assert!(matches!(lsma.values[1], IndicatorValue::None));

        // Third value should be close to 102.0 (uptrend)
        if let IndicatorValue::Single(v) = lsma.values[2] {
            assert!((v - 102.0).abs() < 0.01);
        }
    }
}
