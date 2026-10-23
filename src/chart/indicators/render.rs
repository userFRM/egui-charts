use crate::chart::renderers::{ChartMapping, RenderContext};
use crate::model::Bar;
use crate::scales::PriceMap;
/// Indicator Renderer
/// Renders indicator lines overlay on the chart
use crate::studies::{CrossDirection, Indicator, IndicatorLineStyle, IndicatorValue};
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

                let style = indicator.line_style(line_idx);
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
                            points.push((Pos2::new(x, y), price));
                        }
                    } else {
                        // Break the line if we have a None value
                        if !points.is_empty() {
                            Self::draw_line(context, &points, color, style);
                        }
                        points.clear();
                    }
                }

                if indicator.extend_right() && visible_end == values.len() && points.len() >= 2 {
                    let ((a, pa), (b, pb)) = (points[points.len() - 2], points[points.len() - 1]);
                    if b.x > a.x && b.x < context.rect.right() {
                        let painter = context.painter.with_clip_rect(context.rect);
                        let mut previous = b;
                        // Project prices before mapping them, so a logarithmic axis preserves the fit.
                        for step in 1..=((context.rect.right() - b.x).ceil() as usize) {
                            let x = (b.x + step as f32).min(context.rect.right());
                            let price = pb + (pb - pa) * ((x - b.x) / (b.x - a.x)) as f64;
                            let point = Pos2::new(x, price_scale.price_to_y(price));
                            if point.y.is_finite() && previous.y.is_finite() {
                                painter.line_segment(
                                    [previous, point],
                                    Stroke::new(style.width, color),
                                );
                            }
                            previous = point;
                        }
                    }
                }
                if let Some((line, label)) = indicator.plot_label()
                    && line == line_idx
                    && let Some(last) = points.last()
                {
                    let at = Pos2::new(last.0.x.min(context.rect.right() - 90.0), last.0.y);
                    context.painter.text(
                        at,
                        egui::Align2::LEFT_BOTTOM,
                        label,
                        egui::FontId::proportional(12.0),
                        color,
                    );
                }

                // Draw final segment
                if !points.is_empty() {
                    Self::draw_line(context, &points, color, style);
                }
            }
            let painter = context.painter.with_clip_rect(context.rect);
            for marker in indicator.markers() {
                if marker.bar < coords.start_idx
                    || marker.bar >= coords.start_idx + bars.len()
                    || !marker.value.is_finite()
                {
                    continue;
                }
                let at = Pos2::new(
                    coords.idx_to_x(marker.bar),
                    price_scale.price_to_y(marker.value),
                );
                let (sign, color) = match marker.direction {
                    CrossDirection::Up => (1.0, DESIGN_TOKENS.semantic.extended.success),
                    CrossDirection::Down => (-1.0, DESIGN_TOKENS.semantic.extended.error),
                };
                let triangle = vec![
                    at + egui::vec2(0.0, sign * 4.0),
                    at + egui::vec2(-4.0, sign * 11.0),
                    at + egui::vec2(4.0, sign * 11.0),
                ];
                painter.add(egui::Shape::convex_polygon(triangle, color, Stroke::NONE));
            }
        }
    }

    fn draw_line(
        context: &RenderContext,
        points: &[(Pos2, f64)],
        color: Color32,
        style: IndicatorLineStyle,
    ) {
        let painter = context.painter.with_clip_rect(context.rect);
        if let [(point, _)] = points {
            painter.circle_filled(*point, DESIGN_TOKENS.stroke.thick, color);
            return;
        }
        let mut segment_color = color;
        for segment in points.windows(2) {
            let [(a, pa), (b, pb)] = segment else {
                unreachable!()
            };
            if pb > pa {
                segment_color = style.rising.unwrap_or(color);
            } else if pb < pa {
                segment_color = style.falling.unwrap_or(color);
            }
            if let Some(fill) = style.fill {
                let top = Color32::from_rgba_unmultiplied(
                    fill.color.r(),
                    fill.color.g(),
                    fill.color.b(),
                    fill.top_alpha,
                );
                let bottom = Color32::from_rgba_unmultiplied(
                    fill.color.r(),
                    fill.color.g(),
                    fill.color.b(),
                    fill.bottom_alpha,
                );
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(*a, top);
                mesh.colored_vertex(*b, top);
                mesh.colored_vertex(Pos2::new(a.x, context.rect.bottom()), bottom);
                mesh.colored_vertex(Pos2::new(b.x, context.rect.bottom()), bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(1, 3, 2);
                painter.add(egui::Shape::mesh(mesh));
            }
            painter.line_segment([*a, *b], Stroke::new(style.width, segment_color));
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceScaleMode, PriceTransform};
    use crate::studies::LinearRegressionChannel;
    use crate::studies::{AverageLine, AverageType, Indicator, MACombination, MARibbon};
    use crate::tokens::DESIGN_TOKENS;
    use crate::widget::test_frames::{chart, shapes};
    use chrono::{TimeZone, Utc};
    use egui::{Color32, Context, Shape};
    use egui::{RawInput, Rect, Vec2};

    /// Overlay rendering consumes ribbon widths/fades and both crossing directions.
    #[test]
    fn ribbon_styles_and_crossing_markers_are_painted() {
        let cross = MACombination::new(vec![
            AverageLine::new(AverageType::Simple, 1, Color32::BLUE),
            AverageLine::new(AverageType::Simple, 2, Color32::RED),
        ])
        .unwrap();
        for (study, filled, marked) in [
            (Box::new(MARibbon::new()) as Box<dyn Indicator>, true, false),
            (Box::new(MARibbon::muted()), false, false),
            (Box::new(cross), false, true),
        ] {
            let mut chart = chart(&[0.0], &[1.0, 2.0]);
            chart.add_indicator(study);
            let shapes = shapes(&Context::default(), &mut chart, 0.0, Vec::new());
            let fading = shapes.iter().any(|shape| matches!(shape,Shape::Mesh(mesh)
                if mesh.vertices.len()==4 && mesh.vertices[0].color.a()==38 && mesh.vertices[2].color.a()==10));
            assert_eq!(fading, filled, "bottom fade");
            let thick = shapes
                .iter()
                .any(|shape| matches!(shape,Shape::LineSegment {stroke,..} if stroke.width==3.0));
            assert_eq!(thick, filled, "slowest ribbon width");
            for color in [
                DESIGN_TOKENS.semantic.extended.success,
                DESIGN_TOKENS.semantic.extended.error,
            ] {
                let triangle = shapes.iter().any(|shape| matches!(shape,Shape::Path(path) if path.points.len()==3 && path.fill==color));
                assert_eq!(triangle, marked, "crossing direction {color:?}");
            }
        }
    }
    /// A fit of prices 2, 4, 6 continues by two per bar on every price scale.
    #[test]
    fn regression_extension_projects_prices_before_mapping_to_the_axis() {
        let bars: Vec<_> = [2.0, 4.0, 6.0]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), c, c, c, c, 1.0))
            .collect();
        for mode in [PriceScaleMode::Normal, PriceScaleMode::Logarithmic] {
            for inverted in [false, true] {
                let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(300.0));
                let transform = PriceTransform { mode, base: 2.0 };
                let price = PriceMap::new(
                    transform,
                    PriceRange::new(transform.to_scale(1.0), transform.to_scale(20.0)),
                    0.0,
                    300.0,
                    inverted,
                );
                let coords = ChartMapping::new(rect, 20.0, 0, 2, 3.0, price);
                let mut study = LinearRegressionChannel::new(3, 0.0).with_extend_right(true);
                study.set_colors(vec![Color32::BLUE]);
                study.calculate(&bars);
                let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
                let output = Context::default().run_ui(
                    RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ui| {
                        IndicatorRenderer::render(
                            &RenderContext::new(ui.painter(), rect),
                            &indicators,
                            &bars,
                            &price,
                            &coords,
                        );
                    },
                );
                let want_price = 6.0 + 2.0 * ((rect.right() - coords.idx_to_x(2)) / 20.0) as f64;
                let ends: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|clipped| match clipped.shape {
                        Shape::LineSegment { points, stroke }
                            if stroke.color == Color32::BLUE && points[1].x == rect.right() =>
                        {
                            Some(points[1].y)
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                assert_eq!(ends.len(), 3);
                for y in ends {
                    assert!(
                        (y - price.price_to_y(want_price)).abs() < 1e-3,
                        "{mode:?}, inverted={inverted}: {y}"
                    );
                }
            }
        }
    }
}
