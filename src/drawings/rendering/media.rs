//! Media placeholder rendering implementations
//!
//! Includes: font icon, image placeholder, tweet placeholder, and idea placeholder.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders an image placeholder with resizable frame.
    /// Features: landscape icon, resize handles, optional caption.
    pub(crate) fn render_image_placeholder(&self, painter: &egui::Painter) {
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

        // Calculate frame size from two points if available
        let (width, height) = if self.points.len() >= 2 {
            let p2 = self.points[1];
            (
                (p2.x - pos.x)
                    .abs()
                    .max(DESIGN_TOKENS.sizing.media.image_min_width),
                (p2.y - pos.y)
                    .abs()
                    .max(DESIGN_TOKENS.sizing.media.image_min_height),
            )
        } else {
            (
                DESIGN_TOKENS.sizing.media.image_default_width,
                DESIGN_TOKENS.sizing.media.image_default_height,
            ) // Default 4:3 aspect ratio
        };

        let frame_rect = Rect::from_min_size(pos, egui::vec2(width, height));

        // Background with subtle pattern (checkerboard for transparency indication)
        painter.rect_filled(
            frame_rect,
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.9),
        );

        // Checkerboard pattern (simplified)
        let check_size = DESIGN_TOKENS.sizing.media.checkerboard_size;
        let check_color = DESIGN_TOKENS.semantic.extended.gray.gamma_multiply(0.8);
        let cols = (width / check_size) as i32;
        let rows = (height / check_size) as i32;
        for row in 0..rows {
            for col in 0..cols {
                if (row + col) % 2 == 0 {
                    let check_rect = Rect::from_min_size(
                        Pos2::new(
                            pos.x + col as f32 * check_size,
                            pos.y + row as f32 * check_size,
                        ),
                        egui::vec2(check_size, check_size),
                    );
                    if frame_rect.contains_rect(check_rect) {
                        painter.rect_filled(check_rect, 0.0, check_color);
                    }
                }
            }
        }

        // Border
        painter.rect_stroke(
            frame_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::THICK, color),
            StrokeKind::Outside,
        );

        // Image icon (landscape with sun and mountains)
        let icon_rect = Rect::from_center_size(frame_rect.center(), egui::vec2(50.0, 35.0));
        let icon_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);

        // Sun
        painter.circle_filled(
            Pos2::new(icon_rect.right() - 10.0, icon_rect.top() + 8.0),
            6.0,
            DESIGN_TOKENS.semantic.extended.favorite_gold,
        );

        // Mountains (filled triangles)
        let mountain1 = vec![
            Pos2::new(icon_rect.left(), icon_rect.bottom()),
            Pos2::new(icon_rect.left() + 18.0, icon_rect.center().y - 5.0),
            Pos2::new(icon_rect.left() + 36.0, icon_rect.bottom()),
        ];
        let mountain1_shape = egui::epaint::PathShape::convex_polygon(
            mountain1,
            DESIGN_TOKENS.semantic.extended.success.gamma_multiply(0.6),
            Stroke::NONE,
        );
        painter.add(mountain1_shape);

        let mountain2 = vec![
            Pos2::new(icon_rect.center().x - 5.0, icon_rect.bottom()),
            Pos2::new(icon_rect.center().x + 15.0, icon_rect.center().y),
            Pos2::new(icon_rect.right(), icon_rect.bottom()),
        ];
        let mountain2_shape = egui::epaint::PathShape::convex_polygon(
            mountain2,
            DESIGN_TOKENS.semantic.extended.success.gamma_multiply(0.5),
            Stroke::NONE,
        );
        painter.add(mountain2_shape);

        // "Drop image here" text
        let text = if self.text.is_some() { "IMG" } else { "Image" };
        painter.text(
            Pos2::new(frame_rect.center().x, frame_rect.max.y - 12.0),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(typography::XS),
            icon_color,
        );

        // Resize handles at corners
        let handle_size = DESIGN_TOKENS.rounding.lg;
        let handles = [
            frame_rect.left_top(),
            frame_rect.right_top(),
            frame_rect.left_bottom(),
            frame_rect.right_bottom(),
        ];
        for handle_pos in handles {
            painter.rect_filled(
                Rect::from_center_size(handle_pos, egui::vec2(handle_size, handle_size)),
                stroke::HAIRLINE,
                color,
            );
        }
    }

    /// Renders a Tweet/X post placeholder card.
    /// Features: X logo, profile placeholder, text lines, engagement icons.
    pub(crate) fn render_tweet_placeholder(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let twitter_blue = DESIGN_TOKENS.semantic.extended.info;
        let x_black = Color32::BLACK;

        // Card dimensions
        let card_width = DESIGN_TOKENS.sizing.media.tweet_card_width;
        let card_height = DESIGN_TOKENS.sizing.media.tweet_card_height;
        let card_rect = Rect::from_min_size(pos, egui::vec2(card_width, card_height));

        // Card background
        painter.rect_filled(card_rect, DESIGN_TOKENS.rounding.xl, Color32::WHITE);
        painter.rect_stroke(
            card_rect,
            DESIGN_TOKENS.rounding.xl,
            Stroke::new(stroke::HAIRLINE, DESIGN_TOKENS.semantic.extended.light_gray),
            StrokeKind::Outside,
        );

        // X logo in top right
        painter.text(
            Pos2::new(card_rect.max.x - 15.0, card_rect.min.y + 12.0),
            egui::Align2::CENTER_CENTER,
            "𝕏",
            egui::FontId::proportional(typography::XXL),
            x_black,
        );

        // Profile picture placeholder
        let profile_center = Pos2::new(card_rect.min.x + 25.0, card_rect.min.y + 25.0);
        painter.circle_filled(
            profile_center,
            DESIGN_TOKENS.sizing.media.profile_radius,
            DESIGN_TOKENS.semantic.extended.light_gray,
        );
        painter.circle_stroke(
            profile_center,
            DESIGN_TOKENS.sizing.media.profile_radius,
            Stroke::new(stroke::HAIRLINE, DESIGN_TOKENS.semantic.extended.disabled),
        );

        // Username and handle
        let text_x = card_rect.min.x + 50.0;
        painter.text(
            Pos2::new(text_x, card_rect.min.y + 15.0),
            egui::Align2::LEFT_TOP,
            "Username",
            egui::FontId::proportional(typography::LG),
            x_black,
        );
        painter.text(
            Pos2::new(text_x, card_rect.min.y + 30.0),
            egui::Align2::LEFT_TOP,
            "@handle",
            egui::FontId::proportional(typography::SM),
            DESIGN_TOKENS.semantic.extended.gray,
        );

        // Tweet text placeholder lines
        let line_color = DESIGN_TOKENS.semantic.extended.disabled;
        let line_widths = [200.0, 180.0, 120.0];
        for (i, width) in line_widths.iter().enumerate() {
            let y = card_rect.min.y + 50.0 + i as f32 * 14.0;
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(text_x, y), egui::vec2(*width, 8.0)),
                DESIGN_TOKENS.rounding.sm,
                line_color,
            );
        }

        // Engagement icons at bottom (using text labels)
        let icon_y = card_rect.max.y - 15.0;
        let icons = ["Reply", "Share", "Like", "Stats"];
        let icon_spacing = 50.0;
        for (i, icon) in icons.iter().enumerate() {
            painter.text(
                Pos2::new(text_x + i as f32 * icon_spacing, icon_y),
                egui::Align2::LEFT_CENTER,
                *icon,
                egui::FontId::proportional(typography::XS),
                DESIGN_TOKENS.semantic.extended.gray,
            );
        }

        // Anchor point
        painter.circle_filled(pos, DESIGN_TOKENS.rounding.md, twitter_blue);
    }

    /// Renders an idea placeholder badge.
    /// Features: lightbulb icon, branding, link indicator.
    pub(crate) fn render_idea_placeholder(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let accent = DESIGN_TOKENS.semantic.extended.accent;

        // Card for idea
        let card_width = DESIGN_TOKENS.sizing.media.idea_card_width;
        let card_height = DESIGN_TOKENS.sizing.media.idea_card_height;
        let card_rect = Rect::from_min_size(pos, egui::vec2(card_width, card_height));

        // Card background
        painter.rect_filled(
            card_rect,
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.94),
        );
        painter.rect_stroke(
            card_rect,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, accent),
            StrokeKind::Outside,
        );

        // Lightbulb icon (left side)
        let bulb_center = Pos2::new(card_rect.min.x + 30.0, card_rect.center().y);
        let bulb_radius = DESIGN_TOKENS.sizing.media.profile_radius;

        // Bulb glow effect
        painter.circle_filled(
            bulb_center,
            bulb_radius + 4.0,
            DESIGN_TOKENS.semantic.extended.caution.gamma_multiply(0.12), // Subtle glow
        );

        // Bulb body
        painter.circle_filled(
            bulb_center,
            bulb_radius,
            DESIGN_TOKENS.semantic.extended.caution,
        );

        // Bulb base
        let base_rect = Rect::from_center_size(
            Pos2::new(bulb_center.x, bulb_center.y + bulb_radius + 5.0),
            egui::vec2(12.0, 8.0),
        );
        painter.rect_filled(
            base_rect,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS.semantic.extended.disabled,
        );

        // Screw lines on base
        for i in 0..3 {
            let y = base_rect.min.y + 2.0 + i as f32 * 2.5;
            painter.hline(
                egui::Rangef::new(base_rect.min.x + 2.0, base_rect.max.x - 2.0),
                y,
                Stroke::new(stroke::HAIRLINE, DESIGN_TOKENS.semantic.extended.gray),
            );
        }

        // "IDEA" text
        let text_x = card_rect.min.x + 60.0;
        painter.text(
            Pos2::new(text_x, card_rect.min.y + 20.0),
            egui::Align2::LEFT_TOP,
            "Trading Idea",
            egui::FontId::proportional(typography::LG),
            Color32::WHITE,
        );

        // Description placeholder
        painter.text(
            Pos2::new(text_x, card_rect.min.y + 40.0),
            egui::Align2::LEFT_TOP,
            "Click to view analysis",
            egui::FontId::proportional(typography::SM),
            DESIGN_TOKENS.semantic.extended.gray,
        );

        // Brand logo text
        painter.text(
            Pos2::new(text_x, card_rect.max.y - 12.0),
            egui::Align2::LEFT_CENTER,
            "Chart",
            egui::FontId::proportional(typography::XS),
            accent,
        );

        // Link icon
        painter.text(
            Pos2::new(card_rect.max.x - 20.0, card_rect.center().y),
            egui::Align2::CENTER_CENTER,
            ">",
            egui::FontId::proportional(typography::XXL),
            DESIGN_TOKENS.semantic.extended.gray,
        );

        // Anchor point
        painter.circle_filled(pos, DESIGN_TOKENS.rounding.md, accent);
        painter.circle_stroke(
            pos,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Renders a font icon (emoji/sticker).
    /// Features: scalable icon, optional background, anchor point.
    pub(crate) fn render_font_icon(&self, painter: &egui::Painter) {
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

        // Default icon if none specified
        let icon = self.text.as_deref().unwrap_or("*");

        // Icon size based on stroke_width or default
        let icon_size = (self.stroke_width * 8.0).clamp(24.0, 72.0);

        // Optional background circle
        let has_bg = self.stroke_width > 2.0;
        if has_bg {
            painter.circle_filled(
                pos,
                icon_size / 2.0 + DESIGN_TOKENS.rounding.lg,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.78),
            );
            painter.circle_stroke(
                pos,
                icon_size / 2.0 + DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, color),
            );
        }

        // Icon
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(icon_size),
            color,
        );

        // Small anchor indicator
        if !has_bg {
            painter.circle_filled(
                Pos2::new(pos.x, pos.y + icon_size / 2.0 + 5.0),
                DESIGN_TOKENS.rounding.sm,
                color,
            );
        }
    }
}
