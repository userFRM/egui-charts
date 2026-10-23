//! Fibonacci tool rendering implementations
//!
//! Includes: retracement, extension, arc, time zones, channel, circles,
//! speed fan, spiral, wedge, and trend-based fib time.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    pub(crate) fn render_fibonacci(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let Some((config, prices)) = self.fib_levels() else {
            return;
        };

        let start = self.points[0];
        let end = self.points[1];

        // Each level sits at the price it is labelled with, on whatever scale the chart uses
        let visible_levels: Vec<_> = config.visible_levels().collect();
        let ys = &self.level_ys;
        if ys.len() != prices.len() {
            return;
        }

        // Draw filled zones between levels if enabled
        if config.show_background && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];

                let (y1, y2) = (ys[i], ys[i + 1]);

                // Fill with semi-transparent color using config opacity
                let fill_color = Color32::from_rgba_unmultiplied(
                    level1.color[0],
                    level1.color[1],
                    level1.color[2],
                    config.background_opacity,
                );
                let fill_rect =
                    Rect::from_x_y_ranges(chart_rect.x_range(), y1.min(y2)..=y1.max(y2));
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }
        }

        // Calculate x range based on extend settings
        let x_start = if config.extend_left {
            chart_rect.min.x
        } else {
            start.x.min(end.x)
        };
        let x_end = if config.extend_right {
            chart_rect.max.x
        } else {
            start.x.max(end.x)
        };

        // Draw lines and labels for visible levels
        for ((level, &y), &price_at_lvl) in visible_levels.iter().zip(ys).zip(&prices) {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            // Draw line with extend support
            painter.hline(x_start..=x_end, y, Stroke::new(stroke::HAIRLINE, lvl_color));

            // Draw label with price if show_price is enabled
            let label_text = if level.show_price {
                format!("{} ({:.2})", level.label, price_at_lvl)
            } else {
                level.label.clone()
            };

            painter.text(
                Pos2::new(
                    x_end - DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x,
                    y - DESIGN_TOKENS.rounding.xl,
                ),
                egui::Align2::RIGHT_BOTTOM,
                label_text,
                egui::FontId::proportional(typography::XS),
                lvl_color,
            );
        }
    }

    pub(crate) fn render_fibonacci_extension(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let Some((config, prices)) = self.fib_levels() else {
            return;
        };

        let start = self.points[0];
        let end = self.points[1];

        // Each level sits at the price it is labelled with, on whatever scale the chart uses
        let visible_levels: Vec<_> = config.visible_levels().collect();
        let ys = &self.level_ys;
        if ys.len() != prices.len() {
            return;
        }

        // Draw filled zones between levels if enabled
        if config.show_background && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];

                let (y1, y2) = (ys[i], ys[i + 1]);

                // Only draw if at least partially visible
                if (y1 >= chart_rect.min.y || y2 >= chart_rect.min.y)
                    && (y1 <= chart_rect.max.y || y2 <= chart_rect.max.y)
                {
                    let fill_color = Color32::from_rgba_unmultiplied(
                        level1.color[0],
                        level1.color[1],
                        level1.color[2],
                        config.background_opacity,
                    );
                    let fill_rect =
                        Rect::from_x_y_ranges(chart_rect.x_range(), y1.min(y2)..=y1.max(y2));
                    painter.rect_filled(fill_rect, 0.0, fill_color);
                }
            }
        }

        // Calculate x range based on extend settings
        let x_start = if config.extend_left {
            chart_rect.min.x
        } else {
            start.x.min(end.x)
        };
        let x_end = if config.extend_right {
            chart_rect.max.x
        } else {
            start.x.max(end.x)
        };

        for ((level, &y), &price_at_lvl) in visible_levels.iter().zip(ys).zip(&prices) {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            // Only draw if within chart rect
            if y >= chart_rect.min.y && y <= chart_rect.max.y {
                // Draw line with extend support
                painter.hline(x_start..=x_end, y, Stroke::new(stroke::HAIRLINE, lvl_color));

                // Draw label with price if show_price is enabled
                let label_text = if level.show_price {
                    format!("{} ({:.2})", level.label, price_at_lvl)
                } else {
                    level.label.clone()
                };

                painter.text(
                    Pos2::new(x_end - 80.0, y),
                    egui::Align2::RIGHT_CENTER,
                    label_text,
                    egui::FontId::proportional(typography::XS),
                    lvl_color,
                );
            }
        }
    }

    pub(crate) fn render_fibonacci_arc(&self, painter: &egui::Painter) {
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

        // Calculate the distance for base radius
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let base_radius = (dx * dx + dy * dy).sqrt();

        // Fibonacci arc ratios
        let fib_ratios = [0.236, 0.382, 0.5, 0.618, 0.786, 1.0];
        let fib_labels = ["23.6%", "38.2%", "50%", "61.8%", "78.6%", "100%"];

        for (ratio, label) in fib_ratios.iter().zip(fib_labels.iter()) {
            let radius = base_radius * ratio;

            // Draw arc segments (approximate with line segments)
            let segments = 32;
            let mut prev_point: Option<Pos2> = None;
            for i in 0..=segments {
                let angle = std::f32::consts::PI * (1.0 - i as f32 / segments as f32);
                let point = speed_arc_point(start, end, radius, angle);

                if let Some(prev) = prev_point {
                    painter.line_segment([prev, point], Stroke::new(stroke::HAIRLINE, color));
                }
                prev_point = Some(point);
            }

            // Draw label at rightmost point
            painter.text(
                Pos2::new(end.x + radius + DESIGN_TOKENS.rounding.lg, end.y),
                egui::Align2::LEFT_CENTER,
                *label,
                egui::FontId::proportional(typography::XS),
                color,
            );
        }

        // Draw base line
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));
    }

    /// Render Fibonacci Time Zones - Vertical zones at Fibonacci sequence intervals
    pub(crate) fn render_fibonacci_time_zones(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        // Collect zone positions for fill
        let mut zone_positions: Vec<f32> = Vec::new();

        for (fib_num, curr_x) in fib_time_zone_lines(start.x, end.x, chart_rect.x_range()) {
            zone_positions.push(curr_x);

            // Determine line style based on position
            let (stroke_width, alpha) = if fib_num <= 2 {
                (self.stroke_width, 255) // First few zones are prominent
            } else {
                (stroke::HAIRLINE, 180)
            };

            let line_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], alpha);

            // Draw vertical line
            painter.vline(
                curr_x,
                chart_rect.y_range(),
                Stroke::new(stroke_width, line_color),
            );

            // Draw label with background at top
            if fib_num > 0 {
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.85);
                let label_text = format!("{fib_num}");
                let label_pos = Pos2::new(curr_x, chart_rect.min.y + DESIGN_TOKENS.rounding.xl);
                let label_rect = Rect::from_center_size(label_pos, egui::Vec2::new(24.0, 14.0));
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    &label_text,
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }

        // Draw alternating zone fills
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 15);

        for (i, window) in zone_positions.windows(2).enumerate() {
            if i % 2 == 0 {
                let zone_rect = Rect::from_x_y_ranges(window[0]..=window[1], chart_rect.y_range());
                painter.rect_filled(zone_rect, 0.0, fill_color);
            }
        }

        // Draw base line connecting anchor points
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));

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
    }

    /// Render Fibonacci Channel - Parallel channel with Fibonacci level lines
    pub(crate) fn render_fibonacci_channel(&self, painter: &egui::Painter, _chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }

        let start = self.points[0];
        let end = self.points[1];

        // Use configurable levels if available
        let config = self.fib_config.as_ref().cloned().unwrap_or_default();
        let visible_levels: Vec<_> = config.levels.iter().filter(|l| l.visible).collect();

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let line_length = (dx * dx + dy * dy).sqrt();

        if line_length < 1.0 {
            return;
        }

        // Channel width from third point if available, otherwise use 50% of line length
        let width_point = self
            .points
            .get(2)
            .copied()
            .unwrap_or_else(|| start + egui::vec2(-dy, dx) * 0.5);
        let level_line = |value: f32| fib_channel_level_line(start, end, width_point, value);

        // Draw filled zones between levels
        if config.show_background && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];
                let level2 = visible_levels[i + 1];

                let [p1_l1, p2_l1] = level_line(level1.value);
                let [p1_l2, p2_l2] = level_line(level2.value);

                let fill_color = Color32::from_rgba_unmultiplied(
                    level1.color[0],
                    level1.color[1],
                    level1.color[2],
                    config.background_opacity,
                );

                painter.add(egui::epaint::PathShape::convex_polygon(
                    vec![p1_l1, p2_l1, p2_l2, p1_l2],
                    fill_color,
                    Stroke::NONE,
                ));
            }
        }

        // Draw channel level lines
        for level in &visible_levels {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            let [p1, p2] = level_line(level.value);

            // Extend lines if configured
            let (draw_start, draw_end) = if config.extend_left || config.extend_right {
                let dir_x = dx / line_length;
                let dir_y = dy / line_length;
                let extend_left = if config.extend_left { 500.0 } else { 0.0 };
                let extend_right = if config.extend_right { 500.0 } else { 0.0 };

                let new_start = Pos2::new(p1.x - dir_x * extend_left, p1.y - dir_y * extend_left);
                let new_end = Pos2::new(p2.x + dir_x * extend_right, p2.y + dir_y * extend_right);
                (new_start, new_end)
            } else {
                (p1, p2)
            };

            let stroke_width = if level.value == 0.0 || level.value == 1.0 {
                self.stroke_width
            } else {
                stroke::HAIRLINE
            };
            painter.line_segment([draw_start, draw_end], Stroke::new(stroke_width, lvl_color));

            // Draw level label with background
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.8);
            let label_text = format!("{:.1}%", level.value * 100.0);
            let label_rect = Rect::from_min_size(
                Pos2::new(
                    draw_end.x + DESIGN_TOKENS.rounding.sm,
                    draw_end.y - DESIGN_TOKENS.rounding.xl,
                ),
                egui::Vec2::new(40.0, 14.0),
            );
            painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
            painter.text(
                Pos2::new(draw_end.x + 23.0, draw_end.y),
                egui::Align2::CENTER_CENTER,
                &label_text,
                egui::FontId::proportional(typography::XS),
                lvl_color,
            );
        }

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
    }

    pub(crate) fn render_fibonacci_circles(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }

        let center = self.points[0];
        let edge = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let base_radius = ((edge.x - center.x).powi(2) + (edge.y - center.y).powi(2)).sqrt();
        let fib_ratios = [0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.618, 2.618];

        for &ratio in &fib_ratios {
            let radius = base_radius * ratio;
            painter.circle_stroke(center, radius, Stroke::new(stroke::HAIRLINE, color));

            // Label
            painter.text(
                Pos2::new(center.x + radius + DESIGN_TOKENS.rounding.sm, center.y),
                egui::Align2::LEFT_CENTER,
                format!("{:.1}%", ratio * 100.0),
                egui::FontId::proportional(typography::XS),
                color,
            );
        }

        painter.line_segment([center, edge], Stroke::new(self.stroke_width, color));
        painter.circle_filled(center, DESIGN_TOKENS.rounding.md, color);
    }

    /// Render Fibonacci Speed Resistance Fan - Fan lines at Fibonacci ratios
    pub(crate) fn render_fibonacci_speed_fan(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        let is_upward = end.y < start.y;

        // Fibonacci speed resistance fan ratios with labels
        let fib_ratios: [(f32, &str); 7] = [
            (0.0, "0%"),
            (0.236, "23.6%"),
            (0.382, "38.2%"),
            (0.5, "50%"),
            (0.618, "61.8%"),
            (0.786, "78.6%"),
            (1.0, "100%"),
        ];

        // Colors for different levels (gradient from green to red for up, reversed for down)
        let level_colors: [Color32; 7] = if is_upward {
            [
                DESIGN_TOKENS.semantic.extended.bearish, // Red (resistance)
                DESIGN_TOKENS.semantic.indicators.ma,    // Orange
                DESIGN_TOKENS.semantic.extended.caution, // Amber
                DESIGN_TOKENS.semantic.indicators.bb_upper, // Purple (50% pivot)
                DESIGN_TOKENS.semantic.indicators.ema,   // Blue
                DESIGN_TOKENS.semantic.extended.bullish, // Teal (support)
                DESIGN_TOKENS.semantic.extended.bullish, // Teal (target)
            ]
        } else {
            [
                DESIGN_TOKENS.semantic.extended.bullish,    // Teal
                DESIGN_TOKENS.semantic.extended.bullish,    // Green
                DESIGN_TOKENS.semantic.indicators.ema,      // Blue
                DESIGN_TOKENS.semantic.indicators.bb_upper, // Purple
                DESIGN_TOKENS.semantic.extended.caution,    // Amber
                DESIGN_TOKENS.semantic.indicators.ma,       // Orange
                DESIGN_TOKENS.semantic.extended.bearish,    // Red
            ]
        };

        // Draw fan lines
        for (i, (ratio, label)) in fib_ratios.iter().enumerate() {
            // Extend to the chart edge on the side of the second anchor
            let fan_end = speed_fan_line_end(start, end, *ratio, chart_rect);
            let fan_end_y = fan_end.y;

            let line_color = level_colors[i];
            let stroke_width = if *ratio == 0.5 {
                stroke::MEDIUM
            } else {
                stroke::HAIRLINE
            };

            // Draw the fan line
            painter.line_segment([start, fan_end], Stroke::new(stroke_width, line_color));

            // Draw label with background, inside the edge the line runs to
            let label_x = if fan_end.x < start.x {
                chart_rect.min.x + DESIGN_TOKENS.rounding.lg + 45.0
            } else {
                chart_rect.max.x - DESIGN_TOKENS.rounding.lg
            };
            let label_pos = Pos2::new(label_x, fan_end_y);
            let label_bg = DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.85);
            let label_rect = Rect::from_min_size(
                Pos2::new(label_pos.x - 45.0, label_pos.y - 7.0),
                egui::Vec2::new(42.0, 14.0),
            );

            if fan_end_y >= chart_rect.min.y && fan_end_y <= chart_rect.max.y {
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, label_bg);
                painter.text(
                    Pos2::new(label_pos.x - 24.0, label_pos.y),
                    egui::Align2::CENTER_CENTER,
                    *label,
                    egui::FontId::proportional(typography::XS),
                    line_color,
                );
            }
        }

        // Draw fill between adjacent fan lines (subtle)
        let fill_alpha = 10u8;
        for i in 0..fib_ratios.len() - 1 {
            let end1 = speed_fan_line_end(start, end, fib_ratios[i].0, chart_rect);
            let end2 = speed_fan_line_end(start, end, fib_ratios[i + 1].0, chart_rect);

            let fill_color = Color32::from_rgba_unmultiplied(
                level_colors[i].r(),
                level_colors[i].g(),
                level_colors[i].b(),
                fill_alpha,
            );

            painter.add(egui::epaint::PathShape::convex_polygon(
                vec![start, end1, end2],
                fill_color,
                Stroke::NONE,
            ));
        }

        // Draw the main trend line (thicker)
        painter.line_segment([start, end], Stroke::new(self.stroke_width + 0.5, color));

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
    }

    pub(crate) fn render_fibonacci_spiral(&self, painter: &egui::Painter) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let edge = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        if center.distance(edge) < 1.0 {
            return;
        }
        // Two turns inside the second anchor, down to 1/47 of its distance, and one turn outside.
        let points = (-80..=40)
            .map(|i| fib_spiral_point(center, edge, i as f32 * 0.05 * std::f32::consts::PI))
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(stroke::HAIRLINE, color),
        ));
    }

    pub(crate) fn render_fibonacci_wedge(&self, painter: &egui::Painter, _chart_rect: Rect) {
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

        let fib_ratios = [0.236, 0.382, 0.5, 0.618, 0.786, 1.0];
        let angle = (end.y - start.y).atan2(end.x - start.x);

        for &ratio in &fib_ratios {
            let wedge_angle = angle * ratio;
            let len = 200.0;
            let end_point = Pos2::new(
                start.x + len * wedge_angle.cos(),
                start.y + len * wedge_angle.sin(),
            );
            painter.line_segment([start, end_point], Stroke::new(stroke::HAIRLINE, color));
        }
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));
    }

    pub(crate) fn render_trend_based_fib_time(&self, painter: &egui::Painter, chart_rect: Rect) {
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

        let dx = p2.x - p1.x;
        let fib_ratios = [0.0, 0.382, 0.5, 0.618, 1.0, 1.618, 2.618];

        for &ratio in &fib_ratios {
            let x = p1.x + dx * ratio;
            painter.vline(
                x,
                chart_rect.y_range(),
                Stroke::new(stroke::HAIRLINE, color),
            );
            painter.text(
                Pos2::new(x, chart_rect.min.y + typography::XS),
                egui::Align2::CENTER_TOP,
                format!("{:.1}%", ratio * 100.0),
                egui::FontId::proportional(typography::XS),
                color,
            );
        }
    }
}

