//! Range-based chart renderers
//!
//! Includes: High-Low, Range Bars

use egui::{Color32, Painter, Pos2, Rect, Shape};

use super::volume::render_volume_bars;
use crate::chart::renderers::{PriceScale, RenderContext, StyleColors};
use crate::model::{Bar, RangeBarConfig, to_range_bars_from_ohlc};

/// High-Low chart: just the range bars
pub(super) fn render_high_low(
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
) {
    let (min_price, max_price) = (price_scale.min_price, price_scale.max_price);
    let price_range = (max_price - min_price).max(1e-12);

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);

        let y_high =
            price_rect.max.y - ((bar.high - min_price) / price_range) as f32 * price_rect.height();
        let y_low =
            price_rect.max.y - ((bar.low - min_price) / price_range) as f32 * price_rect.height();

        let color = if bar.close > bar.open {
            bullish_color
        } else {
            bearish_color
        };

        // Draw high-low bar
        let hl_rect = Rect::from_min_max(
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

/// Range bars: price-based bars
pub(super) fn render_range_bars(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    range_size: f64,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let (min_price, max_price) = (price_scale.min_price, price_scale.max_price);
    let price_range = (max_price - min_price).max(1e-12);

    let config = RangeBarConfig {
        range_size,
        use_atr: false,
        atr_period: 14,
        atr_multiplier: 1.0,
    };
    let range_bars = to_range_bars_from_ohlc(visible_data, &config);

    for (i, range_bar) in range_bars.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);

        let y_open = price_rect.max.y
            - ((range_bar.open - min_price) / price_range) as f32 * price_rect.height();
        let y_close = price_rect.max.y
            - ((range_bar.close - min_price) / price_range) as f32 * price_rect.height();

        let color = if range_bar.close > range_bar.open {
            bullish_color
        } else {
            bearish_color
        };

        let body_rect = Rect::from_min_max(
            Pos2::new(x - bar_width / 2.0, y_open.min(y_close)),
            Pos2::new(x + bar_width / 2.0, y_open.max(y_close)),
        );
        painter.add(Shape::rect_filled(body_rect, 0.0, color));
    }
}
