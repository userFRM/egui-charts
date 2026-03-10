//! Price label and price note rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders a price label badge.
    /// Features: colored badge, price display, arrow indicator.
    pub(crate) fn render_price_label(&self, painter: &egui::Painter) {
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

        // Get price from chart_points if available
        let price_text = if !self.chart_points.is_empty() {
            format!("{:.2}", self.chart_points[0].price)
        } else {
            self.text.as_deref().unwrap_or("0.00").to_string()
        };

        let font = egui::FontId::proportional(typography::SM);
        let galley = painter.layout_no_wrap(price_text.clone(), font.clone(), Color32::WHITE);
        let text_size = galley.size();
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );

        // Arrow pointing left
        let arrow_width = DESIGN_TOKENS.sizing.annotation.pointer_size;
        let box_height = text_size.y + padding.y * 2.0;
        let box_rect = Rect::from_min_size(
            Pos2::new(pos.x + arrow_width, pos.y - box_height / 2.0),
            egui::vec2(text_size.x + padding.x * 2.0, box_height),
        );

        // Arrow triangle
        let arrow_points = vec![
            pos,
            Pos2::new(pos.x + arrow_width, pos.y - box_height / 2.0),
            Pos2::new(pos.x + arrow_width, pos.y + box_height / 2.0),
        ];
        let arrow_shape =
            egui::epaint::PathShape::convex_polygon(arrow_points, color, Stroke::NONE);
        painter.add(arrow_shape);

        // Badge rectangle
        painter.rect_filled(box_rect, DESIGN_TOKENS.rounding.sm, color);

        // Price text
        painter.text(
            box_rect.center(),
            egui::Align2::CENTER_CENTER,
            &price_text,
            font,
            Color32::WHITE,
        );
    }

    /// Render price note - horizontal price line with note box
    pub(crate) fn render_price_note(&self, painter: &egui::Painter, rect: Rect) {
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

        // Draw horizontal price line
        let dashed_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
        painter.hline(
            rect.x_range(),
            pos.y,
            Stroke::new(stroke::HAIRLINE, dashed_color),
        );

        // Draw note box
        let text = self.text.as_deref().unwrap_or("Note");
        let font = egui::FontId::proportional(typography::SM);
        let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
        let text_size = galley.size();
        let padding = egui::vec2(8.0, 4.0);
        let box_rect = Rect::from_min_size(pos, text_size + padding * 2.0);

        painter.rect_filled(
            box_rect,
            DESIGN_TOKENS.rounding.sm,
            Color32::from_black_alpha(220),
        );
        painter.rect_stroke(
            box_rect,
            DESIGN_TOKENS.rounding.sm,
            Stroke::new(stroke::HAIRLINE, color),
            StrokeKind::Outside,
        );
        painter.text(pos + padding, egui::Align2::LEFT_TOP, text, font, color);

        // Anchor circle
        painter.circle_filled(pos, DESIGN_TOKENS.rounding.md, color);
    }
}
