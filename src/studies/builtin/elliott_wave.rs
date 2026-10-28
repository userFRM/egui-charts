//! Elliott impulse waves.
//!
//! Finds five-wave impulses in confirmed swing pivots, with the wave number of each anchor, and
//! lists the developing and the confirmed constructions.
//!
//! # Rule
//!
//! Swing pivots are confirmed `right` bars after their bar and alternate between highs and lows
//! (see [`PivotPointsHighLow`](super::PivotPointsHighLow)). Six pivots `P0` to `P5` starting at a
//! low form a bullish impulse, starting at a high a bearish one, which mirrors it. For the bullish
//! impulse, with `Wn` the length of wave `n` between `P(n-1)` and `Pn`:
//!
//! ```text
//! Wave 2 stays above the start of wave 1:   P2 > P0
//! Wave 3 passes the end of wave 1:          P3 > P1
//! Wave 4 does not overlap wave 1:           P4 > P1
//! Wave 3 is not the shortest of 1, 3 and 5: not (W3 < W1 and W3 < W5)
//! ```
//!
//! A truncated fifth wave is allowed. Whenever the swings change, the latest pivots are checked for
//! the longest construction that satisfies the rules up to the wave reached. From wave 3 on it is
//! developing, with its wave number at the latest anchor; it is invalidated when the close breaks
//! the end of wave 1 (`P1`), where wave 4 would overlap wave 1, or when a later pivot breaks a rule.
//! A construction that reaches `P5` is confirmed on the bar that confirms `P5` and consumes its
//! pivots, except `P5`, so the next one needs new pivots. Confirmed constructions never change.
//!
//! The outputs are the price of the latest developing anchor, the price of the last anchor of a
//! confirmed bullish and of a confirmed bearish impulse, and the signed wave number, positive for a
//! bullish construction, which is hidden. Each is on the bar that confirms the pivot and drawn
//! displaced to the pivot's own bar. The table lists the developing construction and the last
//! `history` confirmed ones with their direction, confirmation time and the price of every anchor
//! numbered from 0. Bars with a non-finite high, low or close are skipped. The defaults are 5 bars on
//! each side of a pivot and 5 confirmed impulses.

use super::swings::{Swing, Swings};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyCell, StudyTable};
use chrono::{DateTime, Utc};
use egui::Color32;
use std::collections::VecDeque;

/// Elliott Wave: five-wave impulses in confirmed swing pivots.
#[derive(Clone)]
pub struct ElliottWave {
    left: usize,
    right: usize,
    history: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    table: StudyTable,
    colors: [Color32; 3],
    visible: bool,
}

/// Anchors `P0` to `Pn` of a construction.
#[derive(Clone)]
struct Construction {
    anchors: Vec<Swing>,
    confirmed: Option<DateTime<Utc>>,
}

impl Construction {
    fn bullish(&self) -> bool {
        !self.anchors[0].high
    }

    /// The wave number reached.
    fn wave(&self) -> usize {
        self.anchors.len() - 1
    }
}

#[derive(Clone, Default)]
struct State {
    swings: Swings,
    developing: Option<Construction>,
    confirmed: VecDeque<Construction>,
}

/// The longest construction of at least three waves that ends at the newest pivot.
fn construct(pivots: &VecDeque<Swing>) -> Option<Construction> {
    (3..=5).rev().find_map(|waves| {
        let anchors: Vec<_> = pivots
            .range(pivots.len().checked_sub(waves + 1)?..)
            .copied()
            .collect();
        // Prices as seen by a bullish impulse, negated for a bearish one.
        let sign = if anchors[0].high { -1.0 } else { 1.0 };
        let p = |n: usize| sign * anchors[n].price;
        let wave = |n: usize| p(n) - p(n - 1);
        let valid = p(2) > p(0)
            && p(3) > p(1)
            && (waves < 4 || p(4) > p(1))
            && (waves < 5 || wave(3) >= wave(1) || wave(3) >= wave(5));
        valid.then_some(Construction {
            anchors,
            confirmed: None,
        })
    })
}

