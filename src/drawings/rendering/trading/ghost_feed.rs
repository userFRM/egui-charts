//! Ghost feed rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::localization as l10n;
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
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

        self.draw_ghost_candles(painter);

        // Draw the ghost price path with gradient fade
        self.draw_ghost_path(painter, ghost_alpha);

        // Anchor point at start (solid)
        self.draw_ghost_anchor(painter, base_color);

        // End point indicator
        if let Some(&last) = self.points.last() {
            painter.circle_filled(last, DESIGN_TOKENS.rounding.md, ghost_color);
        }

        // Dashed connection to current price area (if we have chart points)
        self.draw_ghost_projection(painter, ghost_color);
    }

    /// The projected candles, translucent and outlined in the drawing's colour so they read as
    /// a projection rather than market data: rising ones filled, falling ones hollow.
    fn draw_ghost_candles(&self, painter: &egui::Painter) {
        let [r, g, b, _] = self.color;
        let outline = Stroke::new(
            self.stroke_width.max(stroke::HAIRLINE),
            Color32::from_rgba_unmultiplied(r, g, b, 160),
        );
        for c in &self.ghost_candles {
            painter.vline(c.x, c.high.min(c.low)..=c.high.max(c.low), outline);
            let body = Rect::from_x_y_ranges(
                c.x - c.half_width..=c.x + c.half_width,
                c.open.min(c.close)..=c.open.max(c.close).max(c.open.min(c.close) + 1.0),
            );
            if c.rising {
                painter.rect_filled(body, 0.0, Color32::from_rgba_unmultiplied(r, g, b, 60));
            }
            painter.rect_stroke(body, 0.0, outline, egui::StrokeKind::Inside);
        }
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

    fn draw_ghost_anchor(&self, painter: &egui::Painter, base_color: Color32) {
        if let Some(&first) = self.points.first() {
            painter.circle_filled(first, DESIGN_TOKENS.rounding.lg, base_color);
            painter.circle_stroke(
                first,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            // The path fades, but its label must stay readable.
            let chart = painter.ctx().active_theme().chart;
            let font = egui::FontId::proportional(typography::XS);
            let label_bg = Rect::from_center_size(
                Pos2::new(first.x + 35.0, first.y - 12.0),
                egui::vec2(60.0, 16.0),
            );
            painter.rect_filled(label_bg, DESIGN_TOKENS.rounding.sm, chart.bg_tooltip);
            painter.text(
                label_bg.center(),
                egui::Align2::CENTER_CENTER,
                l10n::text(painter.ctx(), "Ghost"),
                font,
                chart.axis_text,
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
            draw_dashed_line(
                painter,
                last,
                proj_end,
                Stroke::new(stroke::HAIRLINE, ghost_color),
                dash,
                gap,
            );
        }
    }
}
