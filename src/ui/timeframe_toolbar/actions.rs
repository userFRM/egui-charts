//! Actions that can be returned from bottom panel interactions

use crate::model::DateRange;

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
    /// The session control was clicked: set
    /// [`session_menu_open`](super::TimeframeToolbarState::session_menu_open) and call
    /// [`TimeframeToolbar::show_session_menu`](super::TimeframeToolbar::show_session_menu)
    OpenSessionMenu,
}
