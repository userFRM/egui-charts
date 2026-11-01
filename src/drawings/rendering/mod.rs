//! Painting drawings against bars and a coordinate mapping.
//!
//! [`ChartView`] supplies data and mappings to [`Drawing::render`]; [`traits`]
//! provides handle drawing and hit-testing helpers for custom
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
mod outline;
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

pub(crate) use outline::Outline;
pub use traits::DrawingRenderer;

use crate::drawings::domain::{Drawing, DrawingToolType};
use crate::model::{Bar, SpanCache};
use egui::Rect;
use std::rc::Rc;

/// The bars and coordinate mapping that drawings are rendered against.
///
/// Drawing anchors index into `bars`. The anchored VWAP, both volume profiles,
/// the regression trend, the position and forecast results and the bars
/// pattern are computed from these bars.
pub struct ChartView<'a> {
    /// Instrument metadata supplied by the host or feed, for tick and pip measurements.
    pub instrument: Option<&'a crate::model::Instrument>,
    /// Price label formatter supplied by the chart; `None` uses instrument precision.
    pub price_formatter: Option<&'a dyn crate::scales::PriceFormatter>,
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
    /// Price at a screen y, using the same scale and orientation as `price_to_y`.
    pub y_to_price: &'a dyn Fn(f32) -> f64,
    /// Whether the bar times are times, so measurements show the time between anchors; `false`
    /// on a [horizontal scale](crate::scales::HorizontalScale) other than time.
    pub time_axis: bool,
    /// The executions behind `bars`, aggregated on the bars' clock. The volume profile drawings
    /// then show the executed volume at each price, split by aggressor side, instead of
    /// spreading bar volume over bar ranges; `None` estimates from the bars.
    pub executions: Option<&'a crate::data::Footprint>,
    /// Interval of `bars`, or `None` for price-based chart types, trade-based intervals and
    /// horizontal scales other than time. Volume profiles read finer bars only with an interval.
    pub timeframe: Option<crate::model::Timeframe>,
    /// Histories of this symbol at the intervals that
    /// [`Chart::study_data_requests`](crate::Chart::study_data_requests) lists for its drawings.
    /// Without executions in their span, profile drawings use the interval
    /// [`Instrument::profile_resolutions`](crate::model::Instrument::profile_resolutions)
    /// suggests, replacing covered chart bars with finer bars; `&[]` estimates from `bars`.
    pub finer: &'a [crate::data::StudyData<'a>],
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

/// Results kept per kind of drawing, more than the drawings of one kind on screen.
pub(crate) const MAX_CACHED_SPANS: usize = 32;

impl Drawing {
    /// Renders the drawing as unselected.
    pub fn render_original(&self, painter: &egui::Painter, rect: Rect, view: &ChartView) {
        self.render_with_selection(painter, rect, view, false);
    }

