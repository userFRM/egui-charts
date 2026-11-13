//! Text label and anchored text rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use crate::styles::typography::font_size;
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::StrokeKind;

impl Drawing {
    /// Renders Text and Anchored text with the text's top-left corner on the first anchor,
    /// surrounded by a background and border when enabled. Anchored text differs only in
    /// how its anchor follows the pane (`Drawing::update_pane_coords`). Wrapped text breaks at
    /// word boundaries, and inside a word wider than the wrap width. Text that follows the zoom
    /// scales its font, wrap width and box padding with the bar spacing.
    pub(crate) fn render_text_label(&self, painter: &egui::Painter, view: &ChartView) {
        let Some(&pos) = self.points.first() else {
            return;
        };
        let colors = self.text_box_colors(painter.ctx());
        let text = self.display_text(painter.ctx());
        let mut galley = self.annotation_galley(painter, text, colors.text);
        // The font stays within the sizes text is laid out at, and the box keeps its proportions.
        let set = self.annotation_font().size;
        let font = font_size(set * self.text_zoom(view.bar_to_x, pos.x));
        let scale = font / set;
        if self.text_wrap || scale != 1.0 {
            let mut job = (*galley.job).clone();
            for section in &mut job.sections {
                section.format.font_id.size = font;
            }
            if self.text_wrap {
                job.wrap.max_width = self.text_wrap_width * scale;
            }
            galley = painter.layout_job(job);
        }
        if colors.fill.a() > 0 || !colors.border.is_empty() {
            let padding = egui::vec2(
                DESIGN_TOKENS.sizing.annotation.label_padding_x,
                DESIGN_TOKENS.sizing.annotation.text_padding_y,
            ) * scale;
            let box_rect = egui::Rect::from_min_size(pos - padding, galley.size() + padding * 2.0);
            painter.rect(
                box_rect,
                DESIGN_TOKENS.rounding.md * scale,
                colors.fill,
                colors.border,
                StrokeKind::Outside,
            );
        }
        painter.galley(pos, galley, colors.text);
    }
}
