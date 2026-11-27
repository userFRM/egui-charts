//! Bundled drawing glyphs shared by chart rendering and the picker.

use egui::{Color32, Painter, Rect, Shape, Stroke};

#[cfg(feature = "ui")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glyph {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
}

#[cfg(feature = "ui")]
macro_rules! glyphs {
    ($($category:literal => [$($id:literal, $name:literal);* $(;)?]),* $(,)?) => {
        pub(crate) const GLYPHS: &[Glyph] = &[$($(Glyph { id: $id, name: $name, category: $category },)*)*];
    };
}

#[cfg(feature = "ui")]
glyphs! {
    "Icons: signals" => ["icon:up", "Arrow up"; "icon:down", "Arrow down"; "icon:check", "Check"; "icon:cross", "Cross"; "icon:flag", "Flag"; "icon:target", "Target"],
    "Icons: shapes" => ["icon:star", "Star"; "icon:heart", "Heart"; "icon:bolt", "Lightning bolt"; "icon:diamond", "Diamond"; "icon:circle", "Circle"; "icon:square", "Square"],
    "Emoji: faces" => ["😀", "Grinning face"; "🙂", "Slightly smiling face"; "😂", "Tears of joy"; "😎", "Sunglasses"; "😐", "Neutral face"; "😢", "Crying face"; "😡", "Angry face"],
    "Emoji: gestures" => ["👍", "Thumbs up"; "👎", "Thumbs down"; "👏", "Clapping hands"; "🙏", "Folded hands"],
    "Emoji: nature" => ["🔥", "Fire"; "🌞", "Sun"; "🌈", "Rainbow"; "🌲", "Evergreen tree"; "🐂", "Bull"; "🐻", "Bear"],
    "Emoji: food" => ["🍎", "Apple"; "🍕", "Pizza"; "☕", "Coffee"; "🎂", "Birthday cake"],
    "Emoji: travel" => ["🚀", "Rocket"; "🚗", "Car"; "✈", "Airplane"; "🏠", "House"],
    "Emoji: objects" => ["📌", "Pin"; "💰", "Money bag"; "💡", "Light bulb"; "🎯", "Bullseye"],
    "Emoji: symbols" => ["⭐", "Star"; "⚠", "Warning"; "✅", "Check mark"; "❌", "Cross mark"; "❤", "Heart"; "❗", "Exclamation"],
    "Stickers" => ["sticker:bull", "Bull"; "sticker:bear", "Bear"; "sticker:rocket", "Rocket"; "sticker:idea", "Idea"]
}

/// Family name of the bundled colour emoji font. SVG export writes it as the text's font family, so a viewer with
/// the same font installed matches it and others fall back to their own emoji font.
#[cfg(feature = "color-emoji")]
const EMOJI_FONT: &str = "Noto Color Emoji";

/// The picker's glyphs in display order: icons, emoji, stickers. With `color-emoji` the emoji are the full Unicode
/// list; ids from [`GLYPHS`] that are not in it still draw through [`font_id`].
#[cfg(feature = "ui")]
pub(crate) fn picker_glyphs() -> impl Iterator<Item = &'static Glyph> + Clone {
    #[cfg(feature = "color-emoji")]
    return GLYPHS
        .iter()
        .filter(|g| g.category.starts_with("Icons"))
        .chain(super::emoji_set::EMOJI)
        .chain(GLYPHS.iter().filter(|g| g.category == "Stickers"));
    #[cfg(not(feature = "color-emoji"))]
    GLYPHS.iter()
}

/// Whether `text` is one emoji of the colour set. Variation selectors are ignored, so the ids saved before the full
/// set (`"✈"`, `"⚠"`, `"❤"` without U+FE0F) and minimally-qualified or unqualified forms find their emoji.
#[cfg(feature = "color-emoji")]
fn is_color_emoji(text: &str) -> bool {
    use std::collections::HashSet;
    use std::sync::OnceLock;
    static KEYS: OnceLock<HashSet<String>> = OnceLock::new();
    let key = |s: &str| s.chars().filter(|&c| c != '\u{fe0f}').collect::<String>();
    KEYS.get_or_init(|| super::emoji_set::EMOJI.iter().map(|g| key(g.id)).collect())
        .contains(&key(text))
}

