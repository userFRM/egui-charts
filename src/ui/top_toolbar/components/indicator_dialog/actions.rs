//! Actions emitted by the indicator dialog

use crate::studies::Indicator;

/// Actions emitted by the indicator dialog
pub enum IndicatorDialogAction {
    /// No action
    None,
    /// Add this study, created through the dialog's factory with the parameters set in the dialog
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
