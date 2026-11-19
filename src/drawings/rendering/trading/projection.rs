//! Projection sector rendering.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::style::ellipse_arc;
use crate::drawings::validation::MAX_PROJECTION_LEVEL;
use crate::model::LineStyle;
use crate::painting::dashed_line::draw_dashed_polyline;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Align, Align2, Color32, Pos2, Shape, Stroke, Vec2};

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
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(center, rgba(config.background_colors[0]));
            for &point in &sector[1..] {
                mesh.colored_vertex(point, rgba(config.background_colors[1]));
            }
            for i in 1..sector.len() - 1 {
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
            stroke(
                ellipse_arc(
                    center,
                    Vec2::splat(radius * level.coefficient),
                    range,
                    clip,
                    ppp,
                ),
                Stroke::new(level.width.unwrap_or(self.stroke_width), rgba(level.color)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use egui::{Rect, Shape, epaint::ColorMode};
    use serde_json::{Value, json};

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
