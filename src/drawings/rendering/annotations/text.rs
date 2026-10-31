//! Text label and anchored text rendering

use crate::drawings::domain::Drawing;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, epaint::StrokeKind};

impl Drawing {
    /// Renders a text label with customizable styling.
    /// Features: background box, border, multi-line support.
    pub(crate) fn render_text_label(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let text = self.text.as_deref().unwrap_or("Label");
        let colors = self.text_box_colors(painter.ctx());
        let galley = self.annotation_galley(painter, text.to_owned(), colors.text);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.text_padding_y,
        );
        let box_rect = egui::Rect::from_min_size(pos - padding, galley.size() + padding * 2.0);
        painter.rect(
            box_rect,
            DESIGN_TOKENS.rounding.md,
            colors.fill,
            colors.border,
            StrokeKind::Outside,
        );
        painter.galley(pos, galley, colors.text);
    }

    /// Renders anchored text at its pane-relative screen position.
    pub(crate) fn render_anchored_text(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let text = self.text.as_deref().unwrap_or("Anchored Text");
        let colors = self.text_box_colors(painter.ctx());
        let galley = self.annotation_galley(painter, text.to_owned(), colors.text);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.signpost_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );
        // Anchor icon before the text
        let prefix = self.annotation_galley(painter, "#".into(), colors.text);
        let text_offset = prefix.size().x + DESIGN_TOKENS.spacing.sm;
        let box_rect = egui::Rect::from_min_size(
            pos,
            galley.size() + padding * 2.0 + egui::vec2(text_offset, 0.0),
        );
        painter.rect(
            box_rect,
            DESIGN_TOKENS.rounding.sm,
            colors.fill,
            colors.border,
            StrokeKind::Outside,
        );
        painter.galley(pos + padding, prefix, colors.text);
        painter.galley(
            Pos2::new(pos.x + padding.x + text_offset, pos.y + padding.y),
            galley,
            colors.text,
        );
    }
}
