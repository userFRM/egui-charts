//! The dashed-line painter and the clip bound its loops use, shared by the chart and the drawing tools.

use egui::{Mesh, Pos2, Rect, Stroke, emath::GuiRounding};
use std::cell::RefCell;
use std::sync::Arc;

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
/// Works for any angle. The segment is clipped to the painter's clip rect grown by half the
/// stroke width first and only the visible part is dashed, so the cost is bounded by the visible
/// length however far off screen the endpoints are. The pattern stays anchored at `start`.
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

/// Draw equally spaced round dots, clipping before placement and carrying the phase across vertices.
pub(crate) fn draw_dotted_polyline(
    painter: &egui::Painter,
    points: &[Pos2],
    color: egui::Color32,
    spacing: f32,
    radius: f32,
) {
    let Some(clip) = finite_clip(painter) else {
        return;
    };
    if !spacing.is_finite() || spacing <= 0.0 || !radius.is_finite() || radius < 0.0 {
        return;
    }
    // A dot whose centre is outside the pane can still overlap its edge.
    let clip = clip.expand(radius);
    let spacing = f64::from(spacing);
    let mut phase = 0.0;
    for pair in points.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let (x0, y0) = (f64::from(start.x), f64::from(start.y));
        let (dx, dy) = (f64::from(end.x) - x0, f64::from(end.y) - y0);
        let length = dx.hypot(dy);
        if !length.is_finite() || length == 0.0 {
            continue;
        }
        if let Some((t0, t1)) = clip_segment(start, end, clip) {
            let first = ((t0 * length + phase) / spacing).ceil() as i64;
            let last = ((t1 * length + phase) / spacing).floor() as i64;
            for k in first..=last {
                let distance = k as f64 * spacing - phase;
                // The next segment owns a dot exactly on its first vertex.
                if distance < length {
                    let t = distance / length;
                    painter.circle_filled(
                        Pos2::new((x0 + dx * t) as f32, (y0 + dy * t) as f32),
                        radius,
                        color,
                    );
                }
            }
        }
        phase = (phase + length).rem_euclid(spacing);
    }
}

pub(crate) fn draw_dashed_segment(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
    phase: f64,
) {
    draw_dashed_segments(
        painter,
        std::iter::once((start, end, phase)),
        stroke,
        dash_len,
        gap_len,
    );
}

/// Draws square dots of side `stroke.width`, one every `spacing` along each segment from its
/// start, inside the painter's clip only, as one mesh: a dotted guide is one shape however long
/// it is.
#[cfg(feature = "drawings")]
pub(crate) fn draw_dotted_segments(
    painter: &egui::Painter,
    segments: &[[Pos2; 2]],
    stroke: Stroke,
    spacing: f32,
) {
    let size = stroke.width;
    let Some(clip) = finite_clip(painter) else {
        return;
    };
    if !(size > 0.0 && spacing >= size) {
        return;
    }
    // A dot whose centre is just outside the clip still shows its inner half.
    let clip = clip.expand(size / 2.0);
    let pixels_per_point = painter.pixels_per_point();
    let mut dots = Mesh::default();
    for &[start, end] in segments {
        visible_dashes(start, end, clip, size, spacing - size, 0.0, |a, b| {
            let rect = Rect::from_center_size(a.lerp(b, 0.5), egui::Vec2::splat(size));
            let rect = Rect::from_min_size(rect.min.round_to_pixels(pixels_per_point), rect.size());
            dots.add_colored_rect(rect, stroke.color);
        });
    }
    if !dots.is_empty() {
        painter.add(dots);
    }
}

