use super::context::{BarRenderParams, PriceScale, RenderContext, StyleColors};
use crate::model::Bar;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Stroke};

/// Renders a single OHLC bar
/// Vertical line with left tick for open, right tick for close
pub fn render_ohlc_bar(
    context: &RenderContext,
    bar: &Bar,
    price_scale: &PriceScale,
    colors: &StyleColors,
    params: &BarRenderParams,
) {
    let color = colors.bar_color(bar.is_bullish());

    // Convert prices to screen coords using PriceScale helper
    let high_y = price_scale.price_to_y(bar.high, context.rect);
    let low_y = price_scale.price_to_y(bar.low, context.rect);
    let open_y = price_scale.price_to_y(bar.open, context.rect);
    let close_y = price_scale.price_to_y(bar.close, context.rect);

    // Draw vertical line from high to low
    context.painter.line_segment(
        [Pos2::new(params.x, high_y), Pos2::new(params.x, low_y)],
        Stroke::new(DESIGN_TOKENS.stroke.medium, color),
    );

    // Draw horizontal tick for open (left side)
    let tick_width = params.width / 2.0;
    context.painter.line_segment(
        [
            Pos2::new(params.x - tick_width, open_y),
            Pos2::new(params.x, open_y),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.medium, color),
    );

    // Draw horizontal tick for close (right side)
    context.painter.line_segment(
        [
            Pos2::new(params.x, close_y),
            Pos2::new(params.x + tick_width, close_y),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.medium, color),
    );
}
