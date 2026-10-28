//! Reversal.
//!
//! Marks a bar that makes a new extreme of the last `lookback` bars and closes back beyond the
//! close of the bar before it, a key reversal.
//!
//! # Rule
//!
//! With the previous `lookback` bars, the bar before the current one among them:
//!
//! ```text
//! Upward:   low[t]  < lowest low of the previous bars   and  close[t] > close[t - 1]
//! Downward: high[t] > highest high of the previous bars  and  close[t] < close[t - 1]
//! ```
//!
//! The upward output holds the bar's low and the downward output its high, so the shapes sit
//! beneath and above the bar. A signal needs `lookback` earlier bars, is known when its bar closes
//! and never changes with later bars. Bars with a non-finite high, low or close are skipped. The
//! default lookback is 20 bars.

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Reversal: bars that make a new extreme and close beyond the previous close.
#[derive(Clone)]
pub struct Reversal {
    lookback: usize,
    values: IndicatorValues,
    stepper: Stepper<VecDeque<[f64; 3]>>,
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

impl Reversal {
    /// Reversals against the previous `lookback` bars, raised to at least 1.
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback: lookback.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            up_color: palette::GREEN,
            down_color: palette::RED,
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for Reversal {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for Reversal {
    crate::studies::inputs::study_inputs!(self;
        lookback: length("Lookback", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Reversal"
    }

    fn desc(&self) -> &str {
        "Reversal - New extreme of the lookback that closes beyond the previous close"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let lookback = self.lookback;
        self.stepper
            .run(data, from, &mut self.values, |window, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                    return IndicatorValue::None;
                }
                let mut row = [f64::NAN; 2];
                if let (Some(&[_, _, close]), true) = (window.back(), window.len() == lookback) {
                    // shortcut: scans the window on every bar, upgrade to running extremes if a
                    // lookback of thousands of bars is common
                    let lowest = window.iter().map(|b| b[1]).fold(f64::INFINITY, f64::min);
                    let highest = window
                        .iter()
                        .map(|b| b[0])
                        .fold(f64::NEG_INFINITY, f64::max);
                    if bar.low < lowest && bar.close > close {
                        row[0] = bar.low;
                    }
                    if bar.high > highest && bar.close < close {
                        row[1] = bar.high;
                    }
                }
                window.push_back([bar.high, bar.low, bar.close]);
                if window.len() > lookback {
                    window.pop_front();
                }
                IndicatorValue::Multiple(row.to_vec())
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: if line == 0 {
                self.up_color
            } else {
                self.down_color
            },
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 0 {
                crate::studies::PlotType::TriangleUp
            } else {
                crate::studies::PlotType::TriangleDown
            },
            visible: true,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.up_color, self.down_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.up_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.down_color = color;
        }
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
            "Upward Reversal".to_string(),
            "Downward Reversal".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lookback 3, labelled by hand from (high, low, close). Bar 3 makes a new low but closes at 7
    /// under the previous close 7.5; bar 4 makes the low 6 under the earlier 6.5 and closes at 8.2
    /// over the previous 7; bar 6 breaks the high but closes over the previous close; bar 7 breaks
    /// the high 11 with 12 and closes at 7.5 under the previous 10.5.
    #[test]
    fn key_reversals_need_a_new_extreme_and_a_close_beyond_the_previous_close() {
        let bars = super::super::hlc_bars(&[
            (10.0, 8.0, 9.0),
            (9.5, 7.5, 8.0),
            (8.5, 7.0, 7.5),
            (8.0, 6.5, 7.0),
            (8.5, 6.0, 8.2),
            (9.0, 7.8, 8.8),
            (11.0, 8.5, 10.5),
            (12.0, 8.0, 7.5),
        ]);
        let mut study = Reversal::new(3);
        super::super::assert_replay_matches_batch(&study, &bars);
        study.calculate(&bars);
        let line = |line: usize| -> Vec<Option<f64>> {
            (0..bars.len())
                .map(|at| study.values().value(line, at))
                .collect()
        };
        let none = None;
        assert_eq!(
            line(0),
            [none, none, none, none, Some(6.0), none, none, none]
        );
        assert_eq!(
            line(1),
            [none, none, none, none, none, none, none, Some(12.0)]
        );
    }
}
