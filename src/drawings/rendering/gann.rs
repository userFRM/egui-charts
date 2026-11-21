//! Gann tool rendering implementations
//!
//! Includes: Gann fan, square, box, and fixed angle tools.

use super::ChartView;
use crate::drawings::domain::{
    Drawing, DrawingToolType, GANN_MAX_ARC, GANN_SQUARE_UNITS, GannConfig, GannFanDirection,
    GannLevel,
};
use crate::localization as l10n;
use crate::scales::group_thousands;
use crate::styles::typography;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::f32::consts::FRAC_PI_2;
use std::sync::LazyLock;

/// The line and label colour of `level`, or the configuration's one colour while it is set.
fn level_color(config: &GannConfig, level: &GannLevel) -> Color32 {
    let [r, g, b, a] = config.one_color.unwrap_or(level.color);
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The band colour of `level` at `opacity`.
fn band_color(config: &GannConfig, level: &GannLevel, opacity: u8) -> Color32 {
    let [r, g, b, _] = config.one_color.unwrap_or(level.color);
    Color32::from_rgba_unmultiplied(r, g, b, opacity)
}

/// Text and fill of a label drawn on a box: the axis colours, or with one colour that colour
/// under white or black text, whichever reads on it.
fn boxed_label_colors(config: &GannConfig, ctx: &egui::Context) -> (Color32, Color32) {
    match config.one_color {
        Some([r, g, b, a]) => {
            let fill = Color32::from_rgba_unmultiplied(r, g, b, a);
            (super::style::label_text_color(fill), fill)
        }
        None => {
            let chart = ctx.active_theme().chart;
            (chart.axis_text, chart.bg_axis)
        }
    }
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
pub(crate) fn ratio_label(ratio: f32) -> String {
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

/// The arc of `radius` units around `start` inside the square, sampled from the
/// edge at the first anchor's price to the edge at its time, cut where it leaves the square;
/// `None` for an invalid radius or an arc wholly outside.
fn gann_square_arc(
    start: Pos2,
    corner: Pos2,
    radius: f32,
    clip: Rect,
    ppp: f32,
) -> Option<Vec<Pos2>> {
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
        super::style::ellipse_arc(
            start,
            (corner - start) * ratio,
            [f64::from(from), f64::from(to)],
            clip,
            ppp,
        )
    })
}

/// One edge of the bands between the arcs of a square: the arc of `radius` units around `start`,
/// at each of the angles' sine and cosine, every point cut at the square's edge.
fn gann_square_ring_edge(
    start: Pos2,
    corner: Pos2,
    radius: f32,
    angles: &[(f64, f64)],
) -> impl Iterator<Item = Pos2> + '_ {
    let dx = f64::from(corner.x) - f64::from(start.x);
    let dy = f64::from(corner.y) - f64::from(start.y);
    angles.iter().map(move |&(s, c)| {
        // Split angles at the square's corners and circle intersections before sampling.
        let units = f64::from(GANN_SQUARE_UNITS);
        let r = f64::from(radius).min(units / c.max(s)) / units;
        Pos2::new(
            (f64::from(start.x) + dx * r * c) as f32,
            (f64::from(start.y) + dy * r * s) as f32,
        )
    })
}

/// Rays of a Gann box from each corner to the level points on the two edges away from it: the
/// price levels on the far side and the time levels on the far top or bottom. A point on the
/// corner's own edges is skipped, and the opposite corner is reached once, by its price level
/// or, when that is hidden, by its time level.
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
        let far_price = prices.contains(&(1.0 - fy));
        for &t in times
            .iter()
            .filter(|&&t| t != fx && !(t == 1.0 - fx && far_price))
        {
            lines.push([corner, at(t, 1.0 - fy)]);
        }
    }
    lines
}

impl Drawing {
    /// The corner of a Gann square opposite its first anchor `first`: the second anchor `second`,
    /// or for a fixed square five of the first units that the second anchor marks, in bars and in
    /// price, at the screen y of that price from [`Drawing::update_screen_coords`].
    pub(super) fn gann_square_corner(&self, first: Pos2, second: Pos2) -> Pos2 {
        if self.tool_type != DrawingToolType::GannFixed {
            return second;
        }
        let corner = first + (second - first) * GANN_SQUARE_UNITS;
        // Five units of price are not five times the first unit's height on a logarithmic scale.
        match self.level_ys[..] {
            [y] => Pos2::new(corner.x, y),
            _ => corner,
        }
    }

    /// The stroke of a Gann level: its own width or the drawing's, in its colour.
    fn gann_stroke(&self, config: &GannConfig, level: &GannLevel) -> Stroke {
        Stroke::new(
            level.width.unwrap_or(self.stroke_width),
            level_color(config, level),
        )
    }

