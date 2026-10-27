//! Media rendering implementations
//!
//! Includes: font icon, image, post and idea. An image drawing's text is the URI of its image,
//! resolved by the egui image loaders the host installs; a post or idea drawing's text is the
//! content the host or user supplies, its first line naming the author or the title.

use crate::drawings::domain::Drawing;
use crate::painting::dashed_line::finite_clip;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind, load::TexturePoll};

impl Drawing {
    /// Renders the image whose URI is the drawing's text, fitted inside the frame between its
    /// anchors, or the empty frame with a line saying that the image is missing, loading or could
    /// not be loaded.
    pub(crate) fn render_image(&self, painter: &egui::Painter) {
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
        let origin = self
            .points
            .get(1)
            .map_or(pos, |p2| Pos2::new(pos.x.min(p2.x), pos.y.min(p2.y)));
        let frame_rect = Rect::from_min_size(origin, egui::vec2(width, height));

        // The host's image loaders resolve the URI; they repaint once a pending load finishes.
        let uri = self
            .text
            .as_deref()
            .map(str::trim)
            .filter(|uri| !uri.is_empty());
        let poll = uri.map(|uri| {
            painter.ctx().try_load_texture(
                uri,
                egui::TextureOptions::LINEAR,
                egui::SizeHint::default(),
            )
        });
        let status = match poll {
            Some(Ok(TexturePoll::Ready { texture })) if texture.size.min_elem() > 0.0 => {
                let scale =
                    (frame_rect.width() / texture.size.x).min(frame_rect.height() / texture.size.y);
                let rect = Rect::from_center_size(frame_rect.center(), texture.size * scale);
                painter.image(
                    texture.id,
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(stroke::HAIRLINE, color),
                    StrokeKind::Outside,
                );
                return;
            }
            None => "Double-click to add an image",
            Some(Ok(TexturePoll::Pending { .. })) => "Loading image…",
            Some(_) => "Image could not be loaded",
        };

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

        // Checkerboard pattern (simplified), only the squares the painter can show: the anchors may
        // make the frame far larger than the screen.
        let check_size = DESIGN_TOKENS.sizing.media.checkerboard_size;
        let check_color = DESIGN_TOKENS.semantic.extended.gray.gamma_multiply(0.8);
        let cols = (width / check_size) as i32;
        let rows = (height / check_size) as i32;
        let visible = finite_clip(painter).map_or(Rect::NOTHING, |clip| frame_rect.intersect(clip));
        // Squares overlapping `from..to` px from the frame's corner, within `0..count`; none when
        // nothing is visible.
        let span = |from: f32, to: f32, count: i32| {
            (from / check_size).floor().max(0.0) as i32
                ..((to / check_size).ceil() as i32).min(count)
        };
        let corner = frame_rect.min;
        for row in span(visible.min.y - corner.y, visible.max.y - corner.y, rows) {
            for col in span(visible.min.x - corner.x, visible.max.x - corner.x, cols) {
                if row % 2 == col % 2 {
                    let check_rect = Rect::from_min_size(
                        Pos2::new(
                            corner.x + col as f32 * check_size,
                            corner.y + row as f32 * check_size,
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

        let status = painter.layout(
            status.to_owned(),
            egui::FontId::proportional(typography::XS),
            icon_color,
            frame_rect.width() - 2.0 * DESIGN_TOKENS.spacing.sm,
        );
        painter.galley(
            Pos2::new(
                frame_rect.center().x - status.size().x / 2.0,
                frame_rect.max.y - DESIGN_TOKENS.spacing.sm - status.size().y,
            ),
            status,
            icon_color,
        );
    }

    /// Renders a post card from the drawing's text: its first line as the author and the
    /// following lines as the post, wrapped to the card, which grows to fit them.
    pub(crate) fn render_post(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let accent = DESIGN_TOKENS.semantic.extended.info;
        let card_width = DESIGN_TOKENS.sizing.media.tweet_card_width;
        let padding = DESIGN_TOKENS.spacing.lg;
        let (author, body) = self.content(
            "Post",
            "Double-click to add the author on the first line, then the post",
        );
        let author = painter.layout(
            author,
            egui::FontId::proportional(typography::LG),
            Color32::BLACK,
            card_width - 2.0 * padding,
        );
        let body = painter.layout(
            body,
            egui::FontId::proportional(typography::SM),
            DESIGN_TOKENS.semantic.extended.gray,
            card_width - 2.0 * padding,
        );
        let gap = DESIGN_TOKENS.spacing.sm;
        let card_height = DESIGN_TOKENS
            .sizing
            .media
            .tweet_card_height
            .max(2.0 * padding + author.size().y + gap + body.size().y);
        let card_rect = Rect::from_min_size(pos, egui::vec2(card_width, card_height));

        painter.rect_filled(card_rect, DESIGN_TOKENS.rounding.xl, Color32::WHITE);
        painter.rect_stroke(
            card_rect,
            DESIGN_TOKENS.rounding.xl,
            Stroke::new(stroke::HAIRLINE, DESIGN_TOKENS.semantic.extended.light_gray),
            StrokeKind::Outside,
        );
        let text = card_rect.min + egui::vec2(padding, padding);
        let below = text + egui::vec2(0.0, author.size().y + gap);
        painter.galley(text, author, Color32::BLACK);
        painter.galley(below, body, Color32::BLACK);

        // Anchor point
        painter.circle_filled(pos, DESIGN_TOKENS.rounding.md, accent);
    }

    /// Renders an idea card from the drawing's text: its first line as the title and the
    /// following lines as the description, wrapped to the card, which grows to fit them.
    pub(crate) fn render_idea(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let accent = DESIGN_TOKENS.semantic.extended.accent;
        let card_width = DESIGN_TOKENS.sizing.media.idea_card_width;
        let text_left = 60.0;
        let padding = DESIGN_TOKENS.spacing.lg;
        let wrap = card_width - text_left - padding;
        let (title, description) = self.content(
            "Idea",
            "Double-click to add the title on the first line, then the idea",
        );
        let title = painter.layout(
            title,
            egui::FontId::proportional(typography::LG),
            Color32::WHITE,
            wrap,
        );
        let description = painter.layout(
            description,
            egui::FontId::proportional(typography::SM),
            DESIGN_TOKENS.semantic.extended.gray,
            wrap,
        );
        let gap = DESIGN_TOKENS.spacing.xs;
        let card_height = DESIGN_TOKENS
            .sizing
            .media
            .idea_card_height
            .max(2.0 * padding + title.size().y + gap + description.size().y);
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

        let text = Pos2::new(card_rect.min.x + text_left, card_rect.min.y + padding);
        let below = text + egui::vec2(0.0, title.size().y + gap);
        painter.galley(text, title, Color32::WHITE);
        painter.galley(below, description, Color32::WHITE);

        // Anchor point
        painter.circle_filled(pos, DESIGN_TOKENS.rounding.md, accent);
        painter.circle_stroke(
            pos,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// The first line of the drawing's text and the lines after it, or `empty` and `prompt`
    /// while it has no text.
    fn content(&self, empty: &str, prompt: &str) -> (String, String) {
        match self
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            Some(text) => {
                let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
                (first.trim().to_owned(), rest.trim().to_owned())
            }
            None => (empty.to_owned(), prompt.to_owned()),
        }
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

        let icon_size = self.font_size;
        let rect = egui::Rect::from_center_size(pos, egui::Vec2::splat(icon_size));
        if crate::drawings::glyphs::paint(painter, icon, rect, color) {
            return;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;
    use egui::load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError};
    use egui::{ColorImage, Context, LayerId, Painter, RawInput, Shape, SizeHint};
    use std::sync::Arc;

    /// Serves a 50 x 50 picture at `test://square` and never finishes `test://pending`.
    struct Pictures;

    impl ImageLoader for Pictures {
        fn id(&self) -> &str {
            "pictures"
        }

        fn load(&self, _: &Context, uri: &str, _: SizeHint) -> ImageLoadResult {
            match uri {
                "test://square" => Ok(ImagePoll::Ready {
                    image: Arc::new(ColorImage::filled([50, 50], Color32::RED)),
                }),
                "test://pending" => Ok(ImagePoll::Pending { size: None }),
                _ => Err(LoadError::NotSupported),
            }
        }

        fn forget(&self, _: &str) {}

        fn forget_all(&self) {}

        fn byte_size(&self) -> usize {
            0
        }
    }

    /// Guards #985: media drawings show the content in their text, an image fitted inside its
    /// frame without stretching, and say when it is missing, loading or cannot be loaded.
    #[test]
    fn media_drawings_show_their_content() {
        let fitted = Rect::from_min_max(Pos2::new(50.0, 0.0), Pos2::new(150.0, 100.0));
        for (tool, text, texts, picture) in [
            (
                DrawingToolType::Image,
                None,
                vec!["Double-click to add an image"],
                None,
            ),
            (
                DrawingToolType::Image,
                Some("test://pending"),
                vec!["Loading image…"],
                None,
            ),
            (
                DrawingToolType::Image,
                Some("other://picture.png"),
                vec!["Image could not be loaded"],
                None,
            ),
            // The square picture fills the 100 px height of the 200 x 100 frame, centred.
            (
                DrawingToolType::Image,
                Some(" test://square\n"),
                vec![],
                Some(fitted),
            ),
            (
                DrawingToolType::Tweet,
                Some("Ada\nRates hold\nfor now"),
                vec!["Ada", "Rates hold\nfor now"],
                None,
            ),
            (
                DrawingToolType::Idea,
                None,
                vec![
                    "Idea",
                    "Double-click to add the title on the first line, then the idea",
                ],
                None,
            ),
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.points = vec![Pos2::ZERO, Pos2::new(200.0, 100.0)];
            drawing.text = text.map(Into::into);
            let ctx = Context::default();
            ctx.add_image_loader(Arc::new(Pictures));
            let output = ctx.run_ui(RawInput::default(), |_| {
                let painter = Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING);
                match tool {
                    DrawingToolType::Image => drawing.render_image(&painter),
                    DrawingToolType::Tweet => drawing.render_post(&painter),
                    _ => drawing.render_idea(&painter),
                }
            });
            let (mut shown, mut pictures) = (Vec::new(), Vec::new());
            for shape in &output.shapes {
                match &shape.shape {
                    Shape::Text(text) => shown.push(text.galley.text().to_owned()),
                    Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                        pictures.push(mesh.calc_bounds());
                    }
                    _ => {}
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!(shown, texts, "{tool:?} {text:?}");
            assert_eq!(pictures, Vec::from_iter(picture), "{tool:?} {text:?}");
        }
    }

    /// A frame far larger than the screen must cost only the checker squares the painter can
    /// show, also when its clip rect is unbounded (bounded by the 2,000 x 1,000 viewport).
    #[test]
    fn checkers_only_the_visible_part_of_a_huge_image() {
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(2000.0, 1000.0));
        let mut drawing = Drawing::new(1, DrawingToolType::Image);
        drawing.points = vec![Pos2::ZERO, Pos2::new(20_000.0, 20_000.0)];
        // 10 px squares, half of them dark, plus at most 10 fixed shapes (fill, border, icon, status).
        for (clip, dark_squares) in [
            (
                Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1000.0)),
                5_000,
            ),
            (Rect::EVERYTHING, 10_000),
        ] {
            let ctx = Context::default();
            let input = RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |_| {
                drawing.render_image(&Painter::new(ctx.clone(), LayerId::background(), clip));
            });
            let n = output.shapes.len();
            output.drop_without_applying_deltas();
            assert!(n > 10, "the visible pattern must still be drawn");
            assert!(
                n <= dark_squares + 10,
                "{n} shapes for {dark_squares} visible squares"
            );
        }
    }
}
