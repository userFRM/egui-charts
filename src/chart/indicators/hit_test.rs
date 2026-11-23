//! Hit testing for overlay indicators and pane indicators.
//!
//! Detects clicks on indicator lines (SMA, EMA, etc.) on the main chart
//! and on indicator panes (RSI, MACD, etc.).

use super::plots::{candle_geometry, zone_rect};
use super::projection::projection_rect;
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

    if let Some(bar_idx) = crate::chart::rendering::volume_profile::hit_profile(
        click_pos,
        indicator,
        coords,
        &price_to_y,
    ) {
        return Some(IndicatorHitResult {
            indicator_idx,
            bar_idx,
        });
    }

    #[cfg(feature = "drawings")]
    if let Some(bar_idx) = hit_drawings(click_pos, indicator, coords, &price_to_y) {
        return Some(IndicatorHitResult {
            indicator_idx,
            bar_idx,
        });
    }

    let line_count = indicator.line_count();

    // Test each line of the indicator
    for line_idx in 0..line_count {
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

/// The bar under a click on one of the study's drawings.
#[cfg(feature = "drawings")]
fn hit_drawings(
    click_pos: Pos2,
    indicator: &dyn Indicator,
    coords: &ChartMapping,
    value_to_y: impl Fn(f64) -> f32,
) -> Option<usize> {
    if indicator.drawings().is_empty() {
        return None;
    }
    let drawings = crate::chart::indicators::plots::mapped_drawings(indicator, coords, value_to_y);
    crate::drawings::services::hit_test(click_pos, &drawings, "")
        .map(|_| coords.x_to_idx(click_pos.x))
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
    // Plot 0 is hit through the drawings it styles, which the callers test first.
    #[cfg(feature = "drawings")]
    if line_idx == 0 && !indicator.drawings().is_empty() && indicator.drawings_use_plot_style() {
        return None;
    }
    let mut plot = indicator.plot_style(line_idx);
    let bar_spacing = coords.bar_spacing_for_width();
    if !indicator.candles().is_empty() {
        return indicator
            .candles()
            .iter()
            .filter(|candle| candle.plot == line_idx)
            .find_map(|candle| {
                let (body, wick) = candle_geometry(candle, coords, bar_spacing, &value_to_y)?;
                (body.expand(HIT_TOLERANCE).contains(click_pos)
                    || point_to_segment_distance(click_pos, wick[0], wick[1])
                        <= HIT_TOLERANCE + plot.width * 0.5)
                    .then_some(
                        coords
                            .x_to_idx(click_pos.x)
                            .clamp(candle.bars.start, candle.bars.end - 1),
                    )
            });
    }
    if let Some(zone) = indicator
        .zones()
        .iter()
        .filter(|zone| zone.plot == line_idx)
        .find(|zone| {
            zone_rect(
                zone,
                indicator.zones_cover_whole_bars(),
                coords,
                &value_to_y,
            )
            .is_some_and(|rect| rect.expand(HIT_TOLERANCE).contains(click_pos))
        })
    {
        return Some(
            coords
                .x_to_idx(click_pos.x)
                .clamp(zone.bars.start, zone.bars.end - 1),
        );
    }
    if let Some(projection) = indicator
        .projections()
        .iter()
        .filter(|projection| projection.plot == line_idx)
        .find(|projection| {
            projection_rect(projection, coords, &value_to_y)
                .is_some_and(|rect| rect.expand(HIT_TOLERANCE).contains(click_pos))
        })
    {
        return Some(projection.bars.start as usize);
    }
    if plot.plot_type == PlotType::Columns
        || (indicator.is_volume() && plot.plot_type == PlotType::Histogram)
    {
        plot.width = coords.volume_bar_width();
    }
    // Only bars within reach of the click can be hit, so a zoomed-out chart tests a few of them.
    let reach = HIT_TOLERANCE + plot.width + bar_spacing;
    let start = visible_range
        .start
        .max(coords.x_to_idx(click_pos.x - reach).saturating_sub(1));
    let end = visible_range
        .end
        .max(coords.x_to_idx(rect.right()).saturating_add(1))
        .min(coords.x_to_idx(click_pos.x + reach).saturating_add(2));
    let joins = plot.plot_type.joins_gaps();
    // A joined segment that crosses the reach starts at the defined bar before it.
    // shortcut: scans a gap on each test, O(gap length); keep the nearest defined bars if long
    // gaps show in profiles.
    let start = if joins {
        (0..start)
            .rev()
            .find(|&bar| indicator.plot_value(line_idx, bar).is_some())
            .unwrap_or(start)
    } else {
        start
    };
    let last = indicator
        .values()
        .len()
        .saturating_add_signed(indicator.plot_offset(line_idx));
    // Where a segment from `bar` ends: the next defined value of a joined plot, the next bar's
    // value otherwise.
    let next_point = |bar: usize| {
        let (next, value) = if joins {
            (bar + 1..last).find_map(|next| Some((next, indicator.plot_value(line_idx, next)?)))?
        } else {
            (bar + 1, indicator.plot_value(line_idx, bar + 1)?)
        };
        Some(Pos2::new(coords.idx_to_x(next), value_to_y(value)))
    };
    for bar in start..end {
        let Some(value) = indicator.plot_value(line_idx, bar) else {
            continue;
        };
        let point = Pos2::new(coords.idx_to_x(bar), value_to_y(value));
        if !point.is_finite() {
            continue;
        }
        let hit = match plot.plot_type {
            PlotType::BackgroundColor => (click_pos.x - point.x).abs() <= bar_spacing * 0.5,
            PlotType::BarColor => false,
            PlotType::Histogram | PlotType::Columns => {
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
            // On the line, which reaches half its width past the fill, or inside the fill.
            PlotType::Area | PlotType::AreaWithBreaks => {
                let Some(next) = next_point(bar) else {
                    continue;
                };
                if !next.is_finite() {
                    continue;
                }
                point_to_segment_distance(click_pos, point, next)
                    <= HIT_TOLERANCE + plot.width * 0.5
                    || ((point.x..=next.x).contains(&click_pos.x) && {
                        let t = (click_pos.x - point.x) / (next.x - point.x);
                        let edge = point.y + t * (next.y - point.y);
                        let baseline = value_to_y(0.0);
                        let baseline = if baseline.is_finite() {
                            baseline.clamp(rect.top(), rect.bottom())
                        } else {
                            rect.bottom()
                        };
                        (edge.min(baseline)..=edge.max(baseline)).contains(&click_pos.y)
                    })
            }
            PlotType::Circles
            | PlotType::Crosses
            | PlotType::TriangleUp
            | PlotType::TriangleDown => click_pos.distance(point) <= plot.width + HIT_TOLERANCE,
            PlotType::Line
            | PlotType::LineWithBreaks
            | PlotType::StepLine
            | PlotType::StepLineWithBreaks
            | PlotType::StepLineWithDiamonds => {
                let next = if joins {
                    next_point(bar)
                } else {
                    (bar + 1 < end).then(|| next_point(bar)).flatten()
                };
                let next = next.unwrap_or(point);
                let distance = if matches!(
                    plot.plot_type,
                    PlotType::StepLine
                        | PlotType::StepLineWithBreaks
                        | PlotType::StepLineWithDiamonds
                ) {
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
    coords: &ChartMapping,
) -> Option<IndicatorHitResult> {
    let mapped = matches!(indicator.scale(), crate::studies::StudyScale::Shared(_))
        .then(|| crate::chart::rendering::study_mapping(indicator, coords));
    let coords = mapped.as_ref().unwrap_or(coords);
    // Only test non-overlay indicators
    if indicator.is_overlay() || indicator.plots_on_price() || !indicator.is_visible() {
        return None;
    }

    // Ensure click is within chart area
    if !chart_rect.contains(click_pos) {
        return None;
    }

    if let Some(bar_idx) = crate::chart::rendering::volume_profile::hit_profile(
        click_pos,
        indicator,
        coords,
        &|price| coords.price_to_y(price),
    ) {
        return Some(IndicatorHitResult {
            indicator_idx,
            bar_idx,
        });
    }

    #[cfg(feature = "drawings")]
    if let Some(bar_idx) = hit_drawings(click_pos, indicator, coords, |price| {
        coords.price_to_y(price)
    }) {
        return Some(IndicatorHitResult {
            indicator_idx,
            bar_idx,
        });
    }

    let line_count = indicator.line_count();

    let value_to_y = |value| coords.price_to_y(value);

    // Test each line of the indicator
    for line_idx in 0..line_count {
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
        // Whether a click at `point` selects a study with `values` plotted as `kind`.
        let selects = |values: &[IndicatorValue],
                       coords: &ChartMapping,
                       kind: PlotType,
                       point: Pos2,
                       overlay: bool| {
            let values = values.to_vec();
            let visible = 0..values.len();
            let mut study = ConfiguredStudy::new(Box::new(
                CustomIndicator::new("Plot", move |_| values.clone()).with_overlay(overlay),
            ));
            study.calculate(&[]);
            let mut settings = study.settings();
            settings.plots[0].plot_type = kind;
            study.apply_settings(&[], settings).unwrap();
            let found = if overlay {
                hit_test_indicator(point, &study, 3, visible, coords, |v| 100.0 - v as f32)
            } else {
                hit_test_pane_indicator(point, &study, 3, visible, rect, coords)
            };
            if let Some(found) = &found {
                assert_eq!(found.indicator_idx, 3);
            }
            found.is_some()
        };
        // A missing value between two defined ones: x 74, 124 and 174, y 20, none and 60.
        let gapped = ChartMapping::new(rect, 50.0, 0, 2, 0.0, price);
        let gap = [
            IndicatorValue::Single(80.0),
            IndicatorValue::None,
            IndicatorValue::Single(40.0),
        ];
        for overlay in [true, false] {
            for (kind, point, hit) in [
                (PlotType::Histogram, Pos2::new(49.0, 60.0), true),
                (PlotType::Histogram, Pos2::new(99.0, 20.0), false),
                (PlotType::Circles, Pos2::new(99.0, 20.0), false),
                (PlotType::Circles, Pos2::new(149.0, 20.0), true),
                (PlotType::Line, Pos2::new(99.0, 20.0), true),
                (PlotType::Columns, Pos2::new(79.0, 60.0), true),
                (PlotType::StepLine, Pos2::new(99.0, 20.0), true),
                (PlotType::Area, Pos2::new(99.0, 60.0), true),
                // Above the fill, on the line's tolerance: the values sit at y 20.
                (PlotType::Area, Pos2::new(99.0, 17.0), true),
                (PlotType::Area, Pos2::new(99.0, 5.0), false),
                (PlotType::Area, Pos2::new(160.0, 60.0), false),
            ] {
                let values = vec![IndicatorValue::Single(80.0); 2];
                assert_eq!(
                    selects(&values, &coords, kind, point, overlay),
                    hit,
                    "{kind:?}, overlay {overlay}, {point:?}"
                );
            }
            // Joined types are selected on the segment across the gap, the types with breaks
            // are not.
            for (kind, point, hit) in [
                (PlotType::Line, Pos2::new(124.0, 40.0), true),
                (PlotType::LineWithBreaks, Pos2::new(124.0, 40.0), false),
                (PlotType::StepLine, Pos2::new(124.0, 20.0), true),
                (PlotType::StepLineWithBreaks, Pos2::new(124.0, 20.0), false),
                (PlotType::StepLineWithDiamonds, Pos2::new(124.0, 20.0), true),
                (PlotType::Area, Pos2::new(124.0, 70.0), true),
                (PlotType::AreaWithBreaks, Pos2::new(124.0, 70.0), false),
            ] {
                assert_eq!(
                    selects(&gap, &gapped, kind, point, overlay),
                    hit,
                    "{kind:?} across a gap, overlay {overlay}, {point:?}"
                );
            }
            #[cfg(feature = "studies")]
            {
                let mut study = ConfiguredStudy::new(Box::new(
                    crate::studies::HigherIntervalCandles::default(),
                ));
                study.set_timeframe(crate::model::Timeframe::Hour1);
                study.set_overlay(overlay);
                let start: chrono::DateTime<chrono::Utc> = "2024-03-11T00:00:00Z".parse().unwrap();
                study.calculate(&[
                    crate::model::Bar::new(start, 30.0, 80.0, 10.0, 50.0, 1.0),
                    crate::model::Bar::new(
                        start + chrono::TimeDelta::hours(1),
                        50.0,
                        60.0,
                        40.0,
                        50.0,
                        1.0,
                    ),
                ]);
                for visible in [true, false] {
                    let mut settings = study.settings();
                    settings.plots[0].visible = visible;
                    study.apply_settings(&[], settings).unwrap();
                    for (point, expected) in [
                        (Pos2::new(99.0, 60.0), true),
                        (Pos2::new(99.0, 20.0), true),
                        (Pos2::new(49.0, 20.0), false),
                    ] {
                        let found = if overlay {
                            hit_test_indicator(point, &study, 3, 0..2, &coords, |v| {
                                100.0 - v as f32
                            })
                        } else {
                            hit_test_pane_indicator(point, &study, 3, 0..2, rect, &coords)
                        };
                        assert_eq!(
                            found.is_some(),
                            expected && visible,
                            "candle, overlay {overlay}, {point:?}"
                        );
                    }
                }
            }
        }
    }
}
