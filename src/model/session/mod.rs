//! Trading session management and break detection.
//!
//! Financial markets operate in defined sessions (e.g., NYSE 9:30 AM--4:00 PM
//! ET). This module provides a pluggable [`SessionProvider`] trait for
//! detecting session boundaries (daily, weekly, monthly, or custom) within
//! bar data, plus concrete implementations:
//!
//! | Provider | Detects |
//! |----------|---------|
//! | [`DailySessionProvider`] | Day boundaries (configurable break hour) |
//! | [`ExchangeSessionProvider`] | Session opens of an instrument's trading hours |
//! | [`WeeklySessionProvider`] | Week boundaries (configurable break day) |
//! | [`MonthlySessionProvider`] | Month boundaries |
//! | [`CompositeSessionProvider`] | Combines multiple providers |
//!
//! Use [`find_session_breaks`] to scan a bar slice for all breaks.

mod exchange;

pub use exchange::ExchangeSessionProvider;

use crate::model::{Bar, TimezoneId, local_instant, local_time};
use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc, Weekday};

/// Trait for defining trading session boundaries.
///
/// Implement this to create custom session-break logic (e.g., exchange-
/// specific holidays, pre-market/after-hours splits, or futures roll
/// dates). All built-in providers implement this trait.
pub trait SessionProvider: Send + Sync {
    /// Check if a session break occurs between two ts
    /// Returns Some(SessionBreak) if there's a break, None otherwise
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak>;

    /// Get all session breaks within a time range, checking at most one minute at a time.
    /// The final check stops at `end`; an empty or reversed range has no breaks.
    fn session_breaks_in_range(
        &self,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> Vec<SessionBreak> {
        let mut breaks = Vec::new();
        let mut current = *start;

        while current < *end {
            let next = current
                .checked_add_signed(Duration::minutes(1))
                .unwrap_or(*end)
                .min(*end);
            if let Some(session_break) = self.session_break_between(&current, &next) {
                breaks.push(session_break);
            }
            current = next;
        }

        breaks
    }

    /// The breaks between consecutive `bars`, as [`find_session_breaks`] returns them. The
    /// default asks [`session_break_between`](Self::session_break_between) for each pair; a
    /// provider can find them in one pass instead.
    fn session_breaks(&self, bars: &[Bar]) -> Vec<(usize, SessionBreak)> {
        let pairs = bars.windows(2).enumerate();
        pairs
            .filter_map(|(i, pair)| {
                let session_break = self.session_break_between(&pair[0].time, &pair[1].time)?;
                Some((i + 1, session_break))
            })
            .collect()
    }

    /// Get display name for this session provider
    fn name(&self) -> &str;
}

/// A detected session break with metadata.
///
/// Produced by [`SessionProvider::session_break_between`] when a session
/// boundary falls between two timestamps. Contains the break time, type,
/// and an optional human-readable label for rendering on the time axis.
#[derive(Debug, Clone)]
pub struct SessionBreak {
    /// Ts of the break
    pub ts: DateTime<Utc>,
    /// Type of session break
    pub break_type: SessionBreakType,
    /// Optional label to display
    pub label: Option<String>,
}

/// Classification of a session break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBreakType {
    /// Daily session break (end of trading day)
    Daily,
    /// Weekly session break (end of trading week)
    Weekly,
    /// Monthly session break (end of trading month)
    Monthly,
    /// Custom session break
    Custom,
    /// Yearly session break (end of calendar year)
    Yearly,
}

/// Daily session provider with a configurable break hour.
///
/// Detects day boundaries at the break hour and minute, in its zone (UTC by default). Use `nyse` (with `timezones`) for a preset
/// matching NYSE regular-session close in exchange time, or [`continuous`](Self::continuous)
/// for 24/7 markets (midnight UTC).
#[derive(Debug, Clone)]
pub struct DailySessionProvider {
    /// Hour of day for session break (0-23)
    pub break_hour: u32,
    /// Minute of hour for session break (0-59)
    pub break_minute: u32,
    /// Zone in which the daily break is defined.
    pub time_zone: TimezoneId,
}

