//! Candlestick-family chart renderers
//!
//! Includes: Candlestick, OHLC Bars, Hollow Candles, Heikin-Ashi, Volume Candles, Columns

use super::heikin_ashi;
use super::helpers::{
    OhlcYCoords, bar_color, draw_body_filled, draw_body_hollow, draw_wicks, prev_close,
};
use crate::chart::renderers::{self, BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;
use crate::scales::PriceMap;
use egui::{Color32, Pos2, Rect};

pub(super) fn render_candles(
    price_ctx: &RenderContext,
    volume_ctx: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    wick_width: f32,
    show_volume: bool,
    max_volume: f64,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let params = BarRenderParams::new(x, bar_width, wick_width);
        renderers::render_candle(price_ctx, bar, price_scale, colors, &params);

        if show_volume {
            let volume_params = BarRenderParams::new(x, bar_width, 0.0);
            renderers::render_volume_bar(volume_ctx, bar, max_volume, colors, &volume_params);
        }
    }
}

pub(super) fn render_ohlc_bars(
    price_ctx: &RenderContext,
    volume_ctx: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    wick_width: f32,
    show_volume: bool,
    max_volume: f64,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let params = BarRenderParams::new(x, bar_width, wick_width);
        renderers::render_ohlc_bar(price_ctx, bar, price_scale, colors, &params);

        if show_volume {
            let volume_params = BarRenderParams::new(x, bar_width, 0.0);
            renderers::render_volume_bar(volume_ctx, bar, max_volume, colors, &volume_params);
        }
    }
}

/// Hollow candles: hollow when bullish, filled when bearish
pub(super) fn render_hollow_candles(
    price_ctx: &RenderContext,
    volume_ctx: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    wick_width: f32,
    show_volume: bool,
    max_volume: f64,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let painter = price_ctx.painter;
    let coords = *price_scale;

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let color = bar_color(bar, bullish_color, bearish_color);
        let y = OhlcYCoords::from_bar(bar, &coords);

        draw_wicks(painter, x, &y, wick_width, color);

        if bar.close > bar.open {
            draw_body_hollow(painter, x, bar_width, &y, color);
        } else {
            draw_body_filled(painter, x, bar_width, &y, color);
        }

        if show_volume {
            let volume_params = BarRenderParams::new(x, bar_width, 0.0);
            renderers::render_volume_bar(volume_ctx, bar, max_volume, colors, &volume_params);
        }
    }
}

/// Columns: one solid column per bar from the pane bottom to the close, coloured by the close
/// versus the previous close.
pub(super) fn render_columns(
    price_ctx: &RenderContext,
    volume_ctx: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    visible_data: &[Bar],
    full_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    show_volume: bool,
    max_volume: f64,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let bottom = price_ctx.rect.max.y;
    for (i, bar) in visible_data.iter().enumerate() {
        let g = start_idx + i;
        let x = idx_to_coord(g, chart_rect_min_x);
        let color = if bar.close >= prev_close(full_data, g) {
            bullish_color
        } else {
            bearish_color
        };
        let top = price_scale.price_to_y(bar.close).min(bottom - 1.0);
        let rect = Rect::from_min_max(
            Pos2::new(x - bar_width / 2.0, top),
            Pos2::new(x + bar_width / 2.0, bottom),
        );
        price_ctx.painter.rect_filled(rect, 0.0, color);
    }

    if show_volume {
        render_volume_for_bars(
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

/// Heikin-Ashi candles: smoothed candlesticks.
///
/// Heikin-Ashi is a cumulative recurrence, so the series is computed over the
/// complete dataset (`full_data`) and cached; the visible window is sliced out
/// of the cached series. This keeps every bar's Heikin-Ashi value invariant as
/// the user pans and zooms, instead of reseeding from the first visible bar.
pub(super) fn render_heikin_ashi(
    price_ctx: &RenderContext,
    volume_ctx: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    visible_data: &[Bar],
    full_data: &[Bar],
    revision: u64,
    start_idx: usize,
    bar_width: f32,
    wick_width: f32,
    show_volume: bool,
    max_volume: f64,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    if visible_data.is_empty() {
        return;
    }

    // Slice the visible window out of the cached full-series transform.
    let ha_window = heikin_ashi::window(full_data, revision, start_idx, visible_data.len());

    let painter = price_ctx.painter;
    let coords = *price_scale;

    for (i, bar) in ha_window.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let color = if bar.close >= bar.open {
            bullish_color
        } else {
            bearish_color
        };
        let y = OhlcYCoords::from_bar(bar, &coords);

        draw_wicks(painter, x, &y, wick_width, color);
        draw_body_filled(painter, x, bar_width, &y, color);
    }

    if show_volume {
        render_volume_for_bars(
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

/// Volume candles: width proportional to volume
pub(super) fn render_volume_candles(
    price_ctx: &RenderContext,
    volume_ctx: &RenderContext,
    price_scale: &PriceMap,
    colors: &StyleColors,
    visible_data: &[Bar],
    start_idx: usize,
    base_bar_width: f32,
    wick_width: f32,
    max_volume: f64,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let painter = price_ctx.painter;
    let coords = *price_scale;

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let color = bar_color(bar, bullish_color, bearish_color);
        let y = OhlcYCoords::from_bar(bar, &coords);

        // Scale bar width by volume (30% to 100% of base width)
        let volume_ratio = if max_volume > 0.0 {
            (bar.volume / max_volume).clamp(0.3, 1.0) as f32
        } else {
            1.0
        };
        let bar_width = base_bar_width * volume_ratio;

        draw_wicks(painter, x, &y, wick_width, color);
        draw_body_filled(painter, x, bar_width, &y, color);
    }

    // Volume candles don't show separate volume bars
    let _ = (volume_ctx, colors);
}

// === Helper Functions ===

fn render_volume_for_bars(
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    max_volume: f64,
    colors: &StyleColors,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let volume_params = BarRenderParams::new(x, bar_width, 0.0);
        renderers::render_volume_bar(volume_ctx, bar, max_volume, colors, &volume_params);
    }
}
