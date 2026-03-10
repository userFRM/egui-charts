//! Hit testing for chart series elements.
//!
//! Provides functions to detect user clicks on candlesticks, bars, and lines.

use crate::chart::coords::ChartMapping;
use crate::chart::hit_test::{HIT_TOLERANCE, point_to_segment_distance};
use crate::model::Bar;
use egui::{Pos2, Rect};

use super::selection::{SeriesHitResult, SeriesId};

/// Body width ratio relative to bar spacing (from chart-rendering.md)
pub const BODY_WIDTH_RATIO: f32 = 0.6;

/// Configuration for series hit testing (tolerance, wick inclusion).
pub struct HitTestConfig {
    /// Tolerance in pixels for line/wick hits
    pub line_tolerance: f32,
    /// Whether to include wick hits for candles
    pub include_wicks: bool,
}

impl Default for HitTestConfig {
    fn default() -> Self {
        Self {
            line_tolerance: HIT_TOLERANCE,
            include_wicks: true,
        }
    }
}

/// Hit test candles to find which bar was clicked.
///
/// Uses the same coordinate system as the chart renderer (right-to-left).
pub fn hit_test_candles<F>(
    click_pos: Pos2,
    bars: &[Bar],
    visible_range: std::ops::Range<usize>,
    coords: &ChartMapping,
    price_to_y: F,
    config: &HitTestConfig,
) -> Option<SeriesHitResult>
where
    F: Fn(f64) -> f32,
{
    // Ensure click is within chart area
    if !coords.rect.contains(click_pos) {
        return None;
    }

    let bar_width = coords.bar_width();

    // Check each visible bar
    for i in visible_range {
        let Some(bar) = bars.get(i) else { continue };

        let idx_to_x = coords.idx_to_x(i);

        // Skip if bar is outside visible area
        if !coords.is_x_visible(idx_to_x) {
            continue;
        }

        // Hit test body
        let body_left = idx_to_x - bar_width / 2.0;
        let body_right = idx_to_x + bar_width / 2.0;
        let body_top = price_to_y(bar.open.max(bar.close));
        let body_bottom = price_to_y(bar.open.min(bar.close));

        // Ensure minimum body height for doji candles
        let body_top = body_top.min(body_bottom - 2.0);
        let body_bottom = body_bottom.max(body_top + 2.0);

        let body_rect = Rect::from_min_max(
            Pos2::new(body_left, body_top),
            Pos2::new(body_right, body_bottom),
        );

        if body_rect.contains(click_pos) {
            return Some(SeriesHitResult {
                series_id: SeriesId::MAIN,
                bar_idx: i,
                position: click_pos,
            });
        }

        // Hit test wicks if enabled
        if config.include_wicks {
            let wick_top = price_to_y(bar.high);
            let wick_bottom = price_to_y(bar.low);

            // Check if click is near the wick line
            if (click_pos.x - idx_to_x).abs() <= config.line_tolerance
                && click_pos.y >= wick_top
                && click_pos.y <= wick_bottom
            {
                return Some(SeriesHitResult {
                    series_id: SeriesId::MAIN,
                    bar_idx: i,
                    position: click_pos,
                });
            }
        }
    }

    None
}

/// Hit test volume bars to find which bar was clicked.
///
/// Uses the same coordinate system as the chart renderer (right-to-left).
pub fn hit_test_volume(
    click_pos: Pos2,
    bars: &[Bar],
    visible_range: std::ops::Range<usize>,
    coords: &ChartMapping,
    max_volume: f64,
) -> Option<SeriesHitResult> {
    // Ensure click is within volume area
    if !coords.rect.contains(click_pos) {
        return None;
    }

    let bar_width = coords.bar_width();

    // Check each visible bar
    for i in visible_range {
        let Some(bar) = bars.get(i) else { continue };

        let idx_to_x = coords.idx_to_x(i);

        // Skip if bar is outside visible area
        if !coords.is_x_visible(idx_to_x) {
            continue;
        }

        // Calculate volume bar bounds
        let bar_left = idx_to_x - bar_width / 2.0;
        let bar_right = idx_to_x + bar_width / 2.0;

        // Volume bar goes from bottom up, using full panel height
        // Must match actual volume rendering: bar_height = (vol/max) * height
        let norm = bar.volume / max_volume;
        let bar_height = norm as f32 * coords.rect.height();
        let bar_top = coords.rect.bottom() - bar_height;
        let bar_bottom = coords.rect.bottom();

        let bar_rect = Rect::from_min_max(
            Pos2::new(bar_left, bar_top),
            Pos2::new(bar_right, bar_bottom),
        );

        if bar_rect.contains(click_pos) {
            return Some(SeriesHitResult {
                series_id: SeriesId::VOLUME,
                bar_idx: i,
                position: click_pos,
            });
        }
    }

    None
}

/// Hit test a line series
pub fn hit_test_line<F>(
    click_pos: Pos2,
    values: &[f64],
    visible_range: std::ops::Range<usize>,
    coords: &ChartMapping,
    price_to_y: F,
    tolerance: f32,
) -> Option<SeriesHitResult>
where
    F: Fn(f64) -> f32,
{
    if !coords.rect.contains(click_pos) {
        return None;
    }

    // Find the segment closest to click position
    for i in visible_range.start..visible_range.end.saturating_sub(1) {
        let Some(&v1) = values.get(i) else { continue };
        let Some(&v2) = values.get(i + 1) else {
            continue;
        };

        let x1 = coords.idx_to_x(i);
        let x2 = coords.idx_to_x(i + 1);

        // Check if click x is within this segment (with some tolerance)
        let min_x = x1.min(x2) - tolerance;
        let max_x = x1.max(x2) + tolerance;
        if click_pos.x < min_x || click_pos.x > max_x {
            continue;
        }

        let y1 = price_to_y(v1);
        let y2 = price_to_y(v2);

        // Calculate distance from point to line segment
        let dist = point_to_segment_distance(click_pos, Pos2::new(x1, y1), Pos2::new(x2, y2));

        if dist <= tolerance {
            return Some(SeriesHitResult {
                series_id: SeriesId::MAIN,
                bar_idx: i,
                position: click_pos,
            });
        }
    }

    None
}