    /// The stored configuration, or the tool's defaults without building them each frame.
    pub(crate) fn gann_config(&self) -> &GannConfig {
        static BOX: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannBox));
        static SQUARE: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannSquare));
        static FIXED: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannFixed));
        static FAN: LazyLock<GannConfig> =
            LazyLock::new(|| GannConfig::for_tool(DrawingToolType::GannFan));
        self.gann.as_ref().unwrap_or_else(|| match self.tool_type {
            DrawingToolType::GannSquare => &SQUARE,
            DrawingToolType::GannFixed => &FIXED,
            DrawingToolType::GannFan => &FAN,
            _ => &BOX,
        })
    }

    /// Extra room beyond the ordinary paint margin for a square's larger label.
    pub(super) fn gann_label_room(&self) -> f32 {
        if !matches!(
            self.tool_type,
            DrawingToolType::GannSquare | DrawingToolType::GannFixed
        ) {
            return 0.0;
        }
        let config = self.gann_config();
        if !config.show_labels {
            return 0.0;
        }
        super::PAINT_MARGIN
            * (typography::font_size(config.label_size) / typography::XS - 1.0).max(0.0)
    }

    /// Render Gann Fan: its visible lines from the first anchor to the chart's edge, the bands
    /// between neighbouring lines and, with labels on, each line's ratio at the edge.
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
                    band_color(config, &pair[1].0, config.background_opacity),
                    Stroke::NONE,
                ));
            }
        }

        let (text, fill) = boxed_label_colors(config, painter.ctx());
        let mut labels = Vec::<Rect>::new();
        for (level, fan_end) in &lines {
            let stroke = self.gann_stroke(config, level);
            match level.line_style.unwrap_or(self.line_style).dash_pattern() {
                Some((dash, gap)) => {
                    if let Some(clip) = crate::painting::dashed_line::finite_clip(painter) {
                        painter.extend(crate::painting::dashed_line::dashed_polyline_shapes(
                            &[start, *fan_end],
                            stroke,
                            dash,
                            gap,
                            clip,
                        ));
                    }
                }
                None => {
                    painter.line_segment([start, *fan_end], stroke);
                }
            }

            // Draw label at chart edge with background
            if config.show_labels && fan_end.y >= chart_rect.min.y && fan_end.y <= chart_rect.max.y
            {
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
                painter.rect_filled(label_rect, DESIGN_TOKENS.rounding.sm, fill);
                painter.text(
                    label_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    ratio_label(level.value),
                    egui::FontId::proportional(typography::XS),
                    text,
                );
            }
        }
    }

    /// Render a Gann square or fixed square: the rings between its arcs, its price and time
    /// levels, fan lines and arcs, over five units per side counted from the first anchor, or
    /// from the opposite corner when reversed, and its optional range and ratio label.
    pub(crate) fn render_gann_square(&self, painter: &egui::Painter, view: &ChartView) {
        let &[start, second, ..] = self.points.as_slice() else {
            return;
        };
        let corner = self.gann_square_corner(start, second);
        let config = self.gann_config();
        // Reverse counts the grid, arcs and fan lines from the corner opposite the first anchor.
        let (start, corner) = if config.reverse {
            (corner, start)
        } else {
            (start, corner)
        };
        let rect = Rect::from_two_pos(start, corner);
        let at = |u: f32, v: f32| {
            Pos2::new(
                start.x + (corner.x - start.x) * u / GANN_SQUARE_UNITS,
                start.y + (corner.y - start.y) * v / GANN_SQUARE_UNITS,
            )
        };
        let stroke = |level: &GannLevel| self.gann_stroke(config, level);
        let mut arcs = visible_levels(&config.arc_ratios, 0.0..=GANN_MAX_ARC);
        arcs.retain(|arc| arc.value > 0.0);
        // Arcs are sampled up to this margin past the pane, so the widest still reaches its edge.
        let widest = arcs
            .iter()
            .filter_map(|arc| arc.width)
            .fold(self.stroke_width, f32::max);
        let clip = painter.clip_rect().expand(widest + 2.0);
        let ppp = painter.pixels_per_point();
        if config.show_arcs
            && config.show_background
            && config.background_opacity > 0
            && !arcs.is_empty()
        {
            // All rings in one mesh, each edge shared by the two rings it bounds, as a square
            // repaints them every frame.
            let mut cuts = vec![
                0.0,
                std::f64::consts::FRAC_PI_4,
                std::f64::consts::FRAC_PI_2,
            ];
            for arc in &arcs {
                let ratio = f64::from(arc.value / GANN_SQUARE_UNITS);
                if ratio > 1.0 && ratio < std::f64::consts::SQRT_2 {
                    cuts.extend([(1.0 / ratio).acos(), (1.0 / ratio).asin()]);
                }
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup();
            let mut parameters = cuts.clone();
            // Splitting a convex arc at its square intersections only refines its chords.
            for arc in &arcs {
                let axes = (corner - start) * (arc.value / GANN_SQUARE_UNITS);
                parameters.extend(
                    super::style::sample_curve(
                        [0.0, std::f64::consts::FRAC_PI_2],
                        [super::style::Acceleration::ellipse(axes)],
                        clip,
                        ppp,
                        |a| [super::style::ellipse_point(start, axes, a)],
                    )
                    .into_iter()
                    .map(|(angle, _)| angle),
                );
            }
            parameters.sort_by(f64::total_cmp);
            parameters.dedup();
            let angles: Vec<_> = parameters.into_iter().map(f64::sin_cos).collect();
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
                    band_color(config, arc, config.background_opacity),
                );
                inner = outer;
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        if config.show_grid {
            let drawn = |l: &&GannLevel| l.visible && (0.0..=GANN_SQUARE_UNITS).contains(&l.value);
            // A fixed square's price levels lie at whole first units of price, which a
            // logarithmic scale spaces unevenly.
            let level_y = |units: f32| match (self.tool_type, &self.chart_points[..]) {
                (DrawingToolType::GannFixed, [a, b, ..]) => {
                    let units = if config.reverse {
                        GANN_SQUARE_UNITS - units
                    } else {
                        units
                    };
                    (view.price_to_y)(a.price + (b.price - a.price) * f64::from(units))
                }
                _ => at(0.0, units).y,
            };
            for level in config.price_levels.iter().filter(drawn) {
                painter.hline(rect.x_range(), level_y(level.value), stroke(level));
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
                if let Some(points) = gann_square_arc(start, corner, arc.value, clip, ppp) {
                    painter.add(egui::Shape::line(points, stroke(arc)));
                }
            }
        }

        if !config.show_labels {
            return;
        }
        let [a, b, ..] = self.chart_points.as_slice() else {
            return;
        };
        let units = if self.tool_type == DrawingToolType::GannFixed {
            f64::from(GANN_SQUARE_UNITS)
        } else {
            1.0
        };
        let range = (b.price - a.price).abs() * units;
        let bars = f64::from((b.bar_idx - a.bar_idx).abs()) * units;
        if range.is_finite() && bars.is_finite() {
            let decimals = super::measurements::change_decimals(view.instrument, a.price, range);
            let range_text = view.price_formatter.map_or_else(
                || group_thousands(format!("{range:.decimals$}"), view.thousands_sep),
                |formatter| formatter.format(range),
            );
            let mut label = range_text;
            label.push('\n');
            label.push_str(&l10n::format_text(
                painter.ctx(),
                "{count} bars",
                &[("count", (bars.round() as i64).to_string())],
            ));
            if bars > 0.0 {
                let ratio_decimals = decimals + 2;
                label.push('\n');
                label.push_str(&l10n::format_text(
                    painter.ctx(),
                    "{ratio}/bar",
                    &[("ratio", format!("{:.ratio_decimals$}", range / bars))],
                ));
            }
            let (text, fill) = boxed_label_colors(config, painter.ctx());
            let format = egui::TextFormat {
                italics: config.label_italic,
                ..egui::TextFormat::simple(
                    egui::FontId::proportional(typography::font_size(config.label_size)),
                    text,
                )
            };
            let galley = painter.layout_job(egui::text::LayoutJob::simple_format(label, format));
            let pos = Pos2::new(rect.max.x + DESIGN_TOKENS.rounding.lg, rect.min.y);
            let frame =
                egui::Align2::LEFT_TOP.anchor_size(pos, galley.size() + egui::vec2(8.0, 2.0));
            painter.rect_filled(frame, DESIGN_TOKENS.rounding.sm, fill);
            let min = frame.center() - galley.size() / 2.0;
            painter.galley(min, galley.clone(), text);
            if config.label_bold {
                painter.galley(
                    min + egui::vec2(super::style::BOLD_OFFSET, 0.0),
                    galley,
                    text,
                );
            }
        }
    }

    /// Render Gann Box: price and time levels counted from the first anchor, or from the second
    /// when reversed, independent price and time bands, labels on the enabled sides, and optional
    /// rays from the corners.
    pub(crate) fn render_gann_box(&self, painter: &egui::Painter) {
        let &[p1, p2, ..] = self.points.as_slice() else {
            return;
        };
        let config = self.gann_config();
        let (p1, p2) = if config.reverse { (p2, p1) } else { (p1, p2) };
        let rect = Rect::from_two_pos(p1, p2);
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
                    band_color(config, &pair[1], config.background_opacity),
                );
            }
        }
        let show_time = config
            .show_time_background
            .unwrap_or(config.show_background);
        let time_opacity = config
            .time_background_opacity
            .unwrap_or(config.background_opacity);
        if show_time && time_opacity > 0 {
            for pair in times.windows(2) {
                painter.rect_filled(
                    Rect::from_two_pos(
                        Pos2::new(x(pair[0].value), rect.min.y),
                        Pos2::new(x(pair[1].value), rect.max.y),
                    ),
                    0.0,
                    band_color(config, &pair[1], time_opacity),
                );
            }
        }
        for level in &prices {
            painter.hline(
                rect.x_range(),
                y(level.value),
                self.gann_stroke(config, level),
            );
        }
        for level in &times {
            painter.vline(
                x(level.value),
                rect.y_range(),
                self.gann_stroke(config, level),
            );
        }
        if config.show_angles {
            let values = |levels: &[GannLevel]| levels.iter().map(|l| l.value).collect::<Vec<_>>();
            // The rays are no level: they keep the drawing's width and colour, or the one colour.
            let color = config.one_color.map_or_else(
                || self.color32(),
                |[r, g, b, a]| Color32::from_rgba_unmultiplied(r, g, b, a),
            );
            let stroke = Stroke::new(self.stroke_width, color);
            for segment in gann_box_angle_lines(p1, p2, &values(&prices), &values(&times)) {
                painter.line_segment(segment, stroke);
            }
        }
        if !config.show_labels {
            return;
        }

        let gap = DESIGN_TOKENS.spacing.xs;
        let left = prices.iter().filter(|_| config.labels_left).map(|level| {
            (
                level,
                Pos2::new(rect.min.x - gap, y(level.value)),
                egui::Align2::RIGHT_CENTER,
            )
        });
        let bottom = times.iter().filter(|_| config.labels_bottom).map(|level| {
            (
                level,
                Pos2::new(x(level.value), rect.max.y + gap),
                egui::Align2::CENTER_TOP,
            )
        });
        let right = prices.iter().filter(|_| config.labels_right).map(|level| {
            (
                level,
                Pos2::new(rect.max.x + gap, y(level.value)),
                egui::Align2::LEFT_CENTER,
            )
        });
        let top = times.iter().filter(|_| config.labels_top).map(|level| {
            (
                level,
                Pos2::new(x(level.value), rect.min.y - gap),
                egui::Align2::CENTER_BOTTOM,
            )
        });
        // A label gives way only to labels of its own kind, so the price and time labels at the
        // corners both show.
        let mut placed = Vec::<(egui::Align2, Rect)>::new();
        for (level, pos, align) in left.chain(bottom).chain(right).chain(top) {
            let color = level_color(config, level);
            let galley = painter.layout_no_wrap(
                level_label(level.value),
                egui::FontId::proportional(typography::XS),
                color,
            );
            let label = align.anchor_size(pos, galley.size());
            if placed
                .iter()
                .any(|&(kind, r)| kind == align && r.intersects(label.expand(2.0)))
            {
                continue;
            }
            placed.push((align, label));
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
    /// corner, which the price level 1 or 0 already reaches. With the price level 1 hidden, the
    /// corners at y = 0 reach the far corner by the time level there instead.
    #[test]
    fn gann_box_angle_lines_run_from_each_corner_to_the_far_level_points() {
        let (p1, p2) = (Pos2::new(100.0, 0.0), Pos2::new(0.0, 80.0));
        let cases = [
            (
                &[0.0, 0.5, 1.0][..],
                [
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
                ],
            ),
            (
                &[0.0, 0.5][..],
                [
                    ((100.0, 0.0), (0.0, 40.0)),
                    ((100.0, 0.0), (75.0, 80.0)),
                    ((100.0, 0.0), (0.0, 80.0)),
                    ((0.0, 0.0), (100.0, 40.0)),
                    ((0.0, 0.0), (100.0, 80.0)),
                    ((0.0, 0.0), (75.0, 80.0)),
                    ((100.0, 80.0), (0.0, 0.0)),
                    ((100.0, 80.0), (0.0, 40.0)),
                    ((100.0, 80.0), (75.0, 0.0)),
                    ((0.0, 80.0), (100.0, 0.0)),
                    ((0.0, 80.0), (100.0, 40.0)),
                    ((0.0, 80.0), (75.0, 0.0)),
                ],
            ),
        ];
        for (prices, want) in cases {
            let want = want.map(|(a, b)| [Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)]);
            let got = gann_box_angle_lines(p1, p2, prices, &[0.0, 0.25, 1.0]);
            assert!(
                got.len() == want.len()
                    && got
                        .iter()
                        .zip(&want)
                        .all(|(g, w)| g[0].distance(w[0]) < 1e-3 && g[1].distance(w[1]) < 1e-3),
                "prices {prices:?}: got {got:?}, want {want:?}"
            );
        }
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

        // A new box from (0, 200) to (400, 0) labels all seven levels at its side and below it,
        // the price and time labels at its bottom left corner included.
        drawing.gann = None;
        drawing.points = vec![Pos2::new(0.0, 200.0), Pos2::new(400.0, 0.0)];
        let shapes = box_shapes(&drawing);
        let labels = |at: fn(Rect) -> bool| {
            shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(t) if at(t.visual_bounding_rect()) => {
                        Some(t.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let levels = ["0", "0.25", "0.382", "0.5", "0.618", "0.75", "1"];
        assert_eq!(labels(|r| r.top() >= 200.0), levels);
        assert_eq!(labels(|r| r.right() <= 0.0), levels);

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

        // On a logarithmic scale a fixed square still spans five units of price: from 100 with a
        // first unit of 10 over 2 bars its far corner is at 150, not 100 * 1.1^5 = 161.05. With 8
        // price decimals, 0.0000003 over 100 bars reads in full.
        let linear = |p: f64| 300.0 - p as f32;
        let log = |p: f64| 300.0 - 100.0 * p.ln() as f32;
        let eight = crate::model::Instrument::new("TEST").with_price_decimals(8);
        let cases = [
            (
                DrawingToolType::GannSquare,
                [(0.0, 100.0), (10.0, 120.0)],
                None,
                false,
                "20.00\n10 bars\n2.0000/bar",
            ),
            (
                DrawingToolType::GannFixed,
                [(0.0, 100.0), (2.0, 104.0)],
                None,
                false,
                "20.00\n10 bars\n2.0000/bar",
            ),
            (
                DrawingToolType::GannFixed,
                [(0.0, 100.0), (2.0, 110.0)],
                None,
                true,
                "50.00\n10 bars\n5.0000/bar",
            ),
            (
                DrawingToolType::GannSquare,
                [(0.0, 0.0000120), (100.0, 0.0000123)],
                Some(&eight),
                false,
                "0.00000030\n100 bars\n0.0000000030/bar",
            ),
        ];
        for (tool, [(b0, p0), (b1, p1)], instrument, logarithmic, want) in cases {
            let price_to_y: &dyn Fn(f64) -> f32 = if logarithmic { &log } else { &linear };
            let mut drawing = Drawing::new(0, tool);
            drawing.gann = Some(GannConfig {
                show_labels: true,
                ..GannConfig::for_tool(tool)
            });
            drawing.chart_points = vec![ChartPoint::new(b0, p0), ChartPoint::new(b1, p1)];
            drawing.update_screen_coords(|b| b * 10.0, price_to_y);
            let units = if tool == DrawingToolType::GannFixed {
                5.0
            } else {
                1.0
            };
            let corner = Pos2::new(
                (b0 + (b1 - b0) * units) * 10.0,
                price_to_y(p0 + (p1 - p0) * f64::from(units)),
            );
            let want_rect = Rect::from_two_pos(drawing.points[0], corner);
            let outline = drawing.outline(Rect::EVERYTHING).unwrap();
            assert_eq!(Rect::from_points(&outline.areas[0]), want_rect, "{want}");
            let bounds = drawing.paint_bounds().unwrap();
            assert!(bounds.contains_rect(want_rect), "{want}: {bounds:?}");
            let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                drawing.render(
                    ui.painter(),
                    pane,
                    &ChartView {
                        price_formatter: None,
                        thousands_sep: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|b| b * 10.0,
                        bar_spacing: 10.0,
                        price_to_y,
                        y_to_price: &|_| f64::NAN,
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    },
                )
            });
            let labelled = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == want)
            });
            output.drop_without_applying_deltas();
            assert!(labelled, "{tool:?}: {want}");
        }
    }

    /// What `drawing` paints through [`Drawing::render`] with bar b at x = 10 b and price p at
    /// y = 300 - p, in an 800 x 600 pane.
    fn painted(drawing: &Drawing) -> Vec<egui::Shape> {
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            drawing.render(
                ui.painter(),
                Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0)),
                &ChartView {
                    price_formatter: None,
                    thousands_sep: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b * 10.0,
                    bar_spacing: 10.0,
                    price_to_y: &|p| 300.0 - p as f32,
                    y_to_price: &|y| 300.0 - f64::from(y),
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                },
            )
        });
        let shapes: Vec<_> = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    /// A `tool` anchored at bar 0, price 100 and bar 4, price 140, which [`painted`] puts at
    /// (0, 200) and (40, 160), with the tool's defaults as `edit` leaves them. A fixed square's
    /// corner is five of these units from the first anchor, at (200, 0).
    fn anchored(tool: DrawingToolType, edit: impl FnOnce(&mut GannConfig)) -> Drawing {
        let mut drawing = Drawing::new(0, tool);
        drawing.chart_points = vec![
            crate::drawings::ChartPoint::new(0.0, 100.0),
            crate::drawings::ChartPoint::new(4.0, 140.0),
        ];
        let mut config = GannConfig::for_tool(tool);
        edit(&mut config);
        drawing.gann = Some(config);
        drawing.update_screen_coords(|b| b * 10.0, |p| 300.0 - p as f32);
        drawing
    }

    fn placed(manager: &mut crate::drawings::DrawingManager, tool: DrawingToolType) -> Drawing {
        let x_to_bar = |x: f32| x / 10.0;
        let y_to_price = |y: f32| 300.0 - f64::from(y);
        manager.start_drawing_with_coords(tool, Pos2::new(0.0, 200.0), x_to_bar, y_to_price);
        manager.add_point_with_coords(Pos2::new(40.0, 160.0), x_to_bar, y_to_price);
        manager.drawings.pop().unwrap()
    }

    /// The stroked lines among `shapes`, segments and arcs alike, as their ends, width and colour.
    fn strokes(shapes: &[egui::Shape]) -> Vec<([Pos2; 2], f32, Color32)> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::LineSegment { points, stroke } => {
                    Some((*points, stroke.width, stroke.color))
                }
                egui::Shape::Path(path) if path.stroke.width > 0.0 => {
                    let egui::epaint::ColorMode::Solid(color) = path.stroke.color else {
                        return None;
                    };
                    let ends = [*path.points.first()?, *path.points.last()?];
                    Some((ends, path.stroke.width, color))
                }
                _ => None,
            })
            .collect()
    }

    /// A newly placed box labels all four sides, which stay on their sides when reversed.
    /// y(0.5) = 180 and x(0.25) = 10.
    /// Price and time bands span (0,160)-(40,200) and (0,160)-(20,200), respectively.
    #[test]
    fn gann_box_labels_sides_and_band_fills_follow_their_settings() {
        const GREY: [u8; 4] = [128, 128, 128, 255];
        for (sides, show, reverse) in [
            (None, true, false),
            (Some([true, false, false, true]), true, false),
            (Some([false, true, false, false]), true, false),
            (Some([false, false, true, false]), true, false),
            (Some([true; 4]), true, false),
            (Some([true; 4]), false, false),
            (Some([false; 4]), true, false),
            (Some([true; 4]), true, true),
        ] {
            let mut drawing = placed(&mut Default::default(), DrawingToolType::GannBox);
            let c = drawing.gann.get_or_insert_with(GannConfig::default);
            c.price_levels = vec![GannLevel::new(0.5, GREY)];
            c.time_levels = vec![GannLevel::new(0.25, GREY)];
            c.show_background = false;
            c.show_labels = show;
            c.reverse = reverse;
            if let Some(sides) = sides {
                [c.labels_left, c.labels_right, c.labels_top, c.labels_bottom] = sides;
            }
            let sides = sides.unwrap_or([true; 4]);
            let shapes = painted(&drawing);
            let labels: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels.len(),
                if show {
                    sides.into_iter().filter(|s| *s).count()
                } else {
                    0
                }
            );
            for (side, enabled) in sides.into_iter().enumerate() {
                let found = labels.iter().find(|text| {
                    let bounds = text.visual_bounding_rect();
                    match side {
                        0 => bounds.right() <= 0.0,
                        1 => bounds.left() >= 40.0,
                        2 => bounds.bottom() <= 160.0,
                        _ => bounds.top() >= 200.0,
                    }
                });
                assert_eq!(found.is_some(), enabled && show, "side {side}, {sides:?}");
                if let Some(text) = found {
                    let bounds = text.visual_bounding_rect();
                    assert_eq!(text.galley.text(), if side < 2 { "0.5" } else { "0.25" });
                    let (got, want) = if side < 2 {
                        (bounds.center().y, 180.0)
                    } else {
                        (bounds.center().x, if reverse { 30.0 } else { 10.0 })
                    };
                    assert!((got - want).abs() <= 1.0, "{side}: {got} != {want}");
                }
            }
        }
        for (price, time, opacity, expected) in [
            (true, None, None, vec![(40.0, 51), (20.0, 51)]),
            (true, Some(true), Some(102), vec![(40.0, 51), (20.0, 102)]),
            (false, Some(true), Some(102), vec![(20.0, 102)]),
            (true, Some(false), None, vec![(40.0, 51)]),
            (true, None, Some(0), vec![(40.0, 51)]),
        ] {
            let drawing = anchored(DrawingToolType::GannBox, |c| {
                c.price_levels = [0.0, 1.0].map(|v| GannLevel::new(v, GREY)).to_vec();
                c.time_levels = [0.0, 0.5].map(|v| GannLevel::new(v, GREY)).to_vec();
                c.show_labels = false;
                c.show_background = price;
                c.background_opacity = 51;
                c.show_time_background = time;
                c.time_background_opacity = opacity;
            });
            let shapes = painted(&drawing);
            let fills: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Rect(r) if r.fill.a() > 0 => Some((r.rect, r.fill)),
                    _ => None,
                })
                .collect();
            let want: Vec<_> = expected
                .into_iter()
                .map(|(x, a)| {
                    (
                        Rect::from_min_max(Pos2::new(0.0, 160.0), Pos2::new(x, 200.0)),
                        Color32::from_rgba_unmultiplied(128, 128, 128, a),
                    )
                })
                .collect();
            assert_eq!(fills, want, "{price}, {time:?}, {opacity:?}");
        }
    }

    /// New squares use 12 point labels and restored squares keep 10; each square's label switch,
    /// typography and paint bounds reach the painted text.
    #[test]
    fn gann_square_labels_follow_their_switch_and_text_style() {
        use DrawingToolType::{GannFixed, GannSquare};
        type Row = (
            DrawingToolType,
            Option<fn(&mut GannConfig)>,
            usize,
            f32,
            bool,
        );
        let rows: [Row; 10] = [
            (GannSquare, Some(|_| {}), 1, 12.0, false),
            (GannFixed, Some(|_| {}), 0, 12.0, false),
            (GannSquare, None, 1, 10.0, false),
            (GannSquare, Some(|c| c.show_labels = false), 0, 12.0, false),
            (GannFixed, None, 0, 10.0, false),
            (GannFixed, Some(|c| c.show_labels = true), 1, 12.0, false),
            (
                GannSquare,
                Some(|c| {
                    c.label_size = 20.0;
                    c.label_italic = true;
                }),
                1,
                20.0,
                true,
            ),
            (GannSquare, Some(|c| c.label_bold = true), 2, 12.0, false),
            (GannSquare, Some(|c| c.label_size = 96.0), 1, 96.0, false),
            (
                GannFixed,
                Some(|c| {
                    c.show_labels = true;
                    c.label_size = 96.0;
                }),
                1,
                96.0,
                false,
            ),
        ];
        for (tool, edit, count, size, italic) in rows {
            let mut drawing = placed(&mut Default::default(), tool);
            if let Some(edit) = edit {
                edit(
                    drawing
                        .gann
                        .get_or_insert_with(|| GannConfig::for_tool(tool)),
                );
            } else {
                drawing.gann = None;
            }
            let shapes = painted(&drawing);
            let labels: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            assert_eq!(labels.len(), count, "{tool:?}, {size}");
            if count == 0 {
                assert!(
                    !shapes
                        .iter()
                        .any(|s| matches!(s, egui::Shape::Rect(r) if r.fill.a() > 0))
                );
            }
            for label in &labels {
                assert!(label.galley.text().contains("bars"));
                let format = &label.galley.job.sections[0].format;
                assert_eq!(format.font_id.size, size);
                assert_eq!(format.italics, italic);
                assert!(
                    drawing
                        .paint_bounds()
                        .unwrap()
                        .contains_rect(label.visual_bounding_rect()),
                    "{tool:?}, {size}"
                );
            }
            if count == 2 {
                assert_eq!(labels[0].galley.text(), labels[1].galley.text());
                assert!(
                    (labels[1].pos
                        - labels[0].pos
                        - egui::vec2(super::super::style::BOLD_OFFSET, 0.0))
                    .length()
                        < 1e-5
                );
            }
        }
    }

    /// At width 2, an 8-on/4-off dash runs (5.657,-5.657) along 1x1 or (7.155,-3.578)
    /// along 1x2. The pane clips them at x = 200 and 400, about 24 and 38 dashes.
    #[test]
    fn gann_fan_lines_take_their_own_line_style() {
        use crate::model::LineStyle::{Dashed, Solid};
        const A: [u8; 4] = [255, 0, 0, 255];
        const B: [u8; 4] = [0, 0, 255, 255];
        for (drawing_style, own, full) in [
            (Solid, Some(Dashed), [false, true]),
            (Dashed, Some(Solid), [true, false]),
            (Dashed, None, [false, false]),
        ] {
            let mut drawing = anchored(DrawingToolType::GannFan, |c| {
                c.show_labels = false;
                c.show_background = false;
                c.fan_ratios = vec![
                    GannLevel {
                        line_style: own,
                        ..GannLevel::new(1.0, A)
                    },
                    GannLevel::new(0.5, B),
                ];
            });
            drawing.line_style = drawing_style;
            let lines = strokes(&painted(&drawing));
            for (i, (color, slope, count, first)) in [
                (A, 1.0, 23..=25, (5.656854, 194.34315)),
                (B, 0.5, 37..=39, (7.155418, 196.42229)),
            ]
            .into_iter()
            .enumerate()
            {
                let color = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                let segments: Vec<_> = lines.iter().filter(|l| l.2 == color).collect();
                assert!(segments.iter().all(|l| l.1 == 2.0));
                if full[i] {
                    assert_eq!(segments.len(), 1, "{drawing_style:?}, {own:?}, line {i}");
                    assert_eq!(
                        segments[0].0,
                        [
                            Pos2::new(0.0, 200.0),
                            Pos2::new(800.0, 200.0 - 800.0 * slope)
                        ]
                    );
                } else {
                    assert!(
                        count.contains(&segments.len()),
                        "{drawing_style:?}, {own:?}, line {i}: {}",
                        segments.len()
                    );
                    assert_eq!(segments[0].0[0], Pos2::new(0.0, 200.0));
                    assert!(segments[0].0[1].distance(Pos2::new(first.0, first.1)) < 0.001);
                    assert!(
                        segments
                            .iter()
                            .flat_map(|l| l.0)
                            .all(|p| (p.y - (200.0 - slope * p.x)).abs() < 0.01)
                    );
                }
            }
        }
    }

    /// One line of each tool at 5 px, its ends by hand: the box's 0.5 price level at
    /// y = 200 - 40 * 0.5 = 180, the fixed square's first time unit at x = 200 / 5 = 40, the fan's
    /// 1x1 line through the second anchor to the pane's edge at x = 800, the square's 1x1 fan line
    /// to its corner and its arc of one unit, a quarter ellipse with axes of 40 / 5 = 8 px. Every
    /// other line keeps the drawing's 2 px.
    #[test]
    fn gann_tools_draw_each_line_at_its_own_width() {
        type Row = (
            DrawingToolType,
            fn(&mut GannConfig) -> &mut GannLevel,
            [(f32, f32); 2],
        );
        let rows: [Row; 5] = [
            (
                DrawingToolType::GannBox,
                |c| &mut c.price_levels[3],
                [(0.0, 180.0), (40.0, 180.0)],
            ),
            (
                DrawingToolType::GannFixed,
                |c| &mut c.time_levels[1],
                [(40.0, 0.0), (40.0, 200.0)],
            ),
            (
                DrawingToolType::GannFan,
                |c| &mut c.fan_ratios[4],
                [(0.0, 200.0), (800.0, -600.0)],
            ),
            (
                DrawingToolType::GannSquare,
                |c| &mut c.fan_ratios[5],
                [(0.0, 200.0), (40.0, 160.0)],
            ),
            (
                DrawingToolType::GannSquare,
                |c| &mut c.arc_ratios[0],
                [(8.0, 200.0), (0.0, 192.0)],
            ),
        ];
        for (tool, line, [a, b]) in rows {
            let drawing = anchored(tool, |config| line(config).width = Some(5.0));
            let (wide, rest): (Vec<_>, Vec<_>) = strokes(&painted(&drawing))
                .into_iter()
                .partition(|&(_, width, _)| width == 5.0);
            assert!(
                matches!(wide[..], [([p, q], ..)]
                    if p.distance(Pos2::new(a.0, a.1)) < 1e-3
                        && q.distance(Pos2::new(b.0, b.1)) < 1e-3),
                "{tool:?}: {wide:?}"
            );
            assert!(
                !rest.is_empty() && rest.iter().all(|&(_, width, _)| width == 2.0),
                "{tool:?}: {rest:?}"
            );
        }
    }

    /// At [`anchored`]'s points, by hand. Without labels the box and the fan paint no text, nor
    /// the fan its label boxes. Reversed, the box counts from its second anchor: its 0.25 levels
    /// lie at y = 160 + 40 * 0.25 = 170 and x = 40 - 40 * 0.25 = 30, not at 190 and 10. The
    /// reversed fixed square is the plain one turned about its centre (100, 100): its first price
    /// unit at 100 + 40 * (5 - 1) = 260, y 40, its first time unit at x 160, its 1x2 fan line from
    /// (200, 0) and its arc of one unit from (160, 0) to (200, 40), where the plain one has y 160,
    /// x 40, a fan line from (0, 200) to (200, 100) and an arc from (40, 200) to (0, 160). In one
    /// colour every stroke, band and label takes it; a label on a box takes it as its fill under
    /// white text, whose contrast on (200, 0, 200) is 1.05 / 0.2145 = 4.9.
    #[test]
    fn gann_switches_hide_labels_mirror_and_draw_in_one_colour() {
        const GREY: [u8; 4] = [128, 128, 128, 255];
        enum Want {
            NoLabels,
            Strokes(&'static [[(f32, f32); 2]]),
            OneColour,
        }
        type Row = (DrawingToolType, fn(&mut GannConfig), Want);
        let one = |c: &mut GannConfig| {
            c.one_color = Some([200, 0, 200, 255]);
            c.show_labels = true;
        };
        let rows: [Row; 8] = [
            (
                DrawingToolType::GannBox,
                |c| {
                    c.show_background = false;
                    c.show_labels = false;
                },
                Want::NoLabels,
            ),
            (
                DrawingToolType::GannFan,
                |c| c.show_labels = false,
                Want::NoLabels,
            ),
            (
                DrawingToolType::GannBox,
                |c| {
                    c.price_levels = vec![GannLevel::new(0.25, GREY)];
                    c.time_levels = vec![GannLevel::new(0.25, GREY)];
                    c.reverse = true;
                },
                Want::Strokes(&[
                    [(0.0, 170.0), (40.0, 170.0)],
                    [(30.0, 160.0), (30.0, 200.0)],
                ]),
            ),
            (
                DrawingToolType::GannFixed,
                |c| {
                    let one = vec![GannLevel::new(1.0, GREY)];
                    c.price_levels = one.clone();
                    c.time_levels = one.clone();
                    c.fan_ratios = vec![GannLevel::new(0.5, GREY)];
                    c.arc_ratios = one;
                    c.show_background = false;
                    c.reverse = true;
                },
                Want::Strokes(&[
                    [(0.0, 40.0), (200.0, 40.0)],
                    [(160.0, 0.0), (160.0, 200.0)],
                    [(200.0, 0.0), (0.0, 100.0)],
                    [(160.0, 0.0), (200.0, 40.0)],
                ]),
            ),
            (
                DrawingToolType::GannBox,
                |c| {
                    c.one_color = Some([200, 0, 200, 255]);
                    c.show_angles = true;
                },
                Want::OneColour,
            ),
            (DrawingToolType::GannFan, one, Want::OneColour),
            (DrawingToolType::GannSquare, one, Want::OneColour),
            (DrawingToolType::GannFixed, one, Want::OneColour),
        ];
        let is_text = |shape: &egui::Shape| matches!(shape, egui::Shape::Text(_));
        for (tool, edit, want) in rows {
            let drawing = anchored(tool, edit);
            let shapes = painted(&drawing);
            match want {
                Want::NoLabels => {
                    assert!(
                        !shapes
                            .iter()
                            .any(|s| is_text(s) || matches!(s, egui::Shape::Rect(_))),
                        "{tool:?}"
                    );
                    let mut labelled = drawing.clone();
                    labelled.gann.as_mut().unwrap().show_labels = true;
                    let labels = painted(&labelled).iter().filter(|s| is_text(s)).count();
                    assert!(
                        if tool == DrawingToolType::GannFan {
                            labels == 2
                        } else {
                            labels > 0
                        },
                        "{tool:?}: {labels} labels"
                    );
                }
                Want::Strokes(want) => {
                    let got = strokes(&shapes);
                    assert!(
                        got.len() == want.len()
                            && got.iter().zip(want).all(|((ends, ..), want)| {
                                ends.iter()
                                    .zip(want)
                                    .all(|(p, w)| p.distance(Pos2::new(w.0, w.1)) < 1e-3)
                            }),
                        "{tool:?}: got {got:?}, want {want:?}"
                    );
                }
                Want::OneColour => {
                    let color = Color32::from_rgb(200, 0, 200);
                    let band = Color32::from_rgba_unmultiplied(200, 0, 200, 51);
                    let boxed = tool == DrawingToolType::GannBox;
                    let (label_fill, text) = if boxed {
                        (band, color)
                    } else {
                        (color, Color32::WHITE)
                    };
                    let lines = strokes(&shapes);
                    assert!(
                        !lines.is_empty() && lines.iter().all(|&(_, _, c)| c == color),
                        "{tool:?}: {lines:?}"
                    );
                    let (mut bands, mut labels) = (0, 0);
                    for shape in &shapes {
                        match shape {
                            egui::Shape::Path(path) if path.fill.a() > 0 => {
                                assert_eq!(path.fill, band, "{tool:?}");
                                bands += 1;
                            }
                            egui::Shape::Mesh(mesh) => {
                                assert!(mesh.vertices.iter().all(|v| v.color == band), "{tool:?}");
                                bands += 1;
                            }
                            egui::Shape::Rect(rect) if rect.fill.a() > 0 => {
                                assert_eq!(rect.fill, label_fill, "{tool:?}");
                                bands += usize::from(boxed);
                            }
                            egui::Shape::Text(label) => {
                                assert_eq!(label.fallback_color, text, "{tool:?}");
                                labels += 1;
                            }
                            _ => {}
                        }
                    }
                    assert!(
                        bands > 0 && labels > 0,
                        "{tool:?}: {bands} bands, {labels} labels"
                    );
                }
            }
        }
    }

    /// A saved `gann` object without fields draws what no `gann` draws, for every Gann tool: the
    /// fan's nine lines, and the square's six levels at whole units rather than the box's seven
    /// fractions. An object saved before levels had a width and the tools their label, reverse
    /// and one colour switches draws its settings as then: labels as the tool restored them,
    /// counted from the first anchor, in the level colours at the drawing's width.
    #[test]
    fn an_empty_gann_object_draws_the_tool_defaults() {
        use crate::drawings::ChartPoint;
        const SAVED: &str = r#"{"price_levels":[{"value":0.0,"visible":true,"color":[128,128,128,255]},{"value":0.5,"visible":true,"color":[255,152,0,255]}],"time_levels":[{"value":0.25,"visible":true,"color":[0,188,212,255]},{"value":1.0,"visible":false,"color":[76,175,80,255]}],"show_grid":true,"show_fan":true,"fan_ratios":[{"value":1.0,"visible":true,"color":[41,98,255,255]},{"value":0.5,"visible":true,"color":[8,153,129,255]}],"fan_direction":"Both","show_arcs":true,"arc_ratios":[{"value":1.0,"visible":true,"color":[255,152,0,255]},{"value":2.0,"visible":true,"color":[0,188,212,255]}],"show_background":true,"background_opacity":51,"show_angles":true}"#;
        let level = |value, color| GannLevel::new(value, color);
        let then = GannConfig {
            price_levels: vec![
                level(0.0, [128, 128, 128, 255]),
                level(0.5, [255, 152, 0, 255]),
            ],
            time_levels: vec![
                level(0.25, [0, 188, 212, 255]),
                GannLevel {
                    visible: false,
                    ..level(1.0, [76, 175, 80, 255])
                },
            ],
            show_grid: true,
            show_fan: true,
            fan_ratios: vec![
                level(1.0, [41, 98, 255, 255]),
                level(0.5, [8, 153, 129, 255]),
            ],
            fan_direction: GannFanDirection::Both,
            show_arcs: true,
            arc_ratios: vec![
                level(1.0, [255, 152, 0, 255]),
                level(2.0, [0, 188, 212, 255]),
            ],
            show_background: true,
            background_opacity: 51,
            show_angles: true,
            show_labels: true,
            reverse: false,
            one_color: None,
            labels_left: true,
            labels_right: false,
            labels_top: false,
            labels_bottom: true,
            show_time_background: None,
            time_background_opacity: None,
            label_size: 10.0,
            label_bold: false,
            label_italic: false,
        };
        for tool in [
            DrawingToolType::GannBox,
            DrawingToolType::GannSquare,
            DrawingToolType::GannFixed,
            DrawingToolType::GannFan,
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(4.0, 140.0)];
            for (saved, gann) in [
                ("null", None),
                ("{}", None),
                (
                    SAVED,
                    Some(GannConfig {
                        show_labels: tool != DrawingToolType::GannFixed,
                        ..then.clone()
                    }),
                ),
            ] {
                let mut file = serde_json::to_value(&drawing).unwrap();
                file["gann"] = serde_json::from_str(saved).unwrap();
                let mut loaded: Drawing = serde_json::from_value(file).unwrap();
                loaded.chart_points = drawing.chart_points.clone();
                drawing.gann = gann;
                for d in [&mut drawing, &mut loaded] {
                    d.update_screen_coords(|b| b * 10.0, |p| 300.0 - p as f32);
                }
                let config = loaded.gann_config();
                assert_eq!(
                    [
                        config.labels_left,
                        config.labels_right,
                        config.labels_top,
                        config.labels_bottom
                    ],
                    [true, false, false, true],
                );
                assert_eq!(config.label_size, 10.0);
                assert_eq!(painted(&loaded), painted(&drawing), "{tool:?} {saved}");

                let template = crate::drawings::DrawingTemplate::from_drawing(&loaded);
                let mut fresh = placed(&mut Default::default(), tool);
                template.apply_to(&mut fresh).unwrap();
                assert_eq!(
                    painted(&fresh),
                    painted(&loaded),
                    "template {tool:?} {saved}"
                );
                let mut manager = crate::drawings::DrawingManager::new();
                manager.tool_defaults.insert(tool, template);
                let fresh = placed(&mut manager, tool);
                assert_eq!(
                    painted(&fresh),
                    painted(&loaded),
                    "default {tool:?} {saved}"
                );
            }
        }

        // A field the object holds keeps its value; only the ones it leaves out take the tool's.
        let mut saved = serde_json::to_value(Drawing::new(0, DrawingToolType::GannSquare)).unwrap();
        saved["gann"] = serde_json::json!({"price_levels": [2.0]});
        let gann = serde_json::from_value::<Drawing>(saved)
            .unwrap()
            .gann
            .unwrap();
        let defaults = GannConfig::for_tool(DrawingToolType::GannSquare);
        assert_eq!(gann.price_levels.len(), 1);
        assert_eq!(gann.price_levels[0].value, 2.0);
        assert_eq!(gann.time_levels, defaults.time_levels);
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
        let arc = |radius| gann_square_arc(start, corner, radius, Rect::EVERYTHING, 1.0);
        let unit = arc(1.0).unwrap();
        assert!(near(unit[0], (20.0, 0.0)) && near(*unit.last().unwrap(), (0.0, 20.0)));
        assert!(near(unit[unit.len() / 2], (14.142, 14.142)));
        let cut = arc(26f32.sqrt()).unwrap();
        assert!(near(cut[0], (100.0, 20.0)) && near(*cut.last().unwrap(), (20.0, 100.0)));
        for radius in [7.5, 0.0, f32::NAN] {
            assert_eq!(arc(radius), None, "{radius}");
        }

        // Radius 5 at 45 degrees is 70.711 on each axis, the square root of 26 is 72.111, and
        // both end on the square's edges at its two sides.
        let angles: Vec<_> = (0..=16)
            .map(|i| (std::f64::consts::FRAC_PI_2 * f64::from(i) / 16.0).sin_cos())
            .collect();
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
