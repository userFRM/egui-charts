//! Series hit-test result types.
//!
//! Series selection itself is tracked by the chart-wide
//! [`SelectionState<ChartElementId>`](crate::chart::selection::SelectionState),
//! so this module only carries the result of a hit test on chart series. The
//! [`SeriesId`] identity lives in [`crate::chart::selection`] and is re-exported
//! here for the series API surface.

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
