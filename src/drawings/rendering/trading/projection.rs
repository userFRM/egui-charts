//! Projection tool rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use crate::drawings::rendering::measurements::boxed_label;
use crate::drawings::rendering::utils::draw_dashed_line;
use crate::styles::stroke;
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
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Main projection line with arrow
        self.draw_projection_main_line(painter, start, end, color);

        // Third point for AB=CD projection
        let proj_point = if self.points.len() >= 3 {
            self.points[2]
        } else {
            Pos2::new(end.x + (end.x - start.x) * 0.5, end.y)
        };

        // Draw projected move (dashed)
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let dashed_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);

        let proj_end = if let [a, b, c, ..] = self.chart_points.as_slice() {
            Pos2::new(
                (view.bar_to_x)(c.bar_idx + b.bar_idx - a.bar_idx),
                (view.price_to_y)(c.price + b.price - a.price),
            )
        } else {
            Pos2::new(proj_point.x + dx, proj_point.y + dy)
        };
        draw_dashed_line(
            painter,
            proj_point,
            proj_end,
            Stroke::new(self.stroke_width, dashed_color),
            8.0,
            5.0,
        );

        // Target zone (shaded rectangle at projection end)
        self.draw_projection_target_zone(painter, proj_end, dy, dashed_color);

        // Anchor points with white stroke
        self.draw_projection_anchors(painter, start, end, proj_point, color);

        // Labels
        self.draw_projection_labels(painter, start, end, proj_end, dy, color, dashed_color);
    }

    pub(super) fn draw_projection_main_line(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        color: Color32,
    ) {
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));

        // Draw arrow at end
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
        dashed_color: Color32,
    ) {
        let zone_width = 20.0;
        let zone_height = (dy.abs() * 0.2).max(20.0);
        let zone_fill =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 30);
        let zone_rect = Rect::from_center_size(proj_end, egui::vec2(zone_width, zone_height));
        painter.rect_filled(zone_rect, DESIGN_TOKENS.rounding.md, zone_fill);
        painter.rect_stroke(
            zone_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, dashed_color),
            StrokeKind::Outside,
        );
    }

    fn draw_projection_anchors(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        proj_point: Pos2,
        color: Color32,
    ) {
        painter.circle_filled(start, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            start,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(end, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            end,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        if self.points.len() >= 3 {
            painter.circle_filled(proj_point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                proj_point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );
        }
    }

    fn draw_projection_labels(
        &self,
        painter: &egui::Painter,
        start: Pos2,
        end: Pos2,
        proj_end: Pos2,
        dy: f32,
        color: Color32,
        dashed_color: Color32,
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
        let label_text = format!("{price_change:.2} ({pct_change:.1}%)");
        boxed_label(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(mid_ab.x + 10.0, mid_ab.y - 10.0),
            label_text,
            color,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );

        // Projection target label
        let zone_height = (dy.abs() * 0.2).max(20.0);
        let target_label = self.chart_points.get(2).map_or_else(
            || "Target: n/a".to_owned(),
            |c| {
                format!(
                    "Target: {:.2}",
                    c.price + self.chart_points[1].price - self.chart_points[0].price
                )
            },
        );
        boxed_label(
            painter,
            egui::Align2::CENTER_CENTER,
            Pos2::new(proj_end.x, proj_end.y - zone_height / 2.0 - 12.0),
            target_label,
            Color32::WHITE,
            dashed_color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingToolType};

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
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                drawing.draw_projection_labels(
                    ui.painter(),
                    Pos2::new(100.0, 300.0),
                    Pos2::new(300.0, 200.0),
                    Pos2::new(500.0, 100.0),
                    -100.0,
                    Color32::BLUE,
                    Color32::BLUE,
                );
            });
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
        let mut drawing = Drawing::new(1, DrawingToolType::ProjectionTool);
        drawing.chart_points = vec![
            ChartPoint::new(10.0, 100.0),
            ChartPoint::new(20.0, 150.0),
            ChartPoint::new(40.0, 120.0),
        ];
        drawing.completed = true;
        chart.drawings_mut().drawings.push(drawing);
        let output = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
        let map = chart.get_chart_mapping();
        // D=C+(B-A): bar 40+(20-10)=50, price 120+(150-100)=170.
        let target = Pos2::new(map.idx_to_x(50), map.price_to_y(170.0));
        // The 20 px target zone, painted as a rect or within a tessellated mesh, whose fill
        // vertices stop half a pixel inside the rect for the feathered edge.
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
                (zone.width() - 19.0).abs() < 1e-3 && zone.center().distance(target) < 1e-2
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
