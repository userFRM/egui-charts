//! Fibonacci tool rendering implementations
//!
//! Includes: retracement, extension, arc, time zones, channel, circles,
//! speed fan, spiral, wedge, and trend-based fib time.

use super::ChartView;
use super::style::{Acceleration, ellipse_arc, ellipse_point, sample_curve, smooth_path};
use crate::drawings::domain::{Drawing, DrawingToolType as Tool, FibonacciConfig, FibonacciLevel};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rangef, Rect, Stroke};
use std::borrow::Cow;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy)]
struct LevelLabelKey<'a> {
    config: &'a FibonacciConfig,
    level: &'a FibonacciLevel,
    price: Option<f64>,
    formatted_price: Option<&'a str>,
    precision: u8,
}

impl Hash for LevelLabelKey<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (
            self.level.value.to_bits(),
            &self.level.label,
            self.level.show_price,
            self.config.show_prices,
            self.config.show_levels,
            self.config.levels_as_percent,
            self.price.map(f64::to_bits),
            self.formatted_price,
            self.precision,
        )
            .hash(state);
    }
}

#[derive(Default)]
struct LevelLabelFormatter;

impl egui::cache::ComputerMut<LevelLabelKey<'_>, String> for LevelLabelFormatter {
    fn compute(&mut self, key: LevelLabelKey<'_>) -> String {
        use crate::scales::PriceFormatter;
        let price = key
            .price
            .filter(|_| key.formatted_price.is_none())
            .map(|price| {
                crate::scales::DefaultPriceFormatter::with_precision(usize::from(key.precision))
                    .format(price)
            });
        key.config
            .label_text(key.level, key.formatted_price.or(price.as_deref()))
    }
}

fn level_galley(
    painter: &egui::Painter,
    config: &FibonacciConfig,
    level: &FibonacciLevel,
    price: Option<f64>,
    view: Option<&ChartView>,
) -> std::sync::Arc<egui::Galley> {
    let price = price.filter(|_| config.show_prices.unwrap_or(level.show_price));
    let formatted_price = price
        .zip(view.and_then(|v| v.price_formatter))
        .map(|(price, formatter)| formatter.format(price));
    let key = LevelLabelKey {
        config,
        level,
        price,
        formatted_price: formatted_price.as_deref(),
        precision: view
            .and_then(|v| v.instrument)
            .and_then(crate::scales::instrument_price_precision)
            .unwrap_or(2),
    };
    // Labels survive pan/zoom; the frame cache evicts text no longer used by any drawing.
    let text = painter.ctx().memory_mut(|memory| {
        memory
            .caches
            .cache::<egui::cache::FrameCache<String, LevelLabelFormatter>>()
            .get(key)
            .clone()
    });
    painter.layout_no_wrap(
        text,
        egui::FontId::proportional(typography::font_size(config.label_size)),
        level_color(level),
    )
}

