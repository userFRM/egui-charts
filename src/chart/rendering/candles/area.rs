//! Area-based chart renderers
//!
//! Includes: Area, HLC Area, Baseline

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{Bar, BaselineLevel, PriceSource};

const AREA_ALPHA: u8 = 50;
const HLC_AREA_ALPHA: u8 = 30;
const BASELINE_FILL_ALPHA: u8 = 30;

pub(super) fn render_area(
    painter: &Painter,
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    fill_color: Color32,
    price_source: PriceSource,
) {
    let coords = *price_scale;
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
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// HLC Area: fills high to close in the bullish colour and close to low in the bearish colour,
/// with the close line
pub(super) fn render_hlc_area(
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
    bullish_color: Color32,
    bearish_color: Color32,
    price_source: PriceSource,
) {
    if visible_data.is_empty() {
        return;
    }

    let coords = *price_scale;
    let points = collect_hlc_points(
        visible_data,
        start_idx,
        &coords,
        chart_rect_min_x,
        &idx_to_coord,
        price_source,
    );

    if points.len() > 1 {
        draw_hlc_fill(painter, &points, bullish_color, bearish_color);
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
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Baseline chart: coloured by the side of the base line, split where a segment crosses it
pub(super) fn render_baseline(
    painter: &Painter,
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
    price_source: PriceSource,
    level: BaselineLevel,
) {
    if visible_data.is_empty() {
        return;
    }

    let coords = *price_scale;
    let baseline_y = match level {
        BaselineLevel::Price(price) => coords.price_to_y(price),
        BaselineLevel::PercentOfPane(percent) => {
            price_rect.max.y - percent / 100.0 * price_rect.height()
        }
    };

    draw_baseline_line(painter, price_rect, baseline_y);
    draw_baseline_segments(
        painter,
        visible_data,
        start_idx,
        &coords,
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
    coords: &PriceMap,
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
    coords: &PriceMap,
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

fn draw_hlc_fill(painter: &Painter, points: &[HlcPoint], bullish: Color32, bearish: Color32) {
    let tint = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), HLC_AREA_ALPHA);
    let (up, down) = (tint(bullish), tint(bearish));

    // Both quads are convex: high >= close >= low at both ends of a segment.
    for pair in points.windows(2) {
        let (p1, p2) = (&pair[0], &pair[1]);
        let above = vec![
            Pos2::new(p1.x, p1.y_high),
            Pos2::new(p2.x, p2.y_high),
            Pos2::new(p2.x, p2.y_close),
            Pos2::new(p1.x, p1.y_close),
        ];
        let below = vec![
            Pos2::new(p1.x, p1.y_close),
            Pos2::new(p2.x, p2.y_close),
            Pos2::new(p2.x, p2.y_low),
            Pos2::new(p1.x, p1.y_low),
        ];
        painter.add(Shape::convex_polygon(above, up, Stroke::NONE));
        painter.add(Shape::convex_polygon(below, down, Stroke::NONE));
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
    coords: &PriceMap,
    baseline_y: f32,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
    price_source: PriceSource,
) {
    let point = |i: usize| {
        let bar = &data[i];
        let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
        Pos2::new(
            idx_to_coord(start_idx + i, chart_rect_min_x),
            coords.price_to_y(value),
        )
    };
    for i in 1..data.len() {
        let (prev, curr) = (point(i - 1), point(i));
        // A segment that crosses the base line is split there, in screen space, where the
        // line is straight on every price scale.
        let crossing = (prev.y < baseline_y && curr.y > baseline_y)
            || (prev.y > baseline_y && curr.y < baseline_y);
        if crossing {
            let t = (baseline_y - prev.y) / (curr.y - prev.y);
            let cross = Pos2::new(prev.x + (curr.x - prev.x) * t, baseline_y);
            draw_baseline_part(
                painter,
                coords,
                prev,
                cross,
                baseline_y,
                bullish_color,
                bearish_color,
            );
            draw_baseline_part(
                painter,
                coords,
                cross,
                curr,
                baseline_y,
                bullish_color,
                bearish_color,
            );
        } else {
            draw_baseline_part(
                painter,
                coords,
                prev,
                curr,
                baseline_y,
                bullish_color,
                bearish_color,
            );
        }
    }
}

/// A part of a segment that does not cross the base line: its line and its fill down to the base.
fn draw_baseline_part(
    painter: &Painter,
    coords: &PriceMap,
    a: Pos2,
    b: Pos2,
    baseline_y: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    // The side is decided by value, so an inverted price scale keeps its colours; a part that
    // only touches the base line takes its other side.
    let color = if coords.y_to_price(f32::midpoint(a.y, b.y)) >= coords.y_to_price(baseline_y) {
        bullish_color
    } else {
        bearish_color
    };
    painter.add(Shape::line_segment(
        [a, b],
        Stroke::new(DESIGN_TOKENS.stroke.thick, color),
    ));

    let fill_color =
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), BASELINE_FILL_ALPHA);
    let fill_points = vec![a, b, Pos2::new(b.x, baseline_y), Pos2::new(a.x, baseline_y)];
    painter.add(Shape::convex_polygon(fill_points, fill_color, Stroke::NONE));
}
