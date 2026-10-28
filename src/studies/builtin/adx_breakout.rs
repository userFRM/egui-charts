//! ADX Breakout.
//!
//! Marks a close beyond the high or low of the last `range` bars when the trend was weak on the
//! bar before, a breakout from a quiet range.
//!
//! # Rule
//!
//! The ADX is Wilder's ([`ADX`]) of `period` bars. With the `range` bars before the current one:
//!
//! ```text
//! Upward:   ADX[t - 1] <= threshold  and  close[t] > highest high of the range
//! Downward: ADX[t - 1] <= threshold  and  close[t] < lowest low of the range
//! ```
//!
//! Both outputs hold the ADX of the breakout bar, so the shapes sit on the ADX line. A signal needs
//! `range` earlier bars and an ADX on the bar before it, is known when its bar closes and never
//! changes with later bars. Bars with a non-finite high, low or close are skipped. The defaults are
//! an ADX of 14 bars, a threshold of 20 and a range of 20 bars.

use super::ADX;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// ADX Breakout: closes beyond the range of the last bars after a weak trend.
#[derive(Clone)]
pub struct AdxBreakout {
    period: usize,
    threshold: usize,
    range: usize,
    adx: ADX,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: [Color32; 3],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    previous: Option<f64>,
    window: VecDeque<(f64, f64)>,
}

impl AdxBreakout {
    /// An ADX of `period` bars and a range of `range` bars, each raised to at least 1, and a
    /// threshold of at most 100.
    pub fn new(period: usize, threshold: usize, range: usize) -> Self {
        Self {
            period: period.max(1),
            threshold: threshold.min(100),
            range: range.max(1),
            adx: ADX::new(period),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [palette::GOLD, palette::GREEN, palette::RED],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for AdxBreakout {
    fn default() -> Self {
        Self::new(14, 20, 20)
    }
}

impl Indicator for AdxBreakout {
    crate::studies::inputs::study_inputs!(self;
        period: length("ADX period", 1, usize::MAX),
        threshold: length("ADX threshold", 0, 100),
        range: length("Range", 1, usize::MAX)
    ; {
        self.adx = ADX::new(self.period);
    });

    fn name(&self) -> &str {
        "ADX Breakout"
    }

    fn desc(&self) -> &str {
        "ADX Breakout - Closes beyond the range of the last bars after a weak trend"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.adx.update(data, from);
        let (adx, range, threshold) = (self.adx.values(), self.range, self.threshold as f64);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let source = state.source;
                state.source += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                    return IndicatorValue::None;
                }
                let current = adx.value(0, source);
                let mut row = [current.unwrap_or(f64::NAN), f64::NAN, f64::NAN];
                if state.window.len() == range && state.previous.is_some_and(|adx| adx <= threshold)
                {
                    // shortcut: scans the window on every bar, upgrade to running extremes if a
                    // range of thousands of bars is common
                    let highest = state
                        .window
                        .iter()
                        .map(|b| b.0)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let lowest = state
                        .window
                        .iter()
                        .map(|b| b.1)
                        .fold(f64::INFINITY, f64::min);
                    if bar.close > highest {
                        row[1] = row[0];
                    } else if bar.close < lowest {
                        row[2] = row[0];
                    }
                }
                state.previous = current;
                state.window.push_back((bar.high, bar.low));
                if state.window.len() > range {
                    state.window.pop_front();
                }
                IndicatorValue::Multiple(row.to_vec())
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 => crate::studies::PlotType::Line,
                1 => crate::studies::PlotType::TriangleUp,
                _ => crate::studies::PlotType::TriangleDown,
            },
            visible: true,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            range: Some((0.0, 100.0)),
            levels: vec![(self.threshold as f64, Color32::from_gray(128), "Threshold")],
            ..Default::default()
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
        ["ADX", "Upward Breakout", "Downward Breakout"]
            .map(String::from)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADX of 2 bars and a range of 3, by hand. Five identical bars give an ADX of 0 from the
    /// fourth. Bar 5 closes at 11 over the range high 10 after an ADX of 0 and lifts the ADX to
    /// (0 + 100) / 2 = 50, since only +DM moves. Bar 6 closes over the range high 11.5 but after an
    /// ADX of 50, so a threshold of 25 withholds it and 100 does not. Negating the prices gives the
    /// downward breakout.
    #[test]
    fn breakouts_need_a_weak_trend_before_a_close_beyond_the_range() {
        let up = [
            (10.0, 9.0, 9.5),
            (10.0, 9.0, 9.5),
            (10.0, 9.0, 9.5),
            (10.0, 9.0, 9.5),
            (10.0, 9.0, 9.5),
            (11.5, 9.5, 11.0),
            (12.5, 10.5, 12.0),
        ];
        let down = up.map(|(high, low, close)| (-low, -high, -close));
        let none = None;
        for (threshold, prices, line, expected) in [
            (25, up, 1, [none, none, none, none, none, Some(50.0), none]),
            (
                100,
                up,
                1,
                [none, none, none, none, none, Some(50.0), Some(75.0)],
            ),
            (
                25,
                down,
                2,
                [none, none, none, none, none, Some(50.0), none],
            ),
        ] {
            let study = AdxBreakout::new(2, threshold, 3);
            let bars = super::super::hlc_bars(&prices);
            super::super::assert_replay_matches_batch(&study, &bars);
            let mut study = study;
            study.calculate(&bars);
            let actual: Vec<_> = (0..bars.len())
                .map(|at| study.values().value(line, at))
                .collect();
            assert_eq!(actual, expected, "threshold {threshold} line {line}");
            assert_eq!(study.values().value(0, 4), Some(0.0));
        }
    }
}
