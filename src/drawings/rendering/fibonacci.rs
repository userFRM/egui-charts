//! Fibonacci tool rendering implementations
//!
//! Includes: retracement, extension, arc, time zones, channel, circles,
//! speed fan, spiral, wedge, and trend-based fib time.

use super::ChartView;
use super::style::circle_sides;
use crate::drawings::domain::{Drawing, DrawingToolType as Tool, FibonacciConfig, FibonacciLevel};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rangef, Rect, Stroke};

impl Drawing {
    /// Effective settings, including the fixed presentation of older drawings without a config.
    pub(crate) fn fibonacci_config(&self) -> FibonacciConfig {
        if let Some(config) = &self.fib_config {
            return config.clone();
        }
        let values: &[f32] = match self.tool_type {
            Tool::FibonacciTimeZones => {
                &[0.0, 1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0]
            }
            Tool::FibonacciSpeedFan => &[0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0],
            Tool::TrendBasedFibTime => &[0.0, 0.382, 0.5, 0.618, 1.0, 1.618, 2.618],
            Tool::FibonacciCircles => &[0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.618, 2.618],
            Tool::FibonacciSpeedResistanceArcs => &[0.236, 0.382, 0.5, 0.618, 0.786, 1.0],
            Tool::FibonacciExtension => return FibonacciConfig::extension_default(),
            Tool::FibonacciWedge => {
                let mut config = FibonacciConfig::default();
                for level in &mut config.levels {
                    level.color = self.color;
                }
                return config;
            }
            _ => return FibonacciConfig::default(),
        };
        let mut config = FibonacciConfig {
            levels: values
                .iter()
                .map(|&value| FibonacciLevel::new(value, "", self.color))
                .collect(),
            show_background: matches!(
                self.tool_type,
                Tool::FibonacciTimeZones | Tool::FibonacciSpeedFan
            ),
            background_opacity: if self.tool_type == Tool::FibonacciTimeZones {
                15
            } else {
                10
            },
            levels_as_percent: self.tool_type != Tool::FibonacciTimeZones,
            ..FibonacciConfig::default()
        };
        if self.tool_type == Tool::FibonacciSpeedFan {
            let tokens = &DESIGN_TOKENS.semantic;
            let colors = if self
                .points
                .get(1)
                .zip(self.points.first())
                .is_some_and(|(b, a)| b.y < a.y)
            {
                [
                    tokens.extended.bearish,
                    tokens.indicators.ma,
                    tokens.extended.caution,
                    tokens.drawings.fib_50,
                    tokens.indicators.ema,
                    tokens.extended.bullish,
                    tokens.extended.bullish,
                ]
            } else {
                [
                    tokens.extended.bullish,
                    tokens.extended.bullish,
                    tokens.indicators.ema,
                    tokens.drawings.fib_50,
                    tokens.extended.caution,
                    tokens.indicators.ma,
                    tokens.extended.bearish,
                ]
            };
            for (level, color) in config.levels.iter_mut().zip(colors) {
                level.color = color.to_srgba_unmultiplied();
            }
        }
        config
    }

    pub(crate) fn render_fibonacci(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let Some((config, prices)) = self.fib_levels() else {
            return;
        };

        // Each level sits at the price it is labelled with, on whatever scale the chart uses
        let visible_levels: Vec<_> = config.visible_levels().collect();
        let ys = &self.level_ys;
        if ys.len() != prices.len() {
            return;
        }

        // Fills, lines and hit regions share the levels' horizontal bounds
        let x_range = self.fib_level_x_range(&config, chart_rect);

        // Draw filled zones between levels if enabled
        if (config.show_background || self.fill_color.is_some()) && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];

                let (y1, y2) = (ys[i], ys[i + 1]);

