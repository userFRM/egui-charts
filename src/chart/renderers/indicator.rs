use super::context::{ChartMapping, RenderContext};
use crate::model::Bar;
use crate::scales::PriceMap;
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
        price_scale: &PriceMap,
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

                // values[i] belongs to bar i of the full series and `bars` are the
                // visible ones from coords.start_idx, so only that slice is walked.
                let visible_end = (coords.start_idx + bars.len()).min(values.len());
                let visible = values
                    .get(coords.start_idx..visible_end)
                    .unwrap_or_default();

                for (i, value) in (coords.start_idx..).zip(visible) {
                    let x = coords.idx_to_x(i);

                    // A NaN line value has no point and breaks the line like `None`.
                    let price = match value {
                        IndicatorValue::Single(p) => Some(*p),
                        IndicatorValue::Multiple(prices) => prices.get(line_idx).copied(),
                        IndicatorValue::None => None,
                    }
                    .filter(|p| p.is_finite());

                    if let Some(price) = price {
                        // Convert price to screen Y coord
                        let y = price_scale.price_to_y(price);

                        // Only add if within chart bounds
                        if y >= context.rect.min.y && y <= context.rect.max.y {
                            points.push(Pos2::new(x, y));
                        }
                    } else {
                        // Break the line if we have a None value
                        if !points.is_empty() {
                            Self::draw_line(context.painter, &points, color);
                        }
                        points.clear();
                    }
                }

                // Draw final segment
                if !points.is_empty() {
                    Self::draw_line(context.painter, &points, color);
                }
            }
        }
    }

    fn draw_line(painter: &egui::Painter, points: &[Pos2], color: Color32) {
        // A run of one point, such as a one-bar trend of a split line, is a dot.
        if let [p] = points {
            painter.circle_filled(*p, DESIGN_TOKENS.stroke.thick, color);
            return;
        }

        for i in 0..points.len() - 1 {
            painter.line_segment(
                [points[i], points[i + 1]],
                Stroke::new(DESIGN_TOKENS.stroke.hairline, color),
            );
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::studies::{BollingerBands, Indicator, SMA};
    use crate::widget::test_frames::{chart, shapes};
    use egui::{Color32, Context, Shape};

    #[test]
    fn overlay_lines_use_one_point_strokes_and_study_colours() {
        let blue = Color32::from_rgb(41, 98, 255);
        let orange = Color32::from_rgb(255, 167, 38);
        let custom = Color32::from_rgb(10, 200, 90);
        let mut recoloured = BollingerBands::default();
        recoloured.set_colors(vec![custom]);
        let mut sma = SMA::default();
        sma.set_colors(vec![custom]);
        let cases: [(Box<dyn Indicator>, Vec<Color32>); 3] = [
            (
                Box::new(BollingerBands::default()),
                vec![blue, orange, blue],
            ),
            (Box::new(sma), vec![custom]),
            (Box::new(recoloured), vec![custom; 3]),
        ];
        for (study, colours) in cases {
            let name = study.name().to_owned();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.add_indicator(study);
            let ctx = Context::default();
            let strokes: Vec<_> = shapes(&ctx, &mut chart, 0.0, Vec::new())
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { stroke, .. } if colours.contains(&stroke.color) => {
                        Some(stroke)
                    }
                    _ => None,
                })
                .collect();
            assert!(!strokes.is_empty(), "{name}: no overlay lines");
            assert!(
                strokes.iter().all(|stroke| stroke.width == 1.0),
                "{name}: {strokes:?}"
            );
            for colour in colours {
                assert!(
                    strokes.iter().any(|stroke| stroke.color == colour),
                    "{name}: missing {colour:?}"
                );
            }
        }
    }
}
