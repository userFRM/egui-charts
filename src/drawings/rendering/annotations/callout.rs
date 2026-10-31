//! Callout and comment rendering

use super::labels::{facing_point, label_text_color, pointed_outline};
use crate::drawings::domain::Drawing;
use crate::localization as l10n;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{
    Color32, CornerRadius, Pos2, Rect, Stroke,
    epaint::{PathShape, RectShape, StrokeKind},
};

impl Drawing {
    /// Renders a callout: a text box at the second anchor whose pointer reaches the first, filled
    /// and outlined as one shape. An anchor at the box needs no pointer.
    pub(crate) fn render_callout(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            if self.points.len() == 1 {
                // Preview mode
                let color = Color32::from_rgba_unmultiplied(
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    self.color[3],
                );
                painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
            }
            return;
        }
        let anchor = self.points[0];
        let box_pos = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Callout box
        let text = self.text.as_deref().unwrap_or("Callout text here");
        let galley = painter.layout_no_wrap(text.to_string(), self.annotation_font(), color);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.callout_padding_y,
        );
        let box_rect = Rect::from_min_size(box_pos, galley.size() + padding * 2.0);
        let radius = DESIGN_TOKENS.rounding.lg;
        let fill = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.94);
        let stroke = Stroke::new(stroke::HAIRLINE, color);

        // The pointer's base stays on the straight part of the side facing the anchor.
        let mid = facing_point(box_rect, anchor);
        let (along, side) = if mid.y == box_rect.center().y {
            (egui::Vec2::Y, box_rect.height())
        } else {
            (egui::Vec2::X, box_rect.width())
        };
        let half_base = DESIGN_TOKENS
            .sizing
            .annotation
            .pointer_size
            .min((side - 2.0 * radius) / 2.0);
        if half_base >= 1.0 && !box_rect.expand(4.0).contains(anchor) {
            painter.add(PathShape {
                points: pointed_outline(
                    box_rect,
                    radius,
                    mid - along * half_base,
                    anchor,
                    mid + along * half_base,
                ),
                closed: true,
                fill,
                stroke: stroke.into(),
            });
        } else {
            painter.rect(box_rect, radius, fill, stroke, StrokeKind::Inside);
        }

        painter.galley(box_pos + padding, galley, color);
    }

    /// Renders a comment: its text in one bubble that grows with it, with the square lower left
    /// corner on the anchor.
    pub(crate) fn render_comment(&self, painter: &egui::Painter) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let text = match self.text.as_deref() {
            Some(text) if !text.is_empty() => text.to_owned(),
            _ => l10n::text(painter.ctx(), "Text"),
        };
        let fill = self.fill_color32().unwrap_or(self.color32());
        let text_color = label_text_color(fill);
        let galley = painter.layout_no_wrap(text, self.annotation_font(), text_color);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.callout_padding_y,
        );
        let size = galley.size() + padding * 2.0;
        let bubble = Rect::from_min_size(Pos2::new(anchor.x, anchor.y - size.y), size);
        let radius = DESIGN_TOKENS.rounding.lg as u8;
        painter.add(RectShape::filled(
            bubble,
            CornerRadius {
                nw: radius,
                ne: radius,
                sw: 0,
                se: radius,
            },
            fill,
        ));
        painter.galley(bubble.min + padding, galley, text_color);
    }
}
