//! Handle manipulation service for drawing tools.
//!
//! Provides all handle-related operations for selected drawings:
//!
//! - **Discovery** -- [`HandleService::get_handles`] returns the handle
//!   positions for a drawing based on its tool type.
//! - **Hit testing** -- [`HandleService::hit_test_handle`] determines which
//!   handle (if any) is under the cursor.
//! - **Dragging** -- [`HandleService::update_handle`] moves a handle and
//!   updates both screen and chart coordinates.
//! - **Rendering** -- [`HandleService::render_handles`] draws round handles
//!   with selection highlighting.

use crate::drawings::domain::{Drawing, DrawingToolType, HandlePos};
use crate::theme::ThemeExt;
use egui::{Color32, Pos2};

/// Configuration for selection handle appearance and hit testing.
#[derive(Clone, Debug)]
pub struct HandleConfig {
    /// Handle diameter in pixels.
    pub size: f32,
    /// Ring color of a handle as RGBA bytes (`Color32::PLACEHOLDER.to_array()`: the theme's
    /// accent).
    pub color: [u8; 4],
    /// Fill color of the handle being dragged as RGBA bytes
    /// (`Color32::PLACEHOLDER.to_array()`: the theme's accent).
    pub selected_color: [u8; 4],
    /// Whether to render handles at all (can be disabled for screenshot mode, etc.).
    pub show_handles: bool,
}

impl Default for HandleConfig {
    fn default() -> Self {
        Self {
            size: 8.0,
            color: Color32::PLACEHOLDER.to_array(),
            selected_color: Color32::PLACEHOLDER.to_array(),
            show_handles: true,
        }
    }
}

/// Stateless service for selection handle operations on drawings.
///
/// All methods are static (`&self`-free) since `HandleService` carries no state.
/// It computes handle positions, performs hit testing, updates handle positions
/// during drags, and renders the handles onto the chart.
pub struct HandleService;

