//! Rendering implementations for all drawing tool types.
//!
//! This module implements `Drawing::render()` and `Drawing::render_original()`,
//! which dispatch to category-specific rendering methods based on
//! [`DrawingToolType`]. Each category is
//! implemented in its own sub-module as `impl Drawing` blocks that add private
//! render methods (e.g., `render_trend_line`, `render_fibonacci`, etc.).
//!
//! # Category sub-modules
//!
//! | Module | Tools | Description |
//! |---|---|---|
//! | `lines` | 9 | Trend lines, rays, extended/horizontal/vertical lines |
//! | `shapes` | 10 | Rectangles, circles, ellipses, triangles, arcs, curves |
//! | `channels` | 4 | Parallel channels, regression, flat top/bottom, disjoint |
//! | `pitchfork` | 4 | Andrews/Schiff/Modified Schiff/Inside pitchforks |
//! | `fibonacci` | 11 | Retracements, extensions, fans, time zones, circles, spiral |
//! | `gann` | 4 | Gann fan, box, square, fixed |
//! | `patterns` | 6 | XABCD, cypher, head & shoulders, ABCD, triangle, three drives |
//! | `elliott` | 5 | Impulse, correction, triangle, double/triple combo waves |
//! | `cycles` | 3 | Cyclic lines, time cycles, sine line |
//! | `annotations` | 11 | Text labels, notes, callouts, price labels, signposts |
//! | `measurements` | 4 | Measure, price range, date range, date+price range |
//! | `trading` | 9 | Long/short positions, forecasts, VWAP, volume profiles |
//! | `media` | 3 | Image/tweet/idea placeholders |
//!
//! # Public API
//!
//! - `DrawingRenderer` trait (in `traits`) -- defines the interface for modular renderers
//! - `Drawing::render()` -- the main entry point, dispatches to `render_original()`
//! - `ChartView` -- the bars and coordinate mapping drawings are rendered against
//!
//! # Usage
//!
//! ```
//! use egui_charts::drawings::{Drawing, DrawingToolType};
//! use egui_charts::drawings::rendering::ChartView;
//!
//! # fn example(
//! #     painter: &egui::Painter,
//! #     price_rect: egui::Rect,
//! #     bars: &[egui_charts::model::Bar],
//! #     revision: u64,
//! #     bar_to_x: impl Fn(f32) -> f32,
//! #     price_to_y: impl Fn(f64) -> f32,
//! # ) {
//! let drawing = Drawing::new(1, DrawingToolType::TrendLine);
//! // After setting up points and calling update_screen_coords:
//! let view = ChartView { instrument: None, bars, revision, bar_to_x: &bar_to_x, price_to_y: &price_to_y };
//! drawing.render(painter, price_rect, &view);
//! # }
//! ```

mod annotations;
mod channels;
mod cycles;
mod elliott;
mod fibonacci;
mod gann;
mod lines;
mod measurements;
mod media;
mod patterns;
mod pitchfork;
mod shapes;
mod trading;
pub mod traits;
// The painter the drawing renderers share with the chart.
pub(crate) mod utils {
    pub(crate) use crate::chart::rendering::draw_dashed_line;
}

pub use traits::DrawingRenderer;

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::model::{Bar, SpanCache};
use egui::{Pos2, Rect};
use std::rc::Rc;

/// The bars and coordinate mapping that drawings are rendered against.
///
/// Drawing anchors index into `bars`. The anchored VWAP, both volume profiles,
/// the regression trend, the position and forecast results and the bars
/// pattern are computed from these bars.
pub struct ChartView<'a> {
    /// Instrument metadata supplied by the host or feed, for tick and pip measurements.
    pub instrument: Option<&'a crate::model::Instrument>,
    /// The series that drawing anchors index into.
    pub bars: &'a [Bar],
    /// [`ChartState::revision`](crate::model::ChartState::revision) of `bars`.
    /// Results computed from the bars are cached under it, so it must change
    /// whenever a bar before the last one changes.
    pub revision: u64,
    /// Screen x of a (fractional) bar index.
    pub bar_to_x: &'a dyn Fn(f32) -> f32,
    /// Screen y of a price.
    pub price_to_y: &'a dyn Fn(f64) -> f32,
}

