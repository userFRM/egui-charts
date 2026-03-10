//! Symmetrically Weighted Moving Average (SWMA) indicator.
//!
//! SWMA is a fixed 4-bar moving average using symmetric weights
//! `[1/6, 2/6, 2/6, 1/6]`. The middle bars receive twice the weight
//! of the outer bars, producing a smooth curve with less lag than SMA
//! and less overshoot than EMA.
//!
//! # Formula
//!
//! ```text
//! SWMA = (1*P[t-3] + 2*P[t-2] + 2*P[t-1] + 1*P[t]) / 6
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{SWMA, Indicator};
//!
//! let mut swma = SWMA::new();
//! swma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Symmetrically Weighted Moving Average indicator.
///
/// A fixed 4-bar weighted average with weights `[1, 2, 2, 1] / 6`.
/// Overlay indicator.
#[derive(Clone)]
pub struct SWMA {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl Default for SWMA {
    fn default() -> Self {
        Self::new()
    }
}

impl SWMA {
    /// Create a new SWMA indicator (fixed 4-bar window, no parameters).
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            visible: true,
        }
    }

    /// Set a custom line colour (builder pattern).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Compute the SWMA value for four consecutive closing prices.
    ///
    /// `SWMA = (1*P1 + 2*P2 + 2*P3 + 1*P4) / 6`
    fn swma_val(p1: f64, p2: f64, p3: f64, p4: f64) -> f64 {
        (p1 + 2.0 * p2 + 2.0 * p3 + p4) / 6.0
    }
}

impl Indicator for SWMA {
    fn name(&self) -> &str {
        "SWMA"
    }

    fn desc(&self) -> &str {
        "Symmetrically Weighted Moving Avg - 4-bar symmetric weighted MA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        // SWMA requires 4 bars min
        if data.len() < 4 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // First 3 bars have no value
        for _ in 0..3 {
            self.values.push(IndicatorValue::None);
        }

        // Calculate SWMA for remaining bars
        for i in 3..data.len() {
            let p1 = data[i - 3].close;
            let p2 = data[i - 2].close;
            let p3 = data[i - 1].close;
            let p4 = data[i].close;

            let swma = Self::swma_val(p1, p2, p3, p4);
            self.values.push(IndicatorValue::Single(swma));
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
        vec!["SWMA".to_string()]
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
    fn test_swma_val() {
        // With equal prices, SWMA should equal that price
        let swma = SWMA::swma_val(100.0, 100.0, 100.0, 100.0);
        assert!((swma - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_swma_weighted() {
        // Middle values should have more weight
        // (1*100 + 2*120 + 2*120 + 1*100) / 6 = (100 + 240 + 240 + 100) / 6 = 680 / 6 = 113.33
        let swma = SWMA::swma_val(100.0, 120.0, 120.0, 100.0);
        assert!((swma - 113.33).abs() < 0.01);
    }

    #[test]
    fn test_swma_calculation() {
        let mut swma = SWMA::new();

        let data = vec![
            make_bar(100.0),
            make_bar(102.0),
            make_bar(101.0),
            make_bar(103.0),
            make_bar(104.0),
        ];

        swma.calculate(&data);

        assert_eq!(swma.values.len(), 5);
        assert!(matches!(swma.values[0], IndicatorValue::None));
        assert!(matches!(swma.values[1], IndicatorValue::None));
        assert!(matches!(swma.values[2], IndicatorValue::None));

        // Fourth bar: (100 + 2*102 + 2*101 + 103) / 6 = 609 / 6 = 101.5
        if let IndicatorValue::Single(v) = swma.values[3] {
            assert!((v - 101.5).abs() < 0.01);
        }
    }
}
