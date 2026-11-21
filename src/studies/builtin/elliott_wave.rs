//! Elliott impulse waves.
//!
//! Finds five-wave impulses in confirmed swing pivots, with the wave number of each anchor, and
//! lists the developing and the confirmed constructions.
//!
//! # Rule
//!
//! Swing pivots alternate between highs and lows. With swings sized by bar count, the default, a
//! pivot is confirmed `right` bars after its bar (see
//! [`PivotPointsHighLow`](super::PivotPointsHighLow)). Sized by price or by percent, a pivot is
//! confirmed by the first later bar that moves back from it by at least the swing move, a price
//! amount or a percentage of the pivot's absolute price. Six pivots `P0` to `P5` starting at a
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
//! developing; it is invalidated when the close breaks the end of wave 1 (`P1`), where wave 4 would
//! overlap wave 1, or when a later pivot breaks a rule.
//! A construction that reaches `P5` is confirmed on the bar that confirms `P5` and consumes its
//! pivots, except `P5`, so the next one needs new pivots. Confirmed constructions never change.
//!
//! The outputs are the price of the latest developing anchor, the price of the last anchor of a
//! confirmed bullish and of a confirmed bearish impulse, and the signed wave number, positive for a
//! bullish construction, which is hidden. Each is on the bar that confirms the pivot, drawn
//! displaced to the pivot's own bar when swings are sized by bar count. The table lists the
//! developing construction and the last `history` confirmed ones with their direction,
//! confirmation time and the price of every anchor numbered from 0. Bars with a non-finite high,
//! low or close are skipped.
//!
//! Each shown construction is drawn as legs joining its anchors, numbered 0 to 5, in the style of
//! its plot: Developing Anchor, dashed by default, or Confirmed Bullish or Bearish Impulse. With
//! Place as drawings each is drawn as an Elliott impulse wave drawing instead, its waves labelled
//! 1 to 5. Hiding the developing wave removes the developing construction, its value and its table
//! row. Projections extend the developing construction by its next wave, dashed: wave 4 retraces
//! 38.2% of wave 3 but stops at the end of wave 1, and wave 5 is as long as the shorter of waves 1
//! and 3. It ends as many bars after the latest anchor as wave 2, or wave 1, took. The legs and
//! drawings need the `drawings` feature. Without it, Place as drawings also hides the anchor
//! numbers; projection numbers remain available.
//!
//! The defaults are swings of 5 bars on each side of a pivot, a swing move of 5, 5 confirmed
//! impulses listed, the developing wave shown, and no projections or drawings.

use super::swings::{Swing, Swings};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, LabelPlacement, StudyCell, StudyLabel, StudyTable,
};
use chrono::{DateTime, Utc};
use egui::Color32;
use std::collections::VecDeque;

/// How the swings an impulse is built from are sized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwingMode {
    /// A pivot beats `left` bars before it and `right` bars after it.
    Bars,
    /// A pivot is confirmed by a move back from it of at least `swing_move` in price.
    Price,
    /// A pivot is confirmed by a move back from it of at least `swing_move` percent of its price.
    Percent,
}

impl SwingMode {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Bars => "Bars",
            Self::Price => "Price",
            Self::Percent => "Percent",
        }
    }
}

/// Elliott Wave: five-wave impulses in confirmed swing pivots.
#[derive(Clone, Debug)]
pub struct ElliottWave {
    swing_mode: SwingMode,
    left: usize,
    right: usize,
    swing_move: f64,
    history: usize,
    show_developing: bool,
    projections: bool,
    as_drawings: bool,
    values: IndicatorValues,
    stepper: Stepper<State>,
    table: StudyTable,
    labels: Vec<StudyLabel>,
    /// Legs, impulse drawings and the projected leg, each with the plot that styles it.
    #[cfg(feature = "drawings")]
    drawings: Vec<crate::drawings::Drawing>,
    #[cfg(feature = "drawings")]
    drawing_plots: Vec<usize>,
    colors: [Color32; 3],
    visible: bool,
}

/// Anchors `P0` to `Pn` of a construction.
#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Default)]
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

