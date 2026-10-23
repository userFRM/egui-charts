use super::context::{BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;
use crate::theme::ThemeExt;
use egui::{Pos2, Rect, Vec2};

/// Renders a volume bar with the active theme's volume colour and opacity.
pub fn render_volume_bar(
    context: &RenderContext,
    candle: &Bar,
    max_volume: f64,
    _colors: &StyleColors,
    params: &BarRenderParams,
) {
    let theme = context.painter.ctx().active_theme();
    let color = if candle.is_bullish() {
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
