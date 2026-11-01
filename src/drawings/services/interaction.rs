//! Drawing interaction service.
//!
//! Provides a self-contained handler for drawing interaction: hit testing
//! against all visible drawings (with timeframe filtering), handle discovery
//! and hit testing, handle drag lifecycle, and handle rendering.
//!
//! This service can be used standalone (without [`DrawingManager`](crate::drawings::DrawingManager))
//! for custom interaction implementations.

use crate::drawings::{Drawing, DrawingOptions, DrawingToolType, HandlePos};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

/// Self-contained interaction handler for chart drawings.
///
/// Manages selection state, handle dragging, and hit testing. Unlike
/// [`DrawingManager`](crate::drawings::DrawingManager) which coordinates all
/// services, `DrawingInteraction` focuses purely on pointer-based interaction.
pub struct DrawingInteraction {
    /// Currently selected drawing ID, or `None`.
    pub sel_drawing: Option<usize>,
    /// Handle currently being dragged: `(drawing_id, handle_position)`.
    pub dragging_handle: Option<(usize, HandlePos)>,
    /// Snapshot of the drawing state before the current drag began (for undo).
    drag_old_state: Option<Drawing>,
}

impl Default for DrawingInteraction {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingInteraction {
    /// Creates a new interaction handler with no selection and no active drag.
    pub fn new() -> Self {
        Self {
            sel_drawing: None,
            dragging_handle: None,
            drag_old_state: None,
        }
    }

    /// Clears the current drawing selection.
    pub fn deselect(&mut self) {
        self.sel_drawing = None;
    }

    /// Returns the selection handle positions for a drawing, determined by its
    /// tool type (see [`HandleService::get_handles`](super::HandleService::get_handles)
    /// for the canonical implementation).
    pub fn get_handles(&self, drawing: &Drawing) -> Vec<(HandlePos, Pos2)> {
        super::HandleService::get_handles(drawing)
    }

    /// Hit-tests a screen point against the handles of a drawing.
    ///
    /// Returns `Some(HandlePos)` if the point is within `handle_size * 1.5`
    /// pixels of a handle center, `None` otherwise.
    pub fn hit_test_handle(
        &self,
        point: Pos2,
        drawing: &Drawing,
        handle_size: f32,
    ) -> Option<HandlePos> {
        let handles = self.get_handles(drawing);
        let hit_distance = handle_size * 1.5;

        for (pos, handle_point) in handles {
            let dist =
                ((point.x - handle_point.x).powi(2) + (point.y - handle_point.y).powi(2)).sqrt();
            if dist <= hit_distance {
                return Some(pos);
            }
        }
        None
    }

    /// Begins a handle drag operation, saving the drawing's current state for undo.
    pub fn start_drag_handle(&mut self, drawing: &Drawing, handle: HandlePos) {
        self.drag_old_state = Some(drawing.clone());
        self.dragging_handle = Some((drawing.id, handle));
    }

    /// Takes the pre-drag drawing state for undo purposes. Consumes the stored
    /// snapshot, returning `None` on subsequent calls until a new drag begins.
    pub fn take_drag_old_state(&mut self) -> Option<Drawing> {
        self.drag_old_state.take()
    }

    /// Ends the handle drag operation, returning the pre-drag drawing state
    /// (for undo) and clearing the drag state.
    pub fn end_drag_handle(&mut self) -> Option<Drawing> {
        self.dragging_handle = None;
        self.drag_old_state.take()
    }

    /// Returns `true` if a handle drag is currently in progress.
    pub fn is_dragging(&self) -> bool {
        self.dragging_handle.is_some()
    }

    /// Returns the current drag target: `(drawing_id, handle_position)`, or
    /// `None` if no drag is active.
    pub fn get_drag_target(&self) -> Option<(usize, HandlePos)> {
        self.dragging_handle
    }

