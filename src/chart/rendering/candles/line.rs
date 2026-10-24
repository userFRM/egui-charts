//! Line-based chart renderers
//!
//! Includes: Line, Line with Markers, Step Line

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::CubicBezierShape;
use egui::{Color32, Painter, Pos2, Shape, Stroke};

use super::volume::render_volume_bars;
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::model::{Bar, LinePlotStyle, LineType, PriceSource};

pub(super) fn render_line(
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
    style: &LinePlotStyle,
    price_source: PriceSource,
) {
    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = price_scale.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect();

    if points.len() > 1 {
        draw_styled_line(painter, with_line_type(points, style.line_type), style);
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

/// Line with circular markers at each data point
pub(super) fn render_line_with_markers(
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
    style: &LinePlotStyle,
    price_source: PriceSource,
    marker_radius: f32,
) {
    let points: Vec<Pos2> = visible_data
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = price_scale.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect();

    if points.len() > 1 {
        draw_styled_line(
            painter,
            with_line_type(points.clone(), style.line_type),
            style,
        );
    }

    // Draw markers
    for point in points
        .iter()
        .filter(|_| marker_radius.is_finite() && marker_radius > 0.0)
    {
        painter.add(Shape::circle_filled(*point, marker_radius, style.color));
        painter.add(Shape::circle_stroke(
            *point,
            marker_radius,
            Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                DESIGN_TOKENS.semantic.chart.crosshair_label_text,
            ),
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

/// Step line: horizontal then vertical movement
pub(super) fn render_step_line(
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
    style: &LinePlotStyle,
    price_source: PriceSource,
) {
    if visible_data.is_empty() {
        return;
    }

    let mut points = Vec::new();
    let mut prev_y = None;

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
        let y = price_scale.price_to_y(value);

        if let Some(prev) = prev_y {
            // Add horizontal step first
            points.push(Pos2::new(x, prev));
        }
        // Then vertical
        points.push(Pos2::new(x, y));
        prev_y = Some(y);
    }

    if points.len() > 1 {
        draw_styled_line(painter, points, style);
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

/// `points` joined as `line_type` asks: unchanged for straight segments, or flattened from a
/// Catmull-Rom spline through every point.
pub(super) fn with_line_type(points: Vec<Pos2>, line_type: LineType) -> Vec<Pos2> {
    match line_type {
        LineType::Simple => points,
        LineType::Curved => curve_through(&points),
    }
}

/// A polyline within a quarter point of the uniform Catmull-Rom spline through `points`. The
/// segment from `p1` to `p2` is the cubic Bezier with control points `p1 + (p2 - p0) / 6` and
/// `p2 - (p3 - p1) / 6`, repeating the end points beyond the ends, so the curve passes through
/// every point with a continuous tangent and its x stays monotonic when theirs is.
fn curve_through(points: &[Pos2]) -> Vec<Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    let mut curve = vec![points[0]];
    for i in 0..last {
        let (p0, p1, p2, p3) = (
            points[i.saturating_sub(1)],
            points[i],
            points[i + 1],
            points[(i + 2).min(last)],
        );
        let segment = CubicBezierShape::from_points_stroke(
            [p1, p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2],
            false,
            Color32::TRANSPARENT,
            Stroke::NONE,
        );
        curve.extend(segment.flatten(Some(0.25)).into_iter().skip(1));
        if curve.last() != Some(&p2) {
            curve.push(p2);
        }
    }
    curve
}

/// Paints the crosshair marker: a dot of `radius` in the series' line colour, ringed in the
/// chart background so it stands out from the line.
pub(crate) fn render_crosshair_marker(
    painter: &Painter,
    point: Pos2,
    radius: f32,
    color: Color32,
    background: Color32,
) {
    if !radius.is_finite() || radius <= 0.0 || !point.is_finite() {
        return;
    }
    painter.circle(
        point,
        radius,
        color,
        Stroke::new(DESIGN_TOKENS.stroke.thick, background),
    );
}

/// Seconds of one last price pulse.
pub(crate) const LAST_PRICE_PULSE_SECONDS: f64 = 2.6;

/// Paints the last price pulse at `phase`, 0 to 1 through one pulse: a dot in the series' line
/// colour and a halo that grows from it and fades out.
pub(crate) fn render_last_price_pulse(painter: &Painter, point: Pos2, color: Color32, phase: f32) {
    if !point.is_finite() {
        return;
    }
    let phase = phase.clamp(0.0, 1.0);
    let dot = 3.0;
    let halo = color.gamma_multiply(0.4 * (1.0 - phase));
    painter.circle_filled(point, dot + 9.0 * phase, halo);
    painter.circle_filled(point, dot, color);
}

/// Paint a line with the chart type's own style; dashes keep their phase through corners.
pub(super) fn draw_styled_line(painter: &Painter, points: Vec<Pos2>, style: &LinePlotStyle) {
    if !style.width.is_finite() || style.width <= 0.0 {
        return;
    }
    let stroke = Stroke::new(style.width, style.color);
    if let Some((dash, gap)) = style.style.dash_pattern() {
        crate::chart::rendering::dashed_line::draw_dashed_polyline(
            painter, &points, stroke, dash, gap,
        );
    } else {
        painter.add(Shape::line(points, stroke));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The uniform Catmull-Rom segment from p1 to p2 is at (-p0 + 9 p1 + 9 p2 - p3) / 16 halfway,
    /// which for these points is (15, 3.75) between (10, 10) and (20, 0).
    #[test]
    fn curve_passes_through_points_and_the_spline_midpoint() {
        let points = [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(20.0, 0.0),
            Pos2::new(30.0, 30.0),
        ];
        let curve = with_line_type(points.to_vec(), LineType::Curved);
        assert!(points.iter().all(|point| curve.contains(point)));
        assert!(curve.windows(2).all(|pair| pair[0].x <= pair[1].x));
        let distance = |target: Pos2| {
            curve
                .windows(2)
                .map(|pair| {
                    let (a, b) = (pair[0], pair[1]);
                    let t = ((target - a).dot(b - a) / (b - a).length_sq()).clamp(0.0, 1.0);
                    target.distance(a + (b - a) * t)
                })
                .fold(f32::INFINITY, f32::min)
        };
        assert!(distance(Pos2::new(15.0, 3.75)) <= 0.25);
        assert_eq!(
            with_line_type(points.to_vec(), LineType::Simple),
            points.to_vec()
        );
    }
}