/// Index of the first bar and the bars from anchor `a` to anchor `b`
/// (inclusive, either order), clamped to the series; `None` when no bar lies
/// between them.
pub(crate) fn bars_between(bars: &[Bar], a: f32, b: f32) -> Option<(usize, &[Bar])> {
    let last = bars.len().checked_sub(1)?;
    let (first, end) = (a.min(b).round(), a.max(b).round());
    if end < 0.0 {
        return None;
    }
    // Clamp as indices: past 2^24 bars an f32 no longer holds every index.
    let (first, end) = (first.max(0.0) as usize, (end as usize).min(last));
    (first <= end).then(|| (first, &bars[first..=end]))
}

impl ChartView<'_> {
    /// `build` of `bars`, the span that starts at bar `first`, cached in
    /// `cache`. A span that ends on the last bar, which may be forming, is
    /// extended one bar at a time with `extend` as the series streams.
    pub(crate) fn cached<T, P: Copy + PartialEq>(
        &self,
        cache: &mut SpanCache<T, P>,
        param: P,
        first: usize,
        bars: &[Bar],
        build: impl Fn(&[Bar]) -> T,
        extend: impl Fn(&T, &Bar) -> Option<T>,
    ) -> Rc<T> {
        if first + bars.len() == self.bars.len() {
            cache.get_extended(param, first, bars, self.revision, build, extend)
        } else {
            cache.get(param, first, bars, self.revision, build)
        }
    }
}

/// The lines a drawing draws and the convex areas it fills, in screen
/// coordinates, from the geometry its renderer draws.
#[derive(Debug, Default)]
pub(crate) struct Outline {
    pub(crate) lines: Vec<[Pos2; 2]>,
    pub(crate) areas: Vec<Vec<Pos2>>,
    /// The width the lines are stroked at, where it is wider than the
    /// drawing's own stroke (the brushes).
    pub(crate) width: f32,
}

