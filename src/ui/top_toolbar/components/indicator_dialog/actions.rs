//! Actions emitted by the indicator dialog

use super::data::ConfiguredIndicator;

/// Actions emitted by the indicator dialog
pub enum IndicatorDialogAction {
    /// No action
    None,
    /// Add indicator with configuration
    AddConfiguredIndicator(ConfiguredIndicator),
    /// Toggle favorite status
    ToggleFavorite(String),
    /// Dialog closed
    Close,
}

impl IndicatorDialogAction {
    /// Check if this action is adding an indicator
    pub fn is_add(&self) -> bool {
        matches!(self, IndicatorDialogAction::AddConfiguredIndicator(_))
    }
}
