//! Hit testing for overlay indicators and pane indicators.
//!
//! Detects clicks on indicator lines (SMA, EMA, etc.) on the main chart
//! and on indicator panes (RSI, MACD, etc.).

use crate::chart::coords::ChartMapping;
use crate::chart::hit_test::{HIT_TOLERANCE, point_to_segment_distance};
use crate::studies::{Indicator, IndicatorValue};
use egui::{Pos2, Rect};

use super::selection::IndicatorId;

/// Describes which indicator line was hit by a click, and where.
#[derive(Clone, Debug)]
pub struct IndicatorHitResult {
    /// The indicator that was hit (by index in registry)
    pub indicator_id: IndicatorId,
    /// Index of the line that was hit (for multi-line indicators)
    pub line_idx: usize,
    /// Index of the bar where the hit occurred
    pub bar_idx: usize,
    /// Screen position of the hit
    pub position: Pos2,
}

/// Hit test an overlay indicator
///
/// Returns the hit result if the click position is on or near the indicator line.
pub fn hit_test_indicator<F>(
    click_pos: Pos2,
    indicator: &dyn Indicator,
    indicator_idx: usize,
    visible_range: std::ops::Range<usize>,
    coords: &ChartMapping,
    price_to_y: F,
) -> Option<IndicatorHitResult>
where
    F: Fn(f64) -> f32,
{
    // Only test overlay indicators
    if !indicator.is_overlay() || !indicator.is_visible() {
        return None;
    }

    // Ensure click is within chart area
    if !coords.rect.contains(click_pos) {
        return None;
    }

    let values = indicator.values();
    let line_cnt = indicator.line_cnt();

    // Test each line of the indicator
    for line_idx in 0..line_cnt {
        if let Some(hit) = hit_test_indicator_line(
            click_pos,
            values,
            line_idx,
            indicator_idx,
            &visible_range,
            coords,
            &price_to_y,
        ) {
            return Some(hit);
        }
    }

    None
}

/// Hit test a specific line of an indicator
fn hit_test_indicator_line<F>(
    click_pos: Pos2,
    values: &[IndicatorValue],
    line_idx: usize,
    indicator_idx: usize,
    visible_range: &std::ops::Range<usize>,
    coords: &ChartMapping,
    price_to_y: F,
) -> Option<IndicatorHitResult>
where
    F: Fn(f64) -> f32,
{
    // Extract values for this line
    let line_values: Vec<Option<f64>> = values
        .iter()
        .map(|v| match v {
            IndicatorValue::Single(val) => Some(*val),
            IndicatorValue::Multiple(vals) => vals.get(line_idx).copied(),
            IndicatorValue::None => None,
        })
        .collect();

    // Check each segment
    for i in visible_range.start..visible_range.end.saturating_sub(1) {
        let v1 = line_values.get(i).and_then(|v| *v);
        let v2 = line_values.get(i + 1).and_then(|v| *v);

        let (Some(v1), Some(v2)) = (v1, v2) else {
            continue;
        };

        let x1 = coords.idx_to_x(i);
        let x2 = coords.idx_to_x(i + 1);

        // Skip if outside visible area
        if x1 > coords.rect.max.x && x2 > coords.rect.max.x {
            continue;
        }
        if x1 < coords.rect.min.x && x2 < coords.rect.min.x {
            continue;
        }

        // Check if click x is within this segment (with tolerance)
        let min_x = x1.min(x2) - HIT_TOLERANCE;
        let max_x = x1.max(x2) + HIT_TOLERANCE;
        if click_pos.x < min_x || click_pos.x > max_x {
            continue;
        }

        let y1 = price_to_y(v1);
        let y2 = price_to_y(v2);

        // Calculate distance from point to line segment
        let dist = point_to_segment_distance(click_pos, Pos2::new(x1, y1), Pos2::new(x2, y2));

        if dist <= HIT_TOLERANCE {
            return Some(IndicatorHitResult {
                indicator_id: IndicatorId(indicator_idx),
                line_idx,
                bar_idx: i,
                position: click_pos,
            });
        }
    }

    None
}

/// Hit test a pane indicator (RSI, MACD, etc.)
///
/// Returns the hit result if the click position is on or near the indicator line
/// in a separate indicator pane.
pub fn hit_test_pane_indicator(
    click_pos: Pos2,
    indicator: &dyn Indicator,
    indicator_idx: usize,
    visible_range: std::ops::Range<usize>,
    chart_rect: Rect,
    y_min: f64,
    y_max: f64,
    coords: &ChartMapping,
) -> Option<IndicatorHitResult> {
    // Only test non-overlay indicators
    if indicator.is_overlay() || !indicator.is_visible() {
        return None;
    }

    // Ensure click is within chart area
    if !chart_rect.contains(click_pos) {
        return None;
    }

    let values = indicator.values();
    let line_cnt = indicator.line_cnt();

    // Create value to y converter
    let value_to_y = |value: f64| -> f32 {
        let y_range = y_max - y_min;
        if y_range.abs() < f64::EPSILON {
            return chart_rect.center().y;
        }
        let normalized = (value - y_min) / y_range;
        chart_rect.max.y - (normalized as f32 * chart_rect.height())
    };

    // Test each line of the indicator
    for line_idx in 0..line_cnt {
        if let Some(hit) = hit_test_pane_indicator_line(
            click_pos,
            values,
            line_idx,
            indicator_idx,
            &visible_range,
            chart_rect,
            coords,
            value_to_y,
        ) {
            return Some(hit);
        }
    }

    None
}

/// Hit test a specific line of a pane indicator
fn hit_test_pane_indicator_line<F>(
    click_pos: Pos2,
    values: &[IndicatorValue],
    line_idx: usize,
    indicator_idx: usize,
    visible_range: &std::ops::Range<usize>,
    chart_rect: Rect,
    coords: &ChartMapping,
    value_to_y: F,
) -> Option<IndicatorHitResult>
where
    F: Fn(f64) -> f32,
{
    // Extract values for this line
    let line_values: Vec<Option<f64>> = values
        .iter()
        .map(|v| match v {
            IndicatorValue::Single(val) => Some(*val),
            IndicatorValue::Multiple(vals) => vals.get(line_idx).copied(),
            IndicatorValue::None => None,
        })
        .collect();

    // Check each segment
    for i in visible_range.start..visible_range.end.saturating_sub(1) {
        let v1 = line_values.get(i).and_then(|v| *v);
        let v2 = line_values.get(i + 1).and_then(|v| *v);

        let (Some(v1), Some(v2)) = (v1, v2) else {
            continue;
        };

        let x1 = coords.idx_to_x(i);
        let x2 = coords.idx_to_x(i + 1);

        // Skip if outside visible area
        if x1 > chart_rect.max.x && x2 > chart_rect.max.x {
            continue;
        }
        if x1 < chart_rect.min.x && x2 < chart_rect.min.x {
            continue;
        }

        // Check if click x is within this segment (with tolerance)
        let min_x = x1.min(x2) - HIT_TOLERANCE;
        let max_x = x1.max(x2) + HIT_TOLERANCE;
        if click_pos.x < min_x || click_pos.x > max_x {
            continue;
        }

        let y1 = value_to_y(v1);
        let y2 = value_to_y(v2);

        // Calculate distance from point to line segment
        let dist = point_to_segment_distance(click_pos, Pos2::new(x1, y1), Pos2::new(x2, y2));

        if dist <= HIT_TOLERANCE {
            return Some(IndicatorHitResult {
                indicator_id: IndicatorId(indicator_idx),
                line_idx,
                bar_idx: i,
                position: click_pos,
            });
        }
    }

    None
}
