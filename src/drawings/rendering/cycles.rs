//! Cyclic tool rendering implementations
//!
//! Includes: cyclic lines, time cycles, and sine line.

use super::style::circle_sides;
use crate::drawings::domain::Drawing;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders full-height lines from the first anchor towards the second at their interval.
    pub(crate) fn render_cyclic_lines(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let step = f64::from(p2.x) - f64::from(p1.x);
        if !step.is_finite() || step.abs() < 1.0 {
            return;
        }

        // Skip off-screen cycles directly; only nonnegative steps from the first anchor exist.
        let anchor = f64::from(p1.x);
        let (near, far) = if step > 0.0 {
            (chart_rect.min.x, chart_rect.max.x)
        } else {
            (chart_rect.max.x, chart_rect.min.x)
        };
        let first = ((f64::from(near) - anchor) / step).ceil().max(0.0) as i64;
        let last = ((f64::from(far) - anchor) / step).floor() as i64;
        for cycle in first..=last {
            let x = (anchor + cycle as f64 * step) as f32;
            painter.vline(
                x,
                chart_rect.y_range(),
                Stroke::new(self.stroke_width, color),
            );
        }

        // Draw anchor points with white stroke
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Renders time cycles: the anchors span one cycle, repeated as half circles
    /// forward in time to the right edge of the chart.
    pub(crate) fn render_time_cycles(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let edge = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let arcs = time_cycle_arcs(center, edge, chart_rect.x_range());
        if arcs.is_empty() {
            return;
        }
        // The last arc may run past the right edge of the pane.
        let painter = &painter.with_clip_rect(chart_rect);
        let arc_stroke = Stroke::new(self.stroke_width, color);
        for (arc_center, radius) in arcs {
            let segments = circle_sides(radius * painter.pixels_per_point()).div_ceil(2);
            // Upper half circle, from the cycle's start to its end.
            let points = (0..=segments)
                .map(|j| {
                    let angle = std::f32::consts::PI * (1.0 + j as f32 / segments as f32);
                    arc_center + radius * egui::vec2(angle.cos(), angle.sin())
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(
                points,
                self.fill_color32().unwrap_or(Color32::TRANSPARENT),
                arc_stroke,
            ));
        }

        // Draw center anchor point
        painter.circle_filled(center, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            center,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );

        // Draw edge point
        painter.circle_filled(edge, DESIGN_TOKENS.rounding.md, color);
        painter.circle_stroke(
            edge,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Renders a sine wave line for cyclical analysis.
    /// The anchors are a peak and a trough half a period apart; the curve extends across the chart.
    pub(crate) fn render_sine_line(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw sine wave extending across chart
        let segments = 200;
        let start_x = chart_rect.min.x;
        let end_x = chart_rect.max.x;
        let x_range = end_x - start_x;

        let mut wave = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let x = start_x + t * x_range;

            // Anchors too close together draw nothing; the first sample returns before any paint.
            let Some(y) = sine_line_y(p1, p2, x) else {
                return;
            };
            wave.push(Pos2::new(x, y));
        }
        // One path, not a segment shape per sample, so the tessellator joins the samples.
        painter.add(egui::Shape::line(
            wave,
            Stroke::new(self.stroke_width, color),
        ));

        // Draw anchor points
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }
}

/// The sine line's y at `x`, with a peak or trough at each anchor and half a period between them.
/// `None` when the anchors are less than 5 px apart horizontally.
fn sine_line_y(p1: Pos2, p2: Pos2, x: f32) -> Option<f32> {
    let half_period = p2.x - p1.x;
    if half_period.abs() < 5.0 {
        return None;
    }
    let center = (p1.y + p2.y) / 2.0;
    Some(center + (p1.y - center) * (std::f32::consts::PI * (x - p1.x) / half_period).cos())
}

/// Centre and radius of each time cycle arc that reaches into `x_range`: the
/// anchors span the first cycle, and arcs as wide follow it forward in time,
/// standing on the first anchor's price. Empty when the anchors are less than
/// 5 px apart horizontally.
fn time_cycle_arcs(p1: Pos2, p2: Pos2, x_range: egui::Rangef) -> Vec<(Pos2, f32)> {
    let period = (p2.x - p1.x).abs();
    if period < 5.0 {
        return Vec::new();
    }
    let start = p1.x.min(p2.x);
    // Skip the cycles that end left of the view without walking through them.
    let first = ((x_range.min - start) / period).floor().max(0.0) as u32;
    (first..)
        .map(|k| start + period * k as f32)
        .take_while(|&left| left < x_range.max)
        .map(|left| (Pos2::new(left + period / 2.0, p1.y), period / 2.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(drawing: &Drawing, rect: Rect) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| match drawing.tool_type {
            crate::drawings::DrawingToolType::CyclicLines => {
                drawing.render_cyclic_lines(ui.painter(), rect);
            }
            crate::drawings::DrawingToolType::TimeCycles => {
                drawing.render_time_cycles(ui.painter(), rect);
            }
            _ => drawing.render_sine_line(ui.painter(), rect),
        });
        let shapes = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    #[test]
    fn cyclic_lines_repeat_only_towards_the_second_anchor() {
        // x = first + n * (second - first), n >= 0, clipped to 0..250.
        for (first, second, expected) in [
            (100.0, 140.0, vec![100.0, 140.0, 180.0, 220.0]),
            (140.0, 100.0, vec![140.0, 100.0, 60.0, 20.0]),
            (-60.0, -20.0, vec![20.0, 60.0, 100.0, 140.0, 180.0, 220.0]),
            (310.0, 270.0, vec![230.0, 190.0, 150.0, 110.0, 70.0, 30.0]),
            (-60.0, -100.0, vec![]),
            (310.0, 350.0, vec![]),
            (0.0, 125.0, vec![0.0, 125.0, 250.0]),
            (100.0, 100.0, vec![]),
        ] {
            let mut drawing = Drawing::new(1, crate::drawings::DrawingToolType::CyclicLines);
            drawing.points = vec![Pos2::new(first, 80.0), Pos2::new(second, 90.0)];
            drawing.fill_color = Some([12, 34, 56, 80]);
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(250.0, 200.0));
            let shapes = paint(&drawing, rect);
            let positions: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::LineSegment { points, .. } => Some(points[0].x),
                    _ => None,
                })
                .collect();
            assert_eq!(positions, expected, "anchors {first}, {second}");
            for shape in shapes {
                match shape {
                    egui::Shape::LineSegment { points, stroke } => {
                        assert_eq!(points[0].x, points[1].x);
                        assert_eq!([points[0].y, points[1].y], [0.0, 200.0]);
                        assert_eq!(stroke, drawing.stroke());
                    }
                    // Permanent anchor decorations belong to a separate change.
                    egui::Shape::Circle(_) | egui::Shape::Noop => {}
                    other => panic!("unexpected cyclic line decoration: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn cycle_curves_have_no_guides_or_labels_and_time_cycles_can_fill() {
        use crate::drawings::DrawingToolType::{SineLine, TimeCycles};
        for tool in [TimeCycles, SineLine] {
            for fill in [None, Some([12, 34, 56, 80])] {
                let mut drawing = Drawing::new(1, tool);
                drawing.points = vec![Pos2::new(100.0, 120.0), Pos2::new(140.0, 160.0)];
                drawing.fill_color = fill;
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(250.0, 200.0));
                let mut paths = 0;
                for shape in paint(&drawing, rect) {
                    match shape {
                        egui::Shape::Path(path) => {
                            paths += 1;
                            assert_eq!(path.closed, tool == TimeCycles);
                            let expected_fill = if tool == TimeCycles {
                                drawing.fill_color32().unwrap_or(Color32::TRANSPARENT)
                            } else {
                                Color32::TRANSPARENT
                            };
                            assert_eq!(path.fill, expected_fill);
                            if tool == TimeCycles {
                                // Diameter 40: endpoints at baseline 120, apex at 100.
                                assert!((path.points.first().unwrap().y - 120.0).abs() < 1e-3);
                                assert!((path.points.last().unwrap().y - 120.0).abs() < 1e-3);
                                assert!(
                                    (path
                                        .points
                                        .iter()
                                        .map(|p| p.y)
                                        .fold(f32::INFINITY, f32::min)
                                        - 100.0)
                                        .abs()
                                        < 0.25
                                );
                            }
                        }
                        egui::Shape::Circle(_) | egui::Shape::Noop => {}
                        other => panic!("unexpected {tool:?} decoration: {other:?}"),
                    }
                }
                assert_eq!(paths, if tool == TimeCycles { 4 } else { 1 });
            }
        }
    }

    /// The anchors span one cycle; arcs of that width follow each other from
    /// the earlier anchor to the right edge, on the first anchor's price.
    #[test]
    fn time_cycles_repeat_the_anchor_interval_forward() {
        let cases = [
            // Cycle 100..140: arcs centred at 120, 160, 200 and 240, the last cut by the edge.
            (
                (100.0, 200.0),
                (140.0, 180.0),
                (0.0, 250.0),
                vec![120.0, 160.0, 200.0, 240.0],
            ),
            // Second anchor first in time; the arc ending at 140 is left of the view.
            (
                (140.0, 200.0),
                (100.0, 180.0),
                (170.0, 250.0),
                vec![160.0, 200.0, 240.0],
            ),
            ((100.0, 200.0), (104.0, 150.0), (0.0, 250.0), vec![]),
        ];
        for ((x1, y1), (x2, y2), (min, max), centres) in cases {
            let got = time_cycle_arcs(
                Pos2::new(x1, y1),
                Pos2::new(x2, y2),
                egui::Rangef::new(min, max),
            );
            let want: Vec<_> = centres.iter().map(|&x| (Pos2::new(x, y1), 20.0)).collect();
            assert!(
                got.len() == want.len()
                    && got
                        .iter()
                        .zip(&want)
                        .all(|(g, w)| g.0.distance(w.0) < 1e-3 && (g.1 - w.1).abs() < 1e-3),
                "anchors x {x1} and {x2}: got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn sine_line_passes_through_both_anchors_half_a_period_apart() {
        // Expected values from y = c + (p1.y - c) * cos(pi * (x - p1.x) / (p2.x - p1.x)), c = (p1.y + p2.y) / 2.
        let (a, b) = (Pos2::new(100.0, 50.0), Pos2::new(150.0, 150.0));
        let cases = [
            (a, b, 100.0, Some(50.0)),
            (a, b, 150.0, Some(150.0)),
            (a, b, 125.0, Some(100.0)),
            (a, b, 200.0, Some(50.0)),
            (a, b, 112.5, Some(64.644_66)),
            (a, b, 75.0, Some(100.0)),
            (b, a, 100.0, Some(50.0)),
            (b, a, 125.0, Some(100.0)),
            (a, Pos2::new(104.0, 150.0), 100.0, None),
        ];
        for (p1, p2, x, want) in cases {
            let got = sine_line_y(p1, p2, x);
            let ok = match (got, want) {
                (Some(g), Some(w)) => (g - w).abs() < 1e-4,
                (g, w) => g == w,
            };
            assert!(ok, "p1 {p1:?} p2 {p2:?} x {x}: got {got:?}, want {want:?}");
        }
    }
}
