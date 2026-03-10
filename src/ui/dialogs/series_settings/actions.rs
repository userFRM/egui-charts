//! Actions from the series settings dialog.

use crate::chart::series::{SeriesId, SeriesSettings};

/// Actions from the series settings dialog
#[derive(Clone, Debug)]
pub enum SeriesSettingsAction {
    /// No action
    None,
    /// Cancel changes
    Cancel,
    /// Apply settings to series
    Apply(SeriesId, SeriesSettings),
    /// Reset series to default settings
    ResetToDefault(SeriesId),
}
