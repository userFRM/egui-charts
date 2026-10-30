//! The painted-geometry cache, painting and hit testing of the drawings.

use super::{DrawingManager, PaintedDrawing};
use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::drawings::rendering::ChartView;
use crate::drawings::services::{DrawingInteraction, HandleService, in_z_order};
use egui::epaint::{ClippedShape, Mesh, TessellationOptions, Tessellator, emath::TSTransform};
use egui::{Pos2, Rect, Shape};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Shapes tessellated at a scale and with options, as what to paint in place of each shape: a
/// run of geometry becomes one mesh in place of its first shape and nothing in place of the rest,
/// and text, circles and textured shapes, which point into texture atlases that can change, stay
/// as they are (`None`).
pub(super) struct Tessellated {
    pixels_per_point: f32,
    options: TessellationOptions,
    shapes: Vec<Option<Shape>>,
}

impl Tessellated {
    fn new(shapes: &[ClippedShape], pixels_per_point: f32, options: TessellationOptions) -> Self {
        // Neither the font atlas size nor its prepared discs are used without text and circles.
        let mut tessellator = Tessellator::new(pixels_per_point, options, [1, 1], Vec::new());
        let mut out: Vec<Option<Shape>> = vec![None; shapes.len()];
        let mut run: Option<(usize, Mesh)> = None;
        for (i, clipped) in shapes.iter().enumerate() {
            let plain = is_plain(&clipped.shape)
                && clipped.transform_after_tessellation == TSTransform::IDENTITY;
            if (!plain
                || run
                    .as_ref()
                    .is_some_and(|(first, _)| shapes[*first].clip_rect != clipped.clip_rect))
                && let Some((first, mesh)) = run.take()
            {
                out[first] = Some(Shape::mesh(mesh));
            }
            if !plain {
                continue;
            }
            let (_, mesh) = run.get_or_insert_with(|| (i, Mesh::default()));
            tessellator.set_clip_rect(clipped.clip_rect);
            tessellator.tessellate_shape(clipped.shape.clone(), mesh);
            out[i] = Some(Shape::Noop);
        }
        if let Some((first, mesh)) = run {
            out[first] = Some(Shape::mesh(mesh));
        }
        Self {
            pixels_per_point,
            options,
            shapes: out,
        }
    }
}

/// Whether `a` tessellates as `b` does: the same plain shapes clipped alike, and the other shapes,
/// which paint as they are, in the same places.
fn same_geometry(a: &[ClippedShape], b: &[ClippedShape]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| match (is_plain(&a.shape), is_plain(&b.shape)) {
                (true, true) => a == b,
                (plain_a, plain_b) => !plain_a && !plain_b,
            })
}

/// Whether the tessellation of `shape` depends only on the shape, the scale and the options.
fn is_plain(shape: &Shape) -> bool {
    match shape {
        Shape::Vec(shapes) => shapes.iter().all(is_plain),
        Shape::Rect(rect) => rect.fill_texture_id() == egui::TextureId::default(),
        Shape::Noop
        | Shape::Ellipse(_)
        | Shape::LineSegment { .. }
        | Shape::Path(_)
        | Shape::Band(_)
        | Shape::QuadraticBezier(_)
        | Shape::CubicBezier(_) => true,
        Shape::Circle(_) | Shape::Text(_) | Shape::Mesh(_) | Shape::Callback(_) => false,
    }
}

