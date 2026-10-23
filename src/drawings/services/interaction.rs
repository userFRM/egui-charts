//! Drawing interaction service.
//!
//! Provides a self-contained handler for drawing interaction: hit testing
//! against all visible drawings (with timeframe filtering), handle discovery
//! and hit testing, handle drag lifecycle, and handle rendering.
//!
//! This service can be used standalone (without [`DrawingManager`](crate::drawings::DrawingManager))
//! for custom interaction implementations.

use crate::drawings::{Drawing, DrawingOptions, DrawingToolType, HandlePos};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke, epaint::StrokeKind};

/// Self-contained interaction handler for chart drawings.
///
/// Manages selection state, handle dragging, and hit testing. Unlike
/// [`DrawingManager`](crate::drawings::DrawingManager) which coordinates all
/// services, `DrawingInteraction` focuses purely on pointer-based interaction.
pub struct DrawingInteraction {
    /// Currently selected drawing ID, or `None`.
    pub sel_drawing: Option<usize>,
    /// Handle currently being dragged: `(drawing_id, handle_position)`.
    pub dragging_handle: Option<(usize, HandlePos)>,
    /// Snapshot of the drawing state before the current drag began (for undo).
    drag_old_state: Option<Drawing>,
}

impl Default for DrawingInteraction {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingInteraction {
    /// Creates a new interaction handler with no selection and no active drag.
    pub fn new() -> Self {
        Self {
            sel_drawing: None,
            dragging_handle: None,
            drag_old_state: None,
        }
    }

    /// Clears the current drawing selection.
    pub fn deselect(&mut self) {
        self.sel_drawing = None;
    }

    /// Returns the selection handle positions for a drawing, determined by its
    /// tool type (see [`HandleService::get_handles`](super::HandleService::get_handles)
    /// for the canonical implementation).
    pub fn get_handles(&self, drawing: &Drawing) -> Vec<(HandlePos, Pos2)> {
        let mut handles = Vec::new();

        match drawing.tool_type {
            DrawingToolType::TrendLine | DrawingToolType::Measure => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
                if drawing.points.len() >= 2 {
                    handles.push((HandlePos::End, drawing.points[1]));
                    // Middle handle
                    let mid = Pos2::new(
                        (drawing.points[0].x + drawing.points[1].x) / 2.0,
                        (drawing.points[0].y + drawing.points[1].y) / 2.0,
                    );
                    handles.push((HandlePos::Middle, mid));
                }
            }
            // Fibonacci tools
            DrawingToolType::FibonacciRetracement
            | DrawingToolType::FibonacciExtension
            | DrawingToolType::FibonacciChannel
            | DrawingToolType::FibonacciSpeedResistanceArcs
            | DrawingToolType::FibonacciTimeZones
            | DrawingToolType::FibonacciCircles
            | DrawingToolType::FibonacciSpeedFan
            | DrawingToolType::FibonacciSpiral
            | DrawingToolType::FibonacciWedge
            | DrawingToolType::TrendBasedFibTime => {
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
            DrawingToolType::Rect => {
                if drawing.points.len() >= 2 {
                    let p1 = drawing.points[0];
                    let p2 = drawing.points[1];
                    handles.push((HandlePos::TopLeft, p1));
                    handles.push((HandlePos::TopRight, Pos2::new(p2.x, p1.y)));
                    handles.push((HandlePos::BottomLeft, Pos2::new(p1.x, p2.y)));
                    handles.push((HandlePos::BottomRight, p2));
                }
            }
            DrawingToolType::HorizontalLine | DrawingToolType::VerticalLine => {
                if !drawing.points.is_empty() {
                    handles.push((HandlePos::Start, drawing.points[0]));
                }
            }
            _ => {
                // For multi-point drawings, show handle at each point
                for (i, &point) in drawing.points.iter().enumerate() {
                    let pos = if i == 0 {
                        HandlePos::Start
                    } else if i == drawing.points.len() - 1 {
                        HandlePos::End
                    } else {
                        HandlePos::Middle
                    };
                    handles.push((pos, point));
                }
            }
        }

        handles
    }