/// The line of a Fibonacci channel level: the base line from `start` to `end`
/// moved `level` times the way onto `width_point`, straight up or down as the
/// parallel channel's second line is, so each level is as far in price from the
/// base at every bar.
fn fib_channel_level_line(start: Pos2, end: Pos2, width_point: Pos2, level: f32) -> [Pos2; 2] {
    let offset = super::channels::offset_onto(start, end, width_point) * level;
    [start + offset, end + offset]
}

/// A point of a speed resistance arc of `radius` around `end`, `angle`
/// radians from the forward direction, on the side of `end` facing `start`'s
/// price: below the end of a rise, above the end of a fall.
fn speed_arc_point(start: Pos2, end: Pos2, radius: f32, angle: f32) -> Pos2 {
    let side = if start.y >= end.y { 1.0 } else { -1.0 };
    end + radius * egui::vec2(angle.cos(), side * angle.sin())
}

/// Where the speed resistance fan line through `start` and the point `ratio`
/// of the way from `start`'s price to `end`'s, above `end`, leaves the chart
/// on the side of `end`.
fn speed_fan_line_end(start: Pos2, end: Pos2, ratio: f32, chart_rect: Rect) -> Pos2 {
    let d = end - start;
    let slope = if d.x.abs() > 0.001 {
        d.y * ratio / d.x
    } else {
        0.0
    };
    let edge = if d.x < 0.0 {
        chart_rect.min.x
    } else {
        chart_rect.max.x
    };
    Pos2::new(edge, start.y + slope * (edge - start.x))
}

