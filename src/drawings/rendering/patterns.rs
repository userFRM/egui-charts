//! Chart pattern rendering implementations
//!
//! Includes: XABCD, Cypher, Head & Shoulders, ABCD, Triangle, and Three Drives patterns.

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::drawings::rendering::ChartView;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

/// The extension of the second correction, as the least and the greatest ratio, in which the third
/// drive is expected to end.
const THREE_DRIVES_EXTENSION: [f64; 2] = [1.272, 1.618];

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

    /// Draw each ratio on the chord joining the outside ends of its two legs.
    fn draw_harmonic_ratios(&self, painter: &egui::Painter, color: Color32) {
        let [b, c, d] = self.harmonic_ratios();
        let cypher = self.tool_type == DrawingToolType::CypherPattern;
        for (ends, ratio) in [
            ([0, 2], b),
            (if cypher { [0, 3] } else { [1, 3] }, c),
            (if cypher { [0, 4] } else { [2, 4] }, d),
            (
                [0, 4],
                (!cypher).then(|| self.leg_ratio([1, 4], [0, 1])).flatten(),
            ),
        ] {
            self.draw_pattern_ratio(painter, color, ends, ratio);
        }
    }

    fn draw_pattern_ratio(
        &self,
        painter: &egui::Painter,
        color: Color32,
        [from, to]: [usize; 2],
        ratio: Option<f64>,
    ) {
        let (Some(&a), Some(&b)) = (self.points.get(from), self.points.get(to)) else {
            return;
        };
        super::utils::draw_dashed_line(
            painter,
            a,
            b,
            Stroke::new(stroke::HAIRLINE, color.gamma_multiply(0.4)),
            DESIGN_TOKENS.spacing.sm,
            DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
        );
        let Some(ratio) = ratio else {
            return;
        };
        let galley = painter.layout_no_wrap(
            format!("{ratio:.3}"),
            egui::FontId::proportional(typography::XS),
            color,
        );
        let at = |t| egui::Align2::CENTER_BOTTOM.anchor_size(a.lerp(b, t), galley.size());
        let rect = [0.5, 0.4, 0.6, 0.3, 0.7]
            .into_iter()
            .map(at)
            .find(|r| {
                !self
                    .points
                    .iter()
                    .any(|p| r.intersects(Rect::from_center_size(*p, egui::vec2(24.0, 48.0))))
            })
            .unwrap_or_else(|| at(0.5));
        painter.galley(rect.min, galley, color);
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
        if let Some((neckline, target_y)) = self.neckline() {
            let neckline_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);
            let last_x = self.points[self.points.len() - 1].x;

            painter.line_segment(neckline, Stroke::new(stroke::MEDIUM, neckline_color));

            // Draw target line (dashed)
            let target_color = DESIGN_TOKENS.semantic.indicators.ma.gamma_multiply(0.7); // Orange
            super::utils::draw_dashed_line(
                painter,
                Pos2::new(last_x, target_y),
                Pos2::new(
                    last_x + DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x,
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
                last_x
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

        self.draw_pattern_ratio(painter, color, [0, 2], self.leg_ratio([1, 2], [0, 1]));
        self.draw_pattern_ratio(painter, color, [1, 3], self.leg_ratio([2, 3], [1, 2]));

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
        if self.points.len() >= 4
            && let Some(ratio) = self.abcd_ratio()
        {
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
                format!("CD/AB: {ratio:.2}"),
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

        // Four anchors zigzag between the two boundaries; fewer are a plain polygon.
        let boundaries = triangle_boundaries(&self.points, painter.clip_rect().right());
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 25);
        if self.points.len() == 3 {
            painter.add(egui::epaint::PathShape::convex_polygon(
                self.points.clone(),
                fill_color,
                Stroke::NONE,
            ));
        } else if let Some((_, fill)) = &boundaries {
            painter.add(egui::epaint::PathShape::convex_polygon(
                fill.clone(),
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

        if self.points.len() == 3 {
            painter.line_segment(
                [self.points[2], self.points[0]],
                Stroke::new(self.stroke_width, color),
            );
        }

        // Draw both boundaries through their anchors, then dashed on to where they meet.
        if let Some((lines, _)) = &boundaries {
            let extend_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
            for (line, last) in lines.iter().zip([self.points[2], self.points[3]]) {
                painter.line_segment([line[0], last], Stroke::new(self.stroke_width, color));
                if line[1].x > last.x {
                    super::utils::draw_dashed_line(
                        painter,
                        last,
                        line[1],
                        Stroke::new(stroke::HAIRLINE, extend_color),
                        DESIGN_TOKENS.spacing.sm + DESIGN_TOKENS.spacing.hairline,
                        DESIGN_TOKENS.spacing.xs + DESIGN_TOKENS.spacing.hairline,
                    );
                }
            }
        }

        if self.points.len() >= 3 {
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

    /// Render Three Drives Pattern - With each leg's ratio and the reversal zone
    pub(crate) fn render_three_drives_pattern(&self, painter: &egui::Painter, view: &ChartView) {
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

        // Each correction's retracement of the drive before it and each drive's extension of the
        // correction before it, on the chord joining the outside ends of the two legs
        for (ends, ratio) in [[0, 2], [1, 3], [2, 4], [3, 5]]
            .into_iter()
            .zip(self.three_drives_ratios())
        {
            self.draw_pattern_ratio(painter, color, ends, ratio);
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

        // Draw the reversal zone, the price interval the third drive is expected to end in, from
        // the last drive's anchor to the pattern's last
        if self.points.len() >= 7
            && let Some(prices) = self.three_drives_zone()
        {
            let reversal_color = DESIGN_TOKENS
                .semantic
                .extended
                .bullish
                .gamma_multiply(60_f32 / 255.0); // Teal with alpha
            let [y1, y2] = prices.map(|price| (view.price_to_y)(price));
            let (x1, x2) = (self.points[5].x, self.points[6].x);
            let zone_rect = Rect::from_x_y_ranges(x1.min(x2)..=x1.max(x2), y1.min(y2)..=y1.max(y2));
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

/// Triangle pattern boundaries from the zigzag anchors (first peak, first trough, second peak,
/// second trough): the line through each pair of same-side anchors, from its first anchor to where
/// the two meet, and the fill between them and the first zigzag leg as a convex polygon. The
/// boundaries end at `right` when they meet beyond it, and at the last anchor when they are
/// parallel, diverge or meet before the first anchor. `None` until four anchors are placed, or
/// while a boundary is vertical.
pub(super) fn triangle_boundaries(
    points: &[Pos2],
    right: f32,
) -> Option<([[Pos2; 2]; 2], Vec<Pos2>)> {
    let &[a0, b0, a1, b1, ..] = points else {
        return None;
    };
    if a1.x == a0.x || b1.x == b0.x {
        return None;
    }
    let (slope_a, slope_b) = ((a1.y - a0.y) / (a1.x - a0.x), (b1.y - b0.y) / (b1.x - b0.x));
    let y_a = |x: f32| a0.y + slope_a * (x - a0.x);
    let y_b = |x: f32| b0.y + slope_b * (x - b0.x);
    let left = a0.x.min(b0.x);
    let last = a1.x.max(b1.x);
    let apex_x = (b0.y - a0.y + slope_a * a0.x - slope_b * b0.x) / (slope_a - slope_b);
    let apex_x = (apex_x.is_finite() && apex_x > left).then_some(apex_x);
    let end = apex_x.map_or(last, |x| x.min(last.max(right)));
    let mut fill = vec![a0, Pos2::new(end, y_a(end))];
    if apex_x != Some(end) {
        fill.push(Pos2::new(end, y_b(end)));
    }
    fill.push(b0);
    Some((
        [
            [a0, Pos2::new(end, y_a(end))],
            [b0, Pos2::new(end, y_b(end))],
        ],
        fill,
    ))
}

/// Head and shoulders neckline and target from the anchors (start, left shoulder, trough, head,
/// trough, right shoulder, end): the neckline runs through the two troughs, from the left trough to
/// `extend` past the last anchor's x, and the target y lies at the last anchor's x, as far beyond
/// the neckline as the head is from it at the head's x, on the side away from the head. `None`
/// until both troughs are placed, or while they share an x.
fn head_and_shoulders_neckline(points: &[Pos2], extend: f32) -> Option<([Pos2; 2], f32)> {
    let &[_, _, left, head, right, ..] = points else {
        return None;
    };
    let dx = right.x - left.x;
    if dx == 0.0 {
        return None;
    }
    let neckline_y = |x: f32| left.y + (right.y - left.y) / dx * (x - left.x);
    let last_x = points[points.len() - 1].x;
    let end_x = last_x + extend;
    Some((
        [left, Pos2::new(end_x, neckline_y(end_x))],
        neckline_y(last_x) - (head.y - neckline_y(head.x)),
    ))
}

impl Drawing {
    /// The head and shoulders' neckline, run on past the last anchor, and the
    /// y of its target.
    pub(super) fn neckline(&self) -> Option<([Pos2; 2], f32)> {
        let extend =
            DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x + DESIGN_TOKENS.spacing.xxxl;
        head_and_shoulders_neckline(&self.points, extend)
    }

    /// B's price retracement of XA, C's retracement of AB (extension of XA for Cypher), and D's price
    /// retracement of XC for Cypher or extension of BC for XABCD.
    fn harmonic_ratios(&self) -> [Option<f64>; 3] {
        if self.tool_type == DrawingToolType::ABCDPattern {
            return [
                self.leg_ratio([1, 2], [0, 1]),
                self.leg_ratio([2, 3], [1, 2]),
                None,
            ];
        }
        let cypher = self.tool_type == DrawingToolType::CypherPattern;
        [
            self.leg_ratio([1, 2], [0, 1]),
            if cypher {
                self.leg_ratio([0, 3], [0, 1])
            } else {
                self.leg_ratio([2, 3], [1, 2])
            },
            self.leg_ratio([3, 4], if cypher { [0, 3] } else { [2, 3] }),
        ]
    }

    /// Price ratios of the Three drives legs: correction 1 over drive 1, drive 2 over correction 1,
    /// correction 2 over drive 2 and drive 3 over correction 2.
    fn three_drives_ratios(&self) -> [Option<f64>; 4] {
        [
            self.leg_ratio([1, 2], [0, 1]),
            self.leg_ratio([2, 3], [1, 2]),
            self.leg_ratio([3, 4], [2, 3]),
            self.leg_ratio([4, 5], [3, 4]),
        ]
    }

    /// The prices the third drive is expected to end between: the second correction extended by
    /// [`THREE_DRIVES_EXTENSION`] from its end, in the direction opposite to that correction;
    /// `None` until both ends are placed or while the correction is flat.
    fn three_drives_zone(&self) -> Option<[f64; 2]> {
        let (from, to) = (
            self.chart_points.get(3)?.price,
            self.chart_points.get(4)?.price,
        );
        if from == to {
            return None;
        }
        Some(THREE_DRIVES_EXTENSION.map(|ratio| to + (from - to) * ratio))
    }

    /// Price ratio of an ABCD pattern's CD leg to its AB leg.
    fn abcd_ratio(&self) -> Option<f64> {
        self.leg_ratio([2, 3], [0, 1])
    }

    /// Price height of the leg between two anchors over that of a base leg; `None` when an anchor
    /// has no chart coordinates or the base leg is flat.
    fn leg_ratio(&self, leg: [usize; 2], base: [usize; 2]) -> Option<f64> {
        let height = |[from, to]: [usize; 2]| {
            Some((self.chart_points.get(to)?.price - self.chart_points.get(from)?.price).abs())
        };
        let base = height(base)?;
        if base == 0.0 {
            return None;
        }
        Some(height(leg)? / base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::{ChartPoint, DrawingToolType};

    #[test]
    fn head_and_shoulders_neckline_runs_through_the_troughs() {
        let anchors = |ys: [f32; 7], last_trough_x: f32| {
            let mut points = ys.map(|y| Pos2::new(0.0, y));
            for (i, p) in points.iter_mut().enumerate() {
                p.x = 10.0 * i as f32;
            }
            points[4].x = last_trough_x;
            points
        };
        let top = anchors([300.0, 200.0, 250.0, 100.0, 260.0, 210.0, 320.0], 40.0);
        let inverse = anchors([100.0, 200.0, 150.0, 300.0, 140.0, 190.0, 80.0], 40.0);
        let vertical = anchors([300.0, 200.0, 250.0, 100.0, 260.0, 210.0, 320.0], 20.0);
        // Neckline slope 0.5 from the left trough: 255 at the head's x (height 155), 270 at the
        // last anchor and 275 at 10 past it, so the target is 270 + 155. The inverse pattern
        // mirrors y about 200: slope -0.5, 130 at the last anchor, target 130 - 155.
        let cases = [
            (&top[..], Some(([top[2], Pos2::new(70.0, 275.0)], 425.0))),
            (
                &inverse[..],
                Some(([inverse[2], Pos2::new(70.0, 125.0)], -25.0)),
            ),
            (&vertical[..], None),
            (&top[..4], None),
        ];
        for (points, expected) in cases {
            assert_eq!(
                head_and_shoulders_neckline(points, 10.0),
                expected,
                "{points:?}"
            );
        }
    }

    #[test]
    fn triangle_boundaries_meet_at_the_apex_and_clip_the_fill() {
        let p = Pos2::new;
        // Zigzag anchors on an upper line y = 0.5x and a lower line y = 95 - 0.5 (x - 10), which
        // meet at (100, 50); parallel lines; and an upper y = 55 - 0.5x with a lower y = 40 + 2x,
        // which meet at x = 6, left of every anchor.
        let converging = [p(0.0, 0.0), p(10.0, 95.0), p(20.0, 10.0), p(30.0, 85.0)];
        let parallel = [p(0.0, 0.0), p(10.0, 50.0), p(20.0, 0.0), p(30.0, 50.0)];
        let diverging = [p(10.0, 50.0), p(20.0, 80.0), p(30.0, 40.0), p(40.0, 120.0)];
        let vertical = [p(0.0, 0.0), p(10.0, 95.0), p(0.0, 10.0), p(30.0, 85.0)];
        let cases = [
            (
                &converging[..],
                f32::INFINITY,
                Some((
                    [
                        [p(0.0, 0.0), p(100.0, 50.0)],
                        [p(10.0, 95.0), p(100.0, 50.0)],
                    ],
                    vec![p(0.0, 0.0), p(100.0, 50.0), p(10.0, 95.0)],
                )),
            ),
            (
                &converging[..],
                60.0,
                Some((
                    [[p(0.0, 0.0), p(60.0, 30.0)], [p(10.0, 95.0), p(60.0, 70.0)]],
                    vec![p(0.0, 0.0), p(60.0, 30.0), p(60.0, 70.0), p(10.0, 95.0)],
                )),
            ),
            (
                &parallel[..],
                f32::INFINITY,
                Some((
                    [[p(0.0, 0.0), p(30.0, 0.0)], [p(10.0, 50.0), p(30.0, 50.0)]],
                    vec![p(0.0, 0.0), p(30.0, 0.0), p(30.0, 50.0), p(10.0, 50.0)],
                )),
            ),
            (
                &diverging[..],
                f32::INFINITY,
                Some((
                    [
                        [p(10.0, 50.0), p(40.0, 35.0)],
                        [p(20.0, 80.0), p(40.0, 120.0)],
                    ],
                    vec![p(10.0, 50.0), p(40.0, 35.0), p(40.0, 120.0), p(20.0, 80.0)],
                )),
            ),
            (&vertical[..], f32::INFINITY, None),
            (&converging[..3], f32::INFINITY, None),
        ];
        for (points, right, expected) in cases {
            assert_eq!(triangle_boundaries(points, right), expected, "{points:?}");
        }
    }

    #[test]
    fn pattern_ratios_are_measured_in_price() {
        // Screen y is 1000 / price, a non-linear scale on which screen ratios differ from price ratios.
        let pattern = |tool, prices: &[f64]| {
            let mut drawing = Drawing::new(0, tool);
            for (i, &price) in prices.iter().enumerate() {
                let x = 10.0 * i as f32;
                drawing.chart_points.push(ChartPoint::new(x, price));
                drawing.points.push(Pos2::new(x, (1000.0 / price) as f32));
            }
            drawing
        };
        // [B retracement, C extension, D retracement/extension, AD/XA or CD/AB]. Cypher measures
        // C against XA and D against XC; XABCD measures D against BC. ABCD uses the
        // first four anchors as A, B, C, D.
        use DrawingToolType::{CypherPattern, XABCDPattern};
        let cases = [
            (
                DrawingToolType::ABCDPattern,
                &[200.0, 100.0, 161.8, 38.2][..],
                [Some(0.618), Some(2.0), None, Some(1.236)],
            ),
            (
                XABCDPattern,
                &[100.0, 200.0, 138.2, 176.4, 78.6][..],
                [
                    Some(61.8 / 100.0),
                    Some(38.2 / 61.8),
                    Some(97.8 / 38.2),
                    Some(121.4 / 100.0),
                ],
            ),
            (
                XABCDPattern,
                &[100.0, 150.0, 125.0, 175.0],
                [Some(25.0 / 50.0), Some(2.0), None, None],
            ),
            (
                XABCDPattern,
                &[100.0, 100.0, 138.2, 176.4],
                [None, Some(1.0), None, None],
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 227.2, 127.2208],
                [Some(0.5), Some(1.272), Some(0.786), None],
            ),
            (
                CypherPattern,
                &[300.0, 200.0, 250.0, 172.8, 272.7792],
                [Some(0.5), Some(1.272), Some(0.786), None],
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 227.2],
                [Some(0.5), Some(1.272), None, None],
            ),
            (
                CypherPattern,
                &[100.0, 100.0, 125.0, 150.0, 110.7],
                [None, None, Some(0.786), None],
            ),
            (
                CypherPattern,
                &[100.0, 200.0, 150.0, 100.0, 120.0],
                [Some(0.5), Some(0.0), None, None],
            ),
        ];
        for (tool, prices, expected) in cases {
            let drawing = pattern(tool, prices);
            let [b, c, d] = drawing.harmonic_ratios();
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let painter = ui.painter().clone();
                if tool == DrawingToolType::ABCDPattern {
                    drawing.render_abcd_pattern(&painter);
                } else {
                    drawing.render_xabcd_pattern(&painter);
                }
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Text(t) = &shape.shape {
                        Some(t.galley.job.text.clone())
                    } else {
                        None
                    }
                })
                .collect();
            output.drop_without_applying_deltas();
            for ratio in expected[..3]
                .iter()
                .copied()
                .flatten()
                .chain((tool == XABCDPattern).then_some(expected[3]).flatten())
            {
                assert!(
                    labels.contains(&format!("{ratio:.3}")),
                    "{tool:?}: {labels:?}"
                );
            }
            let placement = match tool {
                XABCDPattern => drawing.leg_ratio([1, 4], [0, 1]),
                DrawingToolType::ABCDPattern => drawing.abcd_ratio(),
                _ => None,
            };
            for (actual, expected) in [b, c, d, placement].into_iter().zip(expected) {
                match (actual, expected) {
                    (Some(a), Some(e)) => {
                        assert!((a - e).abs() < 1e-6, "{tool:?} {prices:?}: {a} != {e}")
                    }
                    _ => assert_eq!(actual, expected, "{tool:?} {prices:?}"),
                }
            }
        }
    }

    #[test]
    fn three_drives_ratios_and_zone_are_measured_in_price() {
        let drives = |prices: &[f64]| {
            let mut drawing = Drawing::new(0, DrawingToolType::ThreeDrivesPattern);
            drawing.chart_points = prices
                .iter()
                .enumerate()
                .map(|(i, &price)| ChartPoint::new(10.0 * i as f32, price))
                .collect();
            drawing
        };
        // Drive 1 and 2 rise by 100 and 80, corrections fall by 61.8 and 60; drive 3 would end
        // 1.272 to 1.618 corrections above the second trough: 158.2 + 60 * [1.272, 1.618].
        let rising = drives(&[100.0, 200.0, 138.2, 218.2, 158.2, 255.0]);
        let falling = drives(&[300.0, 200.0, 261.8, 181.8, 241.8, 145.0]);
        // A flat second correction has no zone, and a flat first drive no ratio.
        let flat = drives(&[100.0, 100.0, 90.0, 120.0, 120.0]);
        let cases = [
            (
                &rising,
                [
                    Some(0.618),
                    Some(80.0 / 61.8),
                    Some(0.75),
                    Some(96.8 / 60.0),
                ],
                Some([234.52, 255.28]),
            ),
            (
                &falling,
                [
                    Some(0.618),
                    Some(80.0 / 61.8),
                    Some(0.75),
                    Some(96.8 / 60.0),
                ],
                Some([165.48, 144.72]),
            ),
            (&flat, [None, Some(30.0 / 10.0), Some(0.0), None], None),
        ];
        for (drawing, ratios, zone) in cases {
            for (actual, expected) in drawing.three_drives_ratios().into_iter().zip(ratios) {
                match (actual, expected) {
                    (Some(a), Some(e)) => assert!((a - e).abs() < 1e-9, "{a} != {e}"),
                    _ => assert_eq!(actual, expected),
                }
            }
            match (drawing.three_drives_zone(), zone) {
                (Some(a), Some(e)) => assert!(
                    a.iter().zip(e).all(|(a, e)| (a - e).abs() < 1e-9),
                    "{a:?} != {e:?}"
                ),
                (a, e) => assert_eq!(a, e),
            }
        }
    }
}
