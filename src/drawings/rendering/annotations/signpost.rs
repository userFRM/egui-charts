//! Signpost rendering

use super::labels::facing_point;
use crate::drawings::domain::Drawing;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Stroke, epaint::StrokeKind};

/// Height of a label above its bar for a signpost saved with one anchor, in egui logical points.
const DEFAULT_LABEL_HEIGHT: f32 = 40.0;

impl Drawing {
    /// Renders a signpost: a label at the second anchor's height, on a thin pole from the first
    /// anchor's bar. A signpost saved with one anchor shows its label above its bar.
    pub(crate) fn render_signpost(&self, painter: &egui::Painter) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let color = self.color32();
        let colors = self.text_box_colors(painter.ctx());
        let text = self.display_text(painter.ctx());
        let galley = self.annotation_galley(painter, text.into_owned(), colors.text);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.text_padding_y,
        );
        let height = self
            .points
            .get(1)
            .map_or(anchor.y - DEFAULT_LABEL_HEIGHT, |label| label.y);
        let plate =
            Rect::from_center_size(Pos2::new(anchor.x, height), galley.size() + padding * 2.0);
        if !plate.contains(anchor) {
            painter.line_segment(
                [anchor, facing_point(plate, anchor)],
                Stroke::new(stroke::HAIRLINE, color),
            );
        }
        painter.rect(
            plate,
            DESIGN_TOKENS.rounding.md,
            colors.fill,
            colors.border,
            StrokeKind::Inside,
        );
        painter.galley(plate.min + padding, galley, colors.text);
    }
}