impl Default for DailySessionProvider {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl DailySessionProvider {
    /// Create a daily provider that breaks at the given UTC hour and minute.
    pub fn new(hour: u32, minute: u32) -> Self {
        Self {
            break_hour: hour.min(23),
            break_minute: minute.min(59),
            time_zone: TimezoneId::utc(),
        }
    }

    /// Create provider for NYSE regular session (9:30 AM - 4:00 PM ET)
    #[cfg(feature = "timezones")]
    pub fn nyse() -> Self {
        Self {
            time_zone: TimezoneId::named("America/New_York").unwrap(),
            ..Self::new(16, 0)
        }
    }

    /// Create provider for 24/7 markets (midnight UTC)
    pub fn continuous() -> Self {
        Self::default()
    }
}

impl SessionProvider for DailySessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        let day = |time: &DateTime<Utc>| {
            let local = local_time(self.time_zone.clone(), *time);
            let clock =
                NaiveTime::from_hms_opt(self.break_hour.min(23), self.break_minute.min(59), 0)
                    .unwrap();
            let boundary = local_instant(self.time_zone.clone(), local.date().and_time(clock));
            if boundary.is_some_and(|boundary| *time < boundary) {
                local.date().pred_opt().unwrap_or(local.date())
            } else {
                local.date()
            }
        };

        if day(from) != day(to) {
            Some(SessionBreak {
                ts: *to,
                break_type: SessionBreakType::Daily,
                label: Some(day(to).format("%Y-%m-%d").to_string()),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Daily Sessions"
    }
}

/// Weekly session provider that detects ISO-week boundaries.
///
/// Defaults to Friday (typical equity-market week end). Configure with
/// [`new`](Self::new) to use a different day.
#[derive(Debug, Clone)]
pub struct WeeklySessionProvider {
    /// Day of week for session break
    pub break_day: Weekday,
}

impl Default for WeeklySessionProvider {
    fn default() -> Self {
        Self {
            break_day: Weekday::Fri, // Friday close
        }
    }
}

impl WeeklySessionProvider {
    /// Create a weekly provider that breaks when the given weekday changes.
    pub fn new(break_day: Weekday) -> Self {
        Self { break_day }
    }
}

impl SessionProvider for WeeklySessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        let from_week = from.iso_week().week();
        let to_week = to.iso_week().week();

        if from_week != to_week {
            Some(SessionBreak {
                ts: *to,
                break_type: SessionBreakType::Weekly,
                label: Some(format!("Week {to_week}")),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Weekly Sessions"
    }
}

/// Monthly session provider that detects calendar-month boundaries.
#[derive(Debug, Clone, Default)]
pub struct MonthlySessionProvider;

impl SessionProvider for MonthlySessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        if from.month() != to.month() || from.year() != to.year() {
            Some(SessionBreak {
                ts: *to,
                break_type: SessionBreakType::Monthly,
                label: Some(format!("{}", to.format("%B %Y"))),
            })
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Monthly Sessions"
    }
}

/// Composite session provider that combines multiple providers.
///
/// When checking for a break, the first provider that reports one wins
/// (short-circuit). Use [`standard`](Self::standard) for the common
/// daily + weekly + monthly combination.
pub struct CompositeSessionProvider {
    providers: Vec<Box<dyn SessionProvider>>,
}

impl CompositeSessionProvider {
    /// Create an empty composite (no providers yet).
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a session provider to the composite.
    pub fn add_provider(&mut self, provider: Box<dyn SessionProvider>) {
        self.providers.push(provider);
    }

    /// Create standard provider with daily, weekly, and monthly breaks
    pub fn standard() -> Self {
        let mut composite = Self::new();
        composite.add_provider(Box::new(DailySessionProvider::default()));
        composite.add_provider(Box::new(WeeklySessionProvider::default()));
        composite.add_provider(Box::new(MonthlySessionProvider));
        composite
    }
}

impl Default for CompositeSessionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionProvider for CompositeSessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        // Return the highest priority break (Monthly > Weekly > Daily)
        for provider in &self.providers {
            if let Some(break_info) = provider.session_break_between(from, to) {
                return Some(break_info);
            }
        }
        None
    }

    fn name(&self) -> &str {
        "Composite Sessions"
    }
}

