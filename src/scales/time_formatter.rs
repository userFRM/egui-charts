//! Time formatting for X-axis labels.
//!
//! The [`TimeFormatter`] trait controls how timestamps are rendered as labels
//! on the time axis.  The library ships with [`DefaultTimeFormatter`],
//! [`LocaleTimeFormatter`], [`RelativeTimeFormatter`], and a
//! [`TimezoneAwareFormatter`] wrapper for automatic timezone conversion.
//!
//! Use [`TimeFormatterBuilder`] for a fluent construction API.

use super::timescale_marks::TickMarkType;
use crate::config::TimezoneMode;
/// Time formatting trait for customizable time axis labels.
/// Reference: lightweight-charts TimeFormatterFn.
use chrono::{DateTime, TimeZone, Utc};

#[cfg(test)]
use chrono::Datelike;

/// Trait for custom time formatting
/// Allows users to customize how time labels are displayed on the axis
pub trait TimeFormatter: Send + Sync {
    /// Format a ts into a display string
    ///
    /// # Arguments
    /// * `time` - The ts to format
    /// * `mark_type` - The type of tick mark (Year, Month, etc.)
    ///
    /// # Returns
    /// Formatted string to display on the time axis
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String;

    /// Clone this formatter into a Box
    fn clone_box(&self) -> Box<dyn TimeFormatter>;
}

/// Default time formatter using standard formats
#[derive(Debug, Clone)]
pub struct DefaultTimeFormatter {
    /// Whether to use 24-hour format
    pub use_24_hour: bool,

    /// Whether to show seconds
    pub show_seconds: bool,
}

impl Default for DefaultTimeFormatter {
    fn default() -> Self {
        Self {
            use_24_hour: true,
            show_seconds: true,
        }
    }
}

impl TimeFormatter for DefaultTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        match mark_type {
            TickMarkType::Year => time.format("%Y").to_string(),
            TickMarkType::Month => time.format("%b").to_string(),
            // Show "Jan 15" for day boundaries
            TickMarkType::DayOfMonth => time.format("%b %d").to_string(),
            TickMarkType::Time => {
                // Show just time for intraday marks
                if self.use_24_hour {
                    time.format("%H:%M").to_string()
                } else {
                    time.format("%I:%M %p").to_string()
                }
            }
            TickMarkType::TimeWithSeconds => {
                if !self.show_seconds {
                    // Fall back to Time format if seconds disabled
                    return self.format(time, TickMarkType::Time);
                }

                if self.use_24_hour {
                    time.format("%H:%M:%S").to_string()
                } else {
                    time.format("%I:%M:%S %p").to_string()
                }
            }
        }
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Locale-aware time formatter
#[derive(Debug, Clone)]
pub struct LocaleTimeFormatter {
    /// Locale identifier (e.g., "en-US", "ja-JP", "de-DE")
    pub locale: String,

    /// Whether to use 24-hour format
    pub use_24_hour: bool,
}

impl LocaleTimeFormatter {
    /// Create a new locale-aware formatter
    pub fn new(locale: impl Into<String>) -> Self {
        let locale_str = locale.into();
        let use_24_hour = Self::default_24hour_for_locale(&locale_str);
        Self {
            locale: locale_str,
            use_24_hour,
        }
    }

    /// Determine default 24-hour preference for locale
    fn default_24hour_for_locale(locale: &str) -> bool {
        // US, Canada, UK prefer 12-hour
        !matches!(locale, "en-US" | "en-CA" | "en-GB")
    }
}

impl TimeFormatter for LocaleTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        // Use locale-specific formatting
        // For now, fall back to default formatter
        // In production, this would use proper locale libraries
        DefaultTimeFormatter {
            use_24_hour: self.use_24_hour,
            show_seconds: true,
        }
        .format(time, mark_type)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Custom time formatter using a closure
