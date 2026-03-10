//! State management for bottom panel
//!
//! Manages the state of the time scale controls and range selection.

use super::DateRange;
use crate::model::Timeframe;

/// Session type for Regular Trading Hours / Extended Hours
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionType {
    /// Regular Trading Hours only
    #[default]
    RTH,
    /// Extended Trading Hours (pre-market + after-hours)
    ETH,
    /// 24/7 (for crypto, forex)
    Full24x7,
}

impl SessionType {
    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            SessionType::RTH => "RTH",
            SessionType::ETH => "ETH",
            SessionType::Full24x7 => "24/7",
        }
    }

    /// Get tooltip desc
    pub fn tooltip(&self) -> &'static str {
        match self {
            SessionType::RTH => "Regular trading hours",
            SessionType::ETH => "Extended trading hours",
            SessionType::Full24x7 => "24/7 trading",
        }
    }
}

/// Bottom panel state
#[derive(Clone, Debug)]
pub struct TimeframeToolbarState {
    /// Current timeframe
    pub timeframe: Timeframe,
    /// Selected date range preset
    pub sel_range: DateRange,
    /// Whether date picker is open
    pub date_picker_open: bool,
    /// Whether date range dropdown is collapsed (mobile mode)
    pub range_collapsed: bool,
    /// Selected timezone display string (e.g., "UTC", "America/New_York")
    pub timezone: String,
    /// Whether timezone menu is open
    pub timezone_menu_open: bool,
    /// Current session type (RTH/ETH/24x7)
    pub session_type: SessionType,
    /// Whether session menu is open
    pub session_menu_open: bool,
    /// Whether to adjust data for dividends
    pub adjust_for_dividends: bool,
    /// Current display time (for timezone display)
    pub curr_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether the trading panel row is expanded
    pub trading_panel_expanded: bool,
}

impl Default for TimeframeToolbarState {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Min1,
            sel_range: DateRange::Month1,
            date_picker_open: false,
            range_collapsed: false,
            timezone: "UTC".to_string(),
            timezone_menu_open: false,
            session_type: SessionType::RTH,
            session_menu_open: false,
            adjust_for_dividends: false,
            curr_time: None,
            trading_panel_expanded: false,
        }
    }
}

impl TimeframeToolbarState {
    /// Toggle trading panel expanded state
    pub fn toggle_trading_panel(&mut self) {
        self.trading_panel_expanded = !self.trading_panel_expanded;
    }

    /// Set trading panel expanded state
    pub fn set_trading_panel_expanded(&mut self, expanded: bool) {
        self.trading_panel_expanded = expanded;
    }
}

impl TimeframeToolbarState {
    /// Create new bottom panel state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set timeframe
    pub fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = timeframe;
    }

    /// Set selected date range
    pub fn set_date_range(&mut self, range: DateRange) {
        self.sel_range = range;
    }

    /// Toggle date picker
    pub fn toggle_date_picker(&mut self) {
        self.date_picker_open = !self.date_picker_open;
    }

    /// Toggle range collapsed mode
    pub fn toggle_range_collapsed(&mut self) {
        self.range_collapsed = !self.range_collapsed;
    }

    /// Set timezone
    pub fn set_timezone(&mut self, timezone: String) {
        self.timezone = timezone;
    }

    /// Toggle timezone menu
    pub fn toggle_timezone_menu(&mut self) {
        self.timezone_menu_open = !self.timezone_menu_open;
    }

    /// Set session type
    pub fn set_session_type(&mut self, session: SessionType) {
        self.session_type = session;
    }

    /// Toggle session menu
    pub fn toggle_session_menu(&mut self) {
        self.session_menu_open = !self.session_menu_open;
    }

    /// Toggle adjust for dividends
    pub fn toggle_adjust_for_dividends(&mut self) {
        self.adjust_for_dividends = !self.adjust_for_dividends;
    }

    /// Update current time
    pub fn update_time(&mut self) {
        self.curr_time = Some(chrono::Utc::now());
    }

    /// Get formatted time string for display
    pub fn formatted_time(&self) -> String {
        match &self.curr_time {
            Some(time) => {
                let formatted = time.format("%H:%M:%S").to_string();
                format!("{} {}", formatted, self.timezone)
            }
            None => format!("--:--:-- {}", self.timezone),
        }
    }

    /// Sync from application state.
    pub fn sync_from_app_state(&mut self, app_state: &dyn crate::ui::app_state::ChartAppState) {
        self.timeframe = *app_state.active_timeframe();
    }
}
