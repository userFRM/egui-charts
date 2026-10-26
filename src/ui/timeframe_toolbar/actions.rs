//! Actions that can be returned from bottom panel interactions

use super::DateRange;

/// Actions returned from bottom panel interactions
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TimeframeToolbarAction {
    /// No action taken
    None,
    /// Date range preset changed
    DateRangeChanged(DateRange),
    /// Adjust for dividends toggled
    AdjustDividendsToggled(bool),
    /// Open date picker dialog
    OpenDatePicker,
    /// Open timezone menu
    OpenTimezoneMenu,
    /// Open session menu
    OpenSessionMenu,
}
