use super::context::{BarRenderParams, RenderContext, StyleColors};
use crate::model::Bar;
use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Stroke};

/// Renders a single OHLC bar
/// Vertical line with left tick for open, right tick for close, in the bar's own colour if it has
/// one; a whitespace bar draws nothing
pub fn render_ohlc_bar(
    context: &RenderContext,
    bar: &Bar,
    price_scale: &PriceMap,
    colors: &StyleColors,
    params: &BarRenderParams,
) {
    if bar.is_whitespace() {
        return;
    }
    let color = bar
        .colors
        .as_ref()
        .and_then(|own| own.color)
        .unwrap_or_else(|| colors.bar_color(bar.is_bullish()));

    // Convert prices to screen coords
    let high_y = price_scale.price_to_y(bar.high);
    let low_y = price_scale.price_to_y(bar.low);
    let open_y = price_scale.price_to_y(bar.open);
    let close_y = price_scale.price_to_y(bar.close);

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