    /// Hit-tests a screen point against the handles of a drawing.
    ///
    /// Returns `Some(HandlePos)` if the point is within `handle_size * 1.5`
    /// pixels of a handle center, `None` otherwise.
    pub fn hit_test_handle(
        &self,
        point: Pos2,
        drawing: &Drawing,
        handle_size: f32,
    ) -> Option<HandlePos> {
        let handles = self.get_handles(drawing);
        let hit_distance = handle_size * 1.5;

        for (pos, handle_point) in handles {
            let dist =
                ((point.x - handle_point.x).powi(2) + (point.y - handle_point.y).powi(2)).sqrt();
            if dist <= hit_distance {
                return Some(pos);
            }
        }
        None
    }

    /// Begins a handle drag operation, saving the drawing's current state for undo.
    pub fn start_drag_handle(&mut self, drawing: &Drawing, handle: HandlePos) {
        self.drag_old_state = Some(drawing.clone());
        self.dragging_handle = Some((drawing.id, handle));
    }

    /// Takes the pre-drag drawing state for undo purposes. Consumes the stored
    /// snapshot, returning `None` on subsequent calls until a new drag begins.
    pub fn take_drag_old_state(&mut self) -> Option<Drawing> {
        self.drag_old_state.take()
    }

    /// Ends the handle drag operation, returning the pre-drag drawing state
    /// (for undo) and clearing the drag state.
    pub fn end_drag_handle(&mut self) -> Option<Drawing> {
        self.dragging_handle = None;
        self.drag_old_state.take()
    }

    /// Returns `true` if a handle drag is currently in progress.
    pub fn is_dragging(&self) -> bool {
        self.dragging_handle.is_some()
    }

    /// Returns the current drag target: `(drawing_id, handle_position)`, or
    /// `None` if no drag is active.
    pub fn get_drag_target(&self) -> Option<(usize, HandlePos)> {
        self.dragging_handle
    }

    /// Renders selection handles for a drawing as filled squares with borders.
    ///
    /// The actively dragged handle is highlighted with [`DrawingOptions::handle_sel_color`].
    pub fn render_handles(
        &self,
        painter: &egui::Painter,
        drawing: &Drawing,
        options: &DrawingOptions,
    ) {
        if !options.show_handles {
            return;
        }

        let handles = self.get_handles(drawing);
        let size = options.handle_size;

        for (pos, point) in handles {
            let is_dragging = self
                .dragging_handle
                .map(|(id, h)| id == drawing.id && h == pos)
                .unwrap_or(false);

            let color = if is_dragging {
                Color32::from_rgba_unmultiplied(
                    options.handle_sel_color[0],
                    options.handle_sel_color[1],
                    options.handle_sel_color[2],
                    options.handle_sel_color[3],
                )
            } else {
                Color32::from_rgba_unmultiplied(
                    options.handle_color[0],
                    options.handle_color[1],
                    options.handle_color[2],
                    options.handle_color[3],
                )
            };

            // Draw handle as filled square with border
            let rect = Rect::from_center_size(point, egui::vec2(size, size));
            painter.rect_filled(rect, 1.0, color);
            painter.rect_stroke(
                rect,
                1.0,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.extended.chart_bg,
                ),
                StrokeKind::Outside,
            );
        }
    }

