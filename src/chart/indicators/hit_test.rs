//! Hit testing for overlay indicators and pane indicators.
//!
//! Detects clicks on indicator lines (SMA, EMA, etc.) on the main chart
//! and on indicator panes (RSI, MACD, etc.).

use crate::chart::coords::ChartMapping;
use crate::chart::hit_test::{HIT_TOLERANCE, point_to_segment_distance};
use crate::studies::{Indicator, PlotType};
use egui::{Pos2, Rect};

/// Describes which indicator was hit by a click, and at which bar.
#[derive(Clone, Debug)]
pub struct IndicatorHitResult {
    /// Index of the indicator in the registry that was hit
    pub indicator_idx: usize,
    /// Index of the bar where the hit occurred
    pub bar_idx: usize,
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

    let line_cnt = indicator.line_cnt();

    // Test each line of the indicator
    for line_idx in 0..line_cnt {
        if !indicator.plot_style(line_idx).visible {
            continue;
        }
        if let Some(bar_idx) = hit_test_indicator_line(
            click_pos,
            indicator,
            line_idx,
            &visible_range,
            coords.rect,
            coords,
            &price_to_y,
        ) {
            return Some(IndicatorHitResult {
                indicator_idx,
                bar_idx,
            });
        }
    }

    None
}

/// Test the geometry painted for one output line.
fn hit_test_indicator_line<F>(
    click_pos: Pos2,
    indicator: &dyn Indicator,
    line_idx: usize,
    visible_range: &std::ops::Range<usize>,
    rect: Rect,
    coords: &ChartMapping,
    value_to_y: F,
) -> Option<usize>
where
    F: Fn(f64) -> f32,
{
    let mut plot = indicator.plot_style(line_idx);
    if plot.plot_type == PlotType::Columns
        || (indicator.is_volume() && plot.plot_type == PlotType::Histogram)
    {
        plot.width = coords.bar_spacing * 0.8;
    }
    let end = visible_range
        .end
        .max(coords.x_to_idx(rect.right()).saturating_add(1));
    for bar in visible_range.start..end {
        let Some(value) = indicator.plot_value(line_idx, bar) else {
            continue;
        };
        let point = Pos2::new(coords.idx_to_x(bar), value_to_y(value));
        if !point.is_finite() {
            continue;
        }
        let hit = match plot.plot_type {
            PlotType::BackgroundColor => (click_pos.x - point.x).abs() <= coords.bar_spacing * 0.5,
            PlotType::BarColor => false,
            PlotType::Histogram | PlotType::Columns | PlotType::Area => {
                let baseline = value_to_y(0.0);
                let baseline = if baseline.is_finite() {
                    baseline.clamp(rect.top(), rect.bottom())
                } else {
                    rect.bottom()
                };
                Rect::from_two_pos(
                    Pos2::new(point.x - plot.width * 0.5, baseline),
                    Pos2::new(point.x + plot.width * 0.5, point.y),
                )
                .expand(HIT_TOLERANCE)
                .contains(click_pos)
            }
            PlotType::Circles
            | PlotType::Crosses
            | PlotType::TriangleUp
            | PlotType::TriangleDown => click_pos.distance(point) <= plot.width + HIT_TOLERANCE,
            PlotType::Line | PlotType::StepLine => {
                let next = (bar + 1 < end)
                    .then(|| indicator.plot_value(line_idx, bar + 1))
                    .flatten();
                let next = next.map(|v| Pos2::new(coords.idx_to_x(bar + 1), value_to_y(v)));
                let next = next.unwrap_or(point);
                let distance = if plot.plot_type == PlotType::StepLine {
                    let corner = Pos2::new(next.x, point.y);
                    point_to_segment_distance(click_pos, point, corner)
                        .min(point_to_segment_distance(click_pos, corner, next))
                } else {
                    point_to_segment_distance(click_pos, point, next)
                };
                distance <= HIT_TOLERANCE + plot.width * 0.5
            }
        };
        if hit {
            return Some(bar);
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
        if !indicator.plot_style(line_idx).visible {
            continue;
        }
        if let Some(bar_idx) = hit_test_indicator_line(
            click_pos,
            indicator,
            line_idx,
            &visible_range,
            chart_rect,
            coords,
            value_to_y,
        ) {
            return Some(IndicatorHitResult {
                indicator_idx,
                bar_idx,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::studies::{ConfiguredStudy, CustomIndicator, IndicatorValue, PlotType};

    #[test]
    fn selection_follows_plot_geometry_in_overlays_and_panes() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 100.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            0.0,
            100.0,
            false,
        );
        let coords = ChartMapping::new(rect, 100.0, 0, 1, 0.0, price);
        for overlay in [true, false] {
            for (kind, point, hit) in [
                (PlotType::Histogram, Pos2::new(49.0, 60.0), true),
                (PlotType::Histogram, Pos2::new(99.0, 20.0), false),
                (PlotType::Circles, Pos2::new(99.0, 20.0), false),
                (PlotType::Circles, Pos2::new(149.0, 20.0), true),
                (PlotType::Line, Pos2::new(99.0, 20.0), true),
            ] {
                let mut study = ConfiguredStudy::new(Box::new(
                    CustomIndicator::new("Plot", |_| vec![IndicatorValue::Single(80.0); 2])
                        .with_overlay(overlay),
                ));
                study.calculate(&[]);
                let mut settings = study.settings();
                settings.plots[0].plot_type = kind;
                study.apply_settings(&[], settings).unwrap();
                let found = if overlay {
                    hit_test_indicator(point, &study, 3, 0..2, &coords, |v| 100.0 - v as f32)
                } else {
                    hit_test_pane_indicator(point, &study, 3, 0..2, rect, 0.0, 100.0, &coords)
                };
                assert_eq!(
                    found.is_some(),
                    hit,
                    "{kind:?}, overlay {overlay}, {point:?}"
                );
                if let Some(found) = found {
                    assert_eq!(found.indicator_idx, 3);
                }
            }
        }
    }
}
