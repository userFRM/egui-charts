//! Common drawing styles applied to renderer primitives.

use crate::drawings::{ArrowStyle, Drawing, DrawingToolType, FontWeight};
use crate::model::LineStyle;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, epaint::ColorMode, layers::ShapeIdx};

impl Drawing {
    /// Size annotations before laying out both the text and its surrounding box.
    pub(super) fn annotation_font(&self) -> egui::FontId {
        egui::FontId::proportional(crate::styles::typography::font_size(self.font_size))
    }

    /// Apply line patterns and font weight without changing the renderer's widths or fills.
    pub(super) fn finish_style(&self, painter: &Painter, start: usize, rect: Rect) {
        let styled = self.fill_color.is_some()
            || self.line_style != LineStyle::Solid
            || self.font_weight == FontWeight::Bold
            || self.extend_left
            || self.extend_right;
        let clip = crate::painting::dashed_line::finite_clip(painter);
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            // Segments that continue one another become one path, which the tessellator joins
            // with half the vertices and a dash pattern runs along without restarting.
            let mut previous: Option<(ShapeIdx, Pos2, Stroke, Rect)> = None;
            for index in start..list.next_idx().0 {
                let mut segment = None;
                list.mutate_shape(ShapeIdx(index), |s| {
                    if styled
                        && matches!(
                            self.tool_type,
                            DrawingToolType::InfoLine
                                | DrawingToolType::TrendAngle
                                | DrawingToolType::Arrow
                                | DrawingToolType::ArrowMarker
                        )
                        && let Shape::LineSegment { points, .. } = &mut s.shape
                        && self.points.get(..2) == Some(points.as_slice())
                        && let Some((a, b)) = self.trend_line_ends(rect)
                    {
                        *points = [a, b];
                    }
                    if let Shape::LineSegment { points, stroke } = s.shape {
                        segment = Some((points, stroke, s.clip_rect));
                    }
                });
                if let Some((points, stroke, clip)) = segment {
                    if let Some((first, end, old_stroke, old_clip)) = previous
                        && end == points[0]
                        && old_stroke == stroke
                        && old_clip == clip
                    {
                        list.mutate_shape(first, |s| match &mut s.shape {
                            Shape::LineSegment { points: old, .. } => {
                                s.shape = Shape::line(vec![old[0], old[1], points[1]], stroke)
                            }
                            Shape::Path(p) => p.points.push(points[1]),
                            _ => {}
                        });
                        list.reset_shape(ShapeIdx(index));
                        previous = Some((first, points[1], stroke, clip));
                    } else {
                        previous = Some((ShapeIdx(index), points[1], stroke, clip));
                    }
                } else {
                    previous = None;
                }
            }
            if styled {
                for index in start..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(index), |s| self.style_shape(&mut s.shape, clip));
                }
            }
        });
        self.render_end_markers(painter, rect);
        let pixels_per_point = painter.pixels_per_point();
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            for index in start..list.next_idx().0 {
                list.mutate_shape(ShapeIdx(index), |s| {
                    fewer_circle_sides(&mut s.shape, pixels_per_point)
                });
            }
        });
    }

    /// Restyles one shape. Dashes are generated only inside `clip`, so a drawing reaching far off
    /// screen costs no more than its visible part; without a finite clip nothing is dashed.
    fn style_shape(&self, shape: &mut Shape, clip: Option<Rect>) {
        if let Shape::Vec(shapes) = shape {
            for s in shapes {
                self.style_shape(s, clip);
            }
            return;
        }
        if let Shape::Text(t) = shape {
            if self.font_weight == FontWeight::Bold {
                // Synthetic weight keeps host font families and fallback glyphs intact.
                let mut weighted = t.clone();
                weighted.pos.x += 0.6;
                *shape = Shape::Vec(vec![Shape::Text(t.clone()), Shape::Text(weighted)]);
            }
            return;
        }
        let fork = matches!(
            self.tool_type,
            DrawingToolType::Pitchfork
                | DrawingToolType::SchiffPitchfork
                | DrawingToolType::ModifiedSchiffPitchfork
                | DrawingToolType::InsidePitchfork
        );
        let custom_fork = fork && self.pitchfork.is_some();
        // Keep body fills of annotations and legacy forks; analysis labels keep their own fill.
        let fill = |color: &mut Color32| {
            if color.a() > 0
                && !custom_fork
                && matches!(
                    self.tool_type,
                    DrawingToolType::Pitchfork
                        | DrawingToolType::SchiffPitchfork
                        | DrawingToolType::ModifiedSchiffPitchfork
                        | DrawingToolType::InsidePitchfork
                        | DrawingToolType::TextLabel
                        | DrawingToolType::AnchoredText
                        | DrawingToolType::Note
                        | DrawingToolType::Callout
                        | DrawingToolType::PriceLabel
                        | DrawingToolType::FlagNote
                        | DrawingToolType::PriceNote
                        | DrawingToolType::Table
                        | DrawingToolType::Comment
                        | DrawingToolType::Signpost
                        | DrawingToolType::Image
                        | DrawingToolType::Tweet
                        | DrawingToolType::Idea
                        | DrawingToolType::Forecast
                        | DrawingToolType::ArrowMarkUp
                        | DrawingToolType::ArrowMarkDown
                        | DrawingToolType::ArrowMarkLeft
                        | DrawingToolType::ArrowMarkRight
                )
                && let Some(configured) = self.fill_color32()
            {
                *color = configured;
            }
        };
        let dash = if custom_fork || self.tool_type == DrawingToolType::RegressionTrend {
            None
        } else {
            self.line_style.dash_pattern()
        };
        let mut path = Vec::new();
        let stroke = match shape {
            Shape::LineSegment { points, stroke } => {
                path.extend(*points);
                *stroke
            }
            Shape::Path(p) => {
                fill(&mut p.fill);
                let ColorMode::Solid(color) = &p.stroke.color else {
                    return;
                };
                let stroke = Stroke::new(p.stroke.width, *color);
                if dash.is_none() || stroke.is_empty() {
                    return;
                }
                path = p.points.clone();
                if p.closed
                    && let Some(&first) = path.first()
                {
                    path.push(first);
                }
                p.stroke.width = 0.0;
                stroke
            }
            Shape::Rect(r) => {
                fill(&mut r.fill);
                if dash.is_none() || r.stroke.is_empty() {
                    return;
                }
                egui::epaint::tessellator::path::rounded_rectangle(
                    &mut path,
                    egui::epaint::RoundedRect::new(r.rect, r.corner_radius),
                );
                if let Some(&first) = path.first() {
                    path.push(first);
                }
                let stroke = r.stroke;
                r.stroke = Stroke::NONE;
                stroke
            }
            Shape::Circle(c) => {
                // Anchor decorations keep their thin white rings when the body is restyled.
                if c.stroke.color == Color32::WHITE
                    && c.radius <= crate::tokens::DESIGN_TOKENS.rounding.lg
                    && self.tool_type != DrawingToolType::Circle
                {
                    return;
                }

                if dash.is_none() || c.stroke.is_empty() {
                    return;
                }
                path = (0..=64)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 64.0;
                        c.center + c.radius * egui::vec2(angle.cos(), angle.sin())
                    })
                    .collect();
                let stroke = c.stroke;
                c.stroke = Stroke::NONE;
                stroke
            }
            Shape::Ellipse(e) => {
                if dash.is_none() || e.stroke.is_empty() {
                    return;
                }
                path = (0..=64)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 64.0;
                        e.center + e.radius * egui::vec2(angle.cos(), angle.sin())
                    })
                    .collect();
                let stroke = e.stroke;
                e.stroke = Stroke::NONE;
                stroke
            }
            _ => return,
        };
        if let Some((dash, gap)) = dash
            && !stroke.is_empty()
        {
            let mut shapes = clip.map_or_else(Vec::new, |clip| {
                // Widened by the stroke so an edge just outside the clip keeps its visible half.
                let clip = clip.expand(stroke.width);
                crate::painting::dashed_line::dashed_polyline_shapes(&path, stroke, dash, gap, clip)
            });
            if !matches!(shape, Shape::LineSegment { .. }) {
                shapes.insert(0, std::mem::replace(shape, Shape::Noop));
            }
            *shape = Shape::Vec(shapes);
        }
    }

    fn render_end_markers(&self, painter: &Painter, rect: Rect) {
        use DrawingToolType as T;
        if !self.arrow_start_style.is_visible() && !self.arrow_end_style.is_visible() {
            return;
        }
        let Some(&a) = self.points.first() else {
            return;
        };
        let b = self.points.get(1).copied().unwrap_or(a);
        let ends = match self.tool_type {
            T::TrendLine
            | T::InfoLine
            | T::TrendAngle
            | T::Ray
            | T::ExtendedLine
            | T::Arrow
            | T::ArrowMarker => self.trend_line_ends(rect).map(|(a, b)| [a, b]),
            T::HorizontalLine => Some([Pos2::new(rect.min.x, a.y), Pos2::new(rect.max.x, a.y)]),
            T::HorizontalRay => Some([a, Pos2::new(rect.max.x, a.y)]),
            T::VerticalLine => Some([Pos2::new(a.x, rect.min.y), Pos2::new(a.x, rect.max.y)]),
            T::Polyline | T::Path => self.points.last().map(|&b| [a, b]),
            _ => None,
        };
        if let Some(mut ends) = ends {
            if matches!(
                self.tool_type,
                T::TrendLine
                    | T::InfoLine
                    | T::TrendAngle
                    | T::ExtendedLine
                    | T::Ray
                    | T::Arrow
                    | T::ArrowMarker
            ) && (b.x < a.x || (self.tool_type == T::ExtendedLine && b.x == a.x && b.y < a.y))
            {
                ends.swap(0, 1);
            }
            let [start, end] = ends;
            let (start_direction, end_direction) =
                if matches!(self.tool_type, T::Polyline | T::Path) && self.points.len() > 2 {
                    (a - b, end - self.points[self.points.len() - 2])
                } else {
                    (start - end, end - start)
                };
            self.render_marker(painter, start, start_direction, self.arrow_start_style);
            self.render_marker(painter, end, end_direction, self.arrow_end_style);
        }
    }

    fn render_marker(
        &self,
        painter: &Painter,
        tip: Pos2,
        direction: egui::Vec2,
        style: ArrowStyle,
    ) {
        if direction.length_sq() < 1e-6 {
            return;
        }
        let color = self.color32();
        let size = (self.stroke_width * style.size_multiplier()).max(4.0);
        let along = direction.normalized() * size;
        let across = egui::vec2(-along.y, along.x) / 2.0;
        let base = tip - along;
        match style {
            ArrowStyle::None => {}
            ArrowStyle::Arrow => {
                painter.add(Shape::convex_polygon(
                    vec![tip, base + across, base - across],
                    color,
                    Stroke::NONE,
                ));
            }
            ArrowStyle::OpenArrow => {
                painter.add(Shape::line(
                    vec![base + across, tip, base - across],
                    self.stroke(),
                ));
            }
            ArrowStyle::Circle => {
                painter.circle_filled(tip, size / 2.0, color);
            }
            ArrowStyle::Square => {
                painter.rect_filled(
                    Rect::from_center_size(tip, egui::Vec2::splat(size)),
                    0.0,
                    color,
                );
            }
            ArrowStyle::Diamond => {
                painter.add(Shape::convex_polygon(
                    vec![
                        tip + along / 2.0,
                        tip + across,
                        tip - along / 2.0,
                        tip - across,
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// Replaces a circle with the polygon of the fewest sides within a quarter of a physical pixel of
/// it. The tessellator gives a circle 32 sides from a radius of 5 points and 64 from 18, which made
/// the anchor dots and rings of a drawing most of its vertices. A small disc without an outline
/// stays a circle, which the tessellator draws as one textured quad.
fn fewer_circle_sides(shape: &mut Shape, pixels_per_point: f32) {
    match shape {
        Shape::Vec(shapes) => {
            for shape in shapes {
                fewer_circle_sides(shape, pixels_per_point);
            }
        }
        Shape::Circle(c) if !c.stroke.is_empty() || c.radius * pixels_per_point > 6.0 => {
            let sides = circle_sides(c.radius * pixels_per_point);
            let points = (0..sides)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / sides as f32;
                    c.center + c.radius * egui::vec2(angle.cos(), angle.sin())
                })
                .collect();
            let mut path = egui::epaint::PathShape::convex_polygon(points, c.fill, c.stroke);
            path.stroke = path.stroke.outside();
            *shape = Shape::Path(path);
        }
        _ => {}
    }
}

/// The fewest sides of a polygon whose sides stay within a quarter of a physical pixel of a
/// circle of `radius` physical pixels, from 8 to 128.
pub(super) fn circle_sides(radius: f32) -> usize {
    let step = (1.0 - 0.25 / radius).clamp(-1.0, 1.0).acos();
    ((std::f32::consts::PI / step).ceil() as usize).clamp(8, 128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::StrokeKind;

    fn paint(drawing: &Drawing) -> Vec<Shape> {
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(pane),
                ..Default::default()
            },
            |ui| {
                drawing.render_original(
                    ui.painter(),
                    pane,
                    &crate::drawings::rendering::ChartView {
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|x| x,
                        price_to_y: &|p| p as f32,
                        time_axis: true,
                        executions: None,
                    },
                );
            },
        );
        fn collect(shape: &Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                _ => out.push(shape.clone()),
            }
        }
        let mut shapes = Vec::new();
        for s in &output.shapes {
            collect(&s.shape, &mut shapes);
        }
        output.drop_without_applying_deltas();
        shapes
    }

    fn pattern(tool: DrawingToolType) -> Drawing {
        let mut drawing = Drawing::new(1, tool);
        drawing.points = [
            (100., 300.),
            (180., 100.),
            (260., 250.),
            (340., 150.),
            (420., 220.),
            (500., 180.),
            (580., 300.),
        ]
        .map(|(x, y)| Pos2::new(x, y))
        .to_vec();
        drawing.points.truncate(tool.required_points().unwrap_or(7));
        drawing.chart_points = drawing
            .points
            .iter()
            .map(|p| crate::drawings::ChartPoint::new(p.x, f64::from(p.y)))
            .collect();
        drawing
    }

    #[test]
    fn main_widths_are_exact_and_details_stay_thin() {
        use DrawingToolType::*;
        for tool in [
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            ABCDPattern,
            TrianglePattern,
            ThreeDrivesPattern,
            ElliottImpulse,
            ElliottCorrection,
            ElliottTriangle,
            ElliottDoubleCombo,
            ElliottTripleCombo,
            CyclicLines,
            TimeCycles,
            SineLine,
            GannFan,
            GannBox,
            GannSquare,
            GannFixed,
        ] {
            for width in [1.0, 2.0, 3.0] {
                let mut drawing = pattern(tool);
                drawing.stroke_width = width;
                let shapes = paint(&drawing);
                let mut main = 0;
                let mut details = 0;
                for shape in &shapes {
                    let (is_main, stroke) = match shape {
                        Shape::LineSegment { points, stroke } => (
                            drawing.points.windows(2).any(|p| p == points)
                                || tool == CyclicLines
                                || (tool == HeadAndShoulders && stroke.color.a() == 200),
                            *stroke,
                        ),
                        Shape::Path(p) => {
                            let ColorMode::Solid(color) = p.stroke.color else {
                                continue;
                            };
                            (
                                p.points
                                    .windows(2)
                                    .any(|p| drawing.points.windows(2).any(|leg| leg == p))
                                    || (tool == SineLine && p.points.len() > 100)
                                    || (tool == TimeCycles
                                        && p.points.len() > 4
                                        && color == drawing.color32()),
                                Stroke::new(p.stroke.width, color),
                            )
                        }
                        Shape::Rect(r) => (
                            matches!(tool, GannBox | GannSquare | GannFixed)
                                && !r.stroke.is_empty(),
                            r.stroke,
                        ),
                        _ => continue,
                    };
                    if stroke.is_empty() {
                        continue;
                    }
                    if is_main {
                        main += 1;
                        assert_eq!(stroke.width, width, "{tool:?} main at {width}");
                    } else if stroke.color != Color32::WHITE
                        && (stroke.color.a() < 255
                            || matches!(shape, Shape::Path(p) if p.closed && p.points.len() >= 8))
                    {
                        details += 1;
                        assert!(
                            stroke.width <= 2.0,
                            "{tool:?} detail widened to {}",
                            stroke.width
                        );
                    }
                }
                assert!(main > 0, "{tool:?} missing main line");
                if matches!(
                    tool,
                    XABCDPattern
                        | GannBox
                        | GannSquare
                        | ElliottTriangle
                        | ElliottDoubleCombo
                        | ElliottTripleCombo
                ) {
                    assert!(details > 0, "{tool:?} missing details");
                }
            }
        }
    }

    #[test]
    fn pattern_fills_toggle_without_recolouring_labels() {
        use DrawingToolType::*;
        for tool in [
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            ABCDPattern,
            TrianglePattern,
            ThreeDrivesPattern,
            ElliottImpulse,
            ElliottCorrection,
            ElliottTriangle,
            GannBox,
            GannSquare,
            GannFixed,
        ] {
            let mut drawing = pattern(tool);
            if tool != ElliottImpulse {
                assert!(
                    drawing.fill_color.is_some(),
                    "{tool:?} lost its default tint"
                );
            }
            let coloured = Drawing::with_color(1, tool, [67, 89, 123, 255]);
            if tool != ElliottCorrection {
                assert_eq!(
                    &coloured.fill_color.unwrap()[..3],
                    &[67, 89, 123],
                    "{tool:?} default fill must follow its line colour"
                );
            }
            let configured = Color32::from_rgba_unmultiplied(12, 34, 56, 80);
            for fill in [None, Some([12, 34, 56, 80])] {
                drawing.fill_color = fill;
                let shapes = paint(&drawing);
                let mut bodies = 0;
                for shape in &shapes {
                    match shape {
                        Shape::Path(p) if p.closed && p.points.len() <= 7 => {
                            bodies += usize::from(p.fill.a() > 0);
                            assert_eq!(
                                p.fill,
                                fill.map_or(Color32::TRANSPARENT, |_| configured),
                                "{tool:?} body"
                            );
                        }
                        Shape::Rect(r) if r.fill.a() > 0 => {
                            if (matches!(tool, GannBox | GannSquare | GannFixed)
                                && r.rect.contains(drawing.points[0]))
                                || (tool == ThreeDrivesPattern
                                    && r.rect.left() == drawing.points[5].x)
                            {
                                bodies += 1;
                                assert_eq!(r.fill, configured, "{tool:?} body");
                            } else {
                                assert_ne!(r.fill, configured, "{tool:?} label recoloured");
                            }
                        }
                        _ => {}
                    }
                }
                assert_eq!(bodies > 0, fill.is_some(), "{tool:?} fill toggle");
            }
        }
    }

    /// Guards the circles of drawings at 1x and 2x: a circle with an outline, or too large for a
    /// textured quad, becomes a closed polygon whose corners lie on the circle and whose sides
    /// stay within a quarter of a physical pixel of it, with the outline still outside the
    /// radius; a small disc without an outline stays a circle.
    #[test]
    fn circles_become_polygons_within_a_quarter_pixel() {
        let center = Pos2::new(50.0, 40.0);
        let ring = Stroke::new(1.0, Color32::WHITE);
        for (radius, ppp, stroke, polygon) in [
            (3.0, 1.0, Stroke::NONE, false),
            (3.0, 2.0, Stroke::NONE, false),
            (3.0, 1.0, ring, true),
            (8.0, 1.0, Stroke::NONE, true),
            (8.0, 2.0, ring, true),
            (200.0, 2.0, ring, true),
        ] {
            let circle = egui::epaint::CircleShape {
                center,
                radius,
                fill: Color32::RED,
                stroke,
            };
            let mut shape = Shape::Circle(circle);
            fewer_circle_sides(&mut shape, ppp);
            let case = format!("radius {radius} at {ppp}x");
            let Shape::Path(path) = &shape else {
                assert!(!polygon, "{case}: still a circle");
                continue;
            };
            assert!(polygon, "{case}: a quad disc became a polygon");
            assert!(path.closed, "{case}");
            assert_eq!(
                (path.fill, path.stroke.kind),
                (Color32::RED, StrokeKind::Outside)
            );
            let corners = &path.points;
            for (i, a) in corners.iter().enumerate() {
                let b = corners[(i + 1) % corners.len()];
                assert!(((*a - center).length() - radius).abs() < 1e-3, "{case}");
                let error = (radius - (center.distance(a.lerp(b, 0.5)))) * ppp;
                assert!((0.0..=0.25).contains(&error), "{case}: {error} px");
            }
        }
    }

    /// Guards the frame cost of dashed drawings: a rect, a circle and an ellipse reaching a
    /// million points beyond an 800 x 600 pane are dashed only inside it, rather than with one
    /// shape per dash along their whole outline.
    #[test]
    fn far_reaching_dashed_outlines_are_dashed_only_inside_the_pane() {
        use crate::drawings::rendering::ChartView;
        use egui::{Context, RawInput};
        fn leaves(shape: &Shape) -> usize {
            match shape {
                Shape::Vec(shapes) => shapes.iter().map(leaves).sum(),
                _ => 1,
            }
        }
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for (tool, a, b) in [
            (DrawingToolType::Rect, (-1e6, 100.0), (1e6, 200.0)),
            (
                DrawingToolType::Circle,
                (400.0, 1e6 + 300.0),
                (400.0, 300.0),
            ),
            (DrawingToolType::Ellipse, (-1e6, 300.0), (1e6, 2e6 + 300.0)),
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.line_style = LineStyle::Dashed;
            drawing.points = vec![Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)];
            let ctx = Context::default();
            let input = RawInput {
                screen_rect: Some(pane),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                let view = ChartView {
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b,
                    price_to_y: &|p| p as f32,
                    time_axis: true,
                    executions: None,
                };
                drawing.render_original(ui.painter(), pane, &view);
            });
            let shapes: usize = output.shapes.iter().map(|s| leaves(&s.shape)).sum();
            output.drop_without_applying_deltas();
            assert!((1..1_000).contains(&shapes), "{tool:?}: {shapes} shapes");
        }
    }
}
