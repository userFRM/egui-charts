//! Sparse study boxes with fractional logical endpoints, including future whitespace.

/// A projected price band whose endpoints can fall between bar centres.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyProjection {
    /// Logical endpoints, in the same coordinates as the chart's time axis.
    pub bars: std::ops::Range<f64>,
    /// Upper price.
    pub top: f64,
    /// Lower price.
    pub bottom: f64,
    /// Output supplying visibility, stroke and colour.
    pub plot: usize,
}