impl ElliottWave {
    /// Pivots with `left` and `right` bars on each side, each raised to at least 1, and the last
    /// `history` confirmed impulses listed, from 1 to 100.
    pub fn new(left: usize, right: usize, history: usize) -> Self {
        Self {
            left: left.max(1),
            right: right.max(1),
            history: history.clamp(1, 100),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            table: StudyTable {
                title: "Elliott Wave".into(),
                columns: ["Impulse", "Confirmed", "Anchors"]
                    .map(String::from)
                    .to_vec(),
                rows: Vec::new(),
            },
            colors: [palette::ACCENT, palette::GREEN, palette::RED],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for ElliottWave {
    fn default() -> Self {
        Self::new(5, 5, 5)
    }
}

impl Indicator for ElliottWave {
    crate::studies::inputs::study_inputs!(self;
        left: length("Pivot bars left", 1, usize::MAX),
        right: length("Pivot bars right", 1, usize::MAX),
        history: length("Confirmed impulses listed", 1, 100)
    ; {});

    fn name(&self) -> &str {
        "Elliott Wave"
    }

    fn desc(&self) -> &str {
        "Elliott Wave - Five-wave impulses in confirmed swing pivots, developing and confirmed"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut stepper = std::mem::take(&mut self.stepper);
        let mut values = std::mem::take(&mut self.values);
        stepper.run(data, from, &mut values, |state, bar| {
            if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                return IndicatorValue::None;
            }
            let mut row = [f64::NAN; 4];
            if state.swings.push(bar, self.left, self.right, 6) {
                state.developing = construct(&state.swings.pivots);
                if let Some(mut construction) = state.developing.take() {
                    let sign = if construction.bullish() { 1.0 } else { -1.0 };
                    let last = construction.anchors[construction.wave()].price;
                    row[3] = sign * construction.wave() as f64;
                    if construction.wave() == 5 {
                        row[if construction.bullish() { 1 } else { 2 }] = last;
                        construction.confirmed = Some(bar.time);
                        state.confirmed.push_front(construction);
                        state.confirmed.truncate(self.history);
                        // The last pivot starts the next construction.
                        while state.swings.pivots.len() > 1 {
                            state.swings.pivots.pop_front();
                        }
                    } else {
                        row[0] = last;
                        state.developing = Some(construction);
                    }
                }
            } else if let Some(construction) = &state.developing {
                // Wave 4 would overlap wave 1.
                let wave_1_end = construction.anchors[1].price;
                if (construction.bullish() && bar.close < wave_1_end)
                    || (!construction.bullish() && bar.close > wave_1_end)
                {
                    state.developing = None;
                }
            }
            IndicatorValue::Multiple(row.to_vec())
        });
        let state = stepper.state();
        let time = |time: DateTime<Utc>| time.format("%Y-%m-%d %H:%M").to_string();
        let describe = |construction: &Construction| {
            let anchors = construction
                .anchors
                .iter()
                .enumerate()
                .map(|(n, anchor)| format!("{n}:{:.2}", anchor.price))
                .collect::<Vec<_>>()
                .join(" ");
            let direction = if construction.bullish() {
                "Bullish"
            } else {
                "Bearish"
            };
            let confirmed = construction.confirmed.map_or("-".into(), time);
            vec![
                StudyCell::Text(format!("{direction}, wave {}", construction.wave())),
                StudyCell::Text(confirmed),
                StudyCell::Text(anchors),
            ]
        };
        self.table.rows = state
            .developing
            .iter()
            .chain(&state.confirmed)
            .map(describe)
            .collect();
        if self.table.rows.is_empty() {
            let text = |text: &str| StudyCell::Text(text.into());
            self.table
                .rows
                .push(vec![text("No impulse"), text("-"), text("-")]);
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

    /// The pivot's own bar, `right` bars before its confirmation.
    fn plot_offset(&self, line: usize) -> isize {
        if line < 3 {
            -(self.right.min(isize::MAX as usize) as isize)
        } else {
            0
        }
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        (line == 0).then(|| {
            let wave = self.values.value(3, bar)?;
            Some(if wave > 0.0 {
                self.colors[1]
            } else {
                self.colors[2]
            })
        })?
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        self.values.value(line, bar)?;
        let wave = self.values.value(3, bar)?;
        (line < 3).then(|| format!("{}", wave.abs()))
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line.min(2)],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 => crate::studies::PlotType::Circles,
                1 => crate::studies::PlotType::TriangleUp,
                _ => crate::studies::PlotType::TriangleDown,
            },
            visible: line < 3,
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
        [
            "Developing Anchor",
            "Confirmed Bullish Impulse",
            "Confirmed Bearish Impulse",
            "Wave",
        ]
        .map(String::from)
        .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMPULSE: [f64; 13] = [
        105.0, 100.0, 105.0, 110.0, 107.0, 104.0, 115.0, 125.0, 120.0, 115.0, 122.0, 130.0, 128.0,
    ];

    fn run(closes: &[f64]) -> (Vec<Bar>, ElliottWave) {
        let hlc: Vec<_> = closes.iter().map(|&c| (c, c, c)).collect();
        let bars = super::super::hlc_bars(&hlc);
        let mut study = ElliottWave::new(1, 1, 5);
        study.calculate(&bars);
        (bars, study)
    }

    fn column(study: &ElliottWave, line: usize, len: usize) -> Vec<Option<f64>> {
        (0..len).map(|at| study.values().value(line, at)).collect()
    }

    /// Pivots by hand with one bar on each side: low 100 (bar 1), high 110 (3), low 104 (5), high 125
    /// (7), low 115 (9) and high 130 (11), confirmed one bar later. Waves 1, 3 and 5 are 10, 21 and 15
    /// long, wave 2 stays over 100 and wave 4 over 110. Wave 3 is developing on bar 8 and wave 4 on
    /// bar 10, and the impulse is confirmed on bar 12. Negated prices give the bearish impulse.
    #[test]
    fn impulses_are_developing_then_confirmed_without_changes() {
        let none = None;
        for sign in [1.0, -1.0] {
            let closes = IMPULSE.map(|c| sign * c);
            let (bars, study) = run(&closes);
            super::super::assert_replay_matches_batch(&study, &bars);
            let anchor = |price: f64| Some(sign * price);
            assert_eq!(
                column(&study, 0, 13),
                [
                    none,
                    none,
                    none,
                    none,
                    none,
                    none,
                    none,
                    none,
                    anchor(125.0),
                    none,
                    anchor(115.0),
                    none,
                    none
                ]
            );
            let confirmed = column(&study, if sign > 0.0 { 1 } else { 2 }, 13);
            assert_eq!(confirmed[..12], [none; 12]);
            assert_eq!(confirmed[12], anchor(130.0));
            assert_eq!(
                column(&study, if sign > 0.0 { 2 } else { 1 }, 13),
                [none; 13]
            );
            assert_eq!(study.plot_text(0, 8).as_deref(), Some("3"));
            assert_eq!(
                study
                    .plot_text(if sign > 0.0 { 1 } else { 2 }, 12)
                    .as_deref(),
                Some("5")
            );
            assert_eq!(study.plot_offset(0), -1);
            let rows = &study.table().unwrap().rows;
            assert_eq!(rows.len(), 1);
            let StudyCell::Text(anchors) = &rows[0][2] else {
                panic!()
            };
            let expected: Vec<_> = [100.0, 110.0, 104.0, 125.0, 115.0, 130.0]
                .iter()
                .enumerate()
                .map(|(n, p)| format!("{n}:{:.2}", sign * p))
                .collect();
            // A bearish impulse starts at a high, so its anchors are the negated prices.
            assert_eq!(anchors, &expected.join(" "));
            assert_eq!(rows[0][1], StudyCell::Text("1970-01-13 00:00".into()));
        }
    }

    /// Wave 3 shorter than waves 1 and 5 breaks the rule: with P3 = 112, P4 = 111 and P5 = 130
    /// the waves are 10, 8 and 19, so nothing is confirmed. The pivots from the 104 low are a new
    /// wave 3 on bar 12. A close under the end of wave 1 ends the developing wave on the bar.
    #[test]
    fn invalid_sequences_are_not_confirmed() {
        let (_, study) = run(&[
            105.0, 100.0, 105.0, 110.0, 107.0, 104.0, 108.0, 112.0, 111.5, 111.0, 120.0, 130.0,
            128.0,
        ]);
        assert_eq!(column(&study, 1, 13), [None; 13]);
        assert_eq!(study.values().value(0, 12), Some(130.0));
        assert_eq!(study.values().value(3, 12), Some(3.0));
        // The impulse up to bar 8, then a close at 108 under 110.
        let (_, study) = run(&[
            105.0, 100.0, 105.0, 110.0, 107.0, 104.0, 115.0, 125.0, 120.0, 108.0,
        ]);
        assert_eq!(study.values().value(0, 8), Some(125.0));
        let rows = &study.table().unwrap().rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], StudyCell::Text("No impulse".into()));
    }
}
