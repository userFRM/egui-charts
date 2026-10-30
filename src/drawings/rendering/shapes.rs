//! Shape and curve tool rendering implementations
//!
//! Includes: rotated rect, double curve, brush, highlighter, path, curve, and sine line.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Shape, Stroke};

impl Drawing {
    /// The first anchor is the center, the second fixes the radius, and the third fixes
    /// the end ray. The sweep takes the short arc in (-180°, 180°], clockwise at 180°.
    pub(super) fn sector_points(&self) -> Option<Vec<Pos2>> {
        let [center, start, end, ..] = *self.points else {
            return None;
        };
        let radius = center.distance(start);
        if radius < 1e-6 || center.distance_sq(end) < 1e-12 {
            return None;
        }
        let from = (start - center).angle();
        let mut sweep = (end - center).angle() - from;
        if sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        } else if sweep <= -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }
        let steps = ((sweep.abs() / std::f32::consts::TAU * 128.0).ceil() as usize).max(1);
        let mut points = vec![center];
        points.extend((0..=steps).map(|i| {
            let angle = from + sweep * i as f32 / steps as f32;
            center + egui::Vec2::angled(angle) * radius
        }));
        Some(points)
    }

    pub(super) fn render_sector(&self, painter: &egui::Painter) {
        let Some(points) = self.sector_points() else {
            return;
        };
        if let Some(fill) = self.fill_color32() {
            // One mesh keeps translucent fill continuous across the fan's internal edges.
            let mut mesh = egui::Mesh::default();
            for &point in &points {
                mesh.colored_vertex(point, fill);
            }
            for i in 1..points.len() - 1 {
                mesh.add_triangle(0, i as u32, (i + 1) as u32);
            }
            painter.add(Shape::mesh(mesh));
        }
        painter.add(Shape::closed_line(points, self.stroke()));
    }

    /// A rectangle whose first two anchors form its base and whose third sets the width.
    pub(crate) fn render_rotated_rect(&self, painter: &egui::Painter) {
        let Some(corners) = self.rotated_rect_corners() else {
            return;
        };
        let fill = self.fill_color.map_or(Color32::TRANSPARENT, |c| {
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
        });
        painter.add(egui::Shape::convex_polygon(
            corners.to_vec(),
            fill,
            Stroke::new(self.stroke_width, self.color32()),
        ));
    }

    pub(super) fn rotated_rect_corners(&self) -> Option<[Pos2; 4]> {
        let &[a, b, c, ..] = self.points.as_slice() else {
            return None;
        };
        let base = b - a;
        if base.length_sq() < 1e-6 {
            return None;
        }
        let normal = egui::vec2(-base.y, base.x);
        let offset = normal * ((c - a).dot(normal) / base.length_sq());
        Some([a, b, b + offset, a + offset])
    }

    /// Cubic Bezier curve with two independently placed control points.
    pub(crate) fn render_double_curve(&self, painter: &egui::Painter) {
        let Some(points) = self.double_curve_points() else {
            return;
        };
        painter.add(egui::Shape::line(
            points,
            Stroke::new(self.stroke_width, self.color32()),
        ));
    }

    pub(super) fn double_curve_points(&self) -> Option<Vec<Pos2>> {
        let &[a, b, c, d, ..] = self.points.as_slice() else {
            return None;
        };
        Some(
            (0..=64)
                .map(|i| {
                    let t = i as f32 / 64.0;
                    let u = 1.0 - t;
                    (a.to_vec2() * u * u * u
                        + b.to_vec2() * 3.0 * u * u * t
                        + c.to_vec2() * 3.0 * u * t * t
                        + d.to_vec2() * t * t * t)
                        .to_pos2()
                })
                .collect(),
        )
    }

    /// The width a brush is drawn at: twice the stroke width for the brush,
    /// eight times for the highlighter; `None` for other tools.
    pub(crate) fn brush_width(&self) -> Option<f32> {
        match self.tool_type {
            DrawingToolType::Brush => Some(self.stroke_width * 2.0),
            DrawingToolType::Highlighter => Some(self.stroke_width * 8.0),
            _ => None,
        }
    }

    pub(crate) fn render_brush(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment(
                    [self.points[i - 1], point],
                    Stroke::new(self.brush_width().unwrap_or(self.stroke_width), color),
                );
            }
        }
    }

    pub(crate) fn render_highlighter(&self, painter: &egui::Painter) {
        let color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment(
                    [self.points[i - 1], point],
                    Stroke::new(self.brush_width().unwrap_or(self.stroke_width), color),
                );
            }
        }
    }

    // Note: render_sine_line is now in cycles.rs as it's a cyclic analysis tool

    /// Render Path - Multi-point connected freehand path
    pub(crate) fn render_path(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        // Draw connected line segments through all points
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment([self.points[i - 1], point], stroke);
            }
            // Keep the endpoint clear for its arrowhead.
            if i + 1 < self.points.len() || !self.arrow_end_style.is_visible() {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.sm, color);
            }
        }

        // Larger circle at start point
        if !self.points.is_empty() {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render Curve - Bezier quadratic curve
    pub(crate) fn render_curve(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }
        let Some([p0, p1, p2]) = self.curve_controls() else {
            return;
        };

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        for pair in curve_points(p0, p1, p2).windows(2) {
            painter.line_segment([pair[0], pair[1]], stroke);
        }

        // Draw anchor points
        painter.circle_filled(p0, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
        // Draw control point (smaller, semi-transparent)
        painter.circle_filled(
            p1,
            DESIGN_TOKENS.rounding.sm,
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 128),
        );
    }

    /// The curve's start, control and end points: the control point is the
    /// second anchor of three, or else set off to the side of the midpoint.
    /// `None` below two anchors.
    pub(super) fn curve_controls(&self) -> Option<[Pos2; 3]> {
        let (&p0, &p2) = (self.points.first()?, self.points.last()?);
        if self.points.len() < 2 {
            return None;
        }
        let p1 = if self.points.len() >= 3 {
            self.points[1]
        } else {
            let mid = Pos2::new((p0.x + p2.x) / 2.0, (p0.y + p2.y) / 2.0);
            Pos2::new(mid.x - (p2.y - p0.y) * 0.3, mid.y + (p2.x - p0.x) * 0.3)
        };
        Some([p0, p1, p2])
    }
}

