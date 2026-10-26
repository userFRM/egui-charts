//! Range-based chart renderers
//!
//! Includes: High-Low, Range Bars

use crate::scales::PriceMap;
use egui::{Color32, Painter, Pos2, Rect, Shape};

use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{Bar, RangeBar};

use super::japanese::calc_brick_rect;

/// High-Low chart: just the range bars
pub(super) fn render_high_low(
    painter: &Painter,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    for (i, bar) in visible_data.iter().enumerate() {
        if bar.is_whitespace() {
            continue;
        }
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);

        let y_high = price_scale.price_to_y(bar.high);
        let y_low = price_scale.price_to_y(bar.low);

        let color = if bar.close > bar.open {
            bullish_color
        } else {
            bearish_color
        };

        // Draw high-low bar
        let hl_rect = Rect::from_two_pos(
            Pos2::new(x - bar_width / 4.0, y_high),
            Pos2::new(x + bar_width / 4.0, y_low),
        );
        painter.add(Shape::rect_filled(hl_rect, 0.0, color));
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

/// Range bars at their element slots.
pub(super) fn render_range_bars(
    painter: &Painter,
    range_bars: &[RangeBar],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    for (i, range_bar) in range_bars.iter().enumerate() {
        let x = x_of(i);
        let color = if range_bar.close > range_bar.open {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_brick_rect(x, &range_bar.to_bar(), price_scale, bar_width);
        painter.add(Shape::rect_filled(rect, 0.0, color));
    }
}
