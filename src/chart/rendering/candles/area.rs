//! Area-based chart renderers
//!
//! Includes: Area, HLC Area, Baseline

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Painter, Pos2, Rect, Shape, Stroke};

use super::helpers::valued_runs;
use super::line::{draw_bar_line, draw_point_marker, line_color, with_line_type};
use super::volume::render_volume_bars;
use crate::chart::rendering::{RenderContext, StyleColors};
use crate::model::{
    AreaStyle, Bar, BaselineLevel, BaselineStyle, HlcAreaStyle, LinePlotStyle, LineType,
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
    for (point, bar) in line_points.iter().zip(visible_data) {
        if !bar.is_whitespace() {
            draw_point_marker(
                painter,
                *point,
                style.marker_radius,
                line_color(bar, &style.line),
            );
        }
    }

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

/// HLC Area: fills high to close and close to low, with the high, low and close lines on top.
/// The close line is at the bar closes whatever price source the single-value types plot.
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
    style: &HlcAreaStyle,
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
    );

    draw_hlc_fill(painter, visible_data, &points, style);
    for (visible, line, y) in [
        (
            style.high_line_visible,
            &style.high_line,
            (|p| p.y_high) as fn(&HlcPoint) -> f32,
        ),
        (style.low_line_visible, &style.low_line, |p| p.y_low),
        (true, &style.close_line, |p| p.y_close),
    ] {
        if visible {
            let line_points: Vec<Pos2> = points.iter().map(|p| Pos2::new(p.x, y(p))).collect();
            let line = LinePlotStyle {
                line_type: LineType::Simple,
                ..line.clone()
            };
            draw_bar_line(painter, visible_data, &line_points, &line, false);
        }
    }

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
) -> Vec<HlcPoint> {
    data.iter()
        .enumerate()
        .map(|(i, bar)| HlcPoint {
            x: idx_to_coord(start_idx + i, chart_rect_min_x),
            y_high: coords.price_to_y(bar.high),
            y_low: coords.price_to_y(bar.low),
            y_close: coords.price_to_y(bar.close),
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

fn draw_hlc_fill(painter: &Painter, bars: &[Bar], points: &[HlcPoint], style: &HlcAreaStyle) {
    // Both quads are convex: high >= close >= low at both ends of a segment.
    for pair in valued_runs(bars).flat_map(|run| points[run].windows(2)) {
        let (p1, p2) = (&pair[0], &pair[1]);
        if style.high_to_close_fill_visible {
            let quad = vec![
                Pos2::new(p1.x, p1.y_high),
                Pos2::new(p2.x, p2.y_high),
                Pos2::new(p2.x, p2.y_close),
                Pos2::new(p1.x, p1.y_close),
            ];
            painter.add(Shape::convex_polygon(
                quad,
                style.high_to_close_fill,
                Stroke::NONE,
            ));
        }
        if style.close_to_low_fill_visible {
            let quad = vec![
                Pos2::new(p1.x, p1.y_close),
                Pos2::new(p2.x, p2.y_close),
                Pos2::new(p2.x, p2.y_low),
                Pos2::new(p1.x, p1.y_low),
            ];
            painter.add(Shape::convex_polygon(
                quad,
                style.close_to_low_fill,
                Stroke::NONE,
            ));
        }
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
        let (edge, outer, at_base) = if above(a, b) {
            (
                coords.scale_to_y(coords.range().max),
                own.and_then(|colors| colors.top_fill)
                    .unwrap_or(style.above_fill),
                own.and_then(|colors| colors.top_fill_at_base)
                    .or(style.above_fill_at_base),
            )
        } else {
            (
                coords.scale_to_y(coords.range().min),
                own.and_then(|colors| colors.bottom_fill)
                    .unwrap_or(style.below_fill),
                own.and_then(|colors| colors.bottom_fill_at_base)
                    .or(style.below_fill_at_base),
            )
        };
        let at_base = at_base.unwrap_or(outer);
        let color_at = |y: f32| {
            let span = baseline_y - edge;
            let fraction = if span.abs() > f32::EPSILON {
                ((y - edge) / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            outer.lerp_to_gamma(at_base, fraction)
        };
        let mut polygon = vec![a, b, Pos2::new(b.x, baseline_y), Pos2::new(a.x, baseline_y)];
        // Clip before colouring: saturating an offscreen vertex's colour would distort the
        // interpolated gradient inside the pane.
        let (top, bottom) = coords.extent();
        for (bound, keep_below) in [(top, true), (bottom, false)] {
            let inside = |point: &Pos2| {
                if keep_below {
                    point.y >= bound
                } else {
                    point.y <= bound
                }
            };
            if polygon.iter().all(inside) {
                continue;
            }
            let mut clipped = Vec::new();
            for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
                if inside(a) {
                    clipped.push(*a);
                }
                if inside(a) != inside(b) {
                    let t = (bound - a.y) / (b.y - a.y);
                    clipped.push(Pos2::new(a.x + (b.x - a.x) * t, bound));
                }
            }
            polygon = clipped;
        }
        let n = fill.vertices.len() as u32;
        for point in &polygon {
            fill.colored_vertex(*point, color_at(point.y));
        }
        for i in 2..polygon.len() as u32 {
            fill.add_triangle(n, n + i - 1, n + i);
        }
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
    for (point, bar) in points.iter().zip(data) {
        if bar.is_whitespace() {
            continue;
        }
        let own = bar.colors.as_deref();
        let color = if above(*point, *point) {
            own.and_then(|colors| colors.top_line)
                .unwrap_or(style.above.color)
        } else {
            own.and_then(|colors| colors.bottom_line)
                .unwrap_or(style.below.color)
        };
        draw_point_marker(painter, *point, style.marker_radius, color);
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
    use egui::{Color32, Context, LayerId, RawInput, pos2, vec2};

    #[test]
    fn baseline_gradients_keep_endpoints_crossings_and_point_overrides() {
        use crate::model::BarColors;
        use crate::scales::{PriceRange, PriceTransform};
        // On a 0..100 pane with base 50, prices 75 and 25 are halfway along each
        // gradient: (200 + 40) / 2 = 120. The point override fades 160 to 0, halfway 80.
        let style: BaselineStyle = serde_json::from_value(serde_json::json!({
            "above_fill": [0, 200, 0, 200], "above_fill_at_base": [0, 40, 0, 40],
            "below_fill": [200, 0, 0, 200], "below_fill_at_base": [40, 0, 0, 40]
        }))
        .unwrap();
        let own: BarColors = serde_json::from_value(serde_json::json!({
            "top_fill": [0, 0, 160, 160], "top_fill_at_base": [0, 0, 0, 0],
            "bottom_fill": [160, 160, 0, 160], "bottom_fill_at_base": [0, 0, 0, 0]
        }))
        .unwrap();
        let mut bars: Vec<_> = [
            100.0,
            75.0,
            50.0,
            25.0,
            0.0,
            f64::NAN,
            150.0,
            75.0,
            25.0,
            -50.0,
        ]
        .into_iter()
        .map(|price| {
            if price.is_nan() {
                Bar::whitespace(chrono::DateTime::UNIX_EPOCH)
            } else {
                Bar::from_value(chrono::DateTime::UNIX_EPOCH, price)
            }
        })
        .collect();
        bars[7].colors = Some(Box::new(own));
        for inverted in [false, true] {
            for line_type in [
                crate::model::LineType::Simple,
                crate::model::LineType::Curved,
            ] {
                let mut style = style.clone();
                style.above.line_type = line_type;
                let map = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(0.0, 100.0),
                    0.0,
                    100.0,
                    inverted,
                );
                let ctx = Context::default();
                let output = ctx.run_ui(RawInput::default(), |ui| {
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
                });
                let mesh = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Mesh(mesh) => Some(mesh.clone()),
                        _ => None,
                    })
                    .unwrap();
                output.drop_without_applying_deltas();
                for triangle in mesh.indices.as_chunks::<3>().0 {
                    let vertices = triangle.map(|i| &mesh.vertices[i as usize]);
                    if vertices.iter().all(|v| v.pos.y == 50.0) {
                        continue;
                    }
                    let xs = vertices.map(|v| v.pos.x);
                    assert!(xs.iter().all(|x| *x <= 40.0) || xs.iter().all(|x| *x >= 60.0));
                    let point_override = xs.iter().all(|x| (70.0..=80.0).contains(x));
                    let price = |y| if inverted { y } else { 100.0 - y };
                    let above = vertices.iter().map(|v| price(v.pos.y)).sum::<f32>() / 3.0 >= 50.0;
                    for vertex in vertices {
                        assert!(vertex.pos.is_finite());
                        assert!((0.0..=100.0).contains(&vertex.pos.y));
                        let distance = (price(vertex.pos.y) - 50.0).abs();
                        let alpha = if point_override {
                            3.2 * distance
                        } else {
                            40.0 + 3.2 * distance
                        }
                        .round() as u8;
                        let expected = match (point_override, above) {
                            (true, true) => [0, 0, alpha, alpha],
                            (true, false) => [alpha, alpha, 0, alpha],
                            (false, true) => [0, alpha, 0, alpha],
                            (false, false) => [alpha, 0, 0, alpha],
                        };
                        assert_eq!(
                            vertex.color.to_array(),
                            expected,
                            "{inverted} {line_type:?} {:?}",
                            vertex.pos
                        );
                    }
                }
                for (x, price, expected) in [
                    (0.0, 100.0, [0, 200, 0, 200]),
                    (10.0, 75.0, [0, 120, 0, 120]),
                    (10.0, 50.0, [0, 40, 0, 40]),
                    (30.0, 50.0, [40, 0, 0, 40]),
                    (30.0, 25.0, [120, 0, 0, 120]),
                    (40.0, 0.0, [200, 0, 0, 200]),
                    (70.0, 75.0, [0, 0, 80, 80]),
                    (80.0, 25.0, [80, 80, 0, 80]),
                ] {
                    assert!(
                        mesh.vertices
                            .iter()
                            .any(|v| v.pos == pos2(x, map.price_to_y(price))
                                && v.color.to_array() == expected),
                        "missing endpoint or midpoint at {x}, {price}, {inverted}, {line_type:?}"
                    );
                }
            }
        }
    }

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

    /// Every line and fill paints its own price band of an asymmetric bar from its own setting,
    /// and each can go without the others. The close line stays at the closes when the chart's
    /// price source, which the single-value chart types plot, is another price.
    #[test]
    fn hlc_area_paints_each_line_and_fill_from_its_own_setting() {
        use super::super::{
            BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, HeikinAshiCache,
            TradingColors, VolumeSettings, render_chart_type,
        };
        use crate::model::{BarClock, ChartType, ChartTypeStyles, Timeframe};
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
        // Close is nearer the high: at price 100 the y is 0, so high 70, close 60, low 20.
        let bars: Vec<_> = (0..2)
            .map(|_| Bar::new(chrono::DateTime::UNIX_EPOCH, 50.0, 70.0, 20.0, 60.0, 1.0))
            .collect();
        let line = |r, width| LinePlotStyle {
            color: Color32::from_rgb(r, 1, 1),
            width,
            line_type: LineType::Curved,
            ..Default::default()
        };
        let (upper, lower) = (
            Color32::from_rgba_unmultiplied(9, 0, 0, 77),
            Color32::from_rgba_unmultiplied(0, 9, 0, 128),
        );
        let style = HlcAreaStyle {
            high_line: line(10, 1.0),
            low_line: line(20, 2.0),
            close_line: line(30, 3.0),
            high_to_close_fill: upper,
            close_to_low_fill: lower,
            ..Default::default()
        };
        let high = (10, 1.0, 30.0);
        let low = (20, 2.0, 80.0);
        let close = (30, 3.0, 40.0);
        let upper_band = (upper, 30.0, 40.0);
        let lower_band = (lower, 40.0, 80.0);
        for (style, source, fills, lines) in [
            (
                style.clone(),
                PriceSource::Close,
                vec![upper_band, lower_band],
                vec![high, low, close],
            ),
            (
                style.clone(),
                PriceSource::High,
                vec![upper_band, lower_band],
                vec![high, low, close],
            ),
            (
                HlcAreaStyle {
                    high_line_visible: false,
                    ..style.clone()
                },
                PriceSource::Close,
                vec![upper_band, lower_band],
                vec![low, close],
            ),
            (
                HlcAreaStyle {
                    low_line_visible: false,
                    ..style.clone()
                },
                PriceSource::Close,
                vec![upper_band, lower_band],
                vec![high, close],
            ),
            (
                HlcAreaStyle {
                    high_to_close_fill_visible: false,
                    ..style.clone()
                },
                PriceSource::Close,
                vec![lower_band],
                vec![high, low, close],
            ),
            (
                HlcAreaStyle {
                    close_to_low_fill_visible: false,
                    ..style.clone()
                },
                PriceSource::Close,
                vec![upper_band],
                vec![high, low, close],
            ),
            (
                HlcAreaStyle {
                    close_line: line(30, 0.0),
                    ..style.clone()
                },
                PriceSource::Close,
                vec![upper_band, lower_band],
                vec![high, low],
            ),
        ] {
            let type_params = crate::model::ChartTypeParams {
                styles: ChartTypeStyles {
                    hlc_area: style,
                    ..Default::default()
                },
                ..Default::default()
            };
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |_| {
                let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                let context = RenderContext::new(&painter, rect);
                render_chart_type(
                    ChartType::HlcArea,
                    &CandleDataContext {
                        price_ctx: &context,
                        volume_ctx: &context,
                        price_scale: &map,
                        colors: &colors,
                        visible_data: &bars,
                        full_data: &bars,
                        revision: 0,
                        heikin_ashi: &HeikinAshiCache::default(),
                        shows_elements: false,
                        start_idx: 0,
                        clock: &BarClock::new(Timeframe::Min1),
                        tick_size: None,
                        price_formatter: None,
                    },
                    &ChartTypeParams::new(
                        BarDimensions::new(1.0, 1.0),
                        VolumeSettings::new(false, 0.0),
                        &type_params,
                        TradingColors::new(colors.bullish, colors.bearish),
                        CoordMapping::new(0.0),
                        source,
                    ),
                    |i, _| 10.0 + i as f32 * 50.0,
                );
            });
            let mut painted_fills = Vec::new();
            let mut painted_lines = Vec::new();
            for shape in &output.shapes {
                if let Shape::Path(path) = &shape.shape {
                    let ys = || path.points.iter().map(|p| p.y);
                    if path.fill != Color32::TRANSPARENT {
                        let top = ys().fold(f32::INFINITY, f32::min).round();
                        let bottom = ys().fold(f32::NEG_INFINITY, f32::max).round();
                        painted_fills.push((path.fill, top, bottom));
                    } else {
                        let stroke = path.stroke.color.clone();
                        let egui::epaint::ColorMode::Solid(color) = stroke else {
                            panic!("lines take one colour");
                        };
                        // A curve would add points between the two bars.
                        assert_eq!(path.points.len(), 2);
                        painted_lines.push((
                            color.r(),
                            path.stroke.width,
                            path.points[0].y.round(),
                        ));
                    }
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!(painted_fills, fills, "{source:?}");
            assert_eq!(painted_lines, lines, "{source:?}");
        }
    }
}
