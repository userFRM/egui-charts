//! Actions that can be returned from bottom panel interactions

use super::{DateRange, SessionType};
use crate::model::Timeframe;
use chrono::{DateTime, Utc};

/// Actions returned from bottom panel interactions
#[derive(Debug, Clone, PartialEq)]
pub enum TimeframeToolbarAction {
    /// No action taken
    None,
    /// Timeframe changed
    TimeframeChanged(Timeframe),
    /// Date range preset changed
    DateRangeChanged(DateRange),
    /// Go to specific date
    GoToDate(DateTime<Utc>),
    /// Timezone changed
    TimezoneChanged(String),
    /// Session type changed (RTH/ETH/24x7)
    SessionChanged(SessionType),
    /// Adjust for dividends toggled
    AdjustDividendsToggled(bool),
    /// Zoom to fit all data
    ZoomFitAll,
    /// Zoom to specific range
    ZoomToRange(String),
    /// Open date picker dialog
    OpenDatePicker,
    /// Open timezone menu
    OpenTimezoneMenu,
    /// Open session menu
    OpenSessionMenu,
    /// Toggle trading panel row expanded/collapsed
    ToggleTradingPanel,
    /// Open trading panel fullscreen
    TradingPanelFullscreen,
}