    /// Main rendering dispatcher - routes to category-specific render methods
    ///
    /// This method handles rendering for every drawing tool type,
    /// delegating to the appropriate category-specific implementation. `selected`
    /// lets a tool mark its selection, as a position's statistics block does.
    pub(crate) fn render_with_selection(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        view: &ChartView,
        selected: bool,
    ) {
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
            DrawingToolType::Ray | DrawingToolType::ExtendedLine => {
                self.render_trend_line(painter, rect, color, stroke)
            }
            DrawingToolType::HorizontalLine => self.render_horizontal_line(painter, rect, stroke),
            DrawingToolType::HorizontalRay => self.render_horizontal_ray(painter, rect, stroke),
            DrawingToolType::VerticalLine => self.render_vertical_line(painter, rect, stroke),
            DrawingToolType::CrossLine => self.render_cross_line(painter, rect, stroke),
            DrawingToolType::TrendAngle => {
                return self.render_trend_angle(painter, rect, color, stroke);
            }
            DrawingToolType::InfoLine => self.render_info_line(painter, view),

            // Shapes (8 tools)
            DrawingToolType::Rect => self.render_rect(painter, rect, color, stroke),
            DrawingToolType::RotatedRect => self.render_rotated_rect(painter),
            DrawingToolType::Circle => self.render_circle(painter, color, stroke),
            DrawingToolType::Ellipse => self.render_ellipse(painter, color, stroke),
            DrawingToolType::Triangle => self.render_triangle(painter, color, stroke),
            DrawingToolType::Arc => self.render_arc(painter, color, stroke),
            DrawingToolType::Polyline => self.render_polyline(painter, color, stroke),
            DrawingToolType::DoubleCurve => self.render_double_curve(painter),

            // Fibonacci (10 tools)
            DrawingToolType::FibonacciRetracement | DrawingToolType::FibonacciExtension => {
                self.render_fibonacci(painter, rect, view)
            }
            DrawingToolType::FibonacciSpeedResistanceArcs => self.render_fibonacci_arc(painter),
            DrawingToolType::FibonacciTimeZones => self.render_fibonacci_time_zones(painter, rect),
            DrawingToolType::FibonacciChannel => self.render_fibonacci_channel(painter, rect, view),
            DrawingToolType::FibonacciCircles => self.render_fibonacci_circles(painter),
            DrawingToolType::FibonacciSpeedFan => self.render_fibonacci_speed_fan(painter, rect),
            DrawingToolType::FibonacciSpiral => self.render_fibonacci_spiral(painter, rect),
            DrawingToolType::FibonacciWedge => self.render_fibonacci_wedge(painter, rect),
            DrawingToolType::TrendBasedFibTime => self.render_fibonacci_time_zones(painter, rect),

            // Gann (4 tools)
            DrawingToolType::GannFan => self.render_gann_fan(painter, rect),
            DrawingToolType::GannSquare | DrawingToolType::GannFixed => {
                self.render_gann_square(painter, view)
            }
            DrawingToolType::GannBox => self.render_gann_box(painter),

            // Pitchforks (4 tools)
            DrawingToolType::Pitchfork => self.render_pitchfork(painter, rect),
            DrawingToolType::SchiffPitchfork => self.render_pitchfork(painter, rect),
            DrawingToolType::ModifiedSchiffPitchfork => {
                self.render_modified_schiff_pitchfork(painter, rect)
            }
            DrawingToolType::InsidePitchfork => self.render_inside_pitchfork(painter, rect),

            // Channels (4 tools)
            DrawingToolType::ParallelChannel
            | DrawingToolType::FlatTopBottom
            | DrawingToolType::DisjointChannel => self.render_channel(painter, rect),
            DrawingToolType::RegressionTrend => self.render_regression_trend(painter, view),

            // Patterns (6 tools)
            DrawingToolType::XABCDPattern => self.render_xabcd_pattern(painter),
            DrawingToolType::CypherPattern => self.render_cypher_pattern(painter),
            DrawingToolType::HeadAndShoulders => self.render_head_and_shoulders(painter),
            DrawingToolType::ABCDPattern => self.render_abcd_pattern(painter),
            DrawingToolType::TrianglePattern => self.render_triangle_pattern(painter),
            DrawingToolType::ThreeDrivesPattern => self.render_three_drives_pattern(painter),

            // Elliott Wave (5 tools)
            DrawingToolType::ElliottImpulse
            | DrawingToolType::ElliottCorrection
            | DrawingToolType::ElliottTriangle
            | DrawingToolType::ElliottDoubleCombo
            | DrawingToolType::ElliottTripleCombo => self.render_elliott_waves(painter),

            // Cycles (3 tools)
            DrawingToolType::CyclicLines => self.render_cyclic_lines(painter, rect),
            DrawingToolType::TimeCycles => self.render_time_cycles(painter, rect),
            DrawingToolType::SineLine => self.render_sine_line(painter, rect),

            // Projection (6 tools)
            DrawingToolType::LongPos => self.render_pos_tool(painter, view, true, selected),
            DrawingToolType::ShortPos => self.render_pos_tool(painter, view, false, selected),
            DrawingToolType::Forecast => self.render_forecast(painter, view),
            DrawingToolType::BarsPattern => self.render_bars_pattern(painter, view),
            // Candle colours and the neutral path are independent of the common drawing style.
            DrawingToolType::GhostFeed => return self.render_ghost_feed(painter, view),
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
            DrawingToolType::PriceRange
            | DrawingToolType::DateRange
            | DrawingToolType::DateAndPriceRange => self.render_range(painter, rect, stroke, view),

            // Annotations (7 tools)
            DrawingToolType::TextLabel | DrawingToolType::AnchoredText => {
                self.render_text_label(painter)
            }
            DrawingToolType::Note => self.render_note(painter),
            DrawingToolType::AnchoredNote => self.render_anchored_note(painter, selected),
            DrawingToolType::Callout => self.render_callout(painter),
            DrawingToolType::PriceLabel => self.render_price_label(painter, view),
            DrawingToolType::FlagNote => self.render_flag_note(painter),

            // Brushes (2 tools)
            DrawingToolType::Brush | DrawingToolType::Highlighter => self.render_brush(painter),

            // Arrows (6 tools)
            DrawingToolType::Arrow => self.render_arrow(painter, color, stroke),
            DrawingToolType::ArrowMarker => self.render_arrow_marker(painter, color),
            DrawingToolType::ArrowMarkUp => self.render_arrow_mark(painter, egui::Vec2::UP),
            DrawingToolType::ArrowMarkDown => self.render_arrow_mark(painter, egui::Vec2::DOWN),
            DrawingToolType::ArrowMarkLeft => self.render_arrow_mark(painter, egui::Vec2::LEFT),
            DrawingToolType::ArrowMarkRight => self.render_arrow_mark(painter, egui::Vec2::RIGHT),

            // Additional tools
            DrawingToolType::Pitchfan => self.render_pitchfork(painter, rect),
            DrawingToolType::Sector => self.render_sector(painter),
            DrawingToolType::Path => self.render_path(painter),
            DrawingToolType::Curve => self.render_curve(painter),
            DrawingToolType::PriceNote => self.render_price_note(painter, view),
            DrawingToolType::Table => self.render_table(painter),
            DrawingToolType::Comment => self.render_comment(painter),
            DrawingToolType::Signpost => self.render_signpost(painter),
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
        if self.tool_type == DrawingToolType::Rect {
            self.render_rect_middle_line(painter, rect);
        }
    }

    /// Render this drawing using the modular render system
    pub fn render(&self, painter: &egui::Painter, rect: Rect, view: &ChartView) {
        self.render_original(painter, rect, view);
    }

