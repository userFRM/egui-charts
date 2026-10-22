//! Date range string parser
//!
//! Parses various date range formats into timestamp pairs:
//! - Absolute: "2024-01-01 to 2024-12-31"
//! - Relative: "last 30 days", "last 6 months"
//! - Presets: "YTD", "MTD", "1M", "3M", "6M", "1Y", "5Y", "MAX"

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use thiserror::Error;

/// Date range parsing errors
#[derive(Debug, Clone, Error, PartialEq)]
pub enum DateRangeParseError {
    #[error("Invalid date format: {0}")]
    InvalidDateFormat(String),

    #[error("Invalid range format: {0}")]
    InvalidRangeFormat(String),

    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    #[error("Invalid time unit: {0}")]
    InvalidTimeUnit(String),

    #[error("Start date must be before end date")]
    InvalidDateOrder,

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

/// Result type for internal date range parsing helpers.
type DateRangeResult = Result<Option<(DateTime<Utc>, DateTime<Utc>)>, DateRangeParseError>;

/// Parse a date range string into start/end timestamps
///
/// Supported formats:
/// - Absolute: "2024-01-01 to 2024-12-31", "2024-01-01..2024-12-31"
/// - Relative: "last 30 days", "last 6 months", "last 1 year"
/// - Presets: "YTD", "MTD", "1M", "3M", "6M", "1Y", "5Y", "MAX"
///
/// Returns (start_timestamp, end_timestamp) in Unix seconds
pub fn parse_date_range(input: &str) -> Result<(i64, i64), DateRangeParseError> {
    let trimmed = input.trim();

    // Try preset formats first (most common)
    if let Some((start, end)) = try_parse_preset(trimmed)? {
        return Ok((start.timestamp(), end.timestamp()));
    }

    // Try relative formats ("last N days/weeks/months/years")
    if let Some((start, end)) = try_parse_relative(trimmed)? {
        return Ok((start.timestamp(), end.timestamp()));
    }

    // Try absolute range formats ("2024-01-01 to 2024-12-31")
    if let Some((start, end)) = try_parse_absolute(trimmed)? {
        return Ok((start.timestamp(), end.timestamp()));
    }

    Err(DateRangeParseError::UnsupportedFormat(trimmed.to_string()))
}

/// Try to parse preset formats: YTD, MTD, 1M, 3M, 6M, 1Y, 5Y, MAX
fn try_parse_preset(s: &str) -> DateRangeResult {
    let upper = s.to_uppercase();
    let now = Utc::now();

    let duration = match upper.as_str() {
        "YTD" | "MTD" => {
            // Year or month to date: from 00:00 UTC on the first day of the year or month
            let month = if upper == "YTD" { 1 } else { now.month() };
            let period_start = NaiveDate::from_ymd_opt(now.year(), month, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .ok_or_else(|| DateRangeParseError::InvalidDateFormat(upper.clone()))?;
            let start = Utc.from_utc_datetime(&period_start);
            return Ok(Some((start, now)));
        }
        "1D" => Duration::days(1),
        "5D" => Duration::days(5),
        "1W" => Duration::weeks(1),
        "1M" => Duration::days(30),
        "3M" => Duration::days(90),
        "6M" => Duration::days(180),
        "1Y" => Duration::days(365),
        "5Y" => Duration::days(365 * 5),
        "10Y" => Duration::days(365 * 10),
        "MAX" | "ALL" => Duration::days(365 * 20), // 20 years max
        _ => return Ok(None),                      // Not a preset
    };

    let start = now - duration;
    Ok(Some((start, now)))
}

/// Try to parse relative formats: "last N days/weeks/months/years"
fn try_parse_relative(s: &str) -> DateRangeResult {
    let lower = s.to_lowercase();

    // Match "last N days/weeks/months/years"
    if !lower.starts_with("last ") {
        return Ok(None);
    }

    let parts: Vec<&str> = lower.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(DateRangeParseError::InvalidRangeFormat(s.to_string()));
    }

    let invalid_number = || DateRangeParseError::InvalidNumber(parts[1].to_string());
    let n: i64 = parts[1]
        .parse()
        .ok()
        .filter(|&n| n > 0)
        .ok_or_else(invalid_number)?;

    let unit = parts[2];
    let now = Utc::now();

    let days_per_unit = match unit {
        "day" | "days" => 1,
        "week" | "weeks" => 7,
        "month" | "months" => 30, // Approximate
        "year" | "years" => 365,  // Approximate
        _ => return Err(DateRangeParseError::InvalidTimeUnit(unit.to_string())),
    };

    let start = n
        .checked_mul(days_per_unit)
        .and_then(Duration::try_days)
        .and_then(|duration| now.checked_sub_signed(duration))
        .ok_or_else(invalid_number)?;
    Ok(Some((start, now)))
}

/// Try to parse absolute formats:
/// - "2024-01-01 to 2024-12-31"
/// - "2024-01-01..2024-12-31"
/// - "2024-01-01 - 2024-12-31"
fn try_parse_absolute(s: &str) -> DateRangeResult {
    // Try different separators
    let separator = if s.contains(" to ") {
        " to "
    } else if s.contains("..") {
        ".."
    } else if s.contains(" - ") {
        " - "
    } else {
        return Ok(None); // Not an absolute range
    };

    let parts: Vec<&str> = s.split(separator).collect();
    if parts.len() != 2 {
        return Err(DateRangeParseError::InvalidRangeFormat(s.to_string()));
    }

    let start = parse_date_string(parts[0].trim())?;
    let end = parse_date_string(parts[1].trim())?;

    if start >= end {
        return Err(DateRangeParseError::InvalidDateOrder);
    }

    Ok(Some((start, end)))
}

/// Parse a single date string in YYYY-MM-DD format
fn parse_date_string(s: &str) -> Result<DateTime<Utc>, DateRangeParseError> {
    // Support YYYY-MM-DD and YYYY/MM/DD
    let s_normalized = s.replace('/', "-");

    let parts: Vec<&str> = s_normalized.split('-').collect();
    if parts.len() != 3 {
        return Err(DateRangeParseError::InvalidDateFormat(s.to_string()));
    }

    let year: i32 = parts[0]
        .parse()
        .map_err(|_| DateRangeParseError::InvalidDateFormat(s.to_string()))?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| DateRangeParseError::InvalidDateFormat(s.to_string()))?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|_| DateRangeParseError::InvalidDateFormat(s.to_string()))?;

    let naive_date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| DateRangeParseError::InvalidDateFormat(s.to_string()))?;

    let naive_datetime = naive_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| DateRangeParseError::InvalidDateFormat(s.to_string()))?;

    Ok(Utc.from_utc_datetime(&naive_datetime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_to_date() {
        for preset in ["YTD", "MTD"] {
            let (start, end) = parse_date_range(preset).unwrap();
            let end_dt = Utc.timestamp_opt(end, 0).unwrap();
            let month = if preset == "YTD" { 1 } else { end_dt.month() };
            let expected = Utc
                .with_ymd_and_hms(end_dt.year(), month, 1, 0, 0, 0)
                .unwrap();
            assert_eq!(start, expected.timestamp(), "{preset}");
        }
    }

    #[test]
    fn test_preset_case_insensitive() {
        assert!(parse_date_range("ytd").is_ok());
        assert!(parse_date_range("1m").is_ok());
        assert!(parse_date_range("MAX").is_ok());
    }

    #[test]
    fn test_relative_days() {
        let result = parse_date_range("last 30 days");
        assert!(result.is_ok());
    }

    #[test]
    fn test_relative_months() {
        let result = parse_date_range("last 6 months");
        assert!(result.is_ok());
    }

    #[test]
    fn test_relative_years() {
        let result = parse_date_range("last 1 year");
        assert!(result.is_ok());
    }

    #[test]
    fn test_absolute_to() {
        let result = parse_date_range("2024-01-01 to 2024-12-31");
        assert!(result.is_ok());

        let (start, end) = result.unwrap();
        let start_dt = Utc.timestamp_opt(start, 0).unwrap();
        let end_dt = Utc.timestamp_opt(end, 0).unwrap();

        assert_eq!(start_dt.year(), 2024);
        assert_eq!(start_dt.month(), 1);
        assert_eq!(start_dt.day(), 1);

        assert_eq!(end_dt.year(), 2024);
        assert_eq!(end_dt.month(), 12);
        assert_eq!(end_dt.day(), 31);
    }

    #[test]
    fn test_absolute_dotdot() {
        let result = parse_date_range("2024-01-01..2024-06-30");
        assert!(result.is_ok());
    }

    #[test]
    fn test_absolute_slash() {
        let result = parse_date_range("2024/01/01 to 2024/06/30");
        assert!(result.is_ok());
    }

    #[test]
    fn test_errors() {
        use DateRangeParseError::*;
        let cases = [
            ("2024-12-31 to 2024-01-01", InvalidDateOrder),
            ("invalid", UnsupportedFormat("invalid".into())),
            (
                "2024-13-01 to 2024-12-31",
                InvalidDateFormat("2024-13-01".into()),
            ),
            ("last -5 days", InvalidNumber("-5".into())),
            ("last 0 days", InvalidNumber("0".into())),
            (
                "last 9999999999999 days",
                InvalidNumber("9999999999999".into()),
            ),
            ("last 100000000 days", InvalidNumber("100000000".into())),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_date_range(input), Err(expected), "{input}");
        }
    }
}
