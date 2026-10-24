//! Chart coordinate types
//!
//! ChartPoint exposes editable chart coordinates (bar_idx, price) which remain
//! stable across pan/zoom operations, unlike screen coordinates which change.

/// A persistent point stored as (bar_idx, price) for pan/zoom stability
///
/// A chart retains the timestamp behind each projected index across data changes. Construct
/// points with `ChartPoint::new`; editing `bar_idx` reanchors at that index. Screen coordinates
/// (`Pos2`) are derived from ChartPoint each frame using the current transforms.
///
/// # Example
///
/// ```
/// use egui_charts::drawings::ChartPoint;
///
/// let point = ChartPoint::new(100.0, 150.50);
/// assert_eq!(point.bar_idx, 100.0);
/// assert_eq!(point.price, 150.50);
/// ```
#[derive(Clone, Copy)]
pub struct ChartPoint {
    /// Bar index (can be fractional for points between bars)
    pub bar_idx: f32,
    /// Price level
    pub price: f64,
    /// Time and last projected index; editing the index invalidates the cached time.
    pub(crate) time_anchor: Option<(chrono::DateTime<chrono::Utc>, f32)>,
}

impl ChartPoint {
    /// Create a new chart point
    #[inline]
    pub fn new(bar_idx: f32, price: f64) -> Self {
        Self {
            bar_idx,
            price,
            time_anchor: None,
        }
    }

    /// Create a chart point at origin (0, 0)
    #[inline]
    pub fn zero() -> Self {
        Self {
            bar_idx: 0.0,
            price: 0.0,
            time_anchor: None,
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
            time_anchor: None,
        }
    }

    /// Interpolate between two points
    pub fn lerp(a: ChartPoint, b: ChartPoint, t: f32) -> ChartPoint {
        ChartPoint {
            bar_idx: a.bar_idx + (b.bar_idx - a.bar_idx) * t,
            price: a.price + (b.price - a.price) * t as f64,
            time_anchor: None,
        }
    }
}

impl std::fmt::Debug for ChartPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChartPoint")
            .field("bar_idx", &self.bar_idx)
            .field("price", &self.price)
            .finish()
    }
}

impl PartialEq for ChartPoint {
    fn eq(&self, other: &Self) -> bool {
        self.bar_idx == other.bar_idx && self.price == other.price
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
    fn test_lerp() {
        let a = ChartPoint::new(0.0, 100.0);
        let b = ChartPoint::new(10.0, 200.0);
        let mid = ChartPoint::lerp(a, b, 0.5);
        assert_eq!(mid.bar_idx, 5.0);
        assert_eq!(mid.price, 150.0);
    }
}
