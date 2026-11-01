//! Chart pattern rendering implementations
//!
//! Includes: XABCD, Cypher, Head & Shoulders, ABCD, Triangle, and Three Drives patterns.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::painting::dashed_line::draw_dotted_segments;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, Galley, Pos2, Rect, Stroke, vec2};
use std::sync::Arc;

impl Drawing {
    /// Render XABCD Harmonic Pattern - Gartley, Bat, Butterfly, Crab with Fibonacci ratios
    pub(crate) fn render_xabcd_pattern(&self, painter: &egui::Painter) {
        if let &[x, a, b, c, d, ..] = self.points.as_slice() {
            self.fill_regions(painter, [&[x, a, b][..], &[b, c, d][..]]);
        }
        self.draw_legs_and_anchors(painter);
        let colors = self.label_colors();
        self.draw_point_labels(painter, &["X", "A", "B", "C", "D"], colors);
        self.draw_ratio_chords(painter, 3, colors);
    }

    /// Render Cypher Pattern - XABCD with Cypher-specific ratios
    pub(crate) fn render_cypher_pattern(&self, painter: &egui::Painter) {
        self.render_xabcd_pattern(painter);
    }

    /// Render Head and Shoulders Pattern - With its neckline
    pub(crate) fn render_head_and_shoulders(&self, painter: &egui::Painter) {
        let outline = head_and_shoulders_outline(&self.points);
        if let Some((_, regions)) = &outline {
            self.fill_regions(painter, regions.iter().map(|r| &r[..]));
        }
        self.draw_legs_and_anchors(painter);
        self.draw_point_labels(
            painter,
            &["", "Left Shoulder", "", "Head", "", "Right Shoulder", ""],
            self.label_colors(),
        );
        if let Some((neckline, _)) = outline {
            self.draw_construction_lines(painter, &[neckline]);
        }
    }

    /// Render ABCD Pattern - With its two ratios
    pub(crate) fn render_abcd_pattern(&self, painter: &egui::Painter) {
        self.draw_legs_and_anchors(painter);
        let colors = self.label_colors();
        self.draw_point_labels(painter, &["A", "B", "C", "D"], colors);
        self.draw_ratio_chords(painter, 3, colors);
    }

    /// Render Triangle Pattern - With its boundaries run on to the apex
    pub(crate) fn render_triangle_pattern(&self, painter: &egui::Painter) {
        // Four anchors zigzag between the two boundaries; three close on their first boundary.
        let boundaries = triangle_boundaries(&self.points, painter.clip_rect().right());
        if let &[a, b, c] = self.points.as_slice() {
            self.fill_regions(painter, [&[a, b, c][..]]);
        } else if let Some((_, Some(fill))) = &boundaries {
            self.fill_regions(painter, [&fill[..]]);
        }
        self.draw_legs_and_anchors(painter);
        if let &[a, _, c] = self.points.as_slice() {
            self.draw_construction_lines(painter, &[[a, c]]);
        } else if let Some((lines, _)) = &boundaries {
            self.draw_construction_lines(painter, lines);
        }
        self.draw_point_labels(painter, &["A", "B", "C", "D"], self.label_colors());
    }

    /// Render Three Drives Pattern - With each drive's ratio to the correction before it
    pub(crate) fn render_three_drives_pattern(&self, painter: &egui::Painter) {
        self.draw_legs_and_anchors(painter);
        self.draw_ratio_chords(painter, 2, self.label_colors());
    }