/// Scan a bar slice for session breaks.
///
/// Returns a `Vec` of `(bar_index, SessionBreak)` pairs indicating
/// where each break falls. The `bar_index` is the index of the bar
/// *after* the break (i.e., the first bar of the new session).
pub fn find_session_breaks(
    bars: &[Bar],
    provider: &dyn SessionProvider,
) -> Vec<(usize, SessionBreak)> {
    provider.session_breaks(bars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_bars() -> Vec<Bar> {
        vec![
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 102.0,
                volume: 1000.0,
                colors: None,
                payload: None,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap(),
                open: 102.0,
                high: 107.0,
                low: 101.0,
                close: 105.0,
                volume: 1200.0,
                colors: None,
                payload: None,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 2, 1, 12, 0, 0).unwrap(),
                open: 105.0,
                high: 110.0,
                low: 104.0,
                close: 108.0,
                volume: 1500.0,
                colors: None,
                payload: None,
            },
        ]
    }

    #[test]
    fn test_daily_session_break() {
        for (provider, before, at) in [
            (
                DailySessionProvider::new(21, 0),
                "2024-03-14T20:59:59Z",
                "2024-03-14T21:00:00Z",
            ),
            #[cfg(feature = "timezones")]
            (
                DailySessionProvider::nyse(),
                "2024-03-11T19:59:59Z",
                "2024-03-11T20:00:00Z",
            ),
            #[cfg(feature = "timezones")]
            (
                DailySessionProvider::nyse(),
                "2024-11-04T20:59:59Z",
                "2024-11-04T21:00:00Z",
            ),
            (
                DailySessionProvider {
                    time_zone: TimezoneId::fixed_offset(5 * 3600 + 45 * 60).unwrap(),
                    ..DailySessionProvider::new(16, 0)
                },
                "2024-03-14T10:14:59Z",
                "2024-03-14T10:15:00Z",
            ),
            (
                DailySessionProvider::continuous(),
                "2024-01-01T12:00:00Z",
                "2025-01-01T12:00:00Z",
            ),
        ] {
            let before = before.parse().unwrap();
            let at = at.parse().unwrap();
            assert!(provider.session_break_between(&before, &at).is_some());
            assert!(
                provider
                    .session_break_between(&at, &(at + Duration::seconds(1)))
                    .is_none()
            );
        }
        assert_eq!(
            find_session_breaks(&create_test_bars(), &DailySessionProvider::default()).len(),
            2
        );
    }

    #[test]
    fn session_range_stays_within_its_end_at_timestamp_limits() {
        let midnight = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        let max = DateTime::<Utc>::MAX_UTC;
        let min = DateTime::<Utc>::MIN_UTC;
        for (start, end, expected) in [
            (max - Duration::seconds(1), max, None),
            (min, min + Duration::seconds(1), None),
            (max, max, None),
            (max, min, None),
            (
                midnight - Duration::seconds(30),
                midnight - Duration::seconds(1),
                None,
            ),
            (midnight - Duration::seconds(30), midnight, Some(midnight)),
            (
                midnight - Duration::seconds(30),
                midnight + Duration::seconds(10),
                Some(midnight + Duration::seconds(10)),
            ),
        ] {
            let breaks = DailySessionProvider::continuous().session_breaks_in_range(&start, &end);
            assert_eq!(
                breaks.iter().map(|item| item.ts).collect::<Vec<_>>(),
                expected.into_iter().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_monthly_session_break() {
        let provider = MonthlySessionProvider;
        let bars = create_test_bars();

        let breaks = find_session_breaks(&bars, &provider);
        assert_eq!(breaks.len(), 1); // 1 month change (Jan -> Feb)

        assert_eq!(breaks[0].0, 2); // Break at index 2
        assert_eq!(breaks[0].1.break_type, SessionBreakType::Monthly);
    }

    #[test]
    fn test_composite_provider() {
        let mut composite = CompositeSessionProvider::new();
        composite.add_provider(Box::new(DailySessionProvider::default()));
        composite.add_provider(Box::new(MonthlySessionProvider));

        let bars = create_test_bars();
        let breaks = find_session_breaks(&bars, &composite);

        // Should detect both daily and monthly breaks
        assert!(!breaks.is_empty());
    }
}