/// The font for drawing `text` at `size`: the bundled colour emoji font when `text` is one emoji of the colour set,
/// the proportional font otherwise. Installs the colour font on first use; until egui applies it (the next pass),
/// the application's fonts draw the emoji.
pub(crate) fn font_id(ctx: &egui::Context, text: &str, size: f32) -> egui::FontId {
    #[cfg(feature = "color-emoji")]
    if is_color_emoji(text) {
        let family = egui::FontFamily::Name(EMOJI_FONT.into());
        if ctx.fonts(|f| f.definitions().families.contains_key(&family)) {
            return egui::FontId::new(size, family);
        }
        // One request per pass: many emoji are drawn in a pass, and a host's `set_fonts` may drop the font later.
        let requested = egui::Id::unique("egui_charts::emoji_font_requested");
        let pass = ctx.cumulative_pass_nr();
        if ctx.data(|d| d.get_temp::<u64>(requested)) != Some(pass) {
            ctx.data_mut(|d| d.insert_temp(requested, pass));
            ctx.add_font(egui::FontInsert::new(
                EMOJI_FONT,
                egui::FontData::from_static(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/emoji/Noto-COLRv1.ttf"
                ))),
                vec![egui::InsertFontFamily {
                    family,
                    priority: egui::FontPriority::Highest,
                }],
            ));
        }
    }
    #[cfg(not(feature = "color-emoji"))]
    let _ = (ctx, text);
    egui::FontId::proportional(size)
}

/// Draws original vector glyphs on a 24-unit grid, with no font or image-loader dependency.
/// Emoji and legacy host strings return false so the caller can use the host font fallback.
/// `angle` rotates the glyph clockwise in radians about the centre of `rect`.
pub(crate) fn paint(painter: &Painter, id: &str, rect: Rect, color: Color32, angle: f32) -> bool {
    let color = if id.starts_with("sticker:") {
        Color32::from_rgb(35, 39, 47)
    } else {
        color
    };
    let turn = egui::emath::Rot2::from_angle(angle);
    let p = |x: f32, y: f32| {
        let upright = rect.min + egui::vec2(x, y) * (rect.width() / 24.0);
        // Keep upright coordinates exact, including at large screen offsets.
        if angle == 0.0 {
            upright
        } else {
            rect.center() + turn * (upright - rect.center())
        }
    };
    let stroke = Stroke::new(rect.width() / 16.0, color);
    let line = |coords: &[(f32, f32)], closed| {
        let points = coords.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(if closed {
            Shape::closed_line(points, stroke)
        } else {
            Shape::line(points, stroke)
        });
    };
    let circle = |x, y, radius, fill| {
        painter.circle(p(x, y), rect.width() * radius / 24.0, fill, stroke);
    };
    match id {
        "icon:up" => line(
            &[(5., 13.), (12., 6.), (19., 13.), (12., 6.), (12., 21.)],
            false,
        ),
        "icon:down" => line(
            &[(5., 11.), (12., 18.), (19., 11.), (12., 18.), (12., 3.)],
            false,
        ),
        "icon:check" => line(&[(4., 12.), (10., 18.), (21., 5.)], false),
        "icon:cross" => {
            line(&[(5., 5.), (19., 19.)], false);
            line(&[(5., 19.), (19., 5.)], false);
        }
        "icon:flag" => {
            line(
                &[
                    (5., 21.),
                    (5., 3.),
                    (20., 3.),
                    (16., 8.),
                    (20., 13.),
                    (5., 13.),
                ],
                false,
            );
        }
        "icon:target" => {
            circle(12., 12., 9., Color32::TRANSPARENT);
            circle(12., 12., 4., Color32::TRANSPARENT);
            circle(12., 12., 1., color);
        }
        "icon:star" => {
            let points: Vec<_> = (0..10)
                .map(|i| {
                    let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.;
                    let r = if i % 2 == 0 { 10. } else { 4.5 };
                    (12. + r * angle.cos(), 12. + r * angle.sin())
                })
                .collect();
            line(&points, true);
        }
        "icon:heart" => line(
            &[
                (12., 21.),
                (3., 12.),
                (3., 6.),
                (7., 3.),
                (12., 7.),
                (17., 3.),
                (21., 6.),
                (21., 12.),
            ],
            true,
        ),
        "icon:bolt" => line(
            &[
                (13., 2.),
                (4., 14.),
                (11., 14.),
                (10., 22.),
                (21., 9.),
                (14., 9.),
            ],
            true,
        ),
        "icon:diamond" => line(&[(12., 2.), (22., 12.), (12., 22.), (2., 12.)], true),
        "icon:circle" => circle(12., 12., 9., Color32::TRANSPARENT),
        "icon:square" => line(&[(3., 3.), (21., 3.), (21., 21.), (3., 21.)], true),
        // The bundled monochrome emoji font has no slightly smiling face, so it is drawn in its style.
        "🙂" if cfg!(not(feature = "color-emoji")) => {
            circle(12., 12., 10., Color32::TRANSPARENT);
            circle(8.5, 9.5, 0.75, color);
            circle(15.5, 9.5, 0.75, color);
            let smile: Vec<_> = (0..=8)
                .map(|i| {
                    let angle = (35. + 110. * i as f32 / 8.).to_radians();
                    (12. + 5.5 * angle.cos(), 11. + 5.5 * angle.sin())
                })
                .collect();
            line(&smile, false);
        }
        "sticker:bull" | "sticker:bear" => {
            let bull = id == "sticker:bull";
            let fur = if bull {
                Color32::from_rgb(45, 172, 122)
            } else {
                Color32::from_rgb(194, 124, 75)
            };
            circle(5., 5., 3., fur);
            circle(19., 5., 3., fur);
            circle(12., 12., 9., fur);
            if bull {
                line(&[(4., 6.), (2., 2.), (8., 6.)], false);
                line(&[(16., 6.), (22., 2.), (20., 6.)], false);
            }
            circle(8., 10., 1., color);
            circle(16., 10., 1., color);
            circle(12., 16., 4., Color32::from_rgb(239, 201, 143));
            line(&[(10., 16.), (14., 16.)], false);
        }
        "sticker:rocket" => {
            painter.add(Shape::convex_polygon(
                vec![p(8., 17.), p(12., 3.), p(16., 17.)],
                Color32::from_rgb(88, 152, 232),
                stroke,
            ));
            line(&[(8., 13.), (3., 20.), (8., 18.)], false);
            line(&[(16., 13.), (21., 20.), (16., 18.)], false);
            circle(12., 12., 2., Color32::WHITE);
            painter.add(Shape::convex_polygon(
                vec![p(9., 18.), p(12., 23.), p(15., 18.)],
                Color32::from_rgb(255, 163, 60),
                Stroke::NONE,
            ));
        }
        "sticker:idea" => {
            circle(12., 10., 7., Color32::from_rgb(255, 204, 68));
            line(&[(8., 17.), (8., 21.), (16., 21.), (16., 17.)], false);
            line(&[(10., 13.), (12., 16.), (14., 13.)], false);
        }
        _ => return false,
    }
    true
}