impl Drawing {
    /// Effective settings, including the fixed presentation of older drawings without a config.
    pub(crate) fn fibonacci_config(&self) -> Cow<'_, FibonacciConfig> {
        if let Some(config) = &self.fib_config {
            return Cow::Borrowed(config);
        }
        let values: &[f32] = match self.tool_type {
            Tool::FibonacciTimeZones => {
                &[0.0, 1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0]
            }
            Tool::FibonacciSpeedFan => &[0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0],
            Tool::TrendBasedFibTime => &[0.0, 0.382, 0.5, 0.618, 1.0, 1.618, 2.618],
            Tool::FibonacciCircles => &[0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.618, 2.618],
            Tool::FibonacciSpeedResistanceArcs => &[0.236, 0.382, 0.5, 0.618, 0.786, 1.0],
            Tool::FibonacciExtension => {
                // Before configs were stored, extensions always reached the right pane edge.
                let config = FibonacciConfig {
                    extend_right: true,
                    ..FibonacciConfig::legacy_extension()
                };
                return Cow::Owned(config);
            }
            Tool::FibonacciWedge => {
                let mut config = FibonacciConfig::legacy();
                for level in &mut config.levels {
                    level.color = self.color;
                }
                return Cow::Owned(config);
            }
            _ => return Cow::Owned(FibonacciConfig::legacy()),
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
            ..FibonacciConfig::legacy()
        };
        if self.tool_type == Tool::FibonacciSpeedFan {
            let tokens = &DESIGN_TOKENS.semantic;
            let colors = [
                tokens.extended.bearish,
                tokens.indicators.ma,
                tokens.extended.caution,
                tokens.drawings.fib_50,
                tokens.indicators.ema,
                tokens.extended.bullish,
                tokens.extended.bullish,
            ];
            for (level, color) in config.levels.iter_mut().zip(colors) {
                level.color = color.to_srgba_unmultiplied();
            }
        }
        Cow::Owned(config)
    }

    pub(crate) fn render_fibonacci(
        &self,
        painter: &egui::Painter,
        chart_rect: Rect,
        view: &ChartView,
        selected: bool,
    ) {
        let [start, end, ..] = self.chart_points.as_slice() else {
            return;
        };
        if self.points.len() < 2 {
            return;
        }
        let config = self.fibonacci_config();
        let origin = self.chart_points.get(2).unwrap_or(start).price;
        let prices = config.level_prices(
            start.price,
            end.price,
            self.tool_type == Tool::FibonacciExtension,
            origin,
        );
        let mut levels: Vec<_> = config
            .visible_levels()
            .zip(&self.level_ys)
            .zip(prices)
            .map(|((level, &y), price)| (level, y, price))
            .filter(|(_, y, price)| y.is_finite() && price.is_finite())
            .collect();
        levels.sort_by(|a, b| a.0.value.total_cmp(&b.0.value));
        let range = self.fib_level_x_range(&config, chart_rect);
        if !range.min.is_finite()
            || !range.max.is_finite()
            || range.min >= range.max
            || range.max <= chart_rect.left()
            || range.min >= chart_rect.right()
        {
            self.render_fib_trend(painter);
            return;
        }
        if config.show_background || self.fill_color.is_some() {
            for pair in levels.windows(2) {
                let [(a, y1, _), (b, y2, _)] = [pair[0], pair[1]];
                let outer = if a.value.abs() > b.value.abs() { a } else { b };
                painter.rect_filled(
                    Rect::from_x_y_ranges(range, y1.min(y2)..=y1.max(y2)),
                    0.0,
                    self.fill_color32()
                        .unwrap_or_else(|| band_color(outer, config.background_opacity)),
                );
            }
        }
        for (level, y, price) in levels {
            if !chart_rect.y_range().contains(y) {
                paint_level_line(
                    painter,
                    [Pos2::new(range.min, y), Pos2::new(range.max, y)],
                    &config,
                    level,
                );
                continue;
            }
            let label = level_galley(painter, &config, level, Some(price), Some(view));
            let mut text = painter.layout_no_wrap(
                if config.show_text {
                    level.text.clone()
                } else {
                    String::new()
                },
                egui::FontId::proportional(typography::font_size(config.label_size)),
                level_color(level),
            );
            if text.text().is_empty() && config.show_text && selected && !self.locked {
                let hint = painter.layout_no_wrap(
                    crate::localization::text(painter.ctx(), "Add text"),
                    egui::FontId::proportional(typography::font_size(config.label_size)),
                    level_color(level),
                );
                let bounds = level_text_bounds(
                    range,
                    y,
                    hint.size(),
                    config.text_position,
                    config.text_vertical,
                    self.tool_type == Tool::FibonacciExtension,
                    painter.clip_rect(),
                );
                if painter.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pos| bounds.contains(pos))
                }) {
                    text = hint;
                }
            }
            let mut gaps = Vec::new();
            for (galley, horizontal, vertical, inside) in [
                (
                    label,
                    config.label_position.unwrap_or(if config.labels_on_left {
                        crate::drawings::LineTextHorizontalAlign::Left
                    } else {
                        crate::drawings::LineTextHorizontalAlign::Right
                    }),
                    config.label_vertical,
                    false,
                ),
                (
                    text,
                    config.text_position,
                    config.text_vertical,
                    self.tool_type == Tool::FibonacciExtension,
                ),
            ] {
                if !galley.text().is_empty() {
                    let bounds = level_text_bounds(
                        range,
                        y,
                        galley.size(),
                        horizontal,
                        vertical,
                        inside,
                        painter.clip_rect(),
                    );
                    if bounds.y_range().contains(y)
                        && bounds.right() > range.min
                        && bounds.left() < range.max
                    {
                        gaps.push(bounds.x_range().expand(2.0));
                    }
                    painter.galley(bounds.min, galley, level_color(level));
                }
            }
            gaps.sort_by(|a, b| a.min.total_cmp(&b.min));
            let mut x = range.min;
            for gap in gaps {
                if gap.min > x {
                    paint_level_line(
                        painter,
                        [Pos2::new(x, y), Pos2::new(gap.min.min(range.max), y)],
                        &config,
                        level,
                    );
                }
                x = x.max(gap.max).min(range.max);
            }
            if x < range.max {
                paint_level_line(
                    painter,
                    [Pos2::new(x, y), Pos2::new(range.max, y)],
                    &config,
                    level,
                );
            }
        }
        self.render_fib_trend(painter);
    }

    /// The custom text field of a visible retracement or extension level, including the empty-field hint.
    pub(crate) fn fib_text_bounds(
        &self,
        painter: &egui::Painter,
        pane: Rect,
        index: usize,
        text: &str,
    ) -> Option<Rect> {
        if !matches!(
            self.tool_type,
            Tool::FibonacciRetracement | Tool::FibonacciExtension
        ) || self.points.len() < 2
        {
            return None;
        }
        let config = self.fibonacci_config();
        let level = config.levels.get(index)?;
        if !config.show_text || !level.visible {
            return None;
        }
        let visible_index = config.levels[..index]
            .iter()
            .filter(|level| level.visible)
            .count();
        let y = *self.level_ys.get(visible_index)?;
        if !pane.y_range().contains(y) {
            return None;
        }
        let galley = painter.layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(typography::font_size(config.label_size)),
            level_color(level),
        );
        let range = self.fib_level_x_range(&config, pane);
        if !range.min.is_finite()
            || !range.max.is_finite()
            || range.min >= range.max
            || range.max <= pane.left()
            || range.min >= pane.right()
        {
            return None;
        }
        Some(level_text_bounds(
            range,
            y,
            galley.size(),
            config.text_position,
            config.text_vertical,
            self.tool_type == Tool::FibonacciExtension,
            pane,
        ))
    }

    fn render_fib_trend(&self, painter: &egui::Painter) {
        if !self.fibonacci_config().trend_visible {
            return;
        }
        for pair in self.points.windows(2) {
            self.styled_segment(
                painter,
                pair[0],
                pair[1],
                Stroke::new(self.stroke_width, self.color32()),
            );
        }
    }

    pub(crate) fn render_fibonacci_arc(&self, painter: &egui::Painter) {
        let &[start, end, ..] = self.points.as_slice() else {
            return;
        };
        let base_radius = start.distance(end);
        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().filter(|l| l.value >= 0.0).collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        // The first band reaches the centre.
        let mut previous = 0.0;
        for level in levels {
            let radius = base_radius * level.value;
            if !radius.is_finite() {
                continue;
            }
            let axes = egui::vec2(radius, if end.y >= start.y { radius } else { -radius });
            let angles = [
                0.0,
                if config.close_arcs {
                    std::f64::consts::TAU
                } else {
                    std::f64::consts::PI
                },
            ];
            let clip = painter
                .clip_rect()
                .expand(effective_width(&config, level) + 2.0);
            let ppp = painter.pixels_per_point();
            let points = if config.show_background {
                let (inner, outer) = sampled_band(
                    start,
                    egui::vec2(
                        previous,
                        if end.y >= start.y {
                            previous
                        } else {
                            -previous
                        },
                    ),
                    axes,
                    angles,
                    clip,
                    ppp,
                );
                paint_band(
                    painter,
                    &inner,
                    &outer,
                    band_color(level, config.background_opacity),
                );
                outer
            } else {
                ellipse_arc(start, axes, angles, clip, ppp)
            };
            previous = radius;
            paint_level_path(painter, points, &config, level);
            let (near, far) = if end.y <= start.y {
                (egui::Align2::CENTER_BOTTOM, egui::Align2::CENTER_TOP)
            } else {
                (egui::Align2::CENTER_TOP, egui::Align2::CENTER_BOTTOM)
            };
            let top = speed_arc_point(start, end, radius, std::f32::consts::FRAC_PI_2);
            paint_ratio_label(painter, &config, level, top, near);
            if config.close_arcs {
                let bottom = speed_arc_point(start, end, radius, -std::f32::consts::FRAC_PI_2);
                paint_ratio_label(painter, &config, level, bottom, far);
            }
        }
        self.render_fib_trend(painter);
    }

    /// Vertical projections from the first anchor, or from C for trend-based time.
    pub(crate) fn render_fibonacci_time_zones(&self, painter: &egui::Painter, chart_rect: Rect) {
        let &[start, end, ..] = self.points.as_slice() else {
            return;
        };
        let unit = end.x - start.x;
        if unit.abs() < 1.0 {
            return;
        }
        let origin = if self.tool_type == Tool::TrendBasedFibTime {
            self.points.get(2).unwrap_or(&start).x
        } else {
            start.x
        };
        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        if config.show_background {
            for pair in levels.windows(2) {
                let a = origin + unit * pair[0].value;
                let b = origin + unit * pair[1].value;
                if a.is_finite() && b.is_finite() {
                    painter.rect_filled(
                        Rect::from_x_y_ranges(a.min(b)..=a.max(b), chart_rect.y_range()),
                        0.0,
                        band_color(pair[1], config.background_opacity),
                    );
                }
            }
        }
        for level in levels {
            let x = origin + unit * level.value;
            if !chart_rect.x_range().contains(x) {
                continue;
            }
            paint_level_line(
                painter,
                [
                    Pos2::new(x, chart_rect.top()),
                    Pos2::new(x, chart_rect.bottom()),
                ],
                &config,
                level,
            );
            paint_ratio_label(
                painter,
                &config,
                level,
                Pos2::new(
                    x + if config.labels_on_left { -4.0 } else { 4.0 },
                    chart_rect.bottom() - 4.0,
                ),
                if config.labels_on_left {
                    egui::Align2::RIGHT_BOTTOM
                } else {
                    egui::Align2::LEFT_BOTTOM
                },
            );
        }
        self.render_fib_trend(painter);
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
        let mut levels: Vec<_> = config.visible_levels().zip(&lines).collect();
        levels.sort_by(|a, b| a.0.value.total_cmp(&b.0.value));
        if config.show_background || self.fill_color.is_some() {
            for pair in levels.windows(2) {
                let (level, &[a, b]) = pair[0];
                let (_, &[c, d]) = pair[1];
                if [a, b, c, d].iter().any(|p| !p.is_finite()) {
                    continue;
                }
                painter.add(egui::Shape::convex_polygon(
                    vec![a, b, d, c],
                    self.fill_color32()
                        .unwrap_or_else(|| band_color(level, config.background_opacity)),
                    Stroke::NONE,
                ));
            }
        }
        for (level, &line) in levels {
            paint_level_line(painter, line, &config, level);
            use crate::drawings::LineTextHorizontalAlign as H;
            let position = config.label_position.unwrap_or(if config.labels_on_left {
                H::Left
            } else {
                H::Right
            });
            let point = match position {
                H::Left => line[0],
                H::Center => line[0].lerp(line[1], 0.5),
                H::Right => line[1],
            };
            if !point.is_finite() || !chart_rect.y_range().contains(point.y) {
                continue;
            }
            let price = (view.y_to_price)(point.y);
            let galley = level_galley(
                painter,
                &config,
                level,
                price.is_finite().then_some(price),
                Some(view),
            );
            if !galley.text().is_empty() {
                let bounds = level_text_bounds(
                    Rangef::new(line[0].x, line[1].x),
                    point.y,
                    galley.size(),
                    position,
                    config.label_vertical,
                    false,
                    painter.clip_rect(),
                );
                painter.galley(bounds.min, galley, level_color(level));
            }
        }

        if !self.completed && self.points.len() == 2 {
            for &point in self.points.as_slice() {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, self.color32());
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }
        }
    }

    pub(crate) fn render_fibonacci_circles(&self, painter: &egui::Painter) {
        let &[start, end, ..] = self.points.as_slice() else {
            return;
        };
        let center = start.lerp(end, 0.5);
        let half_axes = (end - start).abs() * 0.5;
        let config = self.fibonacci_config();
        let mut levels: Vec<_> = config.visible_levels().filter(|l| l.value >= 0.0).collect();
        levels.sort_by(|a, b| a.value.total_cmp(&b.value));
        // The first band reaches the centre.
        let mut previous = egui::Vec2::ZERO;
        for level in levels {
            let radii = half_axes * level.value;
            if !radii.is_finite() {
                continue;
            }
            let clip = painter
                .clip_rect()
                .expand(effective_width(&config, level) + 2.0);
            let ppp = painter.pixels_per_point();
            let angles = [0.0, std::f64::consts::TAU];
            let mut points = if config.show_background {
                let (inner, outer) = sampled_band(center, previous, radii, angles, clip, ppp);
                paint_band(
                    painter,
                    &inner,
                    &outer,
                    band_color(level, config.background_opacity),
                );
                outer
            } else {
                ellipse_arc(center, radii, angles, clip, ppp)
            };
            previous = radii;
            if points.is_empty() {
                continue;
            }
            points.pop();
            if level
                .line_style
                .unwrap_or(config.level_style)
                .dash_pattern()
                .is_none()
            {
                painter.add(egui::Shape::closed_line(
                    points,
                    Stroke::new(effective_width(&config, level), level_color(level)),
                ));
            } else {
                points.push(points[0]);
                paint_level_path(painter, points, &config, level);
            }
            paint_ratio_label(
                painter,
                &config,
                level,
                center + egui::vec2(0.0, radii.y + 2.0),
                egui::Align2::CENTER_TOP,
            );
        }
        self.render_fib_trend(painter);
    }

    /// Price and time rays meet the edges of the anchors' box at complementary ratios.
    pub(crate) fn render_fibonacci_speed_fan(&self, painter: &egui::Painter, chart_rect: Rect) {
        let &[start, end, ..] = self.points.as_slice() else {
            return;
        };
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
                        self.fill_color32()
                            .unwrap_or_else(|| band_color(pair[1], config.background_opacity)),
                        Stroke::NONE,
                    ));
                }
            }
        }
        let d = end - start;
        for level in levels {
            paint_level_line(
                painter,
                [
                    start,
                    speed_fan_line_end(start, end, level.value, chart_rect),
                ],
                &config,
                level,
            );
            for line in speed_fan_level_lines(start, end, level.value, chart_rect) {
                paint_level_line(painter, line, &config, level);
            }
            let price_at = start + egui::vec2(d.x, d.y * (1.0 - level.value));
            let time_at = start + egui::vec2(d.x * (1.0 - level.value), d.y);
            paint_ratio_label(
                painter,
                &config,
                level,
                price_at + egui::vec2(if d.x >= 0.0 { 4.0 } else { -4.0 }, 0.0),
                if d.x >= 0.0 {
                    egui::Align2::LEFT_CENTER
                } else {
                    egui::Align2::RIGHT_CENTER
                },
            );
            paint_ratio_label(
                painter,
                &config,
                level,
                time_at + egui::vec2(0.0, if d.y >= 0.0 { 4.0 } else { -4.0 }),
                if d.y >= 0.0 {
                    egui::Align2::CENTER_TOP
                } else {
                    egui::Align2::CENTER_BOTTOM
                },
            );
        }
    }

    pub(crate) fn render_fibonacci_spiral(&self, painter: &egui::Painter, rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let edge = self.points[1];
        let config = self.fibonacci_config();
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );
        if center.distance(edge) < 1.0 {
            return;
        }
        self.styled_segment(
            painter,
            center,
            super::lines::ray_end(center, edge, rect),
            Stroke::new(self.stroke_width, color),
        );
        // Two turns inside the second anchor, down to 1/47 of its distance, and one turn outside.
        let growth = std::f64::consts::GOLDEN_RATIO.ln() / std::f64::consts::FRAC_PI_2;
        let dx = f64::from(edge.x) - f64::from(center.x);
        let dy = f64::from(edge.y) - f64::from(center.y);
        let radius = dx.hypot(dy);
        let mut points = Vec::new();
        for quarter in -8..4 {
            let from = f64::from(quarter) * std::f64::consts::FRAC_PI_2;
            let to = f64::from(quarter + 1) * std::f64::consts::FRAC_PI_2;
            let bound = radius * (growth * to).exp() * (1.0 + growth * growth);
            let span = smooth_path(
                [from, to],
                Acceleration {
                    x: bound,
                    y: bound,
                    length: bound,
                },
                painter.clip_rect().expand(self.stroke_width + 2.0),
                painter.pixels_per_point(),
                |theta| fib_spiral_point(center, edge, theta, config.spiral_counterclockwise),
            );
            points.extend(span.into_iter().skip(usize::from(quarter != -8)));
        }
        painter.add(egui::Shape::line(
            points,
            Stroke::new(self.stroke_width, color),
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
        // The first band reaches the apex.
        let mut previous = 0.0;
        let mut ends = None;
        for level in levels {
            let angles = wedge_angles(start, edge, boundary);
            let clip = painter
                .clip_rect()
                .expand(effective_width(&config, level) + 2.0);
            let ppp = painter.pixels_per_point();
            let radius = start.distance(edge);
            let points = if config.show_background {
                let (inside, outside) = sampled_band(
                    start,
                    egui::Vec2::splat(radius * previous),
                    egui::Vec2::splat(radius * level.value),
                    angles,
                    clip,
                    ppp,
                );
                paint_band(
                    &painter,
                    &inside,
                    &outside,
                    band_color(level, config.background_opacity),
                );
                outside
            } else {
                wedge_arc_points(start, edge, boundary, level.value, clip, ppp)
            };
            if points.is_empty() {
                continue;
            }
            let mid = ellipse_point(
                start,
                egui::Vec2::splat(radius * level.value),
                (angles[0] + angles[1]) / 2.0,
            );
            paint_ratio_label(&painter, &config, level, mid, egui::Align2::CENTER_BOTTOM);
            ends = Some([points[0], *points.last().unwrap()]);
            previous = level.value;
            paint_level_path(&painter, points, &config, level);
        }
        if config.trend_visible
            && let Some(ends) = ends
        {
            for end in ends {
                self.styled_segment(
                    &painter,
                    start,
                    end,
                    Stroke::new(self.stroke_width, self.color32()),
                );
            }
        }
    }

    pub(super) fn fibonacci_wedge_outline(&self, clip: Rect) -> Option<super::Outline> {
        let &[start, edge, boundary, ..] = self.points.as_slice() else {
            return None;
        };
        let config = self.fibonacci_config();
        let mut outline = super::Outline::default();
        let mut outer = 0.0_f32;
        for level in config
            .visible_levels()
            .filter(|l| l.value.is_finite() && l.value >= 0.0)
        {
            let points = wedge_arc_points(start, edge, boundary, level.value, clip, 1.0);
            // Wide arcs add their own ink area without widening the other levels' hit lines.
            let width = effective_width(&config, level);
            if width > 10.0 {
                let mut previous_normal = None;
                for pair in points.windows(2) {
                    if pair[0] == pair[1] {
                        continue;
                    }
                    let normal = (pair[1] - pair[0]).normalized().rot90() * (width / 2.0);
                    if let Some(previous) = previous_normal.replace(normal) {
                        outline.areas.push(vec![
                            pair[0] + previous,
                            pair[0] + normal,
                            pair[0] - previous,
                            pair[0] - normal,
                        ]);
                    }
                    outline.areas.push(vec![
                        pair[0] + normal,
                        pair[1] + normal,
                        pair[1] - normal,
                        pair[0] - normal,
                    ]);
                }
            }
            outline
                .lines
                .extend(points.windows(2).map(|w| [w[0], w[1]]));
            outer = outer.max(level.value);
        }
        if config.trend_visible {
            let outer = wedge_arc_points(start, edge, boundary, outer, clip, 1.0);
            outline
                .lines
                .extend([[start, *outer.first()?], [start, *outer.last()?]]);
        }
        Some(outline)
    }
}