/// The next anchor of a developing construction, in its own direction: wave 4 retraces 38.2% of
/// wave 3 but stops at the end of wave 1, and wave 5 is as long as the shorter of waves 1 and 3.
/// It lies as many bars after the latest anchor as wave 2, or wave 1, took. `None` when the price
/// is not finite.
fn projection(construction: &Construction) -> Option<(usize, f64)> {
    let anchors = &construction.anchors;
    let sign = if construction.bullish() { 1.0 } else { -1.0 };
    let p = |n: usize| sign * anchors[n].price;
    let i = |n: usize| anchors[n].index;
    let (target, bar) = if construction.wave() == 3 {
        let target = p(3) - 0.382 * (p(3) - p(2));
        // Checked first, as `max` would turn a NaN into the end of wave 1.
        (
            target.is_finite().then(|| target.max(p(1)))?,
            i(3) + (i(2) - i(1)),
        )
    } else {
        let target = p(4) + (p(1) - p(0)).min(p(3) - p(2));
        (target.is_finite().then_some(target)?, i(4) + (i(1) - i(0)))
    };
    Some((bar, sign * target))
}

impl ElliottWave {
    /// Pivots with `left` and `right` bars on each side, each raised to at least 1, and the last
    /// `history` confirmed impulses listed, from 1 to 100. The other inputs start at their
    /// defaults; see the module docs.
    pub fn new(left: usize, right: usize, history: usize) -> Self {
        Self {
            swing_mode: SwingMode::Bars,
            left: left.max(1),
            right: right.max(1),
            swing_move: 5.0,
            history: history.clamp(1, 100),
            show_developing: true,
            projections: false,
            as_drawings: false,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            table: StudyTable {
                title: "Elliott Wave".into(),
                columns: ["Impulse", "Confirmed", "Anchors"]
                    .map(String::from)
                    .to_vec(),
                rows: Vec::new(),
                ..Default::default()
            },
            labels: Vec::new(),
            #[cfg(feature = "drawings")]
            drawings: Vec::new(),
            #[cfg(feature = "drawings")]
            drawing_plots: Vec::new(),
            colors: [palette::ACCENT, palette::GREEN, palette::RED],
            visible: true,
        }
    }

