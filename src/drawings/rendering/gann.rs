//! Gann tool rendering implementations
//!
//! Includes: Gann fan, square, box, and fixed angle tools.

use super::ChartView;
use crate::drawings::domain::{
    Drawing, DrawingToolType, GANN_MAX_ARC, GANN_SQUARE_UNITS, GannConfig, GannFanDirection,
    GannLevel,
};
use crate::styles::typography;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::f32::consts::FRAC_PI_2;
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
    ((value * 1e6).round() / 1e6 + 0.0).to_string()
}

/// A fan ratio as price by time units: 2 is "2×1", 0.5 "1×2", one third "1×3".
fn ratio_label(ratio: f32) -> String {
    let (price, time) = if ratio >= 1.0 {
        (ratio, 1.0)
    } else {
        (1.0, 1.0 / ratio)
    };
    format!(
        "{}×{}",
        trim(format!("{price:.2}")),
        trim(format!("{time:.2}"))
    )
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

/// The Gann fan's lines from `start` to the edge of `rect` on `end`'s side, one per valid ratio.
pub(super) fn gann_fan_lines(
    start: Pos2,
    end: Pos2,
    rect: Rect,
    ratios: impl IntoIterator<Item = f32>,
) -> Vec<[Pos2; 2]> {
    let x = if end.x < start.x {
        rect.min.x
    } else {
        rect.max.x
    };
    ratios
        .into_iter()
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .map(|ratio| [start, Pos2::new(x, gann_fan_line_y(start, end, ratio, x))])
        .collect()
}

/// The corner of a Gann square opposite its first anchor: the second anchor, or for a fixed
/// square five times the first unit that the second anchor marks.
pub(super) fn gann_square_corner(tool: DrawingToolType, first: Pos2, second: Pos2) -> Pos2 {
    if tool == DrawingToolType::GannFixed {
        first + (second - first) * GANN_SQUARE_UNITS
    } else {
        second
    }
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

/// The arc of `radius` units around `start` inside the square, `segments + 1` points from the
/// edge at the first anchor's price to the edge at its time, cut where it leaves the square;
/// `None` for an invalid radius or an arc wholly outside.
fn gann_square_arc(start: Pos2, corner: Pos2, radius: f32, segments: usize) -> Option<Vec<Pos2>> {
    if !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let ratio = radius / GANN_SQUARE_UNITS;
    // Both coordinates of the arc stay within the side: cos and sin at most 1 / ratio.
    let (from, to) = if ratio <= 1.0 {
        (0.0, FRAC_PI_2)
    } else {
        ((1.0 / ratio).acos(), (1.0 / ratio).asin())
    };
    (from < to).then(|| {
        let (dx, dy) = (corner.x - start.x, corner.y - start.y);
        (0..=segments)
            .map(|i| {
                let angle = from + (to - from) * i as f32 / segments as f32;
                start + egui::vec2(dx * ratio * angle.cos(), dy * ratio * angle.sin())
            })
            .collect()
    })
}

/// One edge of the bands between the arcs of a square: the arc of `radius` units around `start`,
/// at each of the angles' sine and cosine, every point cut at the square's edge.
fn gann_square_ring_edge(
    start: Pos2,
    corner: Pos2,
    radius: f32,
    angles: &[(f32, f32)],
) -> impl Iterator<Item = Pos2> + '_ {
    let (dx, dy) = (corner.x - start.x, corner.y - start.y);
    angles.iter().map(move |&(s, c)| {
        // The square is convex and star-shaped from `start`, so the band along each ray ends
        // at the edge. The far corner lies at half the sweep in units, whatever the square's
        // proportions on screen, so an even number of segments samples it.
        let r = radius.min(GANN_SQUARE_UNITS / c.max(s)) / GANN_SQUARE_UNITS;
        start + egui::vec2(dx * r * c, dy * r * s)
    })
}

/// The sine and cosine of `segments + 1` angles across a quarter circle.
fn gann_square_ring_angles(segments: usize) -> Vec<(f32, f32)> {
    (0..=segments)
        .map(|i| (FRAC_PI_2 * i as f32 / segments as f32).sin_cos())
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

    /// Render Gann Fan: its visible lines from the first anchor to the chart's edge, the bands
    /// between neighbouring lines and each line's ratio at the edge.
    pub(crate) fn render_gann_fan(&self, painter: &egui::Painter, chart_rect: Rect) {
        let &[start, end, ..] = self.points.as_slice() else {
            return;
        };
        let edge = if end.x < start.x {
            chart_rect.min.x
        } else {
            chart_rect.max.x
        };
        let painter = &painter.with_clip_rect(chart_rect);
        let config = self.gann_config();
        let lines: Vec<(GannLevel, Pos2)> = config
            .fan_ratios
            .iter()
            .filter(|l| l.visible && l.value.is_finite() && l.value > 0.0)
            .filter_map(|&level| {
                let y = gann_fan_line_y(start, end, level.value, edge);
                y.is_finite().then_some((level, Pos2::new(edge, y)))
            })
            .collect();

        if config.show_background && config.background_opacity > 0 {
            let mut sorted = lines.clone();
            sorted.sort_by(|a, b| a.0.value.total_cmp(&b.0.value));
            for pair in sorted.windows(2) {
                painter.add(egui::Shape::convex_polygon(
                    vec![start, pair[0].1, pair[1].1],
                    band_color(&pair[1].0, config.background_opacity),
                    Stroke::NONE,
                ));
            }
        }

        let mut labels = Vec::<Rect>::new();
        for (level, fan_end) in &lines {
            let color = level_color(level);
            painter.line_segment([start, *fan_end], Stroke::new(self.stroke_width, color));

            // Draw label at chart edge with background
            if fan_end.y >= chart_rect.min.y && fan_end.y <= chart_rect.max.y {
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
                        fan_end.y - gann_label_height / 2.0,
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
                    ratio_label(level.value),
                    egui::FontId::proportional(typography::XS),
                    color,
                );
            }
        }
    }

    /// Render a Gann square or fixed square: the rings between its arcs, its price and time
    /// levels, fan lines and arcs, over five units per side, and its range and ratio.
    pub(crate) fn render_gann_square(&self, painter: &egui::Painter, view: &ChartView) {
        let &[start, second, ..] = self.points.as_slice() else {
            return;
        };
        let corner = gann_square_corner(self.tool_type, start, second);
        let rect = Rect::from_two_pos(start, corner);
        let config = self.gann_config();
        let at = |u: f32, v: f32| {
            Pos2::new(
                start.x + (corner.x - start.x) * u / GANN_SQUARE_UNITS,
                start.y + (corner.y - start.y) * v / GANN_SQUARE_UNITS,
            )
        };
        let stroke = |level: &GannLevel| Stroke::new(self.stroke_width, level_color(level));
        let side =
            (corner.x - start.x).abs().max((corner.y - start.y).abs()) * painter.pixels_per_point();
        // A quarter of the circle's sides, even so that the far corner is a sample of the rings.
        let segments = |radius: f32| {
            super::style::circle_sides(radius / GANN_SQUARE_UNITS * side).div_ceil(8) * 2
        };

        let mut arcs = visible_levels(&config.arc_ratios, 0.0..=GANN_MAX_ARC);
        arcs.retain(|arc| arc.value > 0.0);
        if config.show_arcs
            && config.show_background
            && config.background_opacity > 0
            && let Some(outermost) = arcs.last()
        {
            // All rings in one mesh, each edge shared by the two rings it bounds, as a square
            // repaints them every frame.
            let angles = gann_square_ring_angles(segments(outermost.value));
            let points = arcs.len() * angles.len();
            let mut edges = Vec::with_capacity(points);
            for arc in &arcs {
                edges.extend(gann_square_ring_edge(start, corner, arc.value, &angles));
            }
            let mut mesh = egui::Mesh::default();
            mesh.reserve_vertices(points * 2);
            mesh.reserve_triangles(points * 2);
            let mut rings = edges.chunks_exact(angles.len());
            let mut inner = rings.next().unwrap_or_default();
            for (outer, arc) in rings.zip(&arcs[1..]) {
                super::fibonacci::add_band(
                    &mut mesh,
                    inner,
                    outer,
                    band_color(arc, config.background_opacity),
                );
                inner = outer;
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        if config.show_grid {
            let drawn = |l: &&GannLevel| l.visible && (0.0..=GANN_SQUARE_UNITS).contains(&l.value);
            for level in config.price_levels.iter().filter(drawn) {
                painter.hline(rect.x_range(), at(0.0, level.value).y, stroke(level));
            }
            for level in config.time_levels.iter().filter(drawn) {
                painter.vline(at(level.value, 0.0).x, rect.y_range(), stroke(level));
            }
        }
        if config.show_fan {
            for (level, segment) in gann_square_fan_lines(start, corner, config) {
                painter.line_segment(segment, stroke(&level));
            }
        }
        if config.show_arcs {
            for arc in &arcs {
                if let Some(points) = gann_square_arc(start, corner, arc.value, segments(arc.value))
                {
                    painter.add(egui::Shape::line(points, stroke(arc)));
                }
            }
        }

        let [a, b, ..] = self.chart_points.as_slice() else {
            return;
        };
        let (far_price, units) = if self.tool_type == DrawingToolType::GannFixed {
            ((view.y_to_price)(corner.y), f64::from(GANN_SQUARE_UNITS))
        } else {
            (b.price, 1.0)
        };
        let range = (far_price - a.price).abs();
        let bars = f64::from((b.bar_idx - a.bar_idx).abs()) * units;
        if range.is_finite() && bars.is_finite() {
            let decimals = crate::scales::price_decimals(a.price);
            let mut label = format!("{range:.decimals$}\n{} bars", bars.round() as i64);
            if bars > 0.0 {
                let ratio_decimals = decimals + 2;
                label.push_str(&format!("\n{:.ratio_decimals$}/bar", range / bars));
            }
            let chart = painter.ctx().active_theme().chart;
            super::measurements::boxed_label(
                painter,
                egui::Align2::LEFT_TOP,
                Pos2::new(rect.max.x + DESIGN_TOKENS.rounding.lg, rect.min.y),
                label,
                chart.axis_text,
                chart.bg_axis,
            );
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
    fn gann_labels_read_as_values_and_ratios() {
        for (value, want) in [
            (0.0, "0"),
            (0.25, "0.25"),
            (0.382, "0.382"),
            (1.0, "1"),
            (5.0, "5"),
            (5f32.sqrt(), "2.236068"),
        ] {
            assert_eq!(level_label(value), want);
        }
        for (ratio, want) in [
            (8.0, "8×1"),
            (2.0, "2×1"),
            (1.0, "1×1"),
            (0.5, "1×2"),
            (1.0 / 3.0, "1×3"),
            (0.125, "1×8"),
            (1.5, "1.5×1"),
        ] {
            assert_eq!(ratio_label(ratio), want);
        }
    }

    /// Anchors (0, 100) and (10, 90) put the line of ratio r at y = 100 - 100 r on the edge x = 100.
    /// Every line is drawn once at the drawing's width, the hidden 1x2 line and the second anchor
    /// get no segment, and each band takes the colour of its steeper line.
    #[test]
    fn gann_fan_draws_visible_lines_at_one_width_with_bands() {
        const RED: [u8; 4] = [242, 54, 69, 255];
        const CYAN: [u8; 4] = [0, 188, 212, 255];
        const TEAL: [u8; 4] = [8, 153, 129, 255];
        const GREEN: [u8; 4] = [76, 175, 80, 255];
        let start = Pos2::new(0.0, 100.0);
        let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 200.0));
        let shapes = |drawing: &Drawing| {
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                drawing.render_gann_fan(ui.painter(), pane)
            });
            let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
            output.drop_without_applying_deltas();
            shapes
        };
        let segments = |shapes: &[egui::Shape]| {
            shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::LineSegment { points, stroke } => Some((*points, *stroke)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let mut drawing = Drawing::new(1, DrawingToolType::GannFan);
        drawing.stroke_width = 3.0;
        drawing.points = vec![start, Pos2::new(10.0, 90.0)];
        drawing.gann = Some(GannConfig {
            fan_ratios: vec![
                GannLevel::new(2.0, RED),
                GannLevel::new(1.0, CYAN),
                GannLevel {
                    visible: false,
                    ..GannLevel::new(0.5, TEAL)
                },
                GannLevel::new(0.25, GREEN),
            ],
            ..GannConfig::default()
        });
        let color = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let painted = shapes(&drawing);
        assert_eq!(
            segments(&painted),
            [
                (
                    [start, Pos2::new(100.0, -100.0)],
                    Stroke::new(3.0, color(RED))
                ),
                (
                    [start, Pos2::new(100.0, 0.0)],
                    Stroke::new(3.0, color(CYAN))
                ),
                (
                    [start, Pos2::new(100.0, 75.0)],
                    Stroke::new(3.0, color(GREEN))
                ),
            ]
        );
        let bands: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Path(path) => Some((path.points.clone(), path.fill)),
                _ => None,
            })
            .collect();
        let band = |[r, g, b, _]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, 51);
        assert_eq!(
            bands,
            [
                (
                    vec![start, Pos2::new(100.0, 75.0), Pos2::new(100.0, 0.0)],
                    band(CYAN)
                ),
                (
                    vec![start, Pos2::new(100.0, 0.0), Pos2::new(100.0, -100.0)],
                    band(RED)
                ),
            ]
        );

        drawing.gann = None;
        assert_eq!(segments(&shapes(&drawing)).len(), 9);
    }

    /// The second anchor is a square's corner, and five times the first unit from the first anchor
    /// is a fixed square's, at every zoom. Both figures below cover 20 price units over 10 bars.
    #[test]
    fn gann_squares_keep_their_corner_at_every_zoom_and_label_their_scale() {
        use crate::drawings::ChartPoint;
        let cases = [
            (
                DrawingToolType::GannSquare,
                [(120.0, 100.0), (240.0, 50.0), (60.0, 200.0)],
            ),
            (
                DrawingToolType::GannFixed,
                [(520.0, -300.0), (1040.0, -150.0), (260.0, -600.0)],
            ),
        ];
        for (tool, corners) in cases {
            let mut drawing = Drawing::new(0, tool);
            drawing.chart_points = vec![ChartPoint::new(2.0, 20.0), ChartPoint::new(12.0, 10.0)];
            for ((xs, ys), corner) in [(10.0, 10.0), (20.0, 5.0), (5.0, 20.0)]
                .into_iter()
                .zip(corners)
            {
                drawing.update_screen_coords(|b| b * xs, |p| p as f32 * ys);
                let outline = drawing.outline(Rect::EVERYTHING).unwrap();
                let want = Rect::from_two_pos(drawing.points[0], Pos2::new(corner.0, corner.1));
                assert_eq!(
                    Rect::from_points(&outline.areas[0]),
                    want,
                    "{tool:?} at {xs}, {ys}"
                );
            }
        }

        for (tool, second) in [
            (DrawingToolType::GannSquare, ChartPoint::new(10.0, 120.0)),
            (DrawingToolType::GannFixed, ChartPoint::new(2.0, 104.0)),
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), second];
            drawing.update_screen_coords(|b| b * 10.0, |p| 300.0 - p as f32);
            let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                drawing.render(
                    ui.painter(),
                    pane,
                    &ChartView {
                        price_formatter: None,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|b| b * 10.0,
                        price_to_y: &|p| 300.0 - p as f32,
                        y_to_price: &|y| 300.0 - f64::from(y),
                        time_axis: true,
                        executions: None,
                    },
                )
            });
            let labelled = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == "20.00\n10 bars\n2.0000/bar")
            });
            output.drop_without_applying_deltas();
            assert!(labelled, "{tool:?}");
        }
    }

    #[test]
    fn gann_square_fan_and_arcs_follow_their_settings() {
        const GREY: [u8; 4] = [128, 128, 128, 255];
        // Square from (0, 0) to (100, 100): a fan line of ratio r leaves a corner at r times
        // the diagonal's slope and ends on the opposite side, at (100, 100 r) or (100 / r, 100).
        let config = |fan_direction| GannConfig {
            fan_direction,
            fan_ratios: vec![
                GannLevel::new(0.5, GREY),
                GannLevel::new(2.0, GREY),
                GannLevel {
                    visible: false,
                    ..GannLevel::new(1.0, GREY)
                },
                GannLevel::new(0.0, GREY),
                GannLevel::new(f32::NAN, GREY),
                GannLevel::new(-1.0, GREY),
            ],
            ..GannConfig::default()
        };
        let (start, corner) = (Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0));
        let ends = |config: &GannConfig| {
            gann_square_fan_lines(start, corner, config)
                .into_iter()
                .map(|(_, line)| line)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ends(&config(GannFanDirection::Forward)),
            [
                [start, Pos2::new(100.0, 50.0)],
                [start, Pos2::new(50.0, 100.0)]
            ]
        );
        assert_eq!(
            ends(&config(GannFanDirection::Reverse)),
            [
                [corner, Pos2::new(0.0, 50.0)],
                [corner, Pos2::new(50.0, 0.0)]
            ]
        );
        assert_eq!(ends(&config(GannFanDirection::Both)).len(), 4);

        // One unit is 20 px. The arc of 1 unit is a quarter circle through (14.142, 14.142); that
        // of the square root of 26 leaves the square one unit along each far edge; 7.5 lies
        // wholly outside, since the far corner is 5 * sqrt(2) = 7.07 units away.
        let near = |a: Pos2, b: (f32, f32)| a.distance(Pos2::new(b.0, b.1)) < 1e-2;
        let arc = |radius| gann_square_arc(start, corner, radius, 16);
        let unit = arc(1.0).unwrap();
        assert_eq!(unit.len(), 17);
        assert!(near(unit[0], (20.0, 0.0)) && near(unit[16], (0.0, 20.0)));
        assert!(near(unit[8], (14.142, 14.142)));
        let cut = arc(26f32.sqrt()).unwrap();
        assert!(near(cut[0], (100.0, 20.0)) && near(cut[16], (20.0, 100.0)));
        for radius in [7.5, 0.0, f32::NAN] {
            assert_eq!(arc(radius), None, "{radius}");
        }

        // Radius 5 at 45 degrees is 70.711 on each axis, the square root of 26 is 72.111, and
        // both end on the square's edges at its two sides.
        let angles = gann_square_ring_angles(16);
        let inner: Vec<Pos2> = gann_square_ring_edge(start, corner, 5.0, &angles).collect();
        let outer: Vec<Pos2> =
            gann_square_ring_edge(start, corner, 26f32.sqrt(), &angles).collect();
        assert_eq!((inner.len(), outer.len()), (17, 17));
        let square = Rect::from_two_pos(start, corner).expand(1e-3);
        assert!(inner.iter().chain(&outer).all(|p| square.contains(*p)));
        assert!(near(inner[8], (70.711, 70.711)) && near(outer[8], (72.111, 72.111)));
        assert!(near(inner[0], (100.0, 0.0)) && near(outer[0], (100.0, 0.0)));
        assert!(near(inner[16], (0.0, 100.0)) && near(outer[16], (0.0, 100.0)));
    }
}