impl HandleService {
    /// Returns the selection handles for a drawing based on its tool type.
    ///
    /// Handle layouts vary by tool type:
    /// - **Two-point tools** (TrendLine, Fibonacci, etc.): Start, End, and Middle handles
    /// - **Rectangle**: Four corner handles and a Middle handle
    /// - **Single-point tools** (HorizontalLine, VerticalLine): One Start handle
    /// - **Multi-point tools**: Indexed handles at each point plus a centroid Middle handle
    pub fn get_handles(drawing: &Drawing) -> Vec<(HandlePos, Pos2)> {
        let mut handles = Vec::new();

        match drawing.tool_type {
            DrawingToolType::TrendLine | DrawingToolType::Measure => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
                if drawing.points.len() >= 2 {
                    handles.push((HandlePos::End, drawing.points[1]));
                    // Middle handle for moving entire drawing
                    let mid = Pos2::new(
                        (drawing.points[0].x + drawing.points[1].x) / 2.0,
                        (drawing.points[0].y + drawing.points[1].y) / 2.0,
                    );
                    handles.push((HandlePos::Middle, mid));
                }
            }
            DrawingToolType::Rect => {
                if drawing.points.len() >= 2 {
                    let p1 = drawing.points[0];
                    let p2 = drawing.points[1];
                    handles.push((HandlePos::TopLeft, p1));
                    handles.push((HandlePos::TopRight, Pos2::new(p2.x, p1.y)));
                    handles.push((HandlePos::BottomLeft, Pos2::new(p1.x, p2.y)));
                    handles.push((HandlePos::BottomRight, p2));
                    handles.push((HandlePos::Middle, p1 + (p2 - p1) / 2.0));
                }
            }
            DrawingToolType::HorizontalLine => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
            }
            DrawingToolType::VerticalLine => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
            }
            // Fibonacci and similar two-point tools
            DrawingToolType::FibonacciRetracement
            | DrawingToolType::GannFan
            | DrawingToolType::GannSquare
            | DrawingToolType::GannBox => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
                if drawing.points.len() >= 2 {
                    handles.push((HandlePos::End, drawing.points[1]));
                    let mid = Pos2::new(
                        (drawing.points[0].x + drawing.points[1].x) / 2.0,
                        (drawing.points[0].y + drawing.points[1].y) / 2.0,
                    );
                    handles.push((HandlePos::Middle, mid));
                }
            }
            // Multi-point tools - show indexed handles
            _ => {
                for (i, &point) in drawing.points.iter().enumerate() {
                    handles.push((HandlePos::Point(i), point));
                }
                // Add middle handle for moving if we have points
                if drawing.points.len() >= 2 {
                    let cx: f32 = drawing.points.iter().map(|p| p.x).sum::<f32>()
                        / drawing.points.len() as f32;
                    let cy: f32 = drawing.points.iter().map(|p| p.y).sum::<f32>()
                        / drawing.points.len() as f32;
                    handles.push((HandlePos::Middle, Pos2::new(cx, cy)));
                }
            }
        }

        handles
    }

    /// Hit-tests to find which handle (if any) is at a screen position.
    ///
    /// Returns `Some(HandlePos)` if `point` is within `handle_size * 1.5` pixels
    /// of a handle center, `None` otherwise.
    pub fn hit_test_handle(drawing: &Drawing, point: Pos2, handle_size: f32) -> Option<HandlePos> {
        let handles = Self::get_handles(drawing);
        let hit_radius = handle_size * 1.5;

        for (pos, handle_point) in handles {
            let dist =
                ((point.x - handle_point.x).powi(2) + (point.y - handle_point.y).powi(2)).sqrt();
            if dist <= hit_radius {
                return Some(pos);
            }
        }
        None
    }

    /// Moves a handle to a new screen position, updating both screen coordinates
    /// and chart coordinates using the provided conversion closures.
    ///
    /// For the `Middle` handle, all points are translated by the delta between
    /// the old and new centroid positions (move entire drawing).
    pub fn update_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if handle == HandlePos::Middle {
            if let Some((_, center)) = Self::get_handles(drawing)
                .into_iter()
                .find(|(h, _)| *h == handle)
            {
                let bars = x_to_bar(new_pos.x) - x_to_bar(center.x);
                let price = y_to_price(new_pos.y) - y_to_price(center.y);
                for point in &mut drawing.points {
                    *point += new_pos - center;
                }
                for point in &mut drawing.chart_points {
                    point.bar_idx += bars;
                    point.price += price;
                }
            }
            return;
        }
        match drawing.tool_type {
            DrawingToolType::TrendLine
            | DrawingToolType::Measure
            | DrawingToolType::FibonacciRetracement
            | DrawingToolType::GannFan
            | DrawingToolType::GannSquare
            | DrawingToolType::GannBox => {
                Self::update_two_point_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
            }
            DrawingToolType::Rect => {
                Self::update_rect_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
            }
            DrawingToolType::HorizontalLine => {
                if !drawing.points.is_empty() && !drawing.chart_points.is_empty() {
                    drawing.points[0].y = new_pos.y;
                    drawing.chart_points[0].price = y_to_price(new_pos.y);
                }
            }
            DrawingToolType::VerticalLine => {
                if !drawing.points.is_empty() && !drawing.chart_points.is_empty() {
                    drawing.points[0].x = new_pos.x;
                    drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                }
            }
            DrawingToolType::LongPos | DrawingToolType::ShortPos => {
                Self::update_multi_point_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
                // The stop line ends where the target line does; keep its handle there.
                if drawing.points.len() > 2 && drawing.chart_points.len() > 2 {
                    drawing.points[2].x = drawing.points[1].x;
                    drawing.chart_points[2].bar_idx = drawing.chart_points[1].bar_idx;
                }
            }
            _ => {
                Self::update_multi_point_handle(drawing, handle, new_pos, &x_to_bar, &y_to_price);
            }
        }
    }

    /// Update handle for two-point tools (trendline, fib, etc.)
    fn update_two_point_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: &F,
        y_to_price: &G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        match handle {
            HandlePos::Start if !drawing.points.is_empty() => {
                drawing.points[0] = new_pos;
                if !drawing.chart_points.is_empty() {
                    drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                    drawing.chart_points[0].price = y_to_price(new_pos.y);
                }
            }
            HandlePos::End if drawing.points.len() >= 2 => {
                drawing.points[1] = new_pos;
                if drawing.chart_points.len() >= 2 {
                    drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
                    drawing.chart_points[1].price = y_to_price(new_pos.y);
                }
            }
            _ => {}
        }
    }

    /// Update handle for rectangle tool
    fn update_rect_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: &F,
        y_to_price: &G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if drawing.points.len() < 2 || drawing.chart_points.len() < 2 {
            return;
        }

        match handle {
            HandlePos::TopLeft => {
                drawing.points[0] = new_pos;
                drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[0].price = y_to_price(new_pos.y);
            }
            HandlePos::TopRight => {
                drawing.points[0].y = new_pos.y;
                drawing.points[1].x = new_pos.x;
                drawing.chart_points[0].price = y_to_price(new_pos.y);
                drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
            }
            HandlePos::BottomLeft => {
                drawing.points[0].x = new_pos.x;
                drawing.points[1].y = new_pos.y;
                drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[1].price = y_to_price(new_pos.y);
            }
            HandlePos::BottomRight => {
                drawing.points[1] = new_pos;
                drawing.chart_points[1].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[1].price = y_to_price(new_pos.y);
            }
            _ => {}
        }
    }

    /// Update handle for multi-point tools
    fn update_multi_point_handle<F, G>(
        drawing: &mut Drawing,
        handle: HandlePos,
        new_pos: Pos2,
        x_to_bar: &F,
        y_to_price: &G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        match handle {
            HandlePos::Point(idx)
                if idx < drawing.points.len() && idx < drawing.chart_points.len() =>
            {
                drawing.points[idx] = new_pos;
                drawing.chart_points[idx].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[idx].price = y_to_price(new_pos.y);
            }
            HandlePos::Start if !drawing.points.is_empty() && !drawing.chart_points.is_empty() => {
                drawing.points[0] = new_pos;
                drawing.chart_points[0].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[0].price = y_to_price(new_pos.y);
            }
            HandlePos::End if drawing.points.len() >= 2 && drawing.chart_points.len() >= 2 => {
                let last = drawing.points.len() - 1;
                drawing.points[last] = new_pos;
                drawing.chart_points[last].bar_idx = x_to_bar(new_pos.x);
                drawing.chart_points[last].price = y_to_price(new_pos.y);
            }
            _ => {}
        }
    }

    /// Renders selection handles like the selection dots of a series: an accent ring around
    /// the chart background.
    ///
    /// The actively dragged handle (if any) is filled with
    /// [`HandleConfig::selected_color`]; the others are ringed with [`HandleConfig::color`].
    pub fn render_handles(
        painter: &egui::Painter,
        drawing: &Drawing,
        config: &HandleConfig,
        dragging_handle: Option<HandlePos>,
    ) {
        if !config.show_handles {
            return;
        }

        let theme = painter.ctx().active_theme();
        let color = |[r, g, b, a]: [u8; 4]| {
            if [r, g, b, a] == Color32::PLACEHOLDER.to_array() {
                theme.ui.accent
            } else {
                Color32::from_rgba_unmultiplied(r, g, b, a)
            }
        };
        let radius = config.size / 2.0;

        for (pos, point) in Self::get_handles(drawing) {
            if dragging_handle == Some(pos) {
                painter.circle_filled(point, radius, color(config.selected_color));
            } else {
                painter.circle_filled(point, radius, color(config.color));
                painter.circle_filled(point, radius - RING_WIDTH, theme.chart.bg);
            }
        }
    }
}

