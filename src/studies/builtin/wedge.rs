//! Rising and falling wedges.
//!
//! Detects two converging trendlines through confirmed swing pivots, and confirms the wedge when
//! the close leaves it on the side the pattern points to.
//!
//! # Rule
//!
//! Swing pivots are confirmed `right` bars after their bar and alternate between highs and lows
//! (see [`PivotPointsHighLow`](super::PivotPointsHighLow)). Whenever the swings change, the latest
//! `touches` pivot highs and `touches` pivot lows form a candidate when:
//!
//! ```text
//! Rising:   highs rise, lows rise, both lines slope up, the lower line more steeply
//! Falling:  highs fall, lows fall, both lines slope down, the upper line more steeply
//! Both:     width at the last anchor <= width at the first anchor * (100 - convergence) / 100
//!           and the anchors span at most max_bars bars
//! ```
//!
//! The upper line joins the first and last pivot high and the lower line the first and last pivot
//! low; every pivot between them must keep the direction, which is how touches are counted. A
//! candidate is developing: its lines are output on every bar from the bar it is found until it
//! is confirmed, replaced or ends. The wedge is confirmed on the first bar that closes below the
//! lower line of a rising wedge or above the upper line of a falling one. It ends without
//! confirmation when the close leaves it on the other side, when the lines meet, or after
//! `max_bars` bars from the first anchor. A confirmed wedge consumes its pivots, so the next
//! candidate needs new ones.
//!
//! The outputs are the developing upper and lower lines and, on the confirmation bar only, the
//! price of the line the close broke. The table lists the developing wedge and the last `history`
//! confirmed ones with the times of their anchors and confirmation. Nothing found or confirmed
//! changes with later bars. Bars with a non-finite high, low or close are skipped. The defaults
//! are 5 bars on each side of a pivot, 3 touches, a convergence of 10 percent, at most 100 bars and
//! 5 confirmed wedges.

use super::swings::{Swing, Swings};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyCell, StudyTable};
use chrono::{DateTime, Utc};
use egui::Color32;
use std::collections::VecDeque;

/// A wedge that slopes up and narrows, confirmed by a close below its lower line.
pub type RisingWedge = Wedge<true>;
/// A wedge that slopes down and narrows, confirmed by a close above its upper line.
pub type FallingWedge = Wedge<false>;

/// A rising or falling wedge found from confirmed swing pivots.
#[derive(Clone)]
pub struct Wedge<const RISING: bool> {
    left: usize,
    right: usize,
    touches: usize,
    convergence: usize,
    max_bars: usize,
    history: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    table: StudyTable,
    colors: [Color32; 3],
    visible: bool,
}

/// A line through two points, by position among the usable bars.
#[derive(Clone, Copy)]
struct Line {
    from: (f64, f64),
    to: (f64, f64),
}

impl Line {
    fn at(&self, x: f64) -> f64 {
        self.from.1 + (self.to.1 - self.from.1) * (x - self.from.0) / (self.to.0 - self.from.0)
    }
}

#[derive(Clone)]
struct Candidate {
    upper: Line,
    lower: Line,
    first: Swing,
    last: Swing,
}

#[derive(Clone)]
struct Confirmed {
    first: Swing,
    last: Swing,
    time: DateTime<Utc>,
    level: f64,
}

#[derive(Clone, Default)]
struct State {
    swings: Swings,
    candidate: Option<Candidate>,
    confirmed: VecDeque<Confirmed>,
}

impl<const RISING: bool> Wedge<RISING> {
    /// Pivots with `left` and `right` bars on each side, `touches` pivot highs and lows per
    /// wedge, a narrowing of at least `convergence` percent, at most `max_bars` bars between the
    /// first and last anchor and the last `history` confirmed wedges listed. Lengths are raised to
    /// at least 1, touches to 2 and at most 10, convergence to at most 100 and history to at most 100.
    pub fn new(
        left: usize,
        right: usize,
        touches: usize,
        convergence: usize,
        max_bars: usize,
        history: usize,
    ) -> Self {
        Self {
            left: left.max(1),
            right: right.max(1),
            touches: touches.clamp(2, 10),
            convergence: convergence.min(100),
            max_bars: max_bars.max(1),
            history: history.clamp(1, 100),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            table: StudyTable {
                title: if RISING {
                    "Rising Wedge"
                } else {
                    "Falling Wedge"
                }
                .into(),
                columns: ["Wedge", "Anchors", "Confirmed"].map(String::from).to_vec(),
                rows: Vec::new(),
            },
            colors: if RISING {
                [palette::RED, palette::GREEN, palette::RED]
            } else {
                [palette::RED, palette::GREEN, palette::GREEN]
            },
            visible: true,
        }
    }

