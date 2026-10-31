//! Avg True Range (ATR) Indicator
//!
//! ATR measures market volatility by decomposing the entire range of an asset price.
//! It's commonly used for position sizing and stop-loss placement.
//!
//! # Calculation
//! True Range = max(high - low, |high - prev_close|, |low - prev_close|)
//! ATR = Wilder's average of True Range over N periods, or a simple, exponential or weighted one
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ATR, Indicator};
//!
//! let mut atr = ATR::new(14);
//! atr.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Average of the true range that [`ATR`] draws. Every smoothing has its first value on the
/// `period`-th true range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AtrSmoothing {
    /// Wilder's average, alpha 1/period, starting at the mean of the first `period` ranges.
    #[default]
    Wilder,
    /// Mean of the last `period` ranges.
    Simple,
    /// Alpha 2/(period+1), starting at the mean of the first `period` ranges.
    Exponential,
    /// Linear weights 1 to `period` over the last `period` ranges, newest heaviest.
    Weighted,
}

/// Avg True Range indicator
#[derive(Clone)]
pub struct ATR {
    period: usize,
    smoothing: AtrSmoothing,
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone)]
struct State {
    previous: Option<f64>,
    usable: usize,
    atr: f64,
    /// The last `period` true ranges of a simple or weighted average.
    window: VecDeque<f64>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            previous: None,
            usable: 0,
            atr: -0.0,
            window: VecDeque::new(),
        }
    }
}

impl ATR {
    /// Create a new ATR indicator with the specified period
    ///
    /// # Arguments
    /// * `period` - The lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            smoothing: AtrSmoothing::Wilder,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::DARK_RED,
            visible: true,
        }
    }

    /// Choose the average of the true range; Wilder's by default.
    pub fn with_smoothing(mut self, smoothing: AtrSmoothing) -> Self {
        self.smoothing = smoothing;
        self
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate True Range for a single bar
    #[inline]
    pub fn true_range(bar: &Bar, prev_close: f64) -> f64 {
        let hl = bar.high - bar.low;
        let hc = (bar.high - prev_close).abs();
        let lc = (bar.low - prev_close).abs();
        hl.max(hc).max(lc)
    }
}

/// Construct with the conventional default parameters.
impl Default for ATR {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ATR {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smoothing: choice("Smoothing", [AtrSmoothing::Wilder, AtrSmoothing::Simple, AtrSmoothing::Exponential, AtrSmoothing::Weighted])
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "ATR"
    }

    fn desc(&self) -> &str {
        "Avg True Range - Volatility indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let period = self.period;
        let smoothing = self.smoothing;
        let multiplier = if smoothing == AtrSmoothing::Exponential {
            2.0 / (period as f64 + 1.0)
        } else {
            1.0 / period as f64
        };
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|p| p.is_finite()) {
                    return IndicatorValue::None;
                }
                let tr = state
                    .previous
                    .map_or(bar.high - bar.low, |close| Self::true_range(bar, close));
                state.previous = Some(bar.close);
                if matches!(smoothing, AtrSmoothing::Simple | AtrSmoothing::Weighted) {
                    state.window.push_back(tr);
                    if state.window.len() > period {
                        state.window.pop_front();
                    }
                    if state.window.len() < period {
                        return IndicatorValue::None;
                    }
                    return IndicatorValue::Single(if smoothing == AtrSmoothing::Simple {
                        state.window.iter().sum::<f64>() / period as f64
                    } else {
                        let weighted: f64 =
                            (1..).zip(&state.window).map(|(w, tr)| w as f64 * tr).sum();
                        weighted / (period as f64 * (period as f64 + 1.0) / 2.0)
                    });
                }
                state.usable += 1;
                if state.usable <= period {
                    state.atr += tr;
                    if state.usable < period {
                        return IndicatorValue::None;
                    }
                    state.atr /= period as f64;
                } else {
                    state.atr += multiplier * (tr - state.atr);
                }
                IndicatorValue::Single(state.atr)
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
        false // ATR is drawn in separate pane
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
        vec![match self.smoothing {
            AtrSmoothing::Wilder => format!("ATR({})", self.period),
            AtrSmoothing::Simple => format!("ATR({}, SMA)", self.period),
            AtrSmoothing::Exponential => format!("ATR({}, EMA)", self.period),
            AtrSmoothing::Weighted => format!("ATR({}, WMA)", self.period),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_true_range_calculation() {
        let bar = Bar {
            time: Utc::now(),
            open: 100.0,
            high: 105.0,
            low: 98.0,
            close: 103.0,
            volume: 1000.0,
            colors: None,
            payload: None,
        };

        // Case 1: Normal range (high - low is largest)
        let tr = ATR::true_range(&bar, 100.0);
        assert!((tr - 7.0).abs() < 0.001); // high - low = 105 - 98 = 7

        // Case 2: Gap up (high - prev_close is largest)
        let tr = ATR::true_range(&bar, 95.0);
        assert!((tr - 10.0).abs() < 0.001); // high - prev = 105 - 95 = 10

        // Case 3: Gap down (|low - prev_close| is largest)
        let tr = ATR::true_range(&bar, 110.0);
        assert!((tr - 12.0).abs() < 0.001); // |low - prev| = |98 - 110| = 12
    }

    /// Wilder's ATR worked by hand. A bar's ATR does not depend on whether a later bar exists:
    /// with length 1 the first bar has its range. A bar with a non-finite price has no value,
    /// and the next true range uses the close before it.
    #[test]
    fn matches_wilder_by_hand() {
        let bar = |high, low, close| Bar {
            time: Utc::now(),
            open: close,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        };
        let first = bar(102.0, 98.0, 100.0);
        // Length 2 around a bad bar: ranges 4, max(2, 3, 1) = 3, then from the close of 102
        // max(4, 6, 2) = 6 and max(2, 1, 1) = 2; ATR (4 + 3) / 2 = 3.5, (3.5 + 6) / 2 = 4.75,
        // (4.75 + 2) / 2 = 3.375.
        let around = |bad: Bar| {
            vec![
                first.clone(),
                bar(103.0, 101.0, 102.0),
                bad,
                bar(108.0, 104.0, 106.0),
                bar(107.0, 105.0, 106.0),
            ]
        };
        let skipped = vec![None, Some(3.5), None, Some(4.75), Some(3.375)];
        // Ranges by hand: 102 - 98 = 4; for the second bar max(103 - 99, |103 - 100|, |99 - 100|) = 4.
        let cases: [(usize, Vec<Bar>, Vec<Option<f64>>); 4] = [
            (1, vec![first.clone()], vec![Some(4.0)]),
            (
                1,
                vec![first.clone(), bar(103.0, 99.0, 101.0)],
                vec![Some(4.0), Some(4.0)],
            ),
            (
                2,
                around(bar(f64::NAN, f64::NAN, f64::NAN)),
                skipped.clone(),
            ),
            (2, around(bar(f64::INFINITY, 103.0, 104.0)), skipped),
        ];
        for (period, data, expected) in cases {
            let mut atr = ATR::new(period);
            atr.calculate(&data);
            assert_eq!(atr.values().len(), expected.len());
            for (i, (got, want)) in atr.values().to_rows().iter().zip(expected).enumerate() {
                let ok = match (got, want) {
                    (IndicatorValue::None, None) => true,
                    (IndicatorValue::Single(v), Some(w)) => (v - w).abs() < 1e-12,
                    _ => false,
                };
                assert!(ok, "ATR({period}) bar {i}: {got:?} != {want:?}");
            }
        }
    }
}
