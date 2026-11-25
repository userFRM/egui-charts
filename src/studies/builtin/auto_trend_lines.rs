//! Support and resistance lines through swing pivots, drawn to their breakout.

use super::ATR;
use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
use crate::model::{Bar, LineStyle};
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, PlotStyle, PlotType, ShapeSide, palette,
};
use egui::Color32;

/// The most recent bars with prices used to find lines.
const WINDOW: usize = 5000;
/// Wilder's average true range period for swings and touch bands.
const ATR_PERIOD: usize = 14;
/// Bars on each side of a pivot counted as a touch.
const TOUCH_PIVOT: usize = 3;
/// The fraction of the shorter base that overlapping lines must share.
const OVERLAP: f64 = 0.3;
/// The longest extension, as a multiple of the base's bar count.
const MAX_EXTENSION: usize = 2;

/// Which swing sizes lines are built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineSize {
    Small,
    Large,
    Both,
}

impl LineSize {
    /// The label study inputs show for this choice.
    fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Large => "Large",
            Self::Both => "Both",
        }
    }

    /// Bars on each side of a pivot and the smallest swing in average true ranges, small first.
    fn swings(self) -> &'static [(usize, f64)] {
        match self {
            Self::Small => &[(5, 2.0)],
            Self::Large => &[(25, 5.0)],
            Self::Both => &[(5, 2.0), (25, 5.0)],
        }
    }
}

/// Support and resistance lines drawn automatically through swing pivots of the last 5000
/// bars with prices, each up to its breakout.
///
/// Lines join consecutive lows (support) or consecutive highs (resistance) of a swing sequence.
/// Small swings join pivots with 5 bars on each side and a swing above 2 times the 14-bar
/// average true range at the swing's first pivot; large swings 25 bars and 5 times. Line size
/// chooses Small, Large or Both. A line crossed by a close between its pivots is dropped. Of
/// two lines of one kind that run within a touch band of each other, half the average true
/// range at their pivots wide, for more than 30% of the shorter base, the one with more
/// touches by 3-bar pivots is kept, then the longer, then the one whose second pivot is the
/// larger, then the steeper.
///
/// A kept line runs to its breakout, the bar of the last of Bars to breakout consecutive closes
/// beyond it, and one not broken runs on to the right, as long as that extension is at most
/// twice the line's base; otherwise only the base is drawn. Bars without a finite high, low
/// and close are skipped. Every update recalculates the lines.
///
/// Plot 0 styles the support lines and plot 1 the resistance lines; with Show pivots on, each
/// marks the two pivots of its lines, at their low or high, and holds those prices as values.
/// Generated lines belong to the study, not the editable drawing list; read them with
/// [`Indicator::drawings`].
///
/// ```
/// use egui_charts::studies::{AutoTrendLines, Indicator};
/// # let bars: Vec<egui_charts::model::Bar> = Vec::new();
/// let mut study = AutoTrendLines::new().with_bars_to_breakout(5).with_pivots(true);
/// study.calculate(&bars);
/// let lines = study.drawings();
/// # assert!(lines.is_empty());
/// ```
#[derive(Clone, Debug)]
pub struct AutoTrendLines {
    bars_to_breakout: usize,
    line_size: LineSize,
    show_pivots: bool,
    atr: ATR,
    drawings: Vec<Drawing>,
    /// The plot that styles each of `drawings`: 0 Support, 1 Resistance.
    drawing_plots: Vec<usize>,
    /// The bars whose values mark pivots, cleared before the next calculation.
    anchors: Vec<usize>,
    values: IndicatorValues,
    colors: [Color32; 2],
    visible: bool,
}

impl AutoTrendLines {
    /// Construct with 3 bars to breakout, small and large lines and no pivot marks.
    pub fn new() -> Self {
        Self {
            bars_to_breakout: 3,
            line_size: LineSize::Both,
            show_pivots: false,
            atr: ATR::new(ATR_PERIOD),
            drawings: Vec::new(),
            drawing_plots: Vec::new(),
            anchors: Vec::new(),
            values: IndicatorValues::new(),
            colors: [palette::BLUE, palette::RED],
            visible: true,
        }
    }

