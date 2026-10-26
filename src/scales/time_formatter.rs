//! Time formatting for X-axis labels.
//!
//! The [`TimeFormatter`] trait controls how timestamps are rendered as labels
//! on the time axis.  The library ships with [`DefaultTimeFormatter`],
//! [`LocaleTimeFormatter`], [`RelativeTimeFormatter`], and a
//! [`TimezoneAwareFormatter`] wrapper for automatic timezone conversion.
//!
//! Use [`TimeFormatterBuilder`] for a fluent construction API.

use super::timescale_marks::TickMarkType;
use crate::model::TimezoneId;
use chrono::{DateTime, Utc};
use std::sync::Arc;

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

    /// Clock-face time used to identify calendar boundaries. The returned UTC value
    /// holds local fields, not another instant.
    fn wall_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        time
    }

    /// Converts local calendar fields from `wall_time` back to an instant.
    /// Repeated times use the first instant; skipped times use the clock jump.
    fn utc_time(&self, wall_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        Some(wall_time)
    }

    /// Whether intraday marks and hover labels include the clock.
    fn time_visible(&self) -> bool {
        true
    }

    /// Formats the hovered bar, retaining this formatter's date and clock conventions.
    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        let mut label = self.format(time, TickMarkType::DayOfMonth);
        let year = self.format(time, TickMarkType::Year);
        if year != self.format(Utc::now(), TickMarkType::Year) {
            label = format!("{label}, {year}");
        }
        match bar_seconds {
            86_400.. => label,
            60.. => format!("{label} {}", self.format(time, TickMarkType::Time)),
            1.. => format!(
                "{label} {}",
                self.format(time, TickMarkType::TimeWithSeconds)
            ),
            _ => format!(
                "{label} {}{}",
                self.format(time, TickMarkType::TimeWithSeconds),
                time.format("%.3f")
            ),
        }
    }

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

    /// `strftime` pattern for the date in crosshair and tooltip labels, such as
    /// `"%d.%m.%Y"`, followed by the clock for intraday bars. `None` uses the
    /// built-in dates; an invalid pattern falls back to them.
    pub date_format: Option<String>,
}

impl Default for DefaultTimeFormatter {
    fn default() -> Self {
        Self {
            use_24_hour: true,
            show_seconds: true,
            date_format: None,
        }
    }
}

impl DefaultTimeFormatter {
    fn style(&self) -> Style<'_> {
        Style {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: self.date_format.as_deref(),
            #[cfg(feature = "locales")]
            locale: None,
        }
    }

    fn into_box(self) -> Box<dyn TimeFormatter> {
        if self.date_format.is_none() {
            Box::new(ClockTimeFormatter {
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
            })
        } else {
            Box::new(self)
        }
    }
}

impl TimeFormatter for DefaultTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        self.style().format(time, mark_type)
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.style().format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        self.clone().into_box()
    }
}

/// Default clock options without storage for an unused custom date pattern.
#[derive(Clone)]
struct ClockTimeFormatter {
    use_24_hour: bool,
    show_seconds: bool,
}

impl TimeFormatter for ClockTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        DefaultTimeFormatter {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: None,
        }
        .format(time, mark_type)
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        DefaultTimeFormatter {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: None,
        }
        .format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Label layout shared by the default and locale formatters.
struct Style<'a> {
    use_24_hour: bool,
    show_seconds: bool,
    date_format: Option<&'a str>,
    /// Language of the names; `None` writes English ones.
    #[cfg(feature = "locales")]
    locale: Option<chrono::Locale>,
}

