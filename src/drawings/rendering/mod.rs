//! Painting drawings against bars and a coordinate mapping.
//!
//! [`ChartView`] supplies data and mappings to [`Drawing::render`]; [`traits`]
//! provides renderer contexts, handle geometry and hit-testing helpers for custom
//! renderers. A chart invokes drawing rendering automatically.

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
mod style;
mod trading;
pub mod traits;
// The painter the drawing renderers share with the chart.
pub(crate) mod utils {
    pub(crate) use crate::painting::dashed_line::draw_dashed_line;
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
    /// [`ChartState::revision`](crate::studies::ChartState::revision) of `bars`.
    /// Results computed from the bars are cached under it, so it must change
    /// whenever a bar before the last one changes.
    pub revision: u64,
    /// Screen x of a (fractional) bar index.
    pub bar_to_x: &'a dyn Fn(f32) -> f32,
    /// Screen y of a price.
    pub price_to_y: &'a dyn Fn(f64) -> f32,
    /// Whether the bar times are times, so measurements show the time between anchors; `false`
    /// on a [horizontal scale](crate::scales::HorizontalScale) other than time.
    pub time_axis: bool,
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
            | T::ArrowMarker
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
            T::Sector => {
                let points = self.sector_points()?;
                outline.lines = edges(&points);
                if filled {
                    outline.areas.extend(
                        points[1..]
                            .windows(2)
                            .map(|edge| vec![points[0], edge[0], edge[1]]),
                    );
                }
            }
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
                let corners = self.rotated_rect_corners()?;
                outline.lines = edges(&corners);
                if filled {
                    outline.areas.push(corners.to_vec());
                }
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
                outline.lines = self.arc_path().windows(2).map(|w| [w[0], w[1]]).collect();
            }
            T::Curve => {
                let [p0, p1, p2] = self.curve_controls()?;
                outline.lines = shapes::curve_points(p0, p1, p2)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::DoubleCurve => {
                outline.lines = self
                    .double_curve_points()?
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::Pitchfork | T::SchiffPitchfork | T::ModifiedSchiffPitchfork | T::InsidePitchfork => {
                outline.lines = self.pitchfork_lines(rect)?;
                outline.areas = self.pitchfork_bands(rect);
            }
            T::Pitchfan => {
                outline.lines = pitchfork::pitchfan_lines(first, second, *p.get(2)?, rect)
            }
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
            T::FibonacciWedge => return self.fibonacci_wedge_outline(),
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

        let painter = &painter.with_clip_rect(rect);
        let start = painter.ctx().graphics(|g| {
            g.get(painter.layer_id())
                .map_or(0, |list| list.next_idx().0)
        });
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
            DrawingToolType::ThreeDrivesPattern => self.render_three_drives_pattern(painter, view),

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
            DrawingToolType::ProjectionTool => self.render_projection_tool(painter, rect, view),

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

            // Arrows (6 tools)
            DrawingToolType::Arrow => self.render_arrow(painter, color, stroke),
            DrawingToolType::ArrowMarker => self.render_arrow_marker(painter, color),
            DrawingToolType::ArrowMarkUp => self.render_arrow_mark_up(painter),
            DrawingToolType::ArrowMarkDown => self.render_arrow_mark_down(painter),
            DrawingToolType::ArrowMarkLeft => self.render_horizontal_mark(painter, -1.0),
            DrawingToolType::ArrowMarkRight => self.render_horizontal_mark(painter, 1.0),

            // Additional tools
            DrawingToolType::Pitchfan => self.render_pitchfan(painter, rect),
            DrawingToolType::Sector => self.render_sector(painter),
            DrawingToolType::Path => self.render_path(painter),
            DrawingToolType::Curve => self.render_curve(painter),
            DrawingToolType::PriceNote => self.render_price_note(painter, rect),
            DrawingToolType::Table => self.render_table(painter),
            DrawingToolType::Comment => self.render_comment(painter),
            DrawingToolType::Signpost => self.render_signpost(painter, rect),
            DrawingToolType::Image => self.render_image(painter),
            DrawingToolType::Tweet => self.render_post(painter),
            DrawingToolType::Idea => self.render_idea(painter),
            DrawingToolType::FontIcon | DrawingToolType::Sticker => self.render_font_icon(painter),

            // Cursor tools (4 - not rendered as drawings)
            DrawingToolType::CrossCursor
            | DrawingToolType::DotCursor
            | DrawingToolType::ArrowCursor
            | DrawingToolType::Eraser => {}
        }
        self.finish_style(painter, start, rect);
    }

    /// Render this drawing using the modular render system
    pub fn render(&self, painter: &egui::Painter, rect: Rect, view: &ChartView) {
        self.render_original(painter, rect, view);
    }

    /// The screen area this drawing paints in, for tools that paint within three times the
    /// diagonal of their anchors plus [`PAINT_MARGIN`] around them, so a drawing outside the pane
    /// can be skipped. Tools that also paint at prices taken from the bars, such as positions and
    /// volume profiles, or run a leg on a fixed time past its anchor, which a steep leg carries to
    /// any price, are bounded in time only. `None` for tools that can reach the pane's edges, as
    /// lines extended left or right do.
    pub(crate) fn paint_bounds(&self) -> Option<Rect> {
        use DrawingToolType as T;
        let bounded = matches!(
            self.tool_type,
            T::TrendLine
                | T::InfoLine
                | T::TrendAngle
                | T::Rect
                | T::RotatedRect
                | T::Circle
                | T::Ellipse
                | T::Triangle
                | T::Arc
                | T::Polyline
                | T::DoubleCurve
                | T::Path
                | T::Curve
                | T::Brush
                | T::Highlighter
                | T::Sector
                | T::FibonacciSpeedResistanceArcs
                | T::FibonacciCircles
                | T::GannSquare
                | T::GannBox
                | T::GannFixed
                | T::XABCDPattern
                | T::CypherPattern
                | T::HeadAndShoulders
                | T::ABCDPattern
                | T::TrianglePattern
                | T::ElliottImpulse
                | T::ElliottCorrection
                | T::ElliottTriangle
                | T::ElliottDoubleCombo
                | T::ElliottTripleCombo
                | T::LongPos
                | T::ShortPos
                | T::Forecast
                | T::BarsPattern
                | T::GhostFeed
                | T::FixedRangeVolumeProfile
                | T::Measure
                | T::DateAndPriceRange
                | T::Arrow
                | T::ArrowMarker
                | T::ArrowMarkUp
                | T::ArrowMarkDown
                | T::ArrowMarkLeft
                | T::ArrowMarkRight
                | T::FontIcon
                | T::Sticker
                | T::Image
        );
        if !bounded || self.extend_left || self.extend_right {
            return None;
        }
        let hull = self.bounding_rect()?;
        let reach = 3.0 * hull.size().length() + PAINT_MARGIN;
        let bounds = hull.expand(reach);
        let time_only = matches!(
            self.tool_type,
            T::LongPos
                | T::ShortPos
                | T::Forecast
                | T::BarsPattern
                | T::FixedRangeVolumeProfile
                | T::HeadAndShoulders
                | T::TrianglePattern
                | T::ElliottTriangle
        );
        Some(if time_only {
            Rect::from_x_y_ranges(bounds.x_range(), egui::Rangef::EVERYTHING)
        } else {
            bounds
        })
    }
}