    /// Set the consecutive closes beyond a line that break it, at least 1.
    pub fn with_bars_to_breakout(mut self, bars: usize) -> Self {
        self.bars_to_breakout = bars.max(1);
        self
    }

    /// Mark the two pivots of every drawn line, and hold their prices as values.
    pub fn with_pivots(mut self, show: bool) -> Self {
        self.show_pivots = show;
        self
    }
}

/// Construct with the defaults of [`new`](Self::new).
impl Default for AutoTrendLines {
    fn default() -> Self {
        Self::new()
    }
}

/// The pivot size of position `k`, with the first of equal extremes winning, up to `cap`.
fn reach(values: &[f64], k: usize, cap: usize) -> usize {
    let left = values[..k]
        .iter()
        .rev()
        .take(cap)
        .take_while(|&&v| v < values[k])
        .count();
    let right = values[k + 1..]
        .iter()
        .take(cap)
        .take_while(|&&v| v <= values[k])
        .count();
    left.min(right)
}

#[derive(Clone, Copy, Debug)]
struct Pivot {
    k: usize,
    high: bool,
    price: f64,
    atr: f64,
}

/// Alternate confirmed extremes, measuring each reversal against its starting pivot's ATR.
fn swings(highs: &[f64], neg_lows: &[f64], atr: &[f64], size: usize, multiple: f64) -> Vec<Pivot> {
    let mut pivots: Vec<Pivot> = Vec::new();
    for k in 0..highs.len() {
        if !atr[k].is_finite() {
            continue;
        }
        let first_high = pivots.last().is_none_or(|last| !last.high);
        for high in [first_high, !first_high] {
            let values = if high { highs } else { neg_lows };
            if reach(values, k, size) != size {
                continue;
            }
            let pivot = Pivot {
                k,
                high,
                price: if high { highs[k] } else { -neg_lows[k] },
                atr: atr[k],
            };
            if let Some(last) = pivots.last_mut() {
                let movement = if high {
                    pivot.price - last.price
                } else {
                    last.price - pivot.price
                };
                if high == last.high {
                    if movement > 0.0 {
                        *last = pivot;
                    }
                    continue;
                }
                if movement <= multiple * last.atr {
                    continue;
                }
            }
            pivots.push(pivot);
        }
    }
    pivots
}

#[derive(Debug)]
struct Line {
    support: bool,
    a: Pivot,
    b: Pivot,
    end: usize,
    broken: bool,
    ray: bool,
    touches: usize,
    strength: usize,
}

impl Line {
    fn base(&self) -> usize {
        self.b.k - self.a.k
    }

    fn at(&self, k: usize) -> f64 {
        self.a.price + (self.b.price - self.a.price) * (k - self.a.k) as f64 / self.base() as f64
    }

    fn zone(&self) -> f64 {
        (self.a.atr + self.b.atr) / 4.0
    }

    fn length(&self) -> usize {
        self.end - self.a.k
    }

    fn slope(&self) -> f64 {
        (self.b.price - self.a.price).abs() / self.base() as f64
    }

    fn beyond(&self, k: usize, close: f64) -> bool {
        let price = self.at(k);
        price.is_finite()
            && if self.support {
                close < price
            } else {
                close > price
            }
    }

    /// Length of the shared base inside either line's touch band, measured continuously.
    fn overlaps(&self, other: &Self) -> bool {
        let start = self.a.k.max(other.a.k);
        let end = self.b.k.min(other.b.k);
        if self.support != other.support || end <= start {
            return false;
        }
        let ds = self.at(start) - other.at(start);
        let de = self.at(end) - other.at(end);
        let tolerance = self.zone().max(other.zone());
        let shared = (end - start) as f64;
        let length = if ds == de {
            if ds.abs() <= tolerance { shared } else { 0.0 }
        } else {
            let at = |v: f64| {
                (start as f64 + (v - ds) / (de - ds) * shared).clamp(start as f64, end as f64)
            };
            (at(tolerance) - at(-tolerance)).abs()
        };
        length > OVERLAP * self.base().min(other.base()) as f64
    }
}

