//! State management for bottom panel
//!
//! Manages the state of the time scale controls and range selection.

use crate::model::DateRange;
use crate::model::Timeframe;

/// The trading hours the time scale toolbar's session control shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionType {
    /// Regular Trading Hours only
    #[default]
    RTH,
    /// Extended Trading Hours (pre-market + after-hours)
    ETH,
    /// Regular, extended and overnight hours, as
    /// [`Chart::set_overnight_hours`](crate::widget::Chart::set_overnight_hours) requests them.
    Overnight,
    /// 24/7 (for crypto, forex)
    Full24x7,
}

impl SessionType {
    /// Reads the chart's hours, or [`Full24x7`](Self::Full24x7) for an instrument without
    /// session windows or date-specific hours. Without an instrument, reads the hours flags.
    /// Pass this to [`TimeframeToolbar::set_session`](super::TimeframeToolbar::set_session)
    /// before each [`show`](super::TimeframeToolbar::show) so the control follows choices
    /// made elsewhere, including settings and undo.
    pub fn from_chart(chart: &crate::widget::Chart) -> Self {
        if has_trading_hours(chart.instrument()) {
            Self::from_hours(chart.extended_hours(), chart.overnight_hours())
        } else {
            Self::Full24x7
        }
    }

    /// The requested hours; regular hours take precedence, as the chart's setters enforce.
    pub(crate) fn from_hours(extended: bool, overnight: bool) -> Self {
        match (extended, overnight) {
            (false, _) => Self::RTH,
            (true, false) => Self::ETH,
            (true, true) => Self::Overnight,
        }
    }

    /// The choices offered, including overnight hours when available or already selected.
    pub(crate) fn choices(overnight: bool) -> &'static [Self] {
        if overnight {
            &[Self::RTH, Self::ETH, Self::Overnight]
        } else {
            &[Self::RTH, Self::ETH]
        }
    }

    /// The name of this choice in a session selector.
    pub(crate) fn choice_label(self) -> &'static str {
        match self {
            Self::RTH => "Regular trading hours",
            Self::ETH => "Extended trading hours",
            Self::Overnight => "Overnight trading hours",
            Self::Full24x7 => "24/7 trading",
        }
    }

    /// The requested extended and overnight flags, absent for an always-open instrument.
    pub(crate) fn hours(self) -> Option<(bool, bool)> {
        match self {
            Self::RTH => Some((false, false)),
            Self::ETH => Some((true, false)),
            Self::Overnight => Some((true, true)),
            Self::Full24x7 => None,
        }
    }

    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            SessionType::RTH => "RTH",
            SessionType::ETH => "ETH",
            SessionType::Overnight => "ALL",
            SessionType::Full24x7 => "24/7",
        }
    }

    /// Get tooltip desc
    pub fn tooltip(&self) -> &'static str {
        match self {
            SessionType::RTH => "Regular trading hours: on",
            SessionType::ETH => "Regular trading hours: off (extended hours)",
            SessionType::Overnight => "Regular trading hours: off (extended and overnight hours)",
            SessionType::Full24x7 => "Regular trading hours: off (24/7 trading)",
        }
    }
}

/// Whether hours can be chosen. A missing instrument can still request hours, as in the time
/// axis menu; a session without windows or date-specific hours is always open.
pub(crate) fn has_trading_hours(instrument: Option<&crate::model::Instrument>) -> bool {
    instrument.is_none_or(|instrument| {
        instrument.session.as_ref().is_some_and(|session| {
            !session.windows.is_empty()
                || !session.extended_windows.is_empty()
                || !session.overnight_windows.is_empty()
                || !session.overrides.is_empty()
        })
    })
}

