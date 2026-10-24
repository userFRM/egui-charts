//! Line-based chart renderers
//!
//! Includes: Line, Line with Markers, Step Line

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Shape, Stroke};

use super::helpers::valued_runs;
use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{Bar, LinePlotStyle, PriceSource};

pub(super) fn render_line(
    painter: &Painter,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &LinePlotStyle,
    price_source: PriceSource,
) {
    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = price_scale.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect();

    draw_bar_line(painter, visible_data, &points, style, false);

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Line with circular markers at each data point
pub(super) fn render_line_with_markers(
    painter: &Painter,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &LinePlotStyle,
    price_source: PriceSource,
    marker_radius: f32,
) {
    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = price_scale.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect();

    draw_bar_line(painter, visible_data, &points, style, false);

    // Draw markers
    for (point, bar) in points
        .iter()
        .zip(visible_data)
        .filter(|(_, bar)| !bar.is_whitespace() && marker_radius.is_finite() && marker_radius > 0.0)
    {
        painter.add(Shape::circle_filled(
            *point,
            marker_radius,
            line_color(bar, style),
        ));
        painter.add(Shape::circle_stroke(
            *point,
            marker_radius,
            Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                DESIGN_TOKENS.semantic.chart.crosshair_label_text,
            ),
        ));
    }

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Step line: horizontal then vertical movement
pub(super) fn render_step_line(
    painter: &Painter,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &LinePlotStyle,
    price_source: PriceSource,
) {
    if visible_data.is_empty() {
        return;
    }

    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            Pos2::new(x, price_scale.price_to_y(value))
        })
        .collect();
    draw_bar_line(painter, visible_data, &points, style, true);

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// The colour of the line from `bar` to the next bar: the bar's own, or `style`'s.
pub(super) fn line_color(bar: &Bar, style: &LinePlotStyle) -> Color32 {
    bar.colors
        .as_ref()
        .and_then(|own| own.color)
        .unwrap_or(style.color)
}

/// Paint the line through `points`, one per bar of `bars`, with the chart type's own style: broken
/// at whitespace bars, each segment in the [`line_color`] of the bar it starts at, and with
/// `step`, across to the next bar first and then up or down to it.
pub(super) fn draw_bar_line(
    painter: &Painter,
    bars: &[Bar],
    points: &[Pos2],
    style: &LinePlotStyle,
    step: bool,
) {
    for run in valued_runs(bars) {
        let mut from = run.start;
        while from + 1 < run.end {
            let color = line_color(&bars[from], style);
            let to = (from + 1..run.end - 1)
                .find(|&at| line_color(&bars[at], style) != color)
                .unwrap_or(run.end - 1);
            let mut path = vec![points[from]];
            for at in from + 1..=to {
                if step {
                    path.push(Pos2::new(points[at].x, points[at - 1].y));
                }
                path.push(points[at]);
            }
            let style = LinePlotStyle {
                color,
                ..style.clone()
            };
            draw_styled_line(painter, path, &style);
            from = to;
        }
    }
}

/// Paint a line with the chart type's own style; dashes keep their phase through corners.
pub(super) fn draw_styled_line(painter: &Painter, points: Vec<Pos2>, style: &LinePlotStyle) {
    if !style.width.is_finite() || style.width <= 0.0 {
        return;
    }
    let stroke = Stroke::new(style.width, style.color);
    if let Some((dash, gap)) = style.style.dash_pattern() {
        crate::chart::rendering::dashed_line::draw_dashed_polyline(
            painter, &points, stroke, dash, gap,
        );
    } else {
        painter.add(Shape::line(points, stroke));
    }
}