impl Style<'_> {
    /// Writes `time` with a `strftime` pattern, or `None` when the pattern is invalid.
    fn write(&self, time: DateTime<Utc>, pattern: &str) -> Option<String> {
        use std::fmt::Write;
        let mut text = String::new();
        #[cfg(feature = "locales")]
        if let Some(locale) = self.locale {
            write!(text, "{}", time.format_localized(pattern, locale)).ok()?;
            // Some locales pad their names, as in Japanese " 5月".
            return Some(text.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        write!(text, "{}", time.format(pattern)).ok()?;
        Some(text)
    }

    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        let pattern = match (mark_type, self.use_24_hour) {
            (TickMarkType::Year, _) => "%Y",
            (TickMarkType::Month, _) => "%b",
            (TickMarkType::DayOfMonth, _) => "%d",
            (TickMarkType::TimeWithSeconds, true) if self.show_seconds => "%H:%M:%S",
            (TickMarkType::TimeWithSeconds, false) if self.show_seconds => "%I:%M:%S %p",
            // Without seconds, second marks show the minute clock.
            (_, true) => "%H:%M",
            (_, false) => "%I:%M %p",
        };
        self.write(time, pattern).unwrap_or_default()
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        let date = self
            .date_format
            .and_then(|pattern| self.write(time, pattern));
        let custom_date = date.is_some();
        let daily = bar_seconds >= 86_400;
        let pattern = if daily { "%b %d" } else { "%a %d %b '%y" };
        #[cfg(feature = "locales")]
        let date = date.or_else(|| self.locale.and_then(|_| self.write(time, pattern)));
        // The built-in English date writes straight into the label without a temporary String.
        let default_date = time.format(pattern);
        let date: &dyn std::fmt::Display = match &date {
            Some(date) => date,
            None => &default_date,
        };
        if daily {
            return if custom_date
                || time.format("%Y").to_string() == Utc::now().format("%Y").to_string()
            {
                date.to_string()
            } else {
                format!("{date}, {}", time.format("%Y"))
            };
        }
        let kind = if bar_seconds >= 60 {
            TickMarkType::Time
        } else {
            TickMarkType::TimeWithSeconds
        };
        let clock = self.format(time, kind);
        if bar_seconds <= 0 && self.show_seconds {
            format!("{date} {clock}{}", time.format("%.3f"))
        } else {
            format!("{date} {clock}")
        }
    }
}

/// Locale-aware time formatter: month and weekday names and the AM/PM marker
/// in the locale's language, laid out as [`DefaultTimeFormatter`] does.
///
/// The names need the `locales` cargo feature, on by default; without it they
/// are English, as with an unknown locale.
#[derive(Debug, Clone)]
pub struct LocaleTimeFormatter {
    /// Locale identifier: language and region, such as `"en-US"`, `"ja-JP"` or
    /// `"de-DE"` (`"de_DE"` also works). Unknown identifiers use English names.
    pub locale: String,

    /// Whether to use 24-hour format
    pub use_24_hour: bool,

    /// Whether to show seconds
    pub show_seconds: bool,

    /// `strftime` pattern for the date in crosshair and tooltip labels; see
    /// [`DefaultTimeFormatter::date_format`].
    pub date_format: Option<String>,
}

impl LocaleTimeFormatter {
    /// Create a new locale-aware formatter
    pub fn new(locale: impl Into<String>) -> Self {
        let locale_str = locale.into();
        let use_24_hour = Self::default_24hour_for_locale(&locale_str);
        Self {
            locale: locale_str,
            use_24_hour,
            show_seconds: true,
            date_format: None,
        }
    }

    /// Determine default 24-hour preference for locale
    fn default_24hour_for_locale(locale: &str) -> bool {
        // US, Canada, UK prefer 12-hour
        !matches!(locale, "en-US" | "en-CA" | "en-GB")
    }

    fn style(&self) -> Style<'_> {
        Style {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: self.date_format.as_deref(),
            #[cfg(feature = "locales")]
            locale: Some(
                chrono::Locale::try_from(self.locale.replace('-', "_").as_str())
                    .unwrap_or(chrono::Locale::POSIX),
            ),
        }
    }
}

impl TimeFormatter for LocaleTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        self.style().format(time, mark_type)
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.style().format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Custom time formatter using a closure
#[derive(Clone)]
pub struct CustomTimeFormatter {
    formatter: Arc<dyn Fn(DateTime<Utc>, TickMarkType) -> String + Send + Sync>,
}

impl CustomTimeFormatter {
    /// Create a new custom formatter from a closure
    pub fn new<F>(formatter: F) -> Self
    where
        F: Fn(DateTime<Utc>, TickMarkType) -> String + Send + Sync + 'static,
    {
        Self {
            formatter: Arc::new(formatter),
        }
    }
}

impl TimeFormatter for CustomTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        (self.formatter)(time, mark_type)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
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