    /// The wedge the latest `touches` pivot highs and lows form, if any.
    fn detect(&self, pivots: &VecDeque<Swing>) -> Option<Candidate> {
        let used = self.touches * 2;
        let pivots = pivots.range(pivots.len().checked_sub(used)?..);
        let rises = |a: f64, b: f64| if RISING { b > a } else { b < a };
        let side = |high: bool| -> Option<Vec<Swing>> {
            let swings: Vec<_> = pivots.clone().filter(|s| s.high == high).copied().collect();
            swings
                .windows(2)
                .all(|pair| rises(pair[0].price, pair[1].price))
                .then_some(swings)
        };
        let (highs, lows) = (side(true)?, side(false)?);
        let line = |swings: &[Swing]| {
            let (a, b) = (swings[0], swings[swings.len() - 1]);
            Line {
                from: (a.position as f64, a.price),
                to: (b.position as f64, b.price),
            }
        };
        let (upper, lower) = (line(&highs), line(&lows));
        let slope = |line: Line| (line.to.1 - line.from.1) / (line.to.0 - line.from.0);
        let (upper_slope, lower_slope) = (slope(upper), slope(lower));
        let converging = if RISING {
            upper_slope > 0.0 && lower_slope > upper_slope
        } else {
            lower_slope < 0.0 && upper_slope < lower_slope
        };
        let first = *pivots.clone().next()?;
        let last = *pivots.clone().last()?;
        let (start, end) = (first.position as f64, last.position as f64);
        let (before, after) = (
            upper.at(start) - lower.at(start),
            upper.at(end) - lower.at(end),
        );
        let narrows = before > 0.0
            && after > 0.0
            && after * 100.0 <= before * (100 - self.convergence) as f64;
        (converging && narrows && last.position - first.position <= self.max_bars).then_some(
            Candidate {
                upper,
                lower,
                first,
                last,
            },
        )
    }
}

/// Construct with the conventional default parameters.
impl<const RISING: bool> Default for Wedge<RISING> {
    fn default() -> Self {
        Self::new(5, 5, 3, 10, 100, 5)
    }
}

impl<const RISING: bool> Indicator for Wedge<RISING> {
    crate::studies::inputs::study_inputs!(self;
        left: length("Pivot bars left", 1, usize::MAX),
        right: length("Pivot bars right", 1, usize::MAX),
        touches: length("Touches per line", 2, 10),
        convergence: length("Minimum convergence (%)", 0, 100),
        max_bars: length("Maximum bars", 1, usize::MAX),
        history: length("Confirmed wedges listed", 1, 100)
    ; {});

    fn name(&self) -> &str {
        if RISING {
            "Rising Wedge"
        } else {
            "Falling Wedge"
        }
    }

    fn desc(&self) -> &str {
        if RISING {
            "Rising Wedge - Converging upward trendlines through swing pivots, confirmed by a close below the lower line"
        } else {
            "Falling Wedge - Converging downward trendlines through swing pivots, confirmed by a close above the upper line"
        }
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let capacity = self.touches * 2;
        let mut stepper = std::mem::take(&mut self.stepper);
        let mut values = std::mem::take(&mut self.values);
        stepper.run(data, from, &mut values, |state, bar| {
            if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                return IndicatorValue::None;
            }
            let x = state.swings.position as f64;
            if state.swings.push(bar, self.left, self.right, capacity) {
                state.candidate = self.detect(&state.swings.pivots);
            }
            let mut row = [f64::NAN; 3];
            if let Some(candidate) = state.candidate.clone() {
                let (upper, lower) = (candidate.upper.at(x), candidate.lower.at(x));
                let first = candidate.first.position as f64;
                let broke = if RISING {
                    bar.close < lower
                } else {
                    bar.close > upper
                };
                let left_other_side = if RISING {
                    bar.close > upper
                } else {
                    bar.close < lower
                };
                if broke {
                    state.confirmed.push_front(Confirmed {
                        first: candidate.first,
                        last: candidate.last,
                        time: bar.time,
                        level: if RISING { lower } else { upper },
                    });
                    state.confirmed.truncate(self.history);
                    row = [upper, lower, if RISING { lower } else { upper }];
                    state.candidate = None;
                    state.swings.pivots.clear();
                } else if left_other_side || upper <= lower || x - first > self.max_bars as f64 {
                    state.candidate = None;
                } else {
                    row[..2].copy_from_slice(&[upper, lower]);
                }
            }
            IndicatorValue::Multiple(row.to_vec())
        });
        let state = stepper.state();
        let time = |time: DateTime<Utc>| time.format("%Y-%m-%d %H:%M").to_string();
        let anchors =
            |first: &Swing, last: &Swing| format!("{} to {}", time(first.time), time(last.time));
        let text = |text: String| StudyCell::Text(text);
        self.table.rows = state
            .candidate
            .iter()
            .map(|c| {
                vec![
                    text("Developing".into()),
                    text(anchors(&c.first, &c.last)),
                    text("-".into()),
                ]
            })
            .chain(state.confirmed.iter().map(|c| {
                vec![
                    text(format!("Broke {:.2}", c.level)),
                    text(anchors(&c.first, &c.last)),
                    text(time(c.time)),
                ]
            }))
            .collect();
        if self.table.rows.is_empty() {
            self.table
                .rows
                .push(["No wedge", "-", "-"].map(|t| text(t.into())).to_vec());
        }
        self.stepper = stepper;
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 | 1 => crate::studies::PlotType::Line,
                _ if RISING => crate::studies::PlotType::TriangleDown,
                _ => crate::studies::PlotType::TriangleUp,
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
        ["Upper Line", "Lower Line", "Confirmed Breakout"]
            .map(String::from)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_bars(closes: &[f64]) -> Vec<Bar> {
        super::super::hlc_bars(&closes.iter().map(|&c| (c, c, c)).collect::<Vec<_>>())
    }

