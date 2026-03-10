//! Shared hit-test utilities for chart elements.
//!
//! Provides common functions and constants used by both series and indicator
//! hit testing.

use egui::Pos2;

/// Unified hit tolerance in pixels for line/segment detection.
///
/// Used for detecting clicks on lines, wicks, and indicator curves.
pub const HIT_TOLERANCE: f32 = 5.0;

/// Calculate distance from a point to a line segment.
///
/// Uses perpendicular projection for accurate distance calculation.
/// Returns the shortest distance from the point to any point on the segment.
#[inline]
pub fn point_to_segment_distance(point: Pos2, seg_start: Pos2, seg_end: Pos2) -> f32 {
    let dx = seg_end.x - seg_start.x;
    let dy = seg_end.y - seg_start.y;
    let seg_len_sq = dx * dx + dy * dy;

    if seg_len_sq < 0.0001 {
        // Segment is effectively a point
        return ((point.x - seg_start.x).powi(2) + (point.y - seg_start.y).powi(2)).sqrt();
    }

    // Project point onto line and clamp to segment
    let t = ((point.x - seg_start.x) * dx + (point.y - seg_start.y) * dy) / seg_len_sq;
    let t = t.clamp(0.0, 1.0);

    let proj_x = seg_start.x + t * dx;
    let proj_y = seg_start.y + t * dy;

    ((point.x - proj_x).powi(2) + (point.y - proj_y).powi(2)).sqrt()
}

/// Check if a point is near a line segment within the given tolerance.
#[inline]
pub fn is_point_near_segment(point: Pos2, seg_start: Pos2, seg_end: Pos2, tolerance: f32) -> bool {
    point_to_segment_distance(point, seg_start, seg_end) <= tolerance
}

/// Check if a point is near a line segment within the default tolerance.
#[inline]
pub fn is_point_near_segment_default(point: Pos2, seg_start: Pos2, seg_end: Pos2) -> bool {
    is_point_near_segment(point, seg_start, seg_end, HIT_TOLERANCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_on_segment() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(10.0, 0.0);
        let point = Pos2::new(5.0, 0.0);

        let dist = point_to_segment_distance(point, start, end);
        assert!(dist < 0.001);
    }

    #[test]
    fn test_point_perpendicular_to_segment() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(10.0, 0.0);
        let point = Pos2::new(5.0, 3.0);

        let dist = point_to_segment_distance(point, start, end);
        assert!((dist - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_point_beyond_segment_start() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(10.0, 0.0);
        let point = Pos2::new(-3.0, 0.0);

        let dist = point_to_segment_distance(point, start, end);
        assert!((dist - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_point_beyond_segment_end() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(10.0, 0.0);
        let point = Pos2::new(13.0, 0.0);

        let dist = point_to_segment_distance(point, start, end);
        assert!((dist - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_is_near_segment() {
        let start = Pos2::new(0.0, 0.0);
        let end = Pos2::new(10.0, 0.0);

        assert!(is_point_near_segment(Pos2::new(5.0, 4.0), start, end, 5.0));
        assert!(!is_point_near_segment(Pos2::new(5.0, 6.0), start, end, 5.0));
    }
}
