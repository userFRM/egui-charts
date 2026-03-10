//! Gann tool rendering implementations
//!
//! Includes: Gann fan, square, box, and fixed angle tools.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

impl Drawing {
    /// Render Gann Fan - Complete 9-angle Gann fan with bi-directional support
    pub(crate) fn render_gann_fan(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        // Gann fan angles with colors - 9 standard Gann angles
        // Above 1:1 are time-dominant, below are price-dominant
        let angles: [(f32, &str, Color32); 9] = [
            (8.0, "8×1", DESIGN_TOKENS.semantic.extended.bearish), // Steepest resistance
            (4.0, "4×1", DESIGN_TOKENS.semantic.indicators.ma),    // Strong resistance
            (3.0, "3×1", DESIGN_TOKENS.semantic.extended.favorite_gold), // Moderate resistance
            (2.0, "2×1", DESIGN_TOKENS.semantic.extended.caution), // Mild resistance
            (1.0, "1×1", DESIGN_TOKENS.semantic.indicators.bb_upper), // 45° balance line
            (0.5, "1×2", DESIGN_TOKENS.semantic.indicators.ema),   // Mild support
            (0.333, "1×3", DESIGN_TOKENS.semantic.drawings.fib_100), // Moderate support
            (0.25, "1×4", DESIGN_TOKENS.semantic.extended.bullish), // Strong support
            (0.125, "1×8", DESIGN_TOKENS.semantic.extended.bullish), // Strongest support
        ];

        let dy = end.y - start.y;
        let dx = end.x - start.x;

        // Determine primary direction (bullish or bearish)
        let is_bullish = dy < 0.0;
        let direction = if is_bullish { -1.0 } else { 1.0 };

        // Calculate base unit for scaling
        let base_unit = if dx.abs() > 0.001 {
            dy.abs() / dx.abs()
        } else {
            1.0
        };

        for (angle_ratio, label, angle_color) in &angles {
            // Calculate the end point for this Gann angle
            let extend_x = chart_rect.max.x - start.x;
            let angle_slope = base_unit / angle_ratio;
            let end_y = start.y + extend_x * angle_slope * direction;

            // Only draw if within reasonable bounds
            if end_y.is_finite()
                && end_y >= chart_rect.min.y - 200.0
                && end_y <= chart_rect.max.y + 200.0
            {
                let fan_end = Pos2::new(chart_rect.max.x, end_y);

                // Use color based on angle (1:1 is thicker)
                let stroke_width = if (*angle_ratio - 1.0).abs() < 0.01 {
                    stroke::THICK
                } else {
                    stroke::HAIRLINE
                };
                let stroke_color = if self.color[3] == 255 {
                    *angle_color
                } else {
                    Color32::from_rgba_unmultiplied(
                        angle_color.r(),
                        angle_color.g(),
                        angle_color.b(),
                        180,
                    )
                };

                painter.line_segment([start, fan_end], Stroke::new(stroke_width, stroke_color));

                // Draw label at chart edge with background
                if end_y >= chart_rect.min.y && end_y <= chart_rect.max.y {
                    let label_bg = DESIGN_TOKENS
                        .semantic
                        .extended
                        .chart_axis_bg
                        .gamma_multiply(0.85);
                    let gann_label_width = DESIGN_TOKENS.sizing.technical_labels.gann_label_width;
                    let gann_label_height = DESIGN_TOKENS.sizing.technical_labels.gann_label_height;
                    let label_rect = Rect::from_min_size(
                        Pos2::new(
                            chart_rect.max.x - gann_label_width - DESIGN_TOKENS.spacing.xs,
                            end_y - gann_label_height / 2.0,
                        ),
                        egui::Vec2::new(gann_label_width, gann_label_height),
                    );
                    painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                    painter.text(
                        label_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *label,
                        egui::FontId::proportional(typography::XS),
                        stroke_color,
                    );
                }
            }
        }

        // Draw the base trend line (connecting start to end)
        painter.line_segment([start, end], Stroke::new(self.stroke_width + 0.5, color));

        // Draw anchor points
        painter.circle_filled(start, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            start,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(end, DESIGN_TOKENS.rounding.md, color);
        painter.circle_stroke(
            end,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, Color32::WHITE),
        );
    }

    /// Render Gann Square - Square of 9 / Wheel of 24 style with cardinal and diagonal lines
    pub(crate) fn render_gann_square(&self, painter: &egui::Painter) {
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

        // Calculate square dimensions (use larger of width/height for 1:1 aspect)
        let width = (end.x - start.x).abs();
        let height = (end.y - start.y).abs();
        let size = width.max(height);

        // Determine square position
        let sign_x = if end.x >= start.x { 1.0 } else { -1.0 };
        let sign_y = if end.y >= start.y { 1.0 } else { -1.0 };
        let corner = Pos2::new(start.x + size * sign_x, start.y + size * sign_y);
        let square_rect = Rect::from_two_pos(start, corner);

        // Fill background
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 12);
        painter.rect_filled(square_rect, 0.0, fill_color);

        // Draw main square border
        painter.rect_stroke(
            square_rect,
            0.0,
            Stroke::new(self.stroke_width, color),
            StrokeKind::Inside,
        );

