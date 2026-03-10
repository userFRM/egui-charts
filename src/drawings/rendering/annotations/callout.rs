//! Callout and comment rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders a callout with pointer line and auto-positioning.
    /// Features: text box with arrow pointer, background, border.
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
        let font = egui::FontId::proportional(typography::MD);
        let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
        let text_size = galley.size();
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.callout_padding_x,
            DESIGN_TOKENS.sizing.annotation.callout_padding_y,
        );
        let box_rect = Rect::from_min_size(box_pos, text_size + padding * 2.0);

        // Background with subtle gradient effect (darker at top)
        painter.rect_filled(
            box_rect,
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.94),
        );

        // Calculate pointer position (closest edge of box to anchor)
        let box_center = box_rect.center();
        let dx = anchor.x - box_center.x;
        let dy = anchor.y - box_center.y;

        // Find where line from anchor to box center intersects box edge
        let pointer_pos = if dx.abs() / box_rect.width() > dy.abs() / box_rect.height() {
            // Intersects left or right edge
            if dx > 0.0 {
                Pos2::new(box_rect.max.x, box_center.y)
            } else {
                Pos2::new(box_rect.min.x, box_center.y)
            }
        } else {
            // Intersects top or bottom edge
            if dy > 0.0 {
                Pos2::new(box_center.x, box_rect.max.y)
            } else {
                Pos2::new(box_center.x, box_rect.min.y)
            }
        };

        // Draw pointer triangle
        let pointer_size = DESIGN_TOKENS.sizing.annotation.pointer_size;
        let perpendicular = if (pointer_pos.x - box_rect.min.x).abs() < 1.0
            || (pointer_pos.x - box_rect.max.x).abs() < 1.0
        {
            // Left or right edge
            egui::vec2(0.0, pointer_size)
        } else {
            // Top or bottom edge
            egui::vec2(pointer_size, 0.0)
        };

        let pointer_points = vec![
            anchor,
            Pos2::new(
                pointer_pos.x - perpendicular.x / 2.0,
                pointer_pos.y - perpendicular.y / 2.0,
            ),
            Pos2::new(
                pointer_pos.x + perpendicular.x / 2.0,
                pointer_pos.y + perpendicular.y / 2.0,
            ),
        ];
        let pointer_shape = egui::epaint::PathShape::convex_polygon(
            pointer_points,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.94),
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.add(pointer_shape);

        // Border
        painter.rect_stroke(
            box_rect,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, color),
            StrokeKind::Outside,
        );

        // Text
        painter.text(
            Pos2::new(box_pos.x + padding.x, box_pos.y + padding.y),
            egui::Align2::LEFT_TOP,
            text,
            font,
            color,
        );

        // Anchor point
        painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Renders a comment marker with expandable bubble.
    /// Features: speech bubble icon, expandable text, typing indicator.
    pub(crate) fn render_comment(&self, painter: &egui::Painter) {
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

        // Comment bubble icon
        let bubble_width = 28.0;
        let bubble_height = 22.0;
        let bubble_rect = Rect::from_center_size(pos, egui::vec2(bubble_width, bubble_height));

        // Draw rounded bubble
        painter.rect_filled(bubble_rect, DESIGN_TOKENS.rounding.lg, color);

        // Draw tail (triangle pointing down-left)
        let tail_points = vec![
            Pos2::new(bubble_rect.left() + 5.0, bubble_rect.bottom()),
            Pos2::new(bubble_rect.left() - 3.0, bubble_rect.bottom() + 8.0),
            Pos2::new(bubble_rect.left() + 12.0, bubble_rect.bottom()),
        ];
        let tail_shape = egui::epaint::PathShape::convex_polygon(tail_points, color, Stroke::NONE);
        painter.add(tail_shape);

        // Draw dots inside bubble (typing/comment indicator)
        let dot_y = pos.y - 1.0;
        for i in 0..3 {
            let dot_x = pos.x - 7.0 + i as f32 * 7.0;
            painter.circle_filled(Pos2::new(dot_x, dot_y), 2.5, Color32::WHITE);
        }

        // If there's text, show expanded comment
        if let Some(text) = &self.text
            && !text.is_empty()
        {
            let font = egui::FontId::proportional(typography::SM);
            let galley = painter.layout_no_wrap(text.clone(), font.clone(), color);
            let text_size = galley.size();

            let padding = egui::vec2(10.0, 6.0);
            let comment_rect = Rect::from_min_size(
                Pos2::new(
                    pos.x + bubble_width / 2.0 + 8.0,
                    pos.y - text_size.y / 2.0 - padding.y,
                ),
                text_size + padding * 2.0,
            );

            // Connecting line
            painter.line_segment(
                [
                    Pos2::new(bubble_rect.max.x, pos.y),
                    Pos2::new(comment_rect.min.x, comment_rect.center().y),
                ],
                Stroke::new(
                    stroke::HAIRLINE,
                    Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        100,
                    ),
                ),
            );

            // Comment box
            painter.rect_filled(
                comment_rect,
                DESIGN_TOKENS.rounding.md,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.94),
            );
            painter.rect_stroke(
                comment_rect,
                DESIGN_TOKENS.rounding.md,
                Stroke::new(stroke::HAIRLINE, color),
                StrokeKind::Outside,
            );

            // Comment text
            painter.text(
                comment_rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                font,
                color,
            );
        }
    }
}
