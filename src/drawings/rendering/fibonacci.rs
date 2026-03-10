//! Fibonacci tool rendering implementations
//!
//! Includes: retracement, extension, arc, time zones, channel, circles,
//! speed fan, spiral, wedge, and trend-based fib time.

use crate::drawings::FibonacciConfig;
use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    pub(crate) fn render_fibonacci(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }

        let start = self.points[0];
        let end = self.points[1];

        // Use configurable levels if available, otherwise use defaults
        let config = self.fib_config.as_ref().cloned().unwrap_or_default();

        // Collect visible levels for iteration
        let visible_levels: Vec<_> = config.levels.iter().filter(|l| l.visible).collect();

        // Get actual price values from chart_points if available
        let (start_price, end_price) = if self.chart_points.len() >= 2 {
            (self.chart_points[0].price, self.chart_points[1].price)
        } else {
            (0.0, 0.0) // Fallback
        };

        // Draw filled zones between levels if enabled
        if config.show_background && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];
                let level2 = visible_levels[i + 1];

                let y1 = start.y + (end.y - start.y) * level1.value;
                let y2 = start.y + (end.y - start.y) * level2.value;

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
        for level in &visible_levels {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            let y = start.y + (end.y - start.y) * level.value;

            // Draw line with extend support
            painter.hline(x_start..=x_end, y, Stroke::new(stroke::HAIRLINE, lvl_color));

            // Calculate actual price at this level
            let price_at_lvl = if start_price != 0.0 || end_price != 0.0 {
                start_price + (end_price - start_price) * (level.value as f64)
            } else {
                0.0
            };

            // Draw label with price if show_price is enabled
            let label_text = if level.show_price && price_at_lvl != 0.0 {
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

        let start = self.points[0];
        let end = self.points[1];

        // Use configurable levels if available, otherwise use extension defaults
        let config = self
            .fib_config
            .as_ref()
            .cloned()
            .unwrap_or_else(FibonacciConfig::extension_default);

        // Collect visible levels
        let visible_levels: Vec<_> = config.levels.iter().filter(|l| l.visible).collect();

        // Get actual price values from chart_points if available
        let (start_price, end_price) = if self.chart_points.len() >= 2 {
            (self.chart_points[0].price, self.chart_points[1].price)
        } else {
            (0.0, 0.0)
        };

        // Draw filled zones between levels if enabled
        if config.show_background && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];
                let level2 = visible_levels[i + 1];

                let y1 = start.y + (end.y - start.y) * level1.value;
                let y2 = start.y + (end.y - start.y) * level2.value;

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

        for level in &visible_levels {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            let y = start.y + (end.y - start.y) * level.value;

            // Only draw if within chart rect
            if y >= chart_rect.min.y && y <= chart_rect.max.y {
                // Draw line with extend support
                painter.hline(x_start..=x_end, y, Stroke::new(stroke::HAIRLINE, lvl_color));

                // Calculate actual price at this level
                let price_at_lvl = if start_price != 0.0 || end_price != 0.0 {
                    start_price + (end_price - start_price) * (level.value as f64)
                } else {
                    0.0
                };

                // Draw label with price if show_price is enabled
                let label_text = if level.show_price && price_at_lvl != 0.0 {
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
            let start_angle = std::f32::consts::PI;
            let end_angle = 0.0;

            let mut prev_point: Option<Pos2> = None;
            for i in 0..=segments {
                let t = i as f32 / segments as f32;
                let angle = start_angle + (end_angle - start_angle) * t;
                let x = end.x + radius * angle.cos();
                let y = end.y + radius * angle.sin();
                let point = Pos2::new(x, y);

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

        // Base distance for time zones (distance between first two points)
        let base_distance = (end.x - start.x).abs();
        let direction = if end.x >= start.x { 1.0 } else { -1.0 };

        // Fibonacci sequence for time zones
        let fib_sequence: [u32; 12] = [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89];

        // Collect zone positions for fill
        let mut zone_positions: Vec<f32> = Vec::new();

        for (idx, &fib_num) in fib_sequence.iter().enumerate() {
            let curr_x = start.x + base_distance * (fib_num as f32) * direction;

            // Skip if outside chart
            if curr_x < chart_rect.min.x || curr_x > chart_rect.max.x {
                if curr_x > chart_rect.max.x {
                    break;
                }
                continue;
            }

            zone_positions.push(curr_x);

            // Determine line style based on position
            let (stroke_width, alpha) = if idx <= 2 {
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
            if idx > 0 || fib_num > 0 {
                let label_bg = DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.85);
                let label_text = format!("{}", fib_num);
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

        // Calculate perpendicular unit vector
        let perp_x = -dy / line_length;
        let perp_y = dx / line_length;

        // Channel width from third point if available, otherwise use 50% of line length
        let channel_width = if self.points.len() >= 3 {
            let p3 = self.points[2];
            // Project p3 onto perpendicular to get width
            let to_p3_x = p3.x - start.x;
            let to_p3_y = p3.y - start.y;
            (to_p3_x * perp_x + to_p3_y * perp_y).abs()
        } else {
            line_length * 0.5
        };

        // Draw filled zones between levels
        if config.show_background && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];
                let level2 = visible_levels[i + 1];

                let offset1 = channel_width * level1.value;
                let offset2 = channel_width * level2.value;

                let p1_l1 = Pos2::new(start.x + perp_x * offset1, start.y + perp_y * offset1);
                let p2_l1 = Pos2::new(end.x + perp_x * offset1, end.y + perp_y * offset1);
                let p1_l2 = Pos2::new(start.x + perp_x * offset2, start.y + perp_y * offset2);
                let p2_l2 = Pos2::new(end.x + perp_x * offset2, end.y + perp_y * offset2);

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

            let offset = channel_width * level.value;
            let p1 = Pos2::new(start.x + perp_x * offset, start.y + perp_y * offset);
            let p2 = Pos2::new(end.x + perp_x * offset, end.y + perp_y * offset);

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

        // Calculate direction and distances
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let is_upward = dy < 0.0;

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
            let target_y = start.y + dy * ratio;

            // Extend to chart edge
            let extend_distance = chart_rect.max.x - start.x;
            let slope = if dx.abs() > 0.001 {
                (target_y - start.y) / dx
            } else {
                0.0
            };
            let fan_end_y = start.y + slope * extend_distance;
            let fan_end = Pos2::new(chart_rect.max.x, fan_end_y);

            let line_color = level_colors[i];
            let stroke_width = if *ratio == 0.5 {
                stroke::MEDIUM
            } else {
                stroke::HAIRLINE
            };

            // Draw the fan line
            painter.line_segment([start, fan_end], Stroke::new(stroke_width, line_color));

            // Draw label with background
            let label_pos = Pos2::new(chart_rect.max.x - DESIGN_TOKENS.rounding.lg, fan_end_y);
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
            let y1 = start.y + dy * fib_ratios[i].0;
            let y2 = start.y + dy * fib_ratios[i + 1].0;

            let slope1 = if dx.abs() > 0.001 {
                (y1 - start.y) / dx
            } else {
                0.0
            };
            let slope2 = if dx.abs() > 0.001 {
                (y2 - start.y) / dx
            } else {
                0.0
            };

            let extend = chart_rect.max.x - start.x;
            let end_y1 = start.y + slope1 * extend;
            let end_y2 = start.y + slope2 * extend;

            let fill_color = Color32::from_rgba_unmultiplied(
                level_colors[i].r(),
                level_colors[i].g(),
                level_colors[i].b(),
                fill_alpha,
            );

            painter.add(egui::epaint::PathShape::convex_polygon(
                vec![
                    start,
                    Pos2::new(chart_rect.max.x, end_y1),
                    Pos2::new(chart_rect.max.x, end_y2),
                ],
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
        let base_radius = ((edge.x - center.x).powi(2) + (edge.y - center.y).powi(2)).sqrt();

        // Golden spiral approximation
        let phi: f32 = 1.618_034;
        let mut prev_point: Option<Pos2> = None;
        for i in 0..100 {
            let angle = i as f32 * 0.1;
            let r = base_radius * 0.1 * phi.powf(angle / (2.0 * std::f32::consts::PI));
            let x = center.x + r * angle.cos();
            let y = center.y + r * angle.sin();
            let point = Pos2::new(x, y);
            if let Some(prev) = prev_point {
                painter.line_segment([prev, point], Stroke::new(stroke::HAIRLINE, color));
            }
            prev_point = Some(point);
        }
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