    /// Renders selection handles for a drawing as filled squares with borders.
    ///
    /// The actively dragged handle is highlighted with [`DrawingOptions::handle_sel_color`].
    pub fn render_handles(
        &self,
        painter: &egui::Painter,
        drawing: &Drawing,
        options: &DrawingOptions,
    ) {
        if !options.show_handles {
            return;
        }

        let handles = self.get_handles(drawing);
        let size = options.handle_size;

        for (pos, point) in handles {
            let is_dragging = self
                .dragging_handle
                .map(|(id, h)| id == drawing.id && h == pos)
                .unwrap_or(false);

            let color = if is_dragging {
                Color32::from_rgba_unmultiplied(
                    options.handle_sel_color[0],
                    options.handle_sel_color[1],
                    options.handle_sel_color[2],
                    options.handle_sel_color[3],
                )
            } else {
                Color32::from_rgba_unmultiplied(
                    options.handle_color[0],
                    options.handle_color[1],
                    options.handle_color[2],
                    options.handle_color[3],
                )
            };

            // Draw handle as filled square with border
            let rect = Rect::from_center_size(point, egui::vec2(size, size));
            painter.rect_filled(rect, 1.0, color);
            painter.rect_stroke(
                rect,
                1.0,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.extended.chart_bg,
                ),
                StrokeKind::Outside,
            );
        }
    }

    /// Hit-tests a screen point against all visible drawings, from the top layer down
    /// (descending z-order, and the later drawing first where the z-order is the same).
    ///
    /// Returns the ID of the topmost drawing at `point`, or `None` if no drawing
    /// was hit. Respects visibility and per-timeframe visibility filtering.
    /// Uses tool-specific hit testing (line distance, rectangle containment,
    /// Fibonacci level proximity, etc.) with a 5-pixel tolerance.
    pub fn hit_test(&self, point: Pos2, drawings: &[Drawing], timeframe: &str) -> Option<usize> {
        let hit_distance = 5.0;

        for drawing in super::in_z_order(drawings).into_iter().rev() {
            // Skip hidden drawings
            if !drawing.visible {
                continue;
            }

            // Skip drawings not visible on current timeframe
            if !drawing.timeframe_visibility.is_visible_on(timeframe) {
                continue;
            }

            match drawing.tool_type {
                DrawingToolType::HorizontalLine => {
                    if !drawing.points.is_empty()
                        && (point.y - drawing.points[0].y).abs() <= hit_distance
                    {
                        return Some(drawing.id);
                    }
                }
                DrawingToolType::VerticalLine => {
                    if !drawing.points.is_empty()
                        && (point.x - drawing.points[0].x).abs() <= hit_distance
                    {
                        return Some(drawing.id);
                    }
                }
                DrawingToolType::Rect => {
                    if drawing
                        .rectangle_bounds(Rect::EVERYTHING)
                        .is_some_and(|rect| rect.contains(point))
                    {
                        return Some(drawing.id);
                    }
                }
                // Fibonacci tools
                // The level lines the renderer draws, over the extent it draws them
                DrawingToolType::FibonacciRetracement | DrawingToolType::FibonacciExtension => {
                    if drawing.points.len() >= 2 {
                        let config = drawing.fibonacci_config();
                        let on_level = drawing
                            .fib_level_x_range(&config, Rect::EVERYTHING)
                            .contains(point.x)
                            && drawing.level_ys.iter().any(|y| {
                                (point.y - y).abs() <= hit_distance.max(config.level_width / 2.0)
                            });
                        let on_trend = drawing
                            .points
                            .windows(2)
                            .any(|p| point_to_line_distance(point, p[0], p[1]) <= hit_distance);
                        if on_level || on_trend {
                            return Some(drawing.id);
                        }
                    }
                }
                DrawingToolType::FibonacciChannel => {
                    // Extended levels are cut at a box around the anchors and the point, as below.
                    let reach = Rect::from_points(&drawing.points)
                        .union(Rect::from_pos(point))
                        .expand(2.0 * hit_distance);
                    if drawing
                        .fib_channel_level_lines(reach)
                        .is_some_and(|(_, lines)| {
                            lines
                                .iter()
                                .any(|&[a, b]| point_to_line_distance(point, a, b) <= hit_distance)
                        })
                    {
                        return Some(drawing.id);
                    }
                }
                _ => {
                    // Lines that run to the chart's edge are cut at a box around the anchors and
                    // the point, which holds every part of a line within reach of the point.
                    let reach = Rect::from_points(&drawing.points)
                        .union(Rect::from_pos(point))
                        .expand(2.0 * hit_distance);
                    if let Some(outline) = drawing.outline(reach) {
                        let tolerance = hit_distance.max(outline.width / 2.0);
                        let on_line = outline
                            .lines
                            .iter()
                            .any(|&[a, b]| point_to_line_distance(point, a, b) <= tolerance);
                        if on_line
                            || outline
                                .areas
                                .iter()
                                .any(|area| convex_contains(area, point))
                        {
                            return Some(drawing.id);
                        }
                        continue;
                    }
                    // The other tools: proximity to any anchor
                    for &p in &drawing.points {
                        let dist = ((point.x - p.x).powi(2) + (point.y - p.y).powi(2)).sqrt();
                        if dist <= hit_distance * 2.0 {
                            return Some(drawing.id);
                        }
                    }
                }
            }
        }
        None
    }

