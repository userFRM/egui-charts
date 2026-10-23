//! Gann tool rendering implementations
//!
//! Includes: Gann fan, square, box, and fixed angle tools.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

/// Screen y at `x` of the fan line through `start` whose slope is `ratio`
/// times the 1x1 slope from `start` to `end`.
fn gann_fan_line_y(start: Pos2, end: Pos2, ratio: f32, x: f32) -> f32 {
    let dy = end.y - start.y;
    let dx = (end.x - start.x).abs();
    // A vertical base has no slope; fall back to one pixel per pixel in the direction of the move.
    let base_slope = if dx > 0.001 {
        dy / dx
    } else if dy < 0.0 {
        -1.0
    } else {
        1.0
    };
    start.y + (x - start.x) * base_slope * ratio
}

/// Slopes of the Gann fan's lines as multiples of the 1x1 slope: an NxM line
/// moves N price units per M time units.
pub(super) const GANN_FAN_RATIOS: [f32; 9] = [8.0, 4.0, 3.0, 2.0, 1.0, 0.5, 1.0 / 3.0, 0.25, 0.125];

/// The Gann fan's lines from `start` to the right edge of `rect`, then the
/// line to `end`.
pub(super) fn gann_fan_lines(start: Pos2, end: Pos2, rect: Rect) -> Vec<[Pos2; 2]> {
    let x = rect.max.x;
    GANN_FAN_RATIOS
        .iter()
        .map(|&ratio| [start, Pos2::new(x, gann_fan_line_y(start, end, ratio, x))])
        .chain([[start, end]])
        .collect()
}

/// The corner opposite `start` of a Gann square or fixed Gann box: towards
/// `end`, as wide and high as the larger of the two distances between them.
pub(super) fn gann_square_corner(start: Pos2, end: Pos2) -> Pos2 {
    let size = (end.x - start.x).abs().max((end.y - start.y).abs());
    let sign = |a: f32, b: f32| if b >= a { 1.0 } else { -1.0 };
    Pos2::new(
        start.x + size * sign(start.x, end.x),
        start.y + size * sign(start.y, end.y),
    )
}

/// The 2x1, 3x1 and 4x1 lines of a Gann box and their 1x2, 1x3 and 1x4
/// mirrors, from the first anchor to the far edges, where the diagonal to the
/// second anchor is the 1x1 line.
fn gann_box_angle_lines(p1: Pos2, p2: Pos2) -> Vec<[Pos2; 2]> {
    let d = p2 - p1;
    [2.0, 3.0, 4.0]
        .into_iter()
        .flat_map(|ratio| {
            [
                [p1, Pos2::new(p2.x, p1.y + d.y / ratio)],
                [p1, Pos2::new(p1.x + d.x / ratio, p2.y)],
            ]
        })
        .collect()
}

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
        // An NxM line moves N price units per M time units: N/M times the 1x1 slope
        let styles: [(&str, Color32); 9] = [
            ("8×1", DESIGN_TOKENS.semantic.extended.bearish), // Steepest resistance
            ("4×1", DESIGN_TOKENS.semantic.indicators.ma),    // Strong resistance
            ("3×1", DESIGN_TOKENS.semantic.extended.favorite_gold), // Moderate resistance
            ("2×1", DESIGN_TOKENS.semantic.extended.caution), // Mild resistance
            ("1×1", DESIGN_TOKENS.semantic.extended.purple),  // 45° balance line
            ("1×2", DESIGN_TOKENS.semantic.indicators.ema),   // Mild support
            ("1×3", DESIGN_TOKENS.semantic.drawings.fib_100), // Moderate support
            ("1×4", DESIGN_TOKENS.semantic.extended.bullish), // Strong support
            ("1×8", DESIGN_TOKENS.semantic.extended.bullish), // Strongest support
        ];

        for (angle_ratio, (label, angle_color)) in GANN_FAN_RATIOS.iter().zip(&styles) {
            let end_y = gann_fan_line_y(start, end, *angle_ratio, chart_rect.max.x);

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

        let square_rect = Rect::from_two_pos(start, gann_square_corner(start, end));
        let size = square_rect.width();

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

        for segment in gann_box_angle_lines(p1, p2) {
            painter.line_segment(
                segment,
                Stroke::new(DESIGN_TOKENS.stroke.light, angle_color),
            );
        }

        // Draw level labels on the left side (price levels)
        let label_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 180);
        let label_bg = DESIGN_TOKENS
            .semantic
            .extended
            .chart_axis_bg
            .gamma_multiply(0.8);

        // Levels count from the first anchor's price, whichever corner it is.
        for i in 0..=divisions {
            if i == 0 || i == divisions || i == divisions / 2 {
                let y = p1.y + (p2.y - p1.y) * i as f32 / divisions as f32;
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

        let corner = gann_square_corner(p1, p2);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A box started at its top-right corner (100, 0) and dragged to (0, 80):
    /// each line starts at the first anchor and ends 1/2, 1/3 and 1/4 of the
    /// way along the far edges.
    #[test]
    fn gann_box_lines_start_at_the_first_anchor() {
        let (p1, p2) = (Pos2::new(100.0, 0.0), Pos2::new(0.0, 80.0));
        let want = [
            (0.0, 40.0),
            (50.0, 80.0),
            (0.0, 26.6667),
            (66.6667, 80.0),
            (0.0, 20.0),
            (75.0, 80.0),
        ]
        .map(|(x, y)| [p1, Pos2::new(x, y)]);
        let got = gann_box_angle_lines(p1, p2);
        assert!(
            got.len() == want.len()
                && got
                    .iter()
                    .zip(&want)
                    .all(|(g, w)| g[0].distance(w[0]) < 1e-3 && g[1].distance(w[1]) < 1e-3),
            "got {got:?}, want {want:?}"
        );
    }

    /// Anchors (0, 100) and (10, 90) put the 1x1 line through the second anchor.
    /// An NxM line moves N price units per M bars, so at x = 10 it sits at
    /// 100 - 10 * N / M (screen y grows downward).
    #[test]
    fn fan_line_slopes_follow_their_labels() {
        let start = Pos2::new(0.0, 100.0);
        let end = Pos2::new(10.0, 90.0);
        let cases = [
            ("8x1", 8.0, 20.0),
            ("4x1", 4.0, 60.0),
            ("3x1", 3.0, 70.0),
            ("2x1", 2.0, 80.0),
            ("1x1", 1.0, 90.0),
            ("1x2", 0.5, 95.0),
            ("1x3", 1.0 / 3.0, 96.6667),
            ("1x4", 0.25, 97.5),
            ("1x8", 0.125, 98.75),
        ];
        let wrong: Vec<_> = cases
            .iter()
            .map(|&(label, ratio, expected)| {
                (label, gann_fan_line_y(start, end, ratio, 10.0), expected)
            })
            .filter(|&(_, y, expected)| (y - expected).abs() >= 1e-3)
            .collect();
        assert!(wrong.is_empty(), "(label, y, expected): {wrong:?}");
    }
}