#[cfg(all(test, feature = "color-emoji"))]
mod tests {
    use super::*;

    /// Every emoji of the set, and every emoji id saved before it, draws as one colour glyph, and the picker lists
    /// the set in the nine categories, each once.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn color_emoji_set_draws_every_emoji_in_colour_and_keeps_legacy_ids() {
        use super::super::emoji_set::EMOJI;
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            font_id(ui.ctx(), "😀", 8.0);
        })
        .drop_without_applying_deltas();
        let mut failures = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let legacy = GLYPHS.iter().filter(|g| g.category.starts_with("Emoji"));
            for glyph in EMOJI.iter().chain(legacy) {
                let font = font_id(ui.ctx(), glyph.id, 8.0);
                if font.family != egui::FontFamily::Name(EMOJI_FONT.into()) {
                    failures.push(format!("{} {:?}: {:?}", glyph.name, glyph.id, font.family));
                    continue;
                }
                let galley = ui
                    .painter()
                    .layout_no_wrap(glyph.id.into(), font, Color32::WHITE);
                let drawn: Vec<_> = galley
                    .rows
                    .iter()
                    .flat_map(|r| r.glyphs.iter())
                    .filter(|g| !g.uv_rect.is_nothing())
                    .collect();
                if drawn.len() != 1 || !drawn[0].is_color {
                    failures.push(format!("{} {:?}: {drawn:?}", glyph.name, glyph.id));
                }
            }
        })
        .drop_without_applying_deltas();
        assert!(
            failures.is_empty(),
            "{} emoji not drawn as one colour glyph, first: {:#?}",
            failures.len(),
            &failures[..failures.len().min(20)]
        );
        assert_eq!(EMOJI.len(), 3963);
        let mut runs: Vec<&str> = Vec::new();
        for glyph in EMOJI {
            if runs.last() != Some(&glyph.category) {
                runs.push(glyph.category);
            }
        }
        assert_eq!(
            runs,
            [
                "Emoji: smileys and emotion",
                "Emoji: people and body",
                "Emoji: animals and nature",
                "Emoji: food and drink",
                "Emoji: travel and places",
                "Emoji: activities",
                "Emoji: objects",
                "Emoji: symbols",
                "Emoji: flags",
            ]
        );
        assert_eq!(picker_glyphs().count(), 12 + 3963 + 4);
    }
}