/// Timezone display mode for time axis labels.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TimezoneMode {
    /// Display in UTC
    #[default]
    Utc,
    /// Display in user's local timezone
    Local,
    /// Display in specific timezone
    Timezone(TimezoneId),
    /// Display in exchange/venue timezone (user must specify via Timezone)
    Exchange(TimezoneId),
}

impl TimezoneMode {
    /// New York exchange time, with daylight-saving rules (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn nyse() -> Self {
        TimezoneMode::Exchange("America/New_York".parse().unwrap())
    }

    /// London exchange time, with daylight-saving rules (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn lse() -> Self {
        TimezoneMode::Exchange("Europe/London".parse().unwrap())
    }

    /// Tokyo exchange time (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn jse() -> Self {
        TimezoneMode::Exchange("Asia/Tokyo".parse().unwrap())
    }

    /// Hong Kong exchange time (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn hkex() -> Self {
        TimezoneMode::Exchange("Asia/Hong_Kong".parse().unwrap())
    }

    /// Get the selected zone for conversion; UTC and system local time return `None`.
    pub fn to_tz(&self) -> Option<TimezoneId> {
        match self {
            TimezoneMode::Utc => None,   // Already UTC
            TimezoneMode::Local => None, // Will use system local
            TimezoneMode::Timezone(tz) | TimezoneMode::Exchange(tz) => Some(tz.clone()),
        }
    }
}

/// Timezone-aware formatter wrapper
/// Converts ts to target timezone before formatting
pub(crate) struct TimezoneAwareFormatter<T> {
    inner: Box<dyn TimeFormatter>,
    timezone: T,
    time_visible: bool,
}

impl<T: Clone> Clone for TimezoneAwareFormatter<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone_box(),
            timezone: self.timezone.clone(),
            time_visible: self.time_visible,
        }
    }
}

impl<T: chrono::TimeZone> TimezoneAwareFormatter<T> {
    /// Create a new timezone-aware formatter
    pub(crate) fn new(inner: Box<dyn TimeFormatter>, timezone: T, time_visible: bool) -> Self {
        Self {
            inner,
            timezone,
            time_visible,
        }
    }

    /// Convert UTC ts to target timezone
    fn convert_timezone(&self, utc_time: DateTime<Utc>) -> DateTime<Utc> {
        use crate::model::session::local_time;
        local_time(self.timezone.clone(), utc_time).and_utc()
    }
}

impl<T: chrono::TimeZone + Send + Sync + 'static> TimeFormatter for TimezoneAwareFormatter<T> {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        let converted = self.convert_timezone(time);
        if !self.time_visible
            && matches!(
                mark_type,
                TickMarkType::Time | TickMarkType::TimeWithSeconds
            )
        {
            self.inner.format_crosshair(converted, 86_400)
        } else {
            self.inner.format(converted, mark_type)
        }
    }

    fn time_visible(&self) -> bool {
        self.time_visible
    }

    fn wall_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        self.convert_timezone(time)
    }

    fn utc_time(&self, time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        use crate::model::session::local_instant;
        local_instant(self.timezone.clone(), time.naive_utc())
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.inner.format_crosshair(
            self.convert_timezone(time),
            if self.time_visible {
                bar_seconds
            } else {
                86_400
            },
        )
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
    date_format: Option<String>,
    formatter: Option<Box<dyn TimeFormatter>>,
    timezone: Option<TimezoneMode>,
    time_visible: bool,
}

