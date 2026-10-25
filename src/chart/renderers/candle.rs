use super::context::{BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;
use crate::scales::PriceMap;
use egui::{Pos2, Rect, Stroke, Vec2};

/// Renders a single candlestick with customizable body, border, and wick colors; the bar's own
/// [`colors`](Bar::colors) replace them, and a whitespace bar draws nothing
pub fn render_candle(
    context: &RenderContext,
    candle: &Bar,
    price_scale: &PriceMap,
    colors: &StyleColors,
    params: &BarRenderParams,
) {
    if candle.is_whitespace() {
        return;
    }
    let is_bullish = candle.is_bullish();
    let own = candle.colors.as_deref();
    let body_color = own
        .and_then(|own| own.color)
        .unwrap_or_else(|| colors.bar_color(is_bullish));
    let wick_color = own
        .and_then(|own| own.wick)
        .unwrap_or_else(|| colors.wick_color(is_bullish));

    // Convert prices to screen coords
    let high_y = price_scale.price_to_y(candle.high);
    let low_y = price_scale.price_to_y(candle.low);
    let open_y = price_scale.price_to_y(candle.open);
    let close_y = price_scale.price_to_y(candle.close);

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
        let border_color = own
            .and_then(|own| own.border)
            .unwrap_or_else(|| colors.border_color(is_bullish));
        context.painter.rect_stroke(
            body_rect,
            0.0,
            Stroke::new(colors.candle_border_width, border_color),
            egui::StrokeKind::Outside,
        );
    }
}
