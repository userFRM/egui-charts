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

    /// Hit-tests a screen point against all visible drawings (back-to-front order).
    ///
    /// Returns the ID of the topmost drawing at `point`, or `None` if no drawing
    /// was hit. Respects visibility and per-timeframe visibility filtering.
    /// Uses tool-specific hit testing (line distance, rectangle containment,
    /// Fibonacci level proximity, etc.) with a 5-pixel tolerance.
    pub fn hit_test(&self, point: Pos2, drawings: &[Drawing], timeframe: &str) -> Option<usize> {
        let hit_distance = 5.0;

        for drawing in drawings.iter().rev() {
            // Skip hidden drawings
            if !drawing.visible {
                continue;
            }

            // Skip drawings not visible on current timeframe
            if !drawing.timeframe_visibility.is_visible_on(timeframe) {
                continue;
            }

            match drawing.tool_type {
                DrawingToolType::TrendLine | DrawingToolType::Measure => {
                    if drawing.points.len() >= 2 {
                        let dist =
                            point_to_line_distance(point, drawing.points[0], drawing.points[1]);
                        if dist <= hit_distance {
                            return Some(drawing.id);
                        }
                    }
                }
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
                DrawingToolType::FibonacciRetracement
                | DrawingToolType::FibonacciExtension
                | DrawingToolType::FibonacciChannel => {
                    if drawing.points.len() >= 2 {
                        let p1 = drawing.points[0];
                        let p2 = drawing.points[1];

                        // Check main diagonal line
                        let dist = point_to_line_distance(point, p1, p2);
                        if dist <= hit_distance {
                            return Some(drawing.id);
                        }

                        // Check horizontal Fibonacci levels
                        let levels = [0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0];
                        for &level in &levels {
                            let y = p1.y + (p2.y - p1.y) * level;
                            if (point.y - y).abs() <= hit_distance {
                                let min_x = p1.x.min(p2.x);
                                let max_x = p1.x.max(p2.x);
                                if point.x >= min_x && point.x <= max_x {
                                    return Some(drawing.id);
                                }
                            }
                        }
                    }
                }
                // Gann tools
                DrawingToolType::GannFan => {
                    if drawing.points.len() >= 2 {
                        let p1 = drawing.points[0];
                        let p2 = drawing.points[1];

                        let angles = [1.0, 0.5, 0.333, 0.25, 2.0, 3.0, 4.0, 8.0];
                        for &angle in &angles {
                            let dy = (p2.y - p1.y) * angle;
                            let end_point = Pos2::new(p2.x, p1.y + dy);
                            let dist = point_to_line_distance(point, p1, end_point);
                            if dist <= hit_distance {
                                return Some(drawing.id);
                            }
                        }
                    }
                }
                // Parallel channel
                DrawingToolType::ParallelChannel => {
                    if drawing.points.len() >= 3 {
                        let p1 = drawing.points[0];
                        let p2 = drawing.points[1];
                        let p3 = drawing.points[2];

                        let dist1 = point_to_line_distance(point, p1, p2);
                        if dist1 <= hit_distance {
                            return Some(drawing.id);
                        }

                        // Calculate parallel line
                        let dx = p2.x - p1.x;
                        let dy = p2.y - p1.y;
                        let len_sq = dx * dx + dy * dy;
                        if len_sq >= 1e-6 {
                            let t = ((p3.x - p1.x) * dx + (p3.y - p1.y) * dy) / len_sq;
                            let proj_x = p1.x + t * dx;
                            let proj_y = p1.y + t * dy;
                            let offset_x = p3.x - proj_x;
                            let offset_y = p3.y - proj_y;
                            let p4 = Pos2::new(p1.x + offset_x, p1.y + offset_y);
                            let p5 = Pos2::new(p2.x + offset_x, p2.y + offset_y);
                            let dist2 = point_to_line_distance(point, p4, p5);
                            if dist2 <= hit_distance {
                                return Some(drawing.id);
                            }
                        }
                    }
                }
                _ => {
                    // Check proximity to any point
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
