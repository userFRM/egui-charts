//! Ghost feed rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders Ghost Feed with opacity control and historical overlay.
    /// Features: semi-transparent historical price action, time shift indicator.
    pub(crate) fn render_ghost_feed(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }

        let base_color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Ghost color (more transparent)
        let ghost_alpha = 60;
        let ghost_color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            ghost_alpha,
        );

        // Draw the ghost price path with gradient fade
        self.draw_ghost_path(painter, ghost_alpha);

        // Anchor point at start (solid)
        self.draw_ghost_anchor(painter, base_color, ghost_color);

        // End point indicator
        if let Some(&last) = self.points.last() {
            painter.circle_filled(last, DESIGN_TOKENS.rounding.md, ghost_color);
        }

        // Dashed connection to current price area (if we have chart points)
        self.draw_ghost_projection(painter, ghost_color);
    }

    fn draw_ghost_path(&self, painter: &egui::Painter, ghost_alpha: u8) {
        let ghost_color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            ghost_alpha,
        );

        let num_points = self.points.len();
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                // Fade in from left to right
                let progress = i as f32 / num_points as f32;
                let alpha = (ghost_alpha as f32 * (0.3 + 0.7 * progress)) as u8;
                let fade_color = Color32::from_rgba_unmultiplied(
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    alpha,
                );

                painter.line_segment(
                    [self.points[i - 1], point],
                    Stroke::new(self.stroke_width.max(1.5), fade_color),
                );
            }

            // Draw dots at each point (hollow)
            if i % 5 == 0 {
                // Every 5th point to reduce clutter
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.sm,
                    Stroke::new(stroke::HAIRLINE, ghost_color),
                );
            }
        }
    }

    fn draw_ghost_anchor(
        &self,
        painter: &egui::Painter,
        base_color: Color32,
        ghost_color: Color32,
    ) {
        if let Some(&first) = self.points.first() {
            painter.circle_filled(first, DESIGN_TOKENS.rounding.lg, base_color);
            painter.circle_stroke(
                first,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            // Ghost label
            let font = egui::FontId::proportional(typography::XS);
            let label_bg = Rect::from_center_size(
                Pos2::new(first.x + 35.0, first.y - 12.0),
                egui::vec2(60.0, 16.0),
            );
            painter.rect_filled(
                label_bg,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.7),
            );
            painter.text(
                label_bg.center(),
                egui::Align2::CENTER_CENTER,
                "Ghost",
                font,
                ghost_color,
            );
        }
    }

    fn draw_ghost_projection(&self, painter: &egui::Painter, ghost_color: Color32) {
        if !self.chart_points.is_empty() && self.points.len() >= 2 {
            let last = self.points[self.points.len() - 1];
            // Draw dashed line to indicate projection
            let proj_end = Pos2::new(last.x + 50.0, last.y);
            let dash = 6.0;
            let gap = 4.0;
            let mut x = last.x;
            while x < proj_end.x {
                let seg_end = (x + dash).min(proj_end.x);
                painter.line_segment(
                    [Pos2::new(x, last.y), Pos2::new(seg_end, last.y)],
                    Stroke::new(stroke::HAIRLINE, ghost_color),
                );
                x += dash + gap;
            }
        }
    }
}
