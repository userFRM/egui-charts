use super::context::{BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;
use egui::{Color32, Pos2, Rect, Vec2};

/// Renders a volume bar (semi-transparent with ~26% opacity)
pub fn render_volume_bar(
    context: &RenderContext,
    candle: &Bar,
    max_volume: f64,
    colors: &StyleColors,
    params: &BarRenderParams,
) {
    let base_color = colors.bar_color(candle.is_bullish());
    // Use ~26% opacity for volume histogram bars
    let color = Color32::from_rgba_unmultiplied(
        base_color.r(),
        base_color.g(),
        base_color.b(),
        66, // ~26% of 255
    );

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