                // Fill with semi-transparent color using config opacity
                let fill_color = self.fill_color32().unwrap_or_else(|| {
                    Color32::from_rgba_unmultiplied(
                        level1.color[0],
                        level1.color[1],
                        level1.color[2],
                        config.background_opacity,
                    )
                });
                let fill_rect = Rect::from_x_y_ranges(x_range, y1.min(y2)..=y1.max(y2));
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }
        }

        let (x_start, x_end) = (x_range.min, x_range.max);

        // Draw lines and labels for visible levels
        let mut label_ys: Vec<f32> = Vec::new();
        for ((level, &y), &price_at_lvl) in visible_levels.iter().zip(ys).zip(&prices) {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            // Draw line with extend support
            painter.hline(x_start..=x_end, y, Stroke::new(stroke::HAIRLINE, lvl_color));

            let label_text = config.label_text(level, Some(price_at_lvl));
            if label_text.is_empty()
                || label_ys
                    .iter()
                    .any(|previous| (y - previous).abs() < typography::XS + 2.0)
            {
                continue;
            }
            label_ys.push(y);

            painter.text(
                Pos2::new(
                    if config.labels_on_left {
                        x_start + DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                    } else {
                        x_end - DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                    },
                    y - DESIGN_TOKENS.rounding.xl,
                ),
                if config.labels_on_left {
                    egui::Align2::LEFT_BOTTOM
                } else {
                    egui::Align2::RIGHT_BOTTOM
                },
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

        // Each level sits at the price it is labelled with, on whatever scale the chart uses
        let visible_levels: Vec<_> = config.visible_levels().collect();
        let ys = &self.level_ys;
        if ys.len() != prices.len() {
            return;
        }

        // Fills, lines and hit regions share the levels' horizontal bounds
        let x_range = self.fib_level_x_range(&config, chart_rect);

        // Draw filled zones between levels if enabled
        if (config.show_background || self.fill_color.is_some()) && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];

                let (y1, y2) = (ys[i], ys[i + 1]);

                // Only draw if at least partially visible
                if (y1 >= chart_rect.min.y || y2 >= chart_rect.min.y)
                    && (y1 <= chart_rect.max.y || y2 <= chart_rect.max.y)
                {
                    let fill_color = self.fill_color32().unwrap_or_else(|| {
                        Color32::from_rgba_unmultiplied(
                            level1.color[0],
                            level1.color[1],
                            level1.color[2],
                            config.background_opacity,
                        )
                    });
                    let fill_rect = Rect::from_x_y_ranges(x_range, y1.min(y2)..=y1.max(y2));
                    painter.rect_filled(fill_rect, 0.0, fill_color);
                }
            }
        }

        let (x_start, x_end) = (x_range.min, x_range.max);

        let mut label_ys: Vec<f32> = Vec::new();
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

                let label_text = config.label_text(level, Some(price_at_lvl));
                if label_text.is_empty()
                    || label_ys
                        .iter()
                        .any(|previous| (y - previous).abs() < typography::XS + 2.0)
                {
                    continue;
                }
                label_ys.push(y);

                painter.text(
                    Pos2::new(
                        if config.labels_on_left {
                            x_start + DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                        } else {
                            x_end - DESIGN_TOKENS.sizing.technical_labels.fib_label_offset_x
                        },
                        y - DESIGN_TOKENS.rounding.xl,
                    ),
                    if config.labels_on_left {
                        egui::Align2::LEFT_BOTTOM
                    } else {
                        egui::Align2::RIGHT_BOTTOM
                    },
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

        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().filter(|l| l.value >= 0.0).collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        let mut previous = None;
        for level in levels {
            let radius = base_radius * level.value;
            if !radius.is_finite() {
                continue;
            }
            let segments = circle_sides(radius * painter.pixels_per_point()).div_ceil(2);
            let points: Vec<_> = (0..=segments)
                .map(|i| {
                    let angle = std::f32::consts::PI * (1.0 - i as f32 / segments as f32);
                    speed_arc_point(start, end, radius, angle)
                })
                .collect();
            if config.show_background
                && let Some(inner_radius) = previous
            {
                let inner: Vec<_> = (0..=segments)
                    .map(|i| {
                        let angle = std::f32::consts::PI * (1.0 - i as f32 / segments as f32);
                        speed_arc_point(start, end, inner_radius, angle)
                    })
                    .collect();
                paint_band(
                    painter,
                    &inner,
                    &points,
                    band_color(level, config.background_opacity),
                );
            }
            previous = Some(radius);
            let color = level_color(level);
            painter.add(egui::Shape::line(
                points,
                Stroke::new(
                    if self.fib_config.is_some() {
                        self.stroke_width
                    } else {
                        stroke::HAIRLINE
                    },
                    color,
                ),
            ));
            let text = config.label_text(level, None);
            if !text.is_empty() {
                let side = if config.labels_on_left { -1.0 } else { 1.0 };
                painter.text(
                    Pos2::new(end.x + side * (radius + DESIGN_TOKENS.rounding.lg), end.y),
                    if config.labels_on_left {
                        egui::Align2::RIGHT_CENTER
                    } else {
                        egui::Align2::LEFT_CENTER
                    },
                    text,
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
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

        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        let unit = end.x - start.x;
        if unit.abs() < 1.0 {
            return;
        }
        if config.show_background || self.fill_color.is_some() {
            for (i, pair) in levels.windows(2).enumerate() {
                if self.fib_config.is_none() && i % 2 != 0 {
                    continue;
                }
                let (a, b) = (
                    start.x + unit * pair[0].value,
                    start.x + unit * pair[1].value,
                );
                if a.is_finite() && b.is_finite() {
                    painter.rect_filled(
                        Rect::from_x_y_ranges(a.min(b)..=a.max(b), chart_rect.y_range()),
                        0.0,
                        self.fill_color32()
                            .unwrap_or_else(|| band_color(pair[1], config.background_opacity)),
                    );
                }
            }
        }
        for level in levels {
            let x = start.x + unit * level.value;
            if !chart_rect.x_range().contains(x) {
                continue;
            }
            let legacy = self.fib_config.is_none();
            let color = if legacy {
                band_color(level, if level.value > 2.0 { 180 } else { 255 })
            } else {
                level_color(level)
            };
            let width = if legacy && level.value > 2.0 {
                stroke::HAIRLINE
            } else {
                self.stroke_width
            };
            painter.vline(x, chart_rect.y_range(), Stroke::new(width, color));
            let text = config.label_text(level, None);
            if !text.is_empty() && (!legacy || level.value > 0.0) {
                let pos = Pos2::new(x, chart_rect.min.y + DESIGN_TOKENS.rounding.xl);
                if legacy {
                    painter.rect_filled(
                        Rect::from_center_size(pos, egui::vec2(24.0, 14.0)),
                        DESIGN_TOKENS.rounding.sm,
                        DESIGN_TOKENS
                            .semantic
                            .extended
                            .chart_axis_bg
                            .gamma_multiply(0.85),
                    );
                }
                painter.text(
                    pos + if legacy {
                        egui::Vec2::ZERO
                    } else {
                        egui::vec2(if config.labels_on_left { -4.0 } else { 4.0 }, 0.0)
                    },
                    if legacy {
                        egui::Align2::CENTER_CENTER
                    } else if config.labels_on_left {
                        egui::Align2::RIGHT_TOP
                    } else {
                        egui::Align2::LEFT_TOP
                    },
                    text,
                    egui::FontId::proportional(typography::XS),
                    level_color(level),
                );
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
    pub(crate) fn render_fibonacci_channel(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
        view: &ChartView,
    ) {
        let Some((config, lines)) = self.fib_channel_level_lines(chart_rect) else {
            return;
        };
        let (start, end) = (self.points[0], self.points[1]);
        let visible_levels: Vec<_> = config.visible_levels().collect();

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Draw filled zones between levels
        if (config.show_background || self.fill_color.is_some()) && visible_levels.len() > 1 {
            for i in 0..visible_levels.len() - 1 {
                let level1 = visible_levels[i];
                let [p1_l1, p2_l1] = lines[i];
                let [p1_l2, p2_l2] = lines[i + 1];

                let fill_color = self.fill_color32().unwrap_or_else(|| {
                    Color32::from_rgba_unmultiplied(
                        level1.color[0],
                        level1.color[1],
                        level1.color[2],
                        config.background_opacity,
                    )
                });

                painter.add(egui::epaint::PathShape::convex_polygon(
                    vec![p1_l1, p2_l1, p2_l2, p1_l2],
                    fill_color,
                    Stroke::NONE,
                ));
            }
        }

        // Draw channel level lines
        for (level, &[draw_start, draw_end]) in visible_levels.iter().zip(&lines) {
            let lvl_color = Color32::from_rgba_unmultiplied(
                level.color[0],
                level.color[1],
                level.color[2],
                level.color[3],
            );

            let stroke_width = if level.value == 0.0 || level.value == 1.0 {
                self.stroke_width
            } else {
                stroke::HAIRLINE
            };
            painter.line_segment([draw_start, draw_end], Stroke::new(stroke_width, lvl_color));

            let point = if config.labels_on_left {
                draw_start
            } else {
                draw_end
            };
            let price = if config.show_prices == Some(true) {
                let price = (view.y_to_price)(point.y);
                price.is_finite().then_some(price)
            } else {
                None
            };
            let label_text = config.label_text(level, price);
            if !label_text.is_empty() {
                let galley = painter.layout_no_wrap(
                    label_text,
                    egui::FontId::proportional(typography::XS),
                    lvl_color,
                );
                let alignment = if config.labels_on_left {
                    egui::Align2::LEFT_BOTTOM
                } else {
                    egui::Align2::RIGHT_BOTTOM
                };
                let bounds = alignment.anchor_size(
                    point + egui::vec2(if config.labels_on_left { 4.0 } else { -4.0 }, -4.0),
                    galley.size(),
                );
                painter.rect_filled(
                    bounds.expand(2.0),
                    DESIGN_TOKENS.rounding.sm,
                    DESIGN_TOKENS
                        .semantic
                        .extended
                        .chart_axis_bg
                        .gamma_multiply(0.8),
                );
                painter.galley(bounds.min, galley, lvl_color);
            }
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
        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().filter(|l| l.value >= 0.0).collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        let mut previous = None;
        for level in levels {
            let radius = base_radius * level.value;
            if !radius.is_finite() {
                continue;
            }
            if config.show_background
                && let Some(inner_radius) = previous
            {
                let segments = circle_sides(radius * painter.pixels_per_point());
                let ring = |r| {
                    (0..=segments)
                        .map(|i| {
                            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
                            center + r * egui::vec2(angle.cos(), angle.sin())
                        })
                        .collect::<Vec<_>>()
                };
                paint_band(
                    painter,
                    &ring(inner_radius),
                    &ring(radius),
                    band_color(level, config.background_opacity),
                );
            }
            previous = Some(radius);
            let color = level_color(level);
            painter.circle_stroke(
                center,
                radius,
                Stroke::new(
                    if self.fib_config.is_some() {
                        self.stroke_width
                    } else {
                        stroke::HAIRLINE
                    },
                    color,
                ),
            );
            let text = config.label_text(level, None);
            if !text.is_empty() {
                let side = if config.labels_on_left { -1.0 } else { 1.0 };
                painter.text(
                    Pos2::new(
                        center.x + side * (radius + DESIGN_TOKENS.rounding.sm),
                        center.y,
                    ),
                    if config.labels_on_left {
                        egui::Align2::RIGHT_CENTER
                    } else {
                        egui::Align2::LEFT_CENTER
                    },
                    text,
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
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

        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        if config.show_background || self.fill_color.is_some() {
            for pair in levels.windows(2) {
                let a = speed_fan_line_end(start, end, pair[0].value, chart_rect);
                let b = speed_fan_line_end(start, end, pair[1].value, chart_rect);
                if a.is_finite() && b.is_finite() {
                    painter.add(egui::Shape::convex_polygon(
                        vec![start, a, b],
                        self.fill_color32().unwrap_or_else(|| {
                            band_color(
                                if self.fib_config.is_none() {
                                    pair[0]
                                } else {
                                    pair[1]
                                },
                                config.background_opacity,
                            )
                        }),
                        Stroke::NONE,
                    ));
                }
            }
        }
        let mut labels = Vec::<Rect>::new();
        for level in levels {
            let line_color = level_color(level);
            let width = if self.fib_config.is_some() {
                self.stroke_width
            } else if level.value == 0.5 {
                stroke::MEDIUM
            } else {
                stroke::HAIRLINE
            };
            let fan_end = speed_fan_line_end(start, end, level.value, chart_rect);
            if !fan_end.is_finite() {
                continue;
            }
            painter.line_segment([start, fan_end], Stroke::new(width, line_color));
            let text = config.label_text(level, None);
            let position =
                if self.fib_config.is_none() || (fan_end.x < start.x) == config.labels_on_left {
                    fan_end
                } else {
                    start
                };
            if chart_rect.y_range().contains(position.y) && !text.is_empty() {
                let galley = painter.layout_no_wrap(
                    text,
                    egui::FontId::proportional(typography::XS),
                    line_color,
                );
                let align = if config.labels_on_left {
                    egui::Align2::LEFT_BOTTOM
                } else {
                    egui::Align2::RIGHT_BOTTOM
                };
                let bounds = align.anchor_size(
                    position + egui::vec2(if config.labels_on_left { 4.0 } else { -4.0 }, -4.0),
                    galley.size(),
                );
                if !labels.iter().any(|r| r.intersects(bounds.expand(2.0))) {
                    painter.galley(bounds.min, galley, line_color);
                    labels.push(bounds);
                }
            }
            for [a, b] in speed_fan_level_lines(start, end, level.value, chart_rect) {
                if a.is_finite() && b.is_finite() {
                    painter.line_segment(
                        [a, b],
                        Stroke::new(
                            stroke::HAIRLINE,
                            if self.fib_config.is_some() {
                                line_color
                            } else {
                                color.gamma_multiply(0.45)
                            },
                        ),
                    );
                }
            }
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

    pub(crate) fn render_fibonacci_wedge(&self, painter: &egui::Painter, rect: Rect) {
        let &[start, edge, boundary, ..] = self.points.as_slice() else {
            return;
        };
        let config = self.fibonacci_config();
        let painter = painter.with_clip_rect(rect);
        let mut levels: Vec<_> = config
            .visible_levels()
            .filter(|l| l.value >= 0.0 && l.value.is_finite())
            .collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        let mut previous: Option<Vec<Pos2>> = None;
        for level in levels {
            let points = wedge_arc_points(start, edge, boundary, level.value);
            if config.show_background
                && let Some(inner) = previous.as_ref()
            {
                paint_band(
                    &painter,
                    inner,
                    &points,
                    band_color(level, config.background_opacity),
                );
            }
            for pair in points.windows(2) {
                self.styled_segment(
                    &painter,
                    pair[0],
                    pair[1],
                    Stroke::new(self.stroke_width, level_color(level)),
                );
            }
            if config.show_levels == Some(true) {
                let a = points[0];
                let b = *points.last().unwrap();
                let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
                let at = if config.labels_on_left { left } else { right };
                painter.text(
                    at,
                    if config.labels_on_left {
                        egui::Align2::RIGHT_BOTTOM
                    } else {
                        egui::Align2::LEFT_BOTTOM
                    },
                    config.label_text(level, None),
                    egui::FontId::proportional(typography::XS),
                    level_color(level),
                );
            }
            previous = Some(points);
        }
        if let Some(outer) = previous {
            for end in [outer[0], *outer.last().unwrap()] {
                self.styled_segment(
                    &painter,
                    start,
                    end,
                    Stroke::new(self.stroke_width, self.color32()),
                );
            }
        }
    }

    pub(super) fn fibonacci_wedge_outline(&self) -> Option<super::Outline> {
        let &[start, edge, boundary, ..] = self.points.as_slice() else {
            return None;
        };
        let config = self.fib_config.clone().unwrap_or_default();
        let mut outline = super::Outline::default();
        let mut outer = 0.0_f32;
        for level in config
            .visible_levels()
            .filter(|l| l.value.is_finite() && l.value >= 0.0)
        {
            let points = wedge_arc_points(start, edge, boundary, level.value);
            outline
                .lines
                .extend(points.windows(2).map(|w| [w[0], w[1]]));
            outer = outer.max(level.value);
        }
        let outer = wedge_arc_points(start, edge, boundary, outer);
        outline
            .lines
            .extend([[start, outer[0]], [start, *outer.last()?]]);
        Some(outline)
    }

    pub(crate) fn render_trend_based_fib_time(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];
        let dx = p2.x - p1.x;
        let config = self.fibonacci_config();
        let origin = self.points.get(2).unwrap_or(&p1);
        let mut levels: Vec<_> = config.visible_levels().collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        if config.show_background {
            for pair in levels.windows(2) {
                let a = origin.x + dx * pair[0].value;
                let b = origin.x + dx * pair[1].value;
                if a.is_finite() && b.is_finite() {
                    painter.rect_filled(
                        Rect::from_x_y_ranges(a.min(b)..=a.max(b), chart_rect.y_range()),
                        0.0,
                        band_color(pair[1], config.background_opacity),
                    );
                }
            }
        }
        let mut labels = Vec::<Rect>::new();
        for level in levels {
            let x = origin.x + dx * level.value;
            if !x.is_finite() {
                continue;
            }
            let color = level_color(level);
            painter.vline(
                x,
                chart_rect.y_range(),
                Stroke::new(
                    if self.fib_config.is_some() {
                        self.stroke_width
                    } else {
                        stroke::HAIRLINE
                    },
                    color,
                ),
            );
            let text = config.label_text(level, None);
            if text.is_empty() {
                continue;
            }
            let galley =
                painter.layout_no_wrap(text, egui::FontId::proportional(typography::XS), color);
            let rect = egui::Align2::CENTER_TOP.anchor_size(
                Pos2::new(x, chart_rect.min.y + typography::XS),
                galley.size(),
            );
            if !labels.iter().any(|r| r.intersects(rect.expand(2.0))) {
                painter.galley(rect.min, galley, color);
                labels.push(rect);
            }
        }
    }
}

impl Drawing {
    /// The x extent of a Fibonacci retracement's or extension's level lines:
    /// between the anchors, or on to the edge of `chart_rect` on each side the
    /// config extends.
    pub(crate) fn fib_level_x_range(&self, config: &FibonacciConfig, chart_rect: Rect) -> Rangef {
        let (a, b) = (self.points[0].x, self.points[1].x);
        Rangef::new(
            if config.extend_left {
                chart_rect.min.x
            } else {
                a.min(b)
            },
            if config.extend_right {
                chart_rect.max.x
            } else {
                a.max(b)
            },
        )
    }

    /// The Fibonacci channel's config and the line of each visible level as
    /// drawn, run on to the edge of `rect` on each side (left or right on
    /// screen) the config extends; `None` below two anchors or when they nearly
    /// coincide. Without a third anchor the channel is half as wide as the base
    /// line is long.
    pub(crate) fn fib_channel_level_lines(
        &self,
        rect: Rect,
    ) -> Option<(FibonacciConfig, Vec<[Pos2; 2]>)> {
        let &[start, end, ..] = self.points.as_slice() else {
            return None;
        };
        let d = end - start;
        if d.length() < 1.0 {
            return None;
        }
        let config = self.fib_config.clone().unwrap_or_default();
        let width_point = self
            .points
            .get(2)
            .copied()
            .unwrap_or_else(|| start + egui::vec2(-d.y, d.x) * 0.5);
        let forward = if d.x.abs() < 1e-6 {
            d.y >= 0.0
        } else {
            d.x > 0.0
        };
        let lines = config
            .visible_levels()
            .map(|level| {
                let [a, b] = fib_channel_level_line(start, end, width_point, level.value);
                let (left, right) = if forward { (a, b) } else { (b, a) };
                [
                    if config.extend_left {
                        super::lines::ray_end(right, left, rect)
                    } else {
                        left
                    },
                    if config.extend_right {
                        super::lines::ray_end(left, right, rect)
                    } else {
                        right
                    },
                ]
            })
            .collect();
        Some((config, lines))
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

fn speed_fan_level_lines(start: Pos2, end: Pos2, ratio: f32, rect: Rect) -> Vec<[Pos2; 2]> {
    let d = end - start;
    let mut lines = Vec::new();
    if ratio < 1.0 {
        let through = start + egui::vec2(d.x * ratio, d.y);
        lines.push([start, super::lines::ray_end(start, through, rect)]);
    }
    let at = start + d * ratio;
    lines.extend([
        [Pos2::new(at.x, start.y), Pos2::new(at.x, end.y)],
        [Pos2::new(start.x, at.y), Pos2::new(end.x, at.y)],
    ]);
    let arc: Vec<_> = (0..=32)
        .map(|i| {
            let theta = std::f32::consts::FRAC_PI_2 * i as f32 / 32.0;
            start + egui::vec2(d.x * ratio * theta.cos(), d.y * ratio * theta.sin())
        })
        .collect();
    lines.extend(arc.windows(2).map(|w| [w[0], w[1]]));
    lines
}

impl Drawing {
    pub(super) fn fibonacci_time_zone_lines(&self, rect: Rect) -> Vec<[Pos2; 2]> {
        let unit = self.points[1].x - self.points[0].x;
        if unit.abs() < 1.0 {
            return Vec::new();
        }
        self.fibonacci_config()
            .visible_levels()
            .filter_map(|level| {
                let x = self.points[0].x + unit * level.value;
                rect.x_range()
                    .contains(x)
                    .then(|| [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)])
            })
            .collect()
    }

    pub(super) fn fibonacci_speed_fan_lines(&self, rect: Rect) -> Vec<[Pos2; 2]> {
        let (start, end) = (self.points[0], self.points[1]);
        self.fibonacci_config()
            .visible_levels()
            .flat_map(|level| {
                std::iter::once([start, speed_fan_line_end(start, end, level.value, rect)])
                    .chain(speed_fan_level_lines(start, end, level.value, rect))
            })
            .filter(|line| line.iter().all(|p| p.is_finite()))
            .collect()
    }
}

fn level_color(level: &FibonacciLevel) -> Color32 {
    Color32::from_rgba_unmultiplied(
        level.color[0],
        level.color[1],
        level.color[2],
        level.color[3],
    )
}

fn band_color(level: &FibonacciLevel, opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(level.color[0], level.color[1], level.color[2], opacity)
}

/// One mesh avoids antialiased seams between adjoining ring segments.
fn paint_band(painter: &egui::Painter, inner: &[Pos2], outer: &[Pos2], color: Color32) {
    if inner.len() != outer.len() || inner.iter().chain(outer).any(|p| !p.is_finite()) {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for (&inside, &outside) in inner.iter().zip(outer) {
        mesh.colored_vertex(inside, color);
        mesh.colored_vertex(outside, color);
    }
    for i in 1..inner.len() {
        let end = (i * 2) as u32;
        mesh.add_triangle(end - 2, end - 1, end + 1);
        mesh.add_triangle(end - 2, end + 1, end);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Circular ratio arcs between the two trend rays, following their shorter signed sweep.
fn wedge_arc_points(start: Pos2, edge: Pos2, boundary: Pos2, ratio: f32) -> Vec<Pos2> {
    let a = edge - start;
    let b = boundary - start;
    let angle = a.y.atan2(a.x);
    let sweep = (a.x * b.y - a.y * b.x).atan2(a.dot(b));
    (0..=64)
        .map(|i| {
            let theta = angle + sweep * i as f32 / 64.0;
            start + a.length() * ratio * egui::vec2(theta.cos(), theta.sin())
        })
        .collect()
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

    /// Saved ratios, visibility and colours drive every formerly fixed renderer and its hit lines.
    #[test]
    fn configured_fibonacci_levels_drive_paint_and_selection() {
        use crate::drawings::ChartPoint;
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        let red = Color32::from_rgb(211, 41, 73);
        let green = Color32::from_rgb(37, 157, 89);
        let hidden = Color32::from_rgb(61, 79, 199);
        for tool in [
            Tool::FibonacciTimeZones,
            Tool::TrendBasedFibTime,
            Tool::FibonacciSpeedFan,
            Tool::FibonacciCircles,
            Tool::FibonacciSpeedResistanceArcs,
            Tool::FibonacciWedge,
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.points = vec![
                Pos2::new(100.0, 200.0),
                Pos2::new(200.0, 100.0),
                Pos2::new(300.0, 150.0),
            ];
            drawing.chart_points = vec![
                ChartPoint::new(100.0, 100.0),
                ChartPoint::new(200.0, 200.0),
                ChartPoint::new(300.0, 150.0),
            ];
            for background in [false, true] {
                drawing.fib_config = Some(serde_json::from_value(serde_json::json!({
                    "levels":[
                        {"value":0.25,"label":"","color":[211,41,73,255],"visible":true,"show_price":true},
                        {"value":0.5,"label":"","color":[37,157,89,255],"visible":true,"show_price":true},
                        {"value":0.75,"label":"","color":[61,79,199,255],"visible":false,"show_price":true},
                        {"value":1.25,"label":"","color":[37,157,89,255],"visible":true,"show_price":true}
                    ],"show_background":background,"background_opacity":37,
                    "extend_left":false,"extend_right":false,"reverse":false,
                    "levels_as_percent":false,"show_prices":false
                })).unwrap());
                let mut output =
                    egui::Context::default().run_ui(egui::RawInput::default(), |ui| match tool {
                        Tool::FibonacciTimeZones => {
                            drawing.render_fibonacci_time_zones(ui.painter(), rect)
                        }
                        Tool::TrendBasedFibTime => {
                            drawing.render_trend_based_fib_time(ui.painter(), rect)
                        }
                        Tool::FibonacciSpeedFan => {
                            drawing.render_fibonacci_speed_fan(ui.painter(), rect)
                        }
                        Tool::FibonacciCircles => drawing.render_fibonacci_circles(ui.painter()),
                        Tool::FibonacciSpeedResistanceArcs => {
                            drawing.render_fibonacci_arc(ui.painter())
                        }
                        Tool::FibonacciWedge => drawing.render_fibonacci_wedge(ui.painter(), rect),
                        _ => unreachable!(),
                    });
                output.textures_delta.clear();
                let strokes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::LineSegment { stroke, .. } => Some(stroke.color),
                        egui::Shape::Path(p) => match p.stroke.color {
                            egui::epaint::ColorMode::Solid(color) => Some(color),
                            _ => None,
                        },
                        egui::Shape::Circle(c) => Some(c.stroke.color),
                        _ => None,
                    })
                    .collect();
                assert!(
                    strokes.contains(&red) && strokes.contains(&green),
                    "{tool:?}: configured colours"
                );
                assert!(!strokes.contains(&hidden), "{tool:?}: hidden level");
                let fill = output.shapes.iter().any(|s| match &s.shape {
                    egui::Shape::Rect(r) => {
                        r.fill == Color32::from_rgba_unmultiplied(37, 157, 89, 37)
                    }
                    egui::Shape::Path(p) => {
                        p.fill == Color32::from_rgba_unmultiplied(37, 157, 89, 37)
                    }
                    egui::Shape::Mesh(m) => m
                        .vertices
                        .iter()
                        .any(|v| v.color == Color32::from_rgba_unmultiplied(37, 157, 89, 37)),
                    _ => false,
                });
                assert_eq!(
                    fill, background,
                    "{tool:?}: background switch and stored opacity"
                );
                // A->B spans 100 px in time and -100 px in price. A quarter level is
                // x=125 (zones), x=325 (projection from C), or radius sqrt(20000)/4.
                let red_segment = |hit: &dyn Fn(&[Pos2; 2]) -> bool| {
                    output.shapes.iter().any(|s| match &s.shape {
                        egui::Shape::LineSegment { points, stroke } => {
                            stroke.color == red && hit(points)
                        }
                        _ => false,
                    })
                };
                match tool {
                    Tool::FibonacciTimeZones | Tool::TrendBasedFibTime => {
                        let x = if tool == Tool::FibonacciTimeZones {
                            125.0
                        } else {
                            325.0
                        };
                        assert!(
                            red_segment(&|p| p[0].x == x && p[1].x == x),
                            "{tool:?}: quarter interval"
                        );
                        if tool == Tool::FibonacciTimeZones {
                            let lines = drawing.outline(rect).unwrap().lines;
                            assert!(
                                lines.iter().any(|l| l[0].x == 225.0),
                                "added 1.25 level is selectable"
                            );
                            assert!(
                                !lines.iter().any(|l| l[0].x == 175.0),
                                "hidden level has no hit line"
                            );
                        }
                    }
                    Tool::FibonacciCircles => {
                        assert!(output.shapes.iter().any(|s| match &s.shape {
                            egui::Shape::Circle(c) => {
                                c.stroke.color == red && (c.radius - 35.35534).abs() < 1e-3
                            }
                            _ => false,
                        }))
                    }
                    Tool::FibonacciSpeedFan => {
                        let lines = drawing.outline(rect).unwrap().lines;
                        let line = |a: Pos2, b: Pos2| lines.iter().any(|l| l[0] == a && l[1] == b);
                        assert!(
                            line(Pos2::new(100.0, 200.0), Pos2::new(500.0, 100.0)),
                            "quarter price ray"
                        );
                        assert!(
                            !line(Pos2::new(175.0, 200.0), Pos2::new(175.0, 100.0)),
                            "hidden grid line"
                        );
                    }
                    Tool::FibonacciSpeedResistanceArcs | Tool::FibonacciWedge => {
                        let want = if tool == Tool::FibonacciSpeedResistanceArcs {
                            Pos2::new(235.35534, 100.0)
                        } else {
                            Pos2::new(125.0, 175.0)
                        };
                        let near = |p: &Pos2| p.distance(want) < 1e-3;
                        let on_path = output.shapes.iter().any(|s| match &s.shape {
                            egui::Shape::Path(p) => p.points.iter().any(near),
                            _ => false,
                        });
                        assert!(
                            on_path || red_segment(&|p| p.iter().any(near)),
                            "{tool:?}: quarter radius"
                        );
                    }
                    _ => unreachable!(),
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Drawing-wide choices reach the painted labels, including a channel on a log scale.
    #[test]
    fn fibonacci_label_content_and_side_follow_settings() {
        use crate::drawings::ChartPoint;
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        for (tool, log, ratio, price) in [
            (Tool::FibonacciRetracement, false, 0.25, 175.0),
            (Tool::FibonacciExtension, false, 0.25, 175.0),
            (Tool::FibonacciChannel, false, 0.5, 125.0),
            // At B=200 the channel's base price is 200. Its width point is 150
            // at C, where the base is 400: half the log offset gives 200*sqrt(150/400).
            (Tool::FibonacciChannel, true, 0.5, 122.4744871392),
        ] {
            for (show_prices, show_levels, percent, left) in [
                (true, true, false, true),
                (false, true, true, false),
                (true, false, true, false),
                (false, false, false, true),
            ] {
                let mut drawing = Drawing::new(0, tool);
                drawing.chart_points = vec![
                    ChartPoint::new(40.0, 100.0),
                    ChartPoint::new(360.0, 200.0),
                    ChartPoint::new(680.0, 150.0),
                ];
                drawing.fib_config=Some(serde_json::from_value(serde_json::json!({
                    "levels":[{"value":ratio,"label":"","color":[211,41,73,255],"visible":true,"show_price":false}],
                    "show_background":false,"background_opacity":37,"extend_left":false,"extend_right":false,"reverse":false,
                    "show_prices":show_prices,"show_levels":show_levels,"levels_as_percent":percent,"labels_on_left":left
                })).unwrap());
                let mode = if log {
                    crate::scales::PriceScaleMode::Logarithmic
                } else {
                    crate::scales::PriceScaleMode::Normal
                };
                let transform = crate::scales::PriceTransform::new(mode, 100.0);
                let map = crate::scales::PriceMap::new(
                    transform,
                    crate::scales::PriceRange::new(
                        transform.to_scale(10.0),
                        transform.to_scale(1000.0),
                    ),
                    0.0,
                    400.0,
                    false,
                );
                let y = |p| map.price_to_y(p);
                drawing.update_screen_coords(|b| b, y);
                let view = ChartView {
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b,
                    price_to_y: &y,
                    y_to_price: &|y| map.y_to_price(y),
                    time_axis: true,
                    executions: None,
                };
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    drawing.render_original(ui.painter(), rect, &view)
                });
                output.textures_delta.clear();
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect();
                let value = if percent {
                    format!("{}%", ratio * 100.0)
                } else {
                    ratio.to_string()
                };
                // The left channel label sits at A, whose base price is half B's.
                let price = if tool == Tool::FibonacciChannel && left {
                    if log { price / 2.0 } else { price - 100.0 }
                } else {
                    price
                };
                let expected = match (show_prices, show_levels) {
                    (true, true) => format!("{value} ({price:.2})"),
                    (false, true) => value,
                    (true, false) => format!("{price:.2}"),
                    (false, false) => String::new(),
                };
                if expected.is_empty() {
                    assert!(texts.is_empty(), "{tool:?}");
                } else {
                    assert_eq!(texts.len(), 1, "{tool:?}");
                    assert_eq!(
                        texts[0].galley.text(),
                        expected,
                        "{tool:?}, log={log}, left={left}"
                    );
                    assert_eq!(texts[0].pos.x < 200.0, left, "{tool:?}: label side");
                }
                output.drop_without_applying_deltas();
            }
        }
    }

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
        let mut zones = Drawing::new(0, Tool::FibonacciTimeZones);
        zones.fib_config = None;
        zones.points = vec![Pos2::new(100.0, 100.0), Pos2::new(110.0, 100.0)];
        let got: Vec<_> = zones
            .fibonacci_time_zone_lines(rect)
            .into_iter()
            .filter(|line| line[0].x <= 300.0)
            .map(|line| (((line[0].x - 100.0) / 10.0) as u32, line[0].x))
            .collect();
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

        // A leftward zone still fills a non-empty rectangle between consecutive times.
        let mut zones = Drawing::new(0, crate::drawings::DrawingToolType::FibonacciTimeZones);
        zones.fib_config = None;
        zones.color[3] = 128;
        zones.points = vec![Pos2::new(200.0, 100.0), Pos2::new(190.0, 100.0)];
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            zones.render_fibonacci_time_zones(ui.painter(), rect);
        });
        let filled = output.shapes.iter().any(|s| {
            matches!(&s.shape,
            egui::Shape::Rect(r) if r.rect.min.x == 190.0 && r.rect.max.x == 200.0
                && r.rect.height() == rect.height())
        });
        let opaque_origin = output.shapes.iter().any(|s| matches!(&s.shape,
            egui::Shape::LineSegment {points,stroke} if points[0].x==200.0 && points[1].x==200.0 && stroke.color.a()==255));
        output.drop_without_applying_deltas();
        assert!(filled);
        assert!(
            opaque_origin,
            "legacy time zones keep their prominent lines opaque"
        );

        // A fan saved without levels keeps its medium 50% ray.
        let mut fan = Drawing::new(0, Tool::FibonacciSpeedFan);
        fan.fib_config = None;
        fan.points = vec![Pos2::new(100.0, 200.0), Pos2::new(200.0, 100.0)];
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            fan.render_fibonacci_speed_fan(ui.painter(), rect);
        });
        let medium = output.shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::LineSegment { stroke, .. } if stroke.width == stroke::MEDIUM)
        });
        output.drop_without_applying_deltas();
        assert!(medium, "legacy fans keep their medium 50% ray");

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

        // Both fan axes: the 50% time ray goes through (50,100), the
        // 50% price ray through (100,50), with grid and quarter ellipses between them.
        for end in [Pos2::new(100.0, 100.0), Pos2::new(-100.0, -100.0)] {
            let mut fan = Drawing::new(0, Tool::FibonacciSpeedFan);
            fan.points = vec![Pos2::ZERO, end];
            let lines = fan.fibonacci_speed_fan_lines(Rect::from_min_max(
                Pos2::new(-200.0, -400.0),
                Pos2::new(200.0, 400.0),
            ));
            let ray = [Pos2::ZERO, Pos2::new(end.x * 2.0, end.y * 4.0)];
            assert!(
                lines
                    .iter()
                    .any(|l| l[0].distance(ray[0]) < 1e-3 && l[1].distance(ray[1]) < 1e-3),
                "missing time ray: {lines:?}"
            );
            assert!(lines.contains(&[Pos2::new(end.x * 0.5, 0.0), Pos2::new(end.x * 0.5, end.y)]));
        }
        let mut wedge = Drawing::new(0, crate::drawings::DrawingToolType::FibonacciWedge);
        wedge.points = vec![Pos2::ZERO, Pos2::new(100.0, 0.0), Pos2::new(0.0, 200.0)];
        let outline = wedge.outline(rect).unwrap();
        // A quarter circle at level 0.5: (50,0) through (sqrt(1250),sqrt(1250)) to (0,50).
        for want in [
            Pos2::new(50.0, 0.0),
            Pos2::new(35.35534, 35.35534),
            Pos2::new(0.0, 50.0),
        ] {
            assert!(
                outline.lines.iter().flatten().any(|p| near(*p, want)),
                "missing wedge point {want:?}"
            );
        }

        let mut extension = Drawing::new(0, crate::drawings::DrawingToolType::FibonacciExtension);
        extension.chart_points = vec![
            crate::drawings::ChartPoint::new(0.0, 100.0),
            crate::drawings::ChartPoint::new(10.0, 150.0),
            crate::drawings::ChartPoint::new(30.0, 130.0),
        ];
        for (start, end, origin, logarithmic, ratio, want) in [
            (100.0, 150.0, 130.0, false, 0.0, 130.0),
            (100.0, 150.0, 130.0, false, 0.618, 160.9),
            (100.0, 150.0, 130.0, false, 1.0, 180.0),
            // C * (B/A)^0.5 = 20 * sqrt(100) = 200.
            (10.0, 1000.0, 20.0, true, 0.5, 200.0),
            (10.0, 1000.0, 0.0, true, 0.5, 495.0),
            (10.0, 1000.0, -20.0, true, 0.5, 475.0),
            (0.0, 1000.0, 20.0, true, 0.5, 520.0),
            (10.0, 0.0, 20.0, true, 0.5, 15.0),
        ] {
            extension.chart_points[0].price = start;
            extension.chart_points[1].price = end;
            extension.chart_points[2].price = origin;
            extension.fib_config = Some(FibonacciConfig {
                logarithmic,
                levels: vec![crate::drawings::FibonacciLevel::new(ratio, "level", [0; 4])],
                ..FibonacciConfig::extension_default()
            });
            let (_, prices) = extension.fib_levels().unwrap();
            assert!((prices[0] - want).abs() < 1e-4);
        }
        extension.update_screen_coords(|b| b, |p| (1000.0 - p) as f32);
        extension.tool_type = crate::drawings::DrawingToolType::TrendBasedFibTime;
        extension.fib_config = None;
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            extension.render_trend_based_fib_time(ui.painter(), rect)
        });
        let has_origin=output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::LineSegment{points,..} if points[0].x==30.0 && points[1].x==30.0));
        output.drop_without_applying_deltas();
        assert!(has_origin, "time projections must start at C, bar 30");

        // Retracement fills span the level lines: the anchors at x 10 and 50, or on to the
        // pane's right edge when only the right side extends.
        let mut retracement =
            Drawing::new(0, crate::drawings::DrawingToolType::FibonacciRetracement);
        retracement.chart_points = vec![
            crate::drawings::ChartPoint::new(10.0, 100.0),
            crate::drawings::ChartPoint::new(50.0, 200.0),
        ];
        for (extend_right, want) in [(false, 10.0..=50.0), (true, 10.0..=400.0)] {
            retracement.fib_config = Some(FibonacciConfig {
                show_background: true,
                extend_right,
                ..FibonacciConfig::default()
            });
            retracement.update_screen_coords(|b| b, |p| (300.0 - p) as f32);
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                retracement.render_fibonacci(ui.painter(), rect)
            });
            let fills: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r) => Some(r.rect.min.x..=r.rect.max.x),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            if fills.is_empty() || fills.iter().any(|x| *x != want) {
                wrong.push(format!("retracement fills {fills:?}, want {want:?}"));
            }
        }

        // A channel drawn right to left from (300, 100) to (100, 200), extended on the left only:
        // each level runs from the pane's left edge to its right anchor at x 300.
        let mut channel = Drawing::new(0, crate::drawings::DrawingToolType::FibonacciChannel);
        channel.points = vec![
            Pos2::new(300.0, 100.0),
            Pos2::new(100.0, 200.0),
            Pos2::new(200.0, 100.0),
        ];
        channel.fib_config = Some(FibonacciConfig {
            extend_left: true,
            ..FibonacciConfig::default()
        });
        let (_, lines) = channel.fib_channel_level_lines(rect).unwrap();
        for [left, right] in lines {
            if left.x != rect.min.x || right.x != 300.0 {
                wrong.push(format!("channel level from {left:?} to {right:?}"));
            }
        }

        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
