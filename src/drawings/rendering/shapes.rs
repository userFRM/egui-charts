//! Shape and curve tool rendering implementations
//!
//! Includes: rotated rect, double curve, brush, highlighter, path, curve, and sine line.

use crate::drawings::domain::Drawing;
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

        // Draw S-curve between points
        let segments = 32;
        let mut prev = p1;
        for i in 1..=segments {
            let t = i as f32 / segments as f32;
            let x = p1.x + (p2.x - p1.x) * t;
            let offset = ((t * std::f32::consts::PI * 2.0).sin()) * 20.0;
            let y = p1.y + (p2.y - p1.y) * t + offset;
            let point = Pos2::new(x, y);
            painter.line_segment([prev, point], Stroke::new(self.stroke_width, color));
            prev = point;
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
                    Stroke::new(self.stroke_width * 2.0, color),
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
                    Stroke::new(self.stroke_width * 8.0, color),
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
        let p0 = self.points[0];
        let p2 = self.points[self.points.len() - 1];

        // Control point is either the middle point or calculated
        let p1 = if self.points.len() >= 3 {
            self.points[1]
        } else {
            // Auto-generate control point for smooth curve
            let mid_x = (p0.x + p2.x) / 2.0;
            let mid_y = (p0.y + p2.y) / 2.0;
            let dx = p2.x - p0.x;
            let dy = p2.y - p0.y;
            // Perpendicular offset
            Pos2::new(mid_x - dy * 0.3, mid_y + dx * 0.3)
        };

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        // Draw Bezier curve using segments
        let segments = 32;
        let mut prev_point: Option<Pos2> = None;
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let point = Self::quadratic_bezier(p0, p1, p2, t);
            if let Some(prev) = prev_point {
                painter.line_segment([prev, point], stroke);
            }
            prev_point = Some(point);
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
}