    /// Hit-tests the shapes painted for one drawing, including text extents and filled zones.
    ///
    /// Pass the clipped shapes emitted by the renderer, excluding selection handles. The
    /// manager records them during `render_all`; standalone hosts can retain their own paint
    /// commands. Uses a five-pixel tolerance along strokes and respects every shape's clip.
    pub fn hit_test_painted(point: Pos2, shapes: &[egui::epaint::ClippedShape]) -> bool {
        shapes
            .iter()
            .any(|s| s.clip_rect.contains(point) && hit_shape(point, &s.shape))
    }

    /// Hit-tests at `point` and selects the topmost drawing found.
    ///
    /// Returns `true` if a drawing was selected, `false` if the click hit empty
    /// space (which deselects the current selection).
    pub fn select_at(&mut self, point: Pos2, drawings: &[Drawing], timeframe: &str) -> bool {
        if let Some(id) = self.hit_test(point, drawings, timeframe) {
            self.sel_drawing = Some(id);
            true
        } else {
            self.sel_drawing = None;
            false
        }
    }
}

/// Selection follows the renderer's primitives, so text layout and data-driven geometry
/// need no second implementation in the interaction service.
fn hit_shape(point: Pos2, shape: &egui::Shape) -> bool {
    use egui::Shape;
    if !shape.visual_bounding_rect().expand(5.0).contains(point) {
        return false;
    }
    let line = |a, b, width: f32| point_to_line_distance(point, a, b) <= 5.0_f32.max(width / 2.0);
    let path = |points: &[Pos2], closed: bool, fill: bool, width: f32| {
        (fill && polygon_contains(points, point))
            || (width > 0.0
                && (points.windows(2).any(|p| line(p[0], p[1], width))
                    || (closed
                        && points.len() > 1
                        && line(points[points.len() - 1], points[0], width))))
    };
    match shape {
        Shape::Noop | Shape::Callback(_) | Shape::Band(_) => false,
        Shape::Vec(shapes) => shapes.iter().any(|s| hit_shape(point, s)),
        Shape::LineSegment { points, stroke } => {
            stroke.color.a() > 0 && stroke.width > 0.0 && line(points[0], points[1], stroke.width)
        }
        Shape::Path(p) => path(
            &p.points,
            p.closed,
            p.fill.a() > 0,
            if p.stroke.is_empty() {
                0.0
            } else {
                p.stroke.width
            },
        ),
        Shape::Circle(c) => {
            let distance = point.distance(c.center);
            (c.fill.a() > 0 && distance <= c.radius)
                || (!c.stroke.is_empty()
                    && (distance - c.radius).abs() <= 5.0_f32.max(c.stroke.width / 2.0))
        }
        Shape::Ellipse(e) => {
            let ring: Vec<_> = (0..64)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / 64.0;
                    e.center + e.radius * egui::vec2(angle.cos(), angle.sin())
                })
                .collect();
            path(
                &ring,
                true,
                e.fill.a() > 0,
                if e.stroke.is_empty() {
                    0.0
                } else {
                    e.stroke.width
                },
            )
        }
        Shape::Rect(r) => {
            let corners = [
                r.rect.left_top(),
                r.rect.right_top(),
                r.rect.right_bottom(),
                r.rect.left_bottom(),
            ];
            path(
                &corners,
                true,
                r.fill.a() > 0,
                if r.stroke.is_empty() {
                    0.0
                } else {
                    r.stroke.width
                },
            )
        }
        // A turned label is hit over its own box, not the larger upright box around it.
        Shape::Text(t) => t
            .galley
            .mesh_bounds
            .contains((egui::emath::Rot2::from_angle(-t.angle) * (point - t.pos)).to_pos2()),
        Shape::Mesh(m) => m.indices.as_chunks::<3>().0.iter().any(|tri| {
            let points = [
                m.vertices[tri[0] as usize].pos,
                m.vertices[tri[1] as usize].pos,
                m.vertices[tri[2] as usize].pos,
            ];
            polygon_contains(&points, point)
        }),
        Shape::QuadraticBezier(b) => hit_shape(point, &Shape::Path(b.to_path_shape(Some(0.5)))),
        Shape::CubicBezier(b) => b
            .to_path_shapes(Some(0.5), None)
            .iter()
            .any(|p| hit_shape(point, &Shape::Path(p.clone()))),
    }
}

