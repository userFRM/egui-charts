//! Basic drawing tool rendering implementations
//!
//! Includes: lines, rays, shapes (rect, circle, ellipse, triangle, arc),
//! arrows, and polylines.

use crate::drawings::domain::Drawing;
use crate::model::Bar;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Draws the straight segment from `a` to `b` in the drawing's line style.
    pub(super) fn styled_segment(&self, painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
        match self.line_style.dash_pattern() {
            None => {
                painter.line_segment([a, b], stroke);
            }
            Some((dash, gap)) => super::utils::draw_dashed_line(painter, a, b, stroke, dash, gap),
        }
    }

    /// Ends of a trend line from left to right, the left one extended to `min_x` and the right
    /// one to `max_x` when the drawing extends that way; `None` below two anchors.
    pub(crate) fn trend_line_ends(&self, min_x: f32, max_x: f32) -> Option<(Pos2, Pos2)> {
        let &[a, b, ..] = self.points.as_slice() else {
            return None;
        };
        let (mut left, mut right) = if a.x <= b.x { (a, b) } else { (b, a) };
        let slope = (right.y - left.y) / (right.x - left.x);
        if slope.is_finite() {
            if self.extend_left && min_x < left.x {
                left = Pos2::new(min_x, left.y + (min_x - left.x) * slope);
            }
            if self.extend_right && max_x > right.x {
                right = Pos2::new(max_x, right.y + (max_x - right.x) * slope);
            }
        }
        Some((left, right))
    }

    /// Render trend line - a line between two points, extended to the left or right edge of
    /// `rect` when the drawing asks for it
    pub(crate) fn render_trend_line(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        if let Some((left, right)) = self.trend_line_ends(rect.min.x, rect.max.x) {
            self.styled_segment(painter, left, right, stroke);
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
            self.styled_segment(painter, start, ray_end(start, self.points[1], rect), stroke);
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
            let [a, b] = extended_line(p1, p2, rect);
            self.styled_segment(painter, a, b, stroke);

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
            let (left, right) = (Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y));
            self.styled_segment(painter, left, right, stroke);
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
            self.styled_segment(painter, start, Pos2::new(rect.max.x, start.y), stroke);
            painter.circle_filled(start, DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render vertical line
    pub(crate) fn render_vertical_line(&self, painter: &egui::Painter, rect: Rect, stroke: Stroke) {
        if !self.points.is_empty() {
            let x = self.points[0].x;
            let (top, bottom) = (Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y));
            self.styled_segment(painter, top, bottom, stroke);
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
            let (left, right) = (
                Pos2::new(rect.min.x, point.y),
                Pos2::new(rect.max.x, point.y),
            );
            let (top, bottom) = (
                Pos2::new(point.x, rect.min.y),
                Pos2::new(point.x, rect.max.y),
            );
            self.styled_segment(painter, left, right, stroke);
            self.styled_segment(painter, top, bottom, stroke);
            painter.circle_filled(point, DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render trend angle - angle measurement with arc and info
    pub(crate) fn render_trend_angle(
        &self,
        painter: &egui::Painter,
        color: Color32,
        stroke: Stroke,
        bars: &[Bar],
    ) {
        if self.points.len() >= 2 {
            let start = self.points[0];
            let end = self.points[1];

            // Draw main trend line
            self.styled_segment(painter, start, end, stroke);

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

            let info_text = self.measurement_label(bars).unwrap_or_default();
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

            // Width in bars below, height in price on the right
            let height = rect.height();
            let width = rect.width();
            let [width_label, height_label] =
                <[String; 2]>::try_from(self.size_labels(&|_| 0.0)).unwrap_or_default();
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.8);

            // Width label (bottom center)
            if width > 50.0 {
                let width_label_pos = Pos2::new(rect.center().x, rect.max.y + typography::XS);
                super::measurements::boxed_label(
                    painter,
                    egui::Align2::CENTER_CENTER,
                    width_label_pos,
                    width_label,
                    middle_color,
                    label_bg,
                );
            }

            // Height label (right center)
            if height > 50.0 {
                super::measurements::boxed_label(
                    painter,
                    egui::Align2::LEFT_CENTER,
                    Pos2::new(rect.max.x + typography::XS, rect.center().y),
                    height_label,
                    middle_color,
                    label_bg,
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
            let ellipse_points = ellipse_points(self.points[0], self.points[1]);

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
            for pair in arc_points(center, self.points[1]).windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
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

/// Where a ray from `start` through `end` leaves `rect`: on the side `end` is
/// towards, or at the top or bottom when it is vertical.
pub(super) fn ray_end(start: Pos2, end: Pos2, rect: Rect) -> Pos2 {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    if dx.abs() > 1e-6 {
        let edge = if dx > 0.0 { rect.max.x } else { rect.min.x };
        let t = (edge - start.x) / dx;
        Pos2::new(edge, start.y + t * dy)
    } else if dy > 0.0 {
        Pos2::new(start.x, rect.max.y)
    } else {
        Pos2::new(start.x, rect.min.y)
    }
}

/// The line through `p1` and `p2` across `rect`, edge to edge.
pub(super) fn extended_line(p1: Pos2, p2: Pos2, rect: Rect) -> [Pos2; 2] {
    let (dx, dy) = (p2.x - p1.x, p2.y - p1.y);
    if dx.abs() > 1e-6 {
        [rect.min.x, rect.max.x].map(|x| Pos2::new(x, p1.y + (x - p1.x) / dx * dy))
    } else {
        [rect.min.y, rect.max.y].map(|y| Pos2::new(p1.x, y))
    }
}

/// The closed outline of the ellipse inscribed in the box from `p1` to `p2`.
pub(super) fn ellipse_points(p1: Pos2, p2: Pos2) -> Vec<Pos2> {
    let center = Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
    let (rx, ry) = ((p2.x - p1.x).abs() / 2.0, (p2.y - p1.y).abs() / 2.0);
    let segments = 48;
    (0..=segments)
        .map(|i| {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            Pos2::new(center.x + rx * angle.cos(), center.y + ry * angle.sin())
        })
        .collect()
}

/// The half circle around `center` that starts at `edge` and turns clockwise
/// on screen.
pub(super) fn arc_points(center: Pos2, edge: Pos2) -> Vec<Pos2> {
    let radius = center.distance(edge);
    let start_angle = (edge.y - center.y).atan2(edge.x - center.x);
    let segments = 32;
    (0..=segments)
        .map(|i| {
            let angle = start_angle + std::f32::consts::PI * (i as f32 / segments as f32);
            Pos2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In a pane from (0, 0) to (1000, 600), a ray from (500, 100) moving 100 px across and
    /// 50 px down per step reaches the right edge after 5 steps at y = 350, and moving 100 px
    /// left reaches the left edge after 5 steps, also at y = 350. A vertical ray runs to the
    /// bottom or the top edge, whichever its second anchor is towards.
    #[test]
    fn rays_run_through_their_second_anchor_to_the_pane_edge() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1000.0, 600.0));
        let start = Pos2::new(500.0, 100.0);
        let cases = [
            ((600.0, 150.0), (1000.0, 350.0)),
            ((400.0, 150.0), (0.0, 350.0)),
            ((500.0, 300.0), (500.0, 600.0)),
            ((500.0, 50.0), (500.0, 0.0)),
        ];
        let wrong: Vec<String> = cases
            .into_iter()
            .filter_map(|((x, y), (want_x, want_y))| {
                let got = ray_end(start, Pos2::new(x, y), rect);
                (got != Pos2::new(want_x, want_y))
                    .then(|| format!("through ({x}, {y}): got {got:?}, want ({want_x}, {want_y})"))
            })
            .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
