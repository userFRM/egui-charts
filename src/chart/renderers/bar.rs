use super::context::{BarRenderParams, RenderContext, StyleColors};
use crate::model::{Bar, BarsStyle};
use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Stroke};

/// Renders a single OHLC bar
/// Vertical line with left tick for open, right tick for close. Thin bars use the wick width;
/// other bars a stroke of half the bar width, at least 1.5 points.
pub fn render_ohlc_bar(
    context: &RenderContext,
    bar: &Bar,
    price_scale: &PriceMap,
    colors: &StyleColors,
    params: &BarRenderParams,
    style: &BarsStyle,
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

    let stroke = Stroke::new(
        if style.thin_bars {
            params.wick_width
        } else {
            (params.width / 2.0).max(DESIGN_TOKENS.stroke.medium)
        },
        color,
    );

    // Draw vertical line from high to low
    context.painter.line_segment(
        [Pos2::new(params.x, high_y), Pos2::new(params.x, low_y)],
        stroke,
    );

    // Draw horizontal tick for open (left side)
    let tick_width = params.width / 2.0;
    if style.open_visible {
        context.painter.line_segment(
            [
                Pos2::new(params.x - tick_width, open_y),
                Pos2::new(params.x, open_y),
            ],
            stroke,
        );
    }

    // Draw horizontal tick for close (right side)
    context.painter.line_segment(
        [
            Pos2::new(params.x, close_y),
            Pos2::new(params.x + tick_width, close_y),
        ],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceTransform};
    use egui::{Color32, Context, LayerId, Painter, Rect, Shape, vec2};

    #[test]
    fn bar_style_sets_the_open_tick_and_the_stroke() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            0.0,
            100.0,
            false,
        );
        let bar = Bar::new(chrono::DateTime::UNIX_EPOCH, 40.0, 80.0, 20.0, 60.0, 1.0);
        let colors = StyleColors {
            bullish: Color32::GREEN,
            bearish: Color32::RED,
            text: Color32::WHITE,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: 0.0,
        };
        // A 12-point bar with a 1-point wick width: thin bars keep the wick width at any
        // zoom, other bars use half the bar width.
        for (open_visible, thin_bars, segments, width) in [
            (true, true, 3, 1.0),
            (false, true, 2, 1.0),
            (true, false, 3, 6.0),
        ] {
            let ctx = Context::default();
            let output = ctx.run_ui(Default::default(), |_| {
                let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                render_ohlc_bar(
                    &RenderContext::new(&painter, rect),
                    &bar,
                    &map,
                    &colors,
                    &BarRenderParams::new(50.0, 12.0, 1.0),
                    &BarsStyle {
                        open_visible,
                        thin_bars,
                    },
                );
            });
            let strokes: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match shape.shape {
                    Shape::LineSegment { stroke, .. } => Some(stroke),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert_eq!(strokes.len(), segments);
            assert!(
                strokes
                    .iter()
                    .all(|stroke| *stroke == Stroke::new(width, Color32::GREEN))
            );
        }
    }
}
