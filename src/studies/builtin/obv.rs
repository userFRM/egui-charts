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
//! OBV is 0 on the first bar. On each later bar:
//! If close > prev_close: OBV = prev_OBV + volume
//! If close < prev_close: OBV = prev_OBV - volume
//! If close == prev_close: OBV = prev_OBV
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{OnBalanceVolume, Indicator};
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

        // The first bar has no previous close, so the running total starts at 0.
        let mut obv = 0.0;
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
    use chrono::Utc;

    fn bars(closes: &[f64], volumes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .zip(volumes)
            .map(|(&c, &v)| Bar::new(Utc::now(), c, c + 1.0, c - 1.0, c, v))
            .collect()
    }

    /// Expected values are the running total worked by hand: the first bar has
    /// no previous close, so it adds nothing and OBV is 0 there.
    #[test]
    fn test_obv_matches_reference() {
        // (closes, volumes, expected OBV)
        let cases: &[(&[f64], &[f64], &[f64])] = &[
            // Up, down, unchanged, a zero-volume up bar, then down.
            (
                &[10.0, 11.0, 10.5, 10.5, 12.0, 11.0],
                &[1000.0, 1500.0, 700.0, 900.0, 0.0, 400.0],
                &[0.0, 1500.0, 800.0, 800.0, 800.0, 400.0],
            ),
            // Flat series.
            (
                &[10.0, 10.0, 10.0],
                &[1000.0, 2000.0, 3000.0],
                &[0.0, 0.0, 0.0],
            ),
            // Single bar.
            (&[10.0], &[5000.0], &[0.0]),
        ];
        for (closes, volumes, expected) in cases {
            let mut obv = OnBalanceVolume::new();
            obv.calculate(&bars(closes, volumes));
            let got: Vec<f64> = obv
                .values()
                .iter()
                .map(|v| match v {
                    IndicatorValue::Single(x) => *x,
                    other => panic!("unexpected value {other:?}"),
                })
                .collect();
            assert_eq!(got, *expected, "closes {closes:?}");
        }
    }

    #[test]
    fn test_obv_empty_data() {
        let mut obv = OnBalanceVolume::new();
        obv.calculate(&[]);
        assert!(obv.values().is_empty());
    }
}