/// The quadratic Bezier curve from `p0` to `p2` pulled towards `p1`.
pub(super) fn curve_points(p0: Pos2, p1: Pos2, p2: Pos2) -> Vec<Pos2> {
    (0..=32)
        .map(|i| Drawing::quadratic_bezier(p0, p1, p2, i as f32 / 32.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_and_fan_geometry_uses_the_placed_controls() {
        let rect = egui::Rect::from_min_max(egui::pos2(-400.0, -400.0), egui::pos2(400.0, 400.0));
        for (tool, anchors, want) in [
            (
                DrawingToolType::Sector,
                vec![(100.0, 100.0), (200.0, 100.0), (100.0, 150.0)],
                vec![(200.0, 100.0), (170.71068, 170.71068), (100.0, 200.0)],
            ),
            (
                DrawingToolType::Sector,
                vec![(100.0, 100.0), (200.0, 100.0), (100.0, 50.0)],
                vec![(200.0, 100.0), (170.71068, 29.28932), (100.0, 0.0)],
            ),
            (
                DrawingToolType::RotatedRect,
                vec![(0.0, 0.0), (100.0, 100.0), (0.0, 100.0)],
                vec![(50.0, 150.0), (-50.0, 50.0)],
            ),
            (
                DrawingToolType::DoubleCurve,
                vec![(0.0, 0.0), (0.0, 100.0), (100.0, -100.0), (100.0, 0.0)],
                vec![(50.0, 0.0), (100.0, 0.0)],
            ),
            (
                DrawingToolType::Arc,
                vec![(-50.0, 0.0), (50.0, 0.0), (0.0, 50.0)],
                vec![(-50.0, 0.0), (50.0, 0.0), (0.0, 50.0)],
            ),
            (
                DrawingToolType::Pitchfan,
                vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)],
                vec![(400.0, 0.0), (400.0, 200.0), (400.0, 400.0)],
            ),
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.points = anchors.into_iter().map(Pos2::from).collect();
            let outline = drawing.outline(rect).unwrap();
            for point in want.into_iter().map(Pos2::from) {
                assert!(
                    outline
                        .lines
                        .iter()
                        .flatten()
                        .any(|p| p.distance(point) < 1e-3),
                    "{tool:?} missing {point:?}"
                );
            }
        }
        let mut sector = Drawing::new(0, DrawingToolType::Sector);
        for points in [
            vec![Pos2::ZERO; 3],
            vec![Pos2::ZERO, Pos2::new(100.0, 0.0), Pos2::ZERO],
        ] {
            sector.points = points;
            assert!(sector.outline(rect).is_none());
        }
    }
}