    fn lines<const RISING: bool>(wedge: &Wedge<RISING>, bars: &[Bar]) -> [Vec<Option<f64>>; 3] {
        let mut wedge = wedge.clone();
        wedge.calculate(bars);
        [0, 1, 2].map(|line| {
            (0..bars.len())
                .map(|at| wedge.values().value(line, at))
                .collect()
        })
    }

    /// One pivot on each side, two touches, by hand. Pivots: low 10 at bar 1, high 20 at 3, low 14
    /// at 5 and high 23 at 7, confirmed on bar 8. The upper line has slope 3/4 and the lower 1, so
    /// they meet and the width falls from 8.5 at bar 1 to 7 at bar 7. At bar 8 the lines are 23.75
    /// and 17, at bar 9 24.5 and 18. Bar 10 closes at 17.5 under the lower line 19: confirmed.
    /// Negated prices give the falling wedge, confirmed by a close over its upper line.
    #[test]
    fn wedges_follow_the_swing_and_breakout_rules() {
        let rising = [
            12.0, 10.0, 15.0, 20.0, 17.0, 14.0, 19.0, 23.0, 21.0, 20.0, 17.5,
        ];
        let (up, down) = (
            RisingWedge::new(1, 1, 2, 0, 100, 5),
            FallingWedge::new(1, 1, 2, 0, 100, 5),
        );
        let nan = None;
        let bars = flat_bars(&rising);
        assert_eq!(
            lines(&up, &bars),
            [
                [
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    Some(23.75),
                    Some(24.5),
                    Some(25.25)
                ]
                .to_vec(),
                [
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    Some(17.0),
                    Some(18.0),
                    Some(19.0)
                ]
                .to_vec(),
                [nan, nan, nan, nan, nan, nan, nan, nan, nan, nan, Some(19.0)].to_vec(),
            ]
        );
        let mut wedge = up.clone();
        wedge.calculate(&bars);
        let rows: Vec<_> = wedge
            .table()
            .unwrap()
            .rows
            .iter()
            .map(|row| format!("{row:?}"))
            .collect();
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].contains("Broke 19.00")
                && rows[0].contains("1970-01-02 00:00 to 1970-01-08 00:00")
        );
        assert!(rows[0].contains("1970-01-11 00:00"), "{}", rows[0]);
        super::super::assert_replay_matches_batch(&up, &bars);
        // The second high 19 is under the first 20, so no rising wedge forms; the falling one needs
        // falling lows, and the rising series has none.
        let ambiguous = flat_bars(&[
            12.0, 10.0, 15.0, 20.0, 17.0, 14.0, 19.0, 19.0, 18.0, 17.0, 16.0,
        ]);
        assert!(lines(&up, &ambiguous).iter().flatten().all(Option::is_none));
        assert!(lines(&down, &bars).iter().flatten().all(Option::is_none));
        // The mirror image is the falling wedge, its lines swapped and confirmed over the upper.
        let mirrored = flat_bars(&rising.map(|price| -price));
        assert_eq!(
            lines(&down, &mirrored),
            [
                [
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    Some(-17.0),
                    Some(-18.0),
                    Some(-19.0)
                ]
                .to_vec(),
                [
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    Some(-23.75),
                    Some(-24.5),
                    Some(-25.25)
                ]
                .to_vec(),
                [
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    Some(-19.0)
                ]
                .to_vec(),
            ]
        );
        // Bar 9 closes at 25 over the upper line 24.5, which ends the wedge unconfirmed. The low 21
        // on bar 8 is confirmed then and forms a wedge whose upper line is the same, so it ends too.
        let invalidated = flat_bars(&[
            12.0, 10.0, 15.0, 20.0, 17.0, 14.0, 19.0, 23.0, 21.0, 25.0, 26.0,
        ]);
        let [upper, _, confirmed] = lines(&up, &invalidated);
        assert_eq!(upper[8], Some(23.75));
        assert!(upper[9..].iter().chain(&confirmed).all(Option::is_none));
    }
}
