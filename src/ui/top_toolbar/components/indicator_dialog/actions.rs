//! Actions emitted by the indicator dialog

use crate::studies::Indicator;

/// Actions emitted by the indicator dialog
#[derive(Clone)]
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

impl std::fmt::Debug for IndicatorDialogAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::AddIndicator(study) => {
                f.debug_tuple("AddIndicator").field(&study.name()).finish()
            }
            Self::ToggleFavorite(name) => f.debug_tuple("ToggleFavorite").field(name).finish(),
            Self::Close => f.write_str("Close"),
        }
    }
}

impl IndicatorDialogAction {
    /// Check if this action is adding an indicator
    pub fn is_add(&self) -> bool {
        matches!(self, IndicatorDialogAction::AddIndicator(_))
    }
}
