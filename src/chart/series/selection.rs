//! Hit-test results for chart series.
//!
//! The chart owns selection; hosts read it with
//! [`Chart::selected_element`](crate::Chart::selected_element).
//! [`SeriesId`] identifies a price or volume series.

pub use crate::chart::selection::SeriesId;

/// Result of a hit test on chart series
#[derive(Clone, Debug)]
pub struct SeriesHitResult {
    /// The series that was hit
    pub series_id: SeriesId,
    /// Index of the bar that was hit
    pub bar_idx: usize,
    /// Screen position of the hit
    pub position: egui::Pos2,
}