        let center = square_rect.center();
        let half_size = size / 2.0;

        // Draw 8 divisions grid (standard Gann)
        let divisions = 8;
        let step = size / divisions as f32;

        for i in 1..divisions {
            let offset = step * i as f32;
            let alpha = if i == 4 {
                120
            } else if i % 2 == 0 {
                80
            } else {
                50
            };
            let grid_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], alpha);

            // Vertical lines
            painter.vline(
                square_rect.min.x + offset,
                square_rect.y_range(),
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );

            // Horizontal lines
            painter.hline(
                square_rect.x_range(),
                square_rect.min.y + offset,
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );
        }

        // Draw cardinal and diagonal lines from center
        let line_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);

        // Main diagonals
        painter.line_segment(
            [square_rect.left_top(), square_rect.right_bottom()],
            Stroke::new(stroke::MEDIUM, line_color),
        );
        painter.line_segment(
            [square_rect.right_top(), square_rect.left_bottom()],
            Stroke::new(stroke::MEDIUM, line_color),
        );

        // Cardinal lines (horizontal and vertical through center)
        painter.hline(
            square_rect.x_range(),
            center.y,
            Stroke::new(stroke::MEDIUM, line_color),
        );
        painter.vline(
            center.x,
            square_rect.y_range(),
            Stroke::new(stroke::MEDIUM, line_color),
        );

        // Draw circle inscribed in square (Gann wheel)
        let circle_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
        painter.circle_stroke(
            center,
            half_size,
            Stroke::new(stroke::HAIRLINE, circle_color),
        );
        painter.circle_stroke(
            center,
            half_size * 0.5,
            Stroke::new(DESIGN_TOKENS.stroke.extra_thin, circle_color),
        );

        // Draw 45° angle markers
        let angle_45 = std::f32::consts::FRAC_PI_4;
        for i in 0..8 {
            let angle = angle_45 * i as f32;
            let inner_radius = half_size * 0.1;
            let outer_radius = half_size;

            let inner_point = Pos2::new(
                center.x + inner_radius * angle.cos(),
                center.y + inner_radius * angle.sin(),
            );
            let outer_point = Pos2::new(
                center.x + outer_radius * angle.cos(),
                center.y + outer_radius * angle.sin(),
            );

            painter.line_segment(
                [inner_point, outer_point],
                Stroke::new(
                    if i % 2 == 0 {
                        stroke::MEDIUM
                    } else {
                        stroke::HAIRLINE
                    },
                    line_color,
                ),
            );

            // Draw degree labels at corners
            if i % 2 == 0 {
                let label_pos = Pos2::new(
                    center.x + (outer_radius + typography::LG) * angle.cos(),
                    center.y + (outer_radius + typography::LG) * angle.sin(),
                );
                let degree_text = format!("{}°", (i as f32 * 45.0) as i32);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    &degree_text,
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }

        // Draw center point
        painter.circle_filled(center, DESIGN_TOKENS.rounding.md, color);
        painter.circle_stroke(
            center,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, Color32::WHITE),
        );

        // Draw anchor points
        painter.circle_filled(start, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            start,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
    }

    /// Render Gann Box - Time/Price grid with diagonal angle lines
    pub(crate) fn render_gann_box(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }

        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let rect = Rect::from_two_pos(p1, p2);

        // Fill with subtle background
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 15);
        painter.rect_filled(rect, 0.0, fill_color);

        // Draw main box border
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(self.stroke_width, color),
            StrokeKind::Inside,
        );

        // Draw Gann grid (8x8 divisions - standard for Gann analysis)
        let divisions = 8;
        let step_x = rect.width() / divisions as f32;
        let step_y = rect.height() / divisions as f32;

        // Draw vertical and horizontal grid lines
        for i in 1..divisions {
            let x = rect.min.x + step_x * i as f32;
            let y = rect.min.y + step_y * i as f32;

            // Use different opacity for 50% lines (quarter lines are lighter)
            let line_alpha = if i == 4 {
                120
            } else if i % 2 == 0 {
                80
            } else {
                40
            };
            let line_color = Color32::from_rgba_unmultiplied(
                self.color[0],
                self.color[1],
                self.color[2],
                line_alpha,
            );

            painter.vline(
                x,
                rect.y_range(),
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, line_color),
            );
            painter.hline(
                rect.x_range(),
                y,
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, line_color),
            );
        }

        // Draw main diagonals (1:1 angles)
        let diag_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);
        painter.line_segment(
            [rect.left_top(), rect.right_bottom()],
            Stroke::new(stroke::MEDIUM, diag_color),
        );
        painter.line_segment(
            [rect.right_top(), rect.left_bottom()],
            Stroke::new(stroke::MEDIUM, diag_color),
        );

        // Draw Gann angle lines from corners (2:1, 3:1, 4:1 angles)
        let angle_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 120);

        let angle_ratios: [f32; 3] = [2.0, 3.0, 4.0]; // 2:1, 3:1, 4:1

        for &ratio in &angle_ratios {
            // From top-left going down-right (time dominant)
            let end_x1 = rect.min.x + rect.width();
            let end_y1 = rect.min.y + rect.width() / ratio;
            if end_y1 <= rect.max.y {
                painter.line_segment(
                    [rect.left_top(), Pos2::new(end_x1, end_y1)],
                    Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
                );
            }

            // From top-left going down-right (price dominant)
            let end_x2 = rect.min.x + rect.height() / ratio;
            let end_y2 = rect.min.y + rect.height();
            if end_x2 <= rect.max.x {
                painter.line_segment(
                    [rect.left_top(), Pos2::new(end_x2, end_y2)],
                    Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
                );
            }
        }

        // Draw level labels on the left side (price levels)
        let label_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.8);

        for i in 0..=divisions {
            if i == 0 || i == divisions || i == divisions / 2 {
                let y = rect.min.y + step_y * i as f32;
                let pct = (100.0 * i as f32 / divisions as f32) as i32;
                let label_text = format!("{pct}%");
                let label_pos = Pos2::new(rect.min.x - 25.0, y);
                let label_rect =
                    Rect::from_center_size(label_pos, egui::Vec2::new(24.0, typography::MD));
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    &label_text,
                    egui::FontId::proportional(typography::XS),
                    label_color,
                );
            }
        }

        // Draw anchor points
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

        // Draw center point marker
        let center = rect.center();
        painter.circle_filled(center, DESIGN_TOKENS.rounding.sm, color);
    }

    /// Render Gann Fixed - Fixed 1:1 aspect ratio with price/time square
    pub(crate) fn render_gann_fixed(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }

        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Force 1:1 aspect ratio (use the larger dimension)
        let dx = (p2.x - p1.x).abs();
        let dy = (p2.y - p1.y).abs();
        let size = dx.max(dy);

        // Determine direction
        let sign_x = if p2.x >= p1.x { 1.0 } else { -1.0 };
        let sign_y = if p2.y >= p1.y { 1.0 } else { -1.0 };

        let corner = Pos2::new(p1.x + size * sign_x, p1.y + size * sign_y);
        let rect = Rect::from_two_pos(p1, corner);
        let center = rect.center();

        // Fill background
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 15);
        painter.rect_filled(rect, 0.0, fill_color);

        // Draw outer square
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(self.stroke_width, color),
            StrokeKind::Inside,
        );

        // Draw grid - 4x4 subdivision (16 cells, common for Gann Fixed)
        let divisions = 4;
        for i in 1..divisions {
            let t = i as f32 / divisions as f32;
            let alpha = if i == 2 { 120 } else { 80 }; // 50% line is brighter
            let grid_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], alpha);

            // Vertical lines
            let x = rect.min.x + rect.width() * t;
            painter.vline(
                x,
                rect.y_range(),
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );

            // Horizontal lines
            let y = rect.min.y + rect.height() * t;
            painter.hline(
                rect.x_range(),
                y,
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );
        }

        // Draw main diagonals (1:1 Gann angles)
        let diag_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);
        painter.line_segment(
            [rect.left_top(), rect.right_bottom()],
            Stroke::new(stroke::MEDIUM, diag_color),
        );
        painter.line_segment(
            [rect.right_top(), rect.left_bottom()],
            Stroke::new(stroke::MEDIUM, diag_color),
        );

        // Draw cardinal cross through center
        painter.hline(
            rect.x_range(),
            center.y,
            Stroke::new(stroke::HAIRLINE, diag_color),
        );
        painter.vline(
            center.x,
            rect.y_range(),
            Stroke::new(stroke::HAIRLINE, diag_color),
        );

        // Draw 45° angle lines from each corner to opposite edges
        let angle_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);

        // From midpoints to corners (creating 45° angles)
        let mid_top = Pos2::new(center.x, rect.min.y);
        let mid_bottom = Pos2::new(center.x, rect.max.y);
        let mid_left = Pos2::new(rect.min.x, center.y);
        let mid_right = Pos2::new(rect.max.x, center.y);

        painter.line_segment(
            [mid_top, rect.left_bottom()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_top, rect.right_bottom()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_bottom, rect.left_top()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_bottom, rect.right_top()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_left, rect.right_top()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_left, rect.right_bottom()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_right, rect.left_top()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );
        painter.line_segment(
            [mid_right, rect.left_bottom()],
            Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
        );

        // Draw aspect ratio label
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.8);
        let label_pos = Pos2::new(rect.max.x + DESIGN_TOKENS.rounding.lg, rect.min.y);
        let label_rect = Rect::from_min_size(label_pos, egui::Vec2::new(28.0, 14.0));
        painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
        painter.text(
            Pos2::new(label_pos.x + 14.0, label_pos.y + 7.0),
            egui::Align2::CENTER_CENTER,
            "1:1",
            egui::FontId::proportional(typography::XS),
            color,
        );

        // Draw center point
        painter.circle_filled(center, DESIGN_TOKENS.rounding.sm, color);

        // Draw anchor points
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(corner, DESIGN_TOKENS.rounding.md, color);
        painter.circle_stroke(
            corner,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::HAIRLINE, Color32::WHITE),
        );
    }
}