pub struct CustomTimeFormatter {
    formatter: Box<dyn Fn(DateTime<Utc>, TickMarkType) -> String + Send + Sync>,
}

impl CustomTimeFormatter {
    /// Create a new custom formatter from a closure
    pub fn new<F>(formatter: F) -> Self
    where
        F: Fn(DateTime<Utc>, TickMarkType) -> String + Send + Sync + 'static,
    {
        Self {
            formatter: Box::new(formatter),
        }
    }
}

impl TimeFormatter for CustomTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        (self.formatter)(time, mark_type)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        // Note: Cannot clone closures, so this returns default
        // Users should manage their custom formatters explicitly
        Box::new(DefaultTimeFormatter::default())
    }
}

/// Relative time formatter (e.g., "2h ago", "yesterday")
#[derive(Debug, Clone)]
pub struct RelativeTimeFormatter {
    /// Reference time for relative calculations (usually "now")
    pub reference_time: DateTime<Utc>,
}

impl RelativeTimeFormatter {
    /// Create a new relative time formatter
    pub fn new(reference_time: DateTime<Utc>) -> Self {
        Self { reference_time }
    }

    /// Create a formatter relative to current time
    pub fn now() -> Self {
        Self {
            reference_time: Utc::now(),
        }
    }
}

impl TimeFormatter for RelativeTimeFormatter {
    fn format(&self, time: DateTime<Utc>, _mark_type: TickMarkType) -> String {
        let diff = self.reference_time.signed_duration_since(time);

        if diff.num_seconds() < 0 {
            return "future".to_string();
        }

        let seconds = diff.num_seconds();

        if seconds < 60 {
            format!("{seconds}s ago")
        } else if seconds < 3600 {
            format!("{}m ago", seconds / 60)
        } else if seconds < 86400 {
            format!("{}h ago", seconds / 3600)
        } else if seconds < 604800 {
            format!("{}d ago", seconds / 86400)
        } else if seconds < 2592000 {
            format!("{}w ago", seconds / 604800)
        } else if seconds < 31536000 {
            format!("{}mo ago", seconds / 2592000)
        } else {
            format!("{}y ago", seconds / 31536000)
        }
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Timezone-aware formatter wrapper
/// Converts ts to target timezone before formatting
pub struct TimezoneAwareFormatter {
    inner: Box<dyn TimeFormatter>,
    timezone: TimezoneMode,
}

impl Clone for TimezoneAwareFormatter {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone_box(),
            timezone: self.timezone.clone(),
        }
    }
}

impl TimezoneAwareFormatter {
    /// Create a new timezone-aware formatter
    pub fn new(inner: Box<dyn TimeFormatter>, timezone: TimezoneMode) -> Self {
        Self { inner, timezone }
    }

    /// Convert UTC ts to target timezone
    fn convert_timezone(&self, utc_time: DateTime<Utc>) -> DateTime<Utc> {
        use chrono::Datelike;
        use chrono::Timelike;

        match &self.timezone {
            TimezoneMode::Utc => utc_time,
            TimezoneMode::Local => {
                // Convert to local, then back to UTC representation for formatting
                let local = utc_time.with_timezone(&chrono::Local);
                Utc.with_ymd_and_hms(
                    local.year(),
                    local.month(),
                    local.day(),
                    local.hour(),
                    local.minute(),
                    local.second(),
                )
                .single()
                .unwrap_or(utc_time)
            }
            TimezoneMode::Timezone(tz) | TimezoneMode::Exchange(tz) => {
                // Convert to target timezone
                let converted = utc_time.with_timezone(tz);
                Utc.with_ymd_and_hms(
                    converted.year(),
                    converted.month(),
                    converted.day(),
                    converted.hour(),
                    converted.minute(),
                    converted.second(),
                )
                .single()
                .unwrap_or(utc_time)
            }
        }
    }
}

impl TimeFormatter for TimezoneAwareFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        let converted = self.convert_timezone(time);
        self.inner.format(converted, mark_type)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Builder for creating time formatters
