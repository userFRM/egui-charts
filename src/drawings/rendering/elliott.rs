//! Elliott Wave tool rendering implementations
//!
//! Includes: impulse, correction, triangle, double combo, and triple combo waves.

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Render Elliott Impulse Wave - 5-wave motive pattern (0-1-2-3-4-5)
    pub(crate) fn render_elliott_impulse(&self, painter: &egui::Painter, view: &ChartView) {
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Wave labels with degree notation (can be customized)
        let labels = ["0", "1", "2", "3", "4", "5"];

        // Highlight the wave legs only when fill is enabled.
        if self.points.len() >= 3
            && let Some(fill) = self.fill_color32()
        {
            for leg in self.points.windows(2) {
                let [start, end] = [leg[0], leg[1]];
                let mid = start.lerp(end, 0.5) + egui::vec2(0.0, 5.0);
                painter.add(egui::epaint::PathShape::convex_polygon(
                    vec![start, end, mid],
                    fill,
                    Stroke::NONE,
                ));
            }
        }

        for leg in self.points.windows(2) {
            painter.line_segment([leg[0], leg[1]], self.stroke());
        }

        // Wave 3 target, projected from the wave 2 end in price
        if let Some(target) = self.impulse_target() {
            let fib_color = DESIGN_TOKENS.semantic.indicators.ma.gamma_multiply(0.47); // Orange with alpha
            let wave2_end = self.points[2];
            let target_y = (view.price_to_y)(target);

            super::utils::draw_dashed_line(
                painter,
                wave2_end,
                Pos2::new(wave2_end.x + 40.0, target_y),
                Stroke::new(stroke::HAIRLINE, fib_color),
                3.0,
                2.0,
            );

            painter.text(
                Pos2::new(wave2_end.x + 45.0, target_y),
                egui::Align2::LEFT_CENTER,
                "1.618",
                self.annotation_font(),
                fib_color,
            );
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

            // Placement anchor
            if !self.completed {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }

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
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    self.annotation_font(),
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
                ) * (self.annotation_font().size / typography::XS).max(1.0),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Impulse",
                self.annotation_font(),
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
            let fill_color = self.fill_color32().unwrap_or(Color32::TRANSPARENT);
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

            if !self.completed {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }

            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9);

                painter.circle_filled(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    self.annotation_font(),
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
                ) * (self.annotation_font().size / typography::XS).max(1.0),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Correction",
                self.annotation_font(),
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

        let labels = ["0", "A", "B", "C", "D", "E"];

        // Draw fill between the waves, without the origin
        if self.points.len() >= 4 {
            let fill_color = self.fill_color32().unwrap_or(Color32::TRANSPARENT);
            painter.add(egui::epaint::PathShape::convex_polygon(
                self.points[1..].to_vec(),
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

            // Trendline through A, C and E
            let upper_points: Vec<&Pos2> = self
                .points
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 2 == 1)
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

            // Trendline through B and D
            let lower_points: Vec<&Pos2> = self
                .points
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(i, _)| i % 2 == 0)
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

        // Draw points and labels: A, C and E lie on the side of the origin that A moves to
        let a_is_peak = self.points.len() < 2 || self.points[1].y < self.points[0].y;
        for (i, &point) in self.points.iter().enumerate() {
            let is_peak = (i % 2 == 1) == a_is_peak;
            let label_offset = if is_peak {
                -DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.sm
            } else {
                DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.sm
            };

            if !self.completed {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }

            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9);
                painter.circle_filled(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    self.annotation_font(),
                    color,
                );
            }
        }

        // Triangle label
        if self.points.len() >= 6 {
            let label_pos = Pos2::new(
                (self.points[1].x + self.points[5].x) / 2.0,
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
                ) * (self.annotation_font().size / typography::XS).max(1.0),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Triangle",
                self.annotation_font(),
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

            if !self.completed {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }

            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9);
                painter.circle_filled(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    Stroke::new(stroke::HAIRLINE, color),
                );

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    labels[i],
                    self.annotation_font(),
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
                ) * (self.annotation_font().size / typography::XS).max(1.0),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Double Combo",
                self.annotation_font(),
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

            if !self.completed {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, color);
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }

            if i < labels.len() {
                let label_pos = Pos2::new(point.x, point.y + label_offset);
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.9);
                painter.circle_filled(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    label_bg,
                );
                painter.circle_stroke(
                    label_pos,
                    (DESIGN_TOKENS.sizing.technical_labels.elliott_label_size / 1.8)
                        .max(self.annotation_font().size * 0.8),
                    Stroke::new(stroke::HAIRLINE, color),
                );

                // Second X gets subscript styling
                let display_label = if i == 3 { "X₂" } else { labels[i] };

                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    display_label,
                    self.annotation_font(),
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
                ) * (self.annotation_font().size / typography::XS).max(1.0),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                "Triple Combo",
                self.annotation_font(),
                color,
            );
        }
    }
}

