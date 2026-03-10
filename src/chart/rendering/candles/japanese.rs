//! Japanese chart renderers
//!
//! Includes: Renko, Kagi, Line Break, Point & Figure

use super::helpers::{PriceCoords, draw_brick, draw_o_symbol, draw_x_symbol};
use crate::chart::renderers::PriceScale;
use crate::model::{
    Bar, ColumnDirection, KagiConfig, LineBreakConfig, PointFigureConfig, RenkoConfig,
    to_kagi_lines, to_line_break_lines, to_pnf_columns, to_renko_bricks,
};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

const MAX_ELEMENTS: usize = 2000;
const MAX_BOXES_PER_COLUMN: usize = 500;

pub(super) fn render_renko(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    _start_idx: usize,
    bar_width: f32,
    renko_brick_size: f64,
    price_scale: &PriceScale,
    _chart_rect_min_x: f32,
    _idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let bricks = to_renko_bricks(visible_data, &RenkoConfig::new(renko_brick_size));
    let brick_count = bricks.len().min(MAX_ELEMENTS);
    let spacing = calc_spacing(price_rect.width(), brick_count, bar_width);

    for (i, brick) in bricks.iter().take(MAX_ELEMENTS).enumerate() {
        let x = price_rect.min.x + (i as f32 + 0.5) * spacing;
        if !in_range(x, price_rect) {
            continue;
        }

        let bar = brick.to_bar();
        let color = if bar.close > bar.open {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_brick_rect(x, &bar, &coords, spacing);
        draw_brick(painter, rect, color, 1.0);
    }
}

pub(super) fn render_kagi(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    _start_idx: usize,
    kagi_reversal_amount: f64,
    price_scale: &PriceScale,
    _chart_rect_min_x: f32,
    _idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let lines = to_kagi_lines(visible_data, &KagiConfig::new(kagi_reversal_amount));
    if lines.is_empty() {
        return;
    }

    let line_count = lines.len().min(MAX_ELEMENTS);
    let spacing = (price_rect.width() / line_count as f32).max(2.0);

    let points = collect_kagi_points(&lines, &coords, price_rect, spacing);
    draw_kagi_segments(painter, &points, bullish_color, bearish_color);
}

/// Line Break chart (Three Line Break)
pub(super) fn render_line_break(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    _start_idx: usize,
    bar_width: f32,
    price_scale: &PriceScale,
    _chart_rect_min_x: f32,
    _idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let lines = to_line_break_lines(visible_data, &LineBreakConfig::new(3));
    if lines.is_empty() {
        return;
    }

    let line_count = lines.len().min(MAX_ELEMENTS);
    let spacing = price_rect.width() / line_count as f32;
    let actual_width = (spacing * 0.85).min(bar_width).max(3.0);

    for (i, line) in lines.iter().take(MAX_ELEMENTS).enumerate() {
        let x = price_rect.min.x + (i as f32 + 0.5) * spacing;
        if !in_range(x, price_rect) {
            continue;
        }

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
    visible_data: &[Bar],
    _start_idx: usize,
    bar_width: f32,
    box_size: f64,
    price_scale: &PriceScale,
    _chart_rect_min_x: f32,
    _idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let config = PointFigureConfig {
        box_size,
        reversal_boxes: 3,
        use_atr: false,
        atr_period: 14,
        use_close: false,
    };
    let columns = to_pnf_columns(visible_data, &config);
    if columns.is_empty() {
        return;
    }

    let column_count = columns.len().min(1000);
    let spacing = price_rect.width() / column_count as f32;
    let symbol_size = (spacing * 0.7).min(bar_width * 0.8).max(4.0);

    for (i, column) in columns.iter().take(1000).enumerate() {
        let x = price_rect.min.x + (i as f32 + 0.5) * spacing;
        if !in_range(x, price_rect) {
            continue;
        }

        draw_pnf_column(
            painter,
            x,
            column,
            box_size,
            &coords,
            price_rect,
            symbol_size,
            bullish_color,
            bearish_color,
        );
    }
}

// === Helper Functions ===

fn calc_spacing(width: f32, count: usize, bar_width: f32) -> f32 {
    if count > 1 {
        (width / count as f32).min(bar_width)
    } else {
        bar_width
    }
}

fn in_range(x: f32, rect: Rect) -> bool {
    x >= rect.min.x && x <= rect.max.x
}

fn calc_brick_rect(x: f32, bar: &Bar, coords: &PriceCoords, spacing: f32) -> Rect {
    let y_open = coords.price_to_y(bar.open);
    let y_close = coords.price_to_y(bar.close);
    let top = y_open.min(y_close);
    let bottom = y_open.max(y_close);
    let height = (bottom - top).max(2.0);
    let width = (spacing * 0.85).max(3.0);

    Rect::from_min_max(
        Pos2::new(x - width / 2.0, top),
        Pos2::new(x + width / 2.0, top + height),
    )
}

fn calc_line_break_rect(x: f32, open: f64, close: f64, coords: &PriceCoords, width: f32) -> Rect {
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
    coords: &PriceCoords,
    price_rect: Rect,
    spacing: f32,
) -> Vec<(Pos2, crate::model::kagi::KagiThickness)> {
    use crate::model::kagi::KagiThickness;

    let mut points: Vec<(Pos2, KagiThickness)> = Vec::new();

    for (i, line) in lines.iter().take(MAX_ELEMENTS).enumerate() {
        let x = price_rect.min.x + (i as f32 + 0.5) * spacing;
        if !in_range(x, price_rect) {
            continue;
        }

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
            KagiThickness::Thick => (2.5, bullish_color),
            KagiThickness::Thin => (1.5, bearish_color),
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
    box_size: f64,
    coords: &PriceCoords,
    price_rect: Rect,
    symbol_size: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let mut price = column.start_price;
    let end_price = column.end_price;
    let half = (symbol_size / 2.0).max(2.0);
    let mut box_count = 0;

    match column.direction {
        ColumnDirection::Up => {
            while price <= end_price && box_count < MAX_BOXES_PER_COLUMN {
                let y = coords.price_to_y(price);
                if y >= price_rect.min.y && y <= price_rect.max.y {
                    draw_x_symbol(painter, Pos2::new(x, y), half, bullish_color);
                }
                price += box_size;
                box_count += 1;
            }
        }
        ColumnDirection::Down => {
            while price >= end_price && box_count < MAX_BOXES_PER_COLUMN {
                let y = coords.price_to_y(price);
                if y >= price_rect.min.y && y <= price_rect.max.y {
                    draw_o_symbol(painter, Pos2::new(x, y), half, bearish_color);
                }
                price -= box_size;
                box_count += 1;
            }
        }
    }
}
