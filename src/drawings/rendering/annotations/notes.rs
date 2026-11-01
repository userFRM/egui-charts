//! Note, pin, and flag note rendering

use super::labels::paint_anchor_line;
use crate::drawings::domain::Drawing;
use crate::styles::stroke;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Galley, Pos2, Rect, Stroke, epaint::StrokeKind};
use std::sync::Arc;

/// Radius of a pin's head, in egui logical points.
const PIN_HEAD_RADIUS: f32 = 6.0;
/// Height of a pin's head centre above its tip, in egui logical points.
const PIN_HEAD_HEIGHT: f32 = 16.0;
/// Height of a pin's popup above its tip: the top of the head and a 4 px gap.
const PIN_POPUP_OFFSET: f32 = PIN_HEAD_HEIGHT + PIN_HEAD_RADIUS + 4.0;

impl Drawing {
    /// Paints a card holding `galley` in the colours of [`Drawing::text_box_colors`].
    fn paint_card(&self, painter: &egui::Painter, rect: Rect, galley: Arc<Galley>) {
        let colors = self.text_box_colors(painter.ctx());
        painter.rect(
            rect,
            DESIGN_TOKENS.rounding.sm,
            colors.fill,
            colors.border,
            StrokeKind::Inside,
        );
        painter.galley(rect.min + card_padding(), galley, colors.text);
    }

    /// Renders a note: a label at the second anchor, joined to the anchor it annotates by a thin
    /// line. A note saved with one anchor shows its label there.
    pub(crate) fn render_note(&self, painter: &egui::Painter) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let text = self.display_text(painter.ctx());
        let color = self.color32();
        let galley =
            self.annotation_galley(painter, text, self.text_box_colors(painter.ctx()).text);
        let corner = self.points.get(1).copied().unwrap_or(anchor);
        let label = Rect::from_min_size(corner, galley.size() + card_padding() * 2.0);
        paint_anchor_line(painter, label, anchor, color);
        self.paint_card(painter, label, galley);
    }

    /// Renders a pin: a map-pin marker with its tip on the anchor, and its text in a popup above
    /// it while selected.
    pub(crate) fn render_anchored_note(&self, painter: &egui::Painter, selected: bool) {
        let Some(&tip) = self.points.first() else {
            return;
        };
        let color = self.color32();
        // The sides are tangent to the head, which the outline follows over the top.
        let head = tip - egui::vec2(0.0, PIN_HEAD_HEIGHT);
        let tangent = (PIN_HEAD_RADIUS / PIN_HEAD_HEIGHT).acos();
        let outline = std::iter::once(tip)
            .chain((0..=15).map(|step| {
                let angle = tangent + (std::f32::consts::TAU - 2.0 * tangent) * step as f32 / 15.0;
                head + PIN_HEAD_RADIUS * egui::vec2(-angle.sin(), angle.cos())
            }))
            .collect();
        painter.add(egui::epaint::PathShape::convex_polygon(
            outline,
            color,
            Stroke::NONE,
        ));
        painter.circle_filled(head, 2.0, painter.ctx().active_theme().chart.bg);

        if selected && let Some(text) = self.text.as_deref().filter(|text| !text.is_empty()) {
            let text_color = self.text_box_colors(painter.ctx()).text;
            let galley = self.annotation_galley(painter, text.to_owned(), text_color);
            let size = galley.size() + card_padding() * 2.0;
            let popup = Rect::from_min_size(
                Pos2::new(tip.x - size.x / 2.0, tip.y - PIN_POPUP_OFFSET - size.y),
                size,
            );
            self.paint_card(painter, popup, galley);
        }
    }

    /// Renders a flag marker with attached note.
    /// Features: flag pole, waving flag, optional text note.
    pub(crate) fn render_flag_note(&self, painter: &egui::Painter) {
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

        // Flag pole (extends down from anchor)
        let pole_length = 35.0;
        painter.line_segment(
            [pos, Pos2::new(pos.x, pos.y + pole_length)],
            Stroke::new(
                stroke::THICK,
                DESIGN_TOKENS.semantic.extended.chart_text_secondary,
            ),
        );

        // Flag (waving pennant style)
        let flag_width = 25.0;
        let flag_height = 15.0;
        let wave_offset = 3.0;

        let flag_points = vec![
            pos,
            Pos2::new(pos.x + flag_width * 0.3, pos.y + wave_offset),
            Pos2::new(pos.x + flag_width * 0.7, pos.y - wave_offset),
            Pos2::new(pos.x + flag_width, pos.y + flag_height / 2.0 + wave_offset),
            Pos2::new(pos.x + flag_width * 0.7, pos.y + flag_height + wave_offset),
            Pos2::new(pos.x + flag_width * 0.3, pos.y + flag_height - wave_offset),
            Pos2::new(pos.x, pos.y + flag_height),
        ];
        let flag_shape = egui::epaint::PathShape::convex_polygon(
            flag_points,
            color,
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.add(flag_shape);

        // Flag pole cap (small circle at top)
        painter.circle_filled(
            pos,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS.semantic.extended.chart_text_muted,
        );

        // If there's text, show it as a note attached to the flag
        if let Some(text) = &self.text
            && !text.is_empty()
        {
            let colors = self.text_box_colors(painter.ctx());
            let galley = self.annotation_galley(painter, text.clone(), colors.text);
            let text_size = galley.size();

            let note_rect = Rect::from_min_size(
                Pos2::new(
                    pos.x + flag_width + 5.0,
                    pos.y + flag_height / 2.0 - text_size.y / 2.0 - 3.0,
                ),
                text_size + egui::vec2(8.0, 6.0),
            );

            painter.rect(
                note_rect,
                DESIGN_TOKENS.rounding.sm,
                colors.fill,
                colors.border,
                StrokeKind::Outside,
            );
            painter.galley(
                egui::Align2::CENTER_CENTER
                    .anchor_size(note_rect.center(), text_size)
                    .min,
                galley,
                colors.text,
            );
        }
    }
}

fn card_padding() -> egui::Vec2 {
    egui::vec2(
        DESIGN_TOKENS.sizing.annotation.label_padding_x,
        DESIGN_TOKENS.sizing.annotation.label_padding_y,
    )
}
