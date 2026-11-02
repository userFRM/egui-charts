//! Horizontal price-row histograms shared by drawing and study profiles.

use crate::data::{ProfileDisplay, StudyProfile};
use egui::{Pos2, Rect};
use std::ops::Range;

pub(crate) fn profile_range(coords: &super::ChartMapping) -> Range<usize> {
    coords.x_to_idx(coords.rect.left())..coords.x_to_idx(coords.rect.right()).saturating_add(1)
}

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

pub(crate) fn hit_profile(
    point: Pos2,
    study: &dyn crate::studies::Indicator,
    coords: &super::ChartMapping,
    price_to_y: &dyn Fn(f64) -> f32,
) -> Option<usize> {
    let tolerance = crate::chart::hit_test::HIT_TOLERANCE;
    let profiles = study.volume_profiles(profile_range(coords));
    profiles.into_iter().find_map(|plot| {
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
            let bottom = a.max(b);
            let bottom = bottom - ((bottom - top) * 0.5).min(1.0);
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