    /// Tints each region with the drawing's fill. Regions under half a square pixel, which would
    /// paint only their anti-aliased edge, and regions with a non-finite corner are skipped.
    fn fill_regions<'a>(
        &self,
        painter: &egui::Painter,
        regions: impl IntoIterator<Item = &'a [Pos2]>,
    ) {
        let Some(fill) = self.fill_color32() else {
            return;
        };
        for region in regions {
            let doubled: f32 = (0..region.len())
                .map(|i| {
                    let (a, b) = (region[i], region[(i + 1) % region.len()]);
                    a.x * b.y - b.x * a.y
                })
                .sum();
            let area = 0.5 * doubled.abs();
            if area.is_finite() && area >= 0.5 {
                painter.add(egui::epaint::PathShape::convex_polygon(
                    region.to_vec(),
                    fill,
                    Stroke::NONE,
                ));
            }
        }
    }

    /// The legs in anchor order, painted before anything else so the style pass joins them into
    /// one path, then the placement anchors.
    fn draw_legs_and_anchors(&self, painter: &egui::Painter) {
        for leg in self.points.windows(2) {
            painter.line_segment([leg[0], leg[1]], self.stroke());
        }
        if !self.completed {
            for &point in &self.points {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, self.color32());
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }
        }
    }

    /// The neckline or the boundaries: dotted at the drawing's width, apart from the solid legs.
    fn draw_construction_lines(&self, painter: &egui::Painter, lines: &[[Pos2; 2]]) {
        draw_dotted_segments(
            painter,
            lines,
            self.stroke(),
            dot_spacing(self.stroke_width),
        );
    }

    /// The fill of the pattern's label boxes, the line colour, and the colour of their text: white,
    /// or black where white reads at less than 3:1 on the opaque line colour.
    fn label_colors(&self) -> (Color32, Color32) {
        let [r, g, b, _] = self.color;
        let text = if crate::theme::contrast(Color32::WHITE, Color32::from_rgb(r, g, b)) >= 3.0 {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        (self.color32(), text)
    }

    /// Labels the placed points, on the outer side of each; an empty name leaves a point bare.
    fn draw_point_labels(
        &self,
        painter: &egui::Painter,
        names: &[&str],
        colors: (Color32, Color32),
    ) {
        let gap = vec2(0.0, DESIGN_TOKENS.rounding.lg + DESIGN_TOKENS.spacing.xs);
        for (i, (&point, name)) in self.points.iter().zip(names).enumerate() {
            if name.is_empty() {
                continue;
            }
            let galley =
                painter.layout_no_wrap((*name).to_owned(), self.annotation_font(), colors.1);
            let rect = if label_above(&self.points, i) {
                Align2::CENTER_BOTTOM.anchor_size(point - gap, label_box_size(&galley))
            } else {
                Align2::CENTER_TOP.anchor_size(point + gap, label_box_size(&galley))
            };
            draw_label_box(painter, rect, galley, colors);
        }
    }

    /// Draws the placed ratio chords as one dotted mesh, each ratio on its chord, clear of the
    /// anchors where the chord leaves room.
    fn draw_ratio_chords(
        &self,
        painter: &egui::Painter,
        decimals: usize,
        colors: (Color32, Color32),
    ) {
        let chords: Vec<([Pos2; 2], Option<f64>)> = self
            .ratio_chords()
            .into_iter()
            .filter_map(|([from, to], ratio)| {
                Some(([*self.points.get(from)?, *self.points.get(to)?], ratio))
            })
            .collect();
        let lines: Vec<[Pos2; 2]> = chords.iter().map(|(line, _)| *line).collect();
        draw_dotted_segments(
            painter,
            &lines,
            Stroke::new(stroke::HAIRLINE, colors.0),
            dot_spacing(stroke::HAIRLINE),
        );
        for ([a, b], ratio) in chords {
            let Some(label) = ratio.and_then(|ratio| ratio_text(ratio, decimals)) else {
                continue;
            };
            let galley = painter.layout_no_wrap(label, self.annotation_font(), colors.1);
            let size = label_box_size(&galley);
            let at = |t| Align2::CENTER_CENTER.anchor_size(a.lerp(b, t), size);
            let rect = [0.5, 0.4, 0.6, 0.3, 0.7]
                .into_iter()
                .map(at)
                .find(|r| {
                    !self
                        .points
                        .iter()
                        .any(|p| r.intersects(Rect::from_center_size(*p, vec2(24.0, 48.0))))
                })
                .unwrap_or_else(|| at(0.5));
            draw_label_box(painter, rect, galley, colors);
        }
    }
}

/// The padding between a label box and its text.
fn label_padding() -> egui::Vec2 {
    vec2(DESIGN_TOKENS.spacing.sm, DESIGN_TOKENS.spacing.xs)
}