/// The wave 3 target as a multiple of wave 1, projected from the wave 2 end.
const WAVE_3_TARGET: f64 = 1.618;

impl Drawing {
    /// The price of the wave 3 target: the 0 to 1 price leg times [`WAVE_3_TARGET`] from the
    /// wave 2 end, in the direction of wave 1. A logarithmic construction (the drawing's
    /// `fib_config`) scales the wave 2 end by the same multiple of the leg's log return. `None`
    /// until wave 2 is placed or while wave 1 is flat.
    fn impulse_target(&self) -> Option<f64> {
        let [wave0, wave1, wave2, ..] = self.chart_points.as_slice() else {
            return None;
        };
        let (from, to, end) = (wave0.price, wave1.price, wave2.price);
        if from == to {
            return None;
        }
        if self.fib_config.as_ref().is_some_and(|c| c.logarithmic)
            && from > 0.0
            && to > 0.0
            && end > 0.0
        {
            Some((end.ln() + (to.ln() - from.ln()) * WAVE_3_TARGET).exp())
        } else {
            Some(end + (to - from) * WAVE_3_TARGET)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::{ChartPoint, DrawingToolType, FibonacciConfig};

    #[test]
    fn impulse_target_projects_the_first_wave_from_the_wave_2_end_in_price() {
        let target = |prices: &[f64], logarithmic: bool| {
            let mut drawing = Drawing::new(0, DrawingToolType::ElliottImpulse);
            drawing.chart_points = prices
                .iter()
                .enumerate()
                .map(|(i, &price)| ChartPoint::new(10.0 * i as f32, price))
                .collect();
            drawing.fib_config = Some(FibonacciConfig {
                logarithmic,
                ..FibonacciConfig::default()
            });
            drawing.impulse_target()
        };
        // Wave 1 is 100 to 150: 120 + 50 * 1.618 above wave 2, or 120 * 1.5^1.618 on a log scale.
        // The falling impulse mirrors it. A flat wave 1, or no wave 2 yet, has no target.
        let cases = [
            (&[100.0, 150.0, 120.0][..], false, Some(200.9)),
            (
                &[100.0, 150.0, 120.0],
                true,
                Some(120.0 * 1.5f64.powf(1.618)),
            ),
            (&[150.0, 100.0, 130.0], false, Some(49.1)),
            (
                &[150.0, 100.0, 130.0],
                true,
                Some(130.0 * (2.0f64 / 3.0).powf(1.618)),
            ),
            (&[-10.0, 10.0, 0.0], true, Some(32.36)),
            (&[100.0, 100.0, 90.0], false, None),
            (&[100.0, 150.0], false, None),
        ];
        for (prices, logarithmic, want) in cases {
            match (target(prices, logarithmic), want) {
                (Some(got), Some(want)) => {
                    assert!((got - want).abs() < 1e-9, "{prices:?}: {got} != {want}")
                }
                (got, want) => assert_eq!(got, want, "{prices:?}"),
            }
        }
    }
}