impl Indicator for AutoTrendLines {
    crate::studies::inputs::study_inputs!(self;
        bars_to_breakout: length("Bars to breakout", 1, usize::MAX),
        line_size: choice("Line size", [LineSize::Small, LineSize::Large, LineSize::Both]),
        show_pivots: toggle("Show pivots")
    ; {});

    fn name(&self) -> &str {
        "Auto Trend Line"
    }

    fn desc(&self) -> &str {
        "Support and resistance lines through swing pivots, drawn to their breakout"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.values = IndicatorValues::missing(data.len());
        } else {
            for &bar in &self.anchors {
                if bar < self.values.len() {
                    self.values.set(bar, IndicatorValue::None);
                }
            }
            self.values.resize(data.len(), IndicatorValue::None);
        }
        self.anchors.clear();
        self.atr.update(data, from);
        let mut idx: Vec<_> = data
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, bar)| [bar.high, bar.low, bar.close].iter().all(|p| p.is_finite()))
            .take(WINDOW)
            .map(|(i, _)| i)
            .collect();
        idx.reverse();
        let highs: Vec<_> = idx.iter().map(|&i| data[i].high).collect();
        let neg_lows: Vec<_> = idx.iter().map(|&i| -data[i].low).collect();
        let closes: Vec<_> = idx.iter().map(|&i| data[i].close).collect();
        let atr: Vec<_> = idx
            .iter()
            .map(|&i| self.atr.values().value(0, i).unwrap_or(f64::NAN))
            .collect();
        let mut candidates = Vec::new();
        for &(size, multiple) in self.line_size.swings() {
            let pivots = swings(&highs, &neg_lows, &atr, size, multiple);
            for support in [true, false] {
                let points: Vec<_> = pivots.iter().filter(|p| p.high != support).collect();
                let extremes = if support { &neg_lows } else { &highs };
                for pair in points.windows(2) {
                    let (a, b) = (*pair[0], *pair[1]);
                    let mut line = Line {
                        support,
                        a,
                        b,
                        end: b.k,
                        broken: false,
                        ray: false,
                        touches: 0,
                        strength: reach(extremes, b.k, usize::MAX),
                    };
                    if (a.k + 1..b.k).any(|k| line.beyond(k, closes[k])) {
                        continue;
                    }
                    let limit = MAX_EXTENSION * line.base();
                    let mut run = 0;
                    for (k, &close) in closes
                        .iter()
                        .enumerate()
                        .take((b.k + limit + 1).min(idx.len()))
                        .skip(b.k + 1)
                    {
                        run = if line.beyond(k, close) { run + 1 } else { 0 };
                        if run == self.bars_to_breakout {
                            line.end = k;
                            line.broken = true;
                            break;
                        }
                    }
                    if !line.broken && idx.len() - 1 - b.k <= limit {
                        line.end = idx.len() - 1;
                        line.ray = true;
                    }
                    line.touches = (a.k..=line.end)
                        .filter(|&k| {
                            let price = line.at(k);
                            price.is_finite()
                                && reach(extremes, k, TOUCH_PIVOT) == TOUCH_PIVOT
                                && if support {
                                    -neg_lows[k] <= price + line.zone()
                                } else {
                                    highs[k] >= price - line.zone()
                                }
                        })
                        .count();
                    candidates.push(line);
                }
            }
        }
        candidates.sort_by(|a, b| {
            b.touches
                .cmp(&a.touches)
                .then_with(|| b.length().cmp(&a.length()))
                .then_with(|| b.strength.cmp(&a.strength))
                .then_with(|| b.slope().total_cmp(&a.slope()))
        });
        let mut kept: Vec<Line> = Vec::new();
        for line in candidates {
            if !kept.iter().any(|other| line.overlaps(other)) {
                kept.push(line);
            }
        }
        kept.sort_by_key(|line| (line.a.k, line.b.k, !line.support));
        self.drawings.clear();
        self.drawing_plots.clear();
        for line in kept {
            let plot = usize::from(!line.support);
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.completed = true;
            drawing.color = self.colors[plot].to_srgba_unmultiplied();
            drawing.chart_points = vec![
                ChartPoint::new(idx[line.a.k] as f32, line.a.price),
                if line.broken {
                    ChartPoint::new(idx[line.end] as f32, line.at(line.end))
                } else {
                    ChartPoint::new(idx[line.b.k] as f32, line.b.price)
                },
            ];
            drawing.extend_right = line.ray;
            self.drawings.push(drawing);
            self.drawing_plots.push(plot);
            if self.show_pivots {
                for pivot in [line.a, line.b] {
                    let bar = idx[pivot.k];
                    let mut row = [
                        self.values.value(0, bar).unwrap_or(f64::NAN),
                        self.values.value(1, bar).unwrap_or(f64::NAN),
                    ];
                    row[plot] = pivot.price;
                    self.values.set(bar, IndicatorValue::Multiple(row.to_vec()));
                    self.anchors.push(bar);
                }
            }
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Support".into(), "Resistance".into()]
    }

    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors[line.min(1)],
            width: 1.0,
            pattern: LineStyle::Solid,
            plot_type: if line == 0 {
                PlotType::TriangleUp
            } else {
                PlotType::TriangleDown
            },
            visible: true,
        }
    }

    fn plot_shape_side(&self, line: usize) -> ShapeSide {
        // Support lines join pivot lows, resistance lines pivot highs.
        if line == 0 {
            ShapeSide::Below
        } else {
            ShapeSide::Above
        }
    }

    fn drawings(&self) -> &[Drawing] {
        &self.drawings
    }

    fn drawings_use_plot_style(&self) -> bool {
        false
    }

    fn drawing_plot(&self, index: usize) -> Option<usize> {
        self.drawing_plots.get(index).copied()
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (slot, color) in self.colors.iter_mut().zip(colors) {
            *slot = color;
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

    // Its drawings sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{IndicatorFactory, InputValue};

    const F1: &[(usize, f64)] = &[
        (0, 100.0),
        (14, 100.0),
        (24, 110.0),
        (34, 100.0),
        (46, 112.0),
        (56, 102.0),
        (70, 116.0),
        (90, 96.0),
        (100, 106.0),
    ];
    const A: &[(usize, f64)] = &[
        (0, 120.0),
        (10, 130.0),
        (40, 100.0),
        (70, 130.0),
        (100, 100.5),
        (106, 106.5),
        (114, 99.6),
        (117, 99.6),
        (147, 129.6),
        (170, 107.0),
    ];
    const B: &[(usize, f64)] = &[
        (0, 120.0),
        (10, 130.0),
        (40, 100.0),
        (70, 130.0),
        (100, 100.5),
        (106, 106.5),
        (112, 101.2),
        (115, 103.7),
        (120, 99.6),
        (123, 99.6),
        (153, 129.6),
        (177, 106.0),
    ];
    /// The low at 64 replaces the flat low at 40, whose close of 92 at bar 41 lies below the
    /// support from 20 to 64 (93.27 there).
    const CROSSED: &[(usize, f64)] = &[
        (0, 110.0),
        (10, 110.0),
        (20, 100.0),
        (26, 106.0),
        (40, 92.0),
        (60, 92.0),
        (64, 88.0),
        (80, 104.0),
        (92, 92.0),
        (104, 100.0),
    ];

    /// Linear closes move at most 1 per bar, with high/low 1 away: every true range is 2,
    /// ATR14 is 2 from bar 13, the small/large swing thresholds are 4/10 and touch bands are 1.
    fn path(turns: &[(usize, f64)]) -> Vec<Bar> {
        (0..=turns.last().unwrap().0)
            .map(|bar| {
                let pair = turns.windows(2).find(|pair| bar <= pair[1].0).unwrap();
                let (a, b) = (pair[0], pair[1]);
                let close = a.1 + (b.1 - a.1) * (bar - a.0) as f64 / (b.0 - a.0) as f64;
                Bar::new(
                    chrono::DateTime::from_timestamp(bar as i64 * 60, 0).unwrap(),
                    close,
                    close + 1.0,
                    close - 1.0,
                    close,
                    1.0,
                )
            })
            .collect()
    }

    type ExpectedLine = (usize, (usize, f64), (usize, f64), bool);

    fn check_lines(
        study: &dyn Indicator,
        expected: &[ExpectedLine],
        label: &str,
        support_only: bool,
    ) {
        let drawings: Vec<_> = study
            .drawings()
            .iter()
            .enumerate()
            .filter(|&(i, _)| !support_only || study.drawing_plot(i) == Some(0))
            .collect();
        assert_eq!(drawings.len(), expected.len(), "{label}");
        for ((i, drawing), &(plot, a, b, ray)) in drawings.into_iter().zip(expected) {
            assert_eq!(study.drawing_plot(i), Some(plot), "{label} line {i}");
            assert_eq!(drawing.tool_type, DrawingToolType::TrendLine);
            assert!(drawing.completed && !drawing.extend_left);
            assert_eq!(drawing.extend_right, ray, "{label} line {i}");
            assert_eq!(drawing.chart_points.len(), 2);
            for (point, (bar, price)) in drawing.chart_points.iter().zip([a, b]) {
                assert_eq!(point.bar_idx, bar as f32, "{label} line {i}");
                assert!(
                    (point.price - price).abs() < 1e-9,
                    "{label} line {i}: {} != {price}",
                    point.price
                );
            }
        }
    }

    /// Catalogue inputs select both scales, ranked overlaps and exact breakout endpoints;
    /// equality at the extension limit and the first equal extreme retain their intended side.
    #[test]
    fn auto_trend_lines_follow_their_pivots_inputs_and_breakouts() {
        use InputValue::{Choice, Length, Toggle};
        let f1 = path(F1);
        let a = path(A);
        let b = path(B);
        let crossed = path(CROSSED);
        let mut turns = F1.to_vec();
        turns.push((5050, 106.0));
        let window = path(&turns);
        let normal = |end| {
            vec![
                (1, (24, 111.0), (46, 113.0), false),
                (
                    0,
                    (34, 99.0),
                    (end, 99.0 + 2.0 * (end - 34) as f64 / 22.0),
                    false,
                ),
                (1, (46, 113.0), (70, 117.0), true),
                (0, (56, 101.0), (90, 95.0), true),
            ]
        };
        let a_small = vec![
            (0, (40, 99.0), (116, 99.0 + 0.5 * 76.0 / 60.0), false),
            (1, (70, 131.0), (120, 131.0 - 23.5 * 50.0 / 36.0), false),
            (0, (100, 99.5), (114, 98.6), false),
            (1, (106, 107.5), (147, 130.6), true),
        ];
        let mut no_breakout = normal(85);
        no_breakout[1] = (0, (34, 99.0), (56, 101.0), true);
        let rows = [
            (false, "F1 default", &f1, vec![], normal(85)),
            (
                false,
                "F1 five closes",
                &f1,
                vec![("bars_to_breakout", Length(5))],
                normal(87),
            ),
            (
                false,
                "F1 extension limit",
                &f1,
                vec![("bars_to_breakout", Length(30))],
                no_breakout,
            ),
            (
                false,
                "F1 small",
                &f1,
                vec![("line_size", Choice(0))],
                normal(85),
            ),
            (
                false,
                "F1 large",
                &f1,
                vec![("line_size", Choice(1))],
                vec![],
            ),
            (
                false,
                "A both ranks length",
                &a,
                vec![],
                vec![(0, (40, 99.0), (114, 98.6), true), a_small[1], a_small[3]],
            ),
            (
                false,
                "A small",
                &a,
                vec![("line_size", Choice(0))],
                a_small,
            ),
            (
                false,
                "A large",
                &a,
                vec![("line_size", Choice(1))],
                vec![(0, (40, 99.0), (114, 98.6), true)],
            ),
            (
                true,
                "B both ranks touches",
                &b,
                vec![],
                vec![
                    (0, (40, 99.0), (122, 99.0 + 0.5 * 82.0 / 60.0), false),
                    (0, (100, 99.5), (121, 99.5 + 0.7 * 21.0 / 12.0), false),
                    (0, (112, 100.2), (120, 98.6), false),
                ],
            ),
            (
                true,
                "B large",
                &b,
                vec![("line_size", Choice(1))],
                vec![(0, (40, 99.0), (120, 98.6), true)],
            ),
            (
                false,
                "crossed base",
                &crossed,
                vec![],
                vec![
                    (1, (26, 107.0), (80, 105.0), true),
                    (0, (64, 87.0), (92, 91.0), true),
                ],
            ),
            (
                false,
                "W last 5000",
                &window,
                vec![],
                vec![
                    (0, (56, 101.0), (90, 95.0), false),
                    (1, (70, 117.0), (106, 105.0), false),
                ],
            ),
        ];
        for (support_only, label, bars, inputs, expected) in rows {
            let mut study = IndicatorFactory::new().create("Auto Trend Line").unwrap();
            for (id, value) in inputs {
                study.set_input(id, value).unwrap();
            }
            study.calculate(bars);
            check_lines(study.as_ref(), &expected, label, support_only);
            assert_eq!(study.values().len(), bars.len());
            assert!(
                study
                    .values()
                    .to_rows()
                    .iter()
                    .all(|v| matches!(v, IndicatorValue::None))
            );
        }
        let mut study = IndicatorFactory::new().create("Auto Trend Line").unwrap();
        for (id, default) in [
            ("bars_to_breakout", Length(3)),
            ("line_size", Choice(2)),
            ("show_pivots", Toggle(false)),
        ] {
            let inputs = study.inputs();
            let input = inputs.iter().find(|input| input.id == id).unwrap();
            assert_eq!(input.value, default);
            assert_eq!(input.default, default);
        }
        study.set_input("show_pivots", Toggle(true)).unwrap();
        study.calculate(&f1);
        check_lines(study.as_ref(), &normal(85), "F1 pivots", false);
        for bar in 0..f1.len() {
            for (plot, points) in [
                (0, &[(34, 99.0), (56, 101.0), (90, 95.0)][..]),
                (1, &[(24, 111.0), (46, 113.0), (70, 117.0)][..]),
            ] {
                let expected = points.iter().find(|&&(k, _)| k == bar).map(|&(_, p)| p);
                let actual = study.values().value(plot, bar).filter(|v| v.is_finite());
                assert_eq!(actual, expected, "plot {plot} bar {bar}");
            }
        }
    }

    /// Appends and revised live closes add and withdraw breakouts, matching a fresh study's
    /// drawings and pivot columns after each change.
    #[test]
    fn auto_trend_lines_follow_live_bars() {
        let bars = path(F1);
        let mut revised = bars[..86].to_vec();
        revised[85] = Bar::new(bars[85].time, 104.0, 105.0, 103.0, 104.0, 1.0);
        // Revising the old low moves its pivot from 56 to 55, which must clear the old mark.
        let mut moved = bars.clone();
        moved[56] = Bar::new(bars[56].time, 104.0, 105.0, 103.0, 104.0, 1.0);
        let mut study = AutoTrendLines::new().with_pivots(true);
        study.calculate(&bars[..85]);
        for (data, from, end, price, ray) in [
            (&bars[..85], 0, 56, 101.0, true),
            (&bars[..86], 85, 85, 99.0 + 2.0 * 51.0 / 22.0, false),
            (&revised[..], 85, 56, 101.0, true),
            (&bars[..86], 85, 85, 99.0 + 2.0 * 51.0 / 22.0, false),
            (&bars[..], 86, 85, 99.0 + 2.0 * 51.0 / 22.0, false),
            (&moved[..], 56, 83, 106.0, false),
            (&bars[..], 56, 85, 99.0 + 2.0 * 51.0 / 22.0, false),
        ] {
            study.update(data, from);
            let mut fresh = AutoTrendLines::new().with_pivots(true);
            fresh.calculate(data);
            assert_eq!(
                format!("{:?}", study.drawings()),
                format!("{:?}", fresh.drawings())
            );
            assert_eq!(
                format!("{:?}", study.values().to_rows()),
                format!("{:?}", fresh.values().to_rows())
            );
            let line = study
                .drawings()
                .iter()
                .find(|line| line.chart_points[0].bar_idx == 34.0)
                .unwrap();
            assert_eq!(line.extend_right, ray);
            assert_eq!(line.chart_points[1].bar_idx, end as f32);
            assert!((line.chart_points[1].price - price).abs() < 1e-9);
        }
    }
}
