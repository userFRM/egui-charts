//! Line-based chart renderers
//!
//! Includes: Line, Line with Markers, Step Line

use crate::scales::PriceMap;
use crate::tokens::DESIGN_TOKENS;
use egui::epaint::CubicBezierShape;
use egui::{Color32, Painter, Pos2, Shape, Stroke};
use std::ops::Range;

use super::helpers::valued_runs;
use super::volume::render_volume_bars;
use crate::chart::rendering::{RenderContext, StyleColors};
use crate::model::{Bar, ChartTypeStyles, LinePlotStyle, LineType, PriceSource};

/// `bars`, the first at `start_idx`, are the bars in view at `in_view` and the bar beyond each
/// edge where there is one: the line runs through all of them, volume columns stand under the
/// bars in view. `gradient` holds the start and end colours of a line drawn in a gradient.
pub(super) fn render_line(
    painter: &Painter,
    volume_ctx: &RenderContext,
    bars: &[Bar],
    start_idx: usize,
    in_view: Range<usize>,
    show_volume: bool,
    max_volume: f64,
    volume_width: f32,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &LinePlotStyle,
    gradient: Option<[Color32; 2]>,
    price_source: PriceSource,
) {
    let points: Vec<Pos2> = bars
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = price_scale.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect();

    draw_bar_line(painter, bars, &points, style, gradient, false);

    if show_volume {
        render_volume_bars(
            volume_ctx,
            &bars[in_view.clone()],
            start_idx + in_view.start,
            max_volume,
            volume_width,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Line with circular markers at each bar in view, its bars as [`render_line`] takes them
pub(super) fn render_line_with_markers(
    painter: &Painter,
    volume_ctx: &RenderContext,
    bars: &[Bar],
    start_idx: usize,
    in_view: Range<usize>,
    show_volume: bool,
    max_volume: f64,
    volume_width: f32,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &LinePlotStyle,
    price_source: PriceSource,
    marker_radius: f32,
) {
    let points: Vec<Pos2> = bars
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            let y = price_scale.price_to_y(value);
            Pos2::new(x, y)
        })
        .collect();

    draw_bar_line(painter, bars, &points, style, None, false);

    for (point, bar) in points[in_view.clone()].iter().zip(&bars[in_view.clone()]) {
        if !bar.is_whitespace() {
            draw_point_marker(painter, *point, marker_radius, line_color(bar, style));
        }
    }

    if show_volume {
        render_volume_bars(
            volume_ctx,
            &bars[in_view.clone()],
            start_idx + in_view.start,
            max_volume,
            volume_width,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Step line: horizontal then vertical movement, its bars as [`render_line`] takes them
pub(super) fn render_step_line(
    painter: &Painter,
    volume_ctx: &RenderContext,
    bars: &[Bar],
    start_idx: usize,
    in_view: Range<usize>,
    show_volume: bool,
    max_volume: f64,
    volume_width: f32,
    colors: &StyleColors,
    price_scale: &PriceMap,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &LinePlotStyle,
    price_source: PriceSource,
) {
    if bars.is_empty() {
        return;
    }

    let points: Vec<Pos2> = bars
        .iter()
        .enumerate()
        .map(|(i, bar)| {
            let x = idx_to_coord(start_idx + i, chart_rect_min_x);
            let value = price_source.compute(bar.open, bar.high, bar.low, bar.close);
            Pos2::new(x, price_scale.price_to_y(value))
        })
        .collect();
    draw_bar_line(painter, bars, &points, style, None, true);

    if show_volume {
        render_volume_bars(
            volume_ctx,
            &bars[in_view.clone()],
            start_idx + in_view.start,
            max_volume,
            volume_width,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// The colour of the line from `bar` to the next bar: the bar's own, or `style`'s.
pub(super) fn line_color(bar: &Bar, style: &LinePlotStyle) -> Color32 {
    bar.colors
        .as_ref()
        .and_then(|own| own.color)
        .unwrap_or(style.color)
}

/// Paint a data point in its line colour, with the same outline for every line-based plot.
pub(super) fn draw_point_marker(painter: &Painter, point: Pos2, radius: f32, color: Color32) {
    if !radius.is_finite() || radius <= 0.0 || !point.is_finite() {
        return;
    }
    painter.circle_filled(point, radius, color);
    painter.circle_stroke(
        point,
        radius,
        Stroke::new(
            DESIGN_TOKENS.stroke.hairline,
            DESIGN_TOKENS.semantic.chart.crosshair_label_text,
        ),
    );
}

/// The x of the first and the last bar of `bars` that a segment of their line starts at, `x`
/// giving a bar's x from its index in `bars`: where a gradient's start and end colours lie.
/// `None` when the line has no segment.
pub(crate) fn gradient_span(bars: &[Bar], x: impl Fn(usize) -> f32) -> Option<(f32, f32)> {
    let mut starts = valued_runs(bars).flat_map(|run| run.start..run.end - 1);
    let first = starts.next()?;
    Some((x(first), x(starts.last().unwrap_or(first))))
}

/// How far `x` lies from the start to the end of a gradient spanning `span`, from 0 to 1: 0 when
/// the line has one segment or none, or when `x` is not a number.
pub(crate) fn gradient_share(span: Option<(f32, f32)>, x: f32) -> f32 {
    match span {
        Some((first, last)) if last > first => {
            let share = (x - first) / (last - first);
            if share.is_nan() {
                0.0
            } else {
                share.clamp(0.0, 1.0)
            }
        }
        _ => 0.0,
    }
}

/// Paint the line through `points`, one per bar of `bars`, with the chart type's own style: broken
/// at whitespace bars, each segment in the [`line_color`] of the bar it starts at, or with
/// `gradient`, a bar without a colour of its own taking the blend of its start and end colours at
/// the bar's place between the first and the last segment, and with `step`, across to the next bar
/// first and then up or down to it. One solid path per colour, which SVG export can write; dashes
/// keep their phase from one colour to the next.
pub(super) fn draw_bar_line(
    painter: &Painter,
    bars: &[Bar],
    points: &[Pos2],
    style: &LinePlotStyle,
    gradient: Option<[Color32; 2]>,
    step: bool,
) {
    let span = gradient.and_then(|_| gradient_span(bars, |at| points[at].x));
    let color_at = |at: usize| match gradient {
        Some([start, end]) if bars[at].colors.as_ref().and_then(|own| own.color).is_none() => {
            start.lerp_to_gamma(end, gradient_share(span, points[at].x))
        }
        _ => line_color(&bars[at], style),
    };
    for run in valued_runs(bars) {
        let path = if step {
            let mut path = vec![points[run.start]];
            for at in run.start + 1..run.end {
                path.push(Pos2::new(points[at].x, points[at - 1].y));
                path.push(points[at]);
            }
            path
        } else {
            with_line_type(points[run.clone()].to_vec(), style.line_type)
        };
        let mut path_from = 0;
        let mut from = run.start;
        let mut phase = 0.0;
        while from + 1 < run.end {
            let color = color_at(from);
            let to = (from + 1..run.end - 1)
                .find(|&at| color_at(at) != color)
                .unwrap_or(run.end - 1);
            let path_to = path[path_from..]
                .iter()
                .position(|point| *point == points[to])
                .map_or(path.len() - 1, |at| path_from + at);
            let style = LinePlotStyle {
                color,
                ..style.clone()
            };
            phase = draw_styled_line(painter, path[path_from..=path_to].to_vec(), &style, phase);
            path_from = path_to;
            from = to;
        }
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

/// Paint the hovered point with its marker style, falling back to the line colour and
/// chart background for the fill and border.
pub(crate) fn render_crosshair_marker(
    painter: &Painter,
    point: Pos2,
    styles: &ChartTypeStyles,
    color: Color32,
    background: Color32,
) {
    let radius = styles.crosshair_marker_radius;
    if !radius.is_finite() || radius <= 0.0 || !point.is_finite() {
        return;
    }
    let width = styles.crosshair_marker_border_width;
    let stroke = if width.is_finite() && width > 0.0 {
        Stroke::new(
            width,
            styles.crosshair_marker_border_color.unwrap_or(background),
        )
    } else {
        Stroke::NONE
    };
    painter.circle(
        point,
        radius,
        styles.crosshair_marker_fill.unwrap_or(color),
        stroke,
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

/// Paint a line with the chart type's own style, its dashes starting `phase` into their pattern
/// and keeping it through corners. Returns the phase they end at, so that a line painted in
/// pieces keeps one pattern.
pub(super) fn draw_styled_line(
    painter: &Painter,
    points: Vec<Pos2>,
    style: &LinePlotStyle,
    phase: f64,
) -> f64 {
    if !style.width.is_finite() || style.width <= 0.0 {
        return phase;
    }
    let stroke = Stroke::new(style.width, style.color);
    if let Some((dash, gap)) = style.style.dash_pattern() {
        crate::painting::dashed_line::draw_dashed_polyline_from(
            painter, &points, stroke, dash, gap, phase,
        )
    } else {
        painter.add(Shape::line(points, stroke));
        phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::hit_test::point_to_segment_distance;

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
                .map(|pair| point_to_segment_distance(target, pair[0], pair[1]))
                .fold(f32::INFINITY, f32::min)
        };
        assert!(distance(Pos2::new(15.0, 3.75)) <= 0.25);

        // Changing colour at a control point keeps the same spline, and a later gap starts
        // another path without letting its values change this curve's tangent.
        let mut bars = vec![Bar::from_value(chrono::DateTime::UNIX_EPOCH, 1.0); 7];
        bars[1].colors = Some(Box::new(crate::model::BarColors {
            color: Some(Color32::RED),
            ..Default::default()
        }));
        bars[4] = Bar::whitespace(bars[4].time);
        let all_points = [
            points[0],
            points[1],
            points[2],
            points[3],
            Pos2::new(f32::NAN, f32::NAN),
            Pos2::new(50.0, 5.0),
            Pos2::new(60.0, 6.0),
        ];
        let style = LinePlotStyle {
            line_type: LineType::Curved,
            ..Default::default()
        };
        let output = egui::Context::default().run_ui(Default::default(), |ui| {
            draw_bar_line(ui.painter(), &bars, &all_points, &style, None, false);
        });
        let paths: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Path(path) => Some(path.points.clone()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        let mut joined = paths[0].clone();
        for path in &paths[1..3] {
            joined.extend_from_slice(&path[1..]);
        }
        assert_eq!(joined, curve);
        assert_eq!(paths[3], all_points[5..]);
    }
}
