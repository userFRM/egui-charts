//! Signpost rendering

use super::labels::facing_point;
use crate::drawings::domain::Drawing;
use crate::drawings::rendering::style::label_text_color;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

/// Height of a label above its bar for a signpost saved with one anchor, in egui logical points.
const DEFAULT_LABEL_HEIGHT: f32 = 40.0;
/// Radius of the disc behind a signpost's glyph, in egui logical points.
const DISC_RADIUS: f32 = 14.0;
/// Side of the square a signpost's glyph is drawn in, in egui logical points.
const GLYPH_SIZE: f32 = 16.0;
/// Space between a signpost's disc and its text, in egui logical points.
const DISC_GAP: f32 = 4.0;

impl Drawing {
    /// Renders a signpost: a label at the second anchor's height, on a thin pole from the first
    /// anchor's bar. A signpost saved with one anchor shows its label above its bar. With
    /// [`AnnotationMarker::show_glyph`](crate::drawings::AnnotationMarker) on, a disc holding the
    /// glyph takes the label's place on the pole and the text sits beyond it, away from the bar.
    pub(crate) fn render_signpost(&self, painter: &egui::Painter) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let color = self.color32();
        let colors = self.text_box_colors(painter.ctx());
        let text = self.display_text(painter.ctx());
        let galley = self.annotation_galley(painter, text, colors.text);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.text_padding_y,
        );
        let height = self
            .points
            .get(1)
            .map_or(anchor.y - DEFAULT_LABEL_HEIGHT, |label| label.y);
        let size = galley.size() + padding * 2.0;
        let centre = Pos2::new(anchor.x, height);
        let plate = if self.marker.show_glyph {
            // Upwards on screen unless the bar's anchor is above the disc.
            let away = if anchor.y < height { 1.0 } else { -1.0 };
            if (anchor.y - height).abs() > DISC_RADIUS {
                painter.line_segment(
                    [anchor, centre - egui::vec2(0.0, away * DISC_RADIUS)],
                    Stroke::new(stroke::HAIRLINE, color),
                );
            }
            let [r, g, b, a] = self.marker.glyph_color.unwrap_or(self.color);
            let disc = Color32::from_rgba_unmultiplied(r, g, b, a);
            painter.circle_filled(centre, DISC_RADIUS, disc);
            let glyph = Rect::from_center_size(centre, egui::Vec2::splat(GLYPH_SIZE));
            let ink = label_text_color(disc);
            if !crate::drawings::glyphs::paint(painter, &self.marker.glyph, glyph, ink, 0.0) {
                painter.text(
                    centre,
                    egui::Align2::CENTER_CENTER,
                    &self.marker.glyph,
                    crate::drawings::glyphs::font_id(painter.ctx(), &self.marker.glyph, GLYPH_SIZE),
                    ink,
                );
            }
            let reach = DISC_RADIUS + DISC_GAP + size.y / 2.0;
            Rect::from_center_size(centre + egui::vec2(0.0, away * reach), size)
        } else {
            let plate = Rect::from_center_size(centre, size);
            if !plate.contains(anchor) {
                painter.line_segment(
                    [anchor, facing_point(plate, anchor)],
                    Stroke::new(stroke::HAIRLINE, color),
                );
            }
            plate
        };
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
