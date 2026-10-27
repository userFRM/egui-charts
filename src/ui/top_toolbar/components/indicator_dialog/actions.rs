//! Actions emitted by the indicator dialog

use crate::studies::Indicator;

/// Actions emitted by the indicator dialog
#[non_exhaustive]
pub enum IndicatorDialogAction {
    /// No action
    None,
    /// Add this study, created through the dialog's factory with its default inputs and any
    /// source the dialog was opened with
    AddIndicator(Box<dyn Indicator>),
    /// Toggle favorite status
    ToggleFavorite(String),
    /// Dialog closed
    Close,
}

impl IndicatorDialogAction {
    /// Check if this action is adding an indicator
    pub fn is_add(&self) -> bool {
        matches!(self, IndicatorDialogAction::AddIndicator(_))
    }
}
