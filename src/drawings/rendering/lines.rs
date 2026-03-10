//! Basic drawing tool rendering implementations
//!
//! Includes: lines, rays, shapes (rect, circle, ellipse, triangle, arc),
//! arrows, and polylines.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Render trend line - simple line between two points
    pub(crate) fn render_trend_line(
        &self,
        painter: &egui::Painter,
        color: Color32,
        stroke: Stroke,
    ) {
        if self.points.len() >= 2 {
            painter.line_segment([self.points[0], self.points[1]], stroke);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render ray - extends from start through end to chart edge
    pub(crate) fn render_ray(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        if self.points.len() >= 2 {
            let start = self.points[0];
            let end = self.points[1];
            let dx = end.x - start.x;
            let dy = end.y - start.y;

            let extended_end = if dx.abs() > 1e-6 {
                let t = (rect.max.x - start.x) / dx;
                Pos2::new(rect.max.x, start.y + t * dy)
            } else if dy > 0.0 {
                Pos2::new(start.x, rect.max.y)
            } else {
                Pos2::new(start.x, rect.min.y)
            };

            painter.line_segment([start, extended_end], stroke);
            painter.circle_filled(start, DESIGN_TOKENS.rounding.md, color);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render extended line - extends infinitely in both directions
    pub(crate) fn render_extended_line(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        if self.points.len() >= 2 {
            let p1 = self.points[0];
            let p2 = self.points[1];
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;

            if dx.abs() > 1e-6 {
                let t_left = (rect.min.x - p1.x) / dx;
                let t_right = (rect.max.x - p1.x) / dx;

                let left_point = Pos2::new(rect.min.x, p1.y + t_left * dy);
                let right_point = Pos2::new(rect.max.x, p1.y + t_right * dy);

                painter.line_segment([left_point, right_point], stroke);
            } else {
                painter.vline(p1.x, rect.y_range(), stroke);
            }

            painter.circle_filled(p1, DESIGN_TOKENS.rounding.sm, color);
            painter.circle_filled(p2, DESIGN_TOKENS.rounding.sm, color);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render horizontal line
    pub(crate) fn render_horizontal_line(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        stroke: Stroke,
    ) {
        if !self.points.is_empty() {
            let y = self.points[0].y;
            painter.hline(rect.x_range(), y, stroke);
        }
    }

    /// Render horizontal ray - extends right from start point
    pub(crate) fn render_horizontal_ray(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        if !self.points.is_empty() {
            let start = self.points[0];
            painter.line_segment([start, Pos2::new(rect.max.x, start.y)], stroke);
            painter.circle_filled(start, DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render vertical line
    pub(crate) fn render_vertical_line(&self, painter: &egui::Painter, rect: Rect, stroke: Stroke) {
        if !self.points.is_empty() {
            let x = self.points[0].x;
            painter.vline(x, rect.y_range(), stroke);
        }
    }

    /// Render cross line - horizontal and vertical lines intersecting
    pub(crate) fn render_cross_line(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        if !self.points.is_empty() {
            let point = self.points[0];
            painter.hline(rect.x_range(), point.y, stroke);
            painter.vline(point.x, rect.y_range(), stroke);
            painter.circle_filled(point, DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render trend angle - angle measurement with arc and info
    pub(crate) fn render_trend_angle(
        &self,
        painter: &egui::Painter,
        color: Color32,
        stroke: Stroke,
    ) {
        if self.points.len() >= 2 {
            let start = self.points[0];
            let end = self.points[1];

            // Draw main trend line
            painter.line_segment([start, end], stroke);

            // Draw horizontal reference line (dashed)
            let ref_end = Pos2::new(end.x, start.y);
            let dashed_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);

            // Draw dashed reference line
            let ref_segments = 20;
            for i in 0..ref_segments {
                let t1 = (i as f32 * 2.0) / (ref_segments as f32 * 2.0);
                let t2 = ((i as f32 * 2.0) + 1.0) / (ref_segments as f32 * 2.0);
                let p1 = Pos2::new(start.x + t1 * (ref_end.x - start.x), start.y);
                let p2 = Pos2::new(start.x + t2 * (ref_end.x - start.x), start.y);
                painter.line_segment([p1, p2], Stroke::new(stroke::HAIRLINE, dashed_color));
            }

            // Calculate angle
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let angle_rad = dy.atan2(dx);
            let angle_deg = angle_rad.to_degrees();

            // Determine arc direction and bounds
            let arc_radius = DESIGN_TOKENS.sizing.technical_labels.line_arc_radius;
            let segments = 32;
            let start_angle = 0.0f32;
            let end_angle_arc = angle_rad;

            // Draw smooth arc using quadratic bezier approximation
            let mut arc_points: Vec<Pos2> = Vec::with_capacity(segments + 1);
            for i in 0..=segments {
                let t = i as f32 / segments as f32;
                let angle = start_angle + (end_angle_arc - start_angle) * t;
                let x = start.x + arc_radius * angle.cos();
                let y = start.y + arc_radius * angle.sin();
                arc_points.push(Pos2::new(x, y));
            }

            // Draw arc with gradient-like effect (thicker in middle)
            for i in 1..arc_points.len() {
                let t = i as f32 / arc_points.len() as f32;
                let thickness = 1.5 + 0.5 * (std::f32::consts::PI * t).sin();
                painter.line_segment(
                    [arc_points[i - 1], arc_points[i]],
                    Stroke::new(thickness, color),
                );
            }

            // Draw arc endpoints
            if !arc_points.is_empty() {
                painter.circle_filled(arc_points[0], DESIGN_TOKENS.rounding.sm, color);
                painter.circle_filled(
                    *arc_points.last().unwrap(),
                    DESIGN_TOKENS.rounding.sm,
                    color,
                );
            }

            // Calculate distance and slope for info display
            let distance = (dx * dx + dy * dy).sqrt();
            let slope = if dx.abs() > 0.001 {
                dy / dx
            } else {
                f32::INFINITY
            };
            let slope_display = if slope.is_finite() {
                format!("{:.3}", slope)
            } else {
                "∞".to_string()
            };

            // Draw angle label with background box
            let label_angle = angle_rad / 2.0;
            let label_distance = arc_radius + DESIGN_TOKENS.spacing.section_lg;
            let label_pos = Pos2::new(
                start.x + label_distance * label_angle.cos(),
                start.y + label_distance * label_angle.sin(),
            );

            // Main angle text
            let angle_text = format!("{:.1}°", angle_deg.abs());
            let font = egui::FontId::proportional(typography::MD);

            // Create info box
            let box_padding = DESIGN_TOKENS.spacing.sm;
            let text_width = DESIGN_TOKENS.sizing.technical_labels.line_label_width;
            let text_height = DESIGN_TOKENS.sizing.technical_labels.line_label_height;
            let box_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    text_width + box_padding * 2.0,
                    text_height + box_padding * 2.0,
                ),
            );

            // Box background
            let box_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.9);
            painter.rect_filled(box_rect, DESIGN_TOKENS.rounding.sm, box_bg);
            painter.rect_stroke(
                box_rect,
                DESIGN_TOKENS.rounding.sm,
                Stroke::new(stroke::HAIRLINE, color),
                egui::epaint::StrokeKind::Inside,
            );

            // Angle text
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                &angle_text,
                font,
                color,
            );

            // Draw secondary info (distance and slope) near the line
            let mid_point = Pos2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
            let info_offset = DESIGN_TOKENS.sizing.position_tool.label_offset_y;
            let perpendicular_angle = angle_rad + std::f32::consts::FRAC_PI_2;
            let info_pos = Pos2::new(
                mid_point.x + info_offset * perpendicular_angle.cos(),
                mid_point.y + info_offset * perpendicular_angle.sin(),
            );

            let info_text = format!("d: {:.0}px  m: {}", distance, slope_display);
            let info_font = egui::FontId::proportional(typography::XS);
            let muted_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
            painter.text(
                info_pos,
                egui::Align2::CENTER_CENTER,
                &info_text,
                info_font,
                muted_color,
            );

            // Draw anchor points
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
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render rect - with optional middle line and price range
    pub(crate) fn render_rect(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        if self.points.len() >= 2 {
            let rect = Rect::from_two_pos(self.points[0], self.points[1]);

            // Fill if configured
            if let Some(fill) = self.fill_color {
                let fill_color =
                    Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]);
                painter.rect_filled(rect, 0.0, fill_color);
            }

            // Main rectangle stroke
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);

            // Draw middle horizontal line (50% level)
            let middle_y = (rect.min.y + rect.max.y) / 2.0;
            let middle_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
            super::utils::draw_dashed_line(
                painter,
                Pos2::new(rect.min.x, middle_y),
                Pos2::new(rect.max.x, middle_y),
                Stroke::new(stroke::HAIRLINE, middle_color),
                DESIGN_TOKENS.rounding.lg,
                DESIGN_TOKENS.rounding.md,
            );

            // Draw middle vertical line
            let middle_x = (rect.min.x + rect.max.x) / 2.0;
            super::utils::draw_dashed_line(
                painter,
                Pos2::new(middle_x, rect.min.y),
                Pos2::new(middle_x, rect.max.y),
                Stroke::new(stroke::HAIRLINE, middle_color),
                DESIGN_TOKENS.rounding.lg,
                DESIGN_TOKENS.rounding.md,
            );

            // Draw price range label (right side)
            let height = (rect.max.y - rect.min.y).abs();
            let width = (rect.max.x - rect.min.x).abs();
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.8);
            let label_font = egui::FontId::proportional(typography::XS);

            // Width label (bottom center)
            if width > 50.0 {
                let width_label_pos = Pos2::new(rect.center().x, rect.max.y + typography::XS);
                let width_rect = Rect::from_center_size(
                    width_label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.line_arc_radius,
                        DESIGN_TOKENS.sizing.technical_labels.gann_label_height,
                    ),
                );
                painter.rect_filled(width_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    width_label_pos,
                    egui::Align2::CENTER_CENTER,
                    format!("{:.0}px", width),
                    label_font.clone(),
                    middle_color,
                );
            }

            // Height label (right center)
            if height > 50.0 {
                let height_label_pos = Pos2::new(rect.max.x + typography::LG, rect.center().y);
                let height_rect = Rect::from_center_size(
                    height_label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.line_arc_radius,
                        DESIGN_TOKENS.sizing.technical_labels.gann_label_height,
                    ),
                );
                painter.rect_filled(height_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    height_label_pos,
                    egui::Align2::CENTER_CENTER,
                    format!("{:.0}px", height),
                    label_font,
                    middle_color,
                );
            }

            // Corner anchor points
            let corners = [
                rect.min,
                Pos2::new(rect.max.x, rect.min.y),
                rect.max,
                Pos2::new(rect.min.x, rect.max.y),
            ];
            for corner in corners {
                painter.circle_filled(corner, DESIGN_TOKENS.rounding.md, color);
                painter.circle_stroke(
                    corner,
                    DESIGN_TOKENS.rounding.md,
                    Stroke::new(stroke::HAIRLINE, Color32::WHITE),
                );
            }
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render circle
    pub(crate) fn render_circle(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        if self.points.len() >= 2 {
            let center = self.points[0];
            let edge = self.points[1];
            let radius = ((edge.x - center.x).powi(2) + (edge.y - center.y).powi(2)).sqrt();
            if let Some(fill) = self.fill_color {
                let fill_color =
                    Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]);
                painter.circle_filled(center, radius, fill_color);
            }
            painter.circle_stroke(center, radius, stroke);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render ellipse
    pub(crate) fn render_ellipse(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        if self.points.len() >= 2 {
            let p1 = self.points[0];
            let p2 = self.points[1];
            let center = Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
            let rx = (p2.x - p1.x).abs() / 2.0;
            let ry = (p2.y - p1.y).abs() / 2.0;

            let segments = 48;
            let mut ellipse_points: Vec<Pos2> = Vec::with_capacity(segments + 1);
            for i in 0..=segments {
                let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
                let x = center.x + rx * angle.cos();
                let y = center.y + ry * angle.sin();
                ellipse_points.push(Pos2::new(x, y));
            }

            if let Some(fill) = self.fill_color {
                let fill_color =
                    Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]);
                painter.add(egui::Shape::convex_polygon(
                    ellipse_points.clone(),
                    fill_color,
                    Stroke::NONE,
                ));
            }

            for i in 1..ellipse_points.len() {
                painter.line_segment([ellipse_points[i - 1], ellipse_points[i]], stroke);
            }
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render triangle
    pub(crate) fn render_triangle(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        if self.points.len() >= 3 {
            let p1 = self.points[0];
            let p2 = self.points[1];
            let p3 = self.points[2];
            if let Some(fill) = self.fill_color {
                let fill_color =
                    Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]);
                painter.add(egui::Shape::convex_polygon(
                    vec![p1, p2, p3],
                    fill_color,
                    Stroke::NONE,
                ));
            }
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
            painter.line_segment([p3, p1], stroke);
        } else {
            for (i, &p) in self.points.iter().enumerate() {
                painter.circle_filled(p, DESIGN_TOKENS.rounding.sm, color);
                if i > 0 {
                    painter.line_segment([self.points[i - 1], p], stroke);
                }
            }
        }
    }

    /// Render arc - semicircle
    pub(crate) fn render_arc(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        if self.points.len() >= 2 {
            let center = self.points[0];
            let edge = self.points[1];
            let radius = ((edge.x - center.x).powi(2) + (edge.y - center.y).powi(2)).sqrt();

            let start_angle = (edge.y - center.y).atan2(edge.x - center.x);
            let segments = 32;

            let mut prev_point: Option<Pos2> = None;
            for i in 0..=segments {
                let t = i as f32 / segments as f32;
                let angle = start_angle + std::f32::consts::PI * t;
                let x = center.x + radius * angle.cos();
                let y = center.y + radius * angle.sin();
                let point = Pos2::new(x, y);

                if let Some(prev) = prev_point {
                    painter.line_segment([prev, point], stroke);
                }
                prev_point = Some(point);
            }

            painter.circle_filled(center, DESIGN_TOKENS.rounding.sm, color);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render polyline - connected line segments through all points
    pub(crate) fn render_polyline(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment([self.points[i - 1], point], stroke);
            }
            painter.circle_filled(point, DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render arrow - line with arrowhead
    pub(crate) fn render_arrow(&self, painter: &egui::Painter, color: Color32, stroke: Stroke) {
        if self.points.len() >= 2 {
            let start = self.points[0];
            let end = self.points[1];

            painter.line_segment([start, end], stroke);

            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let angle = dy.atan2(dx);
            let arrow_size = DESIGN_TOKENS.spacing.xl;
            let arrow_angle = 0.5;

            let p1 = Pos2::new(
                end.x - arrow_size * (angle - arrow_angle).cos(),
                end.y - arrow_size * (angle - arrow_angle).sin(),
            );
            let p2 = Pos2::new(
                end.x - arrow_size * (angle + arrow_angle).cos(),
                end.y - arrow_size * (angle + arrow_angle).sin(),
            );

            painter.line_segment([end, p1], stroke);
            painter.line_segment([end, p2], stroke);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render arrow marker - right-pointing triangle
    pub(crate) fn render_arrow_marker(&self, painter: &egui::Painter, color: Color32) {
        if !self.points.is_empty() {
            let pos = self.points[0];
            let size = DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs;
            let points = vec![
                Pos2::new(pos.x + size, pos.y),
                Pos2::new(pos.x - size * 0.5, pos.y - size * 0.6),
                Pos2::new(pos.x - size * 0.5, pos.y + size * 0.6),
            ];
            let shape = egui::epaint::PathShape::convex_polygon(points, color, Stroke::NONE);
            painter.add(shape);
        }
    }

    /// Render arrow mark up - upward pointing bullish arrow
    pub(crate) fn render_arrow_mark_up(&self, painter: &egui::Painter) {
        if !self.points.is_empty() {
            let pos = self.points[0];
            let size = typography::MD;
            let points = vec![
                Pos2::new(pos.x, pos.y - size),
                Pos2::new(pos.x - size * 0.6, pos.y + size * 0.5),
                Pos2::new(pos.x + size * 0.6, pos.y + size * 0.5),
            ];
            let fill = DESIGN_TOKENS.semantic.extended.bullish;
            let border = DESIGN_TOKENS
                .semantic
                .extended
                .bullish
                .gamma_multiply(200_f32 / 255.0);
            let shape = egui::epaint::PathShape::convex_polygon(
                points,
                fill,
                Stroke::new(stroke::HAIRLINE, border),
            );
            painter.add(shape);
        }
    }

    /// Render arrow mark down - downward pointing bearish arrow
    pub(crate) fn render_arrow_mark_down(&self, painter: &egui::Painter) {
        if !self.points.is_empty() {
            let pos = self.points[0];
            let size = typography::MD;
            let points = vec![
                Pos2::new(pos.x, pos.y + size),
                Pos2::new(pos.x - size * 0.6, pos.y - size * 0.5),
                Pos2::new(pos.x + size * 0.6, pos.y - size * 0.5),
            ];
            let fill = DESIGN_TOKENS.semantic.extended.bearish;
            let border = DESIGN_TOKENS
                .semantic
                .extended
                .bearish
                .gamma_multiply(200_f32 / 255.0);
            let shape = egui::epaint::PathShape::convex_polygon(
                points,
                fill,
                Stroke::new(stroke::HAIRLINE, border),
            );
            painter.add(shape);
        }
    }
}
