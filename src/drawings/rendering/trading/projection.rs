//! Projection tool rendering

use crate::drawings::domain::{Drawing, ProjectionLevel};
use crate::drawings::rendering::ChartView;
use crate::drawings::rendering::measurements::boxed_label;
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::drawings::validation::MAX_PROJECTION_LEVEL;
use crate::localization as l10n;
use crate::scales::group_thousands;
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders Projection Tool with measured move and percentage.
    /// Features: AB=CD style projection, percentage labels, target zone.
    pub(crate) fn render_projection_tool(
        &self,
        painter: &egui::Painter,
        _chart_rect: Rect,
        view: &ChartView,
    ) {
        if self.points.len() < 2 {
            return;
        }
        let start = self.points[0];
        let end = self.points[1];
        let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let color = rgba(self.color);
        let config = self.projection_config();
        // Older saves keep their 8/5 dash and the shared style applied after rendering.
        let legacy = self.projection.is_none();
        let leg = end - start;
        let dy = leg.y;
        let proj_point = self
            .points
            .get(2)
            .copied()
            .unwrap_or_else(|| Pos2::new(end.x + (end.x - start.x) * 0.5, end.y));
        let levels: Vec<(&ProjectionLevel, Pos2)> = config
            .levels
            .iter()
            .filter(|level| {
                level.visible && (0.0..=MAX_PROJECTION_LEVEL).contains(&level.coefficient)
            })
            .map(|level| {
                (
                    level,
                    self.projection_target(view, proj_point, leg, level.coefficient),
                )
            })
            .collect();

        if config.trend_visible {
            let dash = if legacy {
                None
            } else {
                config.trend_line_style.dash_pattern()
            };
            if let Some((dash, gap)) = dash {
                draw_dashed_line(
                    painter,
                    start,
                    end,
                    Stroke::new(self.stroke_width, color),
                    dash,
                    gap,
                );
                self.draw_projection_arrowhead(painter, start, end, color);
            } else {
                self.draw_projection_main_line(painter, start, end, color);
            }
        }
        for &(level, target) in &levels {
            let stroke = Stroke::new(level.width.unwrap_or(self.stroke_width), rgba(level.color));
            let dash = if legacy {
                Some((8.0, 5.0))
            } else {
                level.line_style.dash_pattern()
            };
            if let Some((dash, gap)) = dash {
                draw_dashed_line(painter, proj_point, target, stroke, dash, gap);
            } else {
                painter.line_segment([proj_point, target], stroke);
            }
        }
        if config.show_background {
            for &(level, target) in &levels {
                self.draw_projection_target_zone(
                    painter,
                    target,
                    dy,
                    rgba(level.color),
                    config.background_colors.map(rgba),
                );
            }
        }

        if !self.completed && self.points.len() == 2 {
            for &point in &[start, end] {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, self.color32());
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }
        }

        if config.trend_visible {
            self.draw_projection_move_label(
                painter,
                start,
                end,
                dy,
                view.price_formatter,
                view.thousands_sep,
            );
        }
        for &(level, target) in &levels {
            self.draw_projection_target_label(
                painter,
                target,
                dy,
                level.coefficient,
                level.color,
                config.show_coefficients,
                view.price_formatter,
                view.thousands_sep,
            );
        }
    }

    fn projection_target(&self, view: &ChartView, from: Pos2, leg: egui::Vec2, k: f32) -> Pos2 {
        if let [a, b, c, ..] = self.chart_points.as_slice() {
            // This order keeps k = 1 equal to the saved drawings' c + b - a.
            Pos2::new(
                (view.bar_to_x)(c.bar_idx + k * b.bar_idx - k * a.bar_idx),
                (view.price_to_y)(c.price + f64::from(k) * b.price - f64::from(k) * a.price),
            )
        } else {
            from + leg * k
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

    fn draw_projection_target_zone(
        &self,
        painter: &egui::Painter,
        proj_end: Pos2,
        dy: f32,
        border: Color32,
        colors: [Color32; 2],
    ) {
        let zone_width = 20.0;
        let zone_height = (dy.abs() * 0.2).max(20.0);
        let zone_rect = Rect::from_center_size(proj_end, egui::vec2(zone_width, zone_height));
        let rounding = egui::CornerRadius::from(DESIGN_TOKENS.rounding.md);
        if colors[0] == colors[1] {
            painter.rect_filled(zone_rect, rounding, colors[0]);
        } else {
            let [near, far] = colors;
            let [upper, lower] = if dy < 0.0 { [far, near] } else { [near, far] };
            painter.rect_filled(
                Rect::from_min_max(zone_rect.min, egui::pos2(zone_rect.max.x, proj_end.y)),
                egui::CornerRadius {
                    sw: 0,
                    se: 0,
                    ..rounding
                },
                upper,
            );
            painter.rect_filled(
                Rect::from_min_max(egui::pos2(zone_rect.min.x, proj_end.y), zone_rect.max),
                egui::CornerRadius {
                    nw: 0,
                    ne: 0,
                    ..rounding
                },
                lower,
            );
        }
        painter.rect_stroke(
            zone_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, border),
            StrokeKind::Outside,
        );
    }

    fn draw_projection_move_label(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        dy: f32,
        price_formatter: Option<&dyn crate::scales::PriceFormatter>,
        thousands_sep: Option<char>,
    ) {
        // Price/percentage labels
        let price_change = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].price - self.chart_points[0].price).abs()
        } else {
            dy.abs() as f64 / 10.0
        };

        let pct_change = if !self.chart_points.is_empty()
            && self.chart_points.len() >= 2
            && self.chart_points[0].price != 0.0
        {
            ((self.chart_points[1].price - self.chart_points[0].price) / self.chart_points[0].price
                * 100.0)
                .abs()
        } else {
            0.0
        };

        // Move label (at midpoint of AB)
        let mid_ab = Pos2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
        let label_text = match price_formatter {
            Some(formatter) => format!("{} ({pct_change:.1}%)", formatter.format(price_change)),
            None => format!(
                "{} ({pct_change:.1}%)",
                group_thousands(format!("{price_change:.2}"), thousands_sep)
            ),
        };
        let chart = painter.ctx().active_theme().chart;
        boxed_label(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(mid_ab.x + 10.0, mid_ab.y - 10.0),
            label_text,
            chart.axis_text,
            chart.bg_axis,
        );
    }

    fn draw_projection_target_label(
        &self,
        painter: &egui::Painter,
        target: Pos2,
        dy: f32,
        coefficient: f32,
        color: [u8; 4],
        show_coefficient: bool,
        price_formatter: Option<&dyn crate::scales::PriceFormatter>,
        thousands_sep: Option<char>,
    ) {
        let [r, g, b, a] = color;
        let zone_height = (dy.abs() * 0.2).max(20.0);
        let target_label = self.chart_points.get(2).map_or_else(
            || l10n::text(painter.ctx(), "Target: n/a"),
            |c| {
                let k = f64::from(coefficient);
                let target =
                    c.price + k * self.chart_points[1].price - k * self.chart_points[0].price;
                let price = match price_formatter {
                    Some(formatter) => formatter.format(target),
                    None => group_thousands(format!("{target:.2}"), thousands_sep),
                };
                l10n::format_text(painter.ctx(), "Target: {price}", &[("price", price)])
            },
        );
        boxed_label(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(target.x, target.y - zone_height / 2.0 - 12.0),
            target_label,
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(r, g, b, a),
        );
        if show_coefficient {
            painter.text(
                Pos2::new(target.x + 14.0, target.y),
                egui::Align2::LEFT_CENTER,
                format!("{coefficient}"),
                egui::FontId::proportional(typography::XS),
                Color32::from_rgb(r, g, b),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingToolType};

    /// Measured levels use chart coordinates; switches, independent strokes and split fills
    /// affect the paint rather than merely being saved. JSON also exercises host-built settings.
    #[test]
    fn projection_settings_change_the_painted_drawing() {
        use crate::widget::test_frames::{chart, shapes};
        use egui::Shape;
        use serde_json::{Value, json};
        fn fill_bounds(shapes: &[Shape], rgba: [u8; 4]) -> Vec<Rect> {
            let [r, g, b, a] = rgba;
            let color = Color32::from_rgba_unmultiplied(r, g, b, a);
            shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Rect(r) if r.fill == color => Some(r.rect),
                    Shape::Mesh(m) => {
                        let points: Vec<_> = m
                            .vertices
                            .iter()
                            .filter(|v| v.color == color)
                            .map(|v| v.pos)
                            .collect();
                        (!points.is_empty()).then(|| Rect::from_points(&points))
                    }
                    _ => None,
                })
                .collect()
        }
        fn text(shapes: &[Shape], expected: &str) -> bool {
            shapes
                .iter()
                .any(|s| matches!(s, Shape::Text(t) if t.galley.job.text == expected))
        }
        fn line(shapes: &[Shape], a: Pos2, b: Pos2, stroke: Stroke) -> bool {
            shapes.iter().any(|s| match s {
                Shape::LineSegment {
                    points,
                    stroke: found,
                } => {
                    *found == stroke && points[0].distance(a) < 0.01 && points[1].distance(b) < 0.01
                }
                Shape::Path(p) => {
                    p.stroke == stroke.into()
                        && p.points.first().is_some_and(|v| v.distance(a) < 0.01)
                        && p.points.get(1).is_some_and(|v| v.distance(b) < 0.01)
                }
                _ => false,
            })
        }
        const NEAR: [u8; 4] = [41, 98, 255, 51];
        const FAR: [u8; 4] = [156, 39, 176, 51];
        type Row = (
            &'static str,
            fn(&mut Value),
            fn(&[Shape], &[Pos2; 6]) -> bool,
        );
        let rows: &[Row] = &[
            (
                "whole move",
                |_| {},
                |s, p| {
                    text(s, "Target: 170.00")
                        && text(s, "1")
                        && fill_bounds(s, NEAR).iter().any(|r| r.center().y > p[3].y)
                        && fill_bounds(s, FAR).iter().any(|r| r.center().y < p[3].y)
                },
            ),
            (
                "half and double",
                |c| {
                    let mut level = c["levels"][0].clone();
                    level["coefficient"] = json!(0.5);
                    c["levels"].as_array_mut().unwrap().push(level.clone());
                    level["coefficient"] = json!(2.0);
                    c["levels"].as_array_mut().unwrap().push(level);
                },
                |s, p| {
                    text(s, "Target: 145.00")
                        && text(s, "Target: 220.00")
                        && text(s, "0.5")
                        && text(s, "2")
                        && fill_bounds(s, NEAR)
                            .iter()
                            .any(|r| (r.center().x - p[4].x).abs() < 1.0)
                        && fill_bounds(s, NEAR)
                            .iter()
                            .any(|r| (r.center().x - p[5].x).abs() < 1.0)
                },
            ),
            (
                "hidden level",
                |c| c["levels"][0]["visible"] = json!(false),
                |s, _| {
                    !text(s, "Target: 170.00")
                        && !text(s, "1")
                        && fill_bounds(s, NEAR).is_empty()
                        && fill_bounds(s, FAR).is_empty()
                },
            ),
            (
                "hidden coefficients",
                |c| c["show_coefficients"] = json!(false),
                |s, _| text(s, "Target: 170.00") && !text(s, "1"),
            ),
            (
                "hidden background",
                |c| {
                    c["show_background"] = json!(false);
                    c["levels"][0]["line_style"] = json!("Solid");
                },
                |s, p| {
                    fill_bounds(s, NEAR).is_empty()
                        && fill_bounds(s, FAR).is_empty()
                        && line(
                            s,
                            p[2],
                            p[3],
                            Stroke::new(2.0, Color32::from_rgba_unmultiplied(41, 98, 255, 150)),
                        )
                },
            ),
            (
                "custom fills",
                |c| c["background_colors"] = json!([[1, 2, 3, 40], [4, 5, 6, 40]]),
                |s, _| {
                    !fill_bounds(s, [1, 2, 3, 40]).is_empty()
                        && !fill_bounds(s, [4, 5, 6, 40]).is_empty()
                },
            ),
            (
                "equal fills",
                |c| c["background_colors"] = json!([[1, 2, 3, 40], [1, 2, 3, 40]]),
                |s, p| {
                    let zones = fill_bounds(s, [1, 2, 3, 40]);
                    zones.len() == 1
                        && zones[0].center().distance(p[3]) < 1.0
                        && (19.0..=20.01).contains(&zones[0].width())
                },
            ),
            (
                "level stroke",
                |c| {
                    let l = &mut c["levels"][0];
                    l["color"] = json!([200, 0, 0, 255]);
                    l["line_style"] = json!("Solid");
                    l["width"] = json!(5.0);
                },
                |s, p| {
                    line(
                        s,
                        p[2],
                        p[3],
                        Stroke::new(5.0, Color32::from_rgb(200, 0, 0)),
                    )
                },
            ),
            (
                "hidden trend",
                |c| c["trend_visible"] = json!(false),
                |s, p| {
                    !text(s, "50.00 (50.0%)")
                        && !line(
                            s,
                            p[0],
                            p[1],
                            Stroke::new(2.0, Color32::from_rgb(41, 98, 255)),
                        )
                },
            ),
            (
                "dotted trend",
                |c| c["trend_line_style"] = json!("Dotted"),
                |s, p| {
                    text(s,"50.00 (50.0%)") && !line(s, p[0], p[1], Stroke::new(2.0, Color32::from_rgb(41,98,255))) && s.iter().filter(|s| matches!(s, Shape::LineSegment {stroke,..} if stroke.color == Color32::from_rgb(41,98,255))).count() > 2
                },
            ),
            (
                "empty levels",
                |c| c["levels"] = json!([]),
                |s, _| {
                    text(s, "50.00 (50.0%)")
                        && !text(s, "Target: 170.00")
                        && fill_bounds(s, NEAR).is_empty()
                },
            ),
            (
                "invalid direct levels",
                |c| {
                    c["levels"][0]["coefficient"] = json!(-0.5);
                },
                |s, _| !text(s, "Target: 170.00") && fill_bounds(s, NEAR).is_empty(),
            ),
        ];
        let mut failures = Vec::new();
        for &(name, edit, check) in rows {
            let mut chart = chart(&[80.0], &[240.0]);
            let mut saved =
                serde_json::to_value(Drawing::new(1, DrawingToolType::ProjectionTool)).unwrap();
            saved["color"] = json!([41, 98, 255, 255]);
            saved["projection"] = json!({
                "levels":[{"coefficient":1.0,"visible":true,"color":[41,98,255,150],"line_style":"Dashed","width":null}],
                "show_background":true,"background_colors":[NEAR,FAR],"show_coefficients":true,
                "trend_visible":true,"trend_line_style":"Solid"
            });
            edit(&mut saved["projection"]);
            let mut drawing: Drawing = serde_json::from_value(saved).unwrap();
            drawing.id = 1;
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(10.0, 100.0),
                ChartPoint::new(20.0, 150.0),
                ChartPoint::new(40.0, 120.0),
            ];
            let ctx = egui::Context::default();
            shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let bar_to_x = |bar: f32| map.index_to_x(f64::from(bar));
            let price_to_y = |price| map.price_to_y(price);
            drawing.update_screen_coords(bar_to_x, price_to_y);
            // Paint on the real mapping without mesh batching or a volume legend text "1".
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    let view = ChartView {
                        price_formatter: None,
                        thousands_sep: None,
                        time_formatter: None,
                        instrument: None,
                        bar_seconds: 0,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &bar_to_x,
                        bar_spacing: 1.0,
                        price_to_y: &price_to_y,
                        y_to_price: &|y| map.y_to_price(y),
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    };
                    drawing.render(ui.painter(), ui.clip_rect(), &view);
                },
            );
            let painted: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
            output.drop_without_applying_deltas();
            // Independently: B-A=(10,50), so C + k(B-A) is (50,170), (45,145), (60,220).
            let points = [
                (10, 100.0),
                (20, 150.0),
                (40, 120.0),
                (50, 170.0),
                (45, 145.0),
                (60, 220.0),
            ]
            .map(|(bar, price)| Pos2::new(map.idx_to_x(bar), map.price_to_y(price)));
            if !check(&painted, &points) {
                failures.push(name);
            }
        }
        assert!(
            failures.is_empty(),
            "settings did not affect paint: {failures:?}"
        );
    }

    #[test]
    fn projection_label_boxes_contain_long_prices() {
        for price in [100.0, 123_456_789.0, -123_456_789.0] {
            let mut drawing = Drawing::new(1, DrawingToolType::ProjectionTool);
            drawing.chart_points = vec![
                ChartPoint::new(0.0, price),
                ChartPoint::new(1.0, price * 2.0),
                ChartPoint::new(2.0, price),
            ];
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                drawing.draw_projection_move_label(
                    ui.painter(),
                    Pos2::new(100.0, 300.0),
                    Pos2::new(300.0, 200.0),
                    -100.0,
                    None,
                    None,
                );
                drawing.draw_projection_target_label(
                    ui.painter(),
                    Pos2::new(500.0, 100.0),
                    -100.0,
                    1.0,
                    [0, 0, 255, 255],
                    false,
                    None,
                    None,
                );
            });
            // egui panics when an output with unfreed textures is dropped.
            output.textures_delta.clear();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            assert_eq!(texts.len(), 2);
            for text in texts {
                let bounds = Rect::from_min_size(text.pos, text.galley.size());
                assert!(
                    output.shapes.iter().any(|s| matches!(
                        &s.shape, egui::Shape::Rect(r) if r.rect.contains_rect(bounds)
                    )),
                    "label box must contain {:?}",
                    text.galley.job.text
                );
            }
        }
    }

    #[test]
    fn projection_adds_the_price_leg_on_a_logarithmic_chart() {
        use crate::widget::test_frames::{chart, shapes};
        let mut chart = chart(&[80.0], &[200.0]);
        chart.config_mut().right_axis_scale_mode = crate::scales::PriceScaleMode::Logarithmic;
        chart.config_mut().show_legend_volume = false;
        chart.config_mut().show_volume = false;
        let mut drawing = Drawing::new(1, DrawingToolType::ProjectionTool);
        drawing.projection = None;
        drawing.chart_points = vec![
            ChartPoint::new(10.0, 100.0),
            ChartPoint::new(20.0, 150.0),
            ChartPoint::new(40.0, 120.0),
        ];
        drawing.completed = true;
        chart.drawings_mut().drawings.push(drawing);
        let output = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
        assert!(
            !output
                .iter()
                .any(|shape| matches!(shape, egui::Shape::Text(t) if t.galley.job.text == "1"))
        );
        let map = chart.get_chart_mapping();
        // D=C+(B-A): bar 40+(20-10)=50, price 120+(150-100)=170.
        let target = Pos2::new(map.idx_to_x(50), map.price_to_y(170.0));
        // The 20 px target zone, painted as a rect or within a tessellated mesh, whose fill
        // vertices stop half a pixel inside the rect for the feathered edge and snap to pixels.
        let [r, g, b, _] = chart.drawings().drawings[0].color;
        let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, 30);
        assert!(output.iter().any(|s| match s {
            egui::Shape::Rect(r) =>
                (r.rect.width() - 20.0).abs() < 1e-3 && r.rect.center().distance(target) < 1e-2,
            egui::Shape::Mesh(m) => {
                let zone = Rect::from_points(
                    &(m.vertices.iter())
                        .filter(|v| v.color == fill)
                        .map(|v| v.pos)
                        .collect::<Vec<_>>(),
                );
                (zone.width() - 19.0).abs() < 1e-3 && zone.center().distance(target) <= 0.5
            }
            _ => false,
        }));
        assert!(
            output
                .iter()
                .any(|s| matches!(s,egui::Shape::Text(t) if t.galley.job.text=="Target: 170.00"))
        );
    }
}
