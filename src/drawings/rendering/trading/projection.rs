//! Projection sector rendering.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::style::ellipse_arc;
use crate::drawings::validation::MAX_PROJECTION_LEVEL;
use crate::model::LineStyle;
use crate::painting::dashed_line::draw_dashed_polyline;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Align, Align2, Color32, Pos2, Rect, Shape, Stroke, Vec2};

impl Drawing {
    /// A circular sector with a radial gradient, independently styled level arcs and coefficients.
    pub(crate) fn render_projection_tool(&self, painter: &egui::Painter) {
        let config = self.projection_config();
        let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let trend = Stroke::new(self.stroke_width, self.color32());
        let clip = painter.clip_rect().expand(self.stroke_width + 2.0);
        let ppp = painter.pixels_per_point();
        let stroke = |mut points: Vec<Pos2>, stroke: Stroke, style: LineStyle, closed: bool| {
            if let Some((dash, gap)) = style.dash_pattern() {
                if closed && let Some(&first) = points.first() {
                    points.push(first);
                }
                draw_dashed_polyline(painter, &points, stroke, dash, gap);
            } else if closed {
                painter.add(Shape::closed_line(points, stroke));
            } else {
                painter.add(Shape::line(points, stroke));
            }
        };
        let Some((center, radius, range)) = self.sector_arc() else {
            // A partial or degenerate sector keeps its first radius while placing.
            if let &[a, b, ..] = self.points.as_slice()
                && a.is_finite()
                && b.is_finite()
            {
                if config.trend_visible {
                    stroke(vec![a, b], trend, config.trend_line_style, false);
                }
                if !self.completed && self.points.len() == 2 {
                    for point in [a, b] {
                        painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, self.color32());
                        painter.circle_stroke(
                            point,
                            DESIGN_TOKENS.rounding.lg,
                            Stroke::new(stroke::MEDIUM, Color32::WHITE),
                        );
                    }
                }
            }
            return;
        };
        let Some(sector) = self.sector_points(clip, ppp) else {
            return;
        };
        if config.show_background {
            let arc = projection_arc(center, radius, range, clip, ppp);
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(center, rgba(config.background_colors[0]));
            for &point in &arc {
                mesh.colored_vertex(point, rgba(config.background_colors[1]));
            }
            for i in 1..arc.len() {
                mesh.add_triangle(0, i as u32, (i + 1) as u32);
            }
            painter.add(Shape::mesh(mesh));
        }
        if config.trend_visible {
            stroke(sector, trend, config.trend_line_style, true);
        }
        let levels = config.levels.iter().filter(|level| {
            level.visible && level.coefficient > 0.0 && level.coefficient <= MAX_PROJECTION_LEVEL
        });
        for level in levels.clone() {
            let width = level.width.unwrap_or(self.stroke_width);
            stroke(
                projection_arc(
                    center,
                    radius * level.coefficient,
                    range,
                    painter.clip_rect().expand(width + 2.0),
                    ppp,
                ),
                Stroke::new(width, rgba(level.color)),
                level.line_style,
                false,
            );
        }
        if config.show_coefficients {
            let direction = Vec2::new(range[1].cos() as f32, range[1].sin() as f32);
            let align = |v: f32| {
                if v > 0.3 {
                    Align::Min
                } else if v < -0.3 {
                    Align::Max
                } else {
                    Align::Center
                }
            };
            for level in levels {
                let [r, g, b, _] = level.color;
                painter.text(
                    center + direction * (radius * level.coefficient + 6.0),
                    Align2([align(direction.x), align(direction.y)]),
                    format!("{}", level.coefficient),
                    egui::FontId::proportional(typography::XS),
                    Color32::from_rgb(r, g, b),
                );
            }
        }
    }

    pub(super) fn draw_projection_main_line(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        color: Color32,
    ) {
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));

        self.draw_projection_arrowhead(painter, start, end, color);
    }

    fn draw_projection_arrowhead(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        color: Color32,
    ) {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let angle = dy.atan2(dx);

        let arrow_size = 12.0;
        let arrow_angle = 0.4;
        let p1 = Pos2::new(
            end.x - arrow_size * (angle - arrow_angle).cos(),
            end.y - arrow_size * (angle - arrow_angle).sin(),
        );
        let p2 = Pos2::new(
            end.x - arrow_size * (angle + arrow_angle).cos(),
            end.y - arrow_size * (angle + arrow_angle).sin(),
        );
        painter.line_segment([end, p1], Stroke::new(stroke::THICK, color));
        painter.line_segment([end, p2], Stroke::new(stroke::THICK, color));
    }
}

