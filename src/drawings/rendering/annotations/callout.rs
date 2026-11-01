//! Callout and comment rendering

use super::labels::{facing_point, pointed_outline};
use crate::drawings::domain::Drawing;
use crate::tokens::DESIGN_TOKENS;
use egui::{
    Color32, CornerRadius, Pos2, Rect,
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
        let colors = self.text_box_colors(painter.ctx());

        // Callout box
        let text = self.display_text(painter.ctx());
        let galley = self.annotation_galley(painter, text.into_owned(), colors.text);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.callout_padding_y,
        );
        let box_rect = Rect::from_min_size(box_pos, galley.size() + padding * 2.0);
        let radius = DESIGN_TOKENS.rounding.lg;

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
                fill: colors.fill,
                stroke: colors.border.into(),
            });
        } else {
            painter.rect(
                box_rect,
                radius,
                colors.fill,
                colors.border,
                StrokeKind::Inside,
            );
        }

        painter.galley(box_pos + padding, galley, colors.text);
    }

    /// Renders a comment: its text in one bubble that grows with it, with the square lower left
    /// corner on the anchor.
    pub(crate) fn render_comment(&self, painter: &egui::Painter) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let text = self.display_text(painter.ctx()).into_owned();
        let colors = self.text_box_colors(painter.ctx());
        let galley = self.annotation_galley(painter, text, colors.text);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.callout_padding_y,
        );
        let size = galley.size() + padding * 2.0;
        let bubble = Rect::from_min_size(Pos2::new(anchor.x, anchor.y - size.y), size);
        let radius = DESIGN_TOKENS.rounding.lg as u8;
        painter.add(RectShape::new(
            bubble,
            CornerRadius {
                nw: radius,
                ne: radius,
                sw: 0,
                se: radius,
            },
            colors.fill,
            colors.border,
            StrokeKind::Inside,
        ));
        painter.galley(bubble.min + padding, galley, colors.text);
    }
}
