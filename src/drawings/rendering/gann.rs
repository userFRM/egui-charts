//! Gann tool rendering implementations
//!
//! Includes: Gann fan, square, box, and fixed angle tools.

use crate::drawings::domain::{Drawing, DrawingToolType, GannConfig, GannFanDirection, GannLevel};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};
use std::sync::LazyLock;

fn level_color(level: &GannLevel) -> Color32 {
    let [r, g, b, a] = level.color;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn band_color(level: &GannLevel, opacity: u8) -> Color32 {
    let [r, g, b, _] = level.color;
    Color32::from_rgba_unmultiplied(r, g, b, opacity)
}

/// The visible levels within `range`, by value. A NaN value is in no range.
fn visible_levels(levels: &[GannLevel], range: std::ops::RangeInclusive<f32>) -> Vec<GannLevel> {
    let mut visible: Vec<GannLevel> = levels
        .iter()
        .filter(|l| l.visible && range.contains(&l.value))
        .copied()
        .collect();
    visible.sort_by(|a, b| a.value.total_cmp(&b.value));
    visible
}

/// A level value with at most six decimals and no trailing zeros: 0, 0.25, 0.382, 2.236068.
fn level_label(value: f32) -> String {
    trim(format!("{:.6}", value + 0.0))
}

fn trim(text: String) -> String {
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

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

/// The visible fan lines of a Gann square whose first anchor is `start` and opposite corner
/// `corner`, each with its level: from `start` towards the opposite corner and from the opposite
/// corner back, as the direction selects, up to the square's edge. Invalid ratios are skipped.
fn gann_square_fan_lines(
    start: Pos2,
    corner: Pos2,
    config: &GannConfig,
) -> Vec<(GannLevel, [Pos2; 2])> {
    let (dx, dy) = (corner.x - start.x, corner.y - start.y);
    let mut lines = Vec::new();
    for &level in config
        .fan_ratios
        .iter()
        .filter(|l| l.visible && l.value.is_finite() && l.value > 0.0)
    {
        let ratio = level.value;
        // Along the first anchor's side: the line rises `ratio` per unit of time, so it
        // reaches the far side of the square at `ratio` of its height or, if steeper, the top.
        let (u, v) = if ratio <= 1.0 {
            (1.0, ratio)
        } else {
            (1.0 / ratio, 1.0)
        };
        let offset = Pos2::new(dx * u, dy * v).to_vec2();
        if config.fan_direction != GannFanDirection::Reverse {
            lines.push((level, [start, start + offset]));
        }
        if config.fan_direction != GannFanDirection::Forward {
            lines.push((level, [corner, corner - offset]));
        }
    }
    lines
}

/// The arcs of a Gann square around its first anchor `start`, one per valid
/// ratio of the square's side, cut to the part inside the square.
fn gann_square_arcs(start: Pos2, corner: Pos2, ratios: &[GannLevel]) -> Vec<Vec<Pos2>> {
    let (dx, dy) = (corner.x - start.x, corner.y - start.y);
    ratios
        .iter()
        .filter(|l| l.visible && l.value.is_finite() && l.value > 0.0)
        .filter_map(|&GannLevel { value: ratio, .. }| {
            // Both coordinates of the arc stay within the side: cos and sin at most 1 / ratio.
            let (from, to) = if ratio <= 1.0 {
                (0.0, std::f32::consts::FRAC_PI_2)
            } else {
                ((1.0 / ratio).acos(), (1.0 / ratio).asin())
            };
            (from < to).then(|| {
                (0..=64)
                    .map(|i| {
                        let angle = from + (to - from) * i as f32 / 64.0;
                        start
                            + Pos2::new(dx * ratio * angle.cos(), dy * ratio * angle.sin())
                                .to_vec2()
                    })
                    .collect()
            })
        })
        .collect()
}

/// Rays of a Gann box from each corner to the level points on the two edges away from it: the
/// price levels on the far side and the time levels on the far top or bottom. A point on the
/// corner's own edges is skipped, and the opposite corner is reached once.
pub(super) fn gann_box_angle_lines(
    p1: Pos2,
    p2: Pos2,
    prices: &[f32],
    times: &[f32],
) -> Vec<[Pos2; 2]> {
    let at = |fx: f32, fy: f32| Pos2::new(p1.x + (p2.x - p1.x) * fx, p1.y + (p2.y - p1.y) * fy);
    let mut lines = Vec::new();
    // Level values are compared rather than screen coordinates, whose rounding differs.
    for (fx, fy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let corner = at(fx, fy);
        for &f in prices.iter().filter(|&&f| f != fy) {
            lines.push([corner, at(1.0 - fx, f)]);
        }
        for &t in times.iter().filter(|&&t| t != fx && t != 1.0 - fx) {
            lines.push([corner, at(t, 1.0 - fy)]);
        }
    }
    lines
}

impl Drawing {
    /// The stored configuration, or the tool's defaults without building them each frame.
    pub(super) fn gann_config(&self) -> &GannConfig {
        static BOX: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannBox));
        static SQUARE: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannSquare));
        static FAN: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannFan));
        self.gann.as_ref().unwrap_or_else(|| match self.tool_type {
            DrawingToolType::GannSquare | DrawingToolType::GannFixed => &SQUARE,
            DrawingToolType::GannFan => &FAN,
            _ => &BOX,
        })
    }

    fn draw_gann_grid(&self, painter: &egui::Painter, start: Pos2, end: Pos2) {
        let config = self.gann_config();
        let rect = Rect::from_two_pos(start, end);
        for (vertical, levels) in [(true, &config.time_levels), (false, &config.price_levels)] {
            for level in levels.iter().filter(|l| l.visible) {
                let ratio = level.value;
                if !(ratio.is_finite() && ratio > 0.0 && ratio < 1.0) {
                    continue;
                }
                let stroke = Stroke::new(DESIGN_TOKENS.stroke.extra_thin, level_color(level));
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
        painter.line_segment([start, end], Stroke::new(self.stroke_width, color));
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

        // Fill background
        let fill_color = self.fill_color32().unwrap_or(Color32::TRANSPARENT);
        painter.rect_filled(square_rect, 0.0, fill_color);

        // Draw main square border
        painter.rect_stroke(
            square_rect,
            0.0,
            Stroke::new(self.stroke_width, color),
            StrokeKind::Inside,
        );

        let config = self.gann_config();
        let corner = gann_square_corner(start, end);
        if config.show_grid {
            self.draw_gann_grid(painter, start, corner);
        }

        if config.show_fan {
            let line_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200);
            for (level, segment) in gann_square_fan_lines(start, corner, config) {
                let width = if level.value == 1.0 {
                    stroke::MEDIUM
                } else {
                    stroke::HAIRLINE
                };
                painter.line_segment(segment, Stroke::new(width, line_color));
            }
        }

        if config.show_arcs {
            let arc_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 100);
            for arc in gann_square_arcs(start, corner, &config.arc_ratios) {
                painter.add(egui::Shape::line(
                    arc,
                    Stroke::new(stroke::HAIRLINE, arc_color),
                ));
            }
        }
    }

    /// Render Gann Box: price and time levels counted from the first anchor, the bands between
    /// them, their value labels and, when switched on, the rays from the corners.
    pub(crate) fn render_gann_box(&self, painter: &egui::Painter) {
        let &[p1, p2, ..] = self.points.as_slice() else {
            return;
        };
        let rect = Rect::from_two_pos(p1, p2);
        let config = self.gann_config();
        let x = |t: f32| p1.x + (p2.x - p1.x) * t;
        let y = |f: f32| p1.y + (p2.y - p1.y) * f;
        let prices = visible_levels(&config.price_levels, 0.0..=1.0);
        let times = visible_levels(&config.time_levels, 0.0..=1.0);

        if config.show_background && config.background_opacity > 0 {
            for pair in prices.windows(2) {
                painter.rect_filled(
                    Rect::from_two_pos(
                        Pos2::new(rect.min.x, y(pair[0].value)),
                        Pos2::new(rect.max.x, y(pair[1].value)),
                    ),
                    0.0,
                    band_color(&pair[1], config.background_opacity),
                );
            }
            for pair in times.windows(2) {
                painter.rect_filled(
                    Rect::from_two_pos(
                        Pos2::new(x(pair[0].value), rect.min.y),
                        Pos2::new(x(pair[1].value), rect.max.y),
                    ),
                    0.0,
                    band_color(&pair[1], config.background_opacity),
                );
            }
        }
        for level in &prices {
            let stroke = Stroke::new(self.stroke_width, level_color(level));
            painter.hline(rect.x_range(), y(level.value), stroke);
        }
        for level in &times {
            let stroke = Stroke::new(self.stroke_width, level_color(level));
            painter.vline(x(level.value), rect.y_range(), stroke);
        }
        if config.show_angles {
            let values = |levels: &[GannLevel]| levels.iter().map(|l| l.value).collect::<Vec<_>>();
            let stroke = Stroke::new(self.stroke_width, self.color32());
            for segment in gann_box_angle_lines(p1, p2, &values(&prices), &values(&times)) {
                painter.line_segment(segment, stroke);
            }
        }

        let gap = DESIGN_TOKENS.spacing.xs;
        let price_labels = prices.iter().map(|level| {
            let pos = Pos2::new(rect.min.x - gap, y(level.value));
            (level, pos, egui::Align2::RIGHT_CENTER)
        });
        let time_labels = times.iter().map(|level| {
            let pos = Pos2::new(x(level.value), rect.max.y + gap);
            (level, pos, egui::Align2::CENTER_TOP)
        });
        let mut placed = Vec::<Rect>::new();
        for (level, pos, align) in price_labels.chain(time_labels) {
            let color = level_color(level);
            let galley = painter.layout_no_wrap(
                level_label(level.value),
                egui::FontId::proportional(typography::XS),
                color,
            );
            let label = align.anchor_size(pos, galley.size());
            if placed.iter().any(|r| r.intersects(label.expand(2.0))) {
                continue;
            }
            placed.push(label);
            painter.galley(label.min, galley, color);
        }
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
        let fill_color = self.fill_color32().unwrap_or(Color32::TRANSPARENT);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box started at its top-right corner (100, 0) and dragged to (0, 80), so x(t) = 100 - 100 t
    /// and y(f) = 80 f: each corner reaches the visible price levels on its far side and the time
    /// level 0.25 on its far top or bottom. Times 0 and 1 are on a corner's own edges or at the far
    /// corner, which the price level 1 or 0 already reaches.
    #[test]
    fn gann_box_angle_lines_run_from_each_corner_to_the_far_level_points() {
        let (p1, p2) = (Pos2::new(100.0, 0.0), Pos2::new(0.0, 80.0));
        let want = [
            ((100.0, 0.0), (0.0, 40.0)),
            ((100.0, 0.0), (0.0, 80.0)),
            ((100.0, 0.0), (75.0, 80.0)),
            ((0.0, 0.0), (100.0, 40.0)),
            ((0.0, 0.0), (100.0, 80.0)),
            ((0.0, 0.0), (75.0, 80.0)),
            ((100.0, 80.0), (0.0, 0.0)),
            ((100.0, 80.0), (0.0, 40.0)),
            ((100.0, 80.0), (75.0, 0.0)),
            ((0.0, 80.0), (100.0, 0.0)),
            ((0.0, 80.0), (100.0, 40.0)),
            ((0.0, 80.0), (75.0, 0.0)),
        ]
        .map(|(a, b): ((f32, f32), (f32, f32))| [Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)]);
        let got = gann_box_angle_lines(p1, p2, &[0.0, 0.5, 1.0], &[0.0, 0.25, 1.0]);
        assert!(
            got.len() == want.len()
                && got
                    .iter()
                    .zip(&want)
                    .all(|(g, w)| g[0].distance(w[0]) < 1e-3 && g[1].distance(w[1]) < 1e-3),
            "got {got:?}, want {want:?}"
        );
    }

    fn box_shapes(drawing: &Drawing) -> Vec<egui::Shape> {
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            drawing.render_gann_box(ui.painter())
        });
        let shapes = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    /// A box from (100, 0) to (0, 80) with price levels 0, 0.5, a hidden 0.75 and 1, and the time
    /// level 0.25: lines at y = 0, 40 and 80 and at x = 75, the bands above and below y = 40 in the
    /// colour of their larger level, and value labels at the side and below.
    #[test]
    fn gann_box_draws_visible_levels_bands_and_value_labels() {
        const GREY: [u8; 4] = [128, 128, 128, 255];
        const ORANGE: [u8; 4] = [255, 152, 0, 255];
        const BLUE: [u8; 4] = [41, 98, 255, 255];
        const CYAN: [u8; 4] = [0, 188, 212, 255];
        let color = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let band = |[r, g, b, _]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, 51);
        let mut drawing = Drawing::new(1, DrawingToolType::GannBox);
        drawing.stroke_width = 2.0;
        drawing.points = vec![Pos2::new(100.0, 0.0), Pos2::new(0.0, 80.0)];
        drawing.gann = Some(GannConfig {
            price_levels: vec![
                GannLevel::new(0.0, GREY),
                GannLevel::new(0.5, ORANGE),
                GannLevel {
                    visible: false,
                    ..GannLevel::new(0.75, BLUE)
                },
                GannLevel::new(1.0, GREY),
            ],
            time_levels: vec![GannLevel::new(0.25, CYAN)],
            ..GannConfig::default()
        });

        let segment = |shape: &egui::Shape| match shape {
            egui::Shape::LineSegment { points, stroke } => Some((*points, *stroke)),
            _ => None,
        };
        let shapes = box_shapes(&drawing);
        let lines: Vec<_> = shapes.iter().filter_map(segment).collect();
        let want = [
            ([(0.0, 0.0), (100.0, 0.0)], GREY),
            ([(0.0, 40.0), (100.0, 40.0)], ORANGE),
            ([(0.0, 80.0), (100.0, 80.0)], GREY),
            ([(75.0, 0.0), (75.0, 80.0)], CYAN),
        ]
        .map(|([a, b], c): ([(f32, f32); 2], _)| {
            (
                [Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)],
                Stroke::new(2.0, color(c)),
            )
        });
        assert_eq!(lines, want);

        let fills: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Rect(r) if r.fill.a() > 0 => Some((r.rect, r.fill)),
                _ => None,
            })
            .collect();
        let rect = |y0, y1| Rect::from_min_max(Pos2::new(0.0, y0), Pos2::new(100.0, y1));
        assert_eq!(
            fills,
            [
                (rect(0.0, 40.0), band(ORANGE)),
                (rect(40.0, 80.0), band(GREY))
            ]
        );

        let label = |text: &str| {
            shapes
                .iter()
                .find_map(|shape| match shape {
                    egui::Shape::Text(t) if t.galley.text() == text => {
                        Some((t.visual_bounding_rect(), t.fallback_color))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no label {text}"))
        };
        for (text, c) in [("0", GREY), ("0.5", ORANGE), ("1", GREY)] {
            let (bounds, text_color) = label(text);
            assert!(bounds.right() <= 0.0, "{text} at {bounds:?}");
            assert_eq!(text_color, color(c), "{text}");
        }
        let (bounds, text_color) = label("0.25");
        assert!(bounds.top() >= 80.0, "0.25 at {bounds:?}");
        assert_eq!(text_color, color(CYAN));
        assert!(
            !shapes
                .iter()
                .any(|shape| matches!(shape, egui::Shape::Text(t) if t.galley.text() == "0.75"))
        );

        drawing.gann.as_mut().unwrap().show_angles = true;
        let shapes = box_shapes(&drawing);
        let rays: Vec<_> = shapes
            .iter()
            .filter_map(segment)
            .filter(|(p, _)| p[0].x != p[1].x && p[0].y != p[1].y)
            .collect();
        assert_eq!(rays.len(), 12);
        assert!(
            rays.iter()
                .all(|(_, stroke)| *stroke == Stroke::new(2.0, drawing.color32()))
        );

        let saved: GannConfig =
            serde_json::from_value(serde_json::json!({"price_levels":[0.3],"time_levels":[0.7]}))
                .unwrap();
        assert_eq!(saved.price_levels, [GannLevel::new(0.3, GREY)]);
        assert_eq!(saved.time_levels, [GannLevel::new(0.7, GREY)]);
        assert_eq!(saved.fan_ratios, GannConfig::default().fan_ratios);
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
    }

    #[test]
    fn gann_square_fan_and_arcs_follow_their_settings() {
        // Square from (0, 0) to (100, 100): a fan line of ratio r leaves a corner at r times
        // the diagonal's slope and ends on the opposite side, at (100, 100 r) or (100 / r, 100).
        let config = |fan_direction, fan_ratios: &[f32]| GannConfig {
            fan_direction,
            fan_ratios: fan_ratios
                .iter()
                .map(|&ratio| GannLevel::new(ratio, [128, 128, 128, 255]))
                .collect(),
            ..GannConfig::default()
        };
        let (start, corner) = (Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0));
        let ends = |config: &GannConfig| {
            gann_square_fan_lines(start, corner, config)
                .into_iter()
                .map(|(_, line)| line)
                .collect::<Vec<_>>()
        };
        let ratios = [0.5, 2.0, 0.0, f32::NAN, -1.0];
        assert_eq!(
            ends(&config(GannFanDirection::Forward, &ratios)),
            [
                [start, Pos2::new(100.0, 50.0)],
                [start, Pos2::new(50.0, 100.0)]
            ]
        );
        assert_eq!(
            ends(&config(GannFanDirection::Reverse, &ratios)),
            [
                [corner, Pos2::new(0.0, 50.0)],
                [corner, Pos2::new(50.0, 0.0)]
            ]
        );
        assert_eq!(ends(&config(GannFanDirection::Both, &ratios)).len(), 4);

        // The arc of ratio 1 is a quarter circle through (70.7, 70.7); that of 1.2 is cut where
        // it leaves the square, at (100, 66.3) and (66.3, 100); one beyond 1.42 is outside it.
        let arcs = gann_square_arcs(
            start,
            corner,
            &[1.0, 1.2, 1.5, 0.0, f32::NAN].map(|r| GannLevel::new(r, [128, 128, 128, 255])),
        );
        assert_eq!(arcs.len(), 2);
        let near = |a: Pos2, b: Pos2| a.distance(b) < 0.1;
        assert!(
            near(arcs[0][0], Pos2::new(100.0, 0.0))
                && near(*arcs[0].last().unwrap(), Pos2::new(0.0, 100.0))
        );
        assert!(arcs[0].iter().any(|&p| near(p, Pos2::new(70.71, 70.71))));
        assert!(
            near(arcs[1][0], Pos2::new(100.0, 66.33))
                && near(*arcs[1].last().unwrap(), Pos2::new(66.33, 100.0))
        );
    }
}