/// Fibonacci number and screen x of each time zone line inside `x_range`:
/// the anchors are one unit apart, and the lines follow the Fibonacci sequence
/// from the first anchor towards the second. Empty when the anchors are less
/// than a pixel apart horizontally.
fn fib_time_zone_lines(start_x: f32, end_x: f32, x_range: egui::Rangef) -> Vec<(u32, f32)> {
    let unit = end_x - start_x;
    if unit.abs() < 1.0 {
        return Vec::new();
    }
    [0, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89]
        .into_iter()
        .map(|n| (n, start_x + unit * n as f32))
        .take_while(|&(_, x)| {
            if unit > 0.0 {
                x <= x_range.max
            } else {
                x >= x_range.min
            }
        })
        .filter(|&(_, x)| x_range.contains(x))
        .collect()
}

/// The point of the golden spiral around `center` that passes through `edge`,
/// `theta` radians past it: the radius grows by the golden ratio every quarter turn.
fn fib_spiral_point(center: Pos2, edge: Pos2, theta: f32) -> Pos2 {
    let d = edge - center;
    let r = d.length() * std::f32::consts::GOLDEN_RATIO.powf(theta / std::f32::consts::FRAC_PI_2);
    let angle = d.y.atan2(d.x) + theta;
    center + r * egui::vec2(angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    /// Each tool against coordinates worked by hand (screen y grows downward).
    #[test]
    fn fibonacci_tools_follow_their_construction() {
        let near = |a: Pos2, b: Pos2| a.distance(b) < 1e-2;
        let mut wrong = Vec::new();

        // Channel along y = 100 with its width point 40 px above: level 1 runs
        // through the width point, level 0.5 halfway. Along y = 100 - x the base
        // is at 50 below the width point (50, 20), 30 px under it: the levels move
        // straight up, by 30 for level 1 and 15 for level 0.5.
        for (start, end, width_point, level, want) in [
            (
                (0.0, 100.0),
                (100.0, 100.0),
                (50.0, 60.0),
                1.0,
                [(0.0, 60.0), (100.0, 60.0)],
            ),
            (
                (0.0, 100.0),
                (100.0, 100.0),
                (50.0, 60.0),
                0.5,
                [(0.0, 80.0), (100.0, 80.0)],
            ),
            (
                (0.0, 100.0),
                (100.0, 0.0),
                (50.0, 20.0),
                1.0,
                [(0.0, 70.0), (100.0, -30.0)],
            ),
            (
                (0.0, 100.0),
                (100.0, 0.0),
                (50.0, 20.0),
                0.5,
                [(0.0, 85.0), (100.0, -15.0)],
            ),
        ] {
            let [start, end, width_point] = [start, end, width_point].map(Pos2::from);
            let got = fib_channel_level_line(start, end, width_point, level);
            let want = want.map(Pos2::from);
            if !(near(got[0], want[0]) && near(got[1], want[1])) {
                wrong.push(format!(
                    "channel {start:?} to {end:?} level {level}: got {got:?}, want {want:?}"
                ));
            }
        }

        // Arcs open towards the first anchor's price: below the end of a rise,
        // above the end of a fall. Radius 50, a quarter turn from forward.
        for (start, end, want) in [
            ((0.0, 100.0), (100.0, 0.0), (100.0, 50.0)),
            ((0.0, 0.0), (100.0, 100.0), (100.0, 50.0)),
        ] {
            let (start, end) = (Pos2::from(start), Pos2::from(end));
            let got = speed_arc_point(start, end, 50.0, FRAC_PI_2);
            if !near(got, Pos2::from(want)) {
                wrong.push(format!(
                    "arc {start:?} to {end:?}: got {got:?}, want {want:?}"
                ));
            }
        }

        // A fan drawn right to left, (200, 100) to (100, 50), leaves on the left
        // edge: slope 0.5 * ratio, so at x = 0, y = 100 - 200 * 0.5 * ratio.
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        for (ratio, want) in [(1.0, (0.0, 0.0)), (0.5, (0.0, 50.0))] {
            let got =
                speed_fan_line_end(Pos2::new(200.0, 100.0), Pos2::new(100.0, 50.0), ratio, rect);
            if !near(got, Pos2::from(want)) {
                wrong.push(format!("fan ratio {ratio}: got {got:?}, want {want:?}"));
            }
        }

        // Time zones 10 px apart from x = 100: one line per Fibonacci number.
        let got = fib_time_zone_lines(100.0, 110.0, egui::Rangef::new(0.0, 300.0));
        let want = [
            (0, 100.0),
            (1, 110.0),
            (2, 120.0),
            (3, 130.0),
            (5, 150.0),
            (8, 180.0),
            (13, 230.0),
        ];
        if got != want {
            wrong.push(format!("time zones: got {got:?}, want {want:?}"));
        }

        // Golden spiral through the second anchor, its radius growing by phi
        // (1.618034) every quarter turn.
        for (edge, theta, want) in [
            ((100.0, 0.0), 0.0, (100.0, 0.0)),
            ((100.0, 0.0), FRAC_PI_2, (0.0, 161.8034)),
            ((100.0, 0.0), -FRAC_PI_2, (0.0, -61.8034)),
            ((0.0, 100.0), 0.0, (0.0, 100.0)),
        ] {
            let got = fib_spiral_point(Pos2::ZERO, Pos2::from(edge), theta);
            if !near(got, Pos2::from(want)) {
                wrong.push(format!(
                    "spiral edge {edge:?} theta {theta}: got {got:?}, want {want:?}"
                ));
            }
        }

        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
