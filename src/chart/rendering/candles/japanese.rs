//! Japanese chart renderers
//!
//! Includes: Renko, Kagi, Line Break, Point & Figure

use super::helpers::{draw_brick, draw_o_symbol, draw_x_symbol};
use crate::model::kagi::KagiLine;
use crate::model::line_break::LineBreakLine;
use crate::model::point_figure::PnfColumn;
use crate::model::{Bar, ColumnDirection, RenkoBrick};
use crate::scales::PriceMap;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

pub(super) fn render_renko(
    painter: &Painter,
    bricks: &[RenkoBrick],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    for (i, brick) in bricks.iter().enumerate() {
        let x = x_of(i);

        let bar = brick.to_bar();
        let color = if bar.close > bar.open {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_brick_rect(x, &bar, &coords, bar_width);
        draw_brick(painter, rect, color, 1.0);
    }
}

pub(super) fn render_kagi(
    painter: &Painter,
    lines: &[KagiLine],
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    if lines.is_empty() {
        return;
    }

    let points = collect_kagi_points(lines, &coords, x_of);
    draw_kagi_segments(painter, &points, bullish_color, bearish_color);
}

/// Line Break chart (Three Line Break)
pub(super) fn render_line_break(
    painter: &Painter,
    lines: &[LineBreakLine],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    if lines.is_empty() {
        return;
    }

    let actual_width = bar_width.max(3.0);
    for (i, line) in lines.iter().enumerate() {
        let x = x_of(i);

        let color = if line.is_bullish() {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_line_break_rect(x, line.open, line.close, &coords, actual_width);
        draw_brick(painter, rect, color, 1.0);
    }
}

/// Point and Figure chart (X and O columns)
pub(super) fn render_point_and_figure(
    painter: &Painter,
    price_rect: Rect,
    columns: &[PnfColumn],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    if columns.is_empty() {
        return;
    }

    let symbol_size = (bar_width * 0.8).max(4.0);
    for (i, column) in columns.iter().enumerate() {
        let x = x_of(i);

        draw_pnf_column(
            painter,
            x,
            column,
            &coords,
            price_rect,
            symbol_size,
            bullish_color,
            bearish_color,
        );
    }
}

// === Helper Functions ===

pub(super) fn calc_brick_rect(x: f32, bar: &Bar, coords: &PriceMap, bar_width: f32) -> Rect {
    let y_open = coords.price_to_y(bar.open);
    let y_close = coords.price_to_y(bar.close);
    let top = y_open.min(y_close);
    let bottom = y_open.max(y_close);
    let height = (bottom - top).max(2.0);
    let width = bar_width.max(3.0);

    Rect::from_min_max(
        Pos2::new(x - width / 2.0, top),
        Pos2::new(x + width / 2.0, top + height),
    )
}

fn calc_line_break_rect(x: f32, open: f64, close: f64, coords: &PriceMap, width: f32) -> Rect {
    let y_open = coords.price_to_y(open);
    let y_close = coords.price_to_y(close);
    let top = y_open.min(y_close);
    let height = (y_open.max(y_close) - top).max(2.0);

    Rect::from_min_max(
        Pos2::new(x - width / 2.0, top),
        Pos2::new(x + width / 2.0, top + height),
    )
}

fn collect_kagi_points(
    lines: &[crate::model::kagi::KagiLine],
    coords: &PriceMap,
    x_of: impl Fn(usize) -> f32,
) -> Vec<(Pos2, crate::model::kagi::KagiThickness)> {
    use crate::model::kagi::KagiThickness;

    let mut points: Vec<(Pos2, KagiThickness)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let x = x_of(i);

        let y_start = coords.price_to_y(line.start_price);
        let y_end = coords.price_to_y(line.end_price);

        if points.is_empty() {
            points.push((Pos2::new(x, y_start), line.thickness));
        }
        points.push((Pos2::new(x, y_end), line.thickness));
    }

    points
}

fn draw_kagi_segments(
    painter: &Painter,
    points: &[(Pos2, crate::model::kagi::KagiThickness)],
    bullish_color: Color32,
    bearish_color: Color32,
) {
    use crate::model::kagi::KagiThickness;

    for i in 0..points.len().saturating_sub(1) {
        let (p1, thickness) = points[i];
        let (p2, _) = points[i + 1];

        let (stroke_width, color) = match thickness {
            KagiThickness::Thick => (2.5_f32, bullish_color),
            KagiThickness::Thin => (1.5_f32, bearish_color),
        };
        let stroke = Stroke::new(stroke_width, color);

        if (p1.x - p2.x).abs() < 0.1 || (p1.y - p2.y).abs() < 0.1 {
            painter.add(Shape::line_segment([p1, p2], stroke));
        } else {
            let shoulder = Pos2::new(p2.x, p1.y);
            painter.add(Shape::line_segment([p1, shoulder], stroke));
            painter.add(Shape::line_segment([shoulder, p2], stroke));
        }
    }
}

fn draw_pnf_column(
    painter: &Painter,
    x: f32,
    column: &crate::model::point_figure::PnfColumn,
    coords: &PriceMap,
    price_rect: Rect,
    symbol_size: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let up = column.direction == ColumnDirection::Up;
    let color = if up { bullish_color } else { bearish_color };
    let step = column.box_size;
    let start = column.start_price;

    // A box under 2 px tall is not drawable as a symbol: draw the column as a line.
    if (coords.price_to_y(start) - coords.price_to_y(start + step)).abs() < 2.0 {
        painter.add(Shape::line_segment(
            [
                Pos2::new(x, coords.price_to_y(start)),
                Pos2::new(x, coords.price_to_y(column.end_price)),
            ],
            Stroke::new(1.5, color),
        ));
        return;
    }

    // Box `k` is at `start +- k * step`; only the boxes inside the pane are visited.
    let (p_a, p_b) = (
        coords.y_to_price(price_rect.max.y),
        coords.y_to_price(price_rect.min.y),
    );
    let (low, high) = (p_a.min(p_b), p_a.max(p_b));
    let (k_a, k_b) = if up {
        ((low - start) / step, (high - start) / step)
    } else {
        ((start - high) / step, (start - low) / step)
    };
    let first = k_a.ceil().max(0.0) as usize;
    let last = k_b.floor().min(column.box_cnt as f64 - 1.0);
    if last < 0.0 {
        return;
    }
    let half = (symbol_size / 2.0).max(2.0);
    for k in first..=last as usize {
        let offset = k as f64 * step;
        let y = coords.price_to_y(if up { start + offset } else { start - offset });
        if up {
            draw_x_symbol(painter, Pos2::new(x, y), half, color);
        } else {
            draw_o_symbol(painter, Pos2::new(x, y), half, color);
        }
    }
}
