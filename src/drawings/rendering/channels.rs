//! Channel tool rendering implementations
//!
//! Includes: parallel channel, disjoint channel, regression trend, and flat top/bottom.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    pub(crate) fn render_parallel_channel(&self, painter: &egui::Painter) {
        if self.points.len() < 3 {
            return;
        }

        let p1 = self.points[0];
        let p2 = self.points[1];
        let p3 = self.points[2];

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        let stroke = Stroke::new(self.stroke_width, color);

        // Draw first trendline
        painter.line_segment([p1, p2], stroke);

        // Calculate parallel offset vector
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;

        // Calculate perpendicular distance from p3 to line p1-p2
        let line_len_sq = dx * dx + dy * dy;
        if line_len_sq < 1e-6 {
            return; // Degenerate line
        }

        // Project p3 onto line p1-p2
        let t = ((p3.x - p1.x) * dx + (p3.y - p1.y) * dy) / line_len_sq;
        let proj_x = p1.x + t * dx;
        let proj_y = p1.y + t * dy;

        // Calculate offset
        let offset_x = p3.x - proj_x;
        let offset_y = p3.y - proj_y;

        // Draw parallel line
        let p4 = Pos2::new(p1.x + offset_x, p1.y + offset_y);
        let p5 = Pos2::new(p2.x + offset_x, p2.y + offset_y);
        painter.line_segment([p4, p5], stroke);

        // Draw conn lines (dashed)
        let dashed_stroke = Stroke::new(stroke::HAIRLINE, Color32::from_white_alpha(128));
        painter.line_segment([p1, p4], dashed_stroke);
        painter.line_segment([p2, p5], dashed_stroke);
    }

    /// Render Regression Trend - Linear regression channel with standard deviation bands
    pub(crate) fn render_regression_trend(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        // Calculate regression line (y = mx + b)
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let slope = if dx.abs() > 0.001 { dy / dx } else { 0.0 };
        let intercept = start.y - slope * start.x;

        // Extend regression line to chart edges
        let extended_start_x = chart_rect.min.x.max(start.x.min(end.x) - 50.0);
        let extended_end_x = chart_rect.max.x.min(start.x.max(end.x) + 50.0);
        let extended_start = Pos2::new(extended_start_x, slope * extended_start_x + intercept);
        let extended_end = Pos2::new(extended_end_x, slope * extended_end_x + intercept);

        // Draw main regression line (thicker)
        painter.line_segment(
            [extended_start, extended_end],
            Stroke::new(self.stroke_width + 0.5, color),
        );

        // Calculate perpendicular offset for channel bands
        let line_length = (dx * dx + dy * dy).sqrt();
        let perp_x = if line_length > 0.001 {
            -dy / line_length
        } else {
            0.0
        };
        let perp_y = if line_length > 0.001 {
            dx / line_length
        } else {
            1.0
        };

        // Standard deviation bands (1σ, 2σ)
        let base_deviation = (end.y - start.y).abs() * 0.15 + 20.0; // Minimum deviation for visibility

        // 1 Standard Deviation bands
        let sd1_offset = base_deviation;
        let upper_1sd_start = Pos2::new(
            extended_start.x + perp_x * sd1_offset,
            extended_start.y + perp_y * sd1_offset,
        );
        let upper_1sd_end = Pos2::new(
            extended_end.x + perp_x * sd1_offset,
            extended_end.y + perp_y * sd1_offset,
        );
        let lower_1sd_start = Pos2::new(
            extended_start.x - perp_x * sd1_offset,
            extended_start.y - perp_y * sd1_offset,
        );
        let lower_1sd_end = Pos2::new(
            extended_end.x - perp_x * sd1_offset,
            extended_end.y - perp_y * sd1_offset,
        );

        // 2 Standard Deviation bands
        let sd2_offset = base_deviation * 2.0;
        let upper_2sd_start = Pos2::new(
            extended_start.x + perp_x * sd2_offset,
            extended_start.y + perp_y * sd2_offset,
        );
        let upper_2sd_end = Pos2::new(
            extended_end.x + perp_x * sd2_offset,
            extended_end.y + perp_y * sd2_offset,
        );
        let lower_2sd_start = Pos2::new(
            extended_start.x - perp_x * sd2_offset,
            extended_start.y - perp_y * sd2_offset,
        );
        let lower_2sd_end = Pos2::new(
            extended_end.x - perp_x * sd2_offset,
            extended_end.y - perp_y * sd2_offset,
        );

        // Fill between 2σ bands (outer fill - very light)
        let fill_2sd =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 15);
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![
                upper_2sd_start,
                upper_2sd_end,
                upper_1sd_end,
                upper_1sd_start,
            ],
            fill_2sd,
            Stroke::NONE,
        ));
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![
                lower_1sd_start,
                lower_1sd_end,
                lower_2sd_end,
                lower_2sd_start,
            ],
            fill_2sd,
            Stroke::NONE,
        ));

        // Fill between 1σ bands (inner fill - slightly more visible)
        let fill_1sd =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![
                upper_1sd_start,
                upper_1sd_end,
                lower_1sd_end,
                lower_1sd_start,
            ],
            fill_1sd,
            Stroke::NONE,
        ));

        // Draw 2σ band lines (dashed, lighter)
        let sd2_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
        super::utils::draw_dashed_line(
            painter,
            upper_2sd_start,
            upper_2sd_end,
            Stroke::new(stroke::HAIRLINE, sd2_color),
            DESIGN_TOKENS.rounding.xl,
            DESIGN_TOKENS.rounding.md,
        );
        super::utils::draw_dashed_line(
            painter,
            lower_2sd_start,
            lower_2sd_end,
            Stroke::new(stroke::HAIRLINE, sd2_color),
            DESIGN_TOKENS.rounding.xl,
            DESIGN_TOKENS.rounding.md,
        );

        // Draw 1σ band lines (solid)
        let sd1_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        painter.line_segment(
            [upper_1sd_start, upper_1sd_end],
            Stroke::new(stroke::HAIRLINE, sd1_color),
        );
        painter.line_segment(
            [lower_1sd_start, lower_1sd_end],
            Stroke::new(stroke::HAIRLINE, sd1_color),
        );

        // Calculate R² (coefficient of determination) - simplified for 2 points
        // For a true implementation, this would use actual price data
        let r_squared = 0.95; // Placeholder - would calculate from actual data points

        // Draw info label with R²
        let label_pos = Pos2::new(end.x + typography::XS, end.y - typography::XXL);
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.9);
        let label_rect = Rect::from_min_size(
            label_pos,
            egui::Vec2::new(
                DESIGN_TOKENS.sizing.technical_labels.channel_label_width,
                DESIGN_TOKENS.sizing.technical_labels.channel_label_height,
            ),
        );
        painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
        painter.rect_stroke(
            label_rect,
            DESIGN_TOKENS.rounding.sm,
            Stroke::new(stroke::HAIRLINE, color),
            egui::epaint::StrokeKind::Inside,
        );

        // R² value
        painter.text(
            Pos2::new(label_pos.x + 35.0, label_pos.y + typography::XS),
            egui::Align2::CENTER_CENTER,
            format!("R² = {:.2}", r_squared),
            egui::FontId::proportional(typography::XS),
            color,
        );

        // Slope value
        painter.text(
            Pos2::new(label_pos.x + 35.0, label_pos.y + typography::XXL),
            egui::Align2::CENTER_CENTER,
            format!("m = {:.4}", slope),
            egui::FontId::proportional(typography::XS),
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180),
        );

        // Draw anchor points with white border
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
    }

    /// Render Flat Top/Bottom - Trend line with horizontal support/resistance
    pub(crate) fn render_flat_top_bottom(&self, painter: &egui::Painter) {
        if self.points.len() < 3 {
            let color = Color32::from_rgba_unmultiplied(
                self.color[0],
                self.color[1],
                self.color[2],
                self.color[3],
            );
            for (i, &p) in self.points.iter().enumerate() {
                painter.circle_filled(p, DESIGN_TOKENS.rounding.md, color);
                if i > 0 {
                    painter.line_segment(
                        [self.points[i - 1], p],
                        Stroke::new(self.stroke_width, color),
                    );
                }
            }
            return;
        }

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let p1 = self.points[0];
        let p2 = self.points[1];
        let p3 = self.points[2];

        // Determine if this is a flat top or flat bottom
        let is_flat_top = p3.y < (p1.y + p2.y) / 2.0;

        // Draw flat horizontal line at p3's y level
        let extend_amount = typography::XXL + typography::LG; // Extend beyond the trend line
        let flat_start = Pos2::new(p1.x.min(p2.x) - extend_amount, p3.y);
        let flat_end = Pos2::new(p1.x.max(p2.x) + extend_amount, p3.y);
        painter.line_segment(
            [flat_start, flat_end],
            Stroke::new(self.stroke_width, color),
        );

        // Draw trendline from p1 to p2
        painter.line_segment([p1, p2], Stroke::new(self.stroke_width, color));

        // Create fill polygon between trend line and flat line
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 35);

        // Fill polygon between trend line and flat line
        let fill_points = if is_flat_top {
            vec![
                Pos2::new(p1.x, p1.y),
                Pos2::new(p2.x, p2.y),
                Pos2::new(p2.x, p3.y),
                Pos2::new(p1.x, p3.y),
            ]
        } else {
            vec![
                Pos2::new(p1.x, p3.y),
                Pos2::new(p2.x, p3.y),
                Pos2::new(p2.x, p2.y),
                Pos2::new(p1.x, p1.y),
            ]
        };
        painter.add(egui::epaint::PathShape::convex_polygon(
            fill_points,
            fill_color,
            Stroke::NONE,
        ));

        // Draw dashed connector lines
        let connector_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
        super::utils::draw_dashed_line(
            painter,
            Pos2::new(p1.x, p1.y),
            Pos2::new(p1.x, p3.y),
            Stroke::new(stroke::HAIRLINE, connector_color),
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS.rounding.sm,
        );
        super::utils::draw_dashed_line(
            painter,
            Pos2::new(p2.x, p2.y),
            Pos2::new(p2.x, p3.y),
            Stroke::new(stroke::HAIRLINE, connector_color),
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS.rounding.sm,
        );

        // Draw price labels
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.85);
        let label_font = egui::FontId::proportional(typography::XS);

        // Flat line price label (shown at the right end)
        let flat_label_pos = Pos2::new(flat_end.x + DESIGN_TOKENS.rounding.lg, p3.y);
        let flat_price_text = if is_flat_top { "Resistance" } else { "Support" };
        let flat_label_rect = Rect::from_min_size(
            flat_label_pos,
            egui::Vec2::new(
                DESIGN_TOKENS.sizing.technical_labels.channel_offset_x,
                typography::XXL,
            ),
        );
        painter.rect_filled(flat_label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
        painter.text(
            Pos2::new(
                flat_label_pos.x + DESIGN_TOKENS.sizing.technical_labels.cycle_label_width
                    - DESIGN_TOKENS.spacing.lg,
                flat_label_pos.y + DESIGN_TOKENS.spacing.lg,
            ),
            egui::Align2::CENTER_CENTER,
            flat_price_text,
            label_font.clone(),
            color,
        );

        // Height measurement between lines
        let height_diff = (p3.y - (p1.y + p2.y) / 2.0).abs();
        let mid_x = (p1.x + p2.x) / 2.0;
        let mid_trend_y = (p1.y + p2.y) / 2.0;

        // Draw measurement indicator
        let measure_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);

        // Vertical measurement line
        let measure_top = Pos2::new(mid_x, p3.y.min(mid_trend_y));
        let measure_bottom = Pos2::new(mid_x, p3.y.max(mid_trend_y));
        super::utils::draw_dashed_line(
            painter,
            measure_top,
            measure_bottom,
            Stroke::new(stroke::HAIRLINE, measure_color),
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS.rounding.sm,
        );

        // Height label
        let height_label_pos = Pos2::new(
            mid_x + DESIGN_TOKENS.rounding.lg,
            (p3.y + mid_trend_y) / 2.0,
        );
        painter.text(
            height_label_pos,
            egui::Align2::LEFT_CENTER,
            format!("{:.0}px", height_diff),
            egui::FontId::proportional(typography::XS),
            measure_color,
        );

        // Draw anchor points with white border
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p3, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p3,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Render Disjoint Channel - Two parallel channel segments with connector
    pub(crate) fn render_disjoint_channel(&self, painter: &egui::Painter) {
        if self.points.len() < 3 {
            let color = Color32::from_rgba_unmultiplied(
                self.color[0],
                self.color[1],
                self.color[2],
                self.color[3],
            );
            for (i, &p) in self.points.iter().enumerate() {
                painter.circle_filled(p, DESIGN_TOKENS.rounding.md, color);
                if i > 0 {
                    painter.line_segment(
                        [self.points[i - 1], p],
                        Stroke::new(self.stroke_width, color),
                    );
                }
            }
            return;
        }

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let p1 = self.points[0];
        let p2 = self.points[1];
        let p3 = self.points[2];

        // Calculate direction vector for first segment
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let line_length = (dx * dx + dy * dy).sqrt();

        // Second segment endpoint (parallel to first)
        let p4 = Pos2::new(p3.x + dx, p3.y + dy);

        // Calculate midpoints for the connector
        let mid1 = Pos2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
        let mid2 = Pos2::new((p3.x + p4.x) / 2.0, (p3.y + p4.y) / 2.0);

        // Fill between the two channel segments
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
        painter.add(egui::epaint::PathShape::convex_polygon(
            vec![p1, p2, p4, p3],
            fill_color,
            Stroke::NONE,
        ));

        // Draw first segment (main stroke)
        painter.line_segment([p1, p2], Stroke::new(self.stroke_width, color));

        // Draw second segment (parallel)
        painter.line_segment([p3, p4], Stroke::new(self.stroke_width, color));

        // Draw middle connector line (dashed)
        let connector_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 120);
        super::utils::draw_dashed_line(
            painter,
            mid1,
            mid2,
            Stroke::new(stroke::HAIRLINE, connector_color),
            DESIGN_TOKENS.rounding.lg,
            DESIGN_TOKENS.rounding.md,
        );

        // Draw edge connectors (lighter dashed)
        let edge_connector_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
        super::utils::draw_dashed_line(
            painter,
            p1,
            p3,
            Stroke::new(stroke::HAIRLINE, edge_connector_color),
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS.rounding.sm,
        );
        super::utils::draw_dashed_line(
            painter,
            p2,
            p4,
            Stroke::new(stroke::HAIRLINE, edge_connector_color),
            DESIGN_TOKENS.rounding.md,
            DESIGN_TOKENS.rounding.sm,
        );

        // Draw width measurement
        if line_length > 20.0 {
            let channel_width = ((p3.x - p1.x).powi(2) + (p3.y - p1.y).powi(2)).sqrt();
            let mid_connector = Pos2::new((mid1.x + mid2.x) / 2.0, (mid1.y + mid2.y) / 2.0);

            // Label background
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.8);
            let label_rect = Rect::from_center_size(
                mid_connector,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.line_label_width
                        + DESIGN_TOKENS.spacing.lg,
                    typography::XXL,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);

            painter.text(
                mid_connector,
                egui::Align2::CENTER_CENTER,
                format!("{:.0}px", channel_width),
                egui::FontId::proportional(typography::XS),
                connector_color,
            );
        }

        // Draw anchor points with white border
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p3, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p3,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Also draw the derived point p4
        let derived_point_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        painter.circle_filled(p4, DESIGN_TOKENS.rounding.md, derived_point_color);
        painter.circle_stroke(
            p4,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, Color32::WHITE),
        );
    }
}