impl DrawingManager {
    /// Uses the fitted endpoints kept with the last paint for data-driven handles.
    pub(super) fn handle_drawing<'a>(&self, drawing: &'a Drawing) -> Cow<'a, Drawing> {
        if drawing.tool_type != DrawingToolType::RegressionTrend || drawing.custom_tool.is_some() {
            return Cow::Borrowed(drawing);
        }
        let painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        let ends = painted
            .iter()
            .find(|p| p.id == drawing.id && p.points == drawing.points)
            .and_then(|p| p.regression_ends);
        let mut projected = drawing.clone();
        projected.points = ends.map(Vec::from).unwrap_or_default();
        Cow::Owned(projected)
    }

    /// Renders selection handles for a drawing, highlighting the currently
    /// dragged handle (if any).
    pub fn render_handles(&self, painter: &egui::Painter, drawing: &Drawing) {
        let dragging = self
            .dragging_handle
            .filter(|(id, _)| *id == drawing.id)
            .map(|(_, h)| h);
        HandleService::render_handles(
            painter,
            &self.handle_drawing(drawing),
            &self.options.handle_config,
            dragging,
        );
    }

    // === Hit testing ===

    /// Hit-tests a screen point against all visible drawings (back-to-front).
    ///
    /// Returns the ID of the topmost drawing at `point`, or `None` if no
    /// drawing was hit. Delegates to [`DrawingInteraction::hit_test`], the single
    /// source of truth for per-segment / per-level geometry hit testing, so that
    /// channels, Fibonacci levels, fans, and pitchforks are selectable along
    /// their drawn geometry rather than only near a control point. Honors the
    /// active timeframe so drawings hidden on the current timeframe are not hit.
    /// After [`Self::render_all`], uses the actual clipped paint geometry, including text and
    /// data-driven bodies. Before the first paint, falls back to the tool's geometric outline.
    pub fn hit_test(&self, point: Pos2) -> Option<usize> {
        let painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        in_z_order(&self.drawings)
            .into_iter()
            .rev()
            .filter(|d| d.selectable && d.is_visible_on_timeframe(&self.curr_timeframe))
            .filter(|d| {
                self.pane_clips.is_empty()
                    || self
                        .pane_clips
                        .get(&d.pane)
                        .is_some_and(|rect| rect.contains(point))
            })
            .find(|drawing| {
                if let Some(id) = &drawing.custom_tool {
                    return self
                        .registry
                        .get(id)
                        .is_some_and(|tool| tool.implementation.hit_test(drawing, point, 6.0));
                }
                let paint = painted
                    .iter()
                    .find(|p| p.id == drawing.id && p.points == drawing.points);
                if paint.is_some_and(|p| !p.shapes.iter().any(|s| s.clip_rect.contains(point))) {
                    return false;
                }
                paint.is_some_and(|p| DrawingInteraction::hit_test_painted(point, &p.shapes))
                    || (drawing.tool_type != DrawingToolType::RegressionTrend
                        && DrawingInteraction::new()
                            .hit_test(point, std::slice::from_ref(*drawing), &self.curr_timeframe)
                            .is_some())
            })
            .map(|d| d.id)
    }

    fn render_drawing(
        &self,
        drawing: &Drawing,
        painter: &egui::Painter,
        rect: Rect,
        view: &ChartView,
    ) {
        if let Some(id) = &drawing.custom_tool {
            if let Some(tool) = self.registry.get(id) {
                tool.implementation.render(drawing, painter, rect, view);
            }
        } else {
            drawing.render_with_selection(
                painter,
                rect,
                view,
                self.selection.is_selected(drawing.id),
            );
        }
    }

    /// Render all drawings against the chart's bars and coordinate mapping
    pub fn render_all(&self, painter: &egui::Painter, price_rect: Rect, view: &ChartView) {
        let painter = &painter.with_clip_rect(price_rect);
        let mut painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        // The paints of the other panes' drawings stay; a set keeps this linear in the drawings.
        let others: HashSet<usize> = (self.drawings.iter())
            .filter(|d| !self.owns_pane(d))
            .map(|d| d.id)
            .collect();
        let mut last: HashMap<usize, PaintedDrawing> = HashMap::new();
        for paint in std::mem::take(&mut *painted) {
            if others.contains(&paint.id) {
                painted.push(paint);
            } else {
                last.insert(paint.id, paint);
            }
        }
        let pixels_per_point = painter.pixels_per_point();
        let options = painter.ctx().tessellation_options(|o| *o);
        // A transformed layer, such as an `egui::Scene`, has its shapes transformed before they
        // are tessellated, so a kept mesh would scale its feathering and rounding with the zoom.
        let untransformed = (painter.ctx())
            .layer_transform_to_global(painter.layer_id())
            .is_none();
        // Render completed drawings, bottom layer first
        for drawing in in_z_order(&self.drawings) {
            let off_pane = drawing.custom_tool.is_none()
                && drawing
                    .paint_bounds()
                    .is_some_and(|bounds| !bounds.intersects(price_rect));
            if self.owns_pane(drawing)
                && drawing.is_visible_on_timeframe(&self.curr_timeframe)
                && !off_pane
            {
                let start = painter.ctx().graphics(|g| {
                    g.get(painter.layer_id())
                        .map_or(0, |list| list.next_idx().0)
                });
                self.render_drawing(drawing, painter, price_rect, view);
                let shapes: Vec<ClippedShape> = painter.ctx().graphics(|g| {
                    g.get(painter.layer_id())
                        .map(|list| list.all_entries().skip(start).cloned().collect())
                        .unwrap_or_default()
                });
                // The same shapes as last frame paint last frame's tessellation, made on the
                // second frame so that a drawing changing every frame is not tessellated twice.
                let tessellated = last
                    .remove(&drawing.id)
                    .filter(|paint| untransformed && same_geometry(&paint.shapes, &shapes))
                    .map(|paint| {
                        paint
                            .tessellated
                            .filter(|t| {
                                t.pixels_per_point == pixels_per_point && t.options == options
                            })
                            .unwrap_or_else(|| Tessellated::new(&shapes, pixels_per_point, options))
                    });
                if let Some(tessellated) = &tessellated {
                    painter.ctx().graphics_mut(|g| {
                        let list = g.entry(painter.layer_id());
                        for (i, shape) in tessellated.shapes.iter().enumerate() {
                            if let Some(shape) = shape {
                                list.set(
                                    egui::layers::ShapeIdx(start + i),
                                    shapes[i].clip_rect,
                                    shape.clone(),
                                );
                            }
                        }
                    });
                }
                painted.push(PaintedDrawing {
                    id: drawing.id,
                    points: drawing.points.clone(),
                    regression_ends: (drawing.tool_type == DrawingToolType::RegressionTrend)
                        .then(|| drawing.regression_ends(view))
                        .flatten(),
                    shapes,
                    tessellated,
                });
            }
        }

        drop(painted);

        // Render current drawing being created
        if let Some(ref drawing) = self.curr_drawing
            && self.owns_pane(drawing)
        {
            self.render_drawing(drawing, painter, price_rect, view);
            if drawing.tool_type == DrawingToolType::Rect {
                drawing.render_rect_guides(painter);
                self.render_handles(painter, drawing);
            }
        }

        if let Some(drawing) = &self.measurement
            && self.owns_pane(drawing)
        {
            self.render_drawing(drawing, painter, price_rect, view);
        }

        // Render handles for every visible selected drawing.
        for drawing in self.drawings.iter().filter(|d| {
            d.selectable
                && self.selection.is_selected(d.id)
                && self.owns_pane(d)
                && d.is_visible_on_timeframe(&self.curr_timeframe)
        }) {
            if drawing.tool_type == DrawingToolType::Rect {
                drawing.render_rect_guides(painter);
            }
            self.render_handles(painter, drawing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::ChartPoint;

    /// Handles follow y = 11 + x for closes 10, 14, 12, regardless of clicked prices,
    /// anchor order or extension. Moving the range to closes 14, 12 gives y = 14 - 2x.
    #[test]
    fn regression_handles_follow_the_fit_and_refit_after_dragging() {
        use crate::drawings::{
            HandlePos,
            persistence::{drawing_to_stored, stored_to_drawing},
        };
        use crate::model::Bar;
        let bars: Vec<_> = [10.0, 14.0, 12.0]
            .into_iter()
            .map(|close| Bar::from_value(chrono::DateTime::UNIX_EPOCH, close))
            .collect();
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 500.0));
        let ctx = egui::Context::default();
        for (start, reverse) in [(0.0, false), (0.0, true), (-1.0, false), (0.4, true)] {
            let mut manager = DrawingManager::new();
            let mut drawing = Drawing::new(7, DrawingToolType::RegressionTrend);
            drawing.chart_points = vec![ChartPoint::new(start, 30.0), ChartPoint::new(2.0, 5.0)];
            if reverse {
                drawing.chart_points.reverse();
            }
            let saved = drawing_to_stored(&drawing, |b| 1_700_000_000_000 + (b * 60_000.0) as i64);
            let mut drawing =
                stored_to_drawing(&saved, |p| (p - 1_700_000_000_000) as f32 / 60_000.0, 7);
            assert_eq!(
                drawing
                    .chart_points
                    .iter()
                    .map(|p| p.bar_idx)
                    .collect::<Vec<_>>(),
                if reverse {
                    vec![2.0, start]
                } else {
                    vec![start, 2.0]
                }
            );
            drawing.extend_right = true;
            manager.drawings.push(drawing);
            manager.select(7);
            let render = |manager: &mut DrawingManager, bars: &[Bar]| {
                manager
                    .update_all_screen_coords(|b| 100.0 + b * 100.0, |p| 500.0 - p as f32 * 10.0);
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(pane),
                        ..Default::default()
                    },
                    |ui| {
                        manager.render_all(
                            ui.painter(),
                            pane,
                            &ChartView {
                                price_formatter: None,
                                instrument: None,
                                bars,
                                revision: 0,
                                bar_to_x: &|b| 100.0 + b * 100.0,
                                price_to_y: &|p| 500.0 - p as f32 * 10.0,
                                y_to_price: &|y| f64::from((500.0 - y) / 10.0),
                                time_axis: true,
                                executions: None,
                            },
                        );
                    },
                )
                .drop_without_applying_deltas();
            };
            render(&mut manager, &bars);
            let (first, last) = if reverse { (1, 0) } else { (0, 1) };
            let handles = manager.get_handles(&manager.drawings[0]);
            assert_eq!(handles.len(), 2);
            for point in &manager.drawings[0].points {
                assert_eq!(manager.hit_test_handle(&manager.drawings[0], *point), None);
                assert_eq!(manager.hit_test(*point), None);
            }
            for (index, point) in [
                (first, Pos2::new(100.0, 390.0)),
                (last, Pos2::new(300.0, 370.0)),
            ] {
                assert!(handles.contains(&(HandlePos::Point(index), point)));
                assert_eq!(
                    manager.hit_test_handle(&manager.drawings[0], point),
                    Some(HandlePos::Point(index))
                );
            }
            let before = manager.drawings[0].chart_points.clone();
            manager.start_drag_handle(7, HandlePos::Point(first));
            manager.update_drag_handle(
                Pos2::new(100.0, 50.0),
                |x| (x - 100.0) / 100.0,
                |y| f64::from(500.0 - y) / 10.0,
                |p, from, to| p - f64::from(to - from) / 10.0,
            );
            assert_eq!(
                manager.drawings[0].chart_points, before,
                "vertical drag is a no-op"
            );
            manager.update_drag_handle(
                Pos2::new(200.0, 50.0),
                |x| (x - 100.0) / 100.0,
                |y| f64::from(500.0 - y) / 10.0,
                |p, from, to| p - f64::from(to - from) / 10.0,
            );
            manager.end_drag_handle();
            render(&mut manager, &bars);
            let handles = manager.get_handles(&manager.drawings[0]);
            assert!(handles.contains(&(HandlePos::Point(first), Pos2::new(200.0, 360.0))));
            assert!(handles.contains(&(HandlePos::Point(last), Pos2::new(300.0, 380.0))));
            assert!(
                manager.drawings[0]
                    .chart_points
                    .iter()
                    .zip(before)
                    .all(|(p, old)| p.price == old.price)
            );
            assert!(manager.undo());
            render(&mut manager, &bars);
            assert_eq!(manager.get_handles(&manager.drawings[0]).len(), 2);
            let mut live = bars.clone();
            live[2].close = 18.0;
            render(&mut manager, &live);
            // Perfect line 10, 14, 18 after a tick: slope 4, intercept 10.
            assert!(
                manager
                    .get_handles(&manager.drawings[0])
                    .contains(&(HandlePos::Point(last), Pos2::new(300.0, 320.0)))
            );
            render(&mut manager, &[]);
            assert!(manager.get_handles(&manager.drawings[0]).is_empty());
        }
    }

    /// Text extents, position zones, Fibonacci rings and repeating cycle lines are
    /// selectable away from every anchor, using their actual paint geometry.
    #[test]
    fn painted_bodies_are_hit_away_from_anchors() {
        use egui::{Context, RawInput};
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        for (tool, anchors, hit, miss) in [
            (
                DrawingToolType::Sector,
                vec![(100., 100.), (200., 100.), (100., 150.)],
                (170.71068, 170.71068),
                (220., 220.),
            ),
            (
                DrawingToolType::Sector,
                vec![(100., 100.), (200., 100.), (100., 50.)],
                (150., 50.),
                (50., 150.),
            ),
            (
                DrawingToolType::ArrowMarkLeft,
                vec![(100., 100.)],
                (110., 100.),
                (80., 100.),
            ),
            (
                DrawingToolType::ArrowMarkRight,
                vec![(100., 100.)],
                (90., 100.),
                (120., 100.),
            ),
            (
                DrawingToolType::TrendLine,
                vec![(50., 100.), (550., 100.)],
                (200., 100.),
                (510., 100.),
            ),
            (
                DrawingToolType::TextLabel,
                vec![(50., 100.)],
                (180., 105.),
                (180., 160.),
            ),
            (
                DrawingToolType::LongPos,
                vec![(50., 150.), (250., 50.), (250., 250.)],
                (180., 100.),
                (350., 300.),
            ),
            (
                DrawingToolType::FibonacciCircles,
                vec![(100., 100.), (200., 200.)],
                (150., 200.),
                (580., 390.),
            ),
            (
                DrawingToolType::CyclicLines,
                vec![(50., 100.), (150., 100.)],
                (250., 300.),
                (480., 390.),
            ),
        ] {
            let ctx = Context::default();
            let mut manager = DrawingManager::new();
            let mut drawing = Drawing::new(7, tool);
            drawing.points = anchors.into_iter().map(|(x, y)| Pos2::new(x, y)).collect();
            drawing.chart_points = drawing
                .points
                .iter()
                .map(|p| ChartPoint::new(p.x / 10.0, 500.0 - p.y as f64))
                .collect();
            if tool == DrawingToolType::Sector {
                drawing.fill_color = Some([30, 150, 180, 60]);
            }
            drawing.text = Some("A long text annotation".into());
            manager.drawings.push(drawing);
            let render = |manager: &DrawingManager| {
                ctx.run_ui(RawInput::default(), |ui| {
                    let view = ChartView {
                        price_formatter: None,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|b| b * 10.0,
                        price_to_y: &|p| 500.0 - p as f32,
                        y_to_price: &|y| 500.0 - f64::from(y),
                        time_axis: true,
                        executions: None,
                    };
                    manager.render_all(ui.painter(), pane, &view);
                })
                .drop_without_applying_deltas();
            };
            render(&manager);
            assert_eq!(
                manager.hit_test(Pos2::new(hit.0, hit.1)),
                Some(7),
                "{tool:?}"
            );
            assert_eq!(
                manager.hit_test(Pos2::new(miss.0, miss.1)),
                None,
                "{tool:?}"
            );
            if tool == DrawingToolType::TextLabel {
                manager.edit_drawing(7, |d| d.text = Some("x".into()));
                assert_eq!(manager.hit_test(Pos2::new(hit.0, hit.1)), None);
            }
            manager.drawings[0].visible = false;
            assert_eq!(manager.hit_test(Pos2::new(hit.0, hit.1)), None);
            manager.delete_drawing(7);
            render(&manager);
            assert!(
                manager.painted.lock().unwrap().is_empty(),
                "deleted geometry is released"
            );
        }
    }

    /// Guards the tessellation cache: a drawing that paints the same shapes on consecutive frames
    /// paints last frame's tessellation, which must come out as the triangles of its shapes, and a
    /// moved drawing, or one in a layer that is then zoomed, must paint its shapes rather than the
    /// old mesh.
    #[test]
    fn cached_tessellation_matches_the_painted_shapes() {
        use egui::{Context, RawInput, epaint::Primitive};
        let ctx = Context::default();
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let mut manager = DrawingManager::new();
        let mut drawing = Drawing::new(7, DrawingToolType::ElliottImpulse);
        drawing.points = [
            (50., 300.),
            (100., 200.),
            (150., 250.),
            (250., 80.),
            (300., 150.),
            (350., 60.),
        ]
        .map(|(x, y)| Pos2::new(x, y))
        .to_vec();
        manager.drawings.push(drawing);
        let meshes = |shapes: Vec<ClippedShape>| -> Vec<(Rect, Mesh)> {
            (ctx.tessellate(shapes, 1.0).into_iter())
                .filter_map(|p| match p.primitive {
                    Primitive::Mesh(mesh) => Some((p.clip_rect, mesh)),
                    Primitive::Callback(_) => None,
                })
                .collect()
        };
        for frame in 0..5 {
            if frame == 3 {
                manager.drawings[0].points[3].y = 40.0;
            }
            let layer = if frame == 4 {
                TSTransform::from_scaling(2.0)
            } else {
                TSTransform::IDENTITY
            };
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                ctx.set_transform_layer(ui.layer_id(), layer);
                let view = ChartView {
                    price_formatter: None,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b * 10.0,
                    price_to_y: &|p| 500.0 - p as f32,
                    y_to_price: &|y| 500.0 - f64::from(y),
                    time_axis: true,
                    executions: None,
                };
                manager.render_all(ui.painter(), pane, &view);
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            if frame == 2 {
                assert!(
                    (shapes.iter()).any(|s| matches!(s.shape, Shape::Mesh(_))),
                    "an unchanged drawing paints its kept tessellation"
                );
            }
            let mut painted = manager.painted.lock().unwrap()[0].shapes.clone();
            painted.iter_mut().for_each(|s| s.transform(layer));
            assert!(meshes(shapes) == meshes(painted), "frame {frame}");
        }
    }
}
