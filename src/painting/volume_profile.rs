//! Price-row profile painting shared by studies and drawing tools.

use crate::data::{ProfileDisplay, VolumeProfile, VolumeProfileConfig};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke, epaint::StrokeKind};
use std::ops::Range;

pub(crate) fn paint_profile(
    painter: &egui::Painter,
    profile: &VolumeProfile,
    config: VolumeProfileConfig,
    color: [u8; 4],
    span: Range<f32>,
    right_aligned: bool,
    price_to_y: &dyn Fn(f64) -> f32,
) {
    let (left, right) = (span.start, span.end);
    if left >= right {
        return;
    }
    let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(config.value_area()) else {
        return;
    };
    let y_at_row = |row: usize| (price_to_y)(profile.low + row as f64 * profile.row_height);
    let max_width = (right - left) * 0.9;
    let max_volume = profile.volume[poc];

    for (row, &volume) in profile.volume.iter().enumerate() {
        // Ordered on screen: the upper row edge is lower down on an inverted scale.
        let (upper, lower) = (y_at_row(row + 1), y_at_row(row));
        let (top, bottom) = (upper.min(lower), upper.max(lower));
        let gap = ((bottom - top) * 0.5).min(1.0);
        let width = (volume / max_volume) as f32 * max_width;

        // Color coding: POC is bright, Value Area is medium, outside is dim
        let bar_color = if row == poc {
            DESIGN_TOKENS.semantic.extended.favorite_gold
        } else if (va_low..=va_high).contains(&row) {
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 180)
        } else {
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 80)
        };

        let paint = |start, width, color| {
            let start = if right_aligned {
                right - (start - left) - width
            } else {
                start
            };
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(start, top),
                    Pos2::new(start + width, bottom - gap),
                ),
                0.0,
                color,
            );
        };
        let rgba = |c: [u8; 4]| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
        match config.display {
            ProfileDisplay::Total => paint(left, width, bar_color),
            ProfileDisplay::UpDown => {
                let up_width = (profile.up[row] / max_volume) as f32 * max_width;
                paint(left, up_width, rgba(config.up_color));
                paint(
                    left + up_width,
                    (width - up_width).max(0.0),
                    rgba(config.down_color),
                );
            }
            ProfileDisplay::Delta => {
                let delta = profile.up[row] - profile.down[row];
                let color = if delta >= 0.0 {
                    config.up_color
                } else {
                    config.down_color
                };
                paint(
                    left,
                    (delta.abs() / max_volume) as f32 * max_width,
                    rgba(color),
                );
            }
        }
    }

    let rect = Rect::from_two_pos(
        Pos2::new(left, y_at_row(profile.volume.len())),
        Pos2::new(right, y_at_row(0)),
    );
    let poc_y = (y_at_row(poc) + y_at_row(poc + 1)) / 2.0;
    draw_volume_profile_lines(
        painter,
        &rect,
        color,
        poc_y,
        y_at_row(va_high + 1),
        y_at_row(va_low),
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(
            stroke::HAIRLINE,
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
        ),
        StrokeKind::Outside,
    );
}
fn draw_volume_profile_lines(
    painter: &egui::Painter,
    rect: &Rect,
    color: [u8; 4],
    poc_y: f32,
    vah_y: f32,
    val_y: f32,
) {
    let font = egui::FontId::proportional(typography::XS);

    // Draw POC line (dashed, extends across range)
    let poc_color = DESIGN_TOKENS.semantic.extended.favorite_gold;
    painter.extend(Shape::dashed_line(
        &[Pos2::new(rect.min.x, poc_y), Pos2::new(rect.max.x, poc_y)],
        Stroke::new(stroke::HAIRLINE, poc_color),
        8.0,
        4.0,
    ));

    let va_color = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 200);
    for y in [vah_y, val_y] {
        painter.hline(
            rect.min.x..=rect.max.x,
            y,
            Stroke::new(stroke::HAIRLINE, va_color),
        );
    }
    if rect.width() < 50.0 {
        return;
    }

    let poc_bg =
        Rect::from_center_size(Pos2::new(rect.max.x - 25.0, poc_y), egui::vec2(40.0, 14.0));
    painter.rect_filled(poc_bg, DESIGN_TOKENS.rounding.sm, poc_color);
    painter.text(
        poc_bg.center(),
        egui::Align2::CENTER_CENTER,
        "POC",
        font.clone(),
        Color32::BLACK,
    );

    // Each label sits outside the value area, away from the other line;
    // the VAH line is the lower one on an inverted scale.
    let away = if vah_y <= val_y { 8.0 } else { -8.0 };
    for (label, y) in [("VAH", vah_y - away), ("VAL", val_y + away)] {
        painter.text(
            Pos2::new(rect.max.x - 25.0, y),
            egui::Align2::CENTER_CENTER,
            label,
            font.clone(),
            va_color,
        );
    }
}
