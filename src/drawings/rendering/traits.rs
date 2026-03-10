//! Drawing renderer trait and rendering utilities.
//!
//! Defines the [`DrawingRenderer`] trait interface for rendering drawing tools,
//! plus helper types ([`RenderContext`], [`HandleHit`]) and utility functions
//! ([`render_handles`], [`point_near_line`], [`point_near_points`]) used by
//! the category-specific renderer implementations.

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Stroke};

/// Trait for rendering drawing tools
///
/// This trait provides the interface for drawing tools to render themselves
/// onto an egui Painter. Implementations handle the specific rendering logic
/// for each tool type.
pub trait DrawingRenderer {
    /// Render the drawing onto the painter
    ///
    /// # Arguments
    ///
    /// * `painter` - The egui painter to draw on
    /// * `chart_rect` - The visible chart area rectangle
    /// * `is_selected` - Whether this drawing is currently selected
    /// * `is_hovered` - Whether this drawing is being hovered
    fn render(&self, painter: &Painter, chart_rect: Rect, is_selected: bool, is_hovered: bool);

    /// Get the bounding box of the drawing
    ///
    /// Returns None if the drawing has no points or cannot be bounded.
    fn bounding_box(&self) -> Option<Rect>;

    /// Check if a point hits this drawing
    ///
    /// # Arguments
    ///
    /// * `point` - Screen position to test
    /// * `tolerance` - Hit test tolerance in pixels
    fn hit_test(&self, point: Pos2, tolerance: f32) -> bool;

    /// Get the anchor points of the drawing
    ///
    /// Returns the screen positions of all anchor points (for handles).
    fn anchor_points(&self) -> Vec<Pos2>;
}

/// Result of a handle hit test on a rendered drawing.
///
/// Used by the trait-based rendering system to distinguish between hitting a
/// specific anchor point, hitting the body of the drawing (for move operations),
/// or missing entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleHit {
    /// No handle or body was hit.
    None,
    /// Hit a specific anchor point handle, identified by its index.
    Point(usize),
    /// Hit the body/interior of the drawing (for move/drag operations).
    Body,
}

/// Rendering context carrying commonly needed values for drawing renderers.
///
/// Bundles the egui [`Painter`], visible chart rectangle, default stroke/fill,
/// and handle configuration into a single value that can be passed through the
/// rendering pipeline. Use the builder methods ([`with_stroke`](Self::with_stroke),
/// [`with_fill`](Self::with_fill), [`with_handles`](Self::with_handles)) to
/// customize before passing to a renderer.
#[derive(Clone)]
pub struct RenderContext<'a> {
    /// The egui painter to draw on.
    pub painter: &'a Painter,
    /// The visible chart area rectangle (clip rect for drawing).
    pub chart_rect: Rect,
    /// Default stroke style for lines and borders.
    pub stroke: Stroke,
    /// Default fill color for shapes.
    pub fill_color: Color32,
    /// Whether to render selection handles.
    pub show_handles: bool,
    /// Handle size in pixels.
    pub handle_size: f32,
}

impl<'a> RenderContext<'a> {
    /// Create a new render context
    pub fn new(painter: &'a Painter, chart_rect: Rect) -> Self {
        Self {
            painter,
            chart_rect,
            stroke: Stroke::new(
                DESIGN_TOKENS.stroke.medium,
                DESIGN_TOKENS.semantic.extended.info,
            ),
            fill_color: {
                let info = DESIGN_TOKENS.semantic.extended.info;
                Color32::from_rgba_unmultiplied(info.r(), info.g(), info.b(), 50)
            },
            show_handles: false,
            handle_size: 8.0,
        }
    }

    /// Set the stroke
    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = stroke;
        self
    }

    /// Set the fill color
    pub fn with_fill(mut self, color: Color32) -> Self {
        self.fill_color = color;
        self
    }

    /// Enable handle rendering
    pub fn with_handles(mut self, show: bool, size: f32) -> Self {
        self.show_handles = show;
        self.handle_size = size;
        self
    }
}

/// Helper function to render selection handles
pub fn render_handles(painter: &Painter, points: &[Pos2], size: f32, selected: bool) {
    if !selected || points.is_empty() {
        return;
    }

    let handle_color = DESIGN_TOKENS.semantic.extended.accent; // Brand blue
    let border_color = Color32::WHITE;
    let radius = size / 2.0;

    for &point in points {
        // White border
        painter.circle_filled(point, radius + 1.0, border_color);
        // Blue fill
        painter.circle_filled(point, radius, handle_color);
    }
}

/// Helper function to check if a point is near a line segment
pub fn point_near_line(point: Pos2, line_start: Pos2, line_end: Pos2, tolerance: f32) -> bool {
    let line_vec = line_end - line_start;
    let line_len_sq = line_vec.length_sq();

    if line_len_sq < 1e-6 {
        // Degenerate line (points are same)
        return (point - line_start).length() <= tolerance;
    }

    // Project point onto line
    let t = ((point - line_start).dot(line_vec) / line_len_sq).clamp(0.0, 1.0);
    let projection = line_start + t * line_vec;
    let distance = (point - projection).length();

    distance <= tolerance
}

/// Helper function to check if a point is near any point in a list
pub fn point_near_points(point: Pos2, points: &[Pos2], tolerance: f32) -> Option<usize> {
    for (i, &p) in points.iter().enumerate() {
        if (point - p).length() <= tolerance {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_near_line() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(100.0, 0.0);

        // Point on line
        assert!(point_near_line(Pos2::new(50.0, 0.0), start, end, 5.0));

        // Point near line
        assert!(point_near_line(Pos2::new(50.0, 3.0), start, end, 5.0));

        // Point far from line
        assert!(!point_near_line(Pos2::new(50.0, 10.0), start, end, 5.0));

        // Point beyond line segment (near start)
        assert!(point_near_line(Pos2::new(-3.0, 0.0), start, end, 5.0));

        // Point beyond line segment (far from start)
        assert!(!point_near_line(Pos2::new(-10.0, 0.0), start, end, 5.0));
    }

    #[test]
    fn test_point_near_points() {
        let points = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 100.0),
            Pos2::new(50.0, 50.0),
        ];

        // Near first point
        assert_eq!(
            point_near_points(Pos2::new(3.0, 0.0), &points, 5.0),
            Some(0)
        );

        // Near third point
        assert_eq!(
            point_near_points(Pos2::new(52.0, 50.0), &points, 5.0),
            Some(2)
        );

        // Not near any point
        assert_eq!(
            point_near_points(Pos2::new(200.0, 200.0), &points, 5.0),
            None
        );
    }
}
