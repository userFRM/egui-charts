//! Chart Type Rendering
//!
//! Renders all chart visualization types (the `candles` directory name is
//! historical; this module handles every `ChartType` variant):
//!
//! - Standard: Candlestick, Bars, Hollow Candles, Volume Candles
//! - Line-based: Line, Line with Markers, Step Line
//! - Area-based: Area, HLC Area, Baseline, Columns
//! - Japanese: Renko, Kagi, Line Break, Heikin-Ashi
//! - Range: High-Low, Range Bars, Point & Figure
//! - Advanced: TPO

mod area;
mod candlestick;
mod heikin_ashi;
mod helpers;
mod japanese;
mod line;
mod params;
mod range;
pub mod tpo;
mod volume;

pub use params::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, TradingColors, VolumeSettings,
};

use crate::model::ChartType;
use crate::styles::typography;

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
    params: &ChartTypeParams<'_>,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    let price_rect = ctx.price_ctx.rect;
    let painter = ctx.price_ctx.painter;

    if let Some(message) = params.type_params.invalid(chart_type) {
        painter.text(
            price_rect.center(),
            egui::Align2::CENTER_CENTER,
            message,
            egui::FontId::proportional(typography::LG),
            ctx.colors.text,
        );
        return;
    }

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
                ctx.full_data,
                ctx.revision,
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
        ChartType::Columns => {
            candlestick::render_columns(
                ctx.price_ctx,
                ctx.volume_ctx,
                ctx.price_scale,
                ctx.colors,
                ctx.visible_data,
                ctx.full_data,
                ctx.start_idx,
                params.bar_width(),
                params.show_volume(),
                params.max_volume(),
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
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
                ctx.visible_data,
                ctx.start_idx,
                params.bar_width(),
                params.type_params.range_size,
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
                params.type_params.renko_brick_size,
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
                params.type_params.kagi_reversal,
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
                params.type_params.line_break_count,
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
                params.type_params.pnf_box_size,
                params.type_params.pnf_reversal,
                ctx.price_scale,
                params.chart_rect_min_x(),
                &idx_to_coord,
                params.bullish_color(),
                params.bearish_color(),
            );
        }

        // === Advanced Types ===
        ChartType::TimePriceOpportunity => {
            // Auto-derive the price-bin size from the visible range: a zero
            // tick_size signals `render_live_tpo` to pick a readable row count.
            let tpo_config = crate::model::TPOConfig {
                tick_size: 0.0,
                ..Default::default()
            };
            tpo::render_live_tpo(
                painter,
                price_rect,
                ctx.visible_data,
                ctx.start_idx,
                ctx.price_scale,
                &tpo_config,
                &idx_to_coord,
                params.chart_rect_min_x(),
                params.bullish_color(),
                params.bearish_color(),
            );
        }
    }
}