impl Drawing {
    /// What this drawing draws, with lines that run to the chart's edge cut at
    /// the edge of `rect`. `None` for the tools not described here (text,
    /// media, the data-driven tools and the Fibonacci arcs, circles, spiral and
    /// wedge, the cycles, the position and projection tools) and below the
    /// anchors a tool needs.
    pub(crate) fn outline(&self, rect: Rect) -> Option<Outline> {
        use DrawingToolType as T;
        let p = self.points.as_slice();
        let path = |closed: bool| {
            let mut lines: Vec<[Pos2; 2]> = p.windows(2).map(|w| [w[0], w[1]]).collect();
            if closed && p.len() >= 3 {
                lines.push([p[p.len() - 1], p[0]]);
            }
            lines
        };
        let corners = |r: Rect| {
            vec![
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ]
        };
        let edges = |c: &[Pos2]| (0..c.len()).map(|i| [c[i], c[(i + 1) % c.len()]]).collect();
        // A stroked and filled box: its border, and its inside.
        let boxed = |outline: &mut Outline, r: Rect| {
            let c = corners(r);
            outline.lines = edges(&c);
            outline.areas.push(c);
        };
        let filled = self.fill_color.is_some();
        let &first = p.first()?;
        let across = |y: f32| [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)];
        let mut outline = Outline::default();
        match self.tool_type {
            T::HorizontalRay => outline.lines = vec![[first, Pos2::new(rect.max.x, first.y)]],
            T::CrossLine => {
                let down = [
                    Pos2::new(first.x, rect.min.y),
                    Pos2::new(first.x, rect.max.y),
                ];
                outline.lines = vec![across(first.y), down];
            }
            _ => {}
        }
        if !outline.lines.is_empty() {
            return Some(outline);
        }
        let &second = p.get(1)?;
        match self.tool_type {
            T::TrendLine => {
                let (left, right) = self.trend_line_ends(rect.min.x, rect.max.x)?;
                outline.lines = vec![[left, right]];
            }
            T::TrendAngle
            | T::InfoLine
            | T::Arrow
            | T::Polyline
            | T::Path
            | T::Brush
            | T::Highlighter
            | T::XABCDPattern
            | T::CypherPattern
            | T::ABCDPattern
            | T::ThreeDrivesPattern
            | T::ElliottImpulse
            | T::ElliottCorrection
            | T::ElliottTriangle
            | T::ElliottDoubleCombo
            | T::ElliottTripleCombo => {
                outline.lines = path(false);
                outline.width = self.brush_width().unwrap_or(0.0);
            }
            T::HeadAndShoulders => {
                outline.lines = path(false);
                outline.lines.extend(self.neckline().map(|(line, _)| line));
            }
            T::TrianglePattern => outline.lines = path(true),
            T::Triangle => {
                let [a, b, c, ..] = *p else { return None };
                outline.lines = vec![[a, b], [b, c], [c, a]];
                if filled {
                    outline.areas.push(vec![a, b, c]);
                }
            }
            T::Measure | T::DateAndPriceRange | T::GannBox => {
                boxed(&mut outline, Rect::from_two_pos(first, second));
            }
            T::GannFixed => boxed(&mut outline, Rect::from_two_pos(first, second)),
            T::GannSquare => {
                let corner = gann::gann_square_corner(first, second);
                boxed(&mut outline, Rect::from_two_pos(first, corner));
            }
            T::RotatedRect => {
                outline.lines = edges(&corners(Rect::from_two_pos(first, second)));
            }
            T::Ray => outline.lines = vec![[first, lines::ray_end(first, second, rect)]],
            T::ExtendedLine => outline.lines = vec![lines::extended_line(first, second, rect)],
            T::PriceRange => {
                let mid_x = (first.x + second.x) / 2.0;
                outline.lines = vec![
                    across(first.y),
                    across(second.y),
                    [Pos2::new(mid_x, first.y), Pos2::new(mid_x, second.y)],
                ];
            }
            T::DateRange => {
                let band = Rect::from_x_y_ranges(
                    first.x.min(second.x)..=first.x.max(second.x),
                    rect.y_range(),
                );
                outline.lines = [first.x, second.x]
                    .map(|x| [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)])
                    .into();
                outline.areas.push(corners(band));
            }
            T::Circle => {
                let radius = first.distance(second);
                let ring: Vec<Pos2> = (0..=64)
                    .map(|i| {
                        let angle = i as f32 / 64.0 * std::f32::consts::TAU;
                        first + radius * egui::vec2(angle.cos(), angle.sin())
                    })
                    .collect();
                outline.lines = ring.windows(2).map(|w| [w[0], w[1]]).collect();
                if filled {
                    outline.areas.push(ring);
                }
            }
            T::Ellipse => {
                let ring = lines::ellipse_points(first, second);
                outline.lines = ring.windows(2).map(|w| [w[0], w[1]]).collect();
                if filled {
                    outline.areas.push(ring);
                }
            }
            T::Arc => {
                outline.lines = lines::arc_points(first, second)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::Curve => {
                let [p0, p1, p2] = self.curve_controls()?;
                outline.lines = shapes::curve_points(p0, p1, p2)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::DoubleCurve => {
                outline.lines = shapes::double_curve_points(first, second)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::Pitchfork | T::SchiffPitchfork | T::ModifiedSchiffPitchfork | T::InsidePitchfork => {
                outline.lines = self.pitchfork_lines(rect)?;
                outline.areas = self.pitchfork_bands(rect);
            }
            T::Pitchfan => outline.lines = pitchfork::pitchfan_lines(first, second, rect),
            T::GannFan => outline.lines = gann::gann_fan_lines(first, second, rect),
            T::FlatTopBottom => {
                let &third = p.get(2)?;
                outline.lines = vec![[first, second], channels::flat_line(first, second, third)];
                outline
                    .areas
                    .push(channels::flat_top_bottom_fill(first, second, third));
            }
            T::ParallelChannel => return self.parallel_channel_outline(rect),
            T::DisjointChannel => {
                let &third = p.get(2)?;
                let [_, fourth] = channels::disjoint_second_line(first, second, third);
                outline.lines = vec![[first, second], [third, fourth]];
                outline.areas.push(vec![first, second, fourth, third]);
            }
            T::FibonacciTimeZones => {
                outline.lines = fibonacci::time_zone_lines(first, second, rect)
            }
            T::FibonacciSpeedFan => outline.lines = fibonacci::speed_fan_lines(first, second, rect),
            _ => return None,
        }
        Some(outline)
    }
}

/// Results kept per kind of drawing, more than the drawings of one kind on screen.
pub(crate) const MAX_CACHED_SPANS: usize = 32;

impl Drawing {
    /// Main rendering dispatcher - routes to category-specific render methods
    ///
    /// This method handles rendering for every drawing tool type,
    /// delegating to the appropriate category-specific implementation.
    pub fn render_original(&self, painter: &egui::Painter, rect: Rect, view: &ChartView) {
        if self.points.is_empty() {
            return;
        }

        let color = self.color32();
        let stroke = self.stroke();

        match self.tool_type {
            // Simple Lines (9 tools)
            DrawingToolType::TrendLine => self.render_trend_line(painter, rect, color, stroke),
            DrawingToolType::Ray => self.render_ray(painter, rect, color, stroke),
            DrawingToolType::ExtendedLine => {
                self.render_extended_line(painter, rect, color, stroke)
            }
            DrawingToolType::HorizontalLine => self.render_horizontal_line(painter, rect, stroke),
            DrawingToolType::HorizontalRay => {
                self.render_horizontal_ray(painter, rect, color, stroke)
            }
            DrawingToolType::VerticalLine => self.render_vertical_line(painter, rect, stroke),
            DrawingToolType::CrossLine => self.render_cross_line(painter, rect, color, stroke),
            DrawingToolType::TrendAngle => self.render_trend_angle(painter, color, stroke, view),
            DrawingToolType::InfoLine => self.render_info_line(painter, view),

            // Shapes (8 tools)
            DrawingToolType::Rect => self.render_rect(painter, color, stroke),
            DrawingToolType::RotatedRect => self.render_rotated_rect(painter),
            DrawingToolType::Circle => self.render_circle(painter, color, stroke),
            DrawingToolType::Ellipse => self.render_ellipse(painter, color, stroke),
            DrawingToolType::Triangle => self.render_triangle(painter, color, stroke),
            DrawingToolType::Arc => self.render_arc(painter, color, stroke),
            DrawingToolType::Polyline => self.render_polyline(painter, color, stroke),
            DrawingToolType::DoubleCurve => self.render_double_curve(painter),

            // Fibonacci (10 tools)
            DrawingToolType::FibonacciRetracement => self.render_fibonacci(painter, rect),
            DrawingToolType::FibonacciExtension => self.render_fibonacci_extension(painter, rect),
            DrawingToolType::FibonacciSpeedResistanceArcs => self.render_fibonacci_arc(painter),
            DrawingToolType::FibonacciTimeZones => self.render_fibonacci_time_zones(painter, rect),
            DrawingToolType::FibonacciChannel => self.render_fibonacci_channel(painter, rect),
            DrawingToolType::FibonacciCircles => self.render_fibonacci_circles(painter),
            DrawingToolType::FibonacciSpeedFan => self.render_fibonacci_speed_fan(painter, rect),
            DrawingToolType::FibonacciSpiral => self.render_fibonacci_spiral(painter),
            DrawingToolType::FibonacciWedge => self.render_fibonacci_wedge(painter, rect),
            DrawingToolType::TrendBasedFibTime => self.render_trend_based_fib_time(painter, rect),

            // Gann (4 tools)
            DrawingToolType::GannFan => self.render_gann_fan(painter, rect),
            DrawingToolType::GannSquare => self.render_gann_square(painter),
            DrawingToolType::GannBox => self.render_gann_box(painter),
            DrawingToolType::GannFixed => self.render_gann_fixed(painter),

            // Pitchforks (4 tools)
            DrawingToolType::Pitchfork => self.render_pitchfork(painter, rect),
            DrawingToolType::SchiffPitchfork => self.render_pitchfork(painter, rect),
            DrawingToolType::ModifiedSchiffPitchfork => {
                self.render_modified_schiff_pitchfork(painter, rect)
            }
            DrawingToolType::InsidePitchfork => self.render_inside_pitchfork(painter, rect),

            // Channels (4 tools)
            DrawingToolType::ParallelChannel => self.render_parallel_channel(painter, rect),
            DrawingToolType::RegressionTrend => self.render_regression_trend(painter, view),
            DrawingToolType::FlatTopBottom => self.render_flat_top_bottom(painter, view),
            DrawingToolType::DisjointChannel => self.render_disjoint_channel(painter, view),

            // Patterns (6 tools)
            DrawingToolType::XABCDPattern => self.render_xabcd_pattern(painter),
            DrawingToolType::CypherPattern => self.render_cypher_pattern(painter),
            DrawingToolType::HeadAndShoulders => self.render_head_and_shoulders(painter),
            DrawingToolType::ABCDPattern => self.render_abcd_pattern(painter),
            DrawingToolType::TrianglePattern => self.render_triangle_pattern(painter),
            DrawingToolType::ThreeDrivesPattern => self.render_three_drives_pattern(painter),

            // Elliott Wave (5 tools)
            DrawingToolType::ElliottImpulse => self.render_elliott_impulse(painter),
            DrawingToolType::ElliottCorrection => self.render_elliott_correction(painter),
            DrawingToolType::ElliottTriangle => self.render_elliott_triangle(painter),
            DrawingToolType::ElliottDoubleCombo => self.render_elliott_double_combo(painter),
            DrawingToolType::ElliottTripleCombo => self.render_elliott_triple_combo(painter),

            // Cycles (3 tools)
            DrawingToolType::CyclicLines => self.render_cyclic_lines(painter, rect),
            DrawingToolType::TimeCycles => self.render_time_cycles(painter, rect),
            DrawingToolType::SineLine => self.render_sine_line(painter, rect),

            // Projection (6 tools)
            DrawingToolType::LongPos => self.render_pos_tool(painter, view, true),
            DrawingToolType::ShortPos => self.render_pos_tool(painter, view, false),
            DrawingToolType::Forecast => self.render_forecast(painter, view),
            DrawingToolType::BarsPattern => self.render_bars_pattern(painter, view),
            DrawingToolType::GhostFeed => self.render_ghost_feed(painter),
            DrawingToolType::ProjectionTool => self.render_projection_tool(painter, rect),

            // Volume (3 tools)
            DrawingToolType::AnchoredVWAP => self.render_anchored_vwap(painter, view),
            DrawingToolType::FixedRangeVolumeProfile => {
                self.render_fixed_range_volume_profile(painter, view)
            }
            DrawingToolType::AnchoredVolumeProfile => {
                self.render_anchored_volume_profile(painter, view)
            }

            // Measurers (4 tools)
            DrawingToolType::Measure => self.render_measure(painter, view),
            DrawingToolType::PriceRange => self.render_price_range(painter, rect, view),
            DrawingToolType::DateRange => self.render_date_range(painter, rect, view),
            DrawingToolType::DateAndPriceRange => self.render_date_and_price_range(painter, view),

            // Annotations (7 tools)
            DrawingToolType::TextLabel => self.render_text_label(painter),
            DrawingToolType::AnchoredText => self.render_anchored_text(painter),
            DrawingToolType::Note => self.render_note(painter),
            DrawingToolType::AnchoredNote => self.render_anchored_note(painter),
            DrawingToolType::Callout => self.render_callout(painter),
            DrawingToolType::PriceLabel => self.render_price_label(painter),
            DrawingToolType::FlagNote => self.render_flag_note(painter),

            // Brushes (2 tools)
            DrawingToolType::Brush => self.render_brush(painter),
            DrawingToolType::Highlighter => self.render_highlighter(painter),

            // Arrows (4 tools)
            DrawingToolType::Arrow => self.render_arrow(painter, color, stroke),
            DrawingToolType::ArrowMarker => self.render_arrow_marker(painter, color),
            DrawingToolType::ArrowMarkUp => self.render_arrow_mark_up(painter),
            DrawingToolType::ArrowMarkDown => self.render_arrow_mark_down(painter),

            // Additional tools
            DrawingToolType::Pitchfan => self.render_pitchfan(painter, rect),
            DrawingToolType::Path => self.render_path(painter),
            DrawingToolType::Curve => self.render_curve(painter),
            DrawingToolType::PriceNote => self.render_price_note(painter, rect),
            DrawingToolType::Table => self.render_table(painter),
            DrawingToolType::Comment => self.render_comment(painter),
            DrawingToolType::Signpost => self.render_signpost(painter, rect),
            DrawingToolType::Image => self.render_image_placeholder(painter),
            DrawingToolType::Tweet => self.render_tweet_placeholder(painter),
            DrawingToolType::Idea => self.render_idea_placeholder(painter),
            DrawingToolType::FontIcon => self.render_font_icon(painter),

            // Cursor tools (4 - not rendered as drawings)
            DrawingToolType::CrossCursor
            | DrawingToolType::DotCursor
            | DrawingToolType::ArrowCursor
            | DrawingToolType::Eraser => {}
        }
    }

    /// Render this drawing using the modular render system
    pub fn render(&self, painter: &egui::Painter, rect: Rect, view: &ChartView) {
        self.render_original(painter, rect, view);
    }
}

/// Bars the data-driven drawing tests share; their reference values were
/// computed independently from these OHLCV rows. The last bar has no range.
#[cfg(test)]
pub(crate) fn sample_bars() -> Vec<Bar> {
    [
        (10.0, 12.0, 9.0, 11.0, 100.0),
        (11.0, 13.0, 10.0, 12.0, 200.0),
        (12.0, 12.5, 11.0, 11.0, 150.0),
        (11.5, 14.0, 11.0, 13.5, 300.0),
        (13.5, 15.0, 13.0, 14.0, 250.0),
        (14.0, 14.5, 12.0, 12.5, 400.0),
        (13.0, 13.0, 13.0, 13.0, 50.0),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (o, h, l, c, v))| {
        let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
        Bar::new(time, o, h, l, c, v)
    })
    .collect()
}