/// Width of a handle's ring, as on a series' selection dots.
const RING_WIDTH: f32 = 2.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::ChartPoint;
    use egui::pos2;

    /// Screen to chart mapping used by the tests: 10 px per bar, price 1000 at
    /// the top falling one unit per pixel.
    fn x_to_bar(x: f32) -> f32 {
        x / 10.0
    }

    fn y_to_price(y: f32) -> f64 {
        1000.0 - y as f64
    }

    fn create_test_drawing(tool_type: DrawingToolType, points: Vec<Pos2>) -> Drawing {
        let mut drawing = Drawing::new(1, tool_type);
        drawing.chart_points = points
            .iter()
            .map(|p| ChartPoint::new(x_to_bar(p.x), y_to_price(p.y)))
            .collect();
        drawing.points = points;
        drawing
    }

    #[test]
    fn get_handles_by_tool() {
        use HandlePos::*;
        let cases = [
            (
                DrawingToolType::TrendLine,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0)],
                vec![
                    (Start, pos2(100.0, 200.0)),
                    (End, pos2(300.0, 400.0)),
                    (Middle, pos2(200.0, 300.0)),
                ],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                vec![
                    (TopLeft, pos2(100.0, 100.0)),
                    (TopRight, pos2(200.0, 100.0)),
                    (BottomLeft, pos2(100.0, 300.0)),
                    (BottomRight, pos2(200.0, 300.0)),
                    (Middle, pos2(150.0, 200.0)),
                ],
            ),
            (
                DrawingToolType::HorizontalLine,
                vec![pos2(50.0, 120.0)],
                vec![(Start, pos2(50.0, 120.0))],
            ),
            (
                DrawingToolType::FibonacciRetracement,
                vec![pos2(0.0, 0.0), pos2(100.0, 50.0)],
                vec![
                    (Start, pos2(0.0, 0.0)),
                    (End, pos2(100.0, 50.0)),
                    (Middle, pos2(50.0, 25.0)),
                ],
            ),
            (
                DrawingToolType::FibonacciChannel,
                vec![pos2(0.0, 0.0), pos2(30.0, 0.0), pos2(0.0, 60.0)],
                vec![
                    (Point(0), pos2(0.0, 0.0)),
                    (Point(1), pos2(30.0, 0.0)),
                    (Point(2), pos2(0.0, 60.0)),
                    (Middle, pos2(10.0, 20.0)),
                ],
            ),
            (
                DrawingToolType::Triangle,
                vec![pos2(0.0, 0.0), pos2(30.0, 0.0), pos2(0.0, 60.0)],
                vec![
                    (Point(0), pos2(0.0, 0.0)),
                    (Point(1), pos2(30.0, 0.0)),
                    (Point(2), pos2(0.0, 60.0)),
                    (Middle, pos2(10.0, 20.0)),
                ],
            ),
        ];
        for (tool, points, expected) in cases {
            let drawing = create_test_drawing(tool, points);
            assert_eq!(HandleService::get_handles(&drawing), expected, "{tool:?}");
        }
        // Renderer control counts, including generated position stops and freehand samples.
        let anchor_counts: &[(&[DrawingToolType], usize)] = &[
            (
                &[
                    DrawingToolType::CrossCursor,
                    DrawingToolType::DotCursor,
                    DrawingToolType::ArrowCursor,
                    DrawingToolType::Eraser,
                ],
                0,
            ),
            (
                &[
                    DrawingToolType::HorizontalLine,
                    DrawingToolType::HorizontalRay,
                    DrawingToolType::VerticalLine,
                    DrawingToolType::CrossLine,
                    DrawingToolType::TextLabel,
                    DrawingToolType::AnchoredText,
                    DrawingToolType::Note,
                    DrawingToolType::AnchoredNote,
                    DrawingToolType::PriceLabel,
                    DrawingToolType::FlagNote,
                    DrawingToolType::ArrowMarkUp,
                    DrawingToolType::ArrowMarkDown,
                    DrawingToolType::AnchoredVWAP,
                    DrawingToolType::AnchoredVolumeProfile,
                    DrawingToolType::Table,
                    DrawingToolType::Comment,
                    DrawingToolType::Signpost,
                    DrawingToolType::FontIcon,
                    DrawingToolType::Tweet,
                    DrawingToolType::Idea,
                ],
                1,
            ),
            (
                &[
                    DrawingToolType::TrendLine,
                    DrawingToolType::Ray,
                    DrawingToolType::InfoLine,
                    DrawingToolType::ExtendedLine,
                    DrawingToolType::TrendAngle,
                    DrawingToolType::RegressionTrend,
                    DrawingToolType::FibonacciRetracement,
                    DrawingToolType::FibonacciTimeZones,
                    DrawingToolType::FibonacciSpeedFan,
                    DrawingToolType::FibonacciCircles,
                    DrawingToolType::FibonacciSpiral,
                    DrawingToolType::FibonacciSpeedResistanceArcs,
                    DrawingToolType::GannBox,
                    DrawingToolType::GannSquare,
                    DrawingToolType::GannFixed,
                    DrawingToolType::GannFan,
                    DrawingToolType::CyclicLines,
                    DrawingToolType::TimeCycles,
                    DrawingToolType::SineLine,
                    DrawingToolType::Forecast,
                    DrawingToolType::FixedRangeVolumeProfile,
                    DrawingToolType::Measure,
                    DrawingToolType::PriceRange,
                    DrawingToolType::DateRange,
                    DrawingToolType::DateAndPriceRange,
                    DrawingToolType::ArrowMarker,
                    DrawingToolType::Arrow,
                    DrawingToolType::Rect,
                    DrawingToolType::Circle,
                    DrawingToolType::Ellipse,
                    DrawingToolType::PriceNote,
                    DrawingToolType::Callout,
                    DrawingToolType::Image,
                ],
                2,
            ),
            (
                &[
                    DrawingToolType::ParallelChannel,
                    DrawingToolType::FlatTopBottom,
                    DrawingToolType::DisjointChannel,
                    DrawingToolType::Pitchfork,
                    DrawingToolType::SchiffPitchfork,
                    DrawingToolType::ModifiedSchiffPitchfork,
                    DrawingToolType::InsidePitchfork,
                    DrawingToolType::FibonacciExtension,
                    DrawingToolType::FibonacciChannel,
                    DrawingToolType::FibonacciWedge,
                    DrawingToolType::TrendBasedFibTime,
                    DrawingToolType::Pitchfan,
                    DrawingToolType::Triangle,
                    DrawingToolType::RotatedRect,
                    DrawingToolType::Arc,
                    DrawingToolType::Curve,
                    DrawingToolType::ProjectionTool,
                    DrawingToolType::BarsPattern,
                    DrawingToolType::GhostFeed,
                    DrawingToolType::ElliottDoubleCombo,
                    DrawingToolType::LongPos,
                    DrawingToolType::ShortPos,
                ],
                3,
            ),
            (
                &[
                    DrawingToolType::ABCDPattern,
                    DrawingToolType::ElliottCorrection,
                    DrawingToolType::TrianglePattern,
                    DrawingToolType::DoubleCurve,
                ],
                4,
            ),
            (
                &[
                    DrawingToolType::XABCDPattern,
                    DrawingToolType::CypherPattern,
                    DrawingToolType::ElliottTriangle,
                    DrawingToolType::ElliottTripleCombo,
                    DrawingToolType::Brush,
                    DrawingToolType::Highlighter,
                    DrawingToolType::Polyline,
                    DrawingToolType::Path,
                ],
                5,
            ),
            (&[DrawingToolType::ElliottImpulse], 6),
            (
                &[
                    DrawingToolType::HeadAndShoulders,
                    DrawingToolType::ThreeDrivesPattern,
                ],
                7,
            ),
        ];
        for &(tools, count) in anchor_counts {
            for &tool in tools {
                let points: Vec<_> = (0..count)
                    .map(|i| pos2(i as f32 * 30.0, i as f32 * 20.0))
                    .collect();
                let drawing = create_test_drawing(tool, points.clone());
                let handles = HandleService::get_handles(&drawing);
                for point in points {
                    assert!(
                        handles.iter().any(|(_, p)| *p == point),
                        "{tool:?}: no handle at {point:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_hit_test_handle() {
        let drawing = create_test_drawing(
            DrawingToolType::TrendLine,
            vec![Pos2::new(100.0, 200.0), Pos2::new(300.0, 400.0)],
        );

        // Test hitting the start handle
        let hit = HandleService::hit_test_handle(&drawing, Pos2::new(102.0, 202.0), 8.0);
        assert_eq!(hit, Some(HandlePos::Start));

        // Test missing all handles
        let miss = HandleService::hit_test_handle(&drawing, Pos2::new(500.0, 500.0), 8.0);
        assert_eq!(miss, None);
    }

    /// A handle drag moves the screen points and the persisted chart points
    /// together: one anchor for an endpoint, one coordinate of each anchor for
    /// a rectangle corner, and every anchor for the Middle handle of a two-point
    /// or multi-point tool.
    #[test]
    fn update_handle_moves_screen_and_chart_points() {
        let cases = [
            (
                DrawingToolType::TrendLine,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0)],
                HandlePos::Start,
                pos2(150.0, 250.0),
                vec![pos2(150.0, 250.0), pos2(300.0, 400.0)],
                vec![ChartPoint::new(15.0, 750.0), ChartPoint::new(30.0, 600.0)],
            ),
            (
                DrawingToolType::TrendLine,
                vec![pos2(100.0, 200.0), pos2(300.0, 400.0)],
                HandlePos::Middle,
                pos2(250.0, 280.0),
                vec![pos2(150.0, 180.0), pos2(350.0, 380.0)],
                vec![ChartPoint::new(15.0, 820.0), ChartPoint::new(35.0, 620.0)],
            ),
            (
                DrawingToolType::Rect,
                vec![pos2(100.0, 100.0), pos2(200.0, 300.0)],
                HandlePos::TopRight,
                pos2(250.0, 80.0),
                vec![pos2(100.0, 80.0), pos2(250.0, 300.0)],
                vec![ChartPoint::new(10.0, 920.0), ChartPoint::new(25.0, 700.0)],
            ),
            (
                DrawingToolType::Triangle,
                vec![pos2(0.0, 0.0), pos2(30.0, 0.0), pos2(0.0, 60.0)],
                HandlePos::Middle,
                pos2(20.0, 30.0),
                vec![pos2(10.0, 10.0), pos2(40.0, 10.0), pos2(10.0, 70.0)],
                vec![
                    ChartPoint::new(1.0, 990.0),
                    ChartPoint::new(4.0, 990.0),
                    ChartPoint::new(1.0, 930.0),
                ],
            ),
        ];
        for (tool, points, handle, to, screen, chart) in cases {
            let mut drawing = create_test_drawing(tool, points);
            HandleService::update_handle(&mut drawing, handle, to, x_to_bar, y_to_price);
            assert_eq!(drawing.points, screen, "{tool:?} {handle:?}");
            assert_eq!(drawing.chart_points, chart, "{tool:?} {handle:?}");
        }
    }
}