/// Batch square dots into one mesh so tessellation does not repeatedly grow its output mesh.
pub(crate) fn draw_dashed_segments(
    painter: &egui::Painter,
    segments: impl IntoIterator<Item = (Pos2, Pos2, f64), IntoIter: Clone>,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let segments = segments.into_iter();
    let mut dots = Mesh::default();
    let mut key = None;
    if dash_len == 1.0
        && stroke.width == 1.0
        && gap_len.is_finite()
        && gap_len >= 0.0
        && let Some(clip) = finite_clip(painter)
    {
        // Every line of the batch is dotted into the mesh, so the mesh depends on these alone.
        key = segments
            .clone()
            .map(|(start, end, phase)| {
                (start.x == end.x || start.y == end.y).then_some((
                    [start.x, start.y, end.x, end.y].map(f32::to_bits),
                    phase.to_bits(),
                ))
            })
            .collect::<Option<Vec<_>>>()
            .map(|segments| DotsKey {
                clip: [clip.min.x, clip.min.y, clip.max.x, clip.max.y].map(f32::to_bits),
                pixels_per_point: painter.pixels_per_point().to_bits(),
                color: stroke.color,
                gap: gap_len.to_bits(),
                segments,
            });
        if let Some(mesh) = key.as_ref().and_then(|key| cached_dots(painter, key)) {
            painter.add(egui::Shape::Mesh(mesh));
            return;
        }
        let clip = clip.expand(stroke.width * 0.5);
        // Bound each clipped span before painting so batching never copies a growing mesh.
        let count: usize = segments
            .clone()
            .filter_map(|(start, end, _)| {
                if !start.is_finite() || !end.is_finite() || (start.x != end.x && start.y != end.y)
                {
                    return None;
                }
                let bounds = Rect::from_two_pos(start, end).intersect(clip);
                if bounds.is_negative() {
                    return None;
                }
                let length = (f64::from(bounds.right()) - f64::from(bounds.left()))
                    .max(f64::from(bounds.bottom()) - f64::from(bounds.top()));
                Some((length / (1.0 + f64::from(gap_len))).ceil() as usize + 2)
            })
            .sum();
        dots.reserve_vertices(count * 4);
        dots.reserve_triangles(count * 2);
    }
    let mut spans = DotSpans::default();
    for (start, end, phase) in segments {
        append_dashed_segment(
            painter, &mut dots, &mut spans, start, end, stroke, dash_len, gap_len, phase,
        );
    }
    if !dots.is_empty() {
        let mesh = Arc::new(dots);
        if let Some(key) = key {
            let pass = painter.ctx().cumulative_pass_nr();
            DOTS.with_borrow_mut(|cache| cache.push((pass, key, mesh.clone())));
        }
        painter.add(egui::Shape::Mesh(mesh));
    }
}

/// What a batch of dotted lines is built from, compared by bits: the painter's clip, scale and
/// the colour, gap and lines.
#[derive(PartialEq)]
struct DotsKey {
    clip: [u32; 4],
    pixels_per_point: u32,
    color: egui::Color32,
    gap: u32,
    segments: Vec<([u32; 4], u64)>,
}

thread_local! {
    /// The dotted meshes of the last two passes with what they were built from: a chart at rest
    /// paints the same grid every pass, and laying out its dots costs more than painting them.
    static DOTS: RefCell<Vec<(u64, DotsKey, Arc<Mesh>)>> = const { RefCell::new(Vec::new()) };
}

/// The mesh built from `key` in this pass or the last, kept for the next pass.
fn cached_dots(painter: &egui::Painter, key: &DotsKey) -> Option<Arc<Mesh>> {
    let pass = painter.ctx().cumulative_pass_nr();
    DOTS.with_borrow_mut(|cache| {
        cache.retain(|(used, _, _)| used.saturating_add(1) >= pass);
        let (used, _, mesh) = cache.iter_mut().find(|(_, cached, _)| cached == key)?;
        *used = pass;
        Some(mesh.clone())
    })
}

/// The dots of the last axis-aligned dotted line along its axis, as pixel-snapped starts and
/// lengths, with the axis, ends and phase they were laid out for. Parallel lines with the same
/// ends and phase, such as grid lines, have the same dots and lay them out once.
#[derive(Default)]
struct DotSpans {
    key: Option<(bool, f32, f32, f64)>,
    spans: Vec<(f32, f32)>,
}