pub struct TimeFormatterBuilder {
    use_24_hour: bool,
    show_seconds: bool,
    locale: Option<String>,
    timezone: Option<TimezoneMode>,
}

impl TimeFormatterBuilder {
    /// Create a new formatter builder
    pub fn new() -> Self {
        Self {
            use_24_hour: true,
            show_seconds: true,
            locale: None,
            timezone: None,
        }
    }

    /// Set 24-hour format
    pub fn with_24_hour(mut self, use_24_hour: bool) -> Self {
        self.use_24_hour = use_24_hour;
        self
    }

    /// Set seconds display
    pub fn with_seconds(mut self, show_seconds: bool) -> Self {
        self.show_seconds = show_seconds;
        self
    }

    /// Set locale
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Set timezone for conversion
    pub fn with_timezone(mut self, timezone: TimezoneMode) -> Self {
        self.timezone = Some(timezone);
        self
    }

    /// Build the formatter
    pub fn build(self) -> Box<dyn TimeFormatter> {
        let base: Box<dyn TimeFormatter> = if let Some(locale) = self.locale {
            Box::new(LocaleTimeFormatter {
                locale,
                use_24_hour: self.use_24_hour,
            })
        } else {
            Box::new(DefaultTimeFormatter {
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
            })
        };

        // Wrap with timezone converter if specified
        if let Some(tz) = self.timezone {
            Box::new(TimezoneAwareFormatter::new(base, tz))
        } else {
            base
        }
    }
}

impl Default for TimeFormatterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_default_formatter() {
        let formatter = DefaultTimeFormatter::default();
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();

