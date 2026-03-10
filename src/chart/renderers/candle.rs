use super::context::{BarRenderParams, PriceScale, RenderContext, StyleColors};
use crate::model::Bar;
use egui::{Pos2, Rect, Stroke, Vec2};

/// Renders a single candlestick with customizable body, border, and wick colors
pub fn render_candle(
    context: &RenderContext,
    candle: &Bar,
    price_scale: &PriceScale,
    colors: &StyleColors,
    params: &BarRenderParams,
) {
    let is_bullish = candle.is_bullish();
    let body_color = colors.bar_color(is_bullish);
    let wick_color = colors.wick_color(is_bullish);

    // Convert prices to screen coords using PriceScale helper
    let high_y = price_scale.price_to_y(candle.high, context.rect);
    let low_y = price_scale.price_to_y(candle.low, context.rect);
    let open_y = price_scale.price_to_y(candle.open, context.rect);
    let close_y = price_scale.price_to_y(candle.close, context.rect);

    // Draw wick (high-low line) - uses separate wick color
    context.painter.line_segment(
        [Pos2::new(params.x, high_y), Pos2::new(params.x, low_y)],
        Stroke::new(params.wick_width, wick_color),
    );

    // Draw body (open-close rect)
    let body_top = open_y.min(close_y);
    let body_bottom = open_y.max(close_y);
    let body_height = (body_bottom - body_top).max(1.0); // Ensure min visibility

    let body_rect = Rect::from_min_size(
        Pos2::new(params.x - params.width / 2.0, body_top),
        Vec2::new(params.width, body_height),
    );

    // Draw filled body
    context.painter.rect_filled(body_rect, 0.0, body_color);

    // Draw border if configured
    if colors.has_border() {
        let border_color = colors.border_color(is_bullish);
        context.painter.rect_stroke(
            body_rect,
            0.0,
            Stroke::new(colors.candle_border_width, border_color),
            egui::StrokeKind::Outside,
        );
    }
}