    /// The screen area this drawing paints in, for tools that paint within three times the
    /// diagonal of their anchors plus [`PAINT_MARGIN`] and the room its text needs around them, so
    /// a drawing outside the pane can be skipped. Tools that also paint at prices taken from the
    /// bars, such as positions and volume profiles, or run a leg on a fixed time past its anchor,
    /// which a steep leg carries to any price, are bounded in time only. `None` for tools that can
    /// reach the pane's edges, as lines extended left or right do.
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
                | T::FixedRangeVolumeProfile
                | T::Measure
                | T::PriceRange
                | T::DateRange
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
        if matches!(
            self.tool_type,
            T::FibonacciCircles | T::FibonacciSpeedResistanceArcs
        ) {
            let &[start, end, ..] = self.points.as_slice() else {
                return None;
            };
            let ratio = self
                .fibonacci_config()
                .visible_levels()
                .map(|l| l.value)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .fold(0.0_f32, f32::max);
            let (center, size) = if self.tool_type == T::FibonacciCircles {
                (start.lerp(end, 0.5), (end - start).abs() * ratio)
            } else {
                (start, egui::Vec2::splat(start.distance(end) * ratio * 2.0))
            };
            return size.is_finite().then(|| {
                Rect::from_center_size(center, size)
                    .union(Rect::from_two_pos(start, end))
                    .expand(PAINT_MARGIN)
            });
        }
        if self.tool_type == T::GannFixed {
            let &[first, second, ..] = self.points.as_slice() else {
                return None;
            };
            // Five times the anchors' span can pass the reach below when the span is steep.
            let corner = self.gann_square_corner(first, second);
            return Some(Rect::from_two_pos(first, corner).expand(PAINT_MARGIN));
        }
        if self.tool_type == T::Image {
            return self.image_frame().map(|frame| frame.expand(PAINT_MARGIN));
        }
        let hull = self.bounding_rect()?;
        // Room for the drawing's text: one and a half ems per character covers the widest glyph
        // advance and the row of a line break.
        let text = self.text.as_deref().map_or(0.0, |t| {
            1.5 * crate::styles::typography::font_size(self.font_size) * t.chars().count() as f32
        });
        let reach = 3.0 * hull.size().length() + PAINT_MARGIN + text;
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
    use egui::{Color32, Context, Pos2, RawInput, Shape};

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
            DrawingToolType::TimeCycles,
            DrawingToolType::XABCDPattern,
            DrawingToolType::PriceRange,
            DrawingToolType::DateRange,
            DrawingToolType::DateAndPriceRange,
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
                    | DrawingToolType::TimeCycles
                    | DrawingToolType::XABCDPattern
                    | DrawingToolType::PriceRange
                    | DrawingToolType::DateRange
                    | DrawingToolType::DateAndPriceRange
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