    /// Rebuild the anchor numbers, legs or impulse drawings and the projection of the shown
    /// constructions, oldest first, from the calculated state.
    fn draw(&mut self) {
        let state = self.stepper.state();
        let developing = state.developing.as_ref().filter(|_| self.show_developing);
        let shown = state
            .confirmed
            .iter()
            .rev()
            .map(|construction| (construction, if construction.bullish() { 1 } else { 2 }))
            .chain(developing.map(|construction| (construction, 0)));
        let label = |bar: usize, price: f64, number: usize, above: bool, plot: usize| StudyLabel {
            bar,
            price,
            text: number.to_string(),
            placement: if above {
                LabelPlacement::Above
            } else {
                LabelPlacement::Below
            },
            plot,
        };
        #[cfg(feature = "drawings")]
        let mut drawings = Vec::new();
        #[cfg(feature = "drawings")]
        let mut drawing = |tool, points: &[(usize, f64)], plot: usize| {
            let style = self.plot_style(plot);
            let mut drawing = crate::drawings::Drawing::new(0, tool);
            drawing.completed = true;
            drawing.stroke_width = style.width;
            drawing.line_style = style.pattern;
            drawing.color = style.color.to_srgba_unmultiplied();
            drawing.chart_points = points
                .iter()
                .map(|&(bar, price)| crate::drawings::ChartPoint::new(bar as f32, price))
                .collect();
            drawings.push((drawing, plot));
        };
        let mut labels = Vec::new();
        let mut previous_end = None;
        for (construction, plot) in shown {
            let points: Vec<_> = construction
                .anchors
                .iter()
                .map(|anchor| (anchor.index, anchor.price))
                .collect();
            if self.as_drawings {
                #[cfg(feature = "drawings")]
                drawing(
                    crate::drawings::DrawingToolType::ElliottImpulse,
                    &points,
                    plot,
                );
            } else {
                for (n, anchor) in construction.anchors.iter().enumerate() {
                    // A construction starts where the one before it ended, which keeps its number.
                    if n > 0 || previous_end != Some(anchor.index) {
                        labels.push(label(anchor.index, anchor.price, n, anchor.high, plot));
                    }
                }
                #[cfg(feature = "drawings")]
                for leg in points.windows(2) {
                    drawing(crate::drawings::DrawingToolType::TrendLine, leg, plot);
                }
            }
            previous_end = points.last().map(|&(bar, _)| bar);
        }
        if self.projections
            && let Some(construction) = developing
            && let Some((bar, price)) = projection(construction)
        {
            let last = construction.anchors[construction.wave()];
            labels.push(label(bar, price, construction.wave() + 1, !last.high, 0));
            #[cfg(feature = "drawings")]
            drawing(
                crate::drawings::DrawingToolType::TrendLine,
                &[(last.index, last.price), (bar, price)],
                0,
            );
        }
        labels.sort_by_key(|label| label.bar);
        self.labels = labels;
        #[cfg(feature = "drawings")]
        {
            (self.drawings, self.drawing_plots) = drawings.into_iter().unzip();
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
        swing_mode: choice("Swing size", [SwingMode::Bars, SwingMode::Price, SwingMode::Percent]),
        left: length("Pivot bars left", 1, usize::MAX) if swing_mode in [0],
        right: length("Pivot bars right", 1, usize::MAX) if swing_mode in [0],
        swing_move: number("Swing move", 0.0, f64::MAX) if swing_mode in [1, 2],
        history: length("Confirmed impulses listed", 1, 100),
        show_developing: toggle("Show developing wave"),
        projections: toggle("Show projections"),
        as_drawings: toggle("Place as drawings")
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
        let (swing_mode, swing_move) = (self.swing_mode, self.swing_move);
        stepper.run(data, from, &mut values, |state, bar| {
            if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                state.swings.skip();
                return IndicatorValue::None;
            }
            let mut row = [f64::NAN; 4];
            let changed = match swing_mode {
                SwingMode::Bars => state.swings.push(bar, self.left, self.right, 6),
                SwingMode::Price => state.swings.push_move(bar, |_| swing_move, 6),
                SwingMode::Percent => state.swings.push_move(
                    bar,
                    // Of the absolute price, so mirrored bars find the mirrored impulse.
                    |price| price.abs() * swing_move / 100.0,
                    6,
                ),
            };
            if changed {
                state.developing = construct(&state.swings.pivots);
                if let Some(mut construction) = state.developing.take() {
                    let sign = if construction.bullish() { 1.0 } else { -1.0 };
                    let last = construction.anchors[construction.wave()].price;
                    let wave = sign * construction.wave() as f64;
                    if construction.wave() == 5 {
                        row[3] = wave;
                        row[if construction.bullish() { 1 } else { 2 }] = last;
                        construction.confirmed = Some(bar.time);
                        state.confirmed.push_front(construction);
                        state.confirmed.truncate(self.history);
                        // The last pivot starts the next construction.
                        while state.swings.pivots.len() > 1 {
                            state.swings.pivots.pop_front();
                        }
                    } else {
                        if self.show_developing {
                            row[0] = last;
                            row[3] = wave;
                        }
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
            .filter(|_| self.show_developing)
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
        self.draw();
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }

    /// With swings sized by bar count, the pivot's own bar, `right` bars before its
    /// confirmation; sized by price, the confirming bar, as the delay varies.
    fn plot_offset(&self, line: usize) -> isize {
        if line < 3 && self.swing_mode == SwingMode::Bars {
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

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line.min(2)],
            width: 1.0,
            // Dashes tell the developing legs apart.
            pattern: if line == 0 {
                crate::model::LineStyle::Dashed
            } else {
                crate::model::LineStyle::Solid
            },
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

    fn labels(&self) -> &[StudyLabel] {
        &self.labels
    }

    #[cfg(feature = "drawings")]
    fn drawings(&self) -> &[crate::drawings::Drawing] {
        &self.drawings
    }

    /// Each construction takes the style of its own plot.
    #[cfg(feature = "drawings")]
    fn drawings_use_plot_style(&self) -> bool {
        false
    }

    #[cfg(feature = "drawings")]
    fn drawing_plot(&self, index: usize) -> Option<usize> {
        self.drawing_plots.get(index).copied()
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
        self.draw();
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    // Its drawings and labels cannot follow source bars onto another symbol or interval.
    fn draws_on_other_bars(&self) -> bool {
        false
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
    use crate::studies::InputValue;

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

    /// `IMPULSE` and a close at 122 that moves 8 back from the last high. Sized by a price move
    /// of 6, or 5.2% of the pivot price, the pivots are low 100 (confirmed on bar 3), high 110
    /// (bar 5, a fall of exactly 6), low 104 (6), high 125 (9), low 115 (10) and high 130 (13).
    /// With a move of 7, or 5.5%, the 6 fall from 110 is too short and the pivots are 100, 125,
    /// 115 and 130, so nothing is confirmed. Negated prices give the bearish impulse.
    #[test]
    fn impulses_follow_price_and_percent_swing_sizes() {
        let mut closes = IMPULSE.to_vec();
        closes.push(122.0);
        for (mode, swing_move, impulse) in [
            (1, 6.0, true),
            (2, 5.2, true),
            (1, 7.0, false),
            (2, 5.5, false),
        ] {
            for sign in [1.0, -1.0] {
                let signed: Vec<_> = closes.iter().map(|c| sign * c).collect();
                let (bars, mut study) = run(&signed);
                study
                    .set_input("swing_mode", InputValue::Choice(mode))
                    .unwrap();
                study
                    .set_input("swing_move", InputValue::Number(swing_move))
                    .unwrap();
                study.calculate(&bars);
                super::super::assert_replay_matches_batch(&study, &bars);
                let row = format!("mode {mode} move {swing_move} sign {sign}");
                let (line, other) = if sign > 0.0 { (1, 2) } else { (2, 1) };
                assert_eq!(column(&study, other, 14), [None; 14], "{row}");
                if !impulse {
                    assert_eq!(column(&study, line, 14), [None; 14], "{row}");
                    continue;
                }
                let mut confirmed = [None; 14];
                confirmed[13] = Some(sign * 130.0);
                assert_eq!(column(&study, line, 14), confirmed, "{row}");
                let mut developing = [None; 14];
                developing[9] = Some(sign * 125.0);
                developing[10] = Some(sign * 115.0);
                assert_eq!(column(&study, 0, 14), developing, "{row}");
                assert_eq!(study.plot_offset(0), 0);
                let rows = &study.table().unwrap().rows;
                let expected: Vec<_> = [100.0, 110.0, 104.0, 125.0, 115.0, 130.0]
                    .iter()
                    .enumerate()
                    .map(|(n, p)| format!("{n}:{:.2}", sign * p))
                    .collect();
                assert_eq!(rows.len(), 1, "{row}");
                assert_eq!(rows[0][2], StudyCell::Text(expected.join(" ")), "{row}");
                assert_eq!(rows[0][1], StudyCell::Text("1970-01-14 00:00".into()));
            }
        }
        // Wide bars cannot confirm their own extremes or reverse a leg they extend. Equal
        // extremes keep the first bar, zero thresholds still need a move, and skips keep indices.
        type SwingRow = (
            &'static [(f64, f64, f64)],
            f64,
            &'static [(usize, f64, bool)],
        );
        let rows: [SwingRow; 6] = [
            (&[(20.0, 0.0, 10.0)], 0.0, &[]),
            (&[(10.0, 5.0, 8.0), (20.0, 0.0, 10.0)], 6.0, &[]),
            (
                &[
                    (10.0, 10.0, 10.0),
                    (10.0, 10.0, 10.0),
                    (12.0, 12.0, 12.0),
                    (11.0, 11.0, 11.0),
                ],
                1.0,
                &[(0, 10.0, false), (2, 12.0, true)],
            ),
            (
                &[(10.0, 10.0, 10.0), (12.0, 12.0, 12.0), (15.0, 5.0, 10.0)],
                1.0,
                &[(0, 10.0, false)],
            ),
            (
                &[
                    (f64::NAN, 10.0, 10.0),
                    (10.0, 10.0, 10.0),
                    (12.0, 12.0, 12.0),
                    (11.0, 11.0, 11.0),
                ],
                1.0,
                &[(1, 10.0, false), (2, 12.0, true)],
            ),
            (&[(10.0, 10.0, 10.0), (10.0, 10.0, 10.0)], 0.0, &[]),
        ];
        for (row, (hlc, threshold, expected)) in rows.into_iter().enumerate() {
            let mut study = ElliottWave::default();
            study
                .set_input("swing_mode", InputValue::Choice(1))
                .unwrap();
            study
                .set_input("swing_move", InputValue::Number(threshold))
                .unwrap();
            let bars = super::super::hlc_bars(hlc);
            study.calculate(&bars);
            let pivots: Vec<_> = study
                .stepper
                .state()
                .swings
                .pivots
                .iter()
                .map(|pivot| (pivot.index, pivot.price, pivot.high))
                .collect();
            assert_eq!(pivots, expected, "swing row {row}");
            super::super::assert_replay_matches_batch(&study, &bars);
        }
    }

    /// Hand geometry with one bar on each side. The confirmed impulse is numbered 0 to 5 at its
    /// pivots; up to bar 10 the same anchors to wave 4 are developing. Projected wave 5 is
    /// 115 + min(10, 21) = 125, two bars (as wave 1 took) after bar 9. Up to bar 8, wave 4 is
    /// 125 - 0.382 * 21 = 116.978, two bars (as wave 2 took) after bar 7. In the last row the
    /// 38.2% retracement of the 100 to 112 wave would pass the end of wave 1, so it stops at 110.
    #[cfg(feature = "drawings")]
    #[test]
    fn display_inputs_change_only_what_they_name() {
        use crate::drawings::DrawingToolType::{ElliottImpulse, TrendLine};
        type Labels = Vec<(usize, f64, &'static str, usize)>;
        type Drawings = Vec<(crate::drawings::DrawingToolType, Vec<(f32, f64)>, usize)>;
        type Row = (&'static [f64], [bool; 3], Labels, Drawings);
        let anchors = [
            (1, 100.0),
            (3, 110.0),
            (5, 104.0),
            (7, 125.0),
            (9, 115.0),
            (11, 130.0),
        ];
        let numbered = |count: usize, plot: usize| -> Labels {
            anchors[..count]
                .iter()
                .zip(["0", "1", "2", "3", "4", "5"])
                .map(|(&(bar, price), text)| (bar, price, text, plot))
                .collect()
        };
        let legs = |count: usize, plot: usize| -> Drawings {
            anchors[..count]
                .windows(2)
                .map(|leg| {
                    let points = leg
                        .iter()
                        .map(|&(bar, price)| (bar as f32, price))
                        .collect();
                    (TrendLine, points, plot)
                })
                .collect()
        };
        let with = |mut list: Drawings, more: Drawings| {
            list.extend(more);
            list
        };
        let with_label = |mut list: Labels, more| {
            list.push(more);
            list
        };
        let leg = |from: (f32, f64), to: (f32, f64)| (TrendLine, vec![from, to], 0);
        let second = [
            (11, 130.0),
            (13, 120.0),
            (15, 126.0),
            (17, 110.0),
            (19, 118.0),
            (21, 105.0),
        ];
        let mut joined_labels = numbered(6, 1);
        joined_labels.extend(
            second[1..]
                .iter()
                .zip(["1", "2", "3", "4", "5"])
                .map(|(&(bar, price), text)| (bar, price, text, 2)),
        );
        let mut joined_legs = legs(6, 1);
        joined_legs.extend(second.windows(2).map(|leg| {
            (
                TrendLine,
                leg.iter()
                    .map(|&(bar, price)| (bar as f32, price))
                    .collect(),
                2,
            )
        }));
        let rows: [Row; 9] = [
            (&IMPULSE, [true, false, false], numbered(6, 1), legs(6, 1)),
            (&IMPULSE, [false, false, false], numbered(6, 1), legs(6, 1)),
            (
                &IMPULSE[..11],
                [true, false, false],
                numbered(5, 0),
                legs(5, 0),
            ),
            (&IMPULSE[..11], [false, true, true], vec![], vec![]),
            (
                &IMPULSE[..11],
                [true, true, false],
                with_label(numbered(5, 0), (11, 125.0, "5", 0)),
                with(legs(5, 0), vec![leg((9.0, 115.0), (11.0, 125.0))]),
            ),
            (
                &IMPULSE[..11],
                [true, true, true],
                vec![(11, 125.0, "5", 0)],
                vec![
                    (
                        ElliottImpulse,
                        anchors[..5].iter().map(|&(b, p)| (b as f32, p)).collect(),
                        0,
                    ),
                    leg((9.0, 115.0), (11.0, 125.0)),
                ],
            ),
            (
                &IMPULSE[..9],
                [true, true, false],
                with_label(numbered(4, 0), (9, 116.978, "4", 0)),
                with(legs(4, 0), vec![leg((7.0, 125.0), (9.0, 116.978))]),
            ),
            (
                &[105.0, 95.0, 105.0, 110.0, 105.0, 100.0, 106.0, 112.0, 111.0],
                [true, true, false],
                vec![
                    (1, 95.0, "0", 0),
                    (3, 110.0, "1", 0),
                    (5, 100.0, "2", 0),
                    (7, 112.0, "3", 0),
                    (9, 110.0, "4", 0),
                ],
                vec![
                    leg((1.0, 95.0), (3.0, 110.0)),
                    leg((3.0, 110.0), (5.0, 100.0)),
                    leg((5.0, 100.0), (7.0, 112.0)),
                    leg((7.0, 112.0), (9.0, 110.0)),
                ],
            ),
            // A bearish impulse starts at the bullish impulse's last anchor. That point keeps
            // the older number 5, rather than painting a second number 0 over it.
            (
                &[
                    105.0, 100.0, 105.0, 110.0, 107.0, 104.0, 115.0, 125.0, 120.0, 115.0, 122.0,
                    130.0, 128.0, 120.0, 123.0, 126.0, 120.0, 110.0, 114.0, 118.0, 110.0, 105.0,
                    108.0,
                ],
                [true, false, false],
                joined_labels,
                joined_legs,
            ),
        ];
        for (row, (closes, [developing, projections, drawings], labels, expected)) in
            rows.into_iter().enumerate()
        {
            let (bars, mut study) = run(closes);
            for (id, on) in [
                ("show_developing", developing),
                ("projections", projections),
                ("as_drawings", drawings),
            ] {
                study.set_input(id, InputValue::Toggle(on)).unwrap();
            }
            study.calculate(&bars);
            let saved = crate::studies::StudyTemplate::from_study(&study);
            let json = serde_json::to_string(&saved).unwrap();
            let saved: crate::studies::StudyTemplate = serde_json::from_str(&json).unwrap();
            let mut restored = saved.create().unwrap();
            restored.calculate(&bars);
            assert_eq!(
                restored.labels(),
                study.labels(),
                "row {row} restored labels"
            );
            assert_eq!(
                serde_json::to_value(restored.drawings()).unwrap(),
                serde_json::to_value(study.drawings()).unwrap(),
                "row {row} restored legs"
            );
            let mut streamed = study.clone();
            streamed.calculate(&[]);
            for end in 1..=bars.len() {
                streamed.update(&bars[..end], end - 1);
                streamed.update(&bars[..end], end - 1);
            }
            assert_eq!(streamed.labels(), study.labels(), "row {row} replay labels");
            assert_eq!(
                serde_json::to_value(streamed.drawings()).unwrap(),
                serde_json::to_value(study.drawings()).unwrap(),
                "row {row} replay legs"
            );
            let round = |price: f64| (price * 1e9).round() / 1e9;
            let painted: Labels = study
                .labels()
                .iter()
                .map(|label| {
                    let text = ["0", "1", "2", "3", "4", "5"]
                        .into_iter()
                        .find(|text| *text == label.text)
                        .unwrap();
                    (label.bar, round(label.price), text, label.plot)
                })
                .collect();
            assert_eq!(painted, labels, "row {row}");
            let drawn: Drawings = study
                .drawings()
                .iter()
                .enumerate()
                .map(|(index, drawing)| {
                    let points = drawing
                        .chart_points
                        .iter()
                        .map(|point| (point.bar_idx, round(point.price)))
                        .collect();
                    (
                        drawing.tool_type,
                        points,
                        study.drawing_plot(index).unwrap(),
                    )
                })
                .collect();
            assert_eq!(drawn, expected, "row {row}");
            for (index, drawing) in study.drawings().iter().enumerate() {
                let plot = study.drawing_plot(index).unwrap();
                assert_eq!(drawing.color, study.colors[plot].to_srgba_unmultiplied());
                assert_eq!(drawing.stroke_width, 1.0);
                assert_eq!(
                    drawing.line_style,
                    if plot == 0 {
                        crate::model::LineStyle::Dashed
                    } else {
                        crate::model::LineStyle::Solid
                    },
                    "row {row} drawing {index}"
                );
            }
            let placements: Vec<_> = study.labels().iter().map(|l| l.placement).collect();
            let above = |high: bool| {
                if high {
                    LabelPlacement::Above
                } else {
                    LabelPlacement::Below
                }
            };
            // Lows are numbered below, highs above, and the projected anchor on its own side.
            let expected_placements: Vec<_> = match row {
                0 => [false, true, false, true, false, true].map(above).to_vec(),
                4 => [false, true, false, true, false, true].map(above).to_vec(),
                7 => [false, true, false, true, false].map(above).to_vec(),
                _ => placements.clone(),
            };
            assert_eq!(placements, expected_placements, "row {row}");
            if !developing {
                // The confirmed impulse keeps its wave number on bar 12.
                assert!(column(&study, 0, bars.len()).iter().all(Option::is_none));
                assert!(
                    column(&study, 3, 12.min(bars.len()))
                        .iter()
                        .all(Option::is_none)
                );
            }
            if row == 1 {
                let (_, shown) = run(closes);
                for line in [1, 2] {
                    assert_eq!(column(&study, line, 13), column(&shown, line, 13));
                }
            }
            if row == 3 {
                let rows = &study.table().unwrap().rows;
                assert_eq!(rows[0][0], StudyCell::Text("No impulse".into()));
            }
        }
    }

    /// The chart paints the five legs of a confirmed impulse end to end in the colour of the
    /// Confirmed Bullish Impulse plot, in light and dark themes, with each anchor numbered above
    /// a high and below a low.
    #[cfg(feature = "drawings")]
    #[test]
    fn confirmed_impulse_paints_labelled_legs_on_the_chart() {
        use crate::widget::test_frames::shapes;
        use egui::{Context, Shape};
        let bull = Color32::from_rgb(10, 200, 30);
        let (bars, _) = run(&IMPULSE);
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            let mut chart =
                crate::widget::Chart::new(crate::model::BarData::from_bars(bars.clone()));
            chart.set_theme(&ctx, &theme);
            let mut study = ElliottWave::new(1, 1, 5);
            study.set_colors(vec![palette::ACCENT, bull, palette::RED]);
            let id = chart.add_indicator(Box::new(study));
            shapes(&ctx, &mut chart, 0.0, Vec::new());
            let painted = shapes(&ctx, &mut chart, 0.1, Vec::new());
            let legs: Vec<[egui::Pos2; 2]> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, stroke } if stroke.color == bull => Some(*points),
                    _ => None,
                })
                .collect();
            assert_eq!(legs.len(), 5);
            let joints: Vec<_> = std::iter::once(legs[0][0])
                .chain(legs.iter().map(|leg| leg[1]))
                .collect();
            for (n, leg) in legs.iter().enumerate() {
                assert!(leg[0].distance(joints[n]) < 0.5 && leg[1].x > leg[0].x);
                // Rising legs alternate with falling ones, from a low.
                assert_eq!(leg[1].y < leg[0].y, n % 2 == 0, "leg {n}");
            }
            for (n, joint) in joints.iter().enumerate() {
                let label = painted.iter().find_map(|shape| match shape {
                    Shape::Text(text)
                        if text.fallback_color == bull && text.galley.text() == n.to_string() =>
                    {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                });
                let label = label.unwrap_or_else(|| panic!("no label {n}"));
                assert!((label.center().x - joint.x).abs() < 1.0, "label {n}");
                assert_eq!(label.max.y <= joint.y, n % 2 == 1, "label {n}");
            }
            let wrapped = chart.indicator_mut(id).unwrap();
            let mut settings = wrapped.settings();
            settings.plots[1].width = 3.0;
            wrapped.apply_settings(&[], settings.clone()).unwrap();
            let painted = shapes(&ctx, &mut chart, 0.2, Vec::new());
            let widths: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { stroke, .. } if stroke.color == bull => Some(stroke.width),
                    _ => None,
                })
                .collect();
            assert_eq!(widths, [3.0; 5]);
            settings.plots[1].visible = false;
            chart
                .indicator_mut(id)
                .unwrap()
                .apply_settings(&[], settings)
                .unwrap();
            let hidden = shapes(&ctx, &mut chart, 0.3, Vec::new());
            assert!(!hidden.iter().any(|shape| match shape {
                Shape::LineSegment { stroke, .. } => stroke.color == bull,
                Shape::Text(text) => text.fallback_color == bull,
                _ => false,
            }));
        }
    }
}
