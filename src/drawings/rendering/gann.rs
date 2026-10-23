//! Gann tool rendering implementations
//!
//! Includes: Gann fan, square, box, and fixed angle tools.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

/// Screen y at `x` of the fan line through `start` whose slope is `ratio`
/// times the 1x1 slope from `start` to `end`.
fn gann_fan_line_y(start: Pos2, end: Pos2, ratio: f32, x: f32) -> f32 {
    let dy = end.y - start.y;
    let dx = end.x - start.x;
    // A vertical base has no slope; fall back to one pixel per pixel in the direction of the move.
    let base_slope = if dx.abs() > 0.001 {
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

/// The Gann fan's lines from `start` towards the edge on `end`'s side, then
/// the line to `end`.
pub(super) fn gann_fan_lines(start: Pos2, end: Pos2, rect: Rect) -> Vec<[Pos2; 2]> {
    let x = if end.x < start.x {
        rect.min.x
    } else {
        rect.max.x
    };
    GANN_FAN_RATIOS
        .iter()
        .map(|&ratio| [start, Pos2::new(x, gann_fan_line_y(start, end, ratio, x))])
        .chain([[start, end]])
        .collect()
}

/// The corner opposite `start` of a flexible Gann square: towards
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
    fn draw_gann_grid(&self, painter: &egui::Painter, start: Pos2, end: Pos2) {
        let config = self.gann.clone().unwrap_or_default();
        let rect = Rect::from_two_pos(start, end);
        let default_fixed = self.gann.is_none() && self.tool_type == DrawingToolType::GannFixed;
        for (vertical, levels) in [(true, config.time_levels), (false, config.price_levels)] {
            for ratio in levels
                .into_iter()
                .filter(|r| r.is_finite() && *r > 0.0 && *r < 1.0)
                .filter(|r| !default_fixed || (r * 4.0).fract() == 0.0)
            {
                let color = Color32::from_rgba_unmultiplied(
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    if ratio == 0.5 {
                        120
                    } else if (ratio * 4.0).fract() == 0.0 {
                        80
                    } else if self.tool_type == DrawingToolType::GannBox {
                        40
                    } else {
                        50
                    },
                );
                let stroke = Stroke::new(DESIGN_TOKENS.stroke.extra_thin, color);
                if vertical {
                    painter.vline(start.x + (end.x - start.x) * ratio, rect.y_range(), stroke);
                } else {
                    painter.hline(rect.x_range(), start.y + (end.y - start.y) * ratio, stroke);
                }
            }
        }
    }

    /// Render Gann Fan - Complete 9-angle Gann fan with bi-directional support
    pub(crate) fn render_gann_fan(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }

        let start = self.points[0];
        let end = self.points[1];
        let edge = if end.x < start.x {
            chart_rect.min.x
        } else {
            chart_rect.max.x
        };
        let painter = &painter.with_clip_rect(chart_rect);

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

        let mut labels = Vec::<Rect>::new();
        for (angle_ratio, (label, angle_color)) in GANN_FAN_RATIOS.iter().zip(&styles) {
            let end_y = gann_fan_line_y(start, end, *angle_ratio, edge);

            // Only draw if within reasonable bounds
            if end_y.is_finite() {
                let fan_end = Pos2::new(edge, end_y);

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
                            if end.x < start.x {
                                edge + DESIGN_TOKENS.spacing.xs
                            } else {
                                edge - gann_label_width - DESIGN_TOKENS.spacing.xs
                            },
                            end_y - gann_label_height / 2.0,
                        ),
                        egui::Vec2::new(gann_label_width, gann_label_height),
                    );
                    if labels.iter().any(|r| r.intersects(label_rect.expand(2.0))) {
                        continue;
                    }
                    labels.push(label_rect);
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

        self.draw_gann_grid(painter, start, gann_square_corner(start, end));

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

        self.draw_gann_grid(painter, p1, p2);

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

        let mut labels = Vec::<Rect>::new();
        for level in self
            .gann
            .clone()
            .unwrap_or_default()
            .price_levels
            .into_iter()
            .filter(|r| r.is_finite() && (0.0..=1.0).contains(r))
        {
            let y = p1.y + (p2.y - p1.y) * level;
            let label_pos = Pos2::new(rect.min.x - 25.0, y);
            let label_rect = Rect::from_center_size(label_pos, egui::vec2(32.0, typography::MD));
            if labels.iter().any(|r| r.intersects(label_rect.expand(2.0))) {
                continue;
            }
            labels.push(label_rect);
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            let percentage = format!("{:.2}", level * 100.0);
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                format!(
                    "{}%",
                    percentage.trim_end_matches('0').trim_end_matches('.')
                ),
                egui::FontId::proportional(typography::XS),
                label_color,
            );
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

    /// Fixed price-per-bar range from its persistent chart anchors.
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

        let corner = p2;
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

        self.draw_gann_grid(painter, p1, p2);

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
        let label = self
            .chart_points
            .first()
            .zip(self.chart_points.get(1))
            .map_or_else(
                || "n/a".to_owned(),
                |(a, b)| {
                    let bars = (b.bar_idx - a.bar_idx).abs();
                    if bars > 0.0 {
                        format!("{:.4}/bar", (b.price - a.price).abs() / f64::from(bars))
                    } else {
                        "n/a".to_owned()
                    }
                },
            );
        super::measurements::boxed_label(
            painter,
            egui::Align2::LEFT_TOP,
            label_pos,
            label,
            color,
            label_bg,
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
        for direction in [1.0, -1.0] {
            for (label, ratio, expected) in cases {
                let end = Pos2::new(end.x * direction, end.y);
                let y = gann_fan_line_y(start, end, ratio, 10.0 * direction);
                assert!(
                    (y - expected).abs() < 1e-3,
                    "{label} direction {direction}: {y} != {expected}"
                );
            }
        }
    }
    #[test]
    fn gann_grids_keep_chart_dimensions_and_custom_divisions() {
        let mut drawing = Drawing::new(0, crate::drawings::DrawingToolType::GannFixed);
        drawing.chart_points = vec![
            crate::drawings::ChartPoint::new(2.0, 20.0),
            crate::drawings::ChartPoint::new(12.0, 10.0),
        ];
        for (x_scale, y_scale) in [(10.0, 10.0), (20.0, 5.0), (5.0, 20.0)] {
            drawing.update_screen_coords(|x| x * x_scale, |y| y as f32 * y_scale);
            let outline = drawing.outline(Rect::EVERYTHING).unwrap();
            assert!(
                outline
                    .lines
                    .iter()
                    .flatten()
                    .any(|p| p.distance(drawing.points[1]) < 1e-3)
            );
        }
        drawing.points = vec![Pos2::new(20.0, 200.0), Pos2::new(120.0, 100.0)];
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            drawing.render_gann_fixed(ui.painter())
        });
        let verticals=output.shapes.iter().filter(|s|matches!(&s.shape,egui::Shape::LineSegment{points,stroke} if stroke.width==DESIGN_TOKENS.stroke.extra_thin && points[0].x==points[1].x && points[0].y==100.0 && points[1].y==200.0)).count();
        output.drop_without_applying_deltas();
        assert_eq!(
            verticals, 3,
            "the default fixed grid keeps quarter divisions"
        );
        let mut saved = serde_json::to_value(&drawing).unwrap();
        saved["gann"] = serde_json::json!({"price_levels":[0.3_f32],"time_levels":[0.7_f32]});
        let mut restored: Drawing = serde_json::from_value(saved.clone()).unwrap();
        restored.points = drawing.points;
        assert_eq!(
            serde_json::to_value(&restored).unwrap()["gann"],
            saved["gann"]
        );
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            restored.render_gann_fixed(ui.painter())
        });
        let lines: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::LineSegment { points, .. } = &s.shape {
                    Some(*points)
                } else {
                    None
                }
            })
            .collect();
        output.drop_without_applying_deltas();
        for want in [
            [Pos2::new(90.0, 100.0), Pos2::new(90.0, 200.0)],
            [Pos2::new(20.0, 170.0), Pos2::new(120.0, 170.0)],
        ] {
            assert!(
                lines.iter().any(
                    |line| line[0].distance(want[0]) < 1e-3 && line[1].distance(want[1]) < 1e-3
                ),
                "missing {want:?}: {lines:?}"
            );
        }
    }
}
