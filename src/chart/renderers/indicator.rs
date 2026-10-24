use crate::chart::renderers::{ChartMapping, RenderContext};
use crate::model::Bar;
use crate::scales::PriceMap;
/// Indicator Renderer
/// Renders indicator lines overlay on the chart
use crate::studies::{
    CrossDirection, Indicator, IndicatorLineStyle, PlotPattern, PlotStyle, PlotType,
};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Stroke};

pub struct IndicatorRenderer;

/// Paint one uninterrupted study segment; callers split at missing values.
pub(crate) fn paint_study_plot(
    painter: &egui::Painter,
    points: &[Pos2],
    plot: PlotStyle,
    baseline: f32,
) {
    if !painter.clip_rect().is_positive() {
        return;
    }
    let baseline = if baseline.is_finite() {
        baseline.clamp(painter.clip_rect().top(), painter.clip_rect().bottom())
    } else {
        painter.clip_rect().bottom()
    };
    match plot.plot_type {
        PlotType::StepLine => {
            let mut steps = Vec::new();
            for (i, point) in points.iter().enumerate() {
                if i > 0 {
                    steps.push(Pos2::new(point.x, points[i - 1].y));
                }
                steps.push(*point);
            }
            paint_study_plot(
                painter,
                &steps,
                PlotStyle {
                    plot_type: PlotType::Line,
                    ..plot
                },
                baseline,
            );
        }
        PlotType::Area => {
            for pair in points.windows(2) {
                paint_band(
                    painter,
                    pair[0],
                    pair[1],
                    Pos2::new(pair[0].x, baseline),
                    Pos2::new(pair[1].x, baseline),
                    plot.color,
                    None,
                );
            }
        }
        PlotType::Crosses => {
            for &point in points {
                for delta in [
                    egui::vec2(plot.width + 1.0, 0.0),
                    egui::vec2(0.0, plot.width + 1.0),
                ] {
                    painter
                        .line_segment([point - delta, point + delta], Stroke::new(1.0, plot.color));
                }
            }
        }
        PlotType::TriangleUp | PlotType::TriangleDown => {
            let sign = if plot.plot_type == PlotType::TriangleUp {
                -1.0
            } else {
                1.0
            };
            for &point in points {
                let size = plot.width + 2.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        point + egui::vec2(0.0, sign * size),
                        point + egui::vec2(-size, -sign * size),
                        point + egui::vec2(size, -sign * size),
                    ],
                    plot.color,
                    Stroke::NONE,
                ));
            }
        }
        PlotType::BackgroundColor => {
            for point in points {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        Pos2::new(point.x - plot.width * 0.5, painter.clip_rect().top()),
                        Pos2::new(point.x + plot.width * 0.5, painter.clip_rect().bottom()),
                    ),
                    0.0,
                    plot.color,
                );
            }
        }
        PlotType::BarColor => {}
        PlotType::Line => match plot.pattern {
            PlotPattern::Solid => {
                painter.add(egui::Shape::line(
                    points.to_vec(),
                    Stroke::new(plot.width, plot.color),
                ));
            }
            PlotPattern::Dashed => {
                painter.extend(egui::Shape::dashed_line(
                    points,
                    Stroke::new(plot.width, plot.color),
                    6.0,
                    4.0,
                ));
            }
            PlotPattern::Dotted => {
                painter.extend(egui::Shape::dotted_line(
                    points,
                    plot.color,
                    4.0,
                    plot.width * 0.5,
                ));
            }
        },
        PlotType::Histogram | PlotType::Columns => {
            for point in points {
                painter.rect_filled(
                    egui::Rect::from_two_pos(
                        Pos2::new(point.x - plot.width * 0.5, baseline),
                        Pos2::new(point.x + plot.width * 0.5, point.y),
                    ),
                    0.0,
                    plot.color,
                );
            }
        }
        PlotType::Circles => {
            for point in points {
                painter.circle_filled(*point, plot.width, plot.color);
            }
        }
    }
}

