//! Missed Pivot Levels.
//!
//! Swing highs and lows that price has not returned to. Each level is the price of a pivot
//! (see [`PivotPointsHighLow`](super::PivotPointsHighLow)) and follows three rules:
//!
//! - **Confirmed** `right` bars after its pivot bar, when the pivot window is complete. Bars of
//!   that window never revisit it.
//! - **Missed** from then on: a pivot high lies above every later bar and a pivot low below.
//! - **Revisited** at the first later bar whose high reaches the level of a pivot high, or whose
//!   low reaches the level of a pivot low, within a tolerance of `tolerance` percent of the level
//!   (high >= level * (1 - t), low <= level * (1 + t), t = tolerance / 100). A revisited level is
//!   invalidated: it is drawn up to the revisit bar and takes no further part.
//!
//! The latest `max_levels` levels, missed or revisited, are kept. Every level is a zone from its
//! pivot bar to its revisit bar, or to the last bar while it is missed, in the colour of its
//! side (pivot highs first, then pivot lows). It has no per-bar values. The defaults are 10 bars on each side, a tolerance of 0.1% and 20 levels. Overlay indicator.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, MissedPivots};
//!
//! // A peak at 10, then a fall that never returns to it.
//! let bars: Vec<Bar> = [5.0, 8.0, 10.0, 8.0, 5.0, 6.0]
//!     .into_iter()
//!     .enumerate()
//!     .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64 * 60, 0).unwrap(), p, p, p, p, 1.0))
//!     .collect();
//! let mut missed = MissedPivots::new(2, 2, 0.1, 20);
//! missed.calculate(&bars);
//! // The pivot high at bar 2 is confirmed at bar 4 and is still missed.
//! assert_eq!(missed.zones().len(), 1);
//! assert_eq!((missed.zones()[0].bars.clone(), missed.zones()[0].top), (2..6, 10.0));
//! ```

use super::clamp_param;
use super::pivot_points_high_low::Swings;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyZone};
use egui::Color32;
use std::collections::VecDeque;

/// The most levels that can be kept.
const MAX_LEVELS: usize = 500;

/// Missed Pivot Levels: swing highs and lows that price has not returned to.
#[derive(Clone)]
pub struct MissedPivots {
    left: usize,
    right: usize,
    tolerance: f64,
    max_levels: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    zones: Vec<StudyZone>,
    /// Missed pivot high, missed pivot low.
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone)]
struct Level {
    bar: usize,
    price: f64,
    high: bool,
    revisited: Option<usize>,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    swings: Swings,
    levels: VecDeque<Level>,
}

impl MissedPivots {
    /// Pivots with `left` and `right` bars on each side (at least 1), revisited within
    /// `tolerance` percent (0 to 100, NaN to 0) of the level, keeping the latest `max_levels`
    /// (1 to 500).
    pub fn new(left: usize, right: usize, tolerance: f64, max_levels: usize) -> Self {
        Self {
            left: left.max(1),
            right: right.max(1),
            tolerance: clamp_param(tolerance, 0.0, 100.0),
            max_levels: max_levels.clamp(1, MAX_LEVELS),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            zones: Vec::new(),
            colors: [palette::RED, palette::GREEN],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for MissedPivots {
    fn default() -> Self {
        Self::new(10, 10, 0.1, 20)
    }
}

impl Indicator for MissedPivots {
    crate::studies::inputs::study_inputs!(self;
        left: length("Pivot left bars", 1, usize::MAX),
        right: length("Pivot right bars", 1, usize::MAX),
        tolerance: number("Revisit tolerance (%)", 0.0, 100.0),
        max_levels: length("Max number of levels", 1, MAX_LEVELS)
    ; {});

    fn name(&self) -> &str {
        "Missed Pivot Levels"
    }

    fn desc(&self) -> &str {
        "Missed Pivot Levels - Swing highs and lows that price has not returned to"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (left, right, max_levels) = (self.left, self.right, self.max_levels);
        let tolerance = self.tolerance / 100.0;
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.source;
                state.source += 1;
                // A bar without a finite high and low takes no part, as if it were absent.
                if !(bar.high.is_finite() && bar.low.is_finite()) {
                    return IndicatorValue::None;
                }
                for level in state.levels.iter_mut().filter(|l| l.revisited.is_none()) {
                    let reach = level.price.abs() * tolerance;
                    let reached = if level.high {
                        bar.high >= level.price - reach
                    } else {
                        bar.low <= level.price + reach
                    };
                    if reached {
                        level.revisited = Some(index);
                    }
                }
                let confirmed = state.swings.push(index, bar, left, right);
                for (high, pivot) in [(true, confirmed[0]), (false, confirmed[1])] {
                    if let Some((bar, price)) = pivot {
                        state.levels.push_back(Level {
                            bar,
                            price,
                            high,
                            revisited: None,
                        });
                    }
                }
                while state.levels.len() > max_levels {
                    state.levels.pop_front();
                }
                IndicatorValue::None
            });
        let state = self.stepper.state();
        self.zones = state
            .levels
            .iter()
            .map(|level| StudyZone {
                bars: level.bar..level.revisited.map_or(state.source, |at| at + 1),
                top: level.price,
                bottom: level.price,
                plot: usize::from(!level.high),
            })
            .collect();
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn zones(&self) -> &[StudyZone] {
        &self.zones
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
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
        vec!["Missed high".to_string(), "Missed low".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bars(highs_lows: &[(f64, f64)]) -> Vec<Bar> {
        highs_lows
            .iter()
            .enumerate()
            .map(|(i, &(high, low))| {
                let time = Utc.timestamp_opt(i as i64 * 60, 0).unwrap();
                Bar::new(time, low, high, low, low, 1.0)
            })
            .collect()
    }

    /// A hand-labelled history with pivots of one bar each side, given to the study a bar at a
    /// time. The pivot high 10 at bar 1 is confirmed at bar 2 and revisited at bar 5, whose high
    /// 9.96 is within 0.5% of 10 (10 - 0.05 = 9.95); the pivot low 4 at bar 3 is confirmed at
    /// bar 4 and still missed, bar 5 lows staying above 4 * 1.005 = 4.02; the pivot high 9.96 at
    /// bar 5 is confirmed at bar 6 and missed.
    #[test]
    fn levels_are_confirmed_missed_and_revisited() {
        let data = bars(&[
            (6.0, 5.0),
            (10.0, 8.0),
            (9.0, 7.0),
            (6.0, 4.0),
            (7.0, 5.0),
            (9.96, 8.0),
            (8.0, 6.0),
        ]);
        // Bars seen, then the zones (bars, price, plot) at that point.
        let want: [(usize, Vec<(std::ops::Range<usize>, f64, usize)>); 4] = [
            (2, vec![]),
            (3, vec![(1..3, 10.0, 0)]),
            (5, vec![(1..5, 10.0, 0), (3..5, 4.0, 1)]),
            (7, vec![(1..6, 10.0, 0), (3..7, 4.0, 1), (5..7, 9.96, 0)]),
        ];
        let mut missed = MissedPivots::new(1, 1, 0.5, 20);
        for (seen, zones) in want {
            missed.calculate(&data[..seen]);
            let got: Vec<_> = missed
                .zones()
                .iter()
                .map(|z| (z.bars.clone(), z.top, z.plot))
                .collect();
            assert_eq!(got, zones, "after {seen} bars");
        }
    }
}
