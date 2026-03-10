use super::context::{ChartMapping, PriceScale, RenderContext};
use crate::model::Bar;
/// Indicator Renderer
/// Renders indicator lines overlay on the chart
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Stroke};

pub struct IndicatorRenderer;

impl IndicatorRenderer {
    pub fn render(
        context: &RenderContext,
        indicators: &[Box<dyn Indicator>],
        bars: &[Bar],
        price_scale: &PriceScale,
        coords: &ChartMapping,
    ) {
        for indicator in indicators {
            if !indicator.is_visible() {
                continue;
            }

            // Only render overlay indicators
            if !indicator.is_overlay() {
                continue;
            }

            let values = indicator.values();
            let colors = indicator.colors();

            // Handle multi-line indicators
            let line_cnt = indicator.line_cnt();

            for line_idx in 0..line_cnt {
                let color = colors.get(line_idx).copied().unwrap_or(Color32::WHITE);

                let mut points = Vec::new();

                // Calculate visible range for indicator values
                // values[i] corresponds to bars[i] in the FULL bars array
                // coords.start_idx is the index of the first visible bar
                let visible_end = coords.start_idx + bars.len();

                for (i, value) in values.iter().enumerate() {
                    // Skip values before visible range
                    if i < coords.start_idx {
                        continue;
                    }
                    // Stop after visible range
                    if i >= visible_end {
                        break;
                    }

                    // i is the global bar index (values[i] corresponds to bar i)
                    let x = coords.idx_to_x(i);

                    let price = match value {
                        IndicatorValue::Single(p) => Some(*p),
                        IndicatorValue::Multiple(prices) => prices.get(line_idx).copied(),
                        IndicatorValue::None => None,
                    };

                    if let Some(price) = price {
                        // Convert price to screen Y coord using PriceScale helper
                        let y = price_scale.price_to_y(price, context.rect);

                        // Only add if within chart bounds
                        if y >= context.rect.min.y && y <= context.rect.max.y {
                            points.push(Pos2::new(x, y));
                        }
                    } else {
                        // Break the line if we have a None value
                        if points.len() > 1 {
                            Self::draw_line(context.painter, &points, color);
                        }
                        points.clear();
                    }
                }

                // Draw final segment
                if points.len() > 1 {
                    Self::draw_line(context.painter, &points, color);
                }
            }
        }
    }

    fn draw_line(painter: &egui::Painter, points: &[Pos2], color: Color32) {
        if points.len() < 2 {
            return;
        }

        for i in 0..points.len() - 1 {
            painter.line_segment(
                [points[i], points[i + 1]],
                Stroke::new(DESIGN_TOKENS.stroke.thick, color),
            );
        }
    }
}