/// Whether weekly or date-specific overnight windows exist.
pub(crate) fn has_overnight_windows(session: Option<&crate::model::Session>) -> bool {
    session.is_some_and(|session| {
        !session.overnight_windows.is_empty()
            || session
                .overrides
                .iter()
                .any(|date| !date.overnight_windows.is_empty())
    })
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
    /// Current session type: RTH, ETH, ALL (overnight) or 24/7
    pub session_type: SessionType,
    /// Whether session menu is open
    pub session_menu_open: bool,
    /// Whether to adjust data for dividends
    pub adjust_for_dividends: bool,
    /// Current display time (for timezone display)
    pub curr_time: Option<chrono::DateTime<chrono::Utc>>,
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
        }
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

    /// Shows the clock in `zone`, the chart's
    /// [`display_timezone`](crate::widget::Chart::display_timezone): exchange time in the zone
    /// the chart resolved for it, local time at the system's present offset from UTC, which the
    /// clock keeps until this is called again.
    pub fn set_display_timezone(&mut self, zone: &crate::scales::TimezoneMode) {
        self.timezone = match zone {
            crate::scales::TimezoneMode::Local => {
                let offset = chrono::Local::now().offset().local_minus_utc();
                crate::model::TimezoneId::fixed_offset(offset).unwrap_or_default()
            }
            zone => zone.to_tz().unwrap_or_default(),
        }
        .to_string();
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
        let zone = self
            .timezone
            .parse::<crate::model::TimezoneId>()
            .unwrap_or(crate::model::TimezoneId::utc());
        match &self.curr_time {
            Some(time) => format!("{} {zone}", time.with_timezone(&zone).format("%H:%M:%S")),
            None => format!("--:--:-- {zone}"),
        }
    }

    /// Sync from application state.
    pub fn sync_from_app_state(&mut self, app_state: &dyn crate::ui::app_state::ChartAppState) {
        self.timeframe = *app_state.active_timeframe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The control follows every hours choice, with or without overnight windows, and reserves
    /// 24/7 for instruments that trade without sessions: #3560.
    #[test]
    fn session_control_follows_the_chart_hours() {
        use crate::model::{Instrument, Session, SessionOverride, SessionWindow};
        use chrono::{NaiveDate, NaiveTime, Weekday::Mon};

        let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let regular = Session::new([SessionWindow::new([Mon], at(9), at(16))]);
        let overnight =
            regular
                .clone()
                .with_overnight_windows([SessionWindow::new([Mon], at(20), at(4))]);
        let regular = Some(Instrument::new("A").with_session(regular));
        let overnight = Some(Instrument::new("A").with_session(overnight));
        let date = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        for (instrument, extended, overnight, expected) in
            [
                (regular.clone(), false, false, "RTH"),
                (regular.clone(), true, false, "ETH"),
                (regular, false, true, "ALL"),
                (overnight.clone(), false, false, "RTH"),
                (overnight.clone(), true, false, "ETH"),
                (overnight, true, true, "ALL"),
                (Some(Instrument::new("BTC")), true, false, "24/7"),
                (
                    Some(Instrument::new("BTC").with_session(Session::new([]))),
                    false,
                    false,
                    "24/7",
                ),
                (
                    Some(Instrument::new("A").with_session(Session::new([]).with_holidays([date]))),
                    true,
                    true,
                    "24/7",
                ),
                (
                    Some(Instrument::new("A").with_session(
                        Session::new([]).with_overrides([SessionOverride::new(date)]),
                    )),
                    false,
                    false,
                    "RTH",
                ),
                (None, true, false, "ETH"),
            ]
        {
            let mut chart = crate::Chart::new(crate::model::BarData::new());
            chart.set_instrument(instrument);
            chart.set_extended_hours(extended);
            chart.set_overnight_hours(overnight);
            assert_eq!(SessionType::from_chart(&chart).label(), expected);
        }
    }

    #[test]
    fn clock_converts_the_named_zone_and_falls_back_to_utc() {
        for (instant, zone, expected) in [
            ("2026-01-15T13:00:00Z", "UTC", "13:00:00 UTC"),
            ("2026-01-15T13:00:00Z", "+05:45", "18:45:00 +05:45"),
            #[cfg(feature = "timezones")]
            (
                "2026-01-15T13:00:00Z",
                "America/New_York",
                "08:00:00 America/New_York",
            ),
            #[cfg(feature = "timezones")]
            (
                "2026-07-15T13:00:00Z",
                "America/New_York",
                "09:00:00 America/New_York",
            ),
            ("2026-01-15T13:00:00Z", "unknown", "13:00:00 UTC"),
        ] {
            let state = TimeframeToolbarState {
                curr_time: Some(instant.parse().unwrap()),
                timezone: zone.to_owned(),
                ..Default::default()
            };
            assert_eq!(state.formatted_time(), expected);
        }
        let state = TimeframeToolbarState {
            timezone: "unknown".to_owned(),
            ..Default::default()
        };
        assert_eq!(state.formatted_time(), "--:--:-- UTC");
    }
}