/// Keeps a projection's radial fill accurate even when its arc is offscreen. Spans of at most
/// π/64 keep the fan's colour interpolation radial; visible arcs still refine to physical pixels.
fn projection_arc(center: Pos2, radius: f32, range: [f64; 2], clip: Rect, ppp: f32) -> Vec<Pos2> {
    let sweep = range[1] - range[0];
    let parts = (sweep.abs() / (std::f64::consts::PI / 64.0))
        .ceil()
        .max(1.0) as usize;
    let mut points = Vec::new();
    for i in 0..parts {
        let angles = [i, i + 1].map(|i| range[0] + sweep * i as f64 / parts as f64);
        let arc = ellipse_arc(center, Vec2::splat(radius), angles, clip, ppp);
        points.extend(arc.into_iter().skip(usize::from(i != 0)));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use egui::{Rect, Shape, epaint::ColorMode};
    use serde_json::{Value, json};

    #[test]
    fn projection_hits_follow_the_sector_and_painted_levels() {
        use crate::drawings::services::{hit_test, hit_test_painted};
        let mut drawing = Drawing::new(7, DrawingToolType::ProjectionTool);
        drawing.points = vec![
            egui::pos2(100.0, 100.0),
            egui::pos2(200.0, 100.0),
            egui::pos2(100.0, 200.0),
        ];
        let inside = egui::pos2(125.0, 125.0);
        let outside = egui::pos2(50.0, 150.0);
        assert_eq!(
            hit_test(inside, std::slice::from_ref(&drawing), "1m"),
            Some(7)
        );
        assert_eq!(
            hit_test(outside, std::slice::from_ref(&drawing), "1m"),
            None
        );

        let config = drawing.projection.as_mut().unwrap();
        config.trend_visible = false;
        config.show_background = false;
        config.show_coefficients = false;
        config.levels[0].coefficient = 0.5;
        let on_level = egui::pos2(135.35534, 135.35534);
        assert_eq!(
            hit_test(on_level, std::slice::from_ref(&drawing), "1m"),
            None
        );
        let ctx = egui::Context::default();
        for visible in [true, false] {
            drawing.projection.as_mut().unwrap().levels[0].visible = visible;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
                    ..Default::default()
                },
                |ui| drawing.render_projection_tool(ui.painter()),
            );
            assert_eq!(hit_test_painted(on_level, &output.shapes), visible);
            assert!(!hit_test_painted(inside, &output.shapes));
            assert!(!hit_test_painted(outside, &output.shapes));
            output.drop_without_applying_deltas();
        }
    }

    /// A wide level can enter the pane while its centreline is outside the sampling clip.
    #[test]
    fn wide_projection_levels_keep_detail_at_the_pane_edge() {
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for (center, coefficient, radius) in [
            (egui::pos2(552.2, 450.0), 4.0, 400.0),
            (egui::pos2(-23.5, 1100.0), 12.0, 1200.0),
        ] {
            let mut drawing = Drawing::new(1, DrawingToolType::ProjectionTool);
            drawing.points = vec![
                center,
                center + egui::vec2(100.0, 0.0),
                center - egui::vec2(0.0, 100.0),
            ];
            let level = &mut drawing.projection.as_mut().unwrap().levels[0];
            level.coefficient = coefficient;
            level.color = [200, 0, 0, 255];
            level.width = Some(100.0);
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(2.0);
            let mut paint_space = (Rect::NOTHING, 0.0);
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, pane.size() * 2.0)),
                    ..Default::default()
                },
                |ui| {
                    let painter = ui.painter().with_clip_rect(pane);
                    paint_space = (painter.clip_rect(), painter.pixels_per_point());
                    drawing.render_projection_tool(&painter);
                },
            );
            let points = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    Shape::Path(path)
                        if path.stroke
                            == Stroke::new(100.0, Color32::from_rgb(200, 0, 0)).into() =>
                    {
                        Some(path.points.clone())
                    }
                    _ => None,
                })
                .unwrap();
            output.drop_without_applying_deltas();
            assert_eq!(paint_space, (pane, 2.0));
            assert!(
                points
                    .iter()
                    .all(|p| (center.distance(*p) - radius).abs() < 0.01)
            );
            // A circular chord's sagitta is its radius minus its midpoint's distance to the centre.
            // A quarter physical pixel at 2x is 0.125 points, including the visible stroke's fringe.
            for edge in points.windows(2) {
                if Rect::from_two_pos(edge[0], edge[1])
                    .expand(50.0)
                    .intersects(pane)
                {
                    let error = radius - center.distance(edge[0].lerp(edge[1], 0.5));
                    assert!(
                        error <= 0.125,
                        "visible level chord has {error} points of error: {edge:?}"
                    );
                }
            }
        }
    }

    /// An offscreen outer arc still determines the radial colours visible inside the pane.
    #[test]
    fn projection_gradient_stays_radial_when_arc_is_offscreen() {
        let mut drawing = Drawing::new(1, DrawingToolType::ProjectionTool);
        drawing.points = vec![
            egui::pos2(400.0, 300.0),
            egui::pos2(1600.0, 300.0),
            egui::pos2(400.0, -900.0),
        ];
        let config = drawing.projection.as_mut().unwrap();
        config.levels.clear();
        config.trend_visible = false;
        config.background_colors = [[255, 0, 0, 255], [0, 0, 255, 255]];
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            },
            |ui| drawing.render_projection_tool(ui.painter()),
        );
        let mesh = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Mesh(mesh) => Some(mesh.clone()),
                _ => None,
            })
            .unwrap();
        output.drop_without_applying_deltas();
        // Radius 400 at -22.5 degrees is one third of the 1200 radius, hence blue = 255 / 3.
        let point = egui::pos2(769.5518, 146.9266);
        let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
        let blue = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .find_map(|triangle| {
                let [a, b, c] = triangle.map(|i| mesh.vertices[i as usize]);
                let area = cross(b.pos - a.pos, c.pos - a.pos);
                let wb = cross(point - a.pos, c.pos - a.pos) / area;
                let wc = cross(b.pos - a.pos, point - a.pos) / area;
                let wa = 1.0 - wb - wc;
                (wa >= 0.0 && wb >= 0.0 && wc >= 0.0).then_some(
                    wa * f32::from(a.color.b())
                        + wb * f32::from(b.color.b())
                        + wc * f32::from(c.color.b()),
                )
            })
            .unwrap();
        assert!(
            (blue - 85.0).abs() < 0.5,
            "blue at one third radius: {blue}"
        );
    }

    /// Radii, level arcs and the radial gradient follow the sector settings in screen space.
    #[test]
    fn projection_settings_change_the_painted_drawing() {
        const NEAR: [u8; 4] = [41, 98, 255, 51];
        const FAR: [u8; 4] = [156, 39, 176, 51];
        const CENTER: Pos2 = egui::pos2(400.0, 300.0);
        let blue = Color32::from_rgb(41, 98, 255);
        fn text(shapes: &[Shape], expected: &str) -> bool {
            shapes
                .iter()
                .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == expected))
        }
        fn arc(shapes: &[Shape], color: Color32, radius: f32, width: f32) -> bool {
            shapes.iter().any(|s| {
                matches!(s, Shape::Path(p)
                if !p.closed && p.stroke == Stroke::new(width, color).into()
                && p.points.len() > 2
                && p.points[0].distance(CENTER + egui::vec2(radius, 0.0)) < 0.01
                && p.points.last().unwrap().distance(CENTER - egui::vec2(0.0, radius)) < 0.01
                && p.points.iter().all(|p| (p.distance(CENTER) - radius).abs() < 0.01))
            })
        }
        fn has_stroke(shapes: &[Shape], color: Color32) -> bool {
            shapes.iter().any(|s| match s {
                Shape::Path(p) => p.stroke.color == ColorMode::Solid(color),
                Shape::LineSegment { stroke, .. } => stroke.color == color,
                _ => false,
            })
        }
        fn has_fill(shapes: &[Shape]) -> bool {
            shapes.iter().any(|s| matches!(s, Shape::Mesh(m) if m.vertices.iter().any(|v|
                [NEAR, FAR].map(|[r,g,b,a]| Color32::from_rgba_unmultiplied(r,g,b,a)).contains(&v.color))))
        }
        type Row = (&'static str, fn(&mut Value), fn(&[Shape]) -> bool);
        let rows: &[Row] = &[
            (
                "sector",
                |c| c["levels"][0]["line_style"] = json!("Solid"),
                |s| {
                    s.iter().any(|s| {
                        matches!(s, Shape::Path(p) if p.closed
                    && p.stroke == Stroke::new(2.0, Color32::from_rgb(41,98,255)).into()
                    && p.points[0] == CENTER
                    && p.points[1].distance(egui::pos2(500.0,300.0)) < 0.01
                    && p.points.last().unwrap().distance(egui::pos2(400.0,200.0)) < 0.01
                    && p.points[1..].iter().all(|p| (p.distance(CENTER)-100.0).abs() < 0.01))
                    }) && arc(
                        s,
                        Color32::from_rgba_unmultiplied(41, 98, 255, 150),
                        100.0,
                        2.0,
                    ) && text(s, "1")
                },
            ),
            (
                "half level",
                |c| {
                    c["levels"].as_array_mut().unwrap().push(json!({"coefficient":0.5,"visible":true,"color":[200,0,0,255],"line_style":"Solid","width":5.0}))
                },
                |s| arc(s, Color32::from_rgb(200, 0, 0), 50.0, 5.0) && text(s, "0.5"),
            ),
            (
                "gradient",
                |_| {},
                |s| {
                    s.iter().any(|s| {
                        matches!(s, Shape::Mesh(m)
                if m.vertices.len() > 3 && m.vertices[0].pos == CENTER
                && m.vertices[0].color == Color32::from_rgba_unmultiplied(41,98,255,51)
                && m.vertices[1..].iter().all(|v| (v.pos.distance(CENTER)-100.0).abs() < 0.01
                    && v.color == Color32::from_rgba_unmultiplied(156,39,176,51)))
                    })
                },
            ),
            (
                "hidden background",
                |c| c["show_background"] = json!(false),
                |s| !has_fill(s),
            ),
            (
                "hidden trend",
                |c| c["trend_visible"] = json!(false),
                |s| !has_stroke(s, Color32::from_rgb(41, 98, 255)),
            ),
            (
                "dotted trend",
                |c| c["trend_line_style"] = json!("Dotted"),
                |s| {
                    !s.iter().any(|s| matches!(s, Shape::Path(p) if p.closed && p.stroke.color == ColorMode::Solid(Color32::from_rgb(41,98,255))))
                    && s.iter().filter(|s| matches!(s, Shape::LineSegment {stroke,..} if stroke.color == Color32::from_rgb(41,98,255))).count() > 2
                },
            ),
            (
                "hidden coefficients",
                |c| c["show_coefficients"] = json!(false),
                |s| !text(s, "1"),
            ),
            (
                "hidden level",
                |c| c["levels"][0]["visible"] = json!(false),
                |s| {
                    !has_stroke(s, Color32::from_rgba_unmultiplied(41, 98, 255, 150))
                        && !text(s, "1")
                },
            ),
            (
                "invalid level",
                |c| c["levels"][0]["coefficient"] = json!(-0.5),
                |s| !has_stroke(s, Color32::from_rgba_unmultiplied(41, 98, 255, 150)),
            ),
            (
                "coefficient zero",
                |c| c["levels"][0]["coefficient"] = json!(0.0),
                |s| {
                    !has_stroke(s, Color32::from_rgba_unmultiplied(41, 98, 255, 150))
                        && !text(s, "0")
                },
            ),
        ];
        let mut failures = Vec::new();
        for &(name, edit, check) in rows {
            let mut config = json!({
                "levels":[{"coefficient":1.0,"visible":true,"color":[41,98,255,150],"line_style":"Dashed","width":null}],
                "show_background":true,"background_colors":[NEAR,FAR],"show_coefficients":true,
                "trend_visible":true,"trend_line_style":"Solid"
            });
            edit(&mut config);
            let mut drawing = Drawing::new(1, DrawingToolType::ProjectionTool);
            drawing.color = blue.to_srgba_unmultiplied();
            drawing.stroke_width = 2.0;
            drawing.completed = true;
            drawing.points = vec![CENTER, egui::pos2(500.0, 300.0), egui::pos2(400.0, 200.0)];
            drawing.projection = Some(serde_json::from_value(config).unwrap());
            let output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    drawing.render_projection_tool(ui.painter());
                },
            );
            let painted: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
            output.drop_without_applying_deltas();
            if !check(&painted) {
                failures.push(name);
            }
        }
        assert!(
            failures.is_empty(),
            "settings did not affect paint: {failures:?}"
        );
    }
}