impl Drawing {
    /// The x extent of a Fibonacci retracement's or extension's level lines:
    /// between the anchors (the second and third of an extension), or on to the
    /// edge of `chart_rect` on each side the config extends.
    pub(crate) fn fib_level_x_range(&self, config: &FibonacciConfig, chart_rect: Rect) -> Rangef {
        let (a, b) = if self.tool_type == Tool::FibonacciExtension && self.points.len() >= 3 {
            (self.points[1].x, self.points[2].x)
        } else {
            (self.points[0].x, self.points[1].x)
        };
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
    ) -> Option<(Cow<'_, FibonacciConfig>, Vec<[Pos2; 2]>)> {
        let &[start, end, ..] = self.points.as_slice() else {
            return None;
        };
        let d = end - start;
        if d.length() < 1.0 {
            return None;
        }
        let config = self.fibonacci_config();
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
/// translated by `level` times the vector from `start` to `width_point`.
fn fib_channel_level_line(start: Pos2, end: Pos2, width_point: Pos2, level: f32) -> [Pos2; 2] {
    let offset = (width_point - start) * level;
    [start + offset, end + offset]
}

fn speed_fan_level_lines(start: Pos2, end: Pos2, ratio: f32, rect: Rect) -> Vec<[Pos2; 2]> {
    let d = end - start;
    let at = start + d * (1.0 - ratio);
    let mut lines = vec![
        [Pos2::new(at.x, start.y), Pos2::new(at.x, end.y)],
        [Pos2::new(start.x, at.y), Pos2::new(end.x, at.y)],
    ];
    if ratio != 0.0 {
        let through = if ratio == 1.0 && d.y == 0.0 {
            start + egui::vec2(0.0, 1.0)
        } else {
            Pos2::new(at.x, end.y)
        };
        lines.push([start, super::lines::ray_end(start, through, rect)]);
    }
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

fn level_width(config: &FibonacciConfig) -> f32 {
    if config.level_width.is_finite() && config.level_width > 0.0 {
        config
            .level_width
            .min(crate::drawings::validation::MAX_STROKE_WIDTH)
    } else {
        2.0
    }
}

fn effective_width(config: &FibonacciConfig, level: &FibonacciLevel) -> f32 {
    level
        .width
        .filter(|width| width.is_finite() && *width > 0.0)
        .map_or_else(
            || level_width(config),
            |width| width.min(crate::drawings::validation::MAX_STROKE_WIDTH),
        )
}

/// Labels sit outside endpoints; extension text sits inside. The centre is shared.
fn level_text_bounds(
    range: Rangef,
    y: f32,
    size: egui::Vec2,
    horizontal: crate::drawings::LineTextHorizontalAlign,
    vertical: crate::drawings::LineTextVerticalAlign,
    inside: bool,
    clip: Rect,
) -> Rect {
    use crate::drawings::{LineTextHorizontalAlign as H, LineTextVerticalAlign as V};
    let (x, align) = match horizontal {
        H::Left if inside => (range.min + 4.0, egui::Align::Min),
        H::Right if inside => (range.max - 4.0, egui::Align::Max),
        H::Left => (range.min - 4.0, egui::Align::Max),
        H::Center => ((range.min + range.max) * 0.5, egui::Align::Center),
        H::Right => (range.max + 4.0, egui::Align::Min),
    };
    let (y, vertical) = match vertical {
        V::Bottom => (y - 2.0, egui::Align::Max),
        V::Middle => (y, egui::Align::Center),
        V::Top => (y + 2.0, egui::Align::Min),
    };
    let bounds = egui::Align2([align, vertical]).anchor_size(Pos2::new(x, y), size);
    let shift = (clip.min - bounds.min).max(egui::Vec2::ZERO)
        + (clip.max - bounds.max).min(egui::Vec2::ZERO);
    bounds.translate(shift)
}

fn paint_level_path(
    painter: &egui::Painter,
    points: Vec<Pos2>,
    config: &FibonacciConfig,
    level: &FibonacciLevel,
) {
    if points.iter().any(|p| !p.is_finite()) {
        return;
    }
    let stroke = Stroke::new(effective_width(config, level), level_color(level));
    if let Some((dash, gap)) = level
        .line_style
        .unwrap_or(config.level_style)
        .dash_pattern()
    {
        crate::painting::dashed_line::draw_dashed_polyline(painter, &points, stroke, dash, gap);
    } else {
        painter.add(egui::Shape::line(points, stroke));
    }
}

fn paint_ratio_label(
    painter: &egui::Painter,
    config: &FibonacciConfig,
    level: &FibonacciLevel,
    at: Pos2,
    align: egui::Align2,
) {
    if !at.is_finite() {
        return;
    }
    let galley = level_galley(painter, config, level, None, None);
    if galley.text().is_empty() {
        return;
    }
    let bounds = align.anchor_size(at, galley.size());
    painter.galley(bounds.min, galley, level_color(level));
}

fn paint_level_line(
    painter: &egui::Painter,
    points: [Pos2; 2],
    config: &FibonacciConfig,
    level: &FibonacciLevel,
) {
    if points.iter().any(|p| !p.is_finite()) {
        return;
    }
    let stroke = Stroke::new(effective_width(config, level), level_color(level));
    if let Some((dash, gap)) = level
        .line_style
        .unwrap_or(config.level_style)
        .dash_pattern()
    {
        super::utils::draw_dashed_line(painter, points[0], points[1], stroke, dash, gap);
    } else {
        painter.line_segment(points, stroke);
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
pub(super) fn paint_band(painter: &egui::Painter, inner: &[Pos2], outer: &[Pos2], color: Color32) {
    let mut mesh = egui::Mesh::default();
    add_band(&mut mesh, inner, outer, color);
    painter.add(egui::Shape::mesh(mesh));
}

/// Adds the band between two equal-length point lists to `mesh`; nothing if they do not match.
pub(super) fn add_band(mesh: &mut egui::Mesh, inner: &[Pos2], outer: &[Pos2], color: Color32) {
    if inner.len() != outer.len() || inner.iter().chain(outer).any(|p| !p.is_finite()) {
        return;
    }
    let first = mesh.vertices.len() as u32;
    mesh.reserve_vertices(inner.len() * 2);
    mesh.reserve_triangles(inner.len().saturating_sub(1) * 2);
    for (&inside, &outside) in inner.iter().zip(outer) {
        mesh.colored_vertex(inside, color);
        mesh.colored_vertex(outside, color);
    }
    for i in 1..inner.len() {
        let end = first + (i * 2) as u32;
        mesh.add_triangle(end - 2, end - 1, end + 1);
        mesh.add_triangle(end - 2, end + 1, end);
    }
}

/// Circular ratio arcs between the two trend rays, following their shorter signed sweep.
fn wedge_angles(start: Pos2, edge: Pos2, boundary: Pos2) -> [f64; 2] {
    let a = edge - start;
    let b = boundary - start;
    let angle = f64::from(a.y.atan2(a.x));
    let sweep = f64::from((a.x * b.y - a.y * b.x).atan2(a.dot(b)));
    [angle, angle + sweep]
}

fn wedge_arc_points(
    start: Pos2,
    edge: Pos2,
    boundary: Pos2,
    ratio: f32,
    clip: Rect,
    ppp: f32,
) -> Vec<Pos2> {
    ellipse_arc(
        start,
        egui::Vec2::splat(start.distance(edge) * ratio),
        wedge_angles(start, edge, boundary),
        clip,
        ppp,
    )
}

/// Both band edges share angles; each edge refines only where its own curvature reaches the view.
fn sampled_band(
    center: Pos2,
    inner: egui::Vec2,
    outer: egui::Vec2,
    angles: [f64; 2],
    clip: Rect,
    ppp: f32,
) -> (Vec<Pos2>, Vec<Pos2>) {
    let at = |angle: f64| {
        let (sin, cos) = angle.sin_cos();
        [inner, outer].map(|axes| {
            Pos2::new(
                (f64::from(center.x) + f64::from(axes.x) * cos) as f32,
                (f64::from(center.y) + f64::from(axes.y) * sin) as f32,
            )
        })
    };
    sample_curve(
        angles,
        [Acceleration::ellipse(inner), Acceleration::ellipse(outer)],
        clip,
        ppp,
        at,
    )
    .into_iter()
    .map(|(_, [a, b])| (a, b))
    .unzip()
}

/// A half-circle around the first anchor, on the second anchor's price side.
fn speed_arc_point(start: Pos2, end: Pos2, radius: f32, angle: f32) -> Pos2 {
    start + radius * speed_arc_direction(start, end, angle)
}

fn speed_arc_direction(start: Pos2, end: Pos2, angle: f32) -> egui::Vec2 {
    let side = if end.y >= start.y { 1.0 } else { -1.0 };
    egui::vec2(angle.cos(), side * angle.sin())
}

/// Where the price ray through `start` and `end.x` at `1 - ratio` of the
/// price swing leaves the chart on the side of `end`.
fn speed_fan_line_end(start: Pos2, end: Pos2, ratio: f32, chart_rect: Rect) -> Pos2 {
    let through = if ratio == 1.0 && end.x == start.x {
        start + egui::vec2(1.0, 0.0)
    } else {
        Pos2::new(end.x, start.y + (end.y - start.y) * (1.0 - ratio))
    };
    super::lines::ray_end(start, through, chart_rect)
}

/// The point of the golden spiral around `center` that passes through `edge`,
/// `theta` radians past it: the radius grows by the golden ratio every quarter turn.
fn fib_spiral_point(center: Pos2, edge: Pos2, theta: f64, counterclockwise: bool) -> Pos2 {
    if theta == 0.0 {
        return edge;
    }
    let growth = std::f64::consts::GOLDEN_RATIO.ln() / std::f64::consts::FRAC_PI_2;
    let factor = (growth * theta).exp();
    let (s, c) = if counterclockwise { -theta } else { theta }.sin_cos();
    let dx = f64::from(edge.x) - f64::from(center.x);
    let dy = f64::from(edge.y) - f64::from(center.y);
    Pos2::new(
        (f64::from(center.x) + factor * (dx * c - dy * s)) as f32,
        (f64::from(center.y) + factor * (dx * s + dy * c)) as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    /// A quarter retracement of 100 to 200 is 175; a half log retracement of 100 to
    /// 400 is 200. Extensions project from 150. Width and custom text choices do not change either geometry.
    #[test]
    fn fibonacci_level_widths_text_and_placement_are_independent() {
        use crate::drawings::ChartPoint;
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1000.0, 700.0));
        for (tool, logarithmic, reverse, end, ratio, price) in [
            (Tool::FibonacciRetracement, false, false, 200.0, 0.25, 175.0),
            (Tool::FibonacciRetracement, false, true, 200.0, 0.25, 125.0),
            (Tool::FibonacciRetracement, true, false, 400.0, 0.5, 200.0),
            (Tool::FibonacciRetracement, true, true, 400.0, 0.5, 200.0),
            (Tool::FibonacciExtension, false, false, 200.0, 0.5, 200.0),
            (Tool::FibonacciExtension, false, true, 200.0, 0.5, 100.0),
            (Tool::FibonacciExtension, true, false, 400.0, 0.5, 300.0),
            (Tool::FibonacciExtension, true, true, 400.0, 0.5, 75.0),
        ] {
            for (shift, extend) in [
                (0.0, false),
                (-1200.0, false),
                (1200.0, false),
                (-1200.0, true),
                (1200.0, true),
                (f32::INFINITY, false),
            ] {
                for (horizontal, vertical, text_horizontal, text_vertical, text_visible) in [
                    ("Left", "Bottom", "Right", "Top", true),
                    ("Center", "Middle", "Left", "Bottom", true),
                    ("Right", "Top", "Center", "Middle", true),
                    ("Right", "Top", "Center", "Middle", false),
                ] {
                    let mut drawing = Drawing::new(1, tool);
                    drawing.chart_points =
                        vec![ChartPoint::new(300.0, 100.0), ChartPoint::new(700.0, end)];
                    if tool == Tool::FibonacciExtension {
                        drawing.chart_points[1].bar_idx = 300.0;
                        drawing.chart_points.push(ChartPoint::new(700.0, 150.0));
                    }
                    drawing.color = [170, 10, 170, 255];
                    drawing.fib_config = Some(serde_json::from_value(serde_json::json!({
                    "levels": [
                        {"value": ratio,"color":[255,0,0,255],"visible":true,"show_price":true,"width":4.0,"text":"My level"},
                        {"value":1.0,"color":[0,255,0,255],"visible":true,"show_price":false}
                    ],
                    "level_width":2.0,"show_background":false,"background_opacity":51,
                    "extend_left":extend && shift < 0.0,"extend_right":extend && shift > 0.0,"reverse":reverse,"logarithmic":logarithmic,
                    "show_levels":false,"label_size":18.0,"trend_visible":false,
                    "label_position":horizontal,"label_vertical":vertical,
                    "text_position":text_horizontal,"text_vertical":text_vertical,"show_text":text_visible
                })).unwrap());
                    drawing.update_screen_coords(|x| x + shift, |price| 600.0 - price as f32);
                    let view = ChartView {
                        price_formatter: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|x| x,
                        price_to_y: &|price| 600.0 - price as f32,
                        y_to_price: &|y| 600.0 - f64::from(y),
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    };
                    let ctx = egui::Context::default();
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(rect),
                            ..Default::default()
                        },
                        |ui| {
                            drawing.render_fibonacci(
                                &ui.painter().with_clip_rect(rect),
                                rect,
                                &view,
                                false,
                            )
                        },
                    );
                    let shapes = output.shapes.clone();
                    output.drop_without_applying_deltas();
                    if shift != 0.0 {
                        assert!(
                            !shapes.iter().any(|s| matches!(
                                s.shape,
                                egui::Shape::Text(_)
                                    | egui::Shape::LineSegment { .. }
                                    | egui::Shape::Rect(_)
                            )),
                            "{tool:?} shift={shift} extend={extend}"
                        );
                        assert!(
                            drawing
                                .fib_text_bounds(&ctx.debug_painter(), rect, 0, "My level")
                                .is_none()
                        );
                        continue;
                    }
                    assert_eq!(
                        crate::drawings::services::DrawingInteraction::new().hit_test(
                            drawing.points[0].lerp(drawing.points[1], 0.1),
                            std::slice::from_ref(&drawing),
                            "1h",
                        ),
                        None,
                        "hidden construction cannot be selected",
                    );
                    let lines: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            egui::Shape::LineSegment { points, stroke } => Some((points, stroke)),
                            _ => None,
                        })
                        .collect();
                    assert!(lines.iter().any(|(points, stroke)| stroke.width == 4.0
                        && stroke.color == Color32::RED
                        && (points[0].y - (600.0 - price)).abs() < 0.001));
                    assert!(
                        lines.iter().any(
                            |(_, stroke)| stroke.width == 2.0 && stroke.color == Color32::GREEN
                        )
                    );
                    assert!(
                        !lines
                            .iter()
                            .any(|(_, stroke)| stroke.color == drawing.color32()),
                        "hidden construction"
                    );
                    let texts: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            egui::Shape::Text(text) => Some(text),
                            _ => None,
                        })
                        .collect();
                    let label = texts
                        .iter()
                        .find(|text| text.galley.text() == format!("{price:.2}"))
                        .unwrap();
                    let custom = text_visible.then(|| {
                        (
                            texts
                                .iter()
                                .find(|text| text.galley.text() == "My level")
                                .unwrap(),
                            text_horizontal,
                            text_vertical,
                            tool == Tool::FibonacciExtension,
                        )
                    });
                    for (text, horizontal, vertical, inside) in
                        std::iter::once((label, horizontal, vertical, false)).chain(custom)
                    {
                        assert_eq!(text.galley.job.sections[0].format.font_id.size, 18.0);
                        assert_eq!(text.galley.job.sections[0].format.color, Color32::RED);
                        let bounds = Rect::from_min_size(text.pos, text.galley.size());
                        match horizontal {
                            "Left" if inside => assert!((bounds.left() - 304.0).abs() < 0.001),
                            "Right" if inside => assert!((bounds.right() - 696.0).abs() < 0.001),
                            "Left" => assert!((bounds.right() - 296.0).abs() < 0.001),
                            "Center" => assert!((bounds.center().x - 500.0).abs() < 0.001),
                            _ => assert!((bounds.left() - 704.0).abs() < 0.001),
                        }
                        match vertical {
                            "Bottom" => assert!((bounds.bottom() - (598.0 - price)).abs() < 0.001),
                            "Middle" => {
                                assert!((bounds.center().y - (600.0 - price)).abs() < 0.001)
                            }
                            _ => assert!((bounds.top() - (602.0 - price)).abs() < 0.001),
                        }
                        if vertical == "Middle" && (300.0..700.0).contains(&bounds.center().x) {
                            assert!(
                                !lines
                                    .iter()
                                    .any(|(points, stroke)| stroke.color == Color32::RED
                                        && points[0].x.min(points[1].x) < bounds.center().x
                                        && points[0].x.max(points[1].x) > bounds.center().x),
                                "text interrupts its level"
                            );
                        }
                    }
                    assert_eq!(
                        texts.iter().any(|text| text.galley.text() == "My level"),
                        text_visible
                    );
                }
            }
        }
    }

    #[test]
    fn fibonacci_label_cache_tracks_text_inputs() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let painter = ctx.debug_painter();
        let mut config = FibonacciConfig::default();
        let mut level = FibonacciLevel::new(0.0, "", [255; 4]);
        for value in [0.0, -0.0, 0.236, 1.618, -0.5, 0.000001, f32::MAX, f32::NAN] {
            level.value = value;
            for percent in [true, false] {
                config.levels_as_percent = percent;
                for label in ["", "custom", "23.6%", "0%"] {
                    level.label = label.into();
                    for show_price in [true, false] {
                        level.show_price = show_price;
                        for show_prices in [None, Some(false), Some(true)] {
                            config.show_prices = show_prices;
                            for show_levels in [None, Some(false), Some(true)] {
                                config.show_levels = show_levels;
                                for price in [None, Some(123.45), Some(678.9)] {
                                    let formatted = price.map(|p| format!("{p:.2}"));
                                    let expected = config.label_text(&level, formatted.as_deref());
                                    for _ in 0..2 {
                                        assert_eq!(
                                            level_galley(&painter, &config, &level, price, None)
                                                .text(),
                                            expected,
                                            "value={value}, percent={percent}, label={label:?}, \
                                             show_price={show_price}, show_prices={show_prices:?}, \
                                             show_levels={show_levels:?}, price={price:?}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fibonacci_bands_keep_both_sampled_edges() {
        for tool in [Tool::FibonacciCircles, Tool::FibonacciSpeedResistanceArcs] {
            for (start, end, inner_ratio) in [
                (Pos2::new(100.0, 100.0), Pos2::new(150.0, 120.0), 0.25),
                (Pos2::new(100.0, 100.0), Pos2::new(150.0, 80.0), 0.25),
                (
                    Pos2::new(-999600.0, -999700.0),
                    Pos2::new(1000400.0, 1000300.0),
                    0.0,
                ),
            ] {
                let mut drawing = Drawing::new(0, tool);
                drawing.points = vec![start, end];
                drawing.fib_config = Some(FibonacciConfig {
                    levels: vec![
                        FibonacciLevel::new(inner_ratio, "", [255; 4]),
                        FibonacciLevel::new(1.0, "", [255; 4]),
                    ],
                    show_background: true,
                    ..Default::default()
                });
                let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
                let mut output = egui::Context::default().run_ui(
                    egui::RawInput {
                        screen_rect: Some(pane),
                        ..Default::default()
                    },
                    |ui| {
                        if tool == Tool::FibonacciCircles {
                            drawing.render_fibonacci_circles(ui.painter());
                        } else {
                            drawing.render_fibonacci_arc(ui.painter());
                        }
                    },
                );
                output.textures_delta.clear();
                let meshes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) => Some(mesh),
                        _ => None,
                    })
                    .collect();
                let mesh = meshes.last().unwrap();
                let pairs = mesh.vertices.as_chunks::<2>().0;
                assert!(pairs.len() >= 3);
                assert!(
                    mesh.vertices.len() < 2048,
                    "{tool:?}: oversized band has {} vertices",
                    mesh.vertices.len()
                );
                assert_eq!(mesh.indices.len(), 6 * (pairs.len() - 1));
                let center = if tool == Tool::FibonacciCircles {
                    start.lerp(end, 0.5)
                } else {
                    start
                };
                let axes = if tool == Tool::FibonacciCircles {
                    (end - start).abs() * 0.5
                } else {
                    egui::Vec2::splat(start.distance(end))
                };
                if inner_ratio > 0.0 {
                    assert_eq!(
                        meshes.len(),
                        2,
                        "{tool:?}: the first band fills to the centre"
                    );
                    assert!(
                        meshes[0]
                            .vertices
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .all(|pair| pair[0].pos.distance(center) < 1e-3)
                    );
                }
                let first = center + egui::vec2(axes.x, 0.0);
                let last = center
                    + egui::vec2(
                        if tool == Tool::FibonacciCircles {
                            axes.x
                        } else {
                            -axes.x
                        },
                        0.0,
                    );
                assert!(pairs.first().unwrap()[1].pos.distance(first) < 1e-3);
                assert!(pairs.last().unwrap()[1].pos.distance(last) < 1e-3);
                let side = if tool == Tool::FibonacciCircles || end.y >= start.y {
                    1.0
                } else {
                    -1.0
                };
                let apex = center + egui::vec2(0.0, axes.y * side);
                if pane.contains(apex) {
                    assert!(pairs.windows(2).any(|w| {
                        let (a, b) = (w[0][1].pos, w[1][1].pos);
                        let chord = b - a;
                        let t = ((apex - a).dot(chord) / chord.length_sq()).clamp(0.0, 1.0);
                        apex.distance(a + chord * t) <= 0.25
                    }));
                }
                for pair in pairs {
                    assert!(
                        pair[0]
                            .pos
                            .distance(center + (pair[1].pos - center) * inner_ratio)
                            < 1e-3
                    );
                    assert!((((pair[1].pos - center) / axes).length() - 1.0).abs() < 1e-5);
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Assigned patterns and widths survive anchor reversal and logarithmic mapping. Closing an
    /// arc fills the other half too, while hidden construction and spare levels paint nothing.
    #[test]
    fn round_controls_keep_independent_strokes_and_geometry() {
        use crate::drawings::ChartPoint;
        let rect = Rect::from_min_max(Pos2::new(-800.0, -800.0), Pos2::new(1200.0, 1200.0));
        for tool in [Tool::FibonacciCircles, Tool::FibonacciSpeedResistanceArcs] {
            for (reverse, log, closed) in [
                (false, false, false),
                (true, false, true),
                (false, true, true),
                (true, true, false),
            ] {
                let mut drawing = Drawing::new(0, tool);
                drawing.chart_points = vec![
                    ChartPoint::new(200.0, 100.0),
                    ChartPoint::new(400.0, 1000.0),
                ];
                if reverse {
                    drawing.chart_points.reverse();
                }
                drawing.update_screen_coords(
                    |b| b,
                    |p| {
                        if log {
                            (1100.0 / p).ln() as f32 * 200.0
                        } else {
                            (1100.0 - p) as f32 / 2.0
                        }
                    },
                );
                let mut saved = serde_json::to_value(drawing.fib_config.as_ref().unwrap()).unwrap();
                saved["levels"] = serde_json::json!([
                    {"value":0.25,"label":"","color":[255,0,0,255],"visible":true,"show_price":false,"width":4.0,"line_style":"Dashed"},
                    {"value":0.5,"label":"","color":[0,255,0,255],"visible":true,"show_price":false,"width":6.0,"line_style":"Solid"},
                    {"value":0.75,"label":"","color":[0,0,255,255],"visible":false,"show_price":false,"width":8.0,"line_style":"Dotted"}
                ]);
                saved["show_levels"] = true.into();
                saved["show_prices"] = false.into();
                saved["level_style"] = "Dotted".into();
                saved["trend_visible"] = false.into();
                saved["close_arcs"] = closed.into();
                saved["label_size"] = 20.0.into();
                drawing.fib_config = Some(serde_json::from_value(saved).unwrap());
                let mut output = egui::Context::default().run_ui(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ui| {
                        let painter = ui.painter().with_clip_rect(rect);
                        if tool == Tool::FibonacciCircles {
                            drawing.render_fibonacci_circles(&painter);
                        } else {
                            drawing.render_fibonacci_arc(&painter);
                        }
                    },
                );
                output.textures_delta.clear();
                let mut red = 0;
                let mut green = 0;
                let mut labels = 0;
                let start = drawing.points[0];
                let end = drawing.points[1];
                let radius = start.distance(end) * 0.5;
                let side = if end.y >= start.y { 1.0 } else { -1.0 };
                let opposite = start - egui::vec2(0.0, side * radius);
                let mut filled_opposite = false;
                for shape in &output.shapes {
                    let stroke = match &shape.shape {
                        egui::Shape::Path(path) => {
                            if path.stroke.color == egui::epaint::ColorMode::Solid(Color32::GREEN) {
                                green += 1;
                                assert_eq!(path.stroke.width, 6.0);
                            }
                            None
                        }
                        egui::Shape::LineSegment { points, stroke } => {
                            if stroke.color == Color32::RED {
                                red += 1;
                                assert_eq!(stroke.width, 4.0);
                                assert!(
                                    points[0].distance(points[1]) <= 8.01,
                                    "independent dashes"
                                );
                            }
                            Some(stroke)
                        }
                        egui::Shape::Mesh(mesh) => {
                            filled_opposite |= mesh
                                .vertices
                                .iter()
                                .any(|v| v.pos.distance(opposite) < 0.01);
                            None
                        }
                        egui::Shape::Text(text) => {
                            labels += 1;
                            assert!(
                                text.galley
                                    .job
                                    .sections
                                    .iter()
                                    .all(|s| s.format.font_id.size == 20.0)
                            );
                            None
                        }
                        _ => None,
                    };
                    if let Some(stroke) = stroke {
                        assert_ne!(stroke.color, drawing.color32(), "hidden construction line");
                        assert_ne!(stroke.color, Color32::BLUE, "hidden level");
                    }
                }
                assert!(red > 1 && green == 1, "{tool:?}: assigned strokes");
                assert_eq!(
                    labels,
                    if tool == Tool::FibonacciSpeedResistanceArcs && closed {
                        4
                    } else {
                        2
                    }
                );
                if tool == Tool::FibonacciSpeedResistanceArcs {
                    assert_eq!(filled_opposite, closed, "full-circle background");
                    assert_eq!(
                        crate::drawings::services::DrawingInteraction::hit_test_painted(
                            opposite,
                            &output.shapes
                        ),
                        closed,
                        "select the added half"
                    );
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// A 100-point radius grows to 161.8034 after a quarter turn in either direction.
    /// Curve width follows the drawing instead of the previous fixed one-point stroke.
    #[test]
    fn spiral_direction_and_width_reach_the_curve() {
        let rect = Rect::from_min_max(Pos2::new(-1000.0, -1000.0), Pos2::new(1000.0, 1000.0));
        for counterclockwise in [false, true] {
            let mut drawing = Drawing::new(0, Tool::FibonacciSpiral);
            drawing.points = vec![Pos2::ZERO, Pos2::new(100.0, 0.0)];
            drawing.stroke_width = 5.0;
            let mut saved = serde_json::to_value(FibonacciConfig::default()).unwrap();
            saved["spiral_counterclockwise"] = counterclockwise.into();
            drawing.fib_config = Some(serde_json::from_value(saved).unwrap());
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    drawing.render_fibonacci_spiral(&ui.painter().with_clip_rect(rect), rect);
                },
            );
            output.textures_delta.clear();
            let curve = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) => Some(path),
                    _ => None,
                })
                .unwrap();
            let expected = Pos2::new(
                0.0,
                if counterclockwise {
                    -161.8034
                } else {
                    161.8034
                },
            );
            assert!(
                curve.points.iter().any(|p| p.distance(expected) < 0.01),
                "quarter-turn geometry"
            );
            assert_eq!(curve.stroke.width, 5.0);
            output.drop_without_applying_deltas();
        }
    }

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
                    "levels_as_percent":false,"show_prices":false,"level_width":3.0
                })).unwrap());
                let mut output =
                    egui::Context::default().run_ui(egui::RawInput::default(), |ui| match tool {
                        Tool::FibonacciTimeZones => {
                            drawing.render_fibonacci_time_zones(ui.painter(), rect)
                        }
                        Tool::TrendBasedFibTime => {
                            drawing.render_fibonacci_time_zones(ui.painter(), rect)
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
                        assert!(
                            output.shapes.iter().any(|s| matches!(&s.shape,
                            egui::Shape::Text(t) if t.galley.text()=="0.25" && t.pos.y>370.0)),
                            "zone ratios belong at the bottom"
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
                            egui::Shape::Path(p) => {
                                p.stroke.color == egui::epaint::ColorMode::Solid(red)
                                    && p.closed
                                    && p.points.first() == Some(&Pos2::new(162.5, 150.0))
                                    && p.points.iter().all(|point| {
                                        (point.distance(Pos2::new(150.0, 150.0)) - 12.5).abs()
                                            < 1e-3
                                    })
                            }
                            _ => false,
                        }))
                    }
                    Tool::FibonacciSpeedFan => {
                        let lines = drawing.outline(rect).unwrap().lines;
                        assert!(
                            !strokes.contains(&drawing.color32()),
                            "fan has no extra construction line"
                        );
                        assert!(
                            lines.iter().all(|l| l[0] == drawing.points[0]
                                || l[0].x == l[1].x
                                || l[0].y == l[1].y),
                            "fan outline has rays and grid only"
                        );
                        let labels: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|s| match &s.shape {
                                egui::Shape::Text(t) if t.galley.text() == "0.25" => Some(t),
                                _ => None,
                            })
                            .collect();
                        assert_eq!(labels.len(), 2);
                        assert!(labels.iter().any(|t| t.pos.x > 200.0 && t.pos.y < 125.0));
                        assert!(labels.iter().any(|t| t.pos.y < 100.0 && t.pos.x < 175.0));
                        let line = |a: Pos2, b: Pos2| lines.iter().any(|l| l[0] == a && l[1] == b);
                        assert!(
                            line(Pos2::new(100.0, 200.0), Pos2::new(500.0, -100.0)),
                            "quarter price ray"
                        );
                        assert!(
                            !line(Pos2::new(125.0, 200.0), Pos2::new(125.0, 100.0)),
                            "hidden grid line"
                        );
                    }
                    Tool::FibonacciSpeedResistanceArcs | Tool::FibonacciWedge => {
                        let want = if tool == Tool::FibonacciSpeedResistanceArcs {
                            Pos2::new(135.35534, 200.0)
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
                if let Some(at) = match tool {
                    Tool::FibonacciCircles => Some(Pos2::new(150.0, 137.5)),
                    Tool::FibonacciSpeedResistanceArcs => Some(Pos2::new(135.35534, 200.0)),
                    Tool::FibonacciWedge => Some(Pos2::new(125.0, 175.0)),
                    _ => None,
                } {
                    assert!(
                        crate::drawings::services::DrawingInteraction::hit_test_painted(
                            at,
                            &output.shapes
                        ),
                        "{tool:?}: selectable painted level"
                    );
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Channel labels follow the price at their alignment point, including reversed anchors and
    /// log scales: the midpoint of 125 and 225 is 175, or sqrt(125*225) on a log scale.
    #[test]
    fn channel_labels_follow_the_local_level_height() {
        use crate::drawings::{
            ChartPoint, LineTextHorizontalAlign as H, LineTextVerticalAlign as V,
        };
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1400.0, 600.0));
        for log in [false, true] {
            for reversed in [false, true] {
                for (position, x, price) in [
                    (H::Left, 300.0, 125.0),
                    (
                        H::Center,
                        500.0,
                        if log {
                            (125.0_f64 * 225.0).sqrt()
                        } else {
                            175.0
                        },
                    ),
                    (H::Right, 700.0, 225.0),
                ] {
                    for (vertical, clip) in [
                        (V::Top, rect),
                        (V::Middle, rect),
                        (V::Bottom, rect),
                        (V::Middle, rect.translate(egui::vec2(0.0, -1000.0))),
                        (V::Middle, rect.translate(egui::vec2(0.0, 1000.0))),
                    ] {
                        let mut d = Drawing::new(0, Tool::FibonacciChannel);
                        // A half-channel translation takes 100 to 125; B is 200 on a linear
                        // scale or 180 on a log scale, giving 225 at the other endpoint.
                        let a = ChartPoint::new(300.0, 100.0);
                        let b = ChartPoint::new(700.0, if log { 180.0 } else { 200.0 });
                        let c = ChartPoint::new(300.0, if log { 156.25 } else { 150.0 });
                        d.chart_points = if reversed {
                            vec![
                                b,
                                a,
                                ChartPoint::new(700.0, if log { 281.25 } else { 250.0 }),
                            ]
                        } else {
                            vec![a, b, c]
                        };
                        let mut config = FibonacciConfig::for_tool(Tool::FibonacciChannel);
                        config.levels = vec![FibonacciLevel::new(0.5, "", [211, 41, 73, 255])];
                        config.label_position = Some(position);
                        config.label_vertical = vertical;
                        config.label_size = 20.0;
                        config.show_levels = Some(false);
                        config.show_background = false;
                        d.fib_config = Some(config);
                        let y = |p: f64| {
                            if log {
                                (600.0 - p.ln() * 100.0) as f32
                            } else {
                                500.0 - p as f32
                            }
                        };
                        let inverse = |y: f32| {
                            if log {
                                ((600.0 - f64::from(y)) / 100.0).exp()
                            } else {
                                500.0 - f64::from(y)
                            }
                        };
                        d.update_screen_coords(|x| x, y);
                        let view = ChartView {
                            price_formatter: None,
                            time_formatter: None,
                            bar_seconds: 0,
                            instrument: None,
                            bars: &[],
                            revision: 0,
                            bar_to_x: &|x| x,
                            price_to_y: &y,
                            y_to_price: &inverse,
                            time_axis: true,
                            executions: None,
                            timeframe: None,
                            finer: &[],
                        };
                        let mut output =
                            egui::Context::default().run_ui(Default::default(), |ui| {
                                d.render_fibonacci_channel(
                                    &ui.painter().with_clip_rect(clip),
                                    clip,
                                    &view,
                                );
                            });
                        output.textures_delta.clear();
                        let text = output.shapes.iter().find_map(|s| match &s.shape {
                            egui::Shape::Text(t) => Some(t),
                            _ => None,
                        });
                        if !clip.y_range().contains(y(price)) {
                            assert!(
                                text.is_none(),
                                "offscreen prices must not be pinned to the pane"
                            );
                            output.drop_without_applying_deltas();
                            continue;
                        }
                        let text = text.expect("channel price label");
                        assert!((text.galley.text().parse::<f64>().unwrap() - price).abs() < 0.01);
                        let bounds = Rect::from_min_size(text.pos, text.galley.size());
                        let near = |a: f32, b: f32| {
                            assert!(
                                (a - b).abs() < 0.01,
                                "{position:?} {vertical:?}: {a} != {b}"
                            )
                        };
                        match position {
                            H::Left => near(bounds.right(), x - 4.0),
                            H::Center => near(bounds.center().x, x),
                            H::Right => near(bounds.left(), x + 4.0),
                        }
                        match vertical {
                            V::Top => near(bounds.top(), y(price) + 2.0),
                            V::Middle => near(bounds.center().y, y(price)),
                            V::Bottom => near(bounds.bottom(), y(price) - 2.0),
                        }
                        assert_eq!(text.galley.job.sections[0].format.font_id.size, 20.0);
                        output.drop_without_applying_deltas();
                    }
                }
            }
        }
    }

    /// Each wedge arc keeps its pattern and thickness; hiding the two radial edges removes
    /// their paint and fallback hit geometry without removing arcs, labels or bands.
    #[test]
    fn wedge_level_styles_and_edges_are_independent() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(600.0, 600.0));
        for trend in [false, true] {
            for reversed in [false, true] {
                let mut d = Drawing::new(7, Tool::FibonacciWedge);
                d.points = vec![
                    Pos2::new(100.0, 300.0),
                    Pos2::new(300.0, 300.0),
                    Pos2::new(100.0, 100.0),
                ];
                if reversed {
                    d.points.swap(1, 2);
                }
                let mut json = serde_json::to_value(d.fib_config.as_ref().unwrap()).unwrap();
                json["trend_visible"] = trend.into();
                json["levels"] = serde_json::json!([
                    {"value":0.5,"label":"","color":[211,41,73,255],"visible":true,"show_price":false,"width":40.0,"line_style":"Dotted"},
                    {"value":1.0,"label":"","color":[37,157,89,255],"visible":true,"show_price":false,"width":3.0,"line_style":"Solid"},
                    {"value":0.0,"label":"","color":[61,79,199,255],"visible":true,"show_price":false,"width":40.0,"line_style":"Solid"}
                ]);
                d.fib_config = Some(serde_json::from_value(json).unwrap());
                let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
                    d.render_fibonacci_wedge(ui.painter(), rect);
                });
                output.textures_delta.clear();
                let red = Color32::from_rgb(211, 41, 73);
                let green = Color32::from_rgb(37, 157, 89);
                assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::LineSegment { stroke, .. } if stroke.color==red && stroke.width==40.0)), "dotted arc uses separate dashes");
                assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::Path(p) if p.stroke.color==egui::epaint::ColorMode::Solid(green) && p.stroke.width==3.0 && p.points.len()>2)), "solid arc stays joined");
                let edges = output
                    .shapes
                    .iter()
                    .filter(|s| {
                        matches!(&s.shape,
                    egui::Shape::LineSegment { stroke, .. } if stroke.color==d.color32())
                    })
                    .count();
                assert_eq!(edges, if trend { 2 } else { 0 });
                let outline = d.fibonacci_wedge_outline(rect).unwrap();
                assert_eq!(
                    outline
                        .lines
                        .iter()
                        .filter(|l| l[0] == d.points[0] && l[1] != d.points[0])
                        .count(),
                    if trend { 2 } else { 0 }
                );
                let interaction = crate::drawings::services::DrawingInteraction::new();
                let at_radius =
                    |radius: f32| d.points[0] + egui::vec2(1.0, -1.0).normalized() * radius;
                assert_eq!(
                    interaction.hit_test(at_radius(112.0), std::slice::from_ref(&d), "1D"),
                    Some(7)
                );
                assert_eq!(
                    interaction.hit_test(at_radius(212.0), std::slice::from_ref(&d), "1D"),
                    None,
                    "the wide inner arc must not enlarge the thin outer arc's hit area"
                );
                // Half a 200-point radius is 100; style must not change this geometry.
                assert!(outline.lines.iter().any(|points| {
                    points
                        .iter()
                        .all(|p| (p.distance(d.points[0]) - 100.0).abs() < 0.001)
                }));
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Drawing-wide choices reach the painted labels, including a channel on a log scale.
    #[test]
    fn fibonacci_label_content_and_side_follow_settings() {
        use crate::drawings::ChartPoint;
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1400.0, 400.0));
        for (tool, log, ratio, price, in_view) in [
            (Tool::FibonacciRetracement, false, 0.25, 175.0, true),
            (Tool::FibonacciExtension, false, 0.25, 175.0, true),
            (Tool::FibonacciChannel, false, 0.5, 225.0, true),
            // Half the log translation from A=100 to C=150: B*sqrt(150/100).
            (Tool::FibonacciChannel, true, 0.5, 244.9489742784, true),
            // The price pane spans 10 to 1000; levels beyond it have no label.
            (Tool::FibonacciRetracement, false, 2.0, 0.0, false),
            (Tool::FibonacciRetracement, false, -10.0, 1200.0, false),
            (Tool::FibonacciExtension, false, -2.0, -50.0, false),
            (Tool::FibonacciExtension, false, 10.0, 1150.0, false),
            (Tool::FibonacciRetracement, false, 1.9, 10.0, true),
            (Tool::FibonacciRetracement, false, -8.0, 1000.0, true),
            (Tool::FibonacciExtension, false, -1.4, 10.0, true),
            (Tool::FibonacciExtension, false, 8.5, 1000.0, true),
        ] {
            for (show_prices, show_levels, percent, left, custom, extend) in [
                (true, true, false, true, false, false),
                (false, true, true, false, false, false),
                (true, false, true, false, false, false),
                (false, false, false, true, false, false),
                (true, true, false, true, true, false),
                // Saved extensions run to the right edge with right-hand labels.
                (true, true, false, false, false, true),
                (true, true, false, true, false, true),
            ] {
                let mut drawing = Drawing::new(0, tool);
                drawing.chart_points = vec![
                    ChartPoint::new(240.0, 100.0),
                    ChartPoint::new(560.0, 200.0),
                    ChartPoint::new(880.0, 150.0),
                ];
                drawing.fib_config=Some(serde_json::from_value(serde_json::json!({
                    "levels":[{"value":ratio,"label":"","color":[211,41,73,255],"visible":true,"show_price":false}],
                    "show_background":false,"background_opacity":37,"extend_left":extend && left,"extend_right":extend && !left,"reverse":false,
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
                let formatter =
                    crate::scales::CustomPriceFormatter::new(|p| format!("host:{p:.5}"));
                let mut instrument = crate::model::Instrument::new("TEST");
                instrument.price_decimals = Some(5);
                let view = ChartView {
                    price_formatter: custom
                        .then_some(&formatter as &dyn crate::scales::PriceFormatter),
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: Some(&instrument),
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b,
                    price_to_y: &y,
                    y_to_price: &|y| map.y_to_price(y),
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                };
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    drawing.render_original(&ui.painter().with_clip_rect(rect), rect, &view)
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
                // Channel endpoints are x=560 and x=880, rising 100 or doubling per 320 px.
                let price = if tool == Tool::FibonacciChannel && extend {
                    let distance = if left { -560.0 } else { 520.0 } / 320.0;
                    if log {
                        price * 2.0_f64.powf(distance)
                    } else {
                        price + 100.0 * distance
                    }
                } else {
                    price
                };
                let in_view =
                    in_view && (tool != Tool::FibonacciChannel || (10.0..=1000.0).contains(&price));
                let formatted = if custom {
                    format!("host:{price:.5}")
                } else {
                    format!("{price:.5}")
                };
                let expected = match (show_prices, show_levels) {
                    (true, true) => format!("{value} ({formatted})"),
                    (false, true) => value.clone(),
                    (true, false) => formatted,
                    (false, false) => String::new(),
                };
                if expected.is_empty() || !in_view {
                    assert!(texts.is_empty(), "{tool:?}");
                } else {
                    assert_eq!(texts.len(), 1, "{tool:?}");
                    if tool == Tool::FibonacciChannel && show_prices {
                        // Channel prices are recovered from f32 screen coordinates.
                        // Check the independently calculated price within that roundoff,
                        // while requiring all five instrument decimals and the host prefix.
                        let text = texts[0].galley.text();
                        let formatted = if show_levels {
                            text.strip_prefix(&format!("{value} ("))
                                .unwrap()
                                .strip_suffix(')')
                                .unwrap()
                        } else {
                            text
                        };
                        let number = if custom {
                            formatted.strip_prefix("host:").unwrap()
                        } else {
                            formatted
                        };
                        assert_eq!(number.split('.').nth(1).unwrap().len(), 5);
                        // An extended channel is labelled at the pane edge, at another price.
                        assert!(extend || (number.parse::<f64>().unwrap() - price).abs() < 1e-4);
                    } else {
                        assert_eq!(
                            texts[0].galley.text(),
                            expected,
                            "{tool:?}, log={log}, left={left}"
                        );
                    }
                    let edge = if tool == Tool::FibonacciRetracement {
                        if left { 240.0 } else { 560.0 }
                    } else if left {
                        560.0
                    } else {
                        880.0
                    };
                    let label = Rect::from_min_size(texts[0].pos, texts[0].galley.size());
                    assert!(
                        if extend {
                            rect.expand(0.01).contains_rect(label)
                        } else if left {
                            label.max.x < edge
                        } else {
                            label.min.x > edge
                        },
                        "{tool:?}, extend={extend}: label outside the level endpoints, in the pane"
                    );
                    if !extend && price > 10.0 && price < 1000.0 {
                        assert!(
                            (label.center().y - y(price)).abs() < 1.0,
                            "{tool:?}: label centred on its level"
                        );
                    }
                    assert!(
                        !output
                            .shapes
                            .iter()
                            .any(|s| matches!(s.shape, egui::Shape::Rect(_))),
                        "labels have no background boxes"
                    );
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn fibonacci_bands_styles_and_extension_range_follow_settings() {
        use crate::drawings::ChartPoint;
        use crate::drawings::services::DrawingInteraction;
        use crate::model::LineStyle;
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(900.0, 600.0));
        for tool in [
            Tool::FibonacciRetracement,
            Tool::FibonacciExtension,
            Tool::FibonacciChannel,
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(200.0, 100.0),
                ChartPoint::new(400.0, 300.0),
                ChartPoint::new(600.0, 200.0),
            ];
            for style in [LineStyle::Solid, LineStyle::Dashed] {
                drawing.fib_config=Some(serde_json::from_value(serde_json::json!({
                    "levels":[
                        {"value":1.0,"color":[0,0,255,255],"visible":true,"show_price":true},
                        {"value":0.0,"color":[255,0,0,255],"visible":true,"show_price":true},
                        {"value":0.5,"color":[0,255,0,255],"visible":true,"show_price":true}],
                    "level_width":3.0,"level_style":style,"show_background":true,"background_opacity":51,
                    "extend_left":false,"extend_right":false,"reverse":false
                })).unwrap());
                drawing.color = [171, 51, 131, 255];
                drawing.stroke_width = 4.0;
                drawing.line_style = LineStyle::Solid;
                drawing.update_screen_coords(|x| x, |p| 600.0 - p as f32);
                let view = ChartView {
                    price_formatter: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|x| x,
                    price_to_y: &|p| 600.0 - p as f32,
                    y_to_price: &|y| 600.0 - f64::from(y),
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                };
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    drawing.render_original(ui.painter(), rect, &view)
                });
                output.textures_delta.clear();
                let fills: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r) if r.fill.a() > 0 => {
                            Some((r.fill, vec![r.rect.min, r.rect.max]))
                        }
                        egui::Shape::Path(p) if p.fill.a() > 0 => Some((p.fill, p.points.clone())),
                        _ => None,
                    })
                    .collect();
                assert_eq!(fills.len(), 2, "{tool:?}: adjacent bands only");
                let colors = if tool == Tool::FibonacciChannel {
                    [[255, 0, 0, 51], [0, 255, 0, 51]]
                } else {
                    [[0, 255, 0, 51], [0, 0, 255, 51]]
                };
                for ((color, _), expected) in fills.iter().zip(colors) {
                    assert_eq!(
                        *color,
                        Color32::from_rgba_unmultiplied(
                            expected[0],
                            expected[1],
                            expected[2],
                            expected[3]
                        )
                    );
                }
                if tool != Tool::FibonacciChannel {
                    let xs = if tool == Tool::FibonacciExtension {
                        [400.0, 600.0]
                    } else {
                        [200.0, 400.0]
                    };
                    for (_, points) in &fills {
                        assert_eq!([points[0].x, points[1].x], xs);
                    }
                    assert!(output.shapes.iter().any(|s| match &s.shape {
                        egui::Shape::LineSegment { points, stroke } =>
                            points == &drawing.points[..2]
                                && stroke.width == 4.0
                                && stroke.color == drawing.color32(),
                        egui::Shape::Path(p) => p.points == drawing.points && p.stroke.width == 4.0,
                        _ => false,
                    }));
                    if tool == Tool::FibonacciExtension {
                        let interaction = DrawingInteraction::new();
                        assert_eq!(
                            interaction.hit_test(
                                Pos2::new(500.0, 300.0),
                                std::slice::from_ref(&drawing),
                                "1D"
                            ),
                            Some(7)
                        );
                        assert_eq!(
                            interaction.hit_test(
                                Pos2::new(300.0, 300.0),
                                std::slice::from_ref(&drawing),
                                "1D"
                            ),
                            None
                        );
                    }
                }
                if style == LineStyle::Solid {
                    assert!(
                        output
                            .shapes
                            .iter()
                            .filter_map(|s| match &s.shape {
                                egui::Shape::LineSegment { stroke, .. }
                                    if stroke.color != drawing.color32() =>
                                    Some(stroke.width),
                                _ => None,
                            })
                            .all(|width| width == 3.0)
                    );
                } else {
                    assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::LineSegment{points,stroke}
                        if stroke.color==Color32::GREEN && stroke.width==3.0 && points[0].distance(points[1])<=8.01)));
                    assert!(!output.shapes.iter().any(
                        |s| matches!(&s.shape,egui::Shape::LineSegment{points,stroke}
                        if stroke.color==Color32::GREEN && points[0].distance(points[1])>8.01)
                    ));
                }
                output.drop_without_applying_deltas();
            }
        }
        assert!(!FibonacciConfig::extension_default().extend_right);
        let mut legacy = Drawing::new(0, Tool::FibonacciExtension);
        legacy.fib_config = None;
        assert!(
            legacy.fibonacci_config().extend_right,
            "extensions saved without a config keep their implicit extension"
        );
    }

    /// Each tool against coordinates worked by hand (screen y grows downward).
    #[test]
    fn fibonacci_tools_follow_their_construction() {
        let near = |a: Pos2, b: Pos2| a.distance(b) < 1e-2;
        let mut wrong = Vec::new();

        // Translate the whole base by c*(C-A): (50,-40) or (50,-80).
        for (start, end, width_point, level, want) in [
            (
                (0.0, 100.0),
                (100.0, 100.0),
                (50.0, 60.0),
                1.0,
                [(50.0, 60.0), (150.0, 60.0)],
            ),
            (
                (0.0, 100.0),
                (100.0, 100.0),
                (50.0, 60.0),
                0.5,
                [(25.0, 80.0), (125.0, 80.0)],
            ),
            (
                (0.0, 100.0),
                (100.0, 0.0),
                (50.0, 20.0),
                1.0,
                [(50.0, 20.0), (150.0, -80.0)],
            ),
            (
                (0.0, 100.0),
                (100.0, 0.0),
                (50.0, 20.0),
                0.5,
                [(25.0, 60.0), (125.0, -40.0)],
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

        // Arcs open towards the second anchor from the first. Radius 50 at a quarter turn.
        for (start, end, want) in [
            ((0.0, 100.0), (100.0, 0.0), (0.0, 50.0)),
            ((0.0, 0.0), (100.0, 100.0), (0.0, 50.0)),
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
        // edge: slope 0.5 * (1-ratio), so at x = 0, y = 100 * ratio.
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        for (ratio, want) in [(0.0, (0.0, 0.0)), (0.5, (0.0, 50.0)), (1.0, (0.0, 100.0))] {
            let got =
                speed_fan_line_end(Pos2::new(200.0, 100.0), Pos2::new(100.0, 50.0), ratio, rect);
            if !near(got, Pos2::from(want)) {
                wrong.push(format!("fan ratio {ratio}: got {got:?}, want {want:?}"));
            }
        }

        for (ratio, want) in [
            (0.0, Pos2::new(100.0, 300.0)),
            (1.0, Pos2::new(400.0, 100.0)),
        ] {
            assert_eq!(
                speed_fan_line_end(
                    Pos2::new(100.0, 100.0),
                    Pos2::new(100.0, 200.0),
                    ratio,
                    rect
                ),
                want
            );
        }
        assert!(
            speed_fan_level_lines(Pos2::new(100.0, 100.0), Pos2::new(200.0, 100.0), 1.0, rect)
                .contains(&[Pos2::new(100.0, 100.0), Pos2::new(100.0, 300.0)])
        );

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
        output.drop_without_applying_deltas();
        assert!(filled);

        // Golden spiral through the second anchor, its radius growing by phi
        // (1.618034) every quarter turn.
        for (edge, theta, want) in [
            ((100.0, 0.0), 0.0, (100.0, 0.0)),
            ((100.0, 0.0), FRAC_PI_2, (0.0, 161.8034)),
            ((100.0, 0.0), -FRAC_PI_2, (0.0, -61.8034)),
            ((0.0, 100.0), 0.0, (0.0, 100.0)),
        ] {
            let got = fib_spiral_point(Pos2::ZERO, Pos2::from(edge), f64::from(theta), false);
            if !near(got, Pos2::from(want)) {
                wrong.push(format!(
                    "spiral edge {edge:?} theta {theta}: got {got:?}, want {want:?}"
                ));
            }
        }

        // Both fan axes: the 50% time ray goes through (50,100), the
        // 50% price ray through (100,50), with a grid and no arcs.
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
        for want in [Pos2::new(50.0, 0.0), Pos2::new(0.0, 50.0)] {
            assert!(
                outline.lines.iter().flatten().any(|p| near(*p, want)),
                "missing wedge point {want:?}"
            );
        }
        let mid = Pos2::new(35.35534, 35.35534);
        assert!(outline.lines.iter().any(|[a, b]| {
            let chord = *b - *a;
            let t = ((mid - *a).dot(chord) / chord.length_sq()).clamp(0.0, 1.0);
            mid.distance(*a + chord * t) <= 0.25
        }));

        assert!(
            wedge
                .fibonacci_config()
                .visible_levels()
                .all(|l| l.value > 0.0),
            "new wedges have no radius-zero arc"
        );
        let mut spiral = Drawing::new(1, Tool::FibonacciSpiral);
        spiral.points = vec![Pos2::new(100.0, 200.0), Pos2::new(200.0, 150.0)];
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            spiral.render_fibonacci_spiral(ui.painter(), rect)
        });
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape,egui::Shape::LineSegment{points,..}
            if points==&[Pos2::new(100.0,200.0),Pos2::new(400.0,50.0)])),
            "spiral construction ray"
        );
        output.drop_without_applying_deltas();

        let mut extension = Drawing::new(0, crate::drawings::DrawingToolType::FibonacciExtension);
        extension.chart_points = vec![
            crate::drawings::ChartPoint::new(0.0, 100.0),
            crate::drawings::ChartPoint::new(10.0, 150.0),
            crate::drawings::ChartPoint::new(30.0, 130.0),
        ];
        for (start, end, origin, logarithmic, reverse, ratio, want) in [
            (100.0, 150.0, 130.0, false, false, 0.0, 130.0),
            (100.0, 150.0, 130.0, false, false, 0.618, 160.9),
            (100.0, 150.0, 130.0, false, false, 1.0, 180.0),
            // C * (B/A)^0.5 = 20 * sqrt(100) = 200.
            (10.0, 1000.0, 20.0, true, false, 0.5, 200.0),
            (10.0, 1000.0, 0.0, true, false, 0.5, 495.0),
            (10.0, 1000.0, -20.0, true, false, 0.5, 475.0),
            (0.0, 1000.0, 20.0, true, false, 0.5, 520.0),
            (10.0, 0.0, 20.0, true, false, 0.5, 15.0),
            (100.0, 150.0, 130.0, false, true, 0.618, 99.1),
            (10.0, 1000.0, 20.0, true, true, 0.5, 2.0),
        ] {
            extension.chart_points[0].price = start;
            extension.chart_points[1].price = end;
            extension.chart_points[2].price = origin;
            extension.fib_config = Some(FibonacciConfig {
                logarithmic,
                reverse,
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
            extension.render_fibonacci_time_zones(ui.painter(), rect)
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
                retracement.render_fibonacci(
                    ui.painter(),
                    rect,
                    &ChartView {
                        price_formatter: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|b| b,
                        price_to_y: &|p| p as f32,
                        y_to_price: &f64::from,
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    },
                    false,
                )
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

        // Right-to-left channel translated by c*(-100,0), extended on the left only.
        let mut channel = Drawing::new(0, crate::drawings::DrawingToolType::FibonacciChannel);
        channel.points = vec![
            Pos2::new(300.0, 100.0),
            Pos2::new(100.0, 200.0),
            Pos2::new(200.0, 100.0),
        ];
        channel.fib_config = Some(FibonacciConfig {
            extend_left: true,
            ..FibonacciConfig::legacy()
        });
        let (_, lines) = channel.fib_channel_level_lines(rect).unwrap();
        for (level, [left, right]) in channel
            .fib_config
            .as_ref()
            .unwrap()
            .visible_levels()
            .zip(lines)
        {
            if (left.x - rect.min.x).abs() > 1e-3
                || (right.x - (300.0 - 100.0 * level.value)).abs() > 1e-3
            {
                wrong.push(format!("channel level from {left:?} to {right:?}"));
            }
        }

        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
