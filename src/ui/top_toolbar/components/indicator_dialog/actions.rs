//! Actions emitted by the indicator dialog

use super::data::ConfiguredIndicator;

/// Actions emitted by the indicator dialog
pub enum IndicatorDialogAction {
    /// No action
    None,
    /// Add indicator by ID
    AddIndicator(String),
    /// Add indicator with configuration
    AddConfiguredIndicator(ConfiguredIndicator),
    /// Toggle favorite status
    ToggleFavorite(String),
    /// Dialog closed
    Close,
}

impl IndicatorDialogAction {
    /// Check if this action is adding an indicator (either by ID or configured)
    pub fn is_add(&self) -> bool {
        matches!(
            self,
            IndicatorDialogAction::AddIndicator(_)
                | IndicatorDialogAction::AddConfiguredIndicator(_)
        )
    }
}
