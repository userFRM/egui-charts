//! Signpost rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders a vertical signpost with label at top.
    /// Features: vertical line to chart top, sign plate, date/event text.
    pub(crate) fn render_signpost(&self, painter: &egui::Painter, rect: Rect) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw vertical line from point to top of chart (dashed)
        let dash = DESIGN_TOKENS.sizing.annotation.signpost_padding_x;
        let gap = DESIGN_TOKENS.sizing.annotation.signpost_padding_y;
        let mut y = rect.min.y;
        while y < pos.y {
            let end_y = (y + dash).min(pos.y);
            painter.line_segment(
                [Pos2::new(pos.x, y), Pos2::new(pos.x, end_y)],
                Stroke::new(stroke::THICK, color),
            );
            y += dash + gap;
        }

        // Draw sign plate at top
        let text = self.text.as_deref().unwrap_or("Event");
        let font = egui::FontId::proportional(typography::SM);
        let galley = painter.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
        let text_size = galley.size();
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.text_padding_y,
        );
        let sign_rect = Rect::from_min_size(
            Pos2::new(
                pos.x - text_size.x / 2.0 - padding.x,
                rect.min.y + DESIGN_TOKENS.spacing.lg,
            ),
            text_size + padding * 2.0,
        );

        // Sign plate with arrow pointing down
        painter.rect_filled(sign_rect, DESIGN_TOKENS.rounding.md, color);

        // Arrow below sign pointing to line
        let arrow_size = DESIGN_TOKENS.sizing.annotation.signpost_padding_x;
        let arrow_points = vec![
            Pos2::new(pos.x, sign_rect.max.y + arrow_size),
            Pos2::new(pos.x - arrow_size, sign_rect.max.y),
            Pos2::new(pos.x + arrow_size, sign_rect.max.y),
        ];
        let arrow_shape =
            egui::epaint::PathShape::convex_polygon(arrow_points, color, Stroke::NONE);
        painter.add(arrow_shape);

        // Sign text
        painter.text(
            sign_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            font,
            Color32::WHITE,
        );

        // Date label if we have chart point data
        if !self.chart_points.is_empty() {
            let bar_idx = self.chart_points[0].bar_idx as i64;
            let date_text = format!("Bar {}", bar_idx);
            let date_font = egui::FontId::proportional(typography::XS);
            painter.text(
                Pos2::new(
                    pos.x,
                    sign_rect.max.y + arrow_size + DESIGN_TOKENS.sizing.annotation.signpost_bracket,
                ),
                egui::Align2::CENTER_TOP,
                date_text,
                date_font,
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150),
            );
        }

        // Anchor point at position
        painter.circle_filled(pos, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            pos,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }
}