fn label_box_size(galley: &Galley) -> egui::Vec2 {
    galley.size() + 2.0 * label_padding()
}

/// Paints a label: its box at `rect`, of [`label_box_size`], and its text inside.
fn draw_label_box(
    painter: &egui::Painter,
    rect: Rect,
    galley: Arc<Galley>,
    (fill, text): (Color32, Color32),
) {
    painter.rect_filled(rect, DESIGN_TOKENS.rounding.sm, fill);
    painter.galley(rect.min + label_padding(), galley, text);
}

/// Whether the label of point `i` goes above it: at a high, a point no lower than the mean of
/// its neighbours, and at a lone point.
fn label_above(points: &[Pos2], i: usize) -> bool {
    let before = i.checked_sub(1).map(|n| points[n].y);
    let mean = match (before, points.get(i + 1).map(|p| p.y)) {
        (Some(a), Some(b)) => (a + b) / 2.0,
        (Some(y), None) | (None, Some(y)) => y,
        (None, None) => return true,
    };
    points[i].y <= mean
}

/// A ratio rounded to `decimals` places without trailing zeros, or `None` when it is not finite.
fn ratio_text(ratio: f64, decimals: usize) -> Option<String> {
    ratio.is_finite().then(|| {
        let text = format!("{ratio:.decimals$}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    })
}

/// Distance between the dots of a dotted line of `width`: two and a half widths, at least 4 px.
fn dot_spacing(width: f32) -> f32 {
    (2.5 * width).max(4.0)
}

/// The two boundaries of a triangle pattern, and the polygon they enclose if any.
type TriangleBoundaries = ([[Pos2; 2]; 2], Option<Vec<Pos2>>);

/// Triangle pattern boundaries from the zigzag anchors (first peak, first trough, second peak,
/// second trough): the line through each pair of same-side anchors, from its first anchor to where
/// the two meet, and the triangle they enclose with the first zigzag leg as a convex polygon. The
/// boundaries and the fill end at `right` when they meet beyond it. Boundaries that are parallel,
/// diverge or meet before the first anchor enclose nothing: they end at the last anchor and have no
/// fill. `None` until four anchors are placed, or while a boundary is vertical.
pub(super) fn triangle_boundaries(points: &[Pos2], right: f32) -> Option<TriangleBoundaries> {
    let &[a0, b0, a1, b1, ..] = points else {
        return None;
    };
    if a1.x == a0.x || b1.x == b0.x {
        return None;
    }
    let (slope_a, slope_b) = ((a1.y - a0.y) / (a1.x - a0.x), (b1.y - b0.y) / (b1.x - b0.x));
    let y_a = |x: f32| a0.y + slope_a * (x - a0.x);
    let y_b = |x: f32| b0.y + slope_b * (x - b0.x);
    let left = a0.x.min(b0.x);
    let last = a1.x.max(b1.x);
    let apex_x = (b0.y - a0.y + slope_a * a0.x - slope_b * b0.x) / (slope_a - slope_b);
    let apex_x = (apex_x.is_finite() && apex_x > left).then_some(apex_x);
    let end = apex_x.map_or(last, |x| x.min(last.max(right)));
    let fill = apex_x.map(|apex_x| {
        let mut fill = vec![a0, Pos2::new(end, y_a(end))];
        if apex_x != end {
            fill.push(Pos2::new(end, y_b(end)));
        }
        fill.push(b0);
        fill
    });
    Some((
        [
            [a0, Pos2::new(end, y_a(end))],
            [b0, Pos2::new(end, y_b(end))],
        ],
        fill,
    ))
}

/// The head and shoulders' neckline and the regions the pattern encloses, from its anchors
/// (start, left shoulder, left trough, head, right trough, right shoulder, end).
///
/// The neckline runs through the two troughs and on to where it meets the first and the last
/// leg. A shoulder encloses a region, and the neckline reaches its outer leg, only while the
/// shoulder lies strictly on the head's side of the neckline and the outer anchor does not;
/// otherwise the neckline ends at that shoulder's trough. The head's region is the triangle of
/// the troughs and the head, unless the head lies on the neckline, which leaves no region.
/// `None` until both troughs are placed, while they coincide, or while a trough or the head is
/// not finite.
pub(super) fn head_and_shoulders_outline(points: &[Pos2]) -> Option<([Pos2; 2], Vec<[Pos2; 3]>)> {
    let &[p0, p1, left, head, right, ..] = points else {
        return None;
    };
    if !(left.is_finite() && head.is_finite() && right.is_finite()) || left == right {
        return None;
    }
    let along = right - left;
    let side = |p: Pos2| along.x * (p.y - left.y) - along.y * (p.x - left.x);
    let h = side(head);
    if h == 0.0 {
        return Some(([left, right], Vec::new()));
    }
    // Where the leg from a shoulder to its outer anchor meets the neckline, while the shoulder
    // lies on the head's side and the outer anchor does not.
    let crossing = |shoulder: Pos2, outer: Pos2| {
        let (s, o) = (side(shoulder), side(outer));
        (s * h > 0.0 && o * h <= 0.0)
            .then(|| shoulder + (outer - shoulder) * (s / (s - o)))
            .filter(|p| p.is_finite())
            .map(|p| (shoulder, p))
    };
    let left_end = crossing(p1, p0);
    let right_end = points.get(5..7).and_then(|leg| crossing(leg[0], leg[1]));

    let mut regions = Vec::with_capacity(3);
    regions.extend(left_end.map(|(shoulder, p)| [p, shoulder, left]));
    regions.push([left, head, right]);
    regions.extend(right_end.map(|(shoulder, p)| [right, shoulder, p]));

    let reach = |p: Pos2| (p - left).dot(along);
    let (mut start, mut end) = (left, right);
    for (_, p) in [left_end, right_end].into_iter().flatten() {
        if reach(p) < reach(start) {
            start = p;
        }
        if reach(p) > reach(end) {
            end = p;
        }
    }
    Some(([start, end], regions))
}

impl Drawing {
    /// The chords a pattern labels with a price ratio: the two anchors each joins, and the ratio,
    /// `None` where its base leg is flat or an anchor has no chart coordinates.
    fn ratio_chords(&self) -> Vec<([usize; 2], Option<f64>)> {
        let ratio = |leg, base| self.leg_ratio(leg, base);
        match self.tool_type {
            // AB/XA, BC/AB, CD/BC, AD/XA
            DrawingToolType::XABCDPattern => vec![
                ([0, 2], ratio([1, 2], [0, 1])),
                ([1, 3], ratio([2, 3], [1, 2])),
                ([2, 4], ratio([3, 4], [2, 3])),
                ([0, 4], ratio([1, 4], [0, 1])),
            ],
            // AB/XA, XC/XA, CD/BC, CD/XC
            DrawingToolType::CypherPattern => vec![
                ([0, 2], ratio([1, 2], [0, 1])),
                ([1, 3], ratio([0, 3], [0, 1])),
                ([2, 4], ratio([3, 4], [2, 3])),
                ([0, 4], ratio([3, 4], [0, 3])),
            ],
            // BC/AB, CD/BC
            DrawingToolType::ABCDPattern => vec![
                ([0, 2], ratio([1, 2], [0, 1])),
                ([1, 3], ratio([2, 3], [1, 2])),
            ],
            // Drive 2 over correction 1, drive 3 over correction 2
            DrawingToolType::ThreeDrivesPattern => vec![
                ([1, 3], ratio([2, 3], [1, 2])),
                ([3, 5], ratio([4, 5], [3, 4])),
            ],
            _ => Vec::new(),
        }
    }

    /// Price height of the leg between two anchors over that of a base leg; `None` when an anchor
    /// has no chart coordinates or the base leg is flat.
    fn leg_ratio(&self, leg: [usize; 2], base: [usize; 2]) -> Option<f64> {
        let height = |[from, to]: [usize; 2]| {
            Some((self.chart_points.get(to)?.price - self.chart_points.get(from)?.price).abs())
        };
        let base = height(base)?;
        if base == 0.0 {
            return None;
        }
        Some(height(leg)? / base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::{ChartPoint, DrawingToolType};
    use crate::drawings::rendering::ChartView;
    use egui::Shape;

    /// A pattern at the given prices, on a screen where y is 1000 / price, a non-linear scale on
    /// which screen ratios differ from price ratios.
    fn pattern(tool: DrawingToolType, prices: &[f64]) -> Drawing {
        let mut drawing = Drawing::new(0, tool);
        for (i, &price) in prices.iter().enumerate() {
            let x = 10.0 * i as f32;
            drawing.chart_points.push(ChartPoint::new(x, price));
            drawing.points.push(Pos2::new(x, (1000.0 / price) as f32));
        }
        drawing
    }

    fn paint(drawing: &Drawing) -> Vec<Shape> {
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            drawing.render_original(
                ui.painter(),
                pane,
                &ChartView {
                    price_formatter: None,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|x| x,
                    price_to_y: &|p| (1000.0 / p) as f32,
                    y_to_price: &|y| 1000.0 / f64::from(y),
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                },
            );
        });
        let shapes = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    /// The text shapes among painted shapes, by their text.
    fn texts(shapes: &[Shape]) -> Vec<(String, &egui::epaint::TextShape)> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(t) => Some((t.galley.job.text.clone(), t)),
                _ => None,
            })
            .collect()
    }

    /// Each tool paints exactly its point and ratio labels, on a box of the line colour with
    /// legible text, on the outer side of each point. Hand values: 99.9792 / 77.2 = 1.29507 and
    /// 96.8 / 60 = 1.6133.
    #[test]
    fn pattern_labels_name_points_and_ratios_only() {
        use DrawingToolType::{
            ABCDPattern, CypherPattern, HeadAndShoulders, ThreeDrivesPattern, TrianglePattern,
            XABCDPattern,
        };
        let cases: [(DrawingToolType, &[f64], &[&str]); 6] = [
            (
                XABCDPattern,
                &[100.0, 200.0, 138.2, 176.4, 78.6],
                &["0.618", "0.618", "1.214", "2.56", "A", "B", "C", "D", "X"],
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 227.2, 127.2208],
                &["0.5", "0.786", "1.272", "1.295", "A", "B", "C", "D", "X"],
            ),
            (
                ABCDPattern,
                &[200.0, 100.0, 161.8, 38.2],
                &["0.618", "2", "A", "B", "C", "D"],
            ),
            (
                HeadAndShoulders,
                &[100.0, 150.0, 120.0, 180.0, 120.0, 150.0, 100.0],
                &["Head", "Left Shoulder", "Right Shoulder"],
            ),
            (
                TrianglePattern,
                &[150.0, 100.0, 140.0, 110.0],
                &["A", "B", "C", "D"],
            ),
            (
                ThreeDrivesPattern,
                &[100.0, 200.0, 138.2, 218.2, 158.2, 255.0, 230.0],
                &["1.29", "1.61"],
            ),
        ];
        let colours = |drawing: &Drawing, expected: Color32| {
            let shapes = paint(drawing);
            for (text, shape) in texts(&shapes) {
                let rect = Rect::from_min_size(shape.pos, shape.galley.size());
                assert!(
                    shapes.iter().any(|s| matches!(
                        s,
                        Shape::Rect(r) if r.fill == drawing.color32() && r.rect.contains_rect(rect)
                    )),
                    "{:?}: {text} has no box of the line colour",
                    drawing.tool_type
                );
                assert!(
                    shape
                        .galley
                        .job
                        .sections
                        .iter()
                        .all(|s| s.format.color == expected),
                    "{:?}: {text}",
                    drawing.tool_type
                );
            }
        };
        for (tool, prices, expected) in cases {
            let drawing = pattern(tool, prices);
            let shapes = paint(&drawing);
            let mut painted: Vec<_> = texts(&shapes).into_iter().map(|(t, _)| t).collect();
            painted.sort();
            assert_eq!(painted, expected, "{tool:?}");
            colours(&drawing, Color32::WHITE);
        }
        // White reads at 1.22:1 on this yellow, black at 17:1.
        let mut yellow = pattern(XABCDPattern, cases[0].1);
        yellow.color = [255, 235, 59, 255];
        colours(&yellow, Color32::BLACK);

        let p = Pos2::new;
        for (points, above) in [
            (
                [
                    p(0.0, 300.0),
                    p(50.0, 100.0),
                    p(100.0, 250.0),
                    p(150.0, 150.0),
                ],
                [false, true, false, true],
            ),
            (
                [
                    p(0.0, 100.0),
                    p(50.0, 300.0),
                    p(100.0, 150.0),
                    p(150.0, 250.0),
                ],
                [true, false, true, false],
            ),
        ] {
            let mut drawing = Drawing::new(0, TrianglePattern);
            drawing.points = points.to_vec();
            drawing.chart_points = points
                .iter()
                .map(|q| ChartPoint::new(q.x, f64::from(q.y)))
                .collect();
            let shapes = paint(&drawing);
            for (i, label) in ["A", "B", "C", "D"].into_iter().enumerate() {
                let (_, shape) = texts(&shapes)
                    .into_iter()
                    .find(|(t, _)| t == label)
                    .unwrap_or_else(|| panic!("no label {label}"));
                let rect = Rect::from_min_size(shape.pos, shape.galley.size());
                if above[i] {
                    assert!(rect.max.y < points[i].y, "{label} of {points:?}: {rect:?}");
                } else {
                    assert!(rect.min.y > points[i].y, "{label} of {points:?}: {rect:?}");
                }
            }
        }
    }

    /// The neckline of the top pattern runs through (20, 250) and (60, 270), y = 250 + (x - 20) / 2,
    /// and every leg to an outer anchor crosses it halfway: (10, 200) to (-10, 280) at (0, 240),
    /// (70, 230) to (90, 330) at (80, 280). The inverse pattern mirrors y about 200.
    #[test]
    fn head_and_shoulders_outline_closes_each_hump_at_the_neckline() {
        let p = Pos2::new;
        let top = [
            p(-10.0, 280.0),
            p(10.0, 200.0),
            p(20.0, 250.0),
            p(40.0, 100.0),
            p(60.0, 270.0),
            p(70.0, 230.0),
            p(90.0, 330.0),
        ];
        let inverse = top.map(|q| p(q.x, 400.0 - q.y));
        let moved = |i: usize, to: Pos2| {
            let mut points = top;
            points[i] = to;
            points
        };
        let left = [p(0.0, 240.0), p(10.0, 200.0), p(20.0, 250.0)];
        let head = [p(20.0, 250.0), p(40.0, 100.0), p(60.0, 270.0)];
        let right = [p(60.0, 270.0), p(70.0, 230.0), p(80.0, 280.0)];
        let full = [p(0.0, 240.0), p(80.0, 280.0)];
        let from_trough = [p(20.0, 250.0), p(80.0, 280.0)];
        type Outline = ([Pos2; 2], Vec<[Pos2; 3]>);
        let cases: [(&[Pos2], Option<Outline>); 9] = [
            (&top, Some((full, vec![left, head, right]))),
            (
                &inverse,
                Some((
                    [p(0.0, 160.0), p(80.0, 120.0)],
                    vec![
                        [p(0.0, 160.0), p(10.0, 200.0), p(20.0, 150.0)],
                        [p(20.0, 150.0), p(40.0, 300.0), p(60.0, 130.0)],
                        [p(60.0, 130.0), p(70.0, 170.0), p(80.0, 120.0)],
                    ],
                )),
            ),
            // The end anchor on the neckline is the crossing.
            (
                &moved(6, p(80.0, 280.0)),
                Some((full, vec![left, head, right])),
            ),
            // The start anchor on the head's side: the first leg never meets the neckline.
            (
                &moved(0, p(-10.0, 220.0)),
                Some((from_trough, vec![head, right])),
            ),
            // The left shoulder beyond the neckline encloses nothing.
            (
                &moved(1, p(10.0, 260.0)),
                Some((from_trough, vec![head, right])),
            ),
            // The head on the neckline leaves no region, and the neckline between the troughs.
            (
                &moved(3, p(40.0, 260.0)),
                Some(([p(20.0, 250.0), p(60.0, 270.0)], Vec::new())),
            ),
            (
                &top[..5],
                Some(([p(0.0, 240.0), p(60.0, 270.0)], vec![left, head])),
            ),
            (&moved(4, p(20.0, 250.0)), None),
            (&top[..4], None),
        ];
        let close = |a: &[Pos2], b: &[Pos2]| a.iter().zip(b).all(|(a, b)| a.distance(*b) < 1e-3);
        for (points, expected) in cases {
            match (head_and_shoulders_outline(points), expected) {
                (Some((line, regions)), Some((expected_line, expected_regions))) => {
                    assert!(close(&line, &expected_line), "{points:?}: {line:?}");
                    assert_eq!(regions.len(), expected_regions.len(), "{points:?}");
                    for (region, expected) in regions.iter().zip(&expected_regions) {
                        assert!(close(region, expected), "{points:?}: {region:?}");
                    }
                }
                (actual, expected) => assert_eq!(actual, expected, "{points:?}"),
            }
        }
    }

    #[test]
    fn triangle_boundaries_meet_at_the_apex_and_clip_the_fill() {
        let p = Pos2::new;
        // Zigzag anchors on an upper line y = 0.5x and a lower line y = 95 - 0.5 (x - 10), which
        // meet at (100, 50); parallel lines; and an upper y = 55 - 0.5x with a lower y = 40 + 2x,
        // which meet at x = 6, left of every anchor. Only the first enclose a triangle.
        let converging = [p(0.0, 0.0), p(10.0, 95.0), p(20.0, 10.0), p(30.0, 85.0)];
        let parallel = [p(0.0, 0.0), p(10.0, 50.0), p(20.0, 0.0), p(30.0, 50.0)];
        let diverging = [p(10.0, 50.0), p(20.0, 80.0), p(30.0, 40.0), p(40.0, 120.0)];
        let vertical = [p(0.0, 0.0), p(10.0, 95.0), p(0.0, 10.0), p(30.0, 85.0)];
        let cases = [
            (
                &converging[..],
                f32::INFINITY,
                Some((
                    [
                        [p(0.0, 0.0), p(100.0, 50.0)],
                        [p(10.0, 95.0), p(100.0, 50.0)],
                    ],
                    Some(vec![p(0.0, 0.0), p(100.0, 50.0), p(10.0, 95.0)]),
                )),
            ),
            (
                &converging[..],
                60.0,
                Some((
                    [[p(0.0, 0.0), p(60.0, 30.0)], [p(10.0, 95.0), p(60.0, 70.0)]],
                    Some(vec![
                        p(0.0, 0.0),
                        p(60.0, 30.0),
                        p(60.0, 70.0),
                        p(10.0, 95.0),
                    ]),
                )),
            ),
            (
                &parallel[..],
                f32::INFINITY,
                Some((
                    [[p(0.0, 0.0), p(30.0, 0.0)], [p(10.0, 50.0), p(30.0, 50.0)]],
                    None,
                )),
            ),
            (
                &diverging[..],
                f32::INFINITY,
                Some((
                    [
                        [p(10.0, 50.0), p(40.0, 35.0)],
                        [p(20.0, 80.0), p(40.0, 120.0)],
                    ],
                    None,
                )),
            ),
            (&vertical[..], f32::INFINITY, None),
            (&converging[..3], f32::INFINITY, None),
        ];
        for (points, right, expected) in cases {
            assert_eq!(triangle_boundaries(points, right), expected, "{points:?}");
        }
    }

    /// Each row's chords and ratios by hand, from prices the screen y of [`pattern`] does not
    /// keep in proportion. Cypher measures C against XA and D against BC and XC; XABCD measures D
    /// against BC and XA; Three drives measures each drive against the correction before it.
    #[test]
    fn pattern_ratios_are_measured_in_price() {
        use DrawingToolType::{ABCDPattern, CypherPattern, ThreeDrivesPattern, XABCDPattern};
        type Chords<'a> = &'a [([usize; 2], Option<f64>)];
        let three_drives: Chords = &[([1, 3], Some(80.0 / 61.8)), ([3, 5], Some(96.8 / 60.0))];
        let cypher: Chords = &[
            ([0, 2], Some(0.5)),
            ([1, 3], Some(1.272)),
            ([2, 4], Some(99.9792 / 77.2)),
            ([0, 4], Some(0.786)),
        ];
        let cases: [(DrawingToolType, &[f64], Chords); 12] = [
            (
                ABCDPattern,
                &[200.0, 100.0, 161.8, 38.2],
                &[([0, 2], Some(0.618)), ([1, 3], Some(2.0))],
            ),
            (
                XABCDPattern,
                &[100.0, 200.0, 138.2, 176.4, 78.6],
                &[
                    ([0, 2], Some(0.618)),
                    ([1, 3], Some(38.2 / 61.8)),
                    ([2, 4], Some(97.8 / 38.2)),
                    ([0, 4], Some(1.214)),
                ],
            ),
            (
                XABCDPattern,
                &[100.0, 150.0, 125.0, 175.0],
                &[
                    ([0, 2], Some(0.5)),
                    ([1, 3], Some(2.0)),
                    ([2, 4], None),
                    ([0, 4], None),
                ],
            ),
            (
                XABCDPattern,
                &[100.0, 100.0, 138.2, 176.4],
                &[
                    ([0, 2], None),
                    ([1, 3], Some(1.0)),
                    ([2, 4], None),
                    ([0, 4], None),
                ],
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 227.2, 127.2208],
                cypher,
            ),
            (
                CypherPattern,
                &[300.0, 200.0, 250.0, 172.8, 272.7792],
                cypher,
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 227.2],
                &[
                    ([0, 2], Some(0.5)),
                    ([1, 3], Some(1.272)),
                    ([2, 4], None),
                    ([0, 4], None),
                ],
            ),
            (
                CypherPattern,
                &[100.0, 100.0, 125.0, 150.0, 110.7],
                &[
                    ([0, 2], None),
                    ([1, 3], None),
                    ([2, 4], Some(39.3 / 25.0)),
                    ([0, 4], Some(0.786)),
                ],
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 100.0, 120.0],
                &[
                    ([0, 2], Some(0.5)),
                    ([1, 3], Some(0.0)),
                    ([2, 4], Some(0.4)),
                    ([0, 4], None),
                ],
            ),
            (
                ThreeDrivesPattern,
                &[100.0, 200.0, 138.2, 218.2, 158.2, 255.0],
                three_drives,
            ),
            (
                ThreeDrivesPattern,
                &[300.0, 200.0, 261.8, 181.8, 241.8, 145.0],
                three_drives,
            ),
            (
                ThreeDrivesPattern,
                &[100.0, 100.0, 90.0, 120.0, 120.0, 150.0],
                &[([1, 3], Some(3.0)), ([3, 5], None)],
            ),
        ];
        for (tool, prices, expected) in cases {
            let chords = pattern(tool, prices).ratio_chords();
            assert_eq!(chords.len(), expected.len(), "{tool:?} {prices:?}");
            for (chord, expected) in chords.iter().zip(expected) {
                assert_eq!(chord.0, expected.0, "{tool:?} {prices:?}");
                match (chord.1, expected.1) {
                    (Some(a), Some(e)) => {
                        assert!((a - e).abs() < 1e-9, "{tool:?} {prices:?}: {a} != {e}")
                    }
                    (a, e) => assert_eq!(a, e, "{tool:?} {prices:?} {:?}", chord.0),
                }
            }
        }
    }

    #[test]
    fn ratio_text_drops_trailing_zeros() {
        for (ratio, decimals, expected) in [
            (0.618, 3, Some("0.618")),
            (2.0, 3, Some("2")),
            (97.8 / 38.2, 3, Some("2.56")),
            (10.0, 3, Some("10")),
            (0.0004, 3, Some("0")),
            (80.0 / 61.8, 2, Some("1.29")),
            (1.3, 2, Some("1.3")),
            (f64::INFINITY, 3, None),
            (f64::NAN, 3, None),
        ] {
            assert_eq!(ratio_text(ratio, decimals).as_deref(), expected, "{ratio}");
        }
    }
}
