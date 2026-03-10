//! Preset date-range selectors for the chart toolbar.
//!
//! Mirrors the "1D / 5D / 1M / 3M / 6M / YTD / 1Y / 5Y / All" buttons
//! found on most financial charting platforms.

/// Preset date range for quick navigation.
///
/// Each variant describes a lookback window from the current time.
/// The default preset is [`Month1`](DateRange::Month1).
///
/// Use [`start_timestamp`](DateRange::start_timestamp) to convert a preset into
/// an absolute `DateTime`, or [`estimated_bars`](DateRange::estimated_bars) to
/// estimate how many bars to fetch for a given timeframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DateRange {
    /// 1 Day
    Day1,
    /// 5 Days
    Day5,
    /// 1 Month
    #[default]
    Month1,
    /// 3 Months
    Month3,
    /// 6 Months
    Month6,
    /// Year to Date (from Jan 1 of current year)
    YTD,
    /// 1 Year
    Year1,
    /// 5 Years
    Year5,
    /// All available data
    All,
}

impl DateRange {
    /// Get display label for the range
    pub fn label(&self) -> &'static str {
        match self {
            DateRange::Day1 => "1D",
            DateRange::Day5 => "5D",
            DateRange::Month1 => "1M",
            DateRange::Month3 => "3M",
            DateRange::Month6 => "6M",
            DateRange::YTD => "YTD",
            DateRange::Year1 => "1Y",
            DateRange::Year5 => "5Y",
            DateRange::All => "All",
        }
    }

    /// Get tooltip desc for the range
    pub fn tooltip(&self) -> &'static str {
        match self {
            DateRange::Day1 => "1 day in 1 minute intervals",
            DateRange::Day5 => "5 days in 5 minutes intervals",
            DateRange::Month1 => "1 month in 30 minutes intervals",
            DateRange::Month3 => "3 months in 1 hour intervals",
            DateRange::Month6 => "6 months in 2 hours intervals",
            DateRange::YTD => "Year to day in 1 day intervals",
            DateRange::Year1 => "1 year in 1 day intervals",
            DateRange::Year5 => "5 years in 1 week intervals",
            DateRange::All => "All data in 1 month intervals",
        }
    }

    /// Get all standard ranges in order
    pub fn default_presets() -> Vec<DateRange> {
        vec![
            DateRange::Day1,
            DateRange::Day5,
            DateRange::Month1,
            DateRange::Month3,
            DateRange::Month6,
            DateRange::YTD,
            DateRange::Year1,
            DateRange::Year5,
            DateRange::All,
        ]
    }

    /// Calculate the start ts for this range
    /// Returns None for DateRange::All (meaning "from earliest available")
    pub fn start_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::{Datelike, Duration, NaiveDate, Utc};
        let now = Utc::now();

        match self {
            DateRange::Day1 => Some(now - Duration::days(1)),
            DateRange::Day5 => Some(now - Duration::days(5)),
            DateRange::Month1 => Some(now - Duration::days(30)),
            DateRange::Month3 => Some(now - Duration::days(90)),
            DateRange::Month6 => Some(now - Duration::days(180)),
            DateRange::YTD => {
                let year_start = NaiveDate::from_ymd_opt(now.year(), 1, 1)?;
                Some(chrono::DateTime::from_naive_utc_and_offset(
                    year_start.and_hms_opt(0, 0, 0)?,
                    Utc,
                ))
            }
            DateRange::Year1 => Some(now - Duration::days(365)),
            DateRange::Year5 => Some(now - Duration::days(365 * 5)),
            DateRange::All => None,
        }
    }

    /// Get the date range as (start, end) ts
    /// Returns (None, now) for All (meaning earliest to now)
    pub fn to_timestamps(
        &self,
    ) -> (
        Option<chrono::DateTime<chrono::Utc>>,
        chrono::DateTime<chrono::Utc>,
    ) {
        (self.start_timestamp(), chrono::Utc::now())
    }

    /// Calculate approximate number of bars needed for this range at a given timeframe
    pub fn estimated_bars(&self, bar_duration_seconds: u64) -> usize {
        let duration_seconds = match self {
            DateRange::Day1 => 24 * 3600,
            DateRange::Day5 => 5 * 24 * 3600,
            DateRange::Month1 => 30 * 24 * 3600,
            DateRange::Month3 => 90 * 24 * 3600,
            DateRange::Month6 => 180 * 24 * 3600,
            DateRange::YTD => {
                let now = chrono::Utc::now();
                let days_since_year_start = chrono::Datelike::ordinal(&now) as u64;
                days_since_year_start * 24 * 3600
            }
            DateRange::Year1 => 365 * 24 * 3600,
            DateRange::Year5 => 5 * 365 * 24 * 3600,
            DateRange::All => 10 * 365 * 24 * 3600,
        };

        (duration_seconds / bar_duration_seconds.max(1)) as usize
    }
}

impl std::fmt::Display for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}
