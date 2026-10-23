//! The dashed-line painter and the clip bound its loops use, shared by the chart and the drawing tools.

use egui::{Mesh, Pos2, Rect, Stroke, emath::GuiRounding};

/// The painter's clip rect, bounded by the viewport when it is unbounded, or `None` when neither is
/// finite.
///
/// The clip is layer-local and may lie outside the viewport (egui::Scene), so the viewport only
/// bounds an unbounded clip such as Rect::EVERYTHING, keeping loops over the visible area finite.
pub(crate) fn finite_clip(painter: &egui::Painter) -> Option<Rect> {
    let clip = painter.clip_rect();
    if clip.is_finite() {
        return Some(clip);
    }
    let clip = clip.intersect(painter.ctx().viewport_rect());
    clip.is_finite().then_some(clip)
}

/// Draw a dashed line between two arbitrary points.
///
/// Works for any angle. The segment is clipped to the painter's clip rect first and
/// only the visible part is dashed, so the cost is bounded by the visible length
/// however far off screen the endpoints are. The pattern stays anchored at `start`.
pub(crate) fn draw_dashed_line(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    draw_dashed_segment(painter, start, end, stroke, dash_len, gap_len, 0.0);
}

/// Draw a polyline with a continuous dash phase, clipping each segment before walking dashes.
pub(crate) fn draw_dashed_polyline(
    painter: &egui::Painter,
    points: &[Pos2],
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let mut phase = 0.0;
    let period = f64::from(dash_len) + f64::from(gap_len);
    for pair in points.windows(2) {
        draw_dashed_segment(painter, pair[0], pair[1], stroke, dash_len, gap_len, phase);
        let dx = f64::from(pair[1].x) - f64::from(pair[0].x);
        let dy = f64::from(pair[1].y) - f64::from(pair[0].y);
        phase = (phase + dx.hypot(dy)).rem_euclid(period);
    }
}

fn draw_dashed_segment(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
    phase: f64,
) {
    let Some(clip) = finite_clip(painter) else {
        return;
    };
    let (x0, y0) = (f64::from(start.x), f64::from(start.y));
    let (dx, dy) = (f64::from(end.x) - x0, f64::from(end.y) - y0);
    let total_len = dx.hypot(dy);
    let period = f64::from(dash_len) + f64::from(gap_len);
    if !total_len.is_finite()
        || total_len < 1.0
        || !period.is_finite()
        || dash_len <= 0.0
        || gap_len < 0.0
    {
        return;
    }

    // Liang-Barsky: narrow [t0, t1] to the part of the segment inside `clip`.
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, x0 - f64::from(clip.min.x)),
        (dx, f64::from(clip.max.x) - x0),
        (-dy, y0 - f64::from(clip.min.y)),
        (dy, f64::from(clip.max.y) - y0),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return;
            }
        } else if p < 0.0 {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
    }
    if t0 >= t1 {
        return;
    }

    let (from, to) = (t0 * total_len, t1 * total_len);
    let at = |d: f64| {
        let t = d / total_len;
        Pos2::new((x0 + dx * t) as f32, (y0 + dy * t) as f32)
    };
    // Integer dash indices covering [from, to]: at most visible_len / period + 2 steps.
    let first = ((from + phase) / period).floor() as i64;
    let last = ((to + phase) / period).ceil() as i64;
    let mut dots = Mesh::default();
    for k in first..last {
        let dash_start = (k as f64 * period - phase).max(from);
        let dash_end = (k as f64 * period - phase + f64::from(dash_len)).min(to);
        if dash_start < dash_end {
            let (start, end) = (at(dash_start), at(dash_end));
            if dash_len == 1.0 && stroke.width == 1.0 && (dx == 0.0 || dy == 0.0) {
                // A mesh keeps one-point dots square; thin rectangles are tessellated as lines.
                let half_width = if dx == 0.0 {
                    egui::vec2(stroke.width / 2.0, 0.0)
                } else {
                    egui::vec2(0.0, stroke.width / 2.0)
                };
                let rect = Rect::from_two_pos(start, end).expand2(half_width);
                let rect = Rect::from_min_size(
                    rect.min.round_to_pixels(painter.pixels_per_point()),
                    rect.size(),
                );
                dots.add_colored_rect(rect, stroke.color);
            } else {
                painter.line_segment([start, end], stroke);
            }
        }
    }
    if !dots.is_empty() {
        painter.add(dots);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Context, LayerId, Painter, RawInput, emath::TSTransform};

    /// An endpoint far off screen (a drawing anchored outside a collapsed
    /// price range) must not make the dash loop walk its full length, even
    /// when the painter's clip rect is unbounded, and must still draw when the
    /// clip lies outside the viewport in a transformed layer (an `egui::Scene`).
    #[test]
    fn dashes_only_the_visible_part_of_a_huge_segment() {
        let (dash, gap) = (6.0, 4.0);
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(2000.0, 1000.0));
        for (clip, layer_offset, visible_width) in [
            (
                Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1000.0)),
                0.0,
                1000.0,
            ),
            (Rect::EVERYTHING, 0.0, 2000.0),
            (
                Rect::from_min_size(Pos2::new(3000.0, 0.0), egui::vec2(1000.0, 1000.0)),
                -3000.0,
                1000.0,
            ),
        ] {
            let ctx = Context::default();
            let input = RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |_| {
                let layer = LayerId::background();
                let offset = TSTransform::from_translation(egui::vec2(layer_offset, 0.0));
                ctx.set_transform_layer(layer, offset);
                let painter = Painter::new(ctx.clone(), layer, clip);
                draw_dashed_line(
                    &painter,
                    Pos2::new(0.0, 500.0),
                    Pos2::new(1e14, 500.0),
                    Stroke::new(1.0_f32, Color32::WHITE),
                    dash,
                    gap,
                );
            });
            let n = output.shapes.len();
            output.drop_without_applying_deltas();
            assert!(n > 0, "the visible part must still be drawn");
            assert!(
                n <= (visible_width / (dash + gap)) as usize + 2,
                "{n} dashes for a {visible_width} px visible span"
            );
        }
    }
}
