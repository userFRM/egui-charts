//! Volume rendering helpers

use super::super::context::{BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;
use crate::theme::ThemeExt;
use egui::{Pos2, Rect, Vec2};

/// Render volume bars for chart types that show volume
pub(crate) fn render_volume_bars(
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    max_volume: f64,
    colors: &StyleColors,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
) {
    let min_pitch = (start_idx.saturating_sub(1)..start_idx + visible_data.len())
        .map(|i| (idx_to_coord(i + 1, chart_rect_min_x) - idx_to_coord(i, chart_rect_min_x)).abs())
        .filter(|pitch| pitch.is_finite() && *pitch > 0.0)
        .fold(f32::INFINITY, f32::min);
    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let pitch = (idx_to_coord(start_idx + i + 1, chart_rect_min_x) - x).abs();
        let volume_params = BarRenderParams::new(x, pitch.min(min_pitch) * 0.8, 0.0);
        render_volume_bar(volume_ctx, bar, max_volume, colors, &volume_params);
    }
}

/// Renders a volume bar with the active theme's volume colour and opacity.
fn render_volume_bar(
    context: &RenderContext,
    candle: &Bar,
    max_volume: f64,
    _colors: &StyleColors,
    params: &BarRenderParams,
) {
    if candle.is_whitespace() {
        return;
    }
    let theme = context.painter.ctx().active_theme();
    let color = if candle.close >= candle.open {
        theme.chart.volume_up()
    } else {
        theme.chart.volume_down()
    };

    let bar_height = (candle.volume / max_volume) as f32 * context.rect.height();
    let bar_bottom = context.rect.max.y;

    context.painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(params.x - params.width / 2.0, bar_bottom - bar_height),
            Vec2::new(params.width, bar_height),
        ),
        0.0,
        color,
    );
}