    /// Hit-tests a screen point against all visible drawings, from the top layer down
    /// (descending z-order, and the later drawing first where the z-order is the same).
    ///
    /// Returns the ID of the topmost drawing at `point`, or `None` if no drawing
    /// was hit. Respects visibility and per-timeframe visibility filtering.
    /// Uses tool-specific hit testing (line distance, rectangle containment,
    /// Fibonacci level proximity, etc.) with a 5-pixel tolerance.
    pub fn hit_test(&self, point: Pos2, drawings: &[Drawing], timeframe: &str) -> Option<usize> {
        let hit_distance = 5.0;

        for drawing in super::in_z_order(drawings).into_iter().rev() {
            // Skip hidden drawings
            if !drawing.visible {
                continue;
            }

            // Skip drawings not visible on current timeframe
            if !drawing.timeframe_visibility.is_visible_on(timeframe) {
                continue;
            }

            match drawing.tool_type {
                DrawingToolType::HorizontalLine => {
                    if !drawing.points.is_empty()
                        && (point.y - drawing.points[0].y).abs() <= hit_distance
                    {
                        return Some(drawing.id);
                    }
                }
                DrawingToolType::VerticalLine => {
                    if !drawing.points.is_empty()
                        && (point.x - drawing.points[0].x).abs() <= hit_distance
                    {
                        return Some(drawing.id);
                    }
                }
                DrawingToolType::Rect => {
                    if drawing.points.len() >= 2 {
                        let rect = Rect::from_two_pos(drawing.points[0], drawing.points[1]);
                        if rect.contains(point) {
                            return Some(drawing.id);
                        }
                    }
                }
                // Fibonacci tools
                // The level lines the renderer draws, over the extent it draws them
                DrawingToolType::FibonacciRetracement | DrawingToolType::FibonacciExtension => {
                    if let Some((config, _)) = drawing.fib_levels()
                        && drawing.points.len() >= 2
                        && drawing
                            .fib_level_x_range(&config, Rect::EVERYTHING)
                            .contains(point.x)
                        && drawing
                            .level_ys
                            .iter()
                            .any(|y| (point.y - y).abs() <= hit_distance)
                    {
                        return Some(drawing.id);
                    }
                }
                DrawingToolType::FibonacciChannel => {
                    if drawing.fib_channel_level_lines().is_some_and(|(_, lines)| {
                        lines
                            .iter()
                            .any(|&[a, b]| point_to_line_distance(point, a, b) <= hit_distance)
                    }) {
                        return Some(drawing.id);
                    }
                }
                // Parallel channel
                DrawingToolType::ParallelChannel => {
                    if drawing.points.len() >= 3 {
                        let p1 = drawing.points[0];
                        let p2 = drawing.points[1];

                        let dist1 = point_to_line_distance(point, p1, p2);
                        if dist1 <= hit_distance {
                            return Some(drawing.id);
                        }

                        if let Some([p4, p5]) = drawing.parallel_channel_second_line()
                            && point_to_line_distance(point, p4, p5) <= hit_distance
                        {
                            return Some(drawing.id);
                        }
                    }
                }
                _ => {
                    // Lines that run to the chart's edge are cut at a box around the anchors and
                    // the point, which holds every part of a line within reach of the point.
                    let reach = Rect::from_points(&drawing.points)
                        .union(Rect::from_pos(point))
                        .expand(2.0 * hit_distance);
                    if let Some(outline) = drawing.outline(reach) {
                        let tolerance = hit_distance.max(outline.width / 2.0);
                        let on_line = outline
                            .lines
                            .iter()
                            .any(|&[a, b]| point_to_line_distance(point, a, b) <= tolerance);
                        if on_line
                            || outline
                                .areas
                                .iter()
                                .any(|area| convex_contains(area, point))
                        {
                            return Some(drawing.id);
                        }
                        continue;
                    }
                    // The other tools: proximity to any anchor
                    for &p in &drawing.points {
                        let dist = ((point.x - p.x).powi(2) + (point.y - p.y).powi(2)).sqrt();
                        if dist <= hit_distance * 2.0 {
                            return Some(drawing.id);
                        }
                    }
                }
            }
        }
        None
    }

    /// Hit-tests at `point` and selects the topmost drawing found.
    ///
    /// Returns `true` if a drawing was selected, `false` if the click hit empty
    /// space (which deselects the current selection).
    pub fn select_at(&mut self, point: Pos2, drawings: &[Drawing], timeframe: &str) -> bool {
        if let Some(id) = self.hit_test(point, drawings, timeframe) {
            self.sel_drawing = Some(id);
            true
        } else {
            self.sel_drawing = None;
            false
        }
    }
}

/// Whether `point` lies inside the convex polygon `area`, wound either way. An
/// area with no extent across has no inside: its edges are the only part hit.
fn convex_contains(area: &[Pos2], point: Pos2) -> bool {
    let sides = area.len();
    let crosses = (0..sides).map(|i| {
        let (edge, to_point) = (area[(i + 1) % sides] - area[i], point - area[i]);
        edge.x * to_point.y - edge.y * to_point.x
    });
    let (mut left, mut right) = (false, false);
    for cross in crosses {
        left |= cross > 0.0;
        right |= cross < 0.0;
    }
    sides >= 3 && (left || right) && !(left && right)
}

