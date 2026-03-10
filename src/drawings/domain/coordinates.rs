//! Chart coordinate types
//!
//! ChartPoint stores coordinates in chart space (bar_idx, price) which remain
//! stable across pan/zoom operations, unlike screen coordinates which change.

/// A persistent point stored as (bar_idx, price) for pan/zoom stability
///
/// This is the canonical representation for drawing points. Screen coordinates
/// (`Pos2`) are derived from ChartPoint each frame using the current transforms.
///
/// # Example
///
/// ```ignore
/// use egui_charts::drawings::ChartPoint;
///
/// let point = ChartPoint::new(100.0, 150.50);
/// assert_eq!(point.bar_idx, 100.0);
/// assert_eq!(point.price, 150.50);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPoint {
    /// Bar index (can be fractional for points between bars)
    pub bar_idx: f32,
    /// Price level
    pub price: f64,
}

impl ChartPoint {
    /// Create a new chart point
    #[inline]
    pub fn new(bar_idx: f32, price: f64) -> Self {
        Self { bar_idx, price }
    }

    /// Create a chart point at origin (0, 0)
    #[inline]
    pub fn zero() -> Self {
        Self {
            bar_idx: 0.0,
            price: 0.0,
        }
    }

    /// Shift the bar index by a delta (used when historical data is prepended)
    #[inline]
    pub fn shift_bar_idx(&mut self, delta: f32) {
        self.bar_idx += delta;
    }

    /// Create a new ChartPoint with shifted bar index
    #[inline]
    pub fn with_shifted_bar_idx(self, delta: f32) -> Self {
        Self {
            bar_idx: self.bar_idx + delta,
            price: self.price,
        }
    }

    /// Interpolate between two points
    pub fn lerp(a: ChartPoint, b: ChartPoint, t: f32) -> ChartPoint {
        ChartPoint {
            bar_idx: a.bar_idx + (b.bar_idx - a.bar_idx) * t,
            price: a.price + (b.price - a.price) * t as f64,
        }
    }
}

impl Default for ChartPoint {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let point = ChartPoint::new(100.0, 150.50);
        assert_eq!(point.bar_idx, 100.0);
        assert_eq!(point.price, 150.50);
    }

    #[test]
    fn test_shift_bar_idx() {
        let mut point = ChartPoint::new(100.0, 150.50);
        point.shift_bar_idx(50.0);
        assert_eq!(point.bar_idx, 150.0);
        assert_eq!(point.price, 150.50);
    }

    #[test]
    fn test_lerp() {
        let a = ChartPoint::new(0.0, 100.0);
        let b = ChartPoint::new(10.0, 200.0);
        let mid = ChartPoint::lerp(a, b, 0.5);
        assert_eq!(mid.bar_idx, 5.0);
        assert_eq!(mid.price, 150.0);
    }
}
