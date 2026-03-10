//! Chart Type Rendering
//!
//! Renders all chart visualization types (the `candles` directory name is
//! historical; this module handles every `ChartType` variant):
//!
//! - Standard: Candlestick, Bars, Hollow Candles, Volume Candles
//! - Line-based: Line, Line with Markers, Step Line
//! - Area-based: Area, HLC Area, Baseline
//! - Japanese: Renko, Kagi, Line Break, Heikin-Ashi
//! - Range: High-Low, Range Bars, Point & Figure
//! - Advanced: Volume Footprint, TPO, Session Volume

mod advanced;
mod area;
mod candlestick;
mod helpers;
mod japanese;
mod line;
mod params;
mod range;
pub mod tpo;
mod volume;

pub use params::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, JapaneseChartSettings,
    TradingColors, VolumeSettings,
};

use crate::model::ChartType;

/// Renders all chart types
///
/// # Parameters
/// - `chart_type`: Which chart type to render
/// - `ctx`: Core rendering context (price_ctx, volume_ctx, price_scale, colors, data)
/// - `params`: Chart rendering parameters (dimensions, colors, settings)
/// - `idx_to_coord`: Function to map bar index to x coordinate
pub fn render_chart_type(
    chart_type: ChartType,
    ctx: &CandleDataContext,
    params: &ChartTypeParams,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    let price_rect = ctx.price_ctx.rect;
    let painter = ctx.price_ctx.painter;

    match chart_type {
        // === Standard OHLC Types ===
        ChartType::Candles => {
            candlestick::render_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
            );
        }
        ChartType::Heikin => {
            candlestick::render_heikin_ashi(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::Bars => {
            candlestick::render_ohlc_bars(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
            );
        }
        ChartType::HollowCandles => {
            candlestick::render_hollow_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::VolumeCandles => {
            candlestick::render_volume_candles(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.wick_width(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }

        // === Line-Based Types ===
        ChartType::Line => {
            line::render_line(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.price_source(),
            );
        }
        ChartType::LineWithMarkers => {
            line::render_line_with_markers(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.price_source(),
            );
        }
        ChartType::StepLine => {
            line::render_step_line(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.price_source(),
            );
        }

        // === Area-Based Types ===
        ChartType::Area => {
            area::render_area(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.price_source(),
            );
        }
        ChartType::HlcArea => {
            area::render_hlc_area(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
                params.price_source(),
            );
        }
        ChartType::Baseline => {
            area::render_baseline(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
                params.price_source(),
            );
        }

        // === Range Types ===
        ChartType::HighLow => {
            range::render_high_low(
                painter,
                price_rect,
                ctx.volume_ctx,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                ctx.colors,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::Range => {
            range::render_range_bars(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.renko_brick_size(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }

        // === Japanese Chart Types ===
        ChartType::Renko => {
            japanese::render_renko(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.renko_brick_size(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::Kagi => {
            japanese::render_kagi(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.kagi_reversal_amount(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::LineBreak => {
            japanese::render_line_break(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::PointAndFigure => {
            japanese::render_point_and_figure(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.renko_brick_size(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }

        // === Advanced Types (placeholders) ===
        ChartType::VolumeFootprint => {
            advanced::render_volume_footprint_placeholder(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
        ChartType::TimePriceOpportunity => {
            advanced::render_tpo_placeholder(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
            );
        }
        ChartType::SessionVolume => {
            advanced::render_session_volume_placeholder(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }
    }
}
