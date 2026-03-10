//! Line-based chart renderers
//!
//! Includes: Line, Line with Markers, Step Line

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::volume::render_volume_bars;
use crate::chart::renderers::{PriceScale, RenderContext, StyleColors};
use crate::model::{Bar, PriceSource};

pub(super) fn render_line(
    painter: &Painter,
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    line_color: Color32,
    price_source: PriceSource,
) {
    let (min_price, max_price) = (price_scale.min_price, price_scale.max_price);
    let price_range = (max_price - min_price).max(1e-12);

    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y =
                price_rect.max.y - ((value - min_price) / price_range) as f32 * price_rect.height();
            Pos2::new(x, y)
        })
        .collect();

    if points.len() > 1 {
        painter.add(Shape::line(
            points,
            Stroke::new(DESIGN_TOKENS.stroke.thick, line_color),
        ));
    }

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            bar_width,
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
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    line_color: Color32,
    price_source: PriceSource,
) {
    let (min_price, max_price) = (price_scale.min_price, price_scale.max_price);
    let price_range = (max_price - min_price).max(1e-12);

    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y =
                price_rect.max.y - ((value - min_price) / price_range) as f32 * price_rect.height();
            Pos2::new(x, y)
        })
        .collect();

    if points.len() > 1 {
        painter.add(Shape::line(
            points.clone(),
            Stroke::new(DESIGN_TOKENS.stroke.thick, line_color),
        ));
    }

    // Draw markers
    let marker_radius = 3.0;
    for point in &points {
        painter.add(Shape::circle_filled(*point, marker_radius, line_color));
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
            bar_width,
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
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    line_color: Color32,
    price_source: PriceSource,
) {
    let (min_price, max_price) = (price_scale.min_price, price_scale.max_price);
    let price_range = (max_price - min_price).max(1e-12);

    if visible_data.is_empty() {
        return;
    }

    let mut points = Vec::new();
    let mut prev_y = None;

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
        let y = price_rect.max.y - ((value - min_price) / price_range) as f32 * price_rect.height();

        if let Some(prev) = prev_y {
            // Add horizontal step first
            points.push(Pos2::new(x, prev));
        }
        // Then vertical
        points.push(Pos2::new(x, y));
        prev_y = Some(y);
    }

    if points.len() > 1 {
        painter.add(Shape::line(
            points,
            Stroke::new(DESIGN_TOKENS.stroke.thick, line_color),
        ));
    }

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            bar_width,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}