/// Calculates the perpendicular distance from `point` to the closest point on
/// the line segment `[line_start, line_end]`.
///
/// The projection is clamped to the segment endpoints. Returns the Euclidean
/// distance to `line_start` if the segment is degenerate (zero-length).
pub fn point_to_line_distance(point: Pos2, line_start: Pos2, line_end: Pos2) -> f32 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-6 {
        // Line is a point
        return ((point.x - line_start.x).powi(2) + (point.y - line_start.y).powi(2)).sqrt();
    }

    // Project point onto line
    let t = ((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let proj_x = line_start.x + t * dx;
    let proj_y = line_start.y + t * dy;

    ((point.x - proj_x).powi(2) + (point.y - proj_y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::ChartPoint;

    #[test]
    fn test_point_to_line_distance() {
        // Point on line
        let dist = point_to_line_distance(
            Pos2::new(50.0, 50.0),
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 100.0),
        );
        assert!(dist < 0.01);

        // Point perpendicular to line
        let dist = point_to_line_distance(
            Pos2::new(50.0, 60.0),
            Pos2::new(0.0, 50.0),
            Pos2::new(100.0, 50.0),
        );
        assert!((dist - 10.0).abs() < 0.01);
    }

    /// Anchors at y 400 then 100: the 23.6% level is at 100 + 300 * 0.236 = 170.8, or at
    /// 400 - 300 * 0.236 = 329.2 when reversed.
    #[test]
    fn fib_retracement_hit_follows_level_orientation() {
        let interaction = DrawingInteraction::new();
        let mut fib = Drawing::new(7, DrawingToolType::FibonacciRetracement);
        fib.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 400.0)];
        let hit = |fib: &mut Drawing, y| {
            fib.update_screen_coords(|bar| bar * 30.0, |price| 500.0 - price as f32);
            interaction.hit_test(Pos2::new(150.0, y), std::slice::from_ref(fib), "1D")
        };

        assert_eq!(hit(&mut fib, 170.8), Some(7));
        fib.fib_config = Some(crate::drawings::FibonacciConfig {
            reverse: true,
            ..Default::default()
        });
        assert_eq!(hit(&mut fib, 170.8), None);
        assert_eq!(hit(&mut fib, 329.2), Some(7));
    }

    /// Prices 10 then 1000 on a logarithmic axis over 5 to 2000 and 500 px: a level is hit at the
    /// y of its price, not at the interpolation of the anchors' y. The retracement's 0.5 level
    /// is the price 505, at y 114.86; the extension's 0.618 level is 10 + 990 * 0.618 = 621.82,
    /// at y 97.49. The anchors' interpolation puts the levels at 250 and 204.6.
    #[test]
    fn fib_level_hits_follow_the_price_scale() {
        let interaction = DrawingInteraction::new();
        let price_to_y =
            |price: f64| (500.0 * (2000f64 / price).ln() / (2000f64 / 5.0).ln()) as f32;
        for (tool, y) in [
            (DrawingToolType::FibonacciRetracement, 114.86),
            (DrawingToolType::FibonacciExtension, 97.49),
        ] {
            let mut fib = Drawing::new(7, tool);
            fib.chart_points = vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(10.0, 1000.0)];
            fib.update_screen_coords(|bar| bar * 30.0, price_to_y);
            let hit = interaction.hit_test(Pos2::new(150.0, y), std::slice::from_ref(&fib), "1D");
            assert_eq!(hit, Some(7), "{tool:?}");
        }
    }

    /// Anchors at bars 0 and 10, prices 100 and 400, on x = 30 per bar and y = 500 - price: the
    /// first anchor is at (0, 400) and the second at (300, 100). A retracement level price is
    /// 400 - 300 * level, at y = 100 + 300 * level; with `extend_right` its lines run on past
    /// x = 300. The point (90, 310) lies on the diagonal between the anchors, which is not drawn,
    /// and 24.6 px from the nearest level (78.6% at 335.8 is 25.8 px away; 61.8% at 285.4).
    /// A channel with its third anchor at bar 5, price 150, (150, 350), moves the base line
    /// 100 px straight down: level 0.5 runs from (0, 450) to (300, 150), so y = 450 - x, and
    /// with `extend_right` on to x = 400 at y = 50. Its drawn levels cross x = 150 between
    /// y = 250 and 350, far from (150, 214.6), where the 61.8% level of a horizontal channel was.
    #[test]
    fn fib_drawings_are_hit_along_the_levels_they_draw() {
        use crate::drawings::FibonacciConfig;
        let interaction = DrawingInteraction::new();
        let extended = || {
            Some(FibonacciConfig {
                extend_right: true,
                ..Default::default()
            })
        };
        let cases = [
            (
                DrawingToolType::FibonacciRetracement,
                None,
                (450.0, 170.8),
                None,
            ),
            (
                DrawingToolType::FibonacciRetracement,
                extended(),
                (450.0, 170.8),
                Some(7),
            ),
            (
                DrawingToolType::FibonacciRetracement,
                None,
                (90.0, 310.0),
                None,
            ),
            (
                DrawingToolType::FibonacciExtension,
                None,
                (90.0, 310.0),
                None,
            ),
            (
                DrawingToolType::FibonacciChannel,
                None,
                (150.0, 300.0),
                Some(7),
            ),
            (
                DrawingToolType::FibonacciChannel,
                None,
                (150.0, 214.6),
                None,
            ),
            (
                DrawingToolType::FibonacciChannel,
                extended(),
                (400.0, 50.0),
                Some(7),
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, config, (x, y), want) in cases {
            let mut fib = Drawing::new(7, tool);
            fib.fib_config = config;
            fib.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 400.0)];
            if tool == DrawingToolType::FibonacciChannel {
                fib.chart_points.push(ChartPoint::new(5.0, 150.0));
            }
            fib.update_screen_coords(|bar| bar * 30.0, |price| 500.0 - price as f32);
            let extend = fib.fib_config.as_ref().is_some_and(|c| c.extend_right);
            let got = interaction.hit_test(Pos2::new(x, y), std::slice::from_ref(&fib), "1D");
            if got != want {
                wrong.push(format!(
                    "{tool:?} extend_right {extend} at ({x}, {y}): got {got:?}, want {want:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// A stored drawing has its chart anchors but no screen points until the next
    /// `update_screen_coords`; until then no tool is hit, and none panics.
    #[test]
    fn drawings_without_screen_points_are_not_hit() {
        let interaction = DrawingInteraction::new();
        let hit: Vec<_> = DrawingToolType::all()
            .iter()
            .filter(|&&tool| {
                let mut drawing = Drawing::new(7, tool);
                drawing.chart_points = (0..5)
                    .map(|i| ChartPoint::new(i as f32, 100.0 + f64::from(i)))
                    .collect();
                interaction
                    .hit_test(Pos2::new(1.0, 1.0), std::slice::from_ref(&drawing), "1D")
                    .is_some()
            })
            .collect();
        assert!(hit.is_empty(), "hit without screen points: {hit:?}");
    }

    /// Each point is at least 40 px from every anchor. A ray from (0, 0) through (100, 50) runs on
    /// through (300, 150) and not back through (-100, -50); one from (500, 100) through
    /// (400, 150) runs left through (200, 250). The cross line through (100, 100) runs
    /// down through (100, 300). The pattern's second leg runs from
    /// (100, 100) to (200, 0) through (150, 50). The triangle's edge from (200, 0) to (100, 200)
    /// passes (150, 100); without a fill its inside is empty. The circle around (100, 100)
    /// through (150, 100) passes (100, 150), and its centre is empty. The pitchfork's median
    /// leaves (0, 100) through the midpoint (100, 100) of the other anchors and runs on
    /// flat. The Gann fan's 1x2 line from (0, 200), half the 1x1 slope to (100, 100), is at
    /// y = 200 - 300 / 2 = 50 at x = 300. The measure fills its box, which holds (20, 80), 42 px
    /// from its diagonal. A measure across bars at one price, from (0, 100) to (100, 100), is hit
    /// 2 px off its border at (50, 102), but not 400 px past its end on the same line. A
    /// highlighter is drawn 8 times its stroke width of 2, 16 px wide, so a point 7 px off its
    /// line is on it. The date range fills the band between x = 100 and 200 at any height.
    /// The ellipse in the box from (0, 0) to (200, 100) is lowest at (100, 100). Every drawing
    /// extends both ways where its tool can: the trend line from (100, 100) to (200, 150) runs on
    /// through (500, 300) and (0, 50).
    #[test]
    fn drawings_are_hit_along_what_they_draw() {
        use DrawingToolType as T;
        let interaction = DrawingInteraction::new();
        // (tool, point, expected hit, anchors)
        let cases = [
            (
                T::Ray,
                (300.0, 150.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 50.0)],
            ),
            (
                T::Ray,
                (-100.0, -50.0),
                None,
                vec![(0.0, 0.0), (100.0, 50.0)],
            ),
            (
                T::TrendLine,
                (500.0, 300.0),
                Some(7),
                vec![(100.0, 100.0), (200.0, 150.0)],
            ),
            (
                T::TrendLine,
                (0.0, 50.0),
                Some(7),
                vec![(100.0, 100.0), (200.0, 150.0)],
            ),
            (
                T::Ray,
                (200.0, 250.0),
                Some(7),
                vec![(500.0, 100.0), (400.0, 150.0)],
            ),
            (T::CrossLine, (100.0, 300.0), Some(7), vec![(100.0, 100.0)]),
            (
                T::XABCDPattern,
                (150.0, 50.0),
                Some(7),
                vec![
                    (0.0, 0.0),
                    (100.0, 100.0),
                    (200.0, 0.0),
                    (300.0, 100.0),
                    (400.0, 0.0),
                ],
            ),
            (
                T::Triangle,
                (150.0, 100.0),
                Some(7),
                vec![(0.0, 0.0), (200.0, 0.0), (100.0, 200.0)],
            ),
            (
                T::Triangle,
                (100.0, 50.0),
                None,
                vec![(0.0, 0.0), (200.0, 0.0), (100.0, 200.0)],
            ),
            (
                T::Circle,
                (100.0, 150.0),
                Some(7),
                vec![(100.0, 100.0), (150.0, 100.0)],
            ),
            (
                T::Circle,
                (100.0, 100.0),
                None,
                vec![(100.0, 100.0), (150.0, 100.0)],
            ),
            (
                T::Pitchfork,
                (400.0, 100.0),
                Some(7),
                vec![(0.0, 100.0), (100.0, 50.0), (100.0, 150.0)],
            ),
            (
                T::GannFan,
                (300.0, 50.0),
                Some(7),
                vec![(0.0, 200.0), (100.0, 100.0)],
            ),
            (
                T::Measure,
                (20.0, 80.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 100.0)],
            ),
            (
                T::Measure,
                (50.0, 102.0),
                Some(7),
                vec![(0.0, 100.0), (100.0, 100.0)],
            ),
            (
                T::Measure,
                (500.0, 100.0),
                None,
                vec![(0.0, 100.0), (100.0, 100.0)],
            ),
            (
                T::Highlighter,
                (50.0, 7.0),
                Some(7),
                vec![(0.0, 0.0), (100.0, 0.0)],
            ),
            (
                T::DateRange,
                (150.0, 400.0),
                Some(7),
                vec![(100.0, 50.0), (200.0, 80.0)],
            ),
            (
                T::Ellipse,
                (100.0, 100.0),
                Some(7),
                vec![(0.0, 0.0), (200.0, 100.0)],
            ),
            (
                T::Ellipse,
                (100.0, 50.0),
                None,
                vec![(0.0, 0.0), (200.0, 100.0)],
            ),
        ];
        let mut wrong = Vec::new();
        for (tool, (x, y), want, anchors) in cases {
            let mut drawing = Drawing::new(7, tool);
            drawing.points = anchors.iter().map(|&(x, y)| Pos2::new(x, y)).collect();
            (drawing.extend_left, drawing.extend_right) = (true, true);
            let got = interaction.hit_test(Pos2::new(x, y), std::slice::from_ref(&drawing), "1D");
            if got != want {
                wrong.push(format!(
                    "{tool:?} at ({x}, {y}): got {got:?}, want {want:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn test_selection() {
        let mut interaction = DrawingInteraction::new();

        assert!(interaction.sel_drawing.is_none());

        interaction.sel_drawing = Some(5);
        assert_eq!(interaction.sel_drawing, Some(5));

        interaction.deselect();
        assert!(interaction.sel_drawing.is_none());
    }
}
