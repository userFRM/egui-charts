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
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// On Balance Volume indicator
#[derive(Clone)]
pub struct OnBalanceVolume {
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<f64>,
    total: f64,
}

impl OnBalanceVolume {
    /// Create a new OBV indicator
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() || !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                if let Some(previous) = state.previous {
                    if bar.close > previous {
                        state.total += bar.volume;
                    } else if bar.close < previous {
                        state.total -= bar.volume;
                    }
                }
                state.previous = Some(bar.close);
                IndicatorValue::Single(state.total)
            });
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
        type Case = (&'static [f64], &'static [f64], &'static [Option<f64>]);
        let cases: &[Case] = &[
            // Up, down, unchanged, a zero-volume up bar, then down.
            (
                &[10.0, 11.0, 10.5, 10.5, 12.0, 11.0],
                &[1000.0, 1500.0, 700.0, 900.0, 0.0, 400.0],
                &[
                    Some(0.0),
                    Some(1500.0),
                    Some(800.0),
                    Some(800.0),
                    Some(800.0),
                    Some(400.0),
                ],
            ),
            // Flat series.
            (
                &[10.0, 10.0, 10.0],
                &[1000.0, 2000.0, 3000.0],
                &[Some(0.0), Some(0.0), Some(0.0)],
            ),
            // Single bar.
            (&[10.0], &[5000.0], &[Some(0.0)]),
            // A NaN volume has no value; the next bar compares with the close before it.
            (
                &[10.0, 11.0, 12.0, 13.0, 14.0],
                &[100.0, 200.0, f64::NAN, 400.0, 500.0],
                &[Some(0.0), Some(200.0), None, Some(600.0), Some(1100.0)],
            ),
        ];
        for (closes, volumes, expected) in cases {
            let mut obv = OnBalanceVolume::new();
            obv.calculate(&bars(closes, volumes));
            let got: Vec<Option<f64>> = obv
                .values()
                .to_rows()
                .iter()
                .map(|v| match v {
                    IndicatorValue::Single(x) => Some(*x),
                    IndicatorValue::None => None,
                    other => panic!("unexpected value {other:?}"),
                })
                .collect();
            assert_eq!(got, *expected, "closes {closes:?}");
        }
    }
}