/// Even-odd containment also handles concave paths. Edges are tested with stroke tolerance.
fn polygon_contains(points: &[Pos2], point: Pos2) -> bool {
    let mut inside = false;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        if (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y)
        {
            inside = !inside;
        }
    }
    inside
}

/// Whether `point` lies inside the convex polygon `area`, wound either way. An
/// area with no extent across has no inside: its edges are the only part hit.
fn convex_contains(area: &[Pos2], point: Pos2) -> bool {
    let sides = area.len();
    let crosses = (0..sides).map(|i| {
        let (edge, to_point) = (area[(i + 1) % sides] - area[i], point - area[i]);
        edge.x * to_point.y - edge.y * to_point.x
    });
    let (mut left, mut right) = (false, false);
    for cross in crosses {
        left |= cross > 0.0;
        right |= cross < 0.0;
    }
    sides >= 3 && (left || right) && !(left && right)
}

/// Calculates the perpendicular distance from `point` to the closest point on
/// the line segment `[line_start, line_end]`.
///
/// The projection is clamped to the segment endpoints. Returns the Euclidean
/// distance to `line_start` if the segment is degenerate (zero-length).
pub fn point_to_line_distance(point: Pos2, line_start: Pos2, line_end: Pos2) -> f32 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-6 {
        // Line is a point
        return ((point.x - line_start.x).powi(2) + (point.y - line_start.y).powi(2)).sqrt();
    }

    // Project point onto line
    let t = ((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let proj_x = line_start.x + t * dx;
    let proj_y = line_start.y + t * dy;

    ((point.x - proj_x).powi(2) + (point.y - proj_y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::ChartPoint;

    /// The glyph box is about [0, w] x [0, h] from the text's position. Turned 45 degrees clockwise,
    /// the upright box around it has its top-right corner at about (w c, 0), with c = cos 45. Turned
    /// back that is (w / 2, -w / 2), far above the glyphs, so the corner must not hit.
    #[test]
    fn turned_text_is_hit_over_its_own_box() {
        use egui::epaint::{ClippedShape, TextShape};
        let pos = Pos2::new(100., 100.);
        let hits = |shape: &TextShape, point: Pos2| {
            DrawingInteraction::hit_test_painted(
                point,
                &[ClippedShape::new(
                    Rect::EVERYTHING,
                    egui::Shape::Text(shape.clone()),
                )],
            )
        };
        egui::Context::default()
            .run_ui(Default::default(), |ui| {
                let galley = ui.painter().layout_no_wrap(
                    "WWWWWWWW".into(),
                    egui::FontId::proportional(20.0),
                    Color32::WHITE,
                );
                let bounds = galley.mesh_bounds;
                let upright = TextShape::new(pos, galley, Color32::WHITE);
                let turned = upright.clone().with_angle(std::f32::consts::FRAC_PI_4);
                let center =
                    egui::emath::Rot2::from_angle(turned.angle) * bounds.center().to_vec2();
                assert!(hits(&turned, pos + center));
                let corner = turned.visual_bounding_rect().right_top() + egui::vec2(-1., 1.);
                assert!(!hits(&turned, corner));
                assert!(hits(&upright, pos + bounds.center().to_vec2()));
                let past = Pos2::new(bounds.max.x + 1., bounds.center().y);
                assert!(!hits(&upright, past + pos.to_vec2()));
            })
            .drop_without_applying_deltas();
    }

    #[test]
    fn test_point_to_line_distance() {
        // Point on line
        let dist = point_to_line_distance(
            Pos2::new(50.0, 50.0),
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 100.0),
        );
        assert!(dist < 0.01);

        // Point perpendicular to line
        let dist = point_to_line_distance(
            Pos2::new(50.0, 60.0),
            Pos2::new(0.0, 50.0),
            Pos2::new(100.0, 50.0),
        );
        assert!((dist - 10.0).abs() < 0.01);
    }

    /// Anchors at y 400 then 100: the 23.6% level is at 100 + 300 * 0.236 = 170.8, or at
    /// 400 - 300 * 0.236 = 329.2 when reversed.
    #[test]
    fn fib_retracement_hit_follows_level_orientation() {
        let interaction = DrawingInteraction::new();
        let mut fib = Drawing::new(7, DrawingToolType::FibonacciRetracement);
        fib.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 400.0)];
        let hit = |fib: &mut Drawing, y| {
            fib.update_screen_coords(|bar| bar * 30.0, |price| 500.0 - price as f32);
            interaction.hit_test(Pos2::new(150.0, y), std::slice::from_ref(fib), "1D")
        };

        assert_eq!(hit(&mut fib, 170.8), Some(7));
        fib.fib_config = Some(crate::drawings::FibonacciConfig {
            reverse: true,
            ..Default::default()
        });
        assert_eq!(hit(&mut fib, 170.8), None);
        assert_eq!(hit(&mut fib, 329.2), Some(7));
    }

    /// Lines, labels, bands and selection share the configured price on either axis.
    #[test]
    fn fib_level_hits_follow_the_price_scale() {
        use crate::drawings::{FibonacciConfig, FibonacciLevel};
        let interaction = DrawingInteraction::new();
        for (tool, logarithmic, ratio, want) in [
            (DrawingToolType::FibonacciRetracement, false, 0.5, 505.0),
            (DrawingToolType::FibonacciRetracement, false, 0.99, 990.1),
            (DrawingToolType::FibonacciExtension, false, 0.618, 621.82),
            (
                DrawingToolType::FibonacciRetracement,
                true,
                0.236,
                29.64831389524342,
            ),
            (DrawingToolType::FibonacciRetracement, true, 0.5, 100.0),
            (
                DrawingToolType::FibonacciRetracement,
                true,
                0.618,
                172.1868574986006,
            ),
            (DrawingToolType::FibonacciExtension, true, 0.5, 100.0),
        ] {
            for log_scale in [false, true] {
                let price_to_y = |price: f64| {
                    if log_scale {
                        (500.0 * (2000f64 / price).ln() / (2000f64 / 5.0).ln()) as f32
                    } else {
                        (500.0 - price * 0.2) as f32
                    }
                };
                let mut fib = Drawing::new(7, tool);
                let (start, end) = if tool == DrawingToolType::FibonacciRetracement {
                    (1000.0, 10.0)
                } else {
                    (10.0, 1000.0)
                };
                fib.chart_points = vec![ChartPoint::new(0.0, start), ChartPoint::new(10.0, end)];
                // An unlabelled level must not suppress the nearby price label.
                let mut unlabelled = FibonacciLevel::new(ratio - 0.001, "", [33, 150, 243, 255]);
                unlabelled.show_price = false;
                fib.fib_config = Some(FibonacciConfig {
                    logarithmic,
                    levels_as_percent: true,
                    show_background: true,
                    levels: vec![
                        unlabelled,
                        FibonacciLevel::new(ratio, "level", [33, 150, 243, 255]),
                        FibonacciLevel::new(1.0, "end", [33, 150, 243, 255]),
                    ],
                    ..Default::default()
                });
                fib.update_screen_coords(|bar| bar * 30.0, price_to_y);
                let y = price_to_y(want);
                let hit =
                    interaction.hit_test(Pos2::new(150.0, y), std::slice::from_ref(&fib), "1D");
                assert_eq!(
                    hit,
                    Some(7),
                    "{tool:?}, log levels {logarithmic}, log scale {log_scale}"
                );
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 500.0));
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    let view = crate::drawings::rendering::ChartView {
                        price_formatter: None,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|b| b * 30.0,
                        price_to_y: &price_to_y,
                        y_to_price: &f64::from,
                        time_axis: true,
                        executions: None,
                    };
                    fib.render_fibonacci(ui.painter(), rect, &view);
                });
                output.textures_delta.clear();
                let line = output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::LineSegment { points, .. } if (points[0].y-y).abs() < 1e-3 && (points[1].y-y).abs() < 1e-3));
                let label = output.shapes.iter().any(|s| {
                    matches!(&s.shape,
                    egui::Shape::Text(t) if t.galley.text() == format!("level ({want:.2})")
                        && t.pos.y + t.galley.size().y < y)
                });
                let band = output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::Rect(r) if (r.rect.top()-y).abs() < 1e-3 || (r.rect.bottom()-y).abs() < 1e-3));
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(Rect::from_min_size(t.pos, t.galley.size())),
                        _ => None,
                    })
                    .collect();
                assert_eq!(labels.len(), 2, "nearby levels retain both labels");
                output.drop_without_applying_deltas();
                assert!(
                    line && label && band,
                    "{tool:?}, log levels {logarithmic}, log scale {log_scale}"
                );
            }
        }
    }

    /// Anchors at bars 0 and 10, prices 100 and 400, on x = 30 per bar and y = 500 - price: the
    /// first anchor is at (0, 400) and the second at (300, 100). A retracement level price is
    /// 400 - 300 * level, at y = 100 + 300 * level; with `extend_right` its lines run on past
    /// x = 300. The point (90, 310) selects the construction diagonal between the anchors,
    /// and 24.6 px from the nearest level (78.6% at 335.8 is 25.8 px away; 61.8% at 285.4).
    /// A channel with its third anchor at bar 5, price 150, (150, 350), translates by (150,-50):
    /// level 0.5 runs from (75,375) to (375,75), so y = 450 - x, and
    /// with `extend_right` on to x = 400 at y = 50. Its drawn levels cross x = 150 between
    /// y = 250 and 350, far from (150, 214.6), where the 61.8% level of a horizontal channel was.
    #[test]
    fn fib_drawings_are_hit_along_the_levels_they_draw() {
        use crate::drawings::FibonacciConfig;
        let interaction = DrawingInteraction::new();
        let extended = || {
            Some(FibonacciConfig {
                extend_right: true,
                ..Default::default()
            })
        };
        let cases = [
            (
                DrawingToolType::FibonacciRetracement,
                None,
                (450.0, 170.8),
                None,
            ),
            (
                DrawingToolType::FibonacciRetracement,
                extended(),
                (450.0, 170.8),
                Some(7),
            ),
            (
                DrawingToolType::FibonacciRetracement,
                None,
                (90.0, 310.0),
                Some(7),
            ),
            (
                DrawingToolType::FibonacciExtension,
                None,
                (90.0, 310.0),
                Some(7),
            ),
            (
                DrawingToolType::FibonacciChannel,
                None,
                (150.0, 300.0),
                Some(7),
            ),
            (
                DrawingToolType::FibonacciChannel,
                None,
                (150.0, 214.6),
                None,
            ),
            (
                DrawingToolType::FibonacciChannel,
                extended(),
                (400.0, 50.0),
                Some(7),
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, config, (x, y), want) in cases {
            let mut fib = Drawing::new(7, tool);
            fib.fib_config = config;
            fib.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 400.0)];
            if tool == DrawingToolType::FibonacciChannel {
                fib.chart_points.push(ChartPoint::new(5.0, 150.0));
            }
            fib.update_screen_coords(|bar| bar * 30.0, |price| 500.0 - price as f32);
            let extend = fib.fib_config.as_ref().is_some_and(|c| c.extend_right);
            let got = interaction.hit_test(Pos2::new(x, y), std::slice::from_ref(&fib), "1D");
            if got != want {
                wrong.push(format!(
                    "{tool:?} extend_right {extend} at ({x}, {y}): got {got:?}, want {want:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// A stored drawing has its chart anchors but no screen points until the next
    /// `update_screen_coords`; until then no tool is hit, and none panics.
    #[test]
    fn drawings_without_screen_points_are_not_hit() {
        let interaction = DrawingInteraction::new();
        let hit: Vec<_> = DrawingToolType::all()
            .iter()
            .filter(|&&tool| {
                let mut drawing = Drawing::new(7, tool);
                drawing.chart_points = (0..5)
                    .map(|i| ChartPoint::new(i as f32, 100.0 + f64::from(i)))
                    .collect();
                interaction
                    .hit_test(Pos2::new(1.0, 1.0), std::slice::from_ref(&drawing), "1D")
                    .is_some()
            })
            .collect();
        assert!(hit.is_empty(), "hit without screen points: {hit:?}");
    }

    /// Each point is at least 40 px from every anchor. A ray from (0, 0) through (100, 50) runs on
    /// through (300, 150) and back through (-100, -50) with both extensions on; one from
    /// (500, 100) through (400, 150) runs left through (200, 250). The cross line through
    /// (100, 100) runs down through (100, 300). The pattern's second leg runs from
    /// (100, 100) to (200, 0) through (150, 50). The triangle's edge from (200, 0) to (100, 200)
    /// passes (150, 100); without a fill its inside is empty. The circle around (100, 100)
    /// through (150, 100) passes (100, 150), and its centre is empty. The pitchfork's median
    /// leaves (0, 100) through the midpoint (100, 100) of the other anchors and runs on
    /// flat. The Gann fan's 1x2 line from (0, 200), half the 1x1 slope to (100, 100), is at
    /// y = 200 - 300 / 2 = 50 at x = 300. The measure fills its box, which holds (20, 80), 42 px
    /// from its diagonal. A measure across bars at one price, from (0, 100) to (100, 100), is hit
    /// 2 px off its border at (50, 102), but not 400 px past its end on the same line. A
    /// highlighter 16 px wide is hit within 8 px of its line, so a point 7 px off is on it and
    /// one 9 px off is not. The date range is a box between its anchors, so (150, 400) below it misses.
    /// The price range holds (120, 65), 15 px from its edges and 30 px from its centre.
    /// The ellipse in the box from (0, 0) to (200, 100) is lowest at (100, 100). Every drawing
    /// extends both ways where its tool can: the trend line from (100, 100) to (200, 150) runs on
    /// through (500, 300) and (0, 50). A head and shoulders on a neckline at y = 300 fills its
    /// head above it, which holds (300, 200), and not the gap between shoulder and head at
    /// (200, 200). The triangle pattern's boundaries y = x / 4 and y = 200 - x / 4 meet at
    /// (400, 100), so its fill holds (300, 100) beyond the anchors.
    #[test]
    fn drawings_are_hit_along_what_they_draw() {
        use DrawingToolType as T;
        let interaction = DrawingInteraction::new();
        // (tool, point, expected hit, anchors)
        let cases = [
            (
                T::Ray,
                (300.0, 150.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 50.0)],
            ),
            (
                T::Ray,
                (-100.0, -50.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 50.0)],
            ),
            (
                T::TrendLine,
                (500.0, 300.0),
                Some(7),
                vec![(100.0, 100.0), (200.0, 150.0)],
            ),
            (
                T::TrendLine,
                (0.0, 50.0),
                Some(7),
                vec![(100.0, 100.0), (200.0, 150.0)],
            ),
            (
                T::Ray,
                (200.0, 250.0),
                Some(7),
                vec![(500.0, 100.0), (400.0, 150.0)],
            ),
            (T::CrossLine, (100.0, 300.0), Some(7), vec![(100.0, 100.0)]),
            (
                T::XABCDPattern,
                (150.0, 50.0),
                Some(7),
                vec![
                    (0.0, 0.0),
                    (100.0, 100.0),
                    (200.0, 0.0),
                    (300.0, 100.0),
                    (400.0, 0.0),
                ],
            ),
            (
                T::HeadAndShoulders,
                (300.0, 200.0),
                Some(7),
                vec![
                    (0.0, 450.0),
                    (100.0, 150.0),
                    (200.0, 300.0),
                    (300.0, 0.0),
                    (400.0, 300.0),
                    (500.0, 150.0),
                    (600.0, 450.0),
                ],
            ),
            (
                T::HeadAndShoulders,
                (200.0, 200.0),
                None,
                vec![
                    (0.0, 450.0),
                    (100.0, 150.0),
                    (200.0, 300.0),
                    (300.0, 0.0),
                    (400.0, 300.0),
                    (500.0, 150.0),
                    (600.0, 450.0),
                ],
            ),
            (
                T::TrianglePattern,
                (300.0, 100.0),
                Some(7),
                vec![(0.0, 0.0), (0.0, 200.0), (200.0, 50.0), (200.0, 150.0)],
            ),
            (
                T::Triangle,
                (150.0, 100.0),
                Some(7),
                vec![(0.0, 0.0), (200.0, 0.0), (100.0, 200.0)],
            ),
            (
                T::Triangle,
                (100.0, 50.0),
                None,
                vec![(0.0, 0.0), (200.0, 0.0), (100.0, 200.0)],
            ),
            (
                T::Circle,
                (100.0, 150.0),
                Some(7),
                vec![(100.0, 100.0), (150.0, 100.0)],
            ),
            (
                T::Circle,
                (100.0, 100.0),
                None,
                vec![(100.0, 100.0), (150.0, 100.0)],
            ),
            (
                T::Pitchfork,
                (400.0, 100.0),
                Some(7),
                vec![(0.0, 100.0), (100.0, 50.0), (100.0, 150.0)],
            ),
            (
                T::GannFan,
                (300.0, 50.0),
                Some(7),
                vec![(0.0, 200.0), (100.0, 100.0)],
            ),
            (
                T::Measure,
                (20.0, 80.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 100.0)],
            ),
            (
                T::Measure,
                (50.0, 102.0),
                Some(7),
                vec![(0.0, 100.0), (100.0, 100.0)],
            ),
            (
                T::Measure,
                (500.0, 100.0),
                None,
                vec![(0.0, 100.0), (100.0, 100.0)],
            ),
            (
                T::Highlighter,
                (50.0, 7.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 0.0)],
            ),
            (
                T::Highlighter,
                (50.0, 9.0),
                None,
                vec![(0.0, 0.0), (100.0, 0.0)],
            ),
            (
                T::PriceRange,
                (120.0, 65.0),
                Some(7),
                vec![(100.0, 50.0), (200.0, 80.0)],
            ),
            (
                T::DateRange,
                (150.0, 400.0),
                None,
                vec![(100.0, 50.0), (200.0, 80.0)],
            ),
            (
                T::Ellipse,
                (100.0, 100.0),
                Some(7),
                vec![(0.0, 0.0), (200.0, 100.0)],
            ),
            (
                T::Ellipse,
                (100.0, 50.0),
                None,
                vec![(0.0, 0.0), (200.0, 100.0)],
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, (x, y), want, anchors) in cases {
            let mut drawing = Drawing::new(7, tool);
            // These shapes start filled; their cases test the outline alone.
            if matches!(tool, T::Triangle | T::Circle | T::Ellipse) {
                drawing.fill_color = None;
            }
            if tool == T::Highlighter {
                // A 16 px stroke: hit within 8 px.
                drawing.stroke_width = 16.0;
            }
            drawing.points = anchors.iter().map(|&(x, y)| Pos2::new(x, y)).collect();
            (drawing.extend_left, drawing.extend_right) = (true, true);
            let got = interaction.hit_test(Pos2::new(x, y), std::slice::from_ref(&drawing), "1D");
            if got != want {
                wrong.push(format!(
                    "{tool:?} at ({x}, {y}): got {got:?}, want {want:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn test_selection() {
        let mut interaction = DrawingInteraction::new();
        let mut drawing = Drawing::new(5, DrawingToolType::HorizontalLine);
        drawing.points = vec![Pos2::new(0.0, 100.0)];
        let drawings = [drawing];
        let hit = Pos2::new(50.0, 102.0);
        assert!(interaction.select_at(hit, &drawings, "1D"));
        assert_eq!(interaction.sel_drawing, Some(5));
        assert!(!interaction.select_at(Pos2::new(50.0, 150.0), &drawings, "1D"));
        assert_eq!(interaction.sel_drawing, None);
        assert!(interaction.select_at(hit, &drawings, "1D"));
        assert_eq!(interaction.sel_drawing, Some(5));
        interaction.deselect();
        assert_eq!(interaction.sel_drawing, None);
    }
}