fn append_dashed_segment(
    painter: &egui::Painter,
    dots: &mut Mesh,
    spans: &mut DotSpans,
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
    let clip = clip.expand(stroke.width * 0.5);
    if dash_len != 1.0 || stroke.width != 1.0 || (start.x != end.x && start.y != end.y) {
        // One paint list lock for all dashes, not one per dash.
        let mut dashes = Vec::new();
        visible_dashes(start, end, clip, dash_len, gap_len, phase, |a, b| {
            dashes.push(egui::Shape::line_segment([a, b], stroke));
        });
        painter.extend(dashes);
        return;
    }
    // A mesh keeps one-point dots square; thin rectangles are tessellated as lines. Callers
    // reserve the mesh for the clipped spans, so adding dots never copies growing buffers.
    let vertical = start.x == end.x;
    let (across, across_clip) = if vertical {
        (start.x, clip.x_range())
    } else {
        (start.y, clip.y_range())
    };
    // Where along the axis the dots fall does not depend on where across the clip the line is.
    if !across_clip.contains(across) {
        return;
    }
    let pixels_per_point = painter.pixels_per_point();
    let key = if vertical {
        (true, start.y, end.y, phase)
    } else {
        (false, start.x, end.x, phase)
    };
    if spans.key != Some(key) {
        spans.key = Some(key);
        spans.spans.clear();
        visible_dashes(start, end, clip, dash_len, gap_len, phase, |a, b| {
            let (a, b) = if vertical { (a.y, b.y) } else { (a.x, b.x) };
            let (min, max) = (a.min(b), a.max(b));
            spans.spans.push((
                (min * pixels_per_point).round() / pixels_per_point,
                max - min,
            ));
        });
    }
    let edge = ((across - 0.5) * pixels_per_point).round() / pixels_per_point;
    let width = (across + 0.5) - (across - 0.5);
    for &(min, len) in &spans.spans {
        let rect = if vertical {
            Rect::from_min_size(Pos2::new(edge, min), egui::vec2(width, len))
        } else {
            Rect::from_min_size(Pos2::new(min, edge), egui::vec2(len, width))
        };
        dots.add_colored_rect(rect, stroke.color);
    }
}

/// The dashes of a polyline whose strokes overlap `clip`, with the dash phase carried
/// across its vertices, for restyling shapes that have no painter.
///
/// Only the visible part of each segment is dashed, so the cost is bounded by the visible length
/// however far off screen the vertices are. The pattern stays anchored at the first point.
#[cfg(feature = "drawings")]
pub(crate) fn dashed_polyline_shapes(
    points: &[Pos2],
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
    clip: Rect,
) -> Vec<egui::Shape> {
    let clip = clip.expand(stroke.width * 0.5);
    let mut shapes = Vec::new();
    let mut phase = 0.0;
    let period = f64::from(dash_len) + f64::from(gap_len);
    for pair in points.windows(2) {
        visible_dashes(pair[0], pair[1], clip, dash_len, gap_len, phase, |a, b| {
            shapes.push(egui::Shape::line_segment([a, b], stroke));
        });
        let dx = f64::from(pair[1].x) - f64::from(pair[0].x);
        let dy = f64::from(pair[1].y) - f64::from(pair[0].y);
        phase = (phase + dx.hypot(dy)).rem_euclid(period);
    }
    shapes
}