/// Room beyond the anchors for the labels, markers and fixed-size extensions of a drawing with
/// [`Drawing::paint_bounds`].
const PAINT_MARGIN: f32 = 200.0;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingManager;
    use egui::{Context, Pos2, RawInput, Shape};

    /// A non-default style reaches geometry in every renderer family, while fills keep
    /// their configured colour. Dashes in a 200 px horizontal edge cannot be one solid edge.
    #[test]
    fn common_stroke_and_fill_styles_reach_tool_families() {
        use crate::model::LineStyle;
        for tool in [
            DrawingToolType::Rect,
            DrawingToolType::Circle,
            DrawingToolType::Triangle,
            DrawingToolType::Curve,
            DrawingToolType::GannFan,
            DrawingToolType::CyclicLines,
            DrawingToolType::XABCDPattern,
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = [
                (100., 100.),
                (300., 300.),
                (200., 250.),
                (350., 150.),
                (400., 200.),
            ]
            .map(|(x, y)| Pos2::new(x, y))
            .to_vec();
            drawing.chart_points = drawing
                .points
                .iter()
                .map(|p| crate::drawings::ChartPoint::new(p.x / 10.0, 500.0 - p.y as f64))
                .collect();
            drawing.line_style = LineStyle::Dotted;
            drawing.stroke_width = 5.0;
            drawing.fill_color = Some([12, 34, 56, 80]);
            let shapes = paint(&drawing);
            let mut lines = Vec::new();
            fn collect<'a>(
                shapes: &'a [Shape],
                lines: &mut Vec<(&'a [Pos2; 2], &'a egui::Stroke)>,
            ) {
                for s in shapes {
                    match s {
                        Shape::Vec(v) => collect(v, lines),
                        Shape::LineSegment { points, stroke } => lines.push((points, stroke)),
                        _ => {}
                    }
                }
            }
            collect(&shapes, &mut lines);
            assert!(
                lines
                    .iter()
                    .any(|(p, s)| p[0].distance(p[1]) <= 3.0 && s.width == 5.0),
                "{tool:?} has no styled dash"
            );
            assert!(
                !lines.iter().any(|(p, _)| p[0].distance(p[1]) > 10.0),
                "{tool:?} retained a solid line"
            );
            assert!(
                !lines
                    .iter()
                    .any(|(_, s)| s.color == egui::Color32::WHITE && s.width > 2.0),
                "{tool:?} restyled anchor decorations"
            );
            if matches!(
                tool,
                DrawingToolType::Rect
                    | DrawingToolType::Circle
                    | DrawingToolType::Triangle
                    | DrawingToolType::CyclicLines
                    | DrawingToolType::XABCDPattern
            ) {
                fn has_fill(shapes: &[Shape], color: egui::Color32) -> bool {
                    shapes.iter().any(|s| match s {
                        Shape::Vec(v) => has_fill(v, color),
                        Shape::Path(p) => p.fill == color,
                        Shape::Rect(r) => r.fill == color,
                        Shape::Circle(c) => c.fill == color,
                        _ => false,
                    })
                }
                assert!(
                    has_fill(&shapes, drawing.fill_color32().unwrap()),
                    "{tool:?} ignored fill"
                );
            }
        }
    }

    /// Guards culling: a drawing with paint bounds paints nothing outside them, for spread,
    /// near-vertical and tiny anchors, so skipping it when the bounds miss the pane hides nothing
    /// in view. A tool added to the bounded list whose paint reaches further, such as a line to a
    /// pane edge, fails it.
    #[test]
    fn drawings_paint_within_their_bounds() {
        let layouts: [&[(f32, f32)]; 3] = [
            &[
                (100., 100.),
                (300., 300.),
                (200., 250.),
                (350., 150.),
                (400., 200.),
                (250., 120.),
                (450., 280.),
                (150., 180.),
            ],
            &[
                (200., 100.),
                (200.5, 300.),
                (201., 150.),
                (201.5, 280.),
                (202., 120.),
                (202.5, 260.),
                (203., 140.),
                (203.5, 300.),
            ],
            &[
                (200., 200.),
                (201., 202.),
                (202., 199.),
                (203., 201.),
                (204., 200.),
                (205., 202.),
                (206., 198.),
                (207., 201.),
            ],
        ];
        let bars = sample_bars();
        // Larger than all bounds, so paint that reaches a pane edge lands outside them.
        let pane = Rect::from_min_max(Pos2::new(-2000., -2000.), Pos2::new(2500., 2500.));
        let mut wrong = Vec::new();
        for &tool in DrawingToolType::all() {
            for layout in layouts {
                let mut drawing = Drawing::new(7, tool);
                let count = tool.required_points().unwrap_or(layout.len());
                drawing.points = layout
                    .iter()
                    .take(count)
                    .map(|&(x, y)| Pos2::new(x, y))
                    .collect();
                drawing.chart_points = (drawing.points.iter())
                    .map(|p| crate::drawings::ChartPoint::new(p.x / 40.0, 500.0 - p.y as f64))
                    .collect();
                drawing.text = Some("Sample".into());
                let Some(bounds) = drawing.paint_bounds() else {
                    continue;
                };
                let ctx = Context::default();
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    let view = ChartView {
                        instrument: None,
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|b| b * 40.0,
                        price_to_y: &|p| 500.0 - p as f32,
                        time_axis: true,
                    };
                    drawing.render(ui.painter(), pane, &view);
                });
                for shape in &output.shapes {
                    let paint = shape.shape.visual_bounding_rect();
                    if !paint.is_negative() && !bounds.expand(0.5).contains_rect(paint) {
                        wrong.push(format!("{tool:?} paints {paint:?} outside {bounds:?}"));
                    }
                }
                output.drop_without_applying_deltas();
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Text size changes the laid-out box; bold adds weight without requiring host fonts.
    #[test]
    fn common_annotation_fonts_are_laid_out_at_the_configured_size() {
        use crate::drawings::FontWeight;
        for tool in [
            DrawingToolType::TextLabel,
            DrawingToolType::AnchoredText,
            DrawingToolType::Note,
            DrawingToolType::AnchoredNote,
            DrawingToolType::Callout,
            DrawingToolType::PriceLabel,
            DrawingToolType::FlagNote,
            DrawingToolType::PriceNote,
            DrawingToolType::Comment,
            DrawingToolType::Signpost,
            DrawingToolType::Table,
            DrawingToolType::FontIcon,
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = vec![Pos2::new(50.0, 100.0), Pos2::new(150.0, 150.0)];
            drawing.text = Some("Sample".into());
            drawing.font_size = 48.0;
            drawing.font_weight = FontWeight::Bold;
            let shapes = paint(&drawing);
            let mut texts = Vec::new();
            fn collect<'a>(shapes: &'a [Shape], texts: &mut Vec<&'a egui::epaint::TextShape>) {
                for s in shapes {
                    match s {
                        Shape::Vec(v) => collect(v, texts),
                        Shape::Text(t) => texts.push(t),
                        _ => {}
                    }
                }
            }
            collect(&shapes, &mut texts);
            if matches!(tool, DrawingToolType::Table | DrawingToolType::Note) {
                let table = shapes
                    .iter()
                    .find_map(|s| match s {
                        Shape::Rect(r) => Some(r.rect),
                        _ => None,
                    })
                    .unwrap();
                assert!(
                    texts
                        .iter()
                        .filter(|t| tool == DrawingToolType::Table || t.galley.job.text == "N")
                        .all(|t| table.contains_rect(t.visual_bounding_rect()))
                );
            }
            assert!(
                texts.iter().any(|t| t
                    .galley
                    .job
                    .sections
                    .iter()
                    .any(|s| s.format.font_id.size == 48.0)),
                "{tool:?} ignored font size"
            );
            let bold_text: Vec<_> = texts.iter().map(|t| (*t).clone()).collect();
            drawing.font_weight = FontWeight::Normal;
            let normal = paint(&drawing);
            let mut normal_text = Vec::new();
            collect(&normal, &mut normal_text);
            assert_ne!(
                bold_text,
                normal_text.into_iter().cloned().collect::<Vec<_>>(),
                "{tool:?} ignored font weight"
            );
        }
    }

    /// Endpoint symbols retain the clicked direction even when the anchors run right to left.
    #[test]
    fn common_endpoint_markers_follow_styles_and_anchor_direction() {
        use crate::drawings::ArrowStyle;
        for &style in ArrowStyle::all() {
            let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
            drawing.points = vec![Pos2::new(300., 100.), Pos2::new(100., 100.)];
            drawing.arrow_start_style = ArrowStyle::Circle;
            drawing.arrow_end_style = style;
            let shapes = paint(&drawing);
            assert!(
                shapes
                    .iter()
                    .any(|s| matches!(s,Shape::Circle(c) if c.center == drawing.points[0])),
                "start circle missing"
            );
            let tip = drawing.points[1];
            let present = match style {
                ArrowStyle::None => !shapes.iter().any(|s| matches!(s,Shape::Path(_))),
                ArrowStyle::Arrow => shapes.iter().any(|s| matches!(s,Shape::Path(p) if p.points == [tip, Pos2::new(108.,96.), Pos2::new(108.,104.)])),
                ArrowStyle::OpenArrow => shapes.iter().any(|s| matches!(s,Shape::Path(p) if p.points.get(1) == Some(&tip) && p.fill == egui::Color32::TRANSPARENT)),
                ArrowStyle::Circle => shapes.iter().any(|s| matches!(s,Shape::Circle(c) if c.center == tip)),
                ArrowStyle::Square => shapes.iter().any(|s| matches!(s,Shape::Rect(r) if r.rect.center() == tip)),
                ArrowStyle::Diamond => shapes.iter().any(|s| matches!(s,Shape::Path(p) if p.points.len() == 4 && p.points[0].x < tip.x && p.points[2].x > tip.x)),
            };
            assert!(present, "{style:?}");
        }
        let mut path = Drawing::new(7, DrawingToolType::Path);
        path.points = vec![
            Pos2::new(100., 100.),
            Pos2::new(200., 100.),
            Pos2::new(200., 200.),
        ];
        assert!(
            !paint(&path)
                .iter()
                .any(|s| matches!(s, Shape::Circle(c) if c.center == path.points[2]))
        );
        // Final segment points down: the arrow base is eight pixels above the tip, four either side.
        assert!(paint(&path).iter().any(|s| matches!(s, Shape::Path(p) if p.points == [Pos2::new(200.,200.), Pos2::new(196.,192.), Pos2::new(204.,192.)])));
        for tool in [
            DrawingToolType::InfoLine,
            DrawingToolType::TrendAngle,
            DrawingToolType::Arrow,
            DrawingToolType::ArrowMarker,
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = vec![Pos2::new(100., 100.), Pos2::new(300., 100.)];
            drawing.line_style = crate::model::LineStyle::Dashed;
            drawing.extend_left = true;
            drawing.extend_right = true;
            fn extent(shapes: &[Shape]) -> (f32, f32) {
                shapes
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), s| {
                        let (a, b) = match s {
                            Shape::Vec(v) => extent(v),
                            Shape::LineSegment { points, .. } => {
                                (points[0].x.min(points[1].x), points[0].x.max(points[1].x))
                            }
                            _ => (min, max),
                        };
                        (min.min(a), max.max(b))
                    })
            }
            let (min, max) = extent(&paint(&drawing));
            assert!(
                min <= 0.0 && max >= 499.0,
                "{tool:?}: extended dashed line spans {min}..{max}"
            );
        }
        for tool in [DrawingToolType::TrendLine, DrawingToolType::ExtendedLine] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = vec![Pos2::new(100., 300.), Pos2::new(100., 100.)];
            drawing.arrow_start_style = ArrowStyle::Circle;
            drawing.arrow_end_style = ArrowStyle::Square;
            let shapes = paint(&drawing);
            let (start, end) = if tool == DrawingToolType::ExtendedLine {
                (Pos2::new(100., 400.), Pos2::new(100., 0.))
            } else {
                (drawing.points[0], drawing.points[1])
            };
            assert!(
                shapes
                    .iter()
                    .any(|s| matches!(s,Shape::Circle(c) if c.center == start)),
                "{tool:?}: vertical start"
            );
            assert!(
                shapes
                    .iter()
                    .any(|s| matches!(s,Shape::Rect(r) if r.rect.center() == end)),
                "{tool:?}: vertical end"
            );
        }
    }

    /// These anchors form a semicircle of radius sqrt(5000), length 222.144 px.
    /// An 8 px dash / 4 px gap paints about two thirds, plus at most one dash.
    #[test]
    fn dash_spacing_continues_across_segmented_curves() {
        let mut drawing = Drawing::new(7, DrawingToolType::Arc);
        drawing.points = vec![
            Pos2::new(100., 100.),
            Pos2::new(200., 200.),
            Pos2::new(200., 100.),
        ];
        drawing.line_style = crate::model::LineStyle::Dashed;
        fn length(shapes: &[Shape]) -> f32 {
            shapes
                .iter()
                .map(|s| match s {
                    Shape::Vec(v) => length(v),
                    Shape::LineSegment { points, .. } => points[0].distance(points[1]),
                    _ => 0.0,
                })
                .sum()
        }
        let painted = length(&paint(&drawing));
        assert!(
            painted > 140.0 && painted < 157.0,
            "painted {painted} px of a 222.144 px arc"
        );
    }

    /// Bundled glyphs must use vector primitives even when the host has no emoji font.
    #[test]
    fn bundled_glyphs_use_vectors_at_their_independent_size() {
        for id in [
            "icon:star",
            "icon:target",
            "sticker:bull",
            "sticker:bear",
            "sticker:rocket",
            "sticker:idea",
        ] {
            for size in [24.0, 48.0] {
                for width in [2.0, 6.0] {
                    let mut drawing = Drawing::new(7, DrawingToolType::FontIcon);
                    drawing.text = Some(id.into());
                    drawing.font_size = size;
                    drawing.stroke_width = width;
                    drawing.points = vec![Pos2::new(100., 100.)];
                    let shapes = paint(&drawing);
                    assert!(!shapes.is_empty());
                    assert!(
                        !shapes.iter().any(|s| matches!(s, Shape::Text(_))),
                        "{id} used a host font"
                    );
                    let bounds = Rect::from_center_size(drawing.points[0], egui::Vec2::splat(size))
                        .expand(width);
                    let painted = shapes.iter().fold(Rect::NOTHING, |bounds, shape| {
                        bounds.union(shape.visual_bounding_rect())
                    });
                    assert!(
                        painted.width() > size * 0.6 && painted.height() > size * 0.6,
                        "{id}: {painted:?} at {size}"
                    );
                    for shape in &shapes {
                        assert!(
                            bounds.contains_rect(shape.visual_bounding_rect()),
                            "{id}: {shape:?}"
                        );
                    }
                }
            }
        }
    }

    fn paint(drawing: &Drawing) -> Vec<Shape> {
        let ctx = Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            let view = ChartView {
                instrument: None,
                bars: &[],
                revision: 0,
                bar_to_x: &|b| b * 10.0,
                price_to_y: &|p| 500.0 - p as f32,
                time_axis: true,
            };
            drawing.render(
                ui.painter(),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(500., 400.)),
                &view,
            );
        });
        let shapes = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    /// A pattern's projected target and selection handles can cross the pane edge;
    /// their paint commands retain the pane's clip, intersected with the caller's clip.
    #[test]
    fn drawings_and_handles_keep_the_pane_clip() {
        let ctx = Context::default();
        let pane = Rect::from_min_max(Pos2::new(30.0, 30.0), Pos2::new(300.0, 250.0));
        let parent = Rect::from_min_max(Pos2::new(50.0, 10.0), Pos2::new(400.0, 400.0));
        let mut manager = DrawingManager::new();
        let mut pattern = Drawing::new(7, DrawingToolType::HeadAndShoulders);
        pattern.points = [
            (40., 200.),
            (80., 150.),
            (120., 200.),
            (160., 100.),
            (200., 200.),
            (240., 150.),
            (280., 260.),
        ]
        .map(|(x, y)| Pos2::new(x, y))
        .to_vec();
        manager.drawings.push(pattern);
        manager.select(7);
        let output = ctx.run_ui(RawInput::default(), |ui| {
            let view = ChartView {
                instrument: None,
                bars: &[],
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y: &|p| p as f32,
                time_axis: true,
            };
            manager.render_all(&ui.painter().with_clip_rect(parent), pane, &view);
        });
        let shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
        let painted: Vec<_> = shapes
            .iter()
            .filter(|s| !matches!(s.shape, Shape::Noop))
            .collect();
        assert!(!painted.is_empty());
        assert!(
            painted
                .iter()
                .all(|s| s.clip_rect == pane.intersect(parent)),
            "drawing or handle escaped its pane"
        );
    }
}