/// Split crossing boundaries at their intersection, so neither triangle crosses the band.
fn paint_band(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    c: Pos2,
    d: Pos2,
    color: Color32,
    inverted: Option<Color32>,
) {
    let polygon = |points: Vec<Pos2>, below: bool| {
        painter.add(egui::Shape::convex_polygon(
            points,
            if below {
                inverted.unwrap_or(color)
            } else {
                color
            },
            Stroke::NONE,
        ));
    };
    let (left, right) = (a.y - c.y, b.y - d.y);
    if (left < 0.0 && right > 0.0) || (left > 0.0 && right < 0.0) {
        let t = left / (left - right);
        let crossing = a.lerp(b, t);
        polygon(vec![a, crossing, c], left > 0.0);
        polygon(vec![crossing, b, d], right > 0.0);
    } else {
        polygon(vec![a, b, d, c], left + right > 0.0);
    }
}

fn paint_fills(
    painter: &egui::Painter,
    study: &dyn Indicator,
    range: std::ops::Range<usize>,
    map: &PriceMap,
    coords: &ChartMapping,
) {
    use crate::studies::FillBoundary;
    for fill in study.fills() {
        let value = |boundary, bar| match boundary {
            FillBoundary::Level(value) => value.is_finite().then_some(value),
            FillBoundary::Plot(line) => (line < study.line_cnt() && study.plot_style(line).visible)
                .then(|| study.plot_value(line, bar))
                .flatten(),
        };
        for bar in range.start..range.end.saturating_sub(1) {
            let Some((a, b, c, d)) = value(fill.first, bar)
                .zip(value(fill.first, bar + 1))
                .zip(value(fill.second, bar))
                .zip(value(fill.second, bar + 1))
                .map(|(((a, b), c), d)| (a, b, c, d))
            else {
                continue;
            };
            let (x, next_x) = (coords.idx_to_x(bar), coords.idx_to_x(bar + 1));
            let points = [
                Pos2::new(x, map.price_to_y(a)),
                Pos2::new(next_x, map.price_to_y(b)),
                Pos2::new(x, map.price_to_y(c)),
                Pos2::new(next_x, map.price_to_y(d)),
            ];
            if points.iter().all(|point| point.is_finite()) {
                // Colour selection uses values so inverted price maps retain the same meaning.
                let colors =
                    if map.y_to_price(coords.rect.top()) < map.y_to_price(coords.rect.bottom()) {
                        (fill.inverted_color.unwrap_or(fill.color), Some(fill.color))
                    } else {
                        (fill.color, fill.inverted_color)
                    };
                paint_band(
                    painter, points[0], points[1], points[2], points[3], colors.0, colors.1,
                );
            }
        }
    }
}

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
            if !indicator.is_overlay() || indicator.is_volume() {
                continue;
            }

            let mapping = super::study_mapping(
                indicator.as_ref(),
                &ChartMapping {
                    price: *price_scale,
                    ..*coords
                },
            );
            Self::render_study(
                context,
                indicator.as_ref(),
                bars,
                coords.start_idx,
                &mapping.price,
                &mapping,
            );
        }
    }

    /// Shared study geometry for overlays and indicator panes.
    pub(crate) fn render_study(
        context: &RenderContext,
        indicator: &dyn Indicator,
        bars: &[Bar],
        bars_start: usize,
        price_scale: &PriceMap,
        coords: &ChartMapping,
    ) {
        let values = indicator.values();
        let painter = context.painter.with_clip_rect(context.rect);
        let display_start = coords
            .start_idx
            .min(coords.x_to_idx(context.rect.left()).saturating_sub(1));
        let display_end = coords.x_to_idx(context.rect.right()).saturating_add(2);
        paint_fills(
            &painter,
            indicator,
            display_start..display_end,
            price_scale,
            coords,
        );

        // Handle multi-line indicators
        let line_cnt = indicator.line_cnt();

        for line_idx in 0..line_cnt {
            let mut plot = indicator.plot_style(line_idx);
            if matches!(
                plot.plot_type,
                PlotType::Columns | PlotType::BackgroundColor
            ) || (indicator.is_volume() && plot.plot_type == PlotType::Histogram)
            {
                plot.width = coords.bar_spacing
                    * if plot.plot_type != PlotType::BackgroundColor {
                        0.8
                    } else {
                        1.0
                    };
            }
            if !plot.visible {
                continue;
            }
            let mut color = plot.color;
            let offset = indicator.plot_offset(line_idx);
            let source_start = display_start
                .checked_add_signed(offset.saturating_neg())
                .unwrap_or(0);
            let source_end = display_end
                .checked_add_signed(offset.saturating_neg())
                .unwrap_or(0)
                .min(values.len());

            let style = indicator.line_style(line_idx);
            let mut points = Vec::new();

            // values[i] belongs to bar i of the full series and `bars` are the
            // visible ones from coords.start_idx, so only that slice is walked.
            let visible_end = source_end;
            for i in source_start..source_end {
                let Some(display_bar) = i.checked_add_signed(offset) else {
                    continue;
                };
                let x = coords.idx_to_x(display_bar);
                let next_color = indicator.plot_color(line_idx, i).unwrap_or(plot.color);
                if next_color != color && !points.is_empty() {
                    Self::draw_line(
                        context,
                        &points,
                        color,
                        style,
                        PlotStyle { color, ..plot },
                        price_scale.price_to_y(0.0),
                    );
                    let last = points.last().copied();
                    points.clear();
                    if matches!(
                        plot.plot_type,
                        PlotType::Line | PlotType::StepLine | PlotType::Area
                    ) {
                        points.extend(last);
                    }
                }
                color = next_color;
                if plot.plot_type == PlotType::BarColor {
                    if values.value(line_idx, i).is_some()
                        && let Some(bar) = i.checked_sub(bars_start).and_then(|i| bars.get(i))
                    {
                        let width = coords.bar_spacing * 0.8;
                        let open = price_scale.price_to_y(bar.open);
                        let close = price_scale.price_to_y(bar.close);
                        let top = open.min(close);
                        let bottom = open.max(close);
                        let high = price_scale.price_to_y(bar.high);
                        let low = price_scale.price_to_y(bar.low);
                        if [top, bottom, high, low].iter().all(|y| y.is_finite()) {
                            painter.line_segment(
                                [Pos2::new(x, high), Pos2::new(x, low)],
                                Stroke::new(1.0, color),
                            );
                            painter.rect_filled(
                                egui::Rect::from_two_pos(
                                    Pos2::new(x - width * 0.5, top),
                                    Pos2::new(x + width * 0.5, bottom.max(top + 1.0)),
                                ),
                                0.0,
                                color,
                            );
                        }
                    }
                    continue;
                }
                let price = values.value(line_idx, i);

                if let Some(price) = price {
                    // Convert price to screen Y coord
                    let y = price_scale.price_to_y(price);

                    // Keep off-scale endpoints: clipping must retain segments crossing the plot.
                    if y.is_finite() {
                        points.push((Pos2::new(x, y), price));
                    }
                } else {
                    // Break the line if we have a None value
                    if !points.is_empty() {
                        Self::draw_line(
                            context,
                            &points,
                            color,
                            style,
                            plot,
                            price_scale.price_to_y(0.0),
                        );
                    }
                    points.clear();
                }
            }

            if plot.plot_type == PlotType::Line
                && indicator.extend_right()
                && visible_end == values.len()
                && points.len() >= 2
            {
                let ((a, pa), (b, pb)) = (points[points.len() - 2], points[points.len() - 1]);
                if b.x > a.x && b.x < context.rect.right() {
                    let painter = context.painter.with_clip_rect(context.rect);
                    let mut previous = b;
                    let mut extension = vec![b];
                    // Project prices before mapping them, so a logarithmic axis preserves the fit.
                    for step in 1..=((context.rect.right() - b.x).ceil() as usize) {
                        let x = (b.x + step as f32).min(context.rect.right());
                        let price = pb + (pb - pa) * ((x - b.x) / (b.x - a.x)) as f64;
                        let point = Pos2::new(x, price_scale.price_to_y(price));
                        if point.y.is_finite() && previous.y.is_finite() {
                            if plot.pattern == PlotPattern::Solid {
                                painter.line_segment(
                                    [previous, point],
                                    Stroke::new(style.width, color),
                                );
                            } else {
                                extension.push(point);
                            }
                        }
                        previous = point;
                    }
                    if plot.pattern != PlotPattern::Solid {
                        paint_study_plot(&painter, &extension, plot, price_scale.price_to_y(0.0));
                    }
                }
            }
            if let Some((line, label)) = indicator.plot_label()
                && line == line_idx
                && let Some(last) = points.last()
            {
                let at = Pos2::new(
                    last.0
                        .x
                        .min(context.rect.right() - 90.0)
                        .max(context.rect.left()),
                    last.0.y,
                );
                context.painter.with_clip_rect(context.rect).text(
                    at,
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::proportional(12.0),
                    color,
                );
            }

            // Draw final segment
            if !points.is_empty() {
                Self::draw_line(
                    context,
                    &points,
                    color,
                    style,
                    plot,
                    price_scale.price_to_y(0.0),
                );
            }
        }
        let painter = context.painter.with_clip_rect(context.rect);
        for marker in indicator
            .markers()
            .iter()
            .filter(|_| (0..line_cnt).any(|line| indicator.plot_style(line).visible))
        {
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

    fn draw_line(
        context: &RenderContext,
        points: &[(Pos2, f64)],
        color: Color32,
        style: IndicatorLineStyle,
        plot: PlotStyle,
        baseline: f32,
    ) {
        let painter = context.painter.with_clip_rect(context.rect);
        let plot = PlotStyle { color, ..plot };
        if plot.plot_type != PlotType::Line || plot.pattern != PlotPattern::Solid {
            let positions: Vec<_> = points.iter().map(|(point, _)| *point).collect();
            paint_study_plot(&painter, &positions, plot, baseline);
            return;
        }
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

    use crate::studies::{BollingerBands, SMA, StandardErrorBands};

    #[test]
    fn plot_geometry_uses_the_selected_stroke_and_zero_baseline() {
        let color = Color32::from_rgb(17, 71, 191);
        let points = [Pos2::new(10.0, 15.0), Pos2::new(20.0, 5.0)];
        for (plot_type, pattern) in [
            (PlotType::Histogram, PlotPattern::Solid),
            (PlotType::Columns, PlotPattern::Solid),
            (PlotType::StepLine, PlotPattern::Solid),
            (PlotType::Crosses, PlotPattern::Solid),
            (PlotType::TriangleUp, PlotPattern::Solid),
            (PlotType::TriangleDown, PlotPattern::Solid),
            (PlotType::Area, PlotPattern::Solid),
            (PlotType::BackgroundColor, PlotPattern::Solid),
            (PlotType::Circles, PlotPattern::Solid),
            (PlotType::Line, PlotPattern::Dashed),
            (PlotType::Line, PlotPattern::Dotted),
        ] {
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                paint_study_plot(
                    &ui.painter()
                        .with_clip_rect(Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0))),
                    &points,
                    PlotStyle {
                        color,
                        width: 4.0,
                        pattern,
                        plot_type,
                        visible: true,
                    },
                    20.0,
                );
            });
            let painted: Vec<_> = std::mem::take(&mut output.shapes)
                .into_iter()
                .map(|s| s.shape)
                .collect();
            output.drop_without_applying_deltas();
            match plot_type {
                PlotType::Histogram | PlotType::Columns => {
                    let rects: Vec<_> = painted
                        .iter()
                        .filter_map(|s| match s {
                            Shape::Rect(r) if r.fill == color => Some(r.rect),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        rects,
                        [
                            Rect::from_min_max(Pos2::new(8.0, 15.0), Pos2::new(12.0, 20.0)),
                            Rect::from_min_max(Pos2::new(18.0, 5.0), Pos2::new(22.0, 20.0)),
                        ]
                    );
                }
                PlotType::Circles => {
                    let circles = painted.iter().filter(
                        |s| matches!(s, Shape::Circle(c) if c.fill == color && c.radius == 4.0),
                    );
                    assert_eq!(circles.count(), 2);
                }
                PlotType::Line if pattern == PlotPattern::Dashed => {
                    assert!(painted.iter().any(|s| {
                        matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == color && stroke.width == 4.0)
                    }));
                }
                PlotType::StepLine => {
                    assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path)
                        if path.points == [points[0], Pos2::new(20.0, 15.0), points[1]])));
                }
                PlotType::Crosses => {
                    assert_eq!(painted.iter().filter(|shape| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == color)).count(), 4);
                }
                PlotType::TriangleUp | PlotType::TriangleDown => {
                    assert_eq!(painted.iter().filter(|shape| matches!(shape, Shape::Path(path) if path.points.len() == 3 && path.fill == color)).count(), 2);
                }
                PlotType::Area => {
                    assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path)
                        if path.points == [points[0], points[1], Pos2::new(20.0, 20.0), Pos2::new(10.0, 20.0)] && path.fill == color)));
                }
                PlotType::BackgroundColor => {
                    assert_eq!(painted.iter().filter(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == color && rect.rect.height() == 100.0)).count(), 2);
                }
                _ => {
                    assert!(painted.iter().any(|s| {
                        matches!(s, Shape::Circle(c) if c.fill == color && c.radius == 2.0)
                    }));
                }
            }
        }
    }

    #[test]
    fn displaced_bands_split_crossings_without_bridging_gaps() {
        use crate::studies::{
            ConfiguredStudy, CustomIndicator, FillBoundary, IndicatorValue, StudyFill,
        };
        let blue = Color32::from_rgba_unmultiplied(41, 98, 255, 51);
        let red = Color32::from_rgba_unmultiplied(255, 0, 0, 51);
        for inverted in [false, true] {
            let mut study = CustomIndicator::new("Band", |_| {
                vec![
                    IndicatorValue::Multiple(vec![2.0, 0.0]),
                    IndicatorValue::Multiple(vec![0.0, 2.0]),
                    IndicatorValue::None,
                    IndicatorValue::Multiple(vec![2.0, 0.0]),
                ]
            })
            .with_line_cnt(2)
            .with_offsets(vec![2, 2])
            .with_fills(vec![StudyFill {
                first: FillBoundary::Plot(0),
                second: FillBoundary::Plot(1),
                color: blue,
                inverted_color: Some(red),
            }]);
            study.calculate(&[]);
            let study = ConfiguredStudy::new(Box::new(study));
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(240.0, 120.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 2.0),
                0.0,
                120.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 30.0, 0, 3, 0.0, map);
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &[],
                    0,
                    &map,
                    &coords,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let bands: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Path(path) if path.fill.a() == 51 => Some(path),
                    _ => None,
                })
                .collect();
            // Values cross halfway from displayed bar 2 to 3, at price 1.
            assert_eq!(bands.len(), 2);
            let crossing = Pos2::new(209.0, 60.0);
            for band in &bands {
                assert_eq!(band.points.len(), 3);
                assert!(band.points.contains(&crossing), "{:?}", band.points);
                assert!(
                    band.points
                        .iter()
                        .all(|point| (194.0..=224.0).contains(&point.x))
                );
            }
            assert_eq!(bands[0].fill, blue);
            assert_eq!(bands[1].fill, red);
        }
    }

    #[test]
    fn bar_colours_preserve_bodies_on_inverted_scales() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        let bars: Vec<_> = [(1.0, 2.0), (2.0, 1.0)]
            .into_iter()
            .map(|(open, close)| {
                Bar::new(Utc.timestamp_opt(0, 0).unwrap(), open, 2.5, 0.5, close, 1.0)
            })
            .collect();
        let mut study = CustomIndicator::new("Colour", |_| vec![IndicatorValue::Single(1.0); 2])
            .with_plot_types(vec![PlotType::BarColor])
            .with_plot_color(|_, bar, _| {
                Some(if bar == 0 {
                    Color32::GREEN
                } else {
                    Color32::RED
                })
            });
        study.calculate(&bars);
        for inverted in [false, true] {
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(240.0, 120.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 3.0),
                0.0,
                120.0,
                inverted,
            );
            let coords = ChartMapping::new(rect, 30.0, 0, 1, 0.0, map);
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &bars,
                    0,
                    &map,
                    &coords,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            for color in [Color32::GREEN, Color32::RED] {
                let body = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Rect(body) if body.fill == color => Some(body.rect),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(body.y_range(), egui::emath::Rangef::new(40.0, 80.0));
            }
        }
    }

    #[test]
    fn overlay_lines_use_one_point_strokes_and_study_colours() {
        let blue = Color32::from_rgb(41, 98, 255);
        let orange = Color32::from_rgb(255, 167, 38);
        let custom = Color32::from_rgb(10, 200, 90);
        let mut recoloured = BollingerBands::default();
        recoloured.set_colors(vec![custom]);
        let mut sma = SMA::default();
        sma.set_colors(vec![custom]);
        let cases: [(Box<dyn Indicator>, Vec<Color32>); 4] = [
            (
                Box::new(BollingerBands::default()),
                vec![blue, orange, blue],
            ),
            (Box::new(sma), vec![custom]),
            (Box::new(recoloured), vec![custom; 3]),
            (
                Box::new(StandardErrorBands::new(20, 2.0)),
                vec![
                    Color32::from_rgb(156, 39, 176),
                    Color32::from_rgb(120, 123, 134),
                ],
            ),
        ];
        for (study, colours) in cases {
            let name = study.name().to_owned();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.add_indicator(study);
            let ctx = Context::default();
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            if name == "BB" {
                assert!(
                    painted
                        .iter()
                        .any(|shape| matches!(shape, Shape::Path(path) if path.fill.a() == 51)),
                    "band between the outer lines"
                );
            }
            let strokes: Vec<_> = painted
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
            for (_, theme) in crate::theme::presets() {
                let mut chart = chart(&[0.0], &[1.0, 2.0]);
                chart.add_indicator(study.clone_box());
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let shapes = shapes(&ctx, &mut chart, 0.0, Vec::new());
                let fading = shapes.iter().any(|shape| matches!(shape,Shape::Mesh(mesh)
                if mesh.vertices.len()==4 && mesh.vertices[0].color.a()==38 && mesh.vertices[2].color.a()==10));
                assert_eq!(fading, filled, "bottom fade");
                let thick = shapes.iter().any(
                    |shape| matches!(shape,Shape::LineSegment {stroke,..} if stroke.width==3.0),
                );
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
                for (width, low, high) in [(300.0, 1.0, 20.0), (70.0, 3.0, 5.0)] {
                    let rect = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(width, 300.0));
                    let transform = PriceTransform { mode, base: 2.0 };
                    let price = PriceMap::new(
                        transform,
                        PriceRange::new(transform.to_scale(low), transform.to_scale(high)),
                        rect.top(),
                        rect.bottom(),
                        inverted,
                    );
                    let coords = ChartMapping::new(rect, 20.0, 0, 2, 3.0, price);
                    let mut study = LinearRegressionChannel::new(3, 0.0).with_extend_right(true);
                    study.set_colors(vec![Color32::BLUE]);
                    study.calculate(&bars);
                    let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
                    let mut output = Context::default().run_ui(
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
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let want_price =
                        6.0 + 2.0 * ((rect.right() - coords.idx_to_x(2)) / 20.0) as f64;
                    let ends: Vec<_> = shapes
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
                    assert!(
                        shapes.iter().any(|clipped| matches!(
                            clipped.shape,
                            Shape::LineSegment { points, stroke }
                                if stroke.color == Color32::BLUE
                                    && points[0].x == coords.idx_to_x(0)
                                    && points[1].x == coords.idx_to_x(1)
                        )),
                        "the segment entering the visible price range must survive"
                    );
                    for clipped in &shapes {
                        if let Shape::Text(text) = &clipped.shape {
                            assert!(clipped.clip_rect == rect, "fit label must stay in the plot");
                            assert!(text.pos.x >= rect.left(), "fit label escapes a narrow plot");
                        }
                    }
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
}
