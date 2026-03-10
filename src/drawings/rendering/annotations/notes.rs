//! Note, anchored note, and flag note rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders a simple note marker (sticky note style).
    pub(crate) fn render_note(&self, painter: &egui::Painter) {
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

        // Sticky note style (yellow with folded corner)
        let size = DESIGN_TOKENS.sizing.annotation.note_size;
        let note_rect = Rect::from_min_size(pos, egui::vec2(size, size));

        // Main note body - use theme caution color for sticky note yellow
        let note_color = DESIGN_TOKENS.semantic.extended.caution;
        painter.rect_filled(note_rect, 0.0, note_color);

        // Folded corner (triangle)
        let fold_size = DESIGN_TOKENS.sizing.annotation.note_fold_size;
        let fold_points = vec![
            Pos2::new(note_rect.max.x - fold_size, note_rect.min.y),
            Pos2::new(note_rect.max.x, note_rect.min.y),
            Pos2::new(note_rect.max.x, note_rect.min.y + fold_size),
        ];
        let fold_shape = egui::epaint::PathShape::convex_polygon(
            fold_points,
            note_color.gamma_multiply(0.85), // Slightly darker fold
            Stroke::NONE,
        );
        painter.add(fold_shape);

        // Shadow fold
        let shadow_points = vec![
            Pos2::new(note_rect.max.x - fold_size, note_rect.min.y),
            Pos2::new(note_rect.max.x - fold_size, note_rect.min.y + fold_size),
            Pos2::new(note_rect.max.x, note_rect.min.y + fold_size),
        ];
        let shadow_shape = egui::epaint::PathShape::convex_polygon(
            shadow_points,
            Color32::from_rgba_unmultiplied(0, 0, 0, 30),
            Stroke::NONE,
        );
        painter.add(shadow_shape);

        // Border (use user color if not yellow, otherwise use gold variant)
        let border_color = if self.color[0] > 200 && self.color[1] > 200 && self.color[2] < 100 {
            note_color.gamma_multiply(0.8) // Yellow-ish, use darker gold border
        } else {
            color // Use user-specified color
        };
        painter.rect_stroke(
            note_rect,
            0.0,
            Stroke::new(stroke::HAIRLINE, border_color),
            StrokeKind::Outside,
        );

        // Note icon in center - dark text on yellow background
        painter.text(
            note_rect.center(),
            egui::Align2::CENTER_CENTER,
            "N",
            egui::FontId::proportional(typography::MD),
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_text
                .gamma_multiply(0.3),
        );

        // If we have text, show tooltip-style expansion
        if let Some(text) = &self.text
            && !text.is_empty()
        {
            let font = egui::FontId::proportional(typography::XS);
            let galley = painter.layout_no_wrap(text.clone(), font.clone(), Color32::BLACK);
            let text_size = galley.size();

            let tooltip_rect = Rect::from_min_size(
                Pos2::new(
                    note_rect.max.x + DESIGN_TOKENS.spacing.sm + DESIGN_TOKENS.spacing.hairline,
                    note_rect.min.y,
                ),
                text_size
                    + egui::vec2(
                        DESIGN_TOKENS.sizing.annotation.label_padding_x,
                        DESIGN_TOKENS.sizing.annotation.label_padding_y,
                    ),
            );
            painter.rect_filled(tooltip_rect, DESIGN_TOKENS.rounding.sm, note_color);
            painter.rect_stroke(
                tooltip_rect,
                DESIGN_TOKENS.rounding.sm,
                Stroke::new(stroke::HAIRLINE, note_color.gamma_multiply(0.8)),
                StrokeKind::Outside,
            );
            painter.text(
                Pos2::new(tooltip_rect.min.x + 4.0, tooltip_rect.center().y),
                egui::Align2::LEFT_CENTER,
                text,
                font,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_text
                    .gamma_multiply(0.3),
            );
        }
    }

    /// Renders an anchored note with pin marker and expandable content.
    pub(crate) fn render_anchored_note(&self, painter: &egui::Painter) {
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

        // Pin marker style
        let pin_head_radius = 6.0;
        let pin_length = 14.0;

        // Pin needle (line going down from head)
        painter.line_segment(
            [pos, Pos2::new(pos.x, pos.y + pin_length)],
            Stroke::new(stroke::THICK, DESIGN_TOKENS.semantic.extended.disabled),
        );

        // Pin head (circle)
        painter.circle_filled(pos, pin_head_radius, color);
        painter.circle_stroke(
            pos,
            pin_head_radius,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Highlight on pin head
        painter.circle_filled(
            Pos2::new(pos.x - 2.0, pos.y - 2.0),
            2.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 150),
        );

        // If there's text, show expandable note
        if let Some(text) = &self.text
            && !text.is_empty()
        {
            let font = egui::FontId::proportional(typography::SM);
            let galley = painter.layout_no_wrap(text.clone(), font.clone(), color);
            let text_size = galley.size();

            let padding = egui::vec2(8.0, 5.0);
            let note_rect = Rect::from_min_size(
                Pos2::new(
                    pos.x + pin_head_radius + 8.0,
                    pos.y - text_size.y / 2.0 - padding.y,
                ),
                text_size + padding * 2.0,
            );

            // Connecting line from pin to note
            painter.line_segment(
                [pos, Pos2::new(note_rect.min.x, note_rect.center().y)],
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

            // Note background
            painter.rect_filled(
                note_rect,
                DESIGN_TOKENS.rounding.md,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9),
            );
            painter.rect_stroke(
                note_rect,
                DESIGN_TOKENS.rounding.md,
                Stroke::new(stroke::HAIRLINE, color),
                StrokeKind::Outside,
            );

            // Note text
            painter.text(
                note_rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                font,
                color,
            );
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
            let font = egui::FontId::proportional(typography::XS);
            let galley = painter.layout_no_wrap(text.clone(), font.clone(), color);
            let text_size = galley.size();

            let note_rect = Rect::from_min_size(
                Pos2::new(
                    pos.x + flag_width + 5.0,
                    pos.y + flag_height / 2.0 - text_size.y / 2.0 - 3.0,
                ),
                text_size + egui::vec2(8.0, 6.0),
            );

            painter.rect_filled(
                note_rect,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.86),
            );
            painter.rect_stroke(
                note_rect,
                DESIGN_TOKENS.rounding.sm,
                Stroke::new(stroke::HAIRLINE, color),
                StrokeKind::Outside,
            );
            painter.text(
                note_rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                font,
                color,
            );
        }

        // Anchor point at base of pole
        painter.circle_filled(
            Pos2::new(pos.x, pos.y + pole_length),
            DESIGN_TOKENS.rounding.md,
            color,
        );
        painter.circle_stroke(
            Pos2::new(pos.x, pos.y + pole_length),
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }
}