/// Calls `dash` with the ends of each dash of `start..end` inside `clip`, the pattern offset by
/// `phase` along the segment. Work is bounded by the visible length.
fn visible_dashes(
    start: Pos2,
    end: Pos2,
    clip: Rect,
    dash_len: f32,
    gap_len: f32,
    phase: f64,
    mut dash: impl FnMut(Pos2, Pos2),
) {
    let (x0, y0) = (f64::from(start.x), f64::from(start.y));
    let (dx, dy) = (f64::from(end.x) - x0, f64::from(end.y) - y0);
    let total_len = dx.hypot(dy);
    let period = f64::from(dash_len) + f64::from(gap_len);
    if !total_len.is_finite()
        || total_len == 0.0
        || !period.is_finite()
        || dash_len <= 0.0
        || gap_len < 0.0
    {
        return;
    }

    let Some((t0, t1)) = clip_segment(start, end, clip) else {
        return;
    };
    let (from, to) = (t0 * total_len, t1 * total_len);
    let at = |d: f64| {
        let t = d / total_len;
        Pos2::new((x0 + dx * t) as f32, (y0 + dy * t) as f32)
    };
    // Integer dash indices covering [from, to]: at most visible_len / period + 2 steps.
    let first = ((from + phase) / period).floor() as i64;
    let last = ((to + phase) / period).ceil() as i64;
    for k in first..last {
        let dash_start = (k as f64 * period - phase).max(from);
        let dash_end = (k as f64 * period - phase + f64::from(dash_len)).min(to);
        if dash_start < dash_end {
            dash(at(dash_start), at(dash_end));
        }
    }
}

