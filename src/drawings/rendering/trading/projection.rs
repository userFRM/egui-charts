//! Projection tool rendering

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Renders Projection Tool with measured move and percentage.
    /// Features: AB=CD style projection, percentage labels, target zone.
    pub(crate) fn render_projection_tool(&self, painter: &egui::Painter, _chart_rect: Rect) {
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

        self.draw_projection_dashed_line(painter, proj_point, dx, dy, dashed_color);

        // Target zone (shaded rectangle at projection end)
        let proj_end = Pos2::new(proj_point.x + dx, proj_point.y + dy);
        self.draw_projection_target_zone(painter, proj_end, dy, dashed_color);

        // Anchor points with white stroke
        self.draw_projection_anchors(painter, start, end, proj_point, color);

        // Labels
        self.draw_projection_labels(painter, start, end, proj_end, dy, color, dashed_color);
    }

    fn draw_projection_main_line(
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

    fn draw_projection_dashed_line(
        &self,
        painter: &egui::Painter,
        proj_point: Pos2,
        dx: f32,
        dy: f32,
        dashed_color: Color32,
    ) {
        let proj_end = Pos2::new(proj_point.x + dx, proj_point.y + dy);
        let dash = 8.0;
        let gap = 5.0;
        let proj_dx = proj_end.x - proj_point.x;
        let proj_dy = proj_end.y - proj_point.y;
        let proj_len = (proj_dx * proj_dx + proj_dy * proj_dy).sqrt();
        let steps = (proj_len / (dash + gap)) as i32;

        for i in 0..steps {
            let t1 = (i as f32 * (dash + gap)) / proj_len;
            let t2 = ((i as f32 * (dash + gap)) + dash) / proj_len;
            if t2 <= 1.0 {
                let p1 = Pos2::new(proj_point.x + proj_dx * t1, proj_point.y + proj_dy * t1);
                let p2 = Pos2::new(
                    proj_point.x + proj_dx * t2.min(1.0),
                    proj_point.y + proj_dy * t2.min(1.0),
                );
                painter.line_segment([p1, p2], Stroke::new(self.stroke_width, dashed_color));
            }
        }
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
        let font = egui::FontId::proportional(typography::XS);

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
        let label_text = format!("{:.2} ({:.1}%)", price_change, pct_change);
        let label_bg = Rect::from_center_size(
            Pos2::new(mid_ab.x + 10.0, mid_ab.y - 10.0),
            egui::vec2(90.0, 16.0),
        );
        painter.rect_filled(
            label_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.text(
            label_bg.center(),
            egui::Align2::CENTER_CENTER,
            label_text,
            font.clone(),
            color,
        );

        // Projection target label
        let zone_height = (dy.abs() * 0.2).max(20.0);
        let target_label = format!("Target: {:.2}", price_change);
        let target_bg = Rect::from_center_size(
            Pos2::new(proj_end.x, proj_end.y - zone_height / 2.0 - 12.0),
            egui::vec2(80.0, 16.0),
        );
        painter.rect_filled(target_bg, DESIGN_TOKENS.rounding.sm, dashed_color);
        painter.text(
            target_bg.center(),
            egui::Align2::CENTER_CENTER,
            target_label,
            font,
            Color32::WHITE,
        );
    }
}