    /// For the box (100, 100) to (300, 300), the arrows end at the second anchor's
    /// coordinate on each axis. Cards sit beyond that end in the tooltip colours.
    #[test]
    fn measurer_arrows_point_to_the_second_anchor_and_stats_sit_on_cards() {
        use crate::drawings::ChartPoint;
        use DrawingToolType as T;
        let chart = crate::theme::Theme::dark().chart;
        for tool in [
            T::Measure,
            T::PriceRange,
            T::DateRange,
            T::DateAndPriceRange,
            T::InfoLine,
        ] {
            for (first, second) in [
                ((300., 100.), (100., 300.)),
                ((100., 300.), (300., 100.)),
                ((100., 100.), (300., 300.)),
                ((300., 300.), (100., 100.)),
                ((100., 200.), (300., 200.)),
                ((200., 100.), (200., 300.)),
                ((200., 200.), (200., 200.)),
            ] {
                let mut drawing = Drawing::new(1, tool);
                drawing.points = [first, second].map(|(x, y)| Pos2::new(x, y)).to_vec();
                drawing.chart_points = drawing
                    .points
                    .iter()
                    .map(|p| ChartPoint::new(p.x / 10.0, 500.0 - f64::from(p.y)))
                    .collect();
                let shapes = paint(&drawing);
                let bounds = Rect::from_two_pos(drawing.points[0], drawing.points[1]);
                let card = shapes
                    .iter()
                    .find_map(|s| match s {
                        Shape::Rect(r) if r.fill == chart.bg_tooltip => Some(r.rect),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{tool:?}: no tooltip card"));
                assert!(
                    shapes.iter().any(|s| matches!(s, Shape::Text(t)
                    if t.galley.job.sections.iter().all(|s| s.format.color == chart.axis_text
                        && s.format.font_id.size == crate::styles::typography::SM_MD))),
                    "{tool:?}: card text"
                );
                if tool == T::InfoLine {
                    assert!(card.min.x >= bounds.center().x && card.min.y >= bounds.center().y);
                    assert_eq!(
                        shapes
                            .iter()
                            .filter(|s| matches!(s, Shape::LineSegment { .. }))
                            .count(),
                        1,
                        "info line has no connector"
                    );
                    continue;
                }
                if second.1 <= first.1 {
                    assert!(card.max.y < bounds.min.y, "{tool:?}: card above");
                } else {
                    assert!(card.min.y > bounds.max.y, "{tool:?}: card below");
                }
                let mut tips = Vec::new();
                if tool != T::DateRange && first.1 != second.1 {
                    tips.push(Pos2::new(bounds.center().x, second.1));
                }
                if tool != T::PriceRange && first.0 != second.0 {
                    tips.push(Pos2::new(second.0, bounds.center().y));
                }
                let heads: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Path(p) if p.closed && p.fill.a() > 0 => Some(p),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    heads.len(),
                    tips.len(),
                    "{tool:?}: heads for {first:?} to {second:?}"
                );
                for tip in tips {
                    let head = heads
                        .iter()
                        .find(|p| p.points[0] == tip)
                        .unwrap_or_else(|| panic!("{tool:?}: no arrow at {tip:?}"));
                    // Both base corners lie behind the tip along the drag direction.
                    let direction = if tip.y == bounds.center().y {
                        egui::vec2(second.0 - first.0, 0.0)
                    } else {
                        egui::vec2(0.0, second.1 - first.1)
                    };
                    assert!(
                        head.points[1..]
                            .iter()
                            .all(|p| (*p - tip).dot(direction) < 0.0)
                    );
                }
            }
        }
    }

    /// The settings the properties dialog offers for a tool are the ones its renderer uses: a
    /// fill colour reaches a filled area, extension moves geometry (each flag flipped from the
    /// tool's default, since an extended line starts extended), the text is painted, each line
    /// end style draws a marker and the middle line flag adds a line.
    /// Icons keep their glyph in the text field, which the glyph picker edits.
    #[test]
    fn tool_capabilities_match_their_renderers() {
        use crate::drawings::ArrowStyle;
        fn found(shapes: &[Shape], hit: &impl Fn(&Shape) -> bool) -> bool {
            shapes.iter().any(|s| match s {
                Shape::Vec(v) => found(v, hit),
                s => hit(s),
            })
        }
        for &tool in DrawingToolType::all() {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = [
                (100., 100.),
                (300., 300.),
                (200., 250.),
                (350., 150.),
                (400., 200.),
                (450., 180.),
                (500., 250.),
            ]
            .map(|(x, y)| Pos2::new(x, y))
            .to_vec();
            drawing.chart_points = drawing
                .points
                .iter()
                .map(|p| crate::drawings::ChartPoint::new(p.x / 10.0, 500.0 - p.y as f64))
                .collect();
            if tool == DrawingToolType::RegressionTrend {
                drawing.chart_points[0].bar_idx = 0.0;
                drawing.chart_points[1].bar_idx = 6.0;
            }
            let mut filled = drawing.clone();
            filled.fill_color = Some([12, 34, 56, 80]);
            let fill = filled.fill_color32().unwrap();
            let fill = found(&paint(&filled), &|s| match s {
                Shape::Path(p) => p.fill == fill,
                Shape::Rect(r) => r.fill == fill,
                Shape::Circle(c) => c.fill == fill,
                Shape::Mesh(m) => m.vertices.iter().any(|v| v.color == fill),
                _ => false,
            });
            assert_eq!(tool.supports_fill(), fill, "{tool:?} fill");
            let mut extended = drawing.clone();
            (extended.extend_left, extended.extend_right) =
                (!drawing.extend_left, !drawing.extend_right);
            let extend = format!("{:?}", paint(&extended)) != format!("{:?}", paint(&drawing));
            assert_eq!(tool.supports_extend(), extend, "{tool:?} extend");
            let mut labelled = drawing.clone();
            labelled.text = Some("Edited text".into());
            let text = found(
                &paint_with(&labelled, true),
                &|s| matches!(s, Shape::Text(t) if t.galley.text().contains("Edited text")),
            );
            let icon = matches!(tool, DrawingToolType::FontIcon | DrawingToolType::Sticker);
            assert_eq!(tool.supports_text(), text && !icon, "{tool:?} text");
            let mut plain = drawing.clone();
            plain.arrow_start_style = ArrowStyle::None;
            plain.arrow_end_style = ArrowStyle::None;
            let changes = |edit: &dyn Fn(&mut Drawing)| {
                let mut edited = plain.clone();
                edit(&mut edited);
                format!("{:?}", paint(&edited)) != format!("{:?}", paint(&plain))
            };
            for (end, styled) in [
                (
                    "start",
                    changes(&|d| d.arrow_start_style = ArrowStyle::Circle),
                ),
                ("end", changes(&|d| d.arrow_end_style = ArrowStyle::Circle)),
            ] {
                assert_eq!(tool.supports_line_ends(), styled, "{tool:?} line {end}");
            }
            let middle = changes(&|d| d.channel_midline = !d.channel_midline);
            assert_eq!(tool.supports_middle_line(), middle, "{tool:?} middle line");
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
            for (i, layout) in layouts.into_iter().enumerate() {
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
                // A label too long for the margin must grow the bounds with it, on the tiny anchors
                // where the margin is nearly all the reach there is.
                drawing.text = Some(if i == 2 {
                    "Sample label ".repeat(8)
                } else {
                    "Sample".into()
                });
                if tool == DrawingToolType::Image {
                    drawing.image = Some(crate::drawings::ImageConfig {
                        size: Some([1500.0, 800.0]),
                        ..Default::default()
                    });
                }
                let Some(bounds) = drawing.paint_bounds() else {
                    continue;
                };
                let ctx = Context::default();
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    let view = ChartView {
                        price_formatter: None,
                        instrument: None,
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|b| b * 40.0,
                        price_to_y: &|p| 500.0 - p as f32,
                        y_to_price: &|y| 500.0 - f64::from(y),
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
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
        for (tool, every_label) in [
            (DrawingToolType::TextLabel, false),
            (DrawingToolType::AnchoredText, false),
            (DrawingToolType::Note, false),
            (DrawingToolType::AnchoredNote, false),
            (DrawingToolType::Callout, false),
            (DrawingToolType::PriceLabel, false),
            (DrawingToolType::FlagNote, false),
            (DrawingToolType::PriceNote, false),
            (DrawingToolType::Comment, false),
            (DrawingToolType::Signpost, false),
            (DrawingToolType::Table, false),
            (DrawingToolType::FontIcon, false),
            (DrawingToolType::XABCDPattern, true),
            (DrawingToolType::CypherPattern, true),
            (DrawingToolType::HeadAndShoulders, true),
            (DrawingToolType::ABCDPattern, true),
            (DrawingToolType::TrianglePattern, true),
            (DrawingToolType::ThreeDrivesPattern, true),
            (DrawingToolType::ElliottImpulse, true),
            (DrawingToolType::ElliottCorrection, true),
            (DrawingToolType::ElliottTriangle, true),
            (DrawingToolType::ElliottDoubleCombo, true),
            (DrawingToolType::ElliottTripleCombo, true),
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = (0..tool.required_points().unwrap_or(2).max(2))
                .map(|i| {
                    Pos2::new(
                        50.0 + i as f32 * 60.0,
                        if i % 2 == 0 { 100.0 } else { 150.0 },
                    )
                })
                .collect();
            // Three drives labels only its ratios, which are measured in price.
            if tool == DrawingToolType::ThreeDrivesPattern {
                drawing.chart_points = (drawing.points.iter())
                    .map(|p| crate::drawings::ChartPoint::new(p.x / 10.0, 500.0 - p.y as f64))
                    .collect();
            }
            drawing.text = Some("Sample".into());
            drawing.font_size = 48.0;
            drawing.font_weight = FontWeight::Bold;
            let shapes = paint_with(&drawing, true);
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
                        .all(|t| table.contains_rect(t.visual_bounding_rect()))
                );
            }
            let matching = texts
                .iter()
                .filter(|t| {
                    t.galley
                        .job
                        .sections
                        .iter()
                        .all(|s| s.format.font_id.size == 48.0)
                })
                .count();
            assert!(
                matching > 0 && (!every_label || matching == texts.len()),
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

    /// Marks are 14 px triangles with their tip on the anchor and an 8.4 px half-base.
    /// The 200 px marker has a 70 by 80 px head and a shaft widening from 16 to 32 px:
    /// area 70 * 80 / 2 + 130 * (16 + 32) / 2 = 5920 px², in every direction.
    /// Saved line ends, extension, fill and line styles must not alter these solid shapes.
    #[test]
    fn arrow_tools_paint_one_solid_shape_in_the_drawing_colour() {
        use crate::drawings::ArrowStyle;
        use DrawingToolType as T;
        use egui::epaint::{Mesh, TessellationOptions, Tessellator};
        let p = Pos2::new;
        for (tool, anchors, want) in [
            (
                T::ArrowMarkUp,
                vec![p(200., 200.)],
                vec![p(200., 200.), p(208.4, 214.), p(191.6, 214.)],
            ),
            (
                T::ArrowMarkDown,
                vec![p(200., 200.)],
                vec![p(200., 200.), p(191.6, 186.), p(208.4, 186.)],
            ),
            (
                T::ArrowMarkLeft,
                vec![p(200., 200.)],
                vec![p(200., 200.), p(214., 191.6), p(214., 208.4)],
            ),
            (
                T::ArrowMarkRight,
                vec![p(200., 200.)],
                vec![p(200., 200.), p(186., 208.4), p(186., 191.6)],
            ),
            (
                T::ArrowMarker,
                vec![p(100., 200.), p(300., 200.)],
                vec![
                    p(300., 200.),
                    p(230., 240.),
                    p(230., 216.),
                    p(100., 208.),
                    p(100., 192.),
                    p(230., 184.),
                    p(230., 160.),
                ],
            ),
            (
                T::ArrowMarker,
                vec![p(300., 200.), p(100., 200.)],
                vec![
                    p(100., 200.),
                    p(170., 160.),
                    p(170., 184.),
                    p(300., 192.),
                    p(300., 208.),
                    p(170., 216.),
                    p(170., 240.),
                ],
            ),
            (
                T::ArrowMarker,
                vec![p(100., 300.), p(100., 100.)],
                vec![
                    p(100., 100.),
                    p(140., 170.),
                    p(116., 170.),
                    p(108., 300.),
                    p(92., 300.),
                    p(84., 170.),
                    p(60., 170.),
                ],
            ),
        ] {
            let mut drawing = Drawing::with_color(7, tool, [12, 34, 56, 255]);
            drawing.points = anchors;
            drawing.arrow_start_style = ArrowStyle::Circle;
            drawing.arrow_end_style = ArrowStyle::Arrow;
            (drawing.extend_left, drawing.extend_right) = (true, true);
            let shapes = paint(&drawing);
            let paths: Vec<_> = shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Path(path) => Some(path),
                    _ => None,
                })
                .collect();
            assert!(
                !shapes
                    .iter()
                    .any(|s| matches!(s, Shape::LineSegment { .. } | Shape::Circle(_))),
                "{tool:?} line or end"
            );
            assert_eq!(paths.len(), 1, "{tool:?}");
            let path = paths[0];
            assert_eq!(path.fill, drawing.color32(), "{tool:?} colour");
            assert!(path.closed && path.stroke.is_empty(), "{tool:?}");
            assert_eq!(path.points.len(), want.len(), "{tool:?}");
            for (got, want) in path.points.iter().zip(&want) {
                assert!(
                    got.distance(*want) < 1e-3,
                    "{tool:?}: {:?}, want {want:?}",
                    path.points
                );
            }
            let mut styled = drawing.clone();
            styled.fill_color = Some([200, 100, 50, 80]);
            styled.line_style = crate::model::LineStyle::Dashed;
            styled.stroke_width = 8.0;
            assert_eq!(
                format!("{:?}", paint(&styled)),
                format!("{shapes:?}"),
                "{tool:?} style"
            );
            if tool == T::ArrowMarker {
                let mut mesh = Mesh::default();
                Tessellator::new(1.0, TessellationOptions::default(), [1, 1], Vec::new())
                    .tessellate_shape(Shape::Path(path.clone()), &mut mesh);
                let mut area = 0.0;
                for tri in mesh.indices.as_chunks::<3>().0 {
                    let vertices = tri.map(|i| mesh.vertices[i as usize]);
                    if vertices.iter().any(|v| v.color.a() != 255) {
                        continue;
                    }
                    let [a, b, c] = vertices.map(|v| v.pos);
                    let centre = a + ((b - a) + (c - a)) / 3.0;
                    let mut inside = false;
                    for i in 0..want.len() {
                        let (a, b) = (want[i], want[(i + 1) % want.len()]);
                        if (a.y > centre.y) != (b.y > centre.y)
                            && centre.x < (b.x - a.x) * (centre.y - a.y) / (b.y - a.y) + a.x
                        {
                            inside = !inside;
                        }
                    }
                    assert!(inside, "triangle outside the arrow: {a:?}, {b:?}, {c:?}");
                    area += ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0;
                }
                // Feathering removes at most 10% of the opaque area.
                assert!((5328.0..=5920.0).contains(&area), "opaque area {area}");
            }
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

    /// y = 250 - x/2 and either y = 150 or its reflection y = 50 + x/2.
    /// Both cross at (200, 150); fills must cover both lobes without stroked ends.
    #[test]
    fn wedge_channels_share_clean_paint_fill_and_extended_hit_geometry() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 400.0));
        for tool in [
            DrawingToolType::FlatTopBottom,
            DrawingToolType::DisjointChannel,
        ] {
            for reverse in [false, true] {
                for (left, right, xs) in [
                    (false, false, [100.0, 300.0]),
                    (true, false, [0.0, 300.0]),
                    (false, true, [100.0, 500.0]),
                    (true, true, [0.0, 500.0]),
                ] {
                    for filled in [false, true] {
                        let mut drawing = Drawing::new(1, tool);
                        drawing.chart_points = vec![
                            crate::drawings::ChartPoint::new(10.0, 300.0),
                            crate::drawings::ChartPoint::new(30.0, 400.0),
                            crate::drawings::ChartPoint::new(20.0, 350.0),
                        ];
                        if reverse {
                            drawing.chart_points.swap(0, 1);
                        }
                        drawing.extend_left = left;
                        drawing.extend_right = right;
                        drawing.fill_color = filled.then_some([12, 34, 56, 80]);
                        drawing.update_screen_coords(|b| b * 10.0, |p| 500.0 - p as f32);
                        let other_y = |x| {
                            if tool == DrawingToolType::FlatTopBottom {
                                150.0
                            } else {
                                50.0 + x / 2.0
                            }
                        };
                        let lines = vec![
                            xs.map(|x| Pos2::new(x, 250.0 - x / 2.0)),
                            xs.map(|x| Pos2::new(x, other_y(x))),
                        ];
                        let outline = drawing.outline(rect).unwrap();
                        assert_eq!(outline.lines, lines, "{tool:?}");
                        assert_eq!(outline.areas.len(), if filled { 2 } else { 0 });
                        let shapes = paint(&drawing);
                        let mut painted_lines = Vec::new();
                        let mut areas = Vec::new();
                        for shape in shapes {
                            match shape {
                                Shape::LineSegment { points, .. } => painted_lines.push(points),
                                Shape::Path(path) if path.fill.a() > 0 => {
                                    assert!(path.stroke.is_empty());
                                    assert_eq!(path.fill, drawing.fill_color32().unwrap());
                                    areas.push(path.points);
                                }
                                Shape::Noop => {}
                                other => panic!("unexpected channel decoration: {other:?}"),
                            }
                        }
                        assert_eq!(painted_lines, lines);
                        assert_eq!(areas, outline.areas);
                        let hit = crate::drawings::services::DrawingInteraction::new();
                        for x in [125.0, 275.0] {
                            for y in [250.0 - x / 2.0, other_y(x)] {
                                assert_eq!(
                                    hit.hit_test(Pos2::new(x, y), &[drawing.clone()], "1m"),
                                    Some(1)
                                );
                            }
                            let y = (250.0 - x / 2.0 + other_y(x)) / 2.0;
                            assert_eq!(
                                hit.hit_test(Pos2::new(x, y), &[drawing.clone()], "1m"),
                                filled.then_some(1)
                            );
                        }
                    }
                }
            }
        }
    }

    /// Even-odd fills and their hit areas agree for concave and crossing paths and curve lobes.
    #[test]
    fn filled_shapes_cover_the_area_their_line_encloses() {
        use crate::drawings::services::DrawingInteraction;
        use DrawingToolType as T;
        let concave = vec![
            (50., 50.),
            (200., 50.),
            (200., 200.),
            (125., 100.),
            (50., 200.),
        ];
        // Analytic areas: a radius-100 semicircle, the quadratic's 300 * 90 * 2/3,
        // and the cubic's two lobes, 2 * 300 * 450 / 32. Curved fills allow the
        // curved boundary length, bounded by control polygons for Beziers, times
        // a quarter point of boundary error.
        for (tool, points, closed, area, area_tolerance, hits, misses) in [
            (
                T::Polyline,
                concave.clone(),
                true,
                15000.,
                2.,
                vec![(125., 75.), (50., 125.)],
                vec![(125., 150.)],
            ),
            (
                T::Polyline,
                concave,
                false,
                0.,
                2.,
                vec![],
                vec![(125., 75.), (50., 125.)],
            ),
            (
                T::Polyline,
                vec![(50., 50.), (250., 250.), (250., 50.), (50., 250.)],
                false,
                20000.,
                2.,
                vec![(70., 150.)],
                vec![(150., 70.)],
            ),
            (
                T::Arc,
                vec![(100., 200.), (300., 200.), (200., 100.)],
                false,
                std::f32::consts::PI * 5000.,
                80.,
                vec![(200., 150.)],
                vec![(200., 250.)],
            ),
            (
                T::Curve,
                vec![(100., 300.), (250., 120.), (400., 300.)],
                false,
                18000.,
                120.,
                vec![(250., 260.)],
                vec![(250., 190.)],
            ),
            (
                T::DoubleCurve,
                vec![(100., 200.), (200., 350.), (300., 50.), (400., 200.)],
                false,
                8437.5,
                170.,
                vec![(163.4, 230.), (336.6, 170.)],
                vec![(163.4, 170.), (250., 240.)],
            ),
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.points = points.into_iter().map(Pos2::from).collect();
            drawing.completed = true;
            drawing.closed = closed;
            drawing.fill_color = (area > 0.).then_some([12, 34, 56, 80]);
            let painted = paint(&drawing);
            let fill = Color32::from_rgba_unmultiplied(12, 34, 56, 80);
            let painted_area: f32 = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Mesh(mesh) => {
                        assert!(mesh.vertices.iter().all(|v| v.color == fill));
                        Some(
                            mesh.indices
                                .as_chunks::<3>()
                                .0
                                .iter()
                                .map(|tri| {
                                    let a = mesh.vertices[tri[0] as usize].pos;
                                    let b = mesh.vertices[tri[1] as usize].pos;
                                    let c = mesh.vertices[tri[2] as usize].pos;
                                    ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs()
                                        / 2.
                                })
                                .sum::<f32>(),
                        )
                    }
                    _ => None,
                })
                .sum();
            assert!(
                (painted_area - area).abs() < area_tolerance,
                "{tool:?}: {painted_area} != {area}"
            );
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500., 400.));
            let clipped: Vec<_> = painted
                .into_iter()
                .map(|shape| egui::epaint::ClippedShape::new(rect, shape))
                .collect();
            for (points, want) in [(hits, true), (misses, false)] {
                for point in points.into_iter().map(Pos2::from) {
                    assert_eq!(
                        DrawingInteraction::new().hit_test(point, &[drawing.clone()], "1m"),
                        want.then_some(1),
                        "{tool:?} outline at {point:?}"
                    );
                    assert_eq!(
                        DrawingInteraction::hit_test_painted(point, &clipped),
                        want,
                        "{tool:?} paint at {point:?}"
                    );
                }
            }
        }
        for points in [
            vec![],
            vec![Pos2::ZERO; 2],
            vec![Pos2::ZERO, Pos2::new(f32::NAN, 1.), Pos2::new(1., 2.)],
        ] {
            assert!(shapes::even_odd_trapezoids(&points).is_empty());
        }
    }

    pub(super) fn paint(drawing: &Drawing) -> Vec<Shape> {
        paint_with(drawing, false)
    }

    fn paint_with(drawing: &Drawing, selected: bool) -> Vec<Shape> {
        let ctx = Context::default();
        let bars = sample_bars();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            let view = ChartView {
                price_formatter: None,
                instrument: None,
                bars: if drawing.tool_type == DrawingToolType::RegressionTrend {
                    &bars
                } else {
                    &[]
                },
                revision: 0,
                bar_to_x: &|b| b * 10.0,
                price_to_y: &|p| 500.0 - p as f32,
                y_to_price: &|y| 500.0 - f64::from(y),
                time_axis: true,
                executions: None,
                timeframe: None,
                finer: &[],
            };
            drawing.render_with_selection(
                ui.painter(),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(500., 400.)),
                &view,
                selected,
            );
        });
        let shapes = output.shapes.iter().map(|s| s.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    /// Each tool with an anchor and a label draws the label where its anchors say, joined to the
    /// anchor by one thin line from the side that faces it, and a pin shows its text only while
    /// selected.
    #[test]
    fn anchored_labels_join_their_anchor() {
        #[derive(Default)]
        struct Painted {
            rects: Vec<egui::epaint::RectShape>,
            segments: Vec<([Pos2; 2], egui::Stroke)>,
            circles: Vec<egui::epaint::CircleShape>,
            paths: Vec<egui::epaint::PathShape>,
            texts: Vec<egui::epaint::TextShape>,
        }
        fn collect(shapes: &[Shape], painted: &mut Painted) {
            for shape in shapes {
                match shape {
                    Shape::Vec(shapes) => collect(shapes, painted),
                    Shape::Rect(r) => painted.rects.push(r.clone()),
                    Shape::LineSegment { points, stroke } => {
                        painted.segments.push((*points, *stroke))
                    }
                    Shape::Circle(c) => painted.circles.push(*c),
                    Shape::Path(p) => painted.paths.push(p.clone()),
                    Shape::Text(t) => painted.texts.push(t.clone()),
                    _ => {}
                }
            }
        }
        let text_of = |t: &egui::epaint::TextShape| t.galley.job.text.clone();
        let pos = |x, y| Pos2::new(x, y);
        // Bars and prices come from the mapping `paint` renders with.
        let draw = |tool, points: &[(f32, f32)], text: &str, selected| {
            let mut drawing = Drawing::new(7, tool);
            // White on this red has a contrast of 1.05 / (0.2195 + 0.05) = 3.90.
            drawing.color = [0xF2, 0x36, 0x45, 0xFF];
            drawing.points = points.iter().map(|&(x, y)| pos(x, y)).collect();
            drawing.chart_points = points
                .iter()
                .map(|&(x, y)| crate::drawings::ChartPoint::new(x / 10.0, 500.0 - f64::from(y)))
                .collect();
            drawing.text = Some(text.into());
            let mut painted = Painted::default();
            collect(&paint_with(&drawing, selected), &mut painted);
            painted
        };

        let note = draw(
            DrawingToolType::Note,
            &[(130., 400.), (100., 100.)],
            "Hi",
            false,
        );
        let [card] = &note.rects[..] else {
            panic!("one label");
        };
        assert_eq!(card.rect.min, pos(100.0, 100.0));
        assert_eq!(card.stroke.width, 1.0);
        // The anchor lies far below and at most 30 px to the side, so the bottom faces it.
        assert_eq!(
            note.segments,
            [(
                [
                    pos(card.rect.center().x, card.rect.max.y),
                    pos(130.0, 400.0)
                ],
                egui::Stroke::new(1.0, Color32::from_rgb(0xF2, 0x36, 0x45)),
            )]
        );
        let [dot] = &note.circles[..] else {
            panic!("one dot");
        };
        assert_eq!((dot.center, dot.radius), (pos(130.0, 400.0), 3.0));
        assert_eq!(note.texts.iter().map(text_of).collect::<Vec<_>>(), ["Hi"]);

        let saved = draw(DrawingToolType::Note, &[(100., 100.)], "", false);
        assert_eq!(saved.rects[0].rect.min, pos(100.0, 100.0));
        assert!(saved.segments.is_empty());
        assert_eq!(
            saved.texts.iter().map(text_of).collect::<Vec<_>>(),
            ["Text"]
        );

        let price_note = draw(
            DrawingToolType::PriceNote,
            &[(130., 400.), (100., 100.)],
            "Entry",
            false,
        );
        assert_eq!(
            price_note.texts.iter().map(text_of).collect::<Vec<_>>(),
            ["100.00 Entry"]
        );
        assert_eq!(price_note.segments.len(), 1, "no line across the pane");
        assert_eq!(
            price_note.rects[0].fill,
            Color32::from_rgb(0xF2, 0x36, 0x45)
        );
        assert_eq!(
            price_note.texts[0].galley.job.sections[0].format.color,
            Color32::WHITE
        );

        let pin = draw(
            DrawingToolType::AnchoredNote,
            &[(200., 300.)],
            "Fill",
            false,
        );
        assert!(pin.texts.is_empty());
        let [marker] = &pin.paths[..] else {
            panic!("one marker");
        };
        let bounds = Rect::from_points(&marker.points);
        assert!(marker.closed && marker.points.contains(&pos(200.0, 300.0)));
        assert!(
            Rect::from_min_max(pos(194.0, 278.0), pos(206.0, 300.0))
                .expand(0.01)
                .contains_rect(bounds)
                && bounds.height() > 20.0,
            "{bounds:?}"
        );
        let selected = draw(DrawingToolType::AnchoredNote, &[(200., 300.)], "Fill", true);
        assert_eq!(
            selected.texts.iter().map(text_of).collect::<Vec<_>>(),
            ["Fill"]
        );
        assert!(selected.texts[0].visual_bounding_rect().max.y <= 274.0);

        let comment = draw(DrawingToolType::Comment, &[(100., 300.)], "Hi", false);
        let [bubble] = &comment.rects[..] else {
            panic!("one bubble");
        };
        assert_eq!((bubble.rect.min.x, bubble.rect.max.y), (100.0, 300.0));
        assert_eq!(
            bubble.corner_radius,
            egui::CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 8
            }
        );
        assert!(comment.circles.is_empty());
        assert!(
            bubble
                .rect
                .contains_rect(comment.texts[0].visual_bounding_rect())
        );

        // Black on this yellow has the better contrast: white has only 1.05 / 0.860 = 1.22.
        let mut pale = Drawing::new(7, DrawingToolType::Comment);
        pale.points = vec![pos(100.0, 300.0)];
        pale.fill_color = Some([255, 235, 59, 255]);
        let mut painted = Painted::default();
        collect(&paint(&pale), &mut painted);
        assert_eq!(
            painted.texts[0].galley.job.sections[0].format.color,
            Color32::BLACK
        );

        let callout = draw(
            DrawingToolType::Callout,
            &[(100., 300.), (200., 100.)],
            "Hi",
            false,
        );
        assert!(callout.rects.is_empty());
        let [outline] = &callout.paths[..] else {
            panic!("box and pointer are one shape");
        };
        assert!(outline.closed && outline.points.contains(&pos(100.0, 300.0)));
        assert_eq!(outline.stroke.width, 1.0);
        let inside = draw(
            DrawingToolType::Callout,
            &[(205., 105.), (200., 100.)],
            "Hi",
            false,
        );
        assert!(inside.paths.is_empty());
        assert_eq!(inside.rects.len(), 1);

        let label = draw(
            DrawingToolType::PriceLabel,
            &[(100., 300.)],
            "Ignored",
            false,
        );
        assert_eq!(
            label.texts.iter().map(text_of).collect::<Vec<_>>(),
            ["200.00"]
        );
        let [badge] = &label.paths[..] else {
            panic!("badge and pointer are one shape");
        };
        assert!(badge.closed && badge.points.contains(&pos(100.0, 300.0)));
        let text = label.texts[0].visual_bounding_rect();
        assert!(text.min.x > 108.0 && text.max.y < 292.0, "{text:?}");

        // A second anchor on another bar still gets a vertical pole; one anchor shows the label
        // 40 px above the bar.
        for (points, centre) in [
            (&[(100., 300.), (160., 200.)][..], pos(100.0, 200.0)),
            (&[(100., 300.)][..], pos(100.0, 260.0)),
        ] {
            let signpost = draw(DrawingToolType::Signpost, points, "Earnings", false);
            let [plate] = &signpost.rects[..] else {
                panic!("one plate");
            };
            assert_eq!(plate.rect.center(), centre);
            let [(pole, stroke)] = signpost.segments[..] else {
                panic!("one pole");
            };
            assert_eq!(pole, [pos(100.0, 300.0), pos(100.0, plate.rect.max.y)]);
            assert_eq!(stroke.width, 1.0);
            assert_eq!(
                signpost.texts.iter().map(text_of).collect::<Vec<_>>(),
                ["Earnings"]
            );
        }
    }

    /// A pattern's labels and selection handles can cross the pane edge;
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
                price_formatter: None,
                instrument: None,
                bars: &[],
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y: &|p| p as f32,
                y_to_price: &f64::from,
                time_axis: true,
                executions: None,
                timeframe: None,
                finer: &[],
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
