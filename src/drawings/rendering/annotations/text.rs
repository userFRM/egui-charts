//! Text label and anchored text rendering

use crate::drawings::domain::Drawing;
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::StrokeKind;

impl Drawing {
    /// Renders Text and Anchored text with the text's top-left corner on the first anchor,
    /// surrounded by a background and border when enabled. Anchored text differs only in
    /// how its anchor follows the pane (`Drawing::update_pane_coords`).
    pub(crate) fn render_text_label(&self, painter: &egui::Painter) {
        let Some(&pos) = self.points.first() else {
            return;
        };
        let colors = self.text_box_colors(painter.ctx());
        let text = self.display_text(painter.ctx());
        let galley = self.annotation_galley(painter, text, colors.text);
        if colors.fill.a() > 0 || !colors.border.is_empty() {
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
        }
        painter.galley(pos, galley, colors.text);
    }
}
