//! Media rendering implementations
//!
//! Includes: font icon, image, post and idea. An image drawing's text is the URI of its image,
//! resolved by the egui image loaders the host installs; a post or idea drawing shows the
//! content its link resolved to, or its text, the first line naming the author or the title.

use crate::drawings::domain::Drawing;
use crate::localization as l10n;
use crate::painting::dashed_line::finite_clip;
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{
    Color32, Pos2, Rect, Stroke,
    epaint::{Galley, StrokeKind},
    load::TexturePoll,
};
use std::sync::Arc;

/// A post or idea card laid out at the drawing's first anchor.
struct Card {
    rect: Rect,
    first: (Pos2, Arc<Galley>),
    rest: (Pos2, Arc<Galley>),
    /// The box of the content's picture, and its address.
    picture: Option<(Rect, String)>,
    /// The Open row of a card with a post link.
    open: Option<(Pos2, Arc<Galley>)>,
}

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
            None => "Double-click to add",
            Some(Ok(TexturePoll::Pending { .. })) => "Loading image…",
            Some(_) => "Could not load image",
        };
        let theme = painter.ctx().active_theme();

        // Background with subtle pattern (checkerboard for transparency indication)
        painter.rect_filled(frame_rect, DESIGN_TOKENS.rounding.md, theme.ui.panel_bg);

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

        // The theme's text reads on its panel colour and on the grey checkers.
        let status = painter.layout(
            l10n::text(painter.ctx(), status),
            egui::FontId::proportional(typography::XS),
            theme.ui.text,
            frame_rect.width() - 2.0 * DESIGN_TOKENS.spacing.sm,
        );
        painter.galley(
            Pos2::new(
                frame_rect.center().x - status.size().x / 2.0,
                frame_rect.max.y - DESIGN_TOKENS.spacing.sm - status.size().y,
            ),
            status,
            theme.ui.text,
        );
    }

    /// Renders a post card from its linked content or hand-typed text, growing to fit it.
    pub(crate) fn render_post(&self, painter: &egui::Painter) {
        let Some(card) = self.card(painter.ctx()) else {
            return;
        };
        let theme = painter.ctx().active_theme();
        painter.rect_filled(card.rect, DESIGN_TOKENS.rounding.xl, theme.ui.panel_bg);
        painter.rect_stroke(
            card.rect,
            DESIGN_TOKENS.rounding.xl,
            Stroke::new(stroke::HAIRLINE, theme.ui.border),
            StrokeKind::Outside,
        );
        Self::paint_post_content(painter, card);
        painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, theme.ui.info);
    }

    /// Renders an idea card from its linked content or hand-typed text, growing to fit it.
    pub(crate) fn render_idea(&self, painter: &egui::Painter) {
        let Some(card) = self.card(painter.ctx()) else {
            return;
        };
        let card_rect = card.rect;
        let theme = painter.ctx().active_theme();
        let accent = theme.ui.accent;
        painter.rect_filled(card_rect, DESIGN_TOKENS.rounding.lg, theme.ui.panel_bg);
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
                Stroke::new(stroke::HAIRLINE, theme.ui.text_secondary),
            );
        }

        Self::paint_post_content(painter, card);
        painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, accent);
        painter.circle_stroke(
            self.points[0],
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, theme.ui.text),
        );
    }

    /// One layout for painting the card and hitting its Open row.
    fn card(&self, ctx: &egui::Context) -> Option<Card> {
        let &pos = self.points.first()?;
        let sizing = &DESIGN_TOKENS.sizing.media;
        let spacing = &DESIGN_TOKENS.spacing;
        let (width, min_height, left, gap, empty, prompt) = match self.tool_type {
            crate::drawings::DrawingToolType::Tweet => (
                sizing.tweet_card_width,
                sizing.tweet_card_height,
                spacing.lg,
                spacing.sm,
                "Post",
                "Double-click to add the author on the first line, then the post",
            ),
            crate::drawings::DrawingToolType::Idea => (
                sizing.idea_card_width,
                sizing.idea_card_height,
                60.0,
                spacing.xs,
                "Idea",
                "Double-click to add the title on the first line, then the idea",
            ),
            _ => return None,
        };
        let theme = ctx.active_theme();
        let column = width - left - spacing.lg;
        let mut y = pos.y + spacing.lg;
        let picture = self
            .post
            .as_ref()
            .and_then(|post| post.content.as_ref())
            .and_then(|content| content.picture.as_deref())
            .map(str::trim)
            .filter(|uri| !uri.is_empty())
            .map(|uri| {
                let rect = Rect::from_min_size(
                    Pos2::new(pos.x + left, y),
                    egui::vec2(column, column * 0.75),
                );
                y = rect.bottom() + gap;
                (rect, uri.to_owned())
            });
        let (first, rest) = self.content(ctx, empty, prompt);
        let first = ctx.fonts_mut(|fonts| {
            fonts.layout(
                first,
                egui::FontId::proportional(typography::LG),
                theme.ui.text,
                column,
            )
        });
        let first = (Pos2::new(pos.x + left, y), first);
        y += first.1.size().y + gap;
        let rest = ctx.fonts_mut(|fonts| {
            fonts.layout(
                rest,
                egui::FontId::proportional(typography::SM),
                theme.ui.text_secondary,
                column,
            )
        });
        let rest = (Pos2::new(pos.x + left, y), rest);
        y += rest.1.size().y;
        let open = self.post.as_ref().map(|_| {
            y += gap;
            let label = l10n::text(ctx, "Open");
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout(
                    label,
                    egui::FontId::proportional(typography::SM),
                    theme.ui.info,
                    column,
                )
            });
            let row = (Pos2::new(pos.x + left, y), galley);
            y += row.1.size().y;
            row
        });
        Some(Card {
            rect: Rect::from_min_size(
                pos,
                egui::vec2(width, min_height.max(y + spacing.lg - pos.y)),
            ),
            first,
            rest,
            picture,
            open,
        })
    }

    fn paint_post_content(painter: &egui::Painter, card: Card) {
        let theme = painter.ctx().active_theme();
        painter.galley(card.first.0, card.first.1, theme.ui.text);
        painter.galley(card.rest.0, card.rest.1, theme.ui.text_secondary);
        if let Some((box_rect, uri)) = card.picture {
            let status = match painter.ctx().try_load_texture(
                &uri,
                egui::TextureOptions::LINEAR,
                egui::SizeHint::default(),
            ) {
                Ok(TexturePoll::Ready { texture }) if texture.size.min_elem() > 0.0 => {
                    let scale =
                        (box_rect.width() / texture.size.x).min(box_rect.height() / texture.size.y);
                    let rect = Rect::from_center_size(box_rect.center(), texture.size * scale);
                    painter.image(
                        texture.id,
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    None
                }
                Ok(TexturePoll::Pending { .. }) => Some("Loading image…"),
                _ => Some("Could not load image"),
            };
            if let Some(status) = status {
                painter.rect_stroke(
                    box_rect,
                    0.0,
                    Stroke::new(stroke::HAIRLINE, theme.ui.border),
                    StrokeKind::Outside,
                );
                painter.text(
                    box_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    l10n::text(painter.ctx(), status),
                    egui::FontId::proportional(typography::XS),
                    theme.ui.text_secondary,
                );
            }
        }
        if let Some((pos, galley)) = card.open {
            painter.galley(pos, galley, theme.ui.info);
        }
    }

    /// Where a post or idea card with a post link shows its Open row.
    pub(crate) fn open_row(&self, ctx: &egui::Context) -> Option<Rect> {
        let (pos, galley) = self.card(ctx)?.open?;
        Some(Rect::from_min_size(pos, galley.size()))
    }

    /// Linked content or the first line of the drawing's text and the lines after it, with
    /// `empty` and `prompt` for a hand-typed card that has no text yet.
    fn content(&self, ctx: &egui::Context, empty: &str, prompt: &str) -> (String, String) {
        if let Some(post) = &self.post {
            if let Some(content) = &post.content {
                let title = content.title.trim();
                let author = content.author.trim();
                let first = if !title.is_empty() { title } else { author };
                let first = if first.is_empty() {
                    l10n::text(ctx, empty)
                } else {
                    first.to_owned()
                };
                let time = content.time.format("%Y-%m-%d %H:%M");
                let mut rest = if !title.is_empty() && !author.is_empty() {
                    format!("{author} \u{b7} {time}")
                } else {
                    time.to_string()
                };
                if !content.text.trim().is_empty() {
                    rest.push('\n');
                    rest.push_str(content.text.trim());
                }
                return (first, rest);
            }
            let mut rest = l10n::text(
                ctx,
                if post.failed {
                    "Could not load this link"
                } else {
                    "Loading…"
                },
            );
            if !post.link.trim().is_empty() {
                rest.push('\n');
                rest.push_str(post.link.trim());
            }
            return (l10n::text(ctx, empty), rest);
        }
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
            None => (l10n::text(ctx, empty), l10n::text(ctx, prompt)),
        }
    }

    /// Renders a font icon (emoji/sticker).
    /// Features: scalable icon and optional background.
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

        let icon_size = crate::styles::typography::font_size(self.font_size);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;
    use crate::model::{PostContent, PostLink};
    use chrono::{TimeZone, Utc};
    use egui::load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError};
    use egui::{ColorImage, Context, LayerId, Painter, RawInput, Shape, SizeHint};
    use std::sync::Arc;

    /// Stickers retain their palette, while icons and emoji follow the drawing colour
    /// and font glyphs have no extra anchor dot.
    #[test]
    fn glyph_colours_and_anchor_free_rendering() {
        let mut results = Vec::new();
        for id in [
            "sticker:bull",
            "sticker:bear",
            "sticker:rocket",
            "sticker:idea",
            "icon:check",
            "😀",
            "*",
        ] {
            let mut drawing = Drawing::new(1, DrawingToolType::FontIcon);
            drawing.text = Some(id.into());
            drawing.points = vec![Pos2::new(100.0, 100.0)];
            let ctx = Context::default();
            let mut render = |color: Color32| {
                drawing.color = color.to_srgba_unmultiplied();
                let mut output = ctx.run_ui(RawInput::default(), |_| {
                    drawing.render_font_icon(&Painter::new(
                        ctx.clone(),
                        LayerId::background(),
                        Rect::EVERYTHING,
                    ));
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                shapes
            };
            let red = render(Color32::RED);
            let blue = render(Color32::BLUE);
            let font_only = id == "😀" || id == "*";
            results.push((
                id,
                red == blue,
                !font_only
                    || red
                        .iter()
                        .all(|s| matches!(s.shape, Shape::Text(_) | Shape::Noop)),
            ));
        }
        assert_eq!(
            results,
            [
                ("sticker:bull", true, true),
                ("sticker:bear", true, true),
                ("sticker:rocket", true, true),
                ("sticker:idea", true, true),
                ("icon:check", false, true),
                ("😀", false, true),
                ("*", false, true),
            ]
        );
    }

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

    /// Media drawings show typed or linked content, fit pictures without stretching and report
    /// missing, loading and failed content. Linked cards keep a distinct Open row.
    #[test]
    fn media_drawings_show_their_content() {
        let fitted = Rect::from_min_max(Pos2::new(50.0, 0.0), Pos2::new(150.0, 100.0));
        let time = Utc.with_ymd_and_hms(2026, 10, 30, 9, 30, 0).unwrap();
        let mut post = PostContent::new("Ada", time, "Rates hold");
        post.picture = Some("test://square".into());
        let mut idea = PostContent::new("Ada", time, "Above the range");
        idea.title = "Breakout".into();
        idea.picture = Some("test://square".into());
        let mut pending = PostContent::new("Ada", time, "Above the range");
        pending.picture = Some("test://pending".into());
        let mut failed = PostLink::new("https://example.invalid/i/3");
        failed.failed = true;
        for (_, theme) in crate::theme::presets() {
            for (tool, text, texts, picture, post) in [
                (
                    DrawingToolType::Image,
                    None,
                    vec!["Double-click to add"],
                    None,
                    None,
                ),
                (
                    DrawingToolType::Image,
                    Some("test://pending"),
                    vec!["Loading image…"],
                    None,
                    None,
                ),
                (
                    DrawingToolType::Image,
                    Some("other://picture.png"),
                    vec!["Image unavailable"],
                    None,
                    None,
                ),
                // The square picture fills the 100 px height of the 200 x 100 frame, centred.
                (
                    DrawingToolType::Image,
                    Some(" test://square\n"),
                    vec![],
                    Some(fitted),
                    None,
                ),
                (
                    DrawingToolType::Tweet,
                    Some("Ada\nRates hold\nfor now"),
                    vec!["Ada", "Rates hold\nfor now"],
                    None,
                    None,
                ),
                (
                    DrawingToolType::Idea,
                    Some("Supplied title\nSupplied description"),
                    vec!["Supplied title", "Supplied description"],
                    None,
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
                    None,
                ),
                (
                    DrawingToolType::Tweet,
                    None,
                    vec!["Ada", "2026-10-30 09:30\nRates hold", "Open"],
                    Some(Rect::from_min_max(
                        Pos2::new(41.0, 8.0),
                        Pos2::new(239.0, 206.0),
                    )),
                    Some(PostLink::resolved(
                        "https://example.invalid/p/1",
                        post.clone(),
                    )),
                ),
                (
                    DrawingToolType::Idea,
                    None,
                    vec![
                        "Breakout",
                        "Ada · 2026-10-30 09:30\nAbove the range",
                        "Open",
                    ],
                    Some(Rect::from_min_max(
                        Pos2::new(76.5, 8.0),
                        Pos2::new(175.5, 107.0),
                    )),
                    Some(PostLink::resolved(
                        "https://example.invalid/i/1",
                        idea.clone(),
                    )),
                ),
                (
                    DrawingToolType::Idea,
                    None,
                    vec![
                        "Ada",
                        "2026-10-30 09:30\nAbove the range",
                        "Loading image…",
                        "Open",
                    ],
                    None,
                    Some(PostLink::resolved(
                        "https://example.invalid/i/2",
                        pending.clone(),
                    )),
                ),
                (
                    DrawingToolType::Tweet,
                    Some("Typed\nby hand"),
                    vec!["Post", "Loading…\nhttps://example.invalid/p/2", "Open"],
                    None,
                    Some(PostLink::new("https://example.invalid/p/2")),
                ),
                (
                    DrawingToolType::Idea,
                    None,
                    vec![
                        "Idea",
                        "Could not load this link\nhttps://example.invalid/i/3",
                        "Open",
                    ],
                    None,
                    Some(failed.clone()),
                ),
            ] {
                let mut drawing = Drawing::new(1, tool);
                drawing.points = vec![Pos2::ZERO, Pos2::new(200.0, 100.0)];
                drawing.text = text.map(Into::into);
                drawing.post = post.clone();
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                crate::localization::set_translator(&ctx, |key| {
                    (key == "Could not load image").then(|| "Image unavailable".into())
                });
                ctx.add_image_loader(Arc::new(Pictures));
                let mut output = ctx.run_ui(RawInput::default(), |_| {
                    let painter =
                        Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING);
                    match tool {
                        DrawingToolType::Image => drawing.render_image(&painter),
                        DrawingToolType::Tweet => drawing.render_post(&painter),
                        _ => drawing.render_idea(&painter),
                    }
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                if tool != DrawingToolType::Image {
                    assert!(shapes.iter().any(|shape| matches!(&shape.shape,
                    Shape::Rect(rect) if rect.rect.min == Pos2::ZERO && rect.fill == theme.ui.panel_bg
                )), "cards follow the theme");
                    for shape in &shapes {
                        if let Shape::Text(text) = &shape.shape {
                            let expected = if text.galley.text() == texts[0] {
                                theme.ui.text
                            } else if text.galley.text() == "Open" {
                                theme.ui.info
                            } else {
                                theme.ui.text_secondary
                            };
                            assert!(
                                text.galley
                                    .job
                                    .sections
                                    .iter()
                                    .all(|section| section.format.color == expected)
                            );
                        }
                    }
                }
                let (mut shown, mut pictures) = (Vec::new(), Vec::new());
                for shape in &shapes {
                    match &shape.shape {
                        Shape::Text(text) => shown.push(text.galley.text().to_owned()),
                        Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                            pictures.push(mesh.calc_bounds());
                        }
                        _ => {}
                    }
                }
                assert_eq!(shown, texts, "{tool:?} {text:?}");
                assert_eq!(pictures, Vec::from_iter(picture), "{tool:?} {text:?}");
            }
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
            ctx.tessellation_options_mut(|options| options.feathering = false);
            let input = RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |_| {
                drawing.render_image(&Painter::new(ctx.clone(), LayerId::background(), clip));
            });
            let n = output.shapes.len();
            let check_color = DESIGN_TOKENS.semantic.extended.gray.gamma_multiply(0.8);
            let mut area = 0.0;
            for primitive in ctx.tessellate(output.shapes.clone(), output.pixels_per_point) {
                if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                    for triangle in mesh.indices.as_chunks::<3>().0 {
                        let vertices = triangle.map(|i| mesh.vertices[i as usize]);
                        if vertices.iter().all(|v| v.color == check_color) {
                            let [a, b, c] = vertices.map(|v| v.pos);
                            area +=
                                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0;
                        }
                    }
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!(area, dark_squares as f32 * 100.0, "visible checker area");
            assert!(
                n <= dark_squares + 10,
                "{n} shapes for {dark_squares} visible squares"
            );
        }
    }
}
