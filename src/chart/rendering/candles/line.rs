//! Line-based chart renderers
//!
//! Includes: Line, Line with Markers, Step Line

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Painter, Pos2, Shape, Stroke};

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

    if points.len() > 1 {
        draw_styled_line(painter, points, style);
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

    if points.len() > 1 {
        draw_styled_line(painter, points.clone(), style);
    }

    // Draw markers
    for point in points
        .iter()
        .filter(|_| marker_radius.is_finite() && marker_radius > 0.0)
    {
        painter.add(Shape::circle_filled(*point, marker_radius, style.color));
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

    let mut points = Vec::new();
    let mut prev_y = None;

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
        let y = price_scale.price_to_y(value);

        if let Some(prev) = prev_y {
            // Add horizontal step first
            points.push(Pos2::new(x, prev));
        }
        // Then vertical
        points.push(Pos2::new(x, y));
        prev_y = Some(y);
    }

    if points.len() > 1 {
        draw_styled_line(painter, points, style);
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
