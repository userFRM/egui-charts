//! Text label and anchored text rendering

use crate::drawings::domain::Drawing;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders a text label with customizable styling.
    /// Features: background box, border, multi-line support.
    pub(crate) fn render_text_label(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let text = self.text.as_deref().unwrap_or("Label");

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw background box
        let font = self.annotation_font();
        let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
        let text_size = galley.size();

        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.text_padding_y,
        );
        let box_rect = egui::Rect::from_min_size(pos - padding, text_size + padding * 2.0);

        // Dark semi-transparent background
        painter.rect_filled(
            box_rect,
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.9),
        );
        painter.rect_stroke(
            box_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, color),
            StrokeKind::Outside,
        );

        // Draw text
        painter.text(pos, egui::Align2::LEFT_TOP, text, font, color);
    }

    /// Renders anchored text at its pane-relative screen position.
    pub(crate) fn render_anchored_text(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let text = self.text.as_deref().unwrap_or("Anchored Text");

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Font and sizing
        let font = self.annotation_font();
        let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
        let text_size = galley.size();

        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.signpost_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );
        let prefix_width = painter
            .layout_no_wrap("#".into(), font.clone(), color)
            .size()
            .x;
        let text_offset = prefix_width + DESIGN_TOKENS.spacing.sm;
        let box_rect = egui::Rect::from_min_size(
            pos,
            text_size + padding * 2.0 + egui::vec2(text_offset, 0.0),
        );

        // Background with anchor icon
        painter.rect_filled(
            box_rect,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.rect_stroke(
            box_rect,
            DESIGN_TOKENS.rounding.sm,
            Stroke::new(stroke::HAIRLINE, color),
            StrokeKind::Outside,
        );

        // Anchor icon before text
        painter.text(
            pos + padding,
            egui::Align2::LEFT_TOP,
            "#",
            self.annotation_font(),
            color,
        );

        // Main text
        painter.text(
            Pos2::new(pos.x + padding.x + text_offset, pos.y + padding.y),
            egui::Align2::LEFT_TOP,
            text,
            font,
            color,
        );
    }
}
