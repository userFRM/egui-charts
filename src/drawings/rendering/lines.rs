//! Basic drawing tool rendering implementations
//!
//! Includes: lines, rays, shapes (rect, circle, ellipse, triangle, arc),
//! arrows, and polylines.

use super::ChartView;
use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
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

    /// Ends from left to right, extended to the pane edges as configured; `None` below two
    /// anchors. Vertical rays extend in anchor order, vertical extended lines top to bottom.
    pub(crate) fn trend_line_ends(&self, rect: Rect) -> Option<(Pos2, Pos2)> {
        let &[a, b, ..] = self.points.as_slice() else {
            return None;
        };
        if a.x == b.x {
            use crate::drawings::DrawingToolType as T;
            if matches!(self.tool_type, T::Ray | T::ExtendedLine) {
                let (start, end) = if self.tool_type == T::ExtendedLine && b.y < a.y {
                    (b, a)
                } else {
                    (a, b)
                };
                return Some((
                    if self.extend_left {
                        ray_end(end, start, rect)
                    } else {
                        start
                    },
                    if self.extend_right {
                        ray_end(start, end, rect)
                    } else {
                        end
                    },
                ));
            }
        }
        let (mut left, mut right) = if a.x <= b.x { (a, b) } else { (b, a) };
        let slope = (right.y - left.y) / (right.x - left.x);
        if slope.is_finite() {
            if self.extend_left && rect.min.x < left.x {
                left = Pos2::new(rect.min.x, left.y + (rect.min.x - left.x) * slope);
            }
            if self.extend_right && rect.max.x > right.x {
                right = Pos2::new(rect.max.x, right.y + (rect.max.x - right.x) * slope);
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
        if let Some((left, right)) = self.trend_line_ends(rect) {
            self.styled_segment(painter, left, right, stroke);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Render a ray with its configured extensions and its start anchor.
    pub(crate) fn render_ray(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        self.render_trend_line(painter, rect, color, stroke);
        if self.points.len() >= 2 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Render an extended line with its configured extensions and its anchors.
    pub(crate) fn render_extended_line(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        self.render_trend_line(painter, rect, color, stroke);
        if self.points.len() >= 2 {
            let p1 = self.points[0];
            let p2 = self.points[1];
            painter.circle_filled(p1, DESIGN_TOKENS.rounding.sm, color);
            painter.circle_filled(p2, DESIGN_TOKENS.rounding.sm, color);
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
        view: &ChartView,
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

            let info_text = self
                .measurement_label(view.bars, view.instrument, view.time_axis)
                .unwrap_or_default();
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

    /// Rectangle border and fill bounds, including its configured extensions.
    pub(crate) fn rectangle_bounds(&self, pane: Rect) -> Option<Rect> {
        let &[a, b, ..] = self.points.as_slice() else {
            return None;
        };
        let mut rect = Rect::from_two_pos(a, b);
        if self.extend_left {
            rect.min.x = rect.min.x.min(pane.min.x);
        }
        if self.extend_right {
            rect.max.x = rect.max.x.max(pane.max.x);
        }
        Some(rect)
    }

    /// Render rect: its border and fill.
    pub(crate) fn render_rect(
        &self,
        painter: &egui::Painter,
        pane: Rect,
        color: Color32,
        stroke: Stroke,
    ) {
        if let Some(rect) = self.rectangle_bounds(pane) {
            // Fill if configured
            if let Some(fill) = self.fill_color {
                let fill_color =
                    Color32::from_rgba_unmultiplied(fill[0], fill[1], fill[2], fill[3]);
                painter.rect_filled(rect, 0.0, fill_color);
            }

            // Main rectangle stroke
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.md, color);
        }
    }

    /// Draw the optional middle line after common border styling, so it stays thin and dashed.
    pub(super) fn render_rect_middle_line(&self, painter: &egui::Painter, pane: Rect) {
        if self.channel_midline
            && let Some(rect) = self.rectangle_bounds(pane)
        {
            super::utils::draw_dashed_line(
                painter,
                Pos2::new(rect.min.x, rect.center().y),
                Pos2::new(rect.max.x, rect.center().y),
                Stroke::new(stroke::HAIRLINE, self.color32()),
                DESIGN_TOKENS.rounding.lg,
                DESIGN_TOKENS.rounding.md,
            );
        }
    }

    /// Render the centre guides and the size labels of a rectangle that is selected or being
    /// drawn. The optional persistent middle line is rendered separately.
    pub(crate) fn render_rect_guides(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }
        let rect = Rect::from_two_pos(self.points[0], self.points[1]);
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
        let chart = painter.ctx().active_theme().chart;
        let label_bg = chart.bg_axis;

        // Width label (bottom center)
        if width > 50.0 {
            let width_label_pos = Pos2::new(rect.center().x, rect.max.y + typography::XS);
            super::measurements::boxed_label(
                painter,
                egui::Align2::CENTER_CENTER,
                width_label_pos,
                width_label,
                chart.axis_text,
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
                chart.axis_text,
                label_bg,
            );
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
            for pair in self.arc_path().windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }

            painter.circle_filled(center, DESIGN_TOKENS.rounding.sm, color);
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Circular arc from the first anchor to the second through the third.
    pub(super) fn arc_path(&self) -> Vec<Pos2> {
        let &[a, b, ..] = self.points.as_slice() else {
            return Vec::new();
        };
        let Some(&c) = self.points.get(2) else {
            return arc_points(a, b);
        };
        let (u, v) = (b - a, c - a);
        let det = 2.0 * (u.x * v.y - u.y * v.x);
        if det.abs() < 1e-6 {
            return vec![a, c, b];
        }
        let center = a + egui::vec2(
            (u.length_sq() * v.y - v.length_sq() * u.y) / det,
            (u.x * v.length_sq() - v.x * u.length_sq()) / det,
        );
        let angle = |p: Pos2| (p.y - center.y).atan2(p.x - center.x);
        let start = angle(a);
        let end = (angle(b) - start).rem_euclid(std::f32::consts::TAU);
        let through = (angle(c) - start).rem_euclid(std::f32::consts::TAU);
        let sweep = if through <= end {
            end
        } else {
            end - std::f32::consts::TAU
        };
        (0..=64)
            .map(|i| {
                let theta = start + sweep * i as f32 / 64.0;
                center + center.distance(a) * egui::vec2(theta.cos(), theta.sin())
            })
            .collect()
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
        } else if self.points.len() == 1 {
            painter.circle_filled(self.points[0], DESIGN_TOKENS.rounding.sm, color);
        }
    }

    /// Arrow marker from its tail anchor to its head anchor
    pub(crate) fn render_arrow_marker(&self, painter: &egui::Painter, color: Color32) {
        self.render_arrow(painter, color, Stroke::new(self.stroke_width, color));
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

    pub(super) fn render_horizontal_mark(&self, painter: &egui::Painter, direction: f32) {
        let Some(&tip) = self.points.first() else {
            return;
        };
        let size = 14.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                tip,
                tip + egui::vec2(-direction * size, -size * 0.6),
                tip + egui::vec2(-direction * size, size * 0.6),
            ],
            self.color32(),
            Stroke::NONE,
        ));
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
/// towards, or at the top or bottom when it is vertical. Keep the defining
/// segment when its endpoint is already beyond that edge, so panning never reverses it.
pub(super) fn ray_end(start: Pos2, end: Pos2, rect: Rect) -> Pos2 {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    if dx.abs() > 1e-6 {
        let edge = if dx > 0.0 { rect.max.x } else { rect.min.x };
        let t = ((edge - start.x) / dx).max(1.0);
        start + (end - start) * t
    } else if dy.abs() > 1e-6 {
        let edge = if dy > 0.0 { rect.max.y } else { rect.min.y };
        start + (end - start) * ((edge - start.y) / dy).max(1.0)
    } else {
        start
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

    #[test]
    fn rectangle_size_labels_use_the_active_theme_pair() {
        use crate::{
            drawings::DrawingToolType,
            theme::{apply_to_egui, presets},
        };
        let ctx = egui::Context::default();
        let mut drawing = Drawing::new(1, DrawingToolType::Rect);
        drawing.add_point(Pos2::new(100.0, 100.0));
        drawing.add_point(Pos2::new(300.0, 300.0));
        for (_, theme) in presets() {
            apply_to_egui(&ctx, &theme);
            let output = ctx.run_ui(Default::default(), |ui| {
                drawing.render_rect_guides(ui.painter());
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.sections[0].format.color),
                    _ => None,
                })
                .collect();
            let fills = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == theme.chart.bg_axis)).count();
            output.drop_without_applying_deltas();
            assert_eq!(labels, vec![theme.chart.axis_text; 2]);
            assert_eq!(fills, 2);
        }
    }

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
        for (end, rect, want) in [
            (
                Pos2::new(600.0, 150.0),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 600.0)),
                Pos2::new(600.0, 150.0),
            ),
            (
                Pos2::new(500.0, 300.0),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1000.0, 50.0)),
                Pos2::new(500.0, 300.0),
            ),
            (start, rect, start),
        ] {
            assert_eq!(ray_end(start, end, rect), want);
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));

        // y = x / 2 - 150: left edge is -150, right edge is 350.
        use crate::drawings::{ArrowStyle, DrawingToolType};
        for tool in [DrawingToolType::Ray, DrawingToolType::ExtendedLine] {
            let defaults = Drawing::new(1, tool);
            assert!(defaults.extend_right);
            assert_eq!(defaults.extend_left, tool == DrawingToolType::ExtendedLine);
            for (left, right, ys) in [
                (false, false, [100., 300.]),
                (false, true, [100., 600.]),
                (true, false, [0., 300.]),
                (true, true, [0., 600.]),
            ] {
                let mut vertical = defaults.clone();
                vertical.points = vec![start, Pos2::new(500., 300.)];
                vertical.extend_left = left;
                vertical.extend_right = right;
                assert_eq!(
                    vertical.outline(rect).unwrap().lines,
                    [ys.map(|y| Pos2::new(500., y))]
                );
            }
            for (left, right, ends) in [
                (false, false, [(500., 100.), (600., 150.)]),
                (false, true, [(500., 100.), (1000., 350.)]),
                (true, false, [(0., -150.), (600., 150.)]),
                (true, true, [(0., -150.), (1000., 350.)]),
            ] {
                for reversed in [false, true] {
                    let mut drawing = defaults.clone();
                    drawing.points = vec![start, Pos2::new(600., 150.)];
                    if reversed {
                        drawing.points.reverse();
                    }
                    drawing.extend_left = left;
                    drawing.extend_right = right;
                    drawing.arrow_start_style = ArrowStyle::Square;
                    drawing.arrow_end_style = ArrowStyle::Square;
                    let expected = ends.map(|(x, y)| Pos2::new(x, y));
                    assert_eq!(drawing.outline(rect).unwrap().lines, [expected]);
                    let interaction = crate::drawings::services::DrawingInteraction::new();
                    for (point, hit) in [
                        (Pos2::new(400., 50.), left),
                        (Pos2::new(550., 125.), true),
                        (Pos2::new(800., 250.), right),
                    ] {
                        assert_eq!(
                            interaction.hit_test(point, std::slice::from_ref(&drawing), "1D"),
                            hit.then_some(1)
                        );
                    }
                    let output = egui::Context::default().run_ui(Default::default(), |ui| {
                        let view = ChartView {
                            instrument: None,
                            bars: &[],
                            executions: None,
                            revision: 0,
                            bar_to_x: &|x| x,
                            price_to_y: &|p| p as f32,
                            time_axis: true,
                        };
                        drawing.render(ui.painter(), rect, &view);
                    });
                    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                        egui::Shape::LineSegment { points, .. } if *points == expected)));
                    for tip in expected {
                        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                            egui::Shape::Rect(r) if r.rect.center() == tip)));
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }

    /// Extended zones paint and select the same box; the optional middle line stays thin and
    /// dashed even with a wide border, and does not require selection.
    #[test]
    fn rectangle_extensions_and_middle_line_follow_the_painted_zone() {
        use crate::drawings::{DrawingToolType, services::DrawingInteraction};
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500., 400.));
        for (left, right, min, max) in [
            (false, false, 100., 300.),
            (true, false, 0., 300.),
            (false, true, 100., 500.),
            (true, true, 0., 500.),
        ] {
            for middle in [false, true] {
                let mut drawing = Drawing::new(1, DrawingToolType::Rect);
                assert!(!drawing.channel_midline);
                drawing.points = vec![Pos2::new(300., 300.), Pos2::new(100., 100.)];
                drawing.extend_left = left;
                drawing.extend_right = right;
                drawing.channel_midline = middle;
                drawing.stroke_width = 5.;
                drawing.fill_color = Some([10, 20, 30, 40]);
                let expected = Rect::from_min_max(Pos2::new(min, 100.), Pos2::new(max, 300.));
                let outline = drawing.outline(pane).expect("rectangle outline");
                assert_eq!(Rect::from_points(&outline.areas[0]), expected);
                let interaction = DrawingInteraction::new();
                for (x, hit) in [(50., left), (200., true), (450., right)] {
                    assert_eq!(
                        interaction.hit_test(
                            Pos2::new(x, 150.),
                            std::slice::from_ref(&drawing),
                            "1D"
                        ),
                        hit.then_some(1)
                    );
                }
                let output = egui::Context::default().run_ui(Default::default(), |ui| {
                    let view = ChartView {
                        instrument: None,
                        bars: &[],
                        executions: None,
                        revision: 0,
                        bar_to_x: &|x| x,
                        price_to_y: &|p| p as f32,
                        time_axis: true,
                    };
                    drawing.render(ui.painter(), pane, &view);
                });
                assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::Rect(r) if r.rect == expected && r.fill == drawing.fill_color32().unwrap())));
                let dashes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match s.shape {
                        egui::Shape::LineSegment { points, stroke }
                            if points[0].y == 200. && points[1].y == 200. =>
                        {
                            Some((points, stroke))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(!dashes.is_empty(), middle);
                if middle {
                    assert_eq!(dashes.first().unwrap().0[0].x, min);
                    assert!(dashes.last().unwrap().0[1].x > max - 10.);
                    assert!(dashes.iter().all(|(p, s)| p[0].distance(p[1]) <= 8.
                        && s.width == stroke::HAIRLINE
                        && s.color == drawing.color32()));
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Size labels and centre guides belong to a selected rectangle; a deselected one has
    /// only its border and fill.
    #[test]
    fn rectangle_guides_show_only_while_selected() {
        use crate::drawings::{DrawingManager, DrawingToolType};
        let mut manager = DrawingManager::new();
        let mut drawing = Drawing::new(1, DrawingToolType::Rect);
        drawing.add_point(Pos2::new(100.0, 100.0));
        drawing.add_point(Pos2::new(300.0, 300.0));
        manager.drawings.push(drawing);
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let texts = |manager: &DrawingManager| {
            let ctx = egui::Context::default();
            let output = ctx.run_ui(Default::default(), |ui| {
                let view = ChartView {
                    instrument: None,
                    bars: &[],
                    executions: None,
                    revision: 0,
                    bar_to_x: &|x| x,
                    price_to_y: &|p| p as f32,
                    time_axis: true,
                };
                manager.render_all(ui.painter(), pane, &view);
            });
            let texts = (output.shapes.iter())
                .filter(|shape| matches!(shape.shape, egui::Shape::Text(_)))
                .count();
            output.drop_without_applying_deltas();
            texts
        };
        assert_eq!(texts(&manager), 0);
        manager.select(1);
        assert_eq!(texts(&manager), 2);
    }
}