impl TimeFormatterBuilder {
    /// Create a new formatter builder
    pub fn new() -> Self {
        Self {
            use_24_hour: true,
            show_seconds: true,
            locale: None,
            date_format: None,
            formatter: None,
            timezone: None,
            time_visible: true,
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

    /// Show the clock in intraday marks and hover labels, or dates alone.
    pub fn with_time_visible(mut self, time_visible: bool) -> Self {
        self.time_visible = time_visible;
        self
    }

    /// Set locale
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Set the `strftime` pattern for crosshair and tooltip dates; see
    /// [`DefaultTimeFormatter::date_format`].
    pub fn with_date_format(mut self, date_format: impl Into<String>) -> Self {
        self.date_format = Some(date_format.into());
        self
    }

    /// Format with `formatter` instead of the default or locale formatter.
    ///
    /// The time zone and clock visibility still apply: `formatter` receives
    /// wall-clock times in the chosen zone, and dates in place of intraday
    /// clocks when the time is hidden. Its own options replace the 24-hour,
    /// seconds, locale and date format settings.
    pub fn with_formatter(mut self, formatter: Box<dyn TimeFormatter>) -> Self {
        self.formatter = Some(formatter);
        self
    }

    /// Set timezone for conversion
    pub fn with_timezone(mut self, timezone: TimezoneMode) -> Self {
        self.timezone = Some(timezone);
        self
    }

    /// Build the formatter
    pub fn build(self) -> Box<dyn TimeFormatter> {
        let base: Box<dyn TimeFormatter> = if let Some(formatter) = self.formatter {
            formatter
        } else if let Some(locale) = self.locale {
            Box::new(LocaleTimeFormatter {
                locale,
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
                date_format: self.date_format,
            })
        } else {
            DefaultTimeFormatter {
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
                date_format: self.date_format,
            }
            .into_box()
        };

        // UTC and system-local formatters need no storage for a selected zone.
        match self.timezone.unwrap_or_default() {
            TimezoneMode::Utc => {
                Box::new(TimezoneAwareFormatter::new(base, Utc, self.time_visible))
            }
            TimezoneMode::Local => Box::new(TimezoneAwareFormatter::new(
                base,
                chrono::Local,
                self.time_visible,
            )),
            TimezoneMode::Timezone(zone) | TimezoneMode::Exchange(zone) => {
                Box::new(TimezoneAwareFormatter::new(base, zone, self.time_visible))
            }
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
        let formatter = DefaultTimeFormatter::default().clone_box();
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
            ..Default::default()
        }
        .clone_box();
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();

        let result = formatter.format(time, TickMarkType::Time);
        assert!(result.contains("PM"));
    }

    #[test]
    fn locale_and_date_format_shape_labels() {
        // Wednesday 13 March 2024, 14:30:45 UTC.
        let time = Utc.with_ymd_and_hms(2024, 3, 13, 14, 30, 45).unwrap();
        // Without the `locales` feature every locale writes English names.
        let localized = |name, english| {
            if cfg!(feature = "locales") {
                name
            } else {
                english
            }
        };
        let german = LocaleTimeFormatter::new("de-DE");
        assert_eq!(
            german.format(time, TickMarkType::Month),
            localized("Mär", "Mar")
        );
        assert_eq!(
            german.format_crosshair(time, 60),
            localized("Mi 13 Mär '24 14:30", "Wed 13 Mar '24 14:30")
        );
        let japanese = LocaleTimeFormatter::new("ja_JP");
        assert_eq!(
            japanese.format(time, TickMarkType::Month),
            localized("3月", "Mar")
        );
        assert_eq!(
            LocaleTimeFormatter::new("xx-YY").format(time, TickMarkType::Month),
            "Mar"
        );
        let dated = |date_format: &str| {
            DefaultTimeFormatter {
                date_format: Some(date_format.into()),
                ..Default::default()
            }
            .clone_box()
        };
        assert_eq!(
            dated("%d.%m.%Y").format_crosshair(time, 60),
            "13.03.2024 14:30"
        );
        assert_eq!(
            dated("%d.%m.%Y").format_crosshair(time, 86_400),
            "13.03.2024"
        );
        // An invalid pattern keeps the built-in date rather than failing to format.
        assert_eq!(
            dated("%Q").format_crosshair(time, 60),
            "Wed 13 Mar '24 14:30"
        );
        let current = time.with_year(Utc::now().year()).unwrap();
        let previous = current.with_year(current.year() - 1).unwrap();
        let default = DefaultTimeFormatter::default().clone_box();
        assert_eq!(default.format_crosshair(current, 86_400), "Mar 13");
        assert_eq!(
            default.format_crosshair(previous, 86_400),
            format!("Mar 13, {}", previous.year())
        );
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

        let cloned = formatter.clone_box();
        assert_eq!(cloned.format(time, TickMarkType::Year), "Year 2024");
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
    fn builder_overrides_host_calendar_and_clock_visibility() {
        #[derive(Clone)]
        struct Host;
        impl TimeFormatter for Host {
            fn format(&self, time: DateTime<Utc>, _: TickMarkType) -> String {
                time.to_rfc3339()
            }
            fn wall_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
                time + chrono::Duration::hours(1)
            }
            fn utc_time(&self, _: DateTime<Utc>) -> Option<DateTime<Utc>> {
                None
            }
            fn time_visible(&self) -> bool {
                false
            }
            fn clone_box(&self) -> Box<dyn TimeFormatter> {
                Box::new(self.clone())
            }
        }
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let formatter = TimeFormatterBuilder::new()
            .with_formatter(Box::new(Host))
            .with_timezone(TimezoneMode::Utc)
            .build();
        for formatter in [formatter.clone_box(), formatter] {
            assert!(formatter.time_visible());
            assert_eq!(formatter.wall_time(time), time);
            assert_eq!(formatter.utc_time(time), Some(time));
            assert_eq!(
                formatter.format(time, TickMarkType::Time),
                time.to_rfc3339()
            );
        }
    }

    #[test]
    fn test_timezone_conversion_utc_and_fixed_offsets() {
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        for (name, label, wall) in [
            ("UTC", "14:30", "2024-06-15T14:30:00Z"),
            ("+05:45", "20:15", "2024-06-15T20:15:00Z"),
            ("-03:30", "11:00", "2024-06-15T11:00:00Z"),
            ("+14:00", "04:30", "2024-06-16T04:30:00Z"),
            ("-12:00", "02:30", "2024-06-15T02:30:00Z"),
            ("+00:19:32", "14:49", "2024-06-15T14:49:32Z"),
            ("+23:59:59", "14:29", "2024-06-16T14:29:59Z"),
            ("-23:59:59", "14:30", "2024-06-14T14:30:01Z"),
        ] {
            let mode = TimezoneMode::Timezone(name.parse().unwrap());
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!(r#"{{"Timezone":"{name}"}}"#));
            assert_eq!(serde_json::from_str::<TimezoneMode>(&json).unwrap(), mode);
            let formatter = TimeFormatterBuilder::new()
                .with_24_hour(true)
                .with_seconds(false)
                .with_timezone(mode)
                .build();
            assert_eq!(formatter.format(time, TickMarkType::Time), label, "{name}");
            assert_eq!(
                formatter.wall_time(time),
                wall.parse::<DateTime<Utc>>().unwrap(),
                "{name}"
            );
            assert_eq!(formatter.utc_time(formatter.wall_time(time)), Some(time));
        }
        for invalid in [
            "",
            "+24:00",
            "-24:00",
            "+01:60",
            "+01:00:60",
            "+01:00junk",
            "+１:00",
            "unknown",
        ] {
            assert!(invalid.parse::<TimezoneId>().is_err(), "{invalid}");
        }
        for seconds in [i32::MIN, -86_400, 86_400, i32::MAX] {
            assert!(TimezoneId::fixed_offset(seconds).is_none());
        }
        assert_eq!(
            TimezoneId::named("America/New_York").is_some(),
            cfg!(feature = "timezones")
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn test_timezone_conversion_ny() {
        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(false)
            .with_timezone(TimezoneMode::Timezone("America/New_York".parse().unwrap()))
            .build();

        // June 15, 2024 14:30 UTC = 10:30 EDT (UTC-4)
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let result = formatter.format(time, TickMarkType::Time);
        assert_eq!(result, "10:30");
        assert_eq!(
            formatter.format_crosshair(time, 3600),
            "Sat 15 Jun '24 10:30"
        );
        let fractional = time + chrono::Duration::milliseconds(123);
        assert_eq!(
            TimeFormatterBuilder::new()
                .with_timezone(TimezoneMode::nyse())
                .build()
                .format_crosshair(fractional, 0),
            "Sat 15 Jun '24 10:30:00.123"
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn test_dst_spring_forward() {
        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(true)
            .with_timezone(TimezoneMode::Exchange("America/New_York".parse().unwrap()))
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

    #[cfg(feature = "timezones")]
    #[test]
    fn test_dst_fall_back() {
        let formatter = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(true)
            .with_timezone(TimezoneMode::Exchange("America/New_York".parse().unwrap()))
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

    #[cfg(feature = "timezones")]
    #[test]
    fn test_timezone_london() {
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

    #[cfg(feature = "timezones")]
    #[test]
    fn test_timezone_tokyo() {
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

    #[cfg(feature = "timezones")]
    #[test]
    fn test_timezone_clone() {
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
