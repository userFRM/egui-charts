//! Area-based chart renderers
//!
//! Includes: Area, HLC Area, Baseline

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::helpers::PriceCoords;
use super::volume::render_volume_bars;
use crate::chart::renderers::{PriceScale, RenderContext, StyleColors};
use crate::model::{Bar, PriceSource};

const AREA_ALPHA: u8 = 50;
const HLC_AREA_ALPHA: u8 = 30;
const BASELINE_FILL_ALPHA: u8 = 30;

pub(super) fn render_area(
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
    fill_color: Color32,
    price_source: PriceSource,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let line_points = collect_price_points(
        visible_data,
        start_idx,
        &coords,
        chart_rect_min_x,
        &idx_to_coord,
        price_source,
    );

    if line_points.len() > 1 {
        draw_area_fill(
            painter,
            &line_points,
            price_rect.max.y,
            fill_color,
            AREA_ALPHA,
        );
        painter.add(Shape::line(
            line_points,
            Stroke::new(DESIGN_TOKENS.stroke.thick, fill_color),
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

/// HLC Area: fills between high and low with close line
pub(super) fn render_hlc_area(
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
    bullish_color: Color32,
    _bearish_color: Color32,
    price_source: PriceSource,
) {
    if visible_data.is_empty() {
        return;
    }

    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let points = collect_hlc_points(
        visible_data,
        start_idx,
        &coords,
        chart_rect_min_x,
        &idx_to_coord,
        price_source,
    );

    if points.len() > 1 {
        draw_hlc_fill(painter, &points, bullish_color);
        let close_points: Vec<Pos2> = points.iter().map(|p| Pos2::new(p.x, p.y_close)).collect();
        painter.add(Shape::line(
            close_points,
            Stroke::new(DESIGN_TOKENS.stroke.thick, bullish_color),
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

/// Baseline chart: colored based on position relative to baseline
pub(super) fn render_baseline(
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
    bullish_color: Color32,
    bearish_color: Color32,
    price_source: PriceSource,
) {
    let first = match visible_data.first() {
        Some(b) => b,
        None => return,
    };

    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let baseline = price_source.compute(first.open, first.high, first.low, first.close);
    let baseline_y = coords.price_to_y(baseline);

    draw_baseline_line(painter, price_rect, baseline_y);
    draw_baseline_segments(
        painter,
        visible_data,
        start_idx,
        &coords,
        baseline,
        baseline_y,
        chart_rect_min_x,
        &idx_to_coord,
        bullish_color,
        bearish_color,
        price_source,
    );

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

// === Helper Functions ===

fn collect_price_points(
    data: &[Bar],
    start_idx: usize,
    coords: &PriceCoords,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    price_source: PriceSource,
) -> Vec<Pos2> {
    data.iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = coords.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect()
}

struct HlcPoint {
    x: f32,
    y_high: f32,
    y_low: f32,
    y_close: f32,
}

fn collect_hlc_points(
    data: &[Bar],
    start_idx: usize,
    coords: &PriceCoords,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    price_source: PriceSource,
) -> Vec<HlcPoint> {
    data.iter()
        .enumerate()
        .map(|(i, bar)| {
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            HlcPoint {
                x: idx_to_coord(start_idx + i, chart_rect_min_x),
                y_high: coords.price_to_y(bar.high),
                y_low: coords.price_to_y(bar.low),
                y_close: coords.price_to_y(value),
            }
        })
        .collect()
}

fn draw_area_fill(painter: &Painter, points: &[Pos2], baseline_y: f32, color: Color32, alpha: u8) {
    let area_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

    for i in 0..points.len() - 1 {
        let p1 = points[i];
        let p2 = points[i + 1];
        let quad = vec![
            p1,
            p2,
            Pos2::new(p2.x, baseline_y),
            Pos2::new(p1.x, baseline_y),
        ];
        painter.add(Shape::convex_polygon(quad, area_color, Stroke::NONE));
    }
}

fn draw_hlc_fill(painter: &Painter, points: &[HlcPoint], color: Color32) {
    let area_color =
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), HLC_AREA_ALPHA);

    for i in 0..points.len() - 1 {
        let p1 = &points[i];
        let p2 = &points[i + 1];
        let quad = vec![
            Pos2::new(p1.x, p1.y_high),
            Pos2::new(p2.x, p2.y_high),
            Pos2::new(p2.x, p2.y_low),
            Pos2::new(p1.x, p1.y_low),
        ];
        painter.add(Shape::convex_polygon(quad, area_color, Stroke::NONE));
    }
}

fn draw_baseline_line(painter: &Painter, rect: Rect, baseline_y: f32) {
    painter.add(Shape::line_segment(
        [
            Pos2::new(rect.min.x, baseline_y),
            Pos2::new(rect.max.x, baseline_y),
        ],
        Stroke::new(
            DESIGN_TOKENS.stroke.hairline,
            DESIGN_TOKENS.semantic.extended.gray,
        ),
    ));
}

fn draw_baseline_segments(
    painter: &Painter,
    data: &[Bar],
    start_idx: usize,
    coords: &PriceCoords,
    baseline: f64,
    baseline_y: f32,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
    price_source: PriceSource,
) {
    for i in 1..data.len() {
        let prev_bar = &data[i - 1];
        let curr_bar = &data[i];

        let prev_x = idx_to_coord(start_idx + i - 1, chart_rect_min_x);
        let curr_x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let prev_value =
            price_source.compute(prev_bar.open, prev_bar.high, prev_bar.low, prev_bar.close);
        let curr_value =
            price_source.compute(curr_bar.open, curr_bar.high, curr_bar.low, curr_bar.close);
        let prev_y = coords.price_to_y(prev_value);
        let curr_y = coords.price_to_y(curr_value);

        let color = if curr_value >= baseline {
            bullish_color
        } else {
            bearish_color
        };

        painter.add(Shape::line_segment(
            [Pos2::new(prev_x, prev_y), Pos2::new(curr_x, curr_y)],
            Stroke::new(DESIGN_TOKENS.stroke.thick, color),
        ));

        let fill_color =
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), BASELINE_FILL_ALPHA);
        let fill_points = vec![
            Pos2::new(prev_x, prev_y),
            Pos2::new(curr_x, curr_y),
            Pos2::new(curr_x, baseline_y),
            Pos2::new(prev_x, baseline_y),
        ];
        painter.add(Shape::convex_polygon(fill_points, fill_color, Stroke::NONE));
    }
}
