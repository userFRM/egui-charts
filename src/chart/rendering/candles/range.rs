//! Range-based chart renderers
//!
//! Includes: High-Low, Range Bars

use crate::scales::PriceMap;
use egui::{Color32, Painter, Pos2, Rect, Shape};

use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{Bar, RangeBar};

use super::japanese::{MAX_ELEMENTS, calc_brick_rect, calc_spacing};

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
            bar_width,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Range bars: the newest ones, spread evenly over the pane like the other price-based charts
pub(super) fn render_range_bars(
    painter: &Painter,
    price_rect: Rect,
    range_bars: &[RangeBar],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let shown = &range_bars[range_bars.len().saturating_sub(MAX_ELEMENTS)..];
    let spacing = calc_spacing(price_rect.width(), shown.len(), bar_width);

    for (i, range_bar) in shown.iter().enumerate() {
        let x = price_rect.min.x + (i as f32 + 0.5) * spacing;
        let color = if range_bar.close > range_bar.open {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_brick_rect(x, &range_bar.to_bar(), price_scale, spacing);
        painter.add(Shape::rect_filled(rect, 0.0, color));
    }
}
