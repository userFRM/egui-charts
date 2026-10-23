//! Shape and curve tool rendering implementations
//!
//! Includes: rotated rect, double curve, brush, highlighter, path, curve, and sine line.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Render Rotated Rectangle
    ///
    /// Rotation is not yet supported; currently draws an axis-aligned rectangle.
    /// A full implementation would require a third control point or rotation angle.
    pub(crate) fn render_rotated_rect(&self, painter: &egui::Painter) {
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

        // For now, draw as regular rect (rotation would require additional control point)
        let rect = Rect::from_two_pos(p1, p2);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(self.stroke_width, color),
            StrokeKind::Inside,
        );
    }

    pub(crate) fn render_double_curve(&self, painter: &egui::Painter) {
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

        for pair in double_curve_points(p1, p2).windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(self.stroke_width, color));
        }
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
            // Draw small circles at each vertex
            painter.circle_filled(point, DESIGN_TOKENS.rounding.sm, color);
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

/// The S curve from `p1` to `p2`: the straight line between them with one
/// sine period of 20 px added to y.
pub(super) fn double_curve_points(p1: Pos2, p2: Pos2) -> Vec<Pos2> {
    (0..=32)
        .map(|i| {
            let t = i as f32 / 32.0;
            let offset = (t * std::f32::consts::TAU).sin() * 20.0;
            Pos2::new(p1.x + (p2.x - p1.x) * t, p1.y + (p2.y - p1.y) * t + offset)
        })
        .collect()
}
