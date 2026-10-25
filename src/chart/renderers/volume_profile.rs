//! Horizontal price-row histograms shared by drawing and study profiles.
use crate::studies::{ProfileDisplay, StudyProfile, VolumeProfile, VolumeProfileConfig};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke, epaint::StrokeKind};
use std::ops::Range;

pub(crate) fn profile_span(plot: &StudyProfile, coords: &super::ChartMapping) -> Range<f32> {
    if plot.bars.is_empty() {
        return 0.0..0.0;
    }
    if plot.right_aligned {
        coords.rect.right() - coords.rect.width() * 0.3..coords.rect.right()
    } else {
        (coords.idx_to_x(plot.bars.start) - coords.bar_spacing * 0.5).max(coords.rect.left())
            ..(coords.idx_to_x(plot.bars.end - 1) + coords.bar_spacing * 0.5)
                .min(coords.rect.right())
    }
}

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
                    Pos2::new(start + width, (bottom - 1.0).max(top)),
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

    // POC label
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

    // Draw VAH line
    let va_color = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 200);
    painter.hline(
        rect.min.x..=rect.max.x,
        vah_y,
        Stroke::new(stroke::HAIRLINE, va_color),
    );

    // Each label sits outside the value area, away from the other line;
    // the VAH line is the lower one on an inverted scale.
    let away = if vah_y <= val_y { 8.0 } else { -8.0 };

    // VAH label
    painter.text(
        Pos2::new(rect.max.x - 25.0, vah_y - away),
        egui::Align2::CENTER_CENTER,
        "VAH",
        font.clone(),
        va_color,
    );

    // Draw VAL line
    painter.hline(
        rect.min.x..=rect.max.x,
        val_y,
        Stroke::new(stroke::HAIRLINE, va_color),
    );

    // VAL label
    painter.text(
        Pos2::new(rect.max.x - 25.0, val_y + away),
        egui::Align2::CENTER_CENTER,
        "VAL",
        font,
        va_color,
    );
}

pub(crate) fn hit_profile(
    point: Pos2,
    study: &dyn crate::studies::Indicator,
    visible: Range<usize>,
    coords: &super::ChartMapping,
    price_to_y: &dyn Fn(f64) -> f32,
) -> Option<usize> {
    let tolerance = crate::chart::hit_test::HIT_TOLERANCE;
    study.volume_profiles(visible).into_iter().find_map(|plot| {
        let profile = &plot.profile;
        let (poc, (va_low, va_high)) = profile.poc_and_value_area(plot.config.value_area())?;
        let span = profile_span(&plot, coords);
        if span.start >= span.end || point.x < span.start || point.x > span.end {
            return None;
        }
        let y_at = |row: usize| price_to_y(profile.low + row as f64 * profile.row_height);
        let on_level = [
            (y_at(poc) + y_at(poc + 1)) * 0.5,
            y_at(va_low),
            y_at(va_high + 1),
        ]
        .into_iter()
        .any(|y| (point.y - y).abs() <= tolerance);
        let on_row = profile.volume.iter().enumerate().any(|(row, &volume)| {
            let amount = if plot.config.display == ProfileDisplay::Delta {
                (profile.up[row] - profile.down[row]).abs()
            } else {
                volume
            };
            if amount <= 0.0 {
                return false;
            }
            let width = (amount / profile.volume[poc]) as f32 * (span.end - span.start) * 0.9;
            let left = if plot.right_aligned {
                span.end - width
            } else {
                span.start
            };
            let (a, b) = (y_at(row), y_at(row + 1));
            let top = a.min(b);
            let bottom = (a.max(b) - 1.0).max(top);
            Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + width, bottom))
                .expand(tolerance)
                .contains(point)
        });
        (on_row || on_level).then(|| {
            coords
                .x_to_idx(point.x)
                .clamp(plot.bars.start, plot.bars.end - 1)
        })
    })
}
