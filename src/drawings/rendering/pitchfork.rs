//! Pitchfork tool rendering implementations
//!
//! Includes: standard pitchfork, modified Schiff, inside pitchfork, and pitchfan.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    pub(crate) fn render_pitchfork(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
        is_schiff: bool,
    ) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Need at least 3 points for pitchfork
        if self.points.len() < 3 {
            // Draw points we have so far
            for (i, &point) in self.points.iter().enumerate() {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.sm, color);
                if i > 0 {
                    painter.line_segment(
                        [self.points[i - 1], point],
                        Stroke::new(stroke::HAIRLINE, color),
                    );
                }
            }
            return;
        }

        let p0 = self.points[0]; // Anchor point
        let p1 = self.points[1]; // First swing
        let p2 = self.points[2]; // Second swing

        // Calculate midpoint of p1-p2
        let midpoint = if is_schiff {
            // Schiff pitchfork uses different anchor calculation
            Pos2::new((p1.x + p2.x) / 2.0, (p0.y + (p1.y + p2.y) / 2.0) / 2.0)
        } else {
            Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0)
        };

        // Draw median line (from anchor through midpoint)
        let median_direction = Pos2::new(midpoint.x - p0.x, midpoint.y - p0.y);
        let median_len = chart_rect.max.x - p0.x;
        let scale = median_len / (midpoint.x - p0.x).max(0.001);
        let median_end = Pos2::new(
            p0.x + median_direction.x * scale,
            p0.y + median_direction.y * scale,
        );

        painter.line_segment([p0, median_end], Stroke::new(self.stroke_width, color));

        // Calculate parallel lines through p1 and p2
        let parallel_direction = Pos2::new(median_end.x - p0.x, median_end.y - p0.y);

        // Upper parallel (through p1)
        let upper_end = Pos2::new(p1.x + parallel_direction.x, p1.y + parallel_direction.y);
        painter.line_segment([p1, upper_end], Stroke::new(stroke::HAIRLINE, color));

        // Lower parallel (through p2)
        let lower_end = Pos2::new(p2.x + parallel_direction.x, p2.y + parallel_direction.y);
        painter.line_segment([p2, lower_end], Stroke::new(stroke::HAIRLINE, color));

        // Draw conn lines
        let conn_stroke = Stroke::new(stroke::HAIRLINE / 2.0, Color32::from_white_alpha(100));
        painter.line_segment([p0, p1], conn_stroke);
        painter.line_segment([p0, p2], conn_stroke);
        painter.line_segment([p1, p2], conn_stroke);

        // Draw anchor points
        painter.circle_filled(p0, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
    }

    pub(crate) fn render_modified_schiff_pitchfork(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
    ) {
        // Modified Schiff moves the anchor point to midway between p0 and p1
        if self.points.len() < 3 {
            for &p in &self.points {
                painter.circle_filled(
                    p,
                    DESIGN_TOKENS.rounding.sm,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        self.color[3],
                    ),
                );
            }
            return;
        }

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let p0 = self.points[0];
        let p1 = self.points[1];
        let p2 = self.points[2];

        // Modified anchor point
        let modified_anchor = Pos2::new((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
        let midpoint = Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);

        // Median line direction
        let dir = Pos2::new(
            midpoint.x - modified_anchor.x,
            midpoint.y - modified_anchor.y,
        );
        let scale = (chart_rect.max.x - modified_anchor.x) / dir.x.max(0.001);
        let median_end = Pos2::new(
            modified_anchor.x + dir.x * scale,
            modified_anchor.y + dir.y * scale,
        );

        painter.line_segment(
            [modified_anchor, median_end],
            Stroke::new(self.stroke_width, color),
        );

        // Parallel lines
        let parallel_dir = Pos2::new(
            median_end.x - modified_anchor.x,
            median_end.y - modified_anchor.y,
        );
        painter.line_segment(
            [p1, Pos2::new(p1.x + parallel_dir.x, p1.y + parallel_dir.y)],
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.line_segment(
            [p2, Pos2::new(p2.x + parallel_dir.x, p2.y + parallel_dir.y)],
            Stroke::new(stroke::HAIRLINE, color),
        );

        // Conn lines
        let light = Stroke::new(stroke::HAIRLINE / 2.0, Color32::from_white_alpha(100));
        painter.line_segment([p0, p1], light);
        painter.line_segment([p1, p2], light);

        painter.circle_filled(p0, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
    }

    pub(crate) fn render_inside_pitchfork(&self, painter: &egui::Painter, chart_rect: Rect) {
        // Inside pitchfork uses the midpoint of the first two points as anchor
        if self.points.len() < 3 {
            for &p in &self.points {
                painter.circle_filled(
                    p,
                    DESIGN_TOKENS.rounding.sm,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        self.color[3],
                    ),
                );
            }
            return;
        }

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let p0 = self.points[0];
        let p1 = self.points[1];
        let p2 = self.points[2];

        // Inside anchor - midpoint of p1-p2
        let anchor = Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);

        // Median from p0 through anchor
        let dir = Pos2::new(anchor.x - p0.x, anchor.y - p0.y);
        let scale = (chart_rect.max.x - p0.x) / dir.x.max(0.001);
        let median_end = Pos2::new(p0.x + dir.x * scale, p0.y + dir.y * scale);

        painter.line_segment([p0, median_end], Stroke::new(self.stroke_width, color));

        // Parallel lines through p1 and p2
        let parallel_dir = Pos2::new(median_end.x - p0.x, median_end.y - p0.y);
        painter.line_segment(
            [p1, Pos2::new(p1.x + parallel_dir.x, p1.y + parallel_dir.y)],
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.line_segment(
            [p2, Pos2::new(p2.x + parallel_dir.x, p2.y + parallel_dir.y)],
            Stroke::new(stroke::HAIRLINE, color),
        );

        painter.circle_filled(p0, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.md, color);
    }

    pub(crate) fn render_pitchfan(&self, painter: &egui::Painter, rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let end = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let line_stroke_main = Stroke::new(self.stroke_width, color);

        // Calculate base angle from center to end
        let dx = end.x - center.x;
        let dy = end.y - center.y;
        let base_angle = dy.atan2(dx);

        // Fibonacci fan angles (relative to base): 0°, 23.6°, 38.2°, 50%, 61.8%, 78.6%
        let fib_ratios = [0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0];
        let max_angle_spread = std::f32::consts::FRAC_PI_4; // 45 degrees spread

        for (i, &ratio) in fib_ratios.iter().enumerate() {
            let angle_offset = (ratio - 0.5) * max_angle_spread * 2.0;
            let fan_angle = base_angle + angle_offset;

            // Extend fan line to chart edge
            let extend_dist = rect.width().max(rect.height()) * 2.0;
            let fan_end = Pos2::new(
                center.x + extend_dist * fan_angle.cos(),
                center.y + extend_dist * fan_angle.sin(),
            );

            let line_stroke = if i == 0 || i == fib_ratios.len() - 1 {
                line_stroke_main
            } else {
                Stroke::new(
                    stroke::HAIRLINE,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        150,
                    ),
                )
            };

            painter.line_segment([center, fan_end], line_stroke);

            // Draw ratio label
            let label_dist = 60.0 + i as f32 * 10.0;
            let label_pos = Pos2::new(
                center.x + label_dist * fan_angle.cos(),
                center.y + label_dist * fan_angle.sin(),
            );
            if ratio > 0.0 {
                painter.text(
                    label_pos,
                    egui::Align2::LEFT_CENTER,
                    format!("{:.1}%", ratio * 100.0),
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }

        painter.circle_filled(center, DESIGN_TOKENS.rounding.md, color);
        painter.circle_filled(end, DESIGN_TOKENS.rounding.sm, color);
    }
}
