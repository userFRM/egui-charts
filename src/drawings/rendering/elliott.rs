//! Elliott Wave tool rendering implementations
//!
//! Includes: impulse, correction, triangle, double combo, and triple combo waves.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Render Elliott Impulse Wave - 5-wave motive pattern (0-1-2-3-4-5)
    pub(crate) fn render_elliott_impulse(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Wave labels with degree notation (can be customized)
        let labels = ["0", "1", "2", "3", "4", "5"];

        // Draw fill for motive waves (1, 3, 5) vs corrective waves (2, 4)
        if self.points.len() >= 3 {
            // Wave 1 fill (bullish - green tint)
            let motive_fill = DESIGN_TOKENS
                .semantic
                .extended
                .bullish
                .gamma_multiply(20_f32 / 255.0);
            // Wave 2 fill (corrective - red tint)
            let corrective_fill = DESIGN_TOKENS
                .semantic
                .extended
                .bearish
                .gamma_multiply(15_f32 / 255.0);

            for i in 1..self.points.len() {
                let fill = if i % 2 == 1 {
                    motive_fill
                } else {
                    corrective_fill
                };

                // Draw subtle highlight under each wave
                if i >= 1 {
                    let start = self.points[i - 1];
                    let end = self.points[i];
                    let mid_x = (start.x + end.x) / 2.0;
                    let mid_y = (start.y + end.y) / 2.0;

                    painter.add(egui::epaint::PathShape::convex_polygon(
                        vec![start, end, Pos2::new(mid_x, mid_y + 5.0)],
                        fill,
                        Stroke::NONE,
                    ));
                }
            }
        }

        // Draw connecting lines with varying thickness
        for i in 1..self.points.len() {
            // Motive waves (1, 3, 5) are drawn thicker
            let thickness = if i % 2 == 1 {
                self.stroke_width + 0.5
            } else {
                self.stroke_width
            };

            painter.line_segment(
                [self.points[i - 1], self.points[i]],
                Stroke::new(thickness, color),
            );
        }

        // Draw Fibonacci extensions between key waves if we have enough points
        if self.points.len() >= 4 {
            // Wave 1 length for Fibonacci projections
            let wave1_length = ((self.points[2].x - self.points[1].x).powi(2)
                + (self.points[2].y - self.points[1].y).powi(2))
            .sqrt();

            // Show 1.618 extension from wave 2 end (common wave 3 target)
            if self.points.len() >= 3 {
                let fib_color = DESIGN_TOKENS.semantic.indicators.ma.gamma_multiply(0.47); // Orange with alpha
                let wave2_end = self.points[2];
                let is_up = self.points[1].y > self.points[0].y;
                let extension_y = if is_up {
                    wave2_end.y - wave1_length * 1.618
                } else {
                    wave2_end.y + wave1_length * 1.618
                };

                super::utils::draw_dashed_line(
                    painter,
                    Pos2::new(wave2_end.x, wave2_end.y),
                    Pos2::new(wave2_end.x + 40.0, extension_y),
                    Stroke::new(stroke::HAIRLINE, fib_color),
                    3.0,
                    2.0,
                );

                painter.text(
                    Pos2::new(wave2_end.x + 45.0, extension_y),
                    egui::Align2::LEFT_CENTER,
                    "1.618",
                    egui::FontId::proportional(typography::XS),
                    fib_color,
                );
            }
        }

        // Draw points and labels
        for (i, &point) in self.points.iter().enumerate() {
            // Determine label position based on wave direction
            let is_up = if i > 0 {
                point.y < self.points[i - 1].y
            } else {
                true
            };
            let label_offset = if is_up {
                -DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.md
            } else {
                DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.md
            };

            // Draw anchor point
            painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                point,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );

            // Draw label with background
            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9);

                // Circle background for wave numbers
                painter.circle_filled(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::SM),
                    color,
                );
            }
        }

        // Draw "Impulse" label for completed pattern
        if self.points.len() >= 6 {
            let label_pos = Pos2::new(
                (self.points[0].x + self.points[5].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min)
                    - 35.0,
            );
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78);
            let label_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                        - DESIGN_TOKENS.spacing.xl,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Impulse",
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }

    /// Render Elliott Correction Wave - A-B-C corrective pattern
    pub(crate) fn render_elliott_correction(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Correction labels (starting after wave 5)
        let labels = ["5", "A", "B", "C"];

        // Draw fill for corrective waves
        if self.points.len() >= 3 {
            let fill_color = DESIGN_TOKENS
                .semantic
                .extended
                .bearish
                .gamma_multiply(20_f32 / 255.0);
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
        for (i, &point) in self.points.iter().enumerate() {
            let is_up = if i > 0 {
                point.y < self.points[i - 1].y
            } else {
                false
            };
            let label_offset = if is_up {
                -DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.md
            } else {
                DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.md
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
                    .gamma_multiply(0.9);

                painter.circle_filled(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::SM),
                    color,
                );
            }
        }

        // Draw "Correction" label
        if self.points.len() >= 4 {
            let label_pos = Pos2::new(
                (self.points[0].x + self.points[3].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    + 30.0,
            );
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78);
            let label_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                        - DESIGN_TOKENS.spacing.sm,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Correction",
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }

    /// Render Elliott Triangle - A-B-C-D-E contracting/expanding triangle
    pub(crate) fn render_elliott_triangle(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let labels = ["A", "B", "C", "D", "E"];

        // Draw fill
        if self.points.len() >= 3 {
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

        // Draw converging trendlines for complete triangle
        if self.points.len() >= 5 {
            let line_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 120);

            // Upper trendline (A-C-E or similar peaks)
            let upper_points: Vec<&Pos2> = self
                .points
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, p)| p)
                .collect();

            if upper_points.len() >= 2 {
                let dx = upper_points[1].x - upper_points[0].x;
                let dy = upper_points[1].y - upper_points[0].y;
                let slope = if dx.abs() > 0.001 { dy / dx } else { 0.0 };
                let extended = Pos2::new(
                    upper_points.last().unwrap().x + 50.0,
                    upper_points.last().unwrap().y + slope * 50.0,
                );
                super::utils::draw_dashed_line(
                    painter,
                    **upper_points.first().unwrap(),
                    extended,
                    Stroke::new(stroke::HAIRLINE, line_color),
                    5.0,
                    3.0,
                );
            }

            // Lower trendline (B-D or similar troughs)
            let lower_points: Vec<&Pos2> = self
                .points
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 2 == 1)
                .map(|(_, p)| p)
                .collect();

            if lower_points.len() >= 2 {
                let dx = lower_points[1].x - lower_points[0].x;
                let dy = lower_points[1].y - lower_points[0].y;
                let slope = if dx.abs() > 0.001 { dy / dx } else { 0.0 };
                let extended = Pos2::new(
                    lower_points.last().unwrap().x + 50.0,
                    lower_points.last().unwrap().y + slope * 50.0,
                );
                super::utils::draw_dashed_line(
                    painter,
                    **lower_points.first().unwrap(),
                    extended,
                    Stroke::new(stroke::HAIRLINE, line_color),
                    5.0,
                    3.0,
                );
            }
        }

        // Draw points and labels
        for (i, &point) in self.points.iter().enumerate() {
            let is_peak = i % 2 == 0;
            let label_offset = if is_peak {
                -DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.sm
            } else {
                DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.sm
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
                    .gamma_multiply(0.9);
                painter.circle_filled(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::SM),
                    color,
                );
            }
        }

        // Triangle label
        if self.points.len() >= 5 {
            let label_pos = Pos2::new(
                (self.points[0].x + self.points[4].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min)
                    - 35.0,
            );
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78);
            let label_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                        - DESIGN_TOKENS.spacing.xl,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Triangle",
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }

    /// Render Elliott Double Combo - W-X-Y pattern
    pub(crate) fn render_elliott_double_combo(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let labels = ["W", "X", "Y"];

        // Draw connecting lines with X wave styled differently
        for i in 1..self.points.len() {
            // X wave (index 1) is drawn dashed
            if i == 2 {
                super::utils::draw_dashed_line(
                    painter,
                    self.points[i - 1],
                    self.points[i],
                    Stroke::new(stroke::HAIRLINE, color),
                    6.0,
                    4.0,
                );
            } else {
                painter.line_segment(
                    [self.points[i - 1], self.points[i]],
                    Stroke::new(self.stroke_width, color),
                );
            }
        }

        // Draw points and labels
        for (i, &point) in self.points.iter().enumerate() {
            let is_up = if i > 0 {
                point.y < self.points[i - 1].y
            } else {
                true
            };
            let label_offset = if is_up {
                -DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.md
            } else {
                DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.md
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
                    .gamma_multiply(0.9);
                painter.circle_filled(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    egui::FontId::proportional(typography::SM),
                    color,
                );
            }
        }

        // Pattern label
        if self.points.len() >= 3 {
            let label_pos = Pos2::new(
                (self.points[0].x + self.points[2].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    + 30.0,
            );
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78);
            let label_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                        + DESIGN_TOKENS.spacing.lg,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Double Combo",
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }

    /// Render Elliott Triple Combo - W-X-Y-X-Z pattern
    pub(crate) fn render_elliott_triple_combo(&self, painter: &egui::Painter) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let labels = ["W", "X", "Y", "X", "Z"];

        // Draw connecting lines with X waves styled differently
        for i in 1..self.points.len() {
            // X waves (index 1 and 3) are drawn dashed
            if i == 2 || i == 4 {
                super::utils::draw_dashed_line(
                    painter,
                    self.points[i - 1],
                    self.points[i],
                    Stroke::new(stroke::HAIRLINE, color),
                    6.0,
                    4.0,
                );
            } else {
                painter.line_segment(
                    [self.points[i - 1], self.points[i]],
                    Stroke::new(self.stroke_width, color),
                );
            }
        }

        // Draw points and labels
        for (i, &point) in self.points.iter().enumerate() {
            let is_up = if i > 0 {
                point.y < self.points[i - 1].y
            } else {
                true
            };
            let label_offset = if is_up {
                -DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.md
            } else {
                DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.md
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
                    .gamma_multiply(0.9);
                painter.circle_filled(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8,
                    Stroke::new(stroke::HAIRLINE, color),
                );

                // Second X gets subscript styling
                let display_label = if i == 3 { "X₂" } else { labels[i] };

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    display_label,
                    egui::FontId::proportional(if i == 3 {
                        typography::XS
                    } else {
                        typography::SM
                    }),
                    color,
                );
            }
        }

        // Pattern label
        if self.points.len() >= 5 {
            let label_pos = Pos2::new(
                (self.points[0].x + self.points[4].x) / 2.0,
                self.points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    + 30.0,
            );
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78);
            let label_rect = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                        + DESIGN_TOKENS.spacing.lg,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Triple Combo",
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }
}
