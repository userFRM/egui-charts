//! Round symbol marks: a host's image, or the symbol's initial on a neutral disc.

use crate::theme::ThemeExt;
use egui::epaint::RectShape;
use egui::load::TexturePoll;
use egui::{Align2, Color32, FontId, ImageSource, Painter, Pos2, Rect, SizeHint, TextureOptions};

/// Paints `image` into `rect` as a circle. Returns `false`, painting nothing, without an image or
/// while it is loading or unreadable.
pub(crate) fn paint_round_image(
    painter: &Painter,
    rect: Rect,
    image: Option<ImageSource<'_>>,
) -> bool {
    let Some(image) = image else {
        return false;
    };
    let pixels = (rect.size() * painter.pixels_per_point()).round();
    let size = SizeHint::Size {
        width: pixels.x as u32,
        height: pixels.y as u32,
        maintain_aspect_ratio: true,
    };
    let Ok(TexturePoll::Ready { texture }) =
        image.load(painter.ctx(), TextureOptions::LINEAR, size)
    else {
        return false;
    };
    painter.add(
        RectShape::filled(rect, rect.width() / 2.0, Color32::WHITE).with_texture(
            texture.id,
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
        ),
    );
    true
}

/// Paints the mark of `symbol` in `rect`: `image` as a circle once it is ready, else the first
/// letter or digit of `symbol`, upper-cased, on a neutral disc. Without a letter or digit the disc
/// is drawn alone, so marks stay aligned in lists.
pub(crate) fn paint_symbol_mark(
    painter: &Painter,
    rect: Rect,
    symbol: &str,
    image: Option<ImageSource<'_>>,
) {
    if paint_round_image(painter, rect, image) {
        return;
    }
    let theme = painter.ctx().active_theme();
    // A tint rather than a fill keeps the disc visible on hovered and selected rows, with the
    // letter at 4.5:1 or more on all of them.
    painter.circle_filled(
        rect.center(),
        rect.width() / 2.0,
        theme.ui.text_secondary.gamma_multiply(0.15),
    );
    if let Some(initial) = symbol.chars().find(|c| c.is_alphanumeric()) {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            initial.to_uppercase(),
            FontId::proportional(rect.height() * 0.5),
            theme.ui.text_secondary,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{apply_to_egui, contrast, presets};
    use egui::{Context, Shape};

    struct PendingLogo;

    impl egui::load::TextureLoader for PendingLogo {
        fn id(&self) -> &str {
            "pending-symbol-logo"
        }
        fn load(
            &self,
            _: &Context,
            uri: &str,
            _: TextureOptions,
            _: SizeHint,
        ) -> egui::load::TextureLoadResult {
            if uri == "pending://logo" {
                Ok(TexturePoll::Pending { size: None })
            } else {
                Err(egui::load::LoadError::NotSupported)
            }
        }
        fn forget(&self, _: &str) {}
        fn forget_all(&self) {}
        fn byte_size(&self) -> usize {
            0
        }
    }

    /// Ready images replace the initial; missing and unreadable images keep a legible mark on
    /// every list background, including selected and hovered rows.
    #[test]
    fn symbol_marks_show_the_image_or_the_symbols_initial() {
        for (name, theme) in presets() {
            let ctx = Context::default();
            apply_to_egui(&ctx, &theme);
            ctx.add_texture_loader(std::sync::Arc::new(PendingLogo));
            let logo = ctx.load_texture(
                "logo",
                egui::ColorImage::filled([18, 18], Color32::BLUE),
                Default::default(),
            );
            let cases = [
                (
                    "aapl",
                    Some(ImageSource::Texture(
                        (logo.id(), egui::Vec2::splat(18.0)).into(),
                    )),
                    None,
                ),
                ("aapl", None, Some("A")),
                (
                    "^SPX",
                    Some(ImageSource::from("bytes://missing.png")),
                    Some("S"),
                ),
                ("1INCH", None, Some("1")),
                ("", None, None),
                ("-", None, None),
                ("$BTC", None, Some("B")),
                ("ß", None, Some("SS")),
                (
                    "pending",
                    Some(ImageSource::from("pending://logo")),
                    Some("P"),
                ),
            ];
            let mut output = ctx.run_ui(Default::default(), |ui| {
                for (index, (symbol, image, _)) in cases.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        egui::pos2(30.0 * index as f32, 0.0),
                        egui::Vec2::splat(18.0),
                    );
                    paint_symbol_mark(ui.painter(), rect, symbol, image.clone());
                }
            });
            output.textures_delta.clear();
            for (index, (_, _, initial)) in cases.iter().enumerate() {
                let rect = Rect::from_min_size(
                    egui::pos2(30.0 * index as f32, 0.0),
                    egui::Vec2::splat(18.0),
                );
                let shapes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter(|shape| rect.contains(shape.shape.visual_bounding_rect().center()))
                    .collect();
                if index == 0 {
                    assert_eq!(shapes.len(), 1);
                    assert!(matches!(&shapes[0].shape, Shape::Rect(image)
                        if image.fill_texture_id() == logo.id() && image.rect == rect
                            && image.corner_radius == egui::CornerRadius::same(9)));
                    continue;
                }
                let disc = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Circle(circle) => Some(circle),
                        _ => None,
                    })
                    .expect("placeholder disc");
                assert_eq!(disc.center, rect.center());
                assert_eq!(disc.radius, 9.0);
                let text = shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::Text(text) => Some(text),
                    _ => None,
                });
                assert_eq!(text.map(|text| text.galley.text()), *initial);
                if let Some(text) = text {
                    assert!(
                        (Rect::from_min_size(text.pos, text.galley.size()).center()
                            - rect.center())
                        .length()
                            <= 1.0
                    );
                    for bg in [
                        theme.ui.panel_bg,
                        theme.chart.bg,
                        theme.ui.btn_bg_hover,
                        theme.ui.btn_bg_active,
                    ] {
                        let fill = bg.blend(disc.fill);
                        assert_ne!(fill, bg, "{name}: disc on {bg:?}");
                        assert!(
                            contrast(text.fallback_color, fill) >= 4.5,
                            "{name}: letter on {fill:?}"
                        );
                    }
                }
            }
            output.drop_without_applying_deltas();
        }
    }
}
