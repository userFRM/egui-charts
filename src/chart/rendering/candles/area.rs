//! Area-based chart renderers
//!
//! Includes: Area, HLC Area, Baseline

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::helpers::valued_runs;
use super::line::{draw_bar_line, with_line_type};
use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{
    AreaStyle, Bar, BaselineLevel, BaselineStyle, HlcAreaStyle, LinePlotStyle, LineStyle,
    PriceSource,
};

pub(super) fn render_area(
    painter: &Painter,
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &AreaStyle,
    price_source: PriceSource,
) {
    let coords = *price_scale;
    let line_points = collect_price_points(
        visible_data,
        start_idx,
        &coords,
        chart_rect_min_x,
        &idx_to_coord,
        price_source,
    );

    let mut fill_points = Vec::new();
    for run in valued_runs(visible_data).filter(|run| run.len() > 1) {
        if !fill_points.is_empty() {
            fill_points.push(Pos2::new(f32::NAN, f32::NAN));
        }
        fill_points.extend(with_line_type(
            line_points[run].to_vec(),
            style.line.line_type,
        ));
    }
    draw_area_fill(painter, &fill_points, price_rect, style);
    draw_bar_line(painter, visible_data, &line_points, &style.line, false);

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// HLC Area: fills high to close in the bullish colour and close to low in the bearish colour,
/// with the close line
pub(super) fn render_hlc_area(
    painter: &Painter,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
    style: &HlcAreaStyle,
    price_source: PriceSource,
) {
    if visible_data.is_empty() {
        return;
    }

    let coords = *price_scale;
    let points = collect_hlc_points(
        visible_data,
        start_idx,
        &coords,
        chart_rect_min_x,
        &idx_to_coord,
        price_source,
    );

    draw_hlc_fill(
        painter,
        visible_data,
        &points,
        bullish_color,
        bearish_color,
        style.fill_opacity,
    );
    let close_points: Vec<Pos2> = points.iter().map(|p| Pos2::new(p.x, p.y_close)).collect();
    let close_line = LinePlotStyle {
        color: bullish_color,
        width: style.line_width,
        style: LineStyle::Solid,
        ..Default::default()
    };
    draw_bar_line(painter, visible_data, &close_points, &close_line, false);

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Baseline chart: coloured by the side of the base line, split where a segment crosses it
pub(super) fn render_baseline(
    painter: &Painter,
    price_rect: Rect,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &BaselineStyle,
    price_source: PriceSource,
    level: BaselineLevel,
) {
    if visible_data.is_empty() {
        return;
    }

    let coords = *price_scale;
    let baseline_y = match level {
        BaselineLevel::Price(price) => coords.price_to_y(price),
        BaselineLevel::PercentOfPane(percent) => {
            price_rect.max.y - percent / 100.0 * price_rect.height()
        }
    };

    draw_baseline_line(painter, price_rect, baseline_y);
    draw_baseline_segments(
        painter,
        visible_data,
        start_idx,
        &coords,
        baseline_y,
        chart_rect_min_x,
        &idx_to_coord,
        style,
        price_source,
    );

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

// === Helper Functions ===

fn collect_price_points(
    data: &[Bar],
    start_idx: usize,
    coords: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    price_source: PriceSource,
) -> Vec<Pos2> {
    data.iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = coords.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect()
}

struct HlcPoint {
    x: f32,
    y_high: f32,
    y_low: f32,
    y_close: f32,
}

fn collect_hlc_points(
    data: &[Bar],
    start_idx: usize,
    coords: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    price_source: PriceSource,
) -> Vec<HlcPoint> {
    data.iter()
        .enumerate()
        .map(|(i, bar)| {
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            HlcPoint {
                x: idx_to_coord(start_idx + i, chart_rect_min_x),
                y_high: coords.price_to_y(bar.high),
                y_low: coords.price_to_y(bar.low),
                y_close: coords.price_to_y(value),
            }
        })
        .collect()
}

/// Fills between the line and the bottom of `rect`, or its top when inverted. The gradient runs
/// from `top_fill` to `bottom_fill` over the whole pane, or with a relative gradient from the
/// fill's far edge on the line to the pane edge it reaches.
fn draw_area_fill(painter: &Painter, points: &[Pos2], rect: Rect, style: &AreaStyle) {
    let finite = points.iter().map(|p| p.y).filter(|y| y.is_finite());
    let (high, low) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |(high, low), y| {
        (high.min(y), low.max(y))
    });
    if !high.is_finite() {
        return;
    }
    let base_y = if style.invert_filled_area {
        rect.min.y
    } else {
        rect.max.y
    };
    let (top, bottom) = match (style.relative_gradient, style.invert_filled_area) {
        (false, _) => (rect.min.y, rect.max.y),
        (true, false) => (high, rect.max.y),
        (true, true) => (rect.min.y, low),
    };
    let color_at = |y: f32| {
        let fraction = if bottom > top {
            ((y - top) / (bottom - top)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        style.top_fill.lerp_to_gamma(style.bottom_fill, fraction)
    };
    let mut mesh = egui::Mesh::default();
    let mut connected = false;
    for point in points {
        if !point.is_finite() {
            connected = false;
            continue;
        }
        let n = mesh.vertices.len() as u32;
        mesh.colored_vertex(*point, color_at(point.y));
        mesh.colored_vertex(Pos2::new(point.x, base_y), color_at(base_y));
        if connected {
            mesh.add_triangle(n - 2, n, n - 1);
            mesh.add_triangle(n - 1, n, n + 1);
        }
        connected = true;
    }
    painter.add(mesh);
}

fn draw_hlc_fill(
    painter: &Painter,
    bars: &[Bar],
    points: &[HlcPoint],
    bullish: Color32,
    bearish: Color32,
    opacity: f32,
) {
    // `as u8` saturates and maps NaN to 0.
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let tint = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);
    let (up, down) = (tint(bullish), tint(bearish));

    // Both quads are convex: high >= close >= low at both ends of a segment.
    for pair in valued_runs(bars).flat_map(|run| points[run].windows(2)) {
        let (p1, p2) = (&pair[0], &pair[1]);
        let above = vec![
            Pos2::new(p1.x, p1.y_high),
            Pos2::new(p2.x, p2.y_high),
            Pos2::new(p2.x, p2.y_close),
            Pos2::new(p1.x, p1.y_close),
        ];
        let below = vec![
            Pos2::new(p1.x, p1.y_close),
            Pos2::new(p2.x, p2.y_close),
            Pos2::new(p2.x, p2.y_low),
            Pos2::new(p1.x, p1.y_low),
        ];
        painter.add(Shape::convex_polygon(above, up, Stroke::NONE));
        painter.add(Shape::convex_polygon(below, down, Stroke::NONE));
    }
}

fn draw_baseline_line(painter: &Painter, rect: Rect, baseline_y: f32) {
    painter.add(Shape::line_segment(
        [
            Pos2::new(rect.min.x, baseline_y),
            Pos2::new(rect.max.x, baseline_y),
        ],
        Stroke::new(
            DESIGN_TOKENS.stroke.hairline,
            DESIGN_TOKENS.semantic.extended.gray,
        ),
    ));
}

fn draw_baseline_segments(
    painter: &Painter,
    data: &[Bar],
    start_idx: usize,
    coords: &PriceMap,
    baseline_y: f32,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &BaselineStyle,
    price_source: PriceSource,
) {
    let points = collect_price_points(
        data,
        start_idx,
        coords,
        chart_rect_min_x,
        idx_to_coord,
        price_source,
    );
    // Split at whitespace before curving, and retain each source bar for its segment colours.
    let mut parts = Vec::new();
    for run in valued_runs(data).filter(|run| run.len() > 1) {
        let path = with_line_type(points[run.clone()].to_vec(), style.above.line_type);
        let mut bar = run.start;
        let mut distance = 0.0;
        for pair in path.windows(2) {
            let (prev, curr) = (pair[0], pair[1]);
            while bar + 1 < run.end - 1 && prev == points[bar + 1] {
                bar += 1;
            }
            let crossing = (prev.y < baseline_y && curr.y > baseline_y)
                || (prev.y > baseline_y && curr.y < baseline_y);
            if crossing {
                let t = (baseline_y - prev.y) / (curr.y - prev.y);
                let cross = Pos2::new(prev.x + (curr.x - prev.x) * t, baseline_y);
                parts.push((prev, cross, distance, bar));
                parts.push((cross, curr, distance + f64::from(prev.distance(cross)), bar));
            } else {
                parts.push((prev, curr, distance, bar));
            }
            distance += f64::from(prev.distance(curr));
        }
    }
    // The side is decided by value, so an inverted price scale keeps its colours; a part that
    // only touches the base line takes its other side.
    let above = |a: Pos2, b: Pos2| {
        coords.y_to_price(f32::midpoint(a.y, b.y)) >= coords.y_to_price(baseline_y)
    };
    // One unfeathered mesh, so adjacent parts of a densely sampled path leave no seams.
    let mut fill = egui::Mesh::default();
    for &(a, b, _, bar) in &parts {
        let own = data[bar].colors.as_deref();
        let color = if above(a, b) {
            own.and_then(|colors| colors.top_fill)
                .unwrap_or(style.above_fill)
        } else {
            own.and_then(|colors| colors.bottom_fill)
                .unwrap_or(style.below_fill)
        };
        let n = fill.vertices.len() as u32;
        for point in [a, b, Pos2::new(b.x, baseline_y), Pos2::new(a.x, baseline_y)] {
            fill.colored_vertex(point, color);
        }
        fill.add_triangle(n, n + 1, n + 2);
        fill.add_triangle(n, n + 2, n + 3);
    }
    painter.add(fill);
    for &(a, b, distance, bar) in &parts {
        let own = data[bar].colors.as_deref();
        let (line, color) = if above(a, b) {
            (&style.above, own.and_then(|colors| colors.top_line))
        } else {
            (&style.below, own.and_then(|colors| colors.bottom_line))
        };
        let line = LinePlotStyle {
            color: color.unwrap_or(line.color),
            ..line.clone()
        };
        draw_baseline_line_part(painter, a, b, &line, distance);
    }
}

/// The line of a part of the path that does not cross the base line, with its dashes `distance`
/// along the path.
fn draw_baseline_line_part(
    painter: &Painter,
    a: Pos2,
    b: Pos2,
    line: &crate::model::LinePlotStyle,
    distance: f64,
) {
    if !line.width.is_finite() || line.width <= 0.0 {
        return;
    }
    let stroke = Stroke::new(line.width, line.color);
    if let Some((dash, gap)) = line.style.dash_pattern() {
        crate::painting::dashed_line::draw_dashed_segment(
            painter,
            a,
            b,
            stroke,
            dash,
            gap,
            distance.rem_euclid(f64::from(dash + gap)),
        );
    } else {
        painter.line_segment([a, b], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, LayerId, RawInput, pos2, vec2};

    #[test]
    fn baseline_dashes_keep_phase_across_short_segments_and_crossings() {
        use crate::model::LineStyle;
        use crate::scales::{PriceRange, PriceTransform};
        for (first, last) in [(105.0, 105.0), (95.0, 95.0), (103.0, 97.0)] {
            for inverted in [false, true] {
                let ctx = Context::default();
                let map = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(90.0, 110.0),
                    0.0,
                    100.0,
                    inverted,
                );
                let bars: Vec<_> = (0..=59)
                    .map(|i| {
                        let price = first + (last - first) * f64::from(i) / 59.0;
                        Bar::new(
                            chrono::DateTime::UNIX_EPOCH,
                            price,
                            price,
                            price,
                            price,
                            0.0,
                        )
                    })
                    .collect();
                let mut style = BaselineStyle::default();
                style.above.style = LineStyle::Dashed;
                style.below.style = LineStyle::Dashed;
                let output = ctx.run_ui(RawInput::default(), |_| {
                    let painter = Painter::new(
                        ctx.clone(),
                        LayerId::background(),
                        Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
                    );
                    draw_baseline_segments(
                        &painter,
                        &bars,
                        0,
                        &map,
                        map.price_to_y(100.0),
                        0.0,
                        |i, _| 10.0 + i as f32 * 30.0 / 59.0,
                        &style,
                        PriceSource::Close,
                    );
                });
                let ink: f32 = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::LineSegment { points, .. } => Some(points[0].distance(points[1])),
                        _ => None,
                    })
                    .sum();
                let length = 30.0_f32.hypot(map.price_to_y(last) - map.price_to_y(first));
                let expected = (length / 12.0).floor() * 8.0 + (length % 12.0).min(8.0);
                output.drop_without_applying_deltas();
                assert!(
                    (ink - expected).abs() < 0.001,
                    "{first} -> {last}, inverted={inverted}: {ink} != {expected}"
                );
            }
        }
    }

    #[test]
    fn curved_fills_keep_whitespace_and_baseline_bar_colours() {
        use crate::model::{BarColors, LineType};
        use crate::scales::{PriceRange, PriceTransform};
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            0.0,
            100.0,
            false,
        );
        let mut bars: Vec<_> = [60.0, 70.0, 60.0, 0.0, 40.0, 30.0, 40.0]
            .into_iter()
            .map(|value| Bar::from_value(chrono::DateTime::UNIX_EPOCH, value))
            .collect();
        bars[3] = Bar::whitespace(bars[3].time);
        bars[1].colors = Some(Box::new(BarColors {
            top_line: Some(Color32::BLUE),
            top_fill: Some(Color32::GOLD),
            ..Default::default()
        }));
        for baseline in [false, true] {
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                if baseline {
                    let mut style = BaselineStyle::default();
                    style.above.line_type = LineType::Curved;
                    draw_baseline_segments(
                        ui.painter(),
                        &bars,
                        0,
                        &map,
                        50.0,
                        0.0,
                        |i, _| i as f32 * 10.0,
                        &style,
                        PriceSource::Close,
                    );
                } else {
                    let mut style = AreaStyle::default();
                    style.line.line_type = LineType::Curved;
                    render_area(
                        ui.painter(),
                        rect,
                        &RenderContext::new(ui.painter(), rect),
                        &bars,
                        0,
                        false,
                        1.0,
                        &StyleColors {
                            bullish: Color32::GREEN,
                            bearish: Color32::RED,
                            text: Color32::WHITE,
                            bullish_border: None,
                            bearish_border: None,
                            bullish_wick: None,
                            bearish_wick: None,
                            candle_border_width: 0.0,
                        },
                        &map,
                        0.0,
                        |i, _| i as f32 * 10.0,
                        &style,
                        PriceSource::Close,
                    );
                }
            });
            let meshes: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .collect();
            assert!(!meshes.is_empty());
            for mesh in &meshes {
                assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
                for triangle in mesh.indices.as_chunks::<3>().0 {
                    let xs = triangle.map(|i| mesh.vertices[i as usize].pos.x);
                    assert!(xs.iter().all(|x| *x <= 20.0) || xs.iter().all(|x| *x >= 40.0));
                }
            }
            if baseline {
                assert!(
                    meshes
                        .iter()
                        .flat_map(|mesh| &mesh.vertices)
                        .any(|vertex| vertex.color == Color32::GOLD)
                );
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                    Shape::LineSegment { points, stroke } if stroke.color == Color32::BLUE && points[0].x >= 10.0 && points[1].x <= 20.0)));
            }
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn area_gradient_spans_the_fill_or_the_pane_on_either_side() {
        // From y=20 to y=100, y=60 is halfway, so 45% becomes 22.5%. Moving the visible high
        // to y=40 keeps the same endpoint opacities. An inverted fill reaches y=0 and spans the
        // lowest point, y=60 or 70; an absolute gradient spans the 100-point pane either way.
        for (top, relative, inverted) in [
            (20.0, true, false),
            (40.0, true, false),
            (20.0, false, false),
            (20.0, true, true),
            (40.0, true, true),
            (40.0, false, true),
        ] {
            let low = (top + 100.0) / 2.0;
            let (span_top, span_bottom) = match (relative, inverted) {
                (false, _) => (0.0, 100.0),
                (true, false) => (top, 100.0),
                (true, true) => (0.0, low),
            };
            let style = AreaStyle {
                top_fill: Color32::from_rgba_unmultiplied(0, 0, 255, 115),
                bottom_fill: Color32::TRANSPARENT,
                relative_gradient: relative,
                invert_filled_area: inverted,
                ..Default::default()
            };
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |_| {
                let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
                let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                draw_area_fill(
                    &painter,
                    &[pos2(10.0, top), pos2(20.0, low), pos2(30.0, top)],
                    rect,
                    &style,
                );
            });
            let vertices: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| {
                    if let Shape::Mesh(mesh) = &s.shape {
                        Some(mesh.vertices.clone())
                    } else {
                        None
                    }
                })
                .flatten()
                .collect();
            output.drop_without_applying_deltas();
            assert!(
                !vertices.is_empty(),
                "area fill must carry per-vertex gradient colours"
            );
            let base = if inverted { 0.0 } else { 100.0 };
            assert_eq!(vertices.iter().filter(|v| v.pos.y == base).count(), 3);
            for vertex in vertices {
                let fraction: f32 =
                    ((vertex.pos.y - span_top) / (span_bottom - span_top)).clamp(0.0, 1.0);
                let expected = (115.0 * (1.0 - fraction)).round() as u8;
                assert_eq!(vertex.color.a(), expected, "{top} {relative} {inverted}");
            }
        }
    }

    /// The fills take the style's opacity as an alpha of 0 to 255 and the close line its width;
    /// opacities outside 0 to 1 are clamped, a non-finite opacity draws no fill, and a
    /// non-positive width hides the line.
    #[test]
    fn hlc_area_fill_opacity_and_close_line_width_follow_the_style() {
        use crate::scales::{PriceRange, PriceTransform};
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            0.0,
            100.0,
            false,
        );
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
        let bars: Vec<_> = (0..2)
            .map(|_| Bar::new(chrono::DateTime::UNIX_EPOCH, 50.0, 60.0, 40.0, 55.0, 1.0))
            .collect();
        for (fill_opacity, line_width, alphas, widths) in [
            (
                HlcAreaStyle::default().fill_opacity,
                2.0,
                vec![30, 30],
                vec![2.0],
            ),
            (0.5, 4.0, vec![128, 128], vec![4.0]),
            (2.0, 1.0, vec![255, 255], vec![1.0]),
            (f32::NAN, 0.0, vec![], vec![]),
        ] {
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |_| {
                let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                let context = RenderContext::new(&painter, rect);
                render_hlc_area(
                    &painter,
                    &context,
                    &bars,
                    0,
                    false,
                    0.0,
                    &colors,
                    &map,
                    0.0,
                    |i, _| 10.0 + i as f32 * 50.0,
                    Color32::GREEN,
                    Color32::RED,
                    &HlcAreaStyle {
                        fill_opacity,
                        line_width,
                    },
                    PriceSource::Close,
                );
            });
            let mut fills = Vec::new();
            let mut lines = Vec::new();
            for shape in &output.shapes {
                if let Shape::Path(path) = &shape.shape {
                    if path.fill != Color32::TRANSPARENT {
                        fills.push(path.fill.a());
                    } else if path.stroke.width > 0.0 {
                        lines.push(path.stroke.width);
                    }
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!(fills, alphas, "{fill_opacity}");
            assert_eq!(lines, widths, "{line_width}");
        }
    }
}
