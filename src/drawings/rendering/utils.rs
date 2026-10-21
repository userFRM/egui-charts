//! Shared rendering utilities.

use egui::{Pos2, Stroke};

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
    let clip = painter.clip_rect();
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
    let first = (from / period).floor() as i64;
    let last = (to / period).ceil() as i64;
    for k in first..last {
        let dash_start = (k as f64 * period).max(from);
        let dash_end = (k as f64 * period + f64::from(dash_len)).min(to);
        if dash_start < dash_end {
            painter.line_segment([at(dash_start), at(dash_end)], stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Context, LayerId, Painter, RawInput, Rect};

    /// An endpoint far off screen (a drawing anchored outside a collapsed
    /// price range) must not make the dash loop walk its full length.
    #[test]
    fn dashes_only_the_visible_part_of_a_huge_segment() {
        let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        let (dash, gap) = (6.0, 4.0);
        let ctx = Context::default();
        let output = ctx.run(RawInput::default(), |ctx| {
            let painter = Painter::new(ctx.clone(), LayerId::background(), clip);
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
        assert!(n > 0, "the visible part must still be drawn");
        assert!(
            n <= (clip.width() / (dash + gap)) as usize + 2,
            "{n} dashes for a 1000 px visible span"
        );
    }
}