        assert_eq!(formatter.format(time, TickMarkType::Year), "2024");
        assert_eq!(formatter.format(time, TickMarkType::Month), "Jun");
        assert_eq!(formatter.format(time, TickMarkType::Time), "14:30");
        assert_eq!(
            formatter.format(time, TickMarkType::TimeWithSeconds),
            "14:30:45"
        );
    }

    #[test]
    fn test_12_hour_format() {
        let formatter = DefaultTimeFormatter {
            use_24_hour: false,
            show_seconds: true,
        };
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();

        let result = formatter.format(time, TickMarkType::Time);
        assert!(result.contains("PM"));
    }

    #[test]
    fn test_relative_formatter() {
        let reference = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let formatter = RelativeTimeFormatter::new(reference);

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 28, 0).unwrap();
        assert_eq!(formatter.format(time, TickMarkType::Time), "2m ago");

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 12, 30, 0).unwrap();
        assert_eq!(formatter.format(time, TickMarkType::Time), "2h ago");
    }

    #[test]
    fn test_custom_formatter() {
        let formatter = CustomTimeFormatter::new(|time, mark_type| match mark_type {
            TickMarkType::Year => format!("Year {}", time.year()),
            _ => time.format("%Y-%m-%d").to_string(),
        });

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        assert_eq!(formatter.format(time, TickMarkType::Year), "Year 2024");
        assert_eq!(formatter.format(time, TickMarkType::Month), "2024-06-15");
    }

    #[test]
    fn test_formatter_builder() {
        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(false)
            .with_seconds(false)
            .build();

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();
        let result = formatter.format(time, TickMarkType::Time);
        assert!(result.contains("PM"));
    }

    #[test]
    fn test_timezone_conversion_utc() {
        use crate::config::TimezoneMode;

        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(false)
            .with_timezone(TimezoneMode::Utc)
            .build();

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let result = formatter.format(time, TickMarkType::Time);
        assert_eq!(result, "14:30");
    }

    #[test]
    fn test_timezone_conversion_ny() {
        use crate::config::TimezoneMode;
        use chrono_tz::America::New_York;

        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(false)
            .with_timezone(TimezoneMode::Timezone(New_York))
            .build();

        // June 15, 2024 14:30 UTC = 10:30 EDT (UTC-4)
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let result = formatter.format(time, TickMarkType::Time);
        assert_eq!(result, "10:30");
    }

    #[test]
    fn test_dst_spring_forward() {
        use crate::config::TimezoneMode;
        use chrono_tz::America::New_York;

        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(true)
            .with_timezone(TimezoneMode::Exchange(New_York))
            .build();

        // 2024 DST transition: March 10, 2:00 AM -> 3:00 AM
        // Before DST: March 10, 2024 06:00 UTC = March 10, 2024 01:00 EST (UTC-5)
        let before_dst = Utc.with_ymd_and_hms(2024, 3, 10, 6, 0, 0).unwrap();
        let result_before = formatter.format(before_dst, TickMarkType::TimeWithSeconds);
        assert_eq!(result_before, "01:00:00");

        // After DST: March 10, 2024 08:00 UTC = March 10, 2024 04:00 EDT (UTC-4)
        let after_dst = Utc.with_ymd_and_hms(2024, 3, 10, 8, 0, 0).unwrap();
        let result_after = formatter.format(after_dst, TickMarkType::TimeWithSeconds);
        assert_eq!(result_after, "04:00:00");
    }

    #[test]
    fn test_dst_fall_back() {
        use crate::config::TimezoneMode;
        use chrono_tz::America::New_York;

        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(true)
            .with_timezone(TimezoneMode::Exchange(New_York))
            .build();

        // 2024 DST transition: November 3, 2:00 AM -> 1:00 AM
        // Before DST end: November 3, 2024 05:00 UTC = November 3, 2024 01:00 EDT (UTC-4)
        let before_dst = Utc.with_ymd_and_hms(2024, 11, 3, 5, 0, 0).unwrap();
        let result_before = formatter.format(before_dst, TickMarkType::TimeWithSeconds);
        assert_eq!(result_before, "01:00:00");

        // After DST end: November 3, 2024 07:00 UTC = November 3, 2024 02:00 EST (UTC-5)
        let after_dst = Utc.with_ymd_and_hms(2024, 11, 3, 7, 0, 0).unwrap();
        let result_after = formatter.format(after_dst, TickMarkType::TimeWithSeconds);
        assert_eq!(result_after, "02:00:00");
    }

    #[test]
    fn test_timezone_london() {
        use crate::config::TimezoneMode;

        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(false)
            .with_timezone(TimezoneMode::lse())
            .build();

        // June 15, 2024 14:30 UTC = 15:30 BST (UTC+1)
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let result = formatter.format(time, TickMarkType::Time);
        assert_eq!(result, "15:30");

        // January 15, 2024 14:30 UTC = 14:30 GMT (UTC+0)
        let time_winter = Utc.with_ymd_and_hms(2024, 1, 15, 14, 30, 0).unwrap();
        let result_winter = formatter.format(time_winter, TickMarkType::Time);
        assert_eq!(result_winter, "14:30");
    }

    #[test]
    fn test_timezone_tokyo() {
        use crate::config::TimezoneMode;

        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(false)
            .with_timezone(TimezoneMode::jse())
            .build();

        // June 15, 2024 14:30 UTC = June 15, 2024 23:30 JST (UTC+9)
        // Note: Japan doesn't use DST
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let result = formatter.format(time, TickMarkType::Time);
        assert_eq!(result, "23:30");
    }

    #[test]
    fn test_timezone_clone() {
        use crate::config::TimezoneMode;

        let formatter1 = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(false)
            .with_timezone(TimezoneMode::nyse())
            .build();

        // Test that clone_box works correctly
        let formatter2 = formatter1.clone_box();

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let result1 = formatter1.format(time, TickMarkType::Time);
        let result2 = formatter2.format(time, TickMarkType::Time);

        assert_eq!(result1, result2);
        assert_eq!(result1, "10:30");
    }
}
