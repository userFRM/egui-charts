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
    "Emoji: faces" => ["😀", "Grinning face"; "😂", "Tears of joy"; "😎", "Sunglasses"; "😐", "Neutral face"; "😢", "Crying face"; "😡", "Angry face"],
    "Emoji: gestures" => ["👍", "Thumbs up"; "👎", "Thumbs down"; "👏", "Clapping hands"; "🙏", "Folded hands"],
    "Emoji: nature" => ["🔥", "Fire"; "🌞", "Sun"; "🌈", "Rainbow"; "🌲", "Evergreen tree"; "🐂", "Bull"; "🐻", "Bear"],
    "Emoji: food" => ["🍎", "Apple"; "🍕", "Pizza"; "☕", "Coffee"; "🎂", "Birthday cake"],
    "Emoji: travel" => ["🚀", "Rocket"; "🚗", "Car"; "✈", "Airplane"; "🏠", "House"],
    "Emoji: objects" => ["📌", "Pin"; "💰", "Money bag"; "💡", "Light bulb"; "🎯", "Bullseye"],
    "Emoji: symbols" => ["⭐", "Star"; "⚠", "Warning"; "✅", "Check mark"; "❌", "Cross mark"; "❤", "Heart"; "❗", "Exclamation"],
    "Stickers" => ["sticker:bull", "Bull"; "sticker:bear", "Bear"; "sticker:rocket", "Rocket"; "sticker:idea", "Idea"]
}

/// Draws original vector glyphs on a 24-unit grid, with no font or image-loader dependency.
/// Emoji and legacy host strings return false so the caller can use the host font fallback.
pub(crate) fn paint(painter: &Painter, id: &str, rect: Rect, color: Color32) -> bool {
    let color = if id.starts_with("sticker:") {
        Color32::from_rgb(35, 39, 47)
    } else {
        color
    };
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y) * (rect.width() / 24.0);
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