/// Liang-Barsky: the parameter interval of the segment inside `clip`.
fn clip_segment(start: Pos2, end: Pos2, clip: Rect) -> Option<(f64, f64)> {
    let (x0, y0) = (f64::from(start.x), f64::from(start.y));
    let (dx, dy) = (f64::from(end.x) - x0, f64::from(end.y) - y0);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, x0 - f64::from(clip.min.x)),
        (dx, f64::from(clip.max.x) - x0),
        (-dy, y0 - f64::from(clip.min.y)),
        (dy, f64::from(clip.max.y) - y0),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Context, LayerId, Painter, RawInput, emath::TSTransform};

    #[test]
    fn batched_dots_preserve_individual_mesh_geometry() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 80.0));
        // Parallel lines with the first one's ends share its dots only at its phase.
        let segments = [
            (Pos2::new(-50.0, 10.25), Pos2::new(150.0, 10.25), 0.0),
            (Pos2::new(f32::NAN, 0.0), Pos2::new(0.0, 0.0), 0.0),
            (Pos2::new(30.25, -10.0), Pos2::new(30.25, 100.0), 2.5),
            (Pos2::new(1e14, 40.0), Pos2::new(-1e14, 40.0), 1.0),
            (Pos2::new(0.0, 90.0), Pos2::new(100.0, 90.0), 0.0),
            (Pos2::new(-50.0, 30.75), Pos2::new(150.0, 30.75), 0.0),
            (Pos2::new(-50.0, 50.5), Pos2::new(150.0, 50.5), 1.0),
        ];
        // Batches repeat across scales, gaps and colours, so a kept mesh must match them all.
        for color in [Color32::from_white_alpha(80), Color32::RED] {
            for scale in [1.0, 1.25, 2.0] {
                for gap in [0.0, 3.0, 7.0] {
                    let render = |batch| {
                        let ctx = Context::default();
                        ctx.set_pixels_per_point(scale);
                        let output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(rect),
                                ..Default::default()
                            },
                            |_| {
                                let painter =
                                    Painter::new(ctx.clone(), LayerId::background(), rect);
                                let stroke = Stroke::new(1.0, color);
                                if batch {
                                    draw_dashed_segments(&painter, segments, stroke, 1.0, gap);
                                } else {
                                    for (start, end, phase) in segments {
                                        draw_dashed_segment(
                                            &painter, start, end, stroke, 1.0, gap, phase,
                                        );
                                    }
                                }
                            },
                        );
                        let mut merged = Mesh::default();
                        let mut count = 0;
                        for shape in &output.shapes {
                            if let egui::Shape::Mesh(mesh) = &shape.shape {
                                assert_eq!(shape.clip_rect, rect);
                                merged.append_ref(mesh);
                                count += 1;
                            }
                        }
                        output.drop_without_applying_deltas();
                        (merged, count)
                    };
                    let (batched, count) = render(true);
                    // Another thread lays the reference out without kept meshes.
                    let (individual, individual_count) =
                        std::thread::scope(|scope| scope.spawn(|| render(false)).join().unwrap());
                    assert_eq!(count, 1);
                    assert_eq!(individual_count, 5);
                    assert_eq!(batched.vertices, individual.vertices);
                    assert_eq!(batched.indices, individual.indices);
                }
            }
        }
    }

    #[test]
    fn dashes_keep_strokes_overlapping_the_clip_edge() {
        let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let stroke = Stroke::new(4.0, Color32::WHITE);
        for offset in [1.5, 2.5] {
            for points in [
                [Pos2::new(10.0, -offset), Pos2::new(26.0, -offset)],
                [
                    Pos2::new(10.0, 100.0 + offset),
                    Pos2::new(26.0, 100.0 + offset),
                ],
                [Pos2::new(-offset, 10.0), Pos2::new(-offset, 26.0)],
                [
                    Pos2::new(100.0 + offset, 10.0),
                    Pos2::new(100.0 + offset, 26.0),
                ],
            ] {
                // A four-point stroke reaches two points from its centre; the period is ten.
                let expected = if offset < 2.0 {
                    let direction = (points[1] - points[0]).normalized();
                    vec![
                        egui::Shape::line_segment([points[0], points[0] + direction * 6.0], stroke),
                        egui::Shape::line_segment(
                            [points[0] + direction * 10.0, points[1]],
                            stroke,
                        ),
                    ]
                } else {
                    Vec::new()
                };
                let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                    draw_dashed_polyline(
                        &ui.painter().with_clip_rect(clip),
                        &points,
                        stroke,
                        6.0,
                        4.0,
                    );
                });
                let painted: Vec<_> = std::mem::take(&mut output.shapes)
                    .into_iter()
                    .map(|shape| {
                        assert_eq!(shape.clip_rect, clip);
                        shape.shape
                    })
                    .collect();
                output.drop_without_applying_deltas();
                assert_eq!(painted, expected, "stroke at {points:?}");
                #[cfg(feature = "drawings")]
                assert_eq!(
                    dashed_polyline_shapes(&points, stroke, 6.0, 4.0, clip),
                    expected
                );
            }
        }
    }

    /// All the segments' dots are one shape, and a segment reaching far off screen walks only its
    /// visible part: 142.13 px of the diagonal, 36 dots at spacing 4, in the clip grown by half a
    /// dot, and all 100 px of the horizontal, 25 dots.
    #[cfg(feature = "drawings")]
    #[test]
    fn dotted_segments_are_one_mesh_inside_the_clip() {
        let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let segments = [
            [Pos2::ZERO, Pos2::new(1e14, 1e14)],
            [Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0)],
        ];
        let paint = |width: f32| {
            let output = Context::default().run_ui(RawInput::default(), |ui| {
                draw_dotted_segments(
                    &ui.painter().with_clip_rect(clip),
                    &segments,
                    Stroke::new(width, Color32::WHITE),
                    4.0,
                );
            });
            let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
            output.drop_without_applying_deltas();
            shapes
        };
        let shapes = paint(1.0);
        let [egui::Shape::Mesh(mesh)] = shapes.as_slice() else {
            panic!("the dots are not one mesh: {} shapes", shapes.len());
        };
        assert_eq!(mesh.vertices.len(), 4 * 61);
        let reach = Rect::from_min_max(Pos2::new(-1.0, -1.0), Pos2::new(101.0, 101.0));
        assert!(mesh.vertices.iter().all(|v| reach.contains(v.pos)));
        assert!(paint(0.0).is_empty());
    }

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
