//! Chart pattern rendering implementations
//!
//! Includes: XABCD, Cypher, Head & Shoulders, ABCD, Triangle, and Three Drives patterns.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Render XABCD Harmonic Pattern - Gartley, Bat, Butterfly, Crab with Fibonacci ratios
    pub(crate) fn render_xabcd_pattern(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let labels = ["X", "A", "B", "C", "D"];

        // Draw pattern fill for completed pattern (PRZ - Potential Reversal Zone)
        if self.points.len() >= 5 {
            // Fill the ABCD portion
            let fill_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);

            // Fill XAB
            painter.add(egui::epaint::PathShape::convex_polygon(
                vec![self.points[0], self.points[1], self.points[2]],
                fill_color,
                Stroke::NONE,
            ));

            // Fill BCD
            painter.add(egui::epaint::PathShape::convex_polygon(
                vec![self.points[2], self.points[3], self.points[4]],
                fill_color,
                Stroke::NONE,
            ));
        }

        // Draw connecting lines
        for i in 1..self.points.len() {
            painter.line_segment(
                [self.points[i - 1], self.points[i]],
                Stroke::new(self.stroke_width, color),
            );
        }

        // Draw XB and AD extension lines (dashed) for completed patterns
        if self.points.len() >= 5 {
            let dashed_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
            super::utils::draw_dashed_line(
                painter,
                self.points[0],
                self.points[2],
                Stroke::new(stroke::HAIRLINE, dashed_color),
                DESIGN_TOKENS.spacing.sm,
                DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
            );
            super::utils::draw_dashed_line(
                painter,
                self.points[1],
                self.points[4],
                Stroke::new(stroke::HAIRLINE, dashed_color),
                DESIGN_TOKENS.spacing.sm,
                DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
            );
        }

        // Draw labels and anchor points
        for (i, &point) in self.points.iter().enumerate() {
            // Determine label position (above or below based on local extrema)
            let is_peak = if i > 0 && i < self.points.len() - 1 {
                point.y < self.points[i - 1].y && point.y < self.points[i + 1].y
            } else if i == 0 {
                self.points.len() > 1 && point.y < self.points[1].y
            } else {
                self.points.len() > 1 && point.y < self.points[i - 1].y
            };

            let label_offset = if is_peak {
                -DESIGN_TOKENS.sizing.technical_labels.elliott_label_size
            } else {
                DESIGN_TOKENS.sizing.technical_labels.elliott_label_size
            };

            // Anchor point
            painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            // Label with background
            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.86);
                let label_rect = Rect::from_center_size(
                    label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
                        DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                    ),
                );
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::MD),
                    color,
                );
            }
        }

        // Calculate and display Fibonacci ratios for completed pattern
        if self.points.len() >= 4 {
            self.draw_harmonic_ratios(painter, color);
        }
    }

    /// Draw Fibonacci ratio labels for harmonic patterns
    fn draw_harmonic_ratios(&self, painter: &egui::Painter, color: Color32) {
        let ratio_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 180);

        // Calculate XAB ratio (B retracement of XA)
        if self.points.len() >= 3 {
            let xa_range = (self.points[1].y - self.points[0].y).abs();
            let xab_range = (self.points[2].y - self.points[0].y).abs();
            let xab_ratio = if xa_range > 0.001 {
                xab_range / xa_range
            } else {
                0.0
            };

            // Position ratio label at midpoint of XB line
            let mid_xb = Pos2::new(
                (self.points[0].x + self.points[2].x) / 2.0,
                (self.points[0].y + self.points[2].y) / 2.0,
            );
            painter.text(
                Pos2::new(mid_xb.x - DESIGN_TOKENS.spacing.xxxl, mid_xb.y),
                egui::Align2::RIGHT_CENTER,
                format!("{:.3}", xab_ratio),
                egui::FontId::proportional(typography::XS),
                ratio_color,
            );
        }

        // Calculate BCD ratio for completed pattern
        if self.points.len() >= 5 {
            let bc_range = (self.points[3].y - self.points[2].y).abs();
            let cd_range = (self.points[4].y - self.points[3].y).abs();
            let bcd_ratio = if bc_range > 0.001 {
                cd_range / bc_range
            } else {
                0.0
            };

            let mid_bd = Pos2::new(
                (self.points[2].x + self.points[4].x) / 2.0,
                (self.points[2].y + self.points[4].y) / 2.0,
            );
            painter.text(
                Pos2::new(mid_bd.x + DESIGN_TOKENS.spacing.xxxl, mid_bd.y),
                egui::Align2::LEFT_CENTER,
                format!("{:.3}", bcd_ratio),
                egui::FontId::proportional(typography::XS),
                ratio_color,
            );
        }
    }

    /// Render Cypher Pattern - Similar to XABCD with Cypher-specific ratios
    pub(crate) fn render_cypher_pattern(&self, painter: &egui::Painter) {
        // Cypher uses same structure as XABCD but with different ideal ratios
        // Ideal: B = 0.382-0.618, C = 1.13-1.414, D = 0.786
        self.render_xabcd_pattern(painter);

        // Add "Cypher" label if pattern is complete
        if self.points.len() >= 5 {
            let color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);

            let label_pos = Pos2::new(
                (self.points[0].x + self.points[4].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min)
                    - DESIGN_TOKENS.spacing.xxxl
                    - DESIGN_TOKENS.spacing.sm,
            );
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86);
            let label_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Cypher",
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }

    /// Render Head and Shoulders Pattern - With neckline and target projection
    pub(crate) fn render_head_and_shoulders(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Labels for H&S points: LS (Left Shoulder), Head, RS (Right Shoulder)
        let labels = ["", "LS", "", "Head", "", "RS", ""];

        // Draw pattern fill
        if self.points.len() >= 5 {
            let fill_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);
            painter.add(egui::epaint::PathShape::convex_polygon(
                self.points.clone(),
                fill_color,
                Stroke::NONE,
            ));
        }

        // Draw connecting lines
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment(
                    [self.points[i - 1], point],
                    Stroke::new(self.stroke_width, color),
                );
            }

            // Anchor points
            painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            // Draw labels for shoulder/head points
            if i < labels.len() && !labels[i].is_empty() {
                let is_peak = i > 0
                    && i < self.points.len() - 1
                    && point.y < self.points[i - 1].y
                    && point.y < self.points[i + 1].y;
                let label_offset = if is_peak {
                    -DESIGN_TOKENS.spacing.xxxl
                } else {
                    DESIGN_TOKENS.spacing.xxxl
                };

                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.86);
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_rect = Rect::from_center_size(
                    label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.hs_label_width,
                        DESIGN_TOKENS.sizing.technical_labels.gann_label_height,
                    ),
                );
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }

        // Draw neckline and target projection for complete pattern
        if self.points.len() >= 5 {
            // Neckline connects the troughs (points 1 and len-2)
            let neckline_start = self.points[1];
            let neckline_end = self.points[self.points.len() - 2];

            let neckline_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);

            // Extend neckline
            let dx = neckline_end.x - neckline_start.x;
            let dy = neckline_end.y - neckline_start.y;
            let extend = DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                + DESIGN_TOKENS.spacing.xxxl;
            let extended_end = if dx.abs() > 0.001 {
                Pos2::new(neckline_end.x + extend, neckline_end.y + dy / dx * extend)
            } else {
                Pos2::new(neckline_end.x, neckline_end.y + extend)
            };

            painter.line_segment(
                [neckline_start, extended_end],
                Stroke::new(stroke::MEDIUM, neckline_color),
            );

            // Calculate and draw target projection
            let head_point = self.points[3]; // Assuming head is point 3
            let neckline_y_at_head = neckline_start.y + dy / dx * (head_point.x - neckline_start.x);
            let pattern_height = (head_point.y - neckline_y_at_head).abs();

            // Target is same distance below neckline
            let is_top = head_point.y < neckline_y_at_head;
            let target_y = if is_top {
                neckline_end.y + pattern_height
            } else {
                neckline_end.y - pattern_height
            };

            // Draw target line (dashed)
            let target_color = DESIGN_TOKENS.semantic.indicators.ma.gamma_multiply(0.7); // Orange
            super::utils::draw_dashed_line(
                painter,
                Pos2::new(neckline_end.x, target_y),
                Pos2::new(
                    neckline_end.x + DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x,
                    target_y,
                ),
                Stroke::new(stroke::HAIRLINE, target_color),
                DESIGN_TOKENS.spacing.md + DESIGN_TOKENS.spacing.xs,
                DESIGN_TOKENS.spacing.sm,
            );

            // Target label
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86);
            let target_label_pos = Pos2::new(
                neckline_end.x
                    + DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                    + DESIGN_TOKENS.spacing.lg,
                target_y,
            );
            let target_rect = Rect::from_center_size(
                target_label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.line_label_width
                        + DESIGN_TOKENS.spacing.sm,
                    DESIGN_TOKENS.sizing.technical_labels.gann_label_height,
                ),
            );
            painter.rect_filled(target_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                target_label_pos,
                egui::Align2::CENTER_CENTER,
                "Target",
                egui::FontId::proportional(typography::XS),
                target_color,
            );
        }
    }

    /// Render ABCD Pattern - With AB=CD validation and Fibonacci overlay
    pub(crate) fn render_abcd_pattern(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let labels = ["A", "B", "C", "D"];

        // Draw fill for completed pattern
        if self.points.len() >= 4 {
            let fill_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
            painter.add(egui::epaint::PathShape::convex_polygon(
                self.points[0..4].to_vec(),
                fill_color,
                Stroke::NONE,
            ));
        }

        // Draw connecting lines
        for i in 1..self.points.len() {
            painter.line_segment(
                [self.points[i - 1], self.points[i]],
                Stroke::new(self.stroke_width, color),
            );
        }

        // Draw AC line (dashed) for complete pattern
        if self.points.len() >= 4 {
            let dashed_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
            super::utils::draw_dashed_line(
                painter,
                self.points[0],
                self.points[2],
                Stroke::new(stroke::HAIRLINE, dashed_color),
                DESIGN_TOKENS.spacing.sm,
                DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
            );
            super::utils::draw_dashed_line(
                painter,
                self.points[1],
                self.points[3],
                Stroke::new(stroke::HAIRLINE, dashed_color),
                DESIGN_TOKENS.spacing.sm,
                DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
            );
        }

        // Draw anchor points and labels
        for (i, &point) in self.points.iter().enumerate() {
            let is_peak = if i > 0 && i < self.points.len() - 1 {
                point.y < self.points[i - 1].y && point.y < self.points[i + 1].y
            } else if i == 0 {
                self.points.len() > 1 && point.y < self.points[1].y
            } else {
                self.points.len() > 1 && point.y < self.points[i - 1].y
            };

            let label_offset = if is_peak {
                -DESIGN_TOKENS.sizing.technical_labels.elliott_label_size
            } else {
                DESIGN_TOKENS.sizing.technical_labels.elliott_label_size
            };

            painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.86);
                let label_rect = Rect::from_center_size(
                    label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
                        DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                    ),
                );
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::MD),
                    color,
                );
            }
        }

        // Calculate and display AB=CD ratio
        if self.points.len() >= 4 {
            let ab_length = ((self.points[1].x - self.points[0].x).powi(2)
                + (self.points[1].y - self.points[0].y).powi(2))
            .sqrt();
            let cd_length = ((self.points[3].x - self.points[2].x).powi(2)
                + (self.points[3].y - self.points[2].y).powi(2))
            .sqrt();
            let ratio = if ab_length > 0.001 {
                cd_length / ab_length
            } else {
                0.0
            };

            // Ratio label
            let ratio_pos = Pos2::new(
                (self.points[0].x + self.points[3].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    + DESIGN_TOKENS.spacing.xxxl
                    + DESIGN_TOKENS.spacing.sm,
            );

            // Color based on how close to 1.0 (perfect AB=CD)
            let ratio_color = if (ratio - 1.0).abs() < 0.05 {
                DESIGN_TOKENS.semantic.extended.bullish // Green - good
            } else if (ratio - 1.0).abs() < 0.15 {
                DESIGN_TOKENS.semantic.extended.favorite_gold // Amber - acceptable
            } else {
                DESIGN_TOKENS.semantic.indicators.ma // Orange - weak
            };

            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86);
            let label_rect = Rect::from_center_size(
                ratio_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                ratio_pos,
                egui::Align2::CENTER_CENTER,
                format!("CD/AB: {:.2}", ratio),
                egui::FontId::proportional(typography::XS),
                ratio_color,
            );
        }
    }

    /// Render Triangle Pattern - With apex calculation and breakout projection
    pub(crate) fn render_triangle_pattern(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw fill for completed triangle
        if self.points.len() >= 3 {
            let fill_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
            painter.add(egui::epaint::PathShape::convex_polygon(
                self.points.clone(),
                fill_color,
                Stroke::NONE,
            ));
        }

        // Draw connecting lines
        for (i, &point) in self.points.iter().enumerate() {
            if i > 0 {
                painter.line_segment(
                    [self.points[i - 1], point],
                    Stroke::new(self.stroke_width, color),
                );
            }

            painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );
        }

        // Close the triangle for 3+ points
        if self.points.len() >= 3 {
            painter.line_segment(
                [self.points[self.points.len() - 1], self.points[0]],
                Stroke::new(self.stroke_width, color),
            );

            // Calculate apex (extension of converging lines)
            // For ascending/descending triangles, project the trendlines to find intersection
            if self.points.len() >= 4 {
                // Upper trendline from peaks
                let upper_start = if self.points[0].y < self.points[2].y {
                    self.points[0]
                } else {
                    self.points[2]
                };
                let upper_end = if self.points[1].y < self.points[3].y {
                    self.points[1]
                } else {
                    self.points[3]
                };

                // Draw extended trendlines (dashed)
                let extend_color = Color32::from_rgba_unmultiplied(
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    80,
                );

                // Extend upper line
                let dx = upper_end.x - upper_start.x;
                let dy = upper_end.y - upper_start.y;
                if dx.abs() > 0.001 {
                    let slope = dy / dx;
                    let extension = DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                        + DESIGN_TOKENS.spacing.xxxl;
                    let extended =
                        Pos2::new(upper_end.x + extension, upper_end.y + slope * extension);
                    super::utils::draw_dashed_line(
                        painter,
                        upper_end,
                        extended,
                        Stroke::new(stroke::HAIRLINE, extend_color),
                        DESIGN_TOKENS.spacing.sm + DESIGN_TOKENS.spacing.hairline,
                        DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
                    );
                }
            }

            // Draw numbered labels
            for (i, &point) in self.points.iter().enumerate() {
                let label_offset = if i % 2 == 0 {
                    -DESIGN_TOKENS.sizing.technical_labels.elliott_label_size
                } else {
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size
                };
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.86);
                let label_rect = Rect::from_center_size(
                    label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                        DESIGN_TOKENS.sizing.technical_labels.gann_label_height,
                    ),
                );
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }
    }

    /// Render Three Drives Pattern - With ratio validation and reversal zone
    pub(crate) fn render_three_drives_pattern(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw fill for complete pattern
        if self.points.len() >= 7 {
            let fill_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);
            painter.add(egui::epaint::PathShape::convex_polygon(
                self.points.clone(),
                fill_color,
                Stroke::NONE,
            ));
        }

        // Draw connecting lines
        for i in 1..self.points.len() {
            painter.line_segment(
                [self.points[i - 1], self.points[i]],
                Stroke::new(self.stroke_width, color),
            );
        }

        // Draw points and labels
        let labels = ["1", "2", "3", "4", "5", "6", "7"];
        for (i, &point) in self.points.iter().enumerate() {
            // Determine if peak or trough for label positioning
            let is_peak = if i > 0 && i < self.points.len() - 1 {
                point.y < self.points[i - 1].y && point.y < self.points[i + 1].y
            } else {
                i % 2 == 1 // Odd points are typically peaks in three drives
            };

            let label_offset = if is_peak {
                -DESIGN_TOKENS.spacing.xxxl
            } else {
                DESIGN_TOKENS.spacing.xxxl
            };

            painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.86);
                let label_rect = Rect::from_center_size(
                    label_pos,
                    egui::Vec2::new(
                        DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
                        DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                    ),
                );
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::SM),
                    color,
                );
            }
        }

        // Draw drive labels (Drive 1, 2, 3) at the peaks
        if self.points.len() >= 5 {
            let drive_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);

            // Identify drive peaks (typically points 1, 3, 5 for bullish)
            let drive_points = [1usize, 3, 5];
            let drive_labels = ["Drive 1", "Drive 2", "Drive 3"];

            for (di, &pi) in drive_points.iter().enumerate() {
                if pi < self.points.len() {
                    let drive_pos = Pos2::new(
                        self.points[pi].x,
                        self.points[pi].y - DESIGN_TOKENS.sizing.technical_labels.hs_label_width,
                    );
                    painter.text(
                        drive_pos,
                        egui::Align2::CENTER_CENTER,
                        drive_labels[di],
                        egui::FontId::proportional(typography::XS),
                        drive_color,
                    );
                }
            }
        }

        // Draw reversal zone if pattern is complete
        if self.points.len() >= 7 {
            let reversal_color = DESIGN_TOKENS
                .semantic
                .extended
                .bullish
                .gamma_multiply(60_f32 / 255.0); // Teal with alpha
            let last_point = self.points[6];
            let zone_height = DESIGN_TOKENS.spacing.section_lg + DESIGN_TOKENS.spacing.lg;
            let zone_width = DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                - DESIGN_TOKENS.spacing.xxxl;

            let zone_rect = Rect::from_center_size(
                Pos2::new(
                    last_point.x + DESIGN_TOKENS.spacing.xxxl + DESIGN_TOKENS.spacing.sm,
                    last_point.y,
                ),
                egui::Vec2::new(zone_width, zone_height),
            );
            painter.rect_filled(zone_rect, DESIGN_TOKENS.rounding.md, reversal_color);

            painter.text(
                zone_rect.center(),
                egui::Align2::CENTER_CENTER,
                "PRZ",
                egui::FontId::proportional(typography::XS),
                DESIGN_TOKENS.semantic.extended.bullish,
            );
        }
    }
}
