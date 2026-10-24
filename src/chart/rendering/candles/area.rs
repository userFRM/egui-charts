//! Area-based chart renderers
//!
//! Includes: Area, HLC Area, Baseline

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::line::{draw_styled_line, with_line_type};
use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{AreaStyle, Bar, BaselineLevel, BaselineStyle, PriceSource};

const HLC_AREA_ALPHA: u8 = 30;

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

    if line_points.len() > 1 {
        let line_points = with_line_type(line_points, style.line.line_type);
        draw_area_fill(
            painter,
            &line_points,
            price_rect.max.y,
            style.top_fill,
            style.bottom_fill,
        );
        draw_styled_line(painter, line_points, &style.line);
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

    if points.len() > 1 {
        draw_hlc_fill(painter, &points, bullish_color, bearish_color);
        let close_points: Vec<Pos2> = points.iter().map(|p| Pos2::new(p.x, p.y_close)).collect();
        painter.add(Shape::line(
            close_points,
            Stroke::new(DESIGN_TOKENS.stroke.thick, bullish_color),
        ));
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

fn draw_area_fill(
    painter: &Painter,
    points: &[Pos2],
    baseline_y: f32,
    top_color: Color32,
    bottom_color: Color32,
) {
    let top = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    if !top.is_finite() || baseline_y <= top {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for (i, point) in points.iter().enumerate() {
        let fraction = ((point.y - top) / (baseline_y - top)).clamp(0.0, 1.0);
        mesh.colored_vertex(*point, top_color.lerp_to_gamma(bottom_color, fraction));
        mesh.colored_vertex(Pos2::new(point.x, baseline_y), bottom_color);
        if i > 0 {
            let n = (2 * i) as u32;
            mesh.add_triangle(n - 2, n, n - 1);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    painter.add(mesh);
}

fn draw_hlc_fill(painter: &Painter, points: &[HlcPoint], bullish: Color32, bearish: Color32) {
    let tint = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), HLC_AREA_ALPHA);
    let (up, down) = (tint(bullish), tint(bearish));

    // Both quads are convex: high >= close >= low at both ends of a segment.
    for pair in points.windows(2) {
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
    let points = with_line_type(
        collect_price_points(
            data,
            start_idx,
            coords,
            chart_rect_min_x,
            idx_to_coord,
            price_source,
        ),
        style.above.line_type,
    );
    let mut distance = 0.0;
    for pair in points.windows(2) {
        let (prev, curr) = (pair[0], pair[1]);
        // A segment that crosses the base line is split there, in screen space, where the
        // line is straight on every price scale.
        let crossing = (prev.y < baseline_y && curr.y > baseline_y)
            || (prev.y > baseline_y && curr.y < baseline_y);
        if crossing {
            let t = (baseline_y - prev.y) / (curr.y - prev.y);
            let cross = Pos2::new(prev.x + (curr.x - prev.x) * t, baseline_y);
            draw_baseline_part(painter, coords, prev, cross, baseline_y, style, distance);
            draw_baseline_part(
                painter,
                coords,
                cross,
                curr,
                baseline_y,
                style,
                distance + f64::from(prev.distance(cross)),
            );
        } else {
            draw_baseline_part(painter, coords, prev, curr, baseline_y, style, distance);
        }
        distance += f64::from(prev.distance(curr));
    }
}

/// A part of a segment that does not cross the base line: its line and its fill down to the base.
fn draw_baseline_part(
    painter: &Painter,
    coords: &PriceMap,
    a: Pos2,
    b: Pos2,
    baseline_y: f32,
    style: &BaselineStyle,
    distance: f64,
) {
    // The side is decided by value, so an inverted price scale keeps its colours; a part that
    // only touches the base line takes its other side.
    let (line, fill) =
        if coords.y_to_price(f32::midpoint(a.y, b.y)) >= coords.y_to_price(baseline_y) {
            (&style.above, style.above_fill)
        } else {
            (&style.below, style.below_fill)
        };
    let fill_points = vec![a, b, Pos2::new(b.x, baseline_y), Pos2::new(a.x, baseline_y)];
    painter.add(Shape::convex_polygon(fill_points, fill, Stroke::NONE));
    if !line.width.is_finite() || line.width <= 0.0 {
        return;
    }
    let stroke = Stroke::new(line.width, line.color);
    if let Some((dash, gap)) = line.style.dash_pattern() {
        crate::chart::rendering::dashed_line::draw_dashed_segment(
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
    fn area_gradient_uses_visible_high_and_reaches_the_plot_bottom() {
        // From y=20 to y=100, y=60 is halfway, so 45% becomes 22.5%.
        // Moving the visible high to y=40 keeps the same endpoint opacities.
        for top in [20.0, 40.0] {
            let ctx = Context::default();
            let output = ctx.run_ui(RawInput::default(), |_| {
                let painter = Painter::new(
                    ctx.clone(),
                    LayerId::background(),
                    Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
                );
                draw_area_fill(
                    &painter,
                    &[
                        pos2(10.0, top),
                        pos2(20.0, (top + 100.0) / 2.0),
                        pos2(30.0, top),
                    ],
                    100.0,
                    Color32::from_rgba_unmultiplied(0, 0, 255, 115),
                    Color32::TRANSPARENT,
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
            for vertex in vertices {
                let expected = (115.0 * (100.0 - vertex.pos.y) / (100.0 - top)).round() as u8;
                assert_eq!(vertex.color.a(), expected);
            }
        }
    }
}
