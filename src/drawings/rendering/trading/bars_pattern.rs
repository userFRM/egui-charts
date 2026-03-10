//! Bars pattern rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders Bars Pattern for copying and projecting historical patterns.
    /// Features: source selection, mirror option, scaling.
    pub(crate) fn render_bars_pattern(&self, painter: &egui::Painter, _chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }

        let source_start = self.points[0];
        let source_end = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Source region box
        let source_rect = Rect::from_two_pos(source_start, source_end);
        self.draw_source_region(painter, source_rect, color);

        // If we have a third point, draw the projection
        if self.points.len() >= 3 {
            let proj_anchor = self.points[2];
            self.draw_pattern_projection(painter, source_rect, proj_anchor, color);
        }

        // Source anchor points
        self.draw_pattern_anchors(painter, source_start, source_end, color);
    }

    fn draw_source_region(&self, painter: &egui::Painter, source_rect: Rect, color: Color32) {
        let source_fill =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);
        painter.rect_filled(source_rect, 0.0, source_fill);
        painter.rect_stroke(
            source_rect,
            0.0,
            Stroke::new(stroke::HAIRLINE, color),
            StrokeKind::Outside,
        );

        // Source label
        let font = egui::FontId::proportional(typography::XS);
        let source_label_bg = Rect::from_center_size(
            Pos2::new(source_rect.center().x, source_rect.min.y - 12.0),
            egui::vec2(60.0, 16.0),
        );
        painter.rect_filled(source_label_bg, DESIGN_TOKENS.rounding.sm, color);
        painter.text(
            source_label_bg.center(),
            egui::Align2::CENTER_CENTER,
            "SOURCE",
            font,
            Color32::WHITE,
        );
    }

    fn draw_pattern_projection(
        &self,
        painter: &egui::Painter,
        source_rect: Rect,
        proj_anchor: Pos2,
        color: Color32,
    ) {
        let font = egui::FontId::proportional(typography::XS);

        // Calculate projection rectangle (same size as source)
        let proj_width = source_rect.width();
        let proj_height = source_rect.height();
        let proj_rect = Rect::from_min_size(proj_anchor, egui::vec2(proj_width, proj_height));

        // Projected region (more transparent)
        let proj_fill =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 40);
        painter.rect_filled(proj_rect, 0.0, proj_fill);

        // Draw pattern replication (simulated bars)
        self.draw_simulated_bars(painter, proj_rect);

        // Dashed border for projection
        let proj_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        self.draw_dashed_rect(painter, proj_rect, proj_color);

        // Projection label
        let proj_label_bg = Rect::from_center_size(
            Pos2::new(proj_rect.center().x, proj_rect.min.y - 12.0),
            egui::vec2(80.0, 16.0),
        );
        painter.rect_filled(proj_label_bg, DESIGN_TOKENS.rounding.sm, proj_color);
        painter.text(
            proj_label_bg.center(),
            egui::Align2::CENTER_CENTER,
            "PROJECTION",
            font,
            Color32::WHITE,
        );

        // Projection anchor point
        painter.circle_filled(proj_anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            proj_anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Connection arrow from source to projection
        self.draw_connection_arrow(painter, source_rect, proj_rect);
    }

    fn draw_simulated_bars(&self, painter: &egui::Painter, proj_rect: Rect) {
        let num_bars = 12;
        let bar_width = proj_rect.width() / num_bars as f32;
        for i in 0..num_bars {
            let x = proj_rect.min.x + bar_width * i as f32 + bar_width / 2.0;
            // Simulate OHLC pattern from source
            let progress = i as f32 / num_bars as f32;
            let height_factor = 0.3 + 0.4 * (progress * std::f32::consts::PI * 2.0).sin().abs();
            let bar_height = proj_rect.height() * height_factor * 0.8;
            let bar_y = proj_rect.center().y - bar_height / 2.0;

            let bar_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(x - bar_width * 0.3, bar_y),
                    egui::vec2(bar_width * 0.6, bar_height),
                ),
                0.0,
                bar_color,
            );
        }
    }

    fn draw_dashed_rect(&self, painter: &egui::Painter, rect: Rect, color: Color32) {
        let dash = 6.0;
        let gap = 4.0;

        // Top edge
        let mut x = rect.min.x;
        while x < rect.max.x {
            let end_x = (x + dash).min(rect.max.x);
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(end_x, rect.min.y)],
                Stroke::new(stroke::HAIRLINE, color),
            );
            x += dash + gap;
        }

        // Bottom edge
        x = rect.min.x;
        while x < rect.max.x {
            let end_x = (x + dash).min(rect.max.x);
            painter.line_segment(
                [Pos2::new(x, rect.max.y), Pos2::new(end_x, rect.max.y)],
                Stroke::new(stroke::HAIRLINE, color),
            );
            x += dash + gap;
        }

        // Left edge
        let mut y = rect.min.y;
        while y < rect.max.y {
            let end_y = (y + dash).min(rect.max.y);
            painter.line_segment(
                [Pos2::new(rect.min.x, y), Pos2::new(rect.min.x, end_y)],
                Stroke::new(stroke::HAIRLINE, color),
            );
            y += dash + gap;
        }

        // Right edge
        y = rect.min.y;
        while y < rect.max.y {
            let end_y = (y + dash).min(rect.max.y);
            painter.line_segment(
                [Pos2::new(rect.max.x, y), Pos2::new(rect.max.x, end_y)],
                Stroke::new(stroke::HAIRLINE, color),
            );
            y += dash + gap;
        }
    }

    fn draw_connection_arrow(&self, painter: &egui::Painter, source_rect: Rect, proj_rect: Rect) {
        let arrow_start = Pos2::new(source_rect.max.x + 10.0, source_rect.center().y);
        let arrow_end = Pos2::new(proj_rect.min.x - 10.0, proj_rect.center().y);
        let arrow_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);

        // Dashed arrow line
        let dash = 6.0;
        let gap = 4.0;
        let mut ax = arrow_start.x;
        while ax < arrow_end.x - 10.0 {
            let end_ax = (ax + dash).min(arrow_end.x - 10.0);
            painter.line_segment(
                [
                    Pos2::new(ax, arrow_start.y),
                    Pos2::new(end_ax, arrow_start.y),
                ],
                Stroke::new(stroke::HAIRLINE, arrow_color),
            );
            ax += dash + gap;
        }

        // Arrow head
        let arrow_size = 8.0;
        painter.line_segment(
            [
                arrow_end,
                Pos2::new(arrow_end.x - arrow_size, arrow_end.y - arrow_size * 0.6),
            ],
            Stroke::new(stroke::MEDIUM, arrow_color),
        );
        painter.line_segment(
            [
                arrow_end,
                Pos2::new(arrow_end.x - arrow_size, arrow_end.y + arrow_size * 0.6),
            ],
            Stroke::new(stroke::MEDIUM, arrow_color),
        );
    }

    fn draw_pattern_anchors(
        &self,
        painter: &egui::Painter,
        source_start: Pos2,
        source_end: Pos2,
        color: Color32,
    ) {
        painter.circle_filled(source_start, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            source_start,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(source_end, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            source_end,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }
}
