//! Drawing renderer trait and rendering utilities.
//!
//! Re-exports the [`DrawingRenderer`] trait for rendering drawing tools and provides the
//! utility functions [`render_handles`], [`point_near_line`] and [`point_near_points`] for
//! custom renderers.

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2};

pub use crate::drawings::registry::DrawingRenderer;

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
