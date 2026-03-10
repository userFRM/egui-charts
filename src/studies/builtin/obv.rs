//! On Balance Volume (OBV) Indicator
//!
//! OBV is a cumulative volume-based indicator that relates volume to price change.
//! It adds volume on up days and subtracts volume on down days.
//!
//! # Interpretation
//! - Rising OBV: Accumulation (buying pressure)
//! - Falling OBV: Distribution (selling pressure)
//! - OBV divergence from price can signal trend changes
//!
//! # Calculation
//! If close > prev_close: OBV = prev_OBV + volume
//! If close < prev_close: OBV = prev_OBV - volume
//! If close == prev_close: OBV = prev_OBV
//!
//! # Example
//! ```ignore
//! use egui_charts::OnBalanceVolume;
//!
//! let mut obv = OnBalanceVolume::new();
//! obv.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// On Balance Volume indicator
#[derive(Clone)]
pub struct OnBalanceVolume {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl OnBalanceVolume {
    /// Create a new OBV indicator
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple, // Deep Purple
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for OnBalanceVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for OnBalanceVolume {
    fn name(&self) -> &str {
        "OBV"
    }

    fn desc(&self) -> &str {
        "On Balance Volume - Cumulative volume indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let mut obv = 0.0;

        // First bar - use volume as initial OBV
        obv += data[0].volume;
        self.values.push(IndicatorValue::Single(obv));

        // Subsequent bars
        for i in 1..data.len() {
            if data[i].close > data[i - 1].close {
                obv += data[i].volume;
            } else if data[i].close < data[i - 1].close {
                obv -= data[i].volume;
            }
            // If close == prev_close, OBV stays the same
            self.values.push(IndicatorValue::Single(obv));
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
        false // OBV is drawn in separate pane
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
        vec!["OBV".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_obv_up_day() {
        let start = Utc::now();
        let bars = vec![
            Bar {
                time: start,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(5),
                open: 100.0,
                high: 102.0,
                low: 100.0,
                close: 101.0,
                volume: 1500.0,
            },
        ];

        let mut obv = OnBalanceVolume::new();
        obv.calculate(&bars);

        // OBV should increase on up day: 1000 + 1500 = 2500
        if let IndicatorValue::Single(v) = obv.values()[1] {
            assert!((v - 2500.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_obv_down_day() {
        let start = Utc::now();
        let bars = vec![
            Bar {
                time: start,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(5),
                open: 100.0,
                high: 100.0,
                low: 98.0,
                close: 99.0,
                volume: 1500.0,
            },
        ];

        let mut obv = OnBalanceVolume::new();
        obv.calculate(&bars);

        // OBV should decrease on down day: 1000 - 1500 = -500
        if let IndicatorValue::Single(v) = obv.values()[1] {
            assert!((v - (-500.0)).abs() < 0.001);
        }
    }

    #[test]
    fn test_obv_flat_day() {
        let start = Utc::now();
        let bars = vec![
            Bar {
                time: start,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(5),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1500.0,
            },
        ];

        let mut obv = OnBalanceVolume::new();
        obv.calculate(&bars);

        // OBV should stay same on flat day: 1000
        if let IndicatorValue::Single(v) = obv.values()[1] {
            assert!((v - 1000.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_obv_empty_data() {
        let mut obv = OnBalanceVolume::new();
        obv.calculate(&[]);
        assert!(obv.values().is_empty());
    }
}
