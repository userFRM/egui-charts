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

use crate::model::{Bar, Session, Timeframe, TimezoneId};
use chrono::{
    DateTime, Datelike, Days, Duration, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone,
    Timelike, Utc, Weekday,
};

/// Local fields, falling back to UTC only beyond the representable date range.
pub(crate) fn local_time<T: TimeZone>(zone: T, time: DateTime<Utc>) -> NaiveDateTime {
    time.naive_utc()
        .checked_add_offset(zone.offset_from_utc_datetime(&time.naive_utc()).fix())
        .unwrap_or(time.naive_utc())
}

/// The first instant of a repeated wall time; a skipped time opens at the clock jump.
pub(crate) fn local_instant<T: TimeZone>(
    zone: T,
    mut local: NaiveDateTime,
) -> Option<DateTime<Utc>> {
    if let Some(time) = zone.from_local_datetime(&local).earliest() {
        return Some(time.to_utc());
    }
    local = local.with_nanosecond(0)?;
    for _ in 0..=24 * 60 * 60 {
        local = local.checked_add_signed(Duration::seconds(1))?;
        if let Some(time) = zone.from_local_datetime(&local).earliest() {
            return Some(time.to_utc());
        }
    }
    None
}

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

    /// Get all session breaks within a time range
    fn session_breaks_in_range(
        &self,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> Vec<SessionBreak> {
        let mut breaks = Vec::new();
        let mut current = *start;

        while current < *end {
            let next = current + Duration::minutes(1);
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
}

/// Daily session provider with a configurable break hour.
///
/// Detects day boundaries at the break hour and minute, in its zone (UTC by default). Use [`nyse`](Self::nyse) for a preset
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
            let local = local_time(self.time_zone.tz(), *time);
            let clock =
                NaiveTime::from_hms_opt(self.break_hour.min(23), self.break_minute.min(59), 0)
                    .unwrap();
            let boundary = local_instant(self.time_zone.tz(), local.date().and_time(clock));
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

/// Daily session provider for an exchange: a day starts at each session open of its trading
/// hours, in its time zone, so bars outside the hours stay with the session before them.
/// Without trading hours a day starts at midnight in the time zone.
#[derive(Debug, Clone)]
pub struct ExchangeSessionProvider {
    time_zone: TimezoneId,
    session: Option<Session>,
    timeframe: Timeframe,
    local_zone: bool,
}

impl ExchangeSessionProvider {
    /// Days of `session`, or calendar days without one, in `time_zone`.
    pub fn new(time_zone: TimezoneId, session: Option<Session>) -> Self {
        Self {
            time_zone,
            session,
            timeframe: Timeframe::Min1,
            local_zone: false,
        }
    }

    /// Calendar sessions in the system's local zone when no exchange zone is known.
    pub fn local(session: Option<Session>) -> Self {
        Self {
            local_zone: true,
            ..Self::new(TimezoneId::utc(), session)
        }
    }

    fn wall_time(&self, time: DateTime<Utc>) -> NaiveDateTime {
        if self.local_zone {
            local_time(chrono::Local, time)
        } else {
            local_time(self.time_zone.tz(), time)
        }
    }

    fn instant(&self, time: NaiveDateTime) -> Option<DateTime<Utc>> {
        if self.local_zone {
            local_instant(chrono::Local, time)
        } else {
            local_instant(self.time_zone.tz(), time)
        }
    }

    fn close_instant(&self, time: NaiveDateTime) -> Option<DateTime<Utc>> {
        let latest = if self.local_zone {
            chrono::Local
                .from_local_datetime(&time)
                .latest()
                .map(|time| time.to_utc())
        } else {
            self.time_zone
                .tz()
                .from_local_datetime(&time)
                .latest()
                .map(|time| time.to_utc())
        };
        latest.or_else(|| self.instant(time))
    }

    /// Draw week boundaries on daily bars and month boundaries on longer bars.
    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.timeframe = timeframe;
        self
    }

    /// The trading date containing `time`, using the earliest window of that date.
    pub fn trading_date(&self, time: DateTime<Utc>) -> NaiveDate {
        if self.timeframe.duration_ms() >= Timeframe::Day1.duration_ms() {
            self.wall_time(time).date()
        } else {
            self.trading_date_in(&self.opens([time]), time)
        }
    }

    /// Whether `time` lies in extended hours and outside regular hours.
    pub fn is_extended(&self, time: DateTime<Utc>) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let contains =
            |window: &crate::model::SessionWindow| self.window_close(time, window).is_some();
        session.extended_windows.iter().any(contains) && !session.windows.iter().any(contains)
    }

    /// Closing instant of the active trading window. Outside trading hours, including holidays,
    /// returns `None`. Extended windows participate only when requested.
    pub fn session_close(&self, time: DateTime<Utc>, extended: bool) -> Option<DateTime<Utc>> {
        let session = self.session.as_ref()?;
        session
            .windows
            .iter()
            .chain(session.extended_windows.iter().filter(|_| extended))
            .filter_map(|window| self.window_close(time, window))
            .max()
    }

    /// Close of the last trading window in a daily, weekly or monthly bar. Daily and longer
    /// bars carry their trading date in UTC; intraday bars use their active exchange window.
    pub fn bar_session_close(
        &self,
        open: DateTime<Utc>,
        timeframe: Timeframe,
        extended: bool,
    ) -> Option<DateTime<Utc>> {
        if timeframe.duration_ms() < 86_400_000 {
            return self.session_close(open, extended);
        }
        let session = self.session.as_ref()?;
        let mut date = open.date_naive();
        let end = match timeframe {
            Timeframe::Day1 => date.succ_opt()?,
            Timeframe::Week1 => date.checked_add_days(chrono::Days::new(7))?,
            Timeframe::Month1 => date
                .with_day(1)?
                .checked_add_months(chrono::Months::new(1))?,
            _ => return None,
        };
        let mut close = None;
        while date < end {
            for window in session
                .windows
                .iter()
                .chain(session.extended_windows.iter().filter(|_| extended))
            {
                let overnight = window.close <= window.open && window.open != NaiveTime::MIN;
                let open_date = if overnight { date.pred_opt()? } else { date };
                if let Some(start) = self.instant(open_date.and_time(window.open))
                    && let Some(end) = self.window_close(start, window)
                {
                    close = Some(close.map_or(end, |previous: DateTime<Utc>| previous.max(end)));
                }
            }
            date = date.succ_opt()?;
        }
        close
    }

    fn window_close(
        &self,
        time: DateTime<Utc>,
        window: &crate::model::SessionWindow,
    ) -> Option<DateTime<Utc>> {
        let session = self.session.as_ref()?;
        let local = self.wall_time(time);
        let overnight = window.close <= window.open && window.open != NaiveTime::MIN;
        [
            Some(local.date()),
            local.date().succ_opt().filter(|_| overnight),
        ]
        .into_iter()
        .flatten()
        .find_map(|close_date| {
            if session.holidays.contains(&close_date)
                || !window.days.contains(&close_date.weekday())
            {
                return None;
            }
            let open_date = if overnight {
                close_date.pred_opt()?
            } else {
                close_date
            };
            let end_date = if window.open == window.close && !overnight {
                close_date.succ_opt()?
            } else {
                close_date
            };
            let open = self.instant(open_date.and_time(window.open))?;
            // The later close retains the full repeated hour in an overnight window.
            let close = self.close_instant(end_date.and_time(window.close))?;
            (time >= open && time < close).then_some(close)
        })
    }

    fn period(&self, date: NaiveDate) -> (i32, u32) {
        if self.timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
            (date.year(), date.ordinal())
        } else if self.timeframe == Timeframe::Day1 {
            (date.iso_week().year(), date.iso_week().week())
        } else {
            (date.year(), date.month())
        }
    }

    /// The instants trading days start around `times`, sorted, each with its trading date: the
    /// session opens whose windows close on dates from at least 9 days before the UTC date
    /// of a time to 2 days after it (the local date is within a day of the UTC one, a weekly schedule opens
    /// at least once a week, and a window open since the evening closes the next day), or local
    /// midnights without trading hours. The range extends back through closures to a valid
    /// trading date. Days
    /// far from every time are skipped, so the work follows the times, not the span between
    /// them. An open that falls in a repeated hour is its first instant, and one that falls in
    /// a skipped hour is the instant the clocks jump.
    fn opens(
        &self,
        times: impl IntoIterator<Item = DateTime<Utc>>,
    ) -> Vec<(DateTime<Utc>, NaiveDate)> {
        // UTC days since the epoch: whole numbers, cheaper per bar than calendar dates.
        let mut days: Vec<i64> = Vec::new();
        for day in times
            .into_iter()
            .map(|time| time.timestamp().div_euclid(86_400))
        {
            if days.last() != Some(&day) {
                days.push(day);
            }
        }
        days.sort_unstable();
        days.dedup();
        let dates = days
            .into_iter()
            .filter_map(|day| DateTime::from_timestamp(day * 86_400, 0))
            .map(|start| start.date_naive());
        // Disjoint, sorted ranges of days, so that each day is visited once.
        let mut ranges: Vec<(NaiveDate, NaiveDate)> = Vec::new();
        for date in dates {
            let mut first = date
                .checked_sub_days(Days::new(9))
                .unwrap_or(NaiveDate::MIN);
            if let Some(session) = &self.session {
                let windows = || session.windows.iter().chain(&session.extended_windows);
                let continuous = windows().next().is_none();
                let scheduled = windows().any(|window| !window.days.is_empty());
                if continuous || scheduled {
                    while session.holidays.contains(&first)
                        || (!continuous
                            && !windows().any(|window| window.days.contains(&first.weekday())))
                    {
                        let Some(previous) = first.pred_opt() else {
                            break;
                        };
                        first = previous;
                    }
                }
            }
            let last = date.checked_add_days(Days::new(2)).unwrap_or(date);
            match ranges.last_mut() {
                Some((_, end)) if first <= *end => *end = last.max(*end),
                _ => ranges.push((first, last)),
            }
        }
        let days = ranges
            .into_iter()
            .flat_map(|(first, last)| first.iter_days().take_while(move |date| *date <= last));
        let mut opens: Vec<_> = match &self.session {
            None => days
                .filter_map(|date| Some((self.instant(date.and_time(NaiveTime::MIN))?, date)))
                .collect(),
            Some(session) if session.windows.is_empty() && session.extended_windows.is_empty() => {
                days.filter(|date| !session.holidays.contains(date))
                    .filter_map(|date| Some((self.instant(date.and_time(NaiveTime::MIN))?, date)))
                    .collect()
            }
            Some(session) => days
                .filter_map(|close_date| {
                    if session.holidays.contains(&close_date) {
                        return None;
                    }
                    // All windows of one trading date share its earliest open, including split sessions.
                    let open = session
                        .windows
                        .iter()
                        .chain(&session.extended_windows)
                        .filter(|window| window.days.contains(&close_date.weekday()))
                        .filter_map(|window| {
                            let open_date =
                                if window.close <= window.open && window.open != NaiveTime::MIN {
                                    close_date.pred_opt()?
                                } else {
                                    close_date
                                };
                            self.instant(open_date.and_time(window.open))
                        })
                        .min()?;
                    Some((open, close_date))
                })
                .collect(),
        };
        opens.sort_by_key(|(open, _)| *open);
        opens
    }

    /// The trading date of `time` among `opens`: that of the last open at or before it, or the
    /// calendar date in the time zone before the first.
    fn trading_date_in(
        &self,
        opens: &[(DateTime<Utc>, NaiveDate)],
        time: DateTime<Utc>,
    ) -> NaiveDate {
        match opens.partition_point(|(open, _)| *open <= time) {
            0 => self.wall_time(time).date(),
            n => opens[n - 1].1,
        }
    }

    fn session_break(&self, date: NaiveDate, ts: DateTime<Utc>) -> SessionBreak {
        SessionBreak {
            ts,
            break_type: if self.timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
                SessionBreakType::Daily
            } else if self.timeframe == Timeframe::Day1 {
                SessionBreakType::Weekly
            } else {
                SessionBreakType::Monthly
            },
            label: Some(date.format("%Y-%m-%d").to_string()),
        }
    }
}

impl SessionProvider for ExchangeSessionProvider {
    fn session_break_between(
        &self,
        from: &DateTime<Utc>,
        to: &DateTime<Utc>,
    ) -> Option<SessionBreak> {
        if self.timeframe.duration_ms() >= Timeframe::Day1.duration_ms() {
            let date = self.wall_time(*to).date();
            return (self.period(self.wall_time(*from).date()) != self.period(date))
                .then(|| self.session_break(date, *to));
        }
        let opens = self.opens([*from, *to]);
        let date = self.trading_date_in(&opens, *to);
        (self.period(self.trading_date_in(&opens, *from)) != self.period(date))
            .then(|| self.session_break(date, *to))
    }

    /// Finds the session opens around the bars' days once and places each bar among them.
    fn session_breaks(&self, bars: &[Bar]) -> Vec<(usize, SessionBreak)> {
        let dates: Vec<_> = if self.timeframe.duration_ms() >= Timeframe::Day1.duration_ms() {
            bars.iter()
                .map(|bar| self.wall_time(bar.time).date())
                .collect()
        } else {
            let opens = self.opens(bars.iter().map(|bar| bar.time));
            bars.iter()
                .map(|bar| self.trading_date_in(&opens, bar.time))
                .collect()
        };
        (1..bars.len())
            .filter(|&i| self.period(dates[i]) != self.period(dates[i - 1]))
            .map(|i| (i, self.session_break(dates[i], bars[i].time)))
            .collect()
    }

    fn name(&self) -> &str {
        "Exchange Sessions"
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
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap(),
                open: 102.0,
                high: 107.0,
                low: 101.0,
                close: 105.0,
                volume: 1200.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 2, 1, 12, 0, 0).unwrap(),
                open: 105.0,
                high: 110.0,
                low: 104.0,
                close: 108.0,
                volume: 1500.0,
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
            (
                DailySessionProvider::nyse(),
                "2024-03-11T19:59:59Z",
                "2024-03-11T20:00:00Z",
            ),
            (
                DailySessionProvider::nyse(),
                "2024-11-04T20:59:59Z",
                "2024-11-04T21:00:00Z",
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
    fn exchange_dates_follow_dst_windows_and_holidays() {
        use crate::model::SessionWindow;
        use Weekday::*;
        let clock = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let every_day = [Mon, Tue, Wed, Thu, Fri, Sat, Sun];
        let window = |open, close| SessionWindow::new(every_day, clock(open, 30), clock(close, 0));
        // Local midnight to the following midnight spans 23 or 25 hours on these dates.
        for (zone, midnight, next, hours) in [
            (
                "America/New_York",
                "2024-03-10T05:00:00Z",
                "2024-03-11T04:00:00Z",
                23,
            ),
            (
                "America/New_York",
                "2024-11-03T04:00:00Z",
                "2024-11-04T05:00:00Z",
                25,
            ),
            (
                "Europe/Zurich",
                "2024-03-30T23:00:00Z",
                "2024-03-31T22:00:00Z",
                23,
            ),
            (
                "Europe/Zurich",
                "2024-10-26T22:00:00Z",
                "2024-10-27T23:00:00Z",
                25,
            ),
        ] {
            let provider = ExchangeSessionProvider::new(TimezoneId::named(zone).unwrap(), None);
            let midnight: DateTime<Utc> = midnight.parse().unwrap();
            let next: DateTime<Utc> = next.parse().unwrap();
            assert_eq!(next - midnight, Duration::hours(hours));
            for instant in [midnight, next] {
                let event = provider
                    .session_break_between(&(instant - Duration::seconds(1)), &instant)
                    .unwrap();
                assert_eq!(
                    event.label,
                    Some(provider.wall_time(instant).date().to_string())
                );
            }
            assert!(
                provider
                    .session_break_between(&midnight, &(next - Duration::seconds(1)))
                    .is_none()
            );
        }
        // A missing 02:30 opens at the jump; a repeated open occurs only at its first instant.
        for (zone, month, day, open_hour, instant) in [
            ("America/New_York", 3, 10, 2, "2024-03-10T07:00:00Z"),
            ("America/New_York", 11, 3, 1, "2024-11-03T05:30:00Z"),
            ("Europe/Zurich", 3, 31, 2, "2024-03-31T01:00:00Z"),
            ("Europe/Zurich", 10, 27, 2, "2024-10-27T00:30:00Z"),
        ] {
            let provider = ExchangeSessionProvider::new(
                TimezoneId::named(zone).unwrap(),
                Some(Session::new([window(open_hour, 23)])),
            );
            let instant: DateTime<Utc> = instant.parse().unwrap();
            assert_eq!(
                provider.wall_time(instant).date(),
                chrono::NaiveDate::from_ymd_opt(2024, month, day).unwrap()
            );
            assert!(
                provider
                    .session_break_between(&(instant - Duration::seconds(1)), &instant)
                    .is_some()
            );
            assert!(
                provider
                    .session_break_between(&instant, &(instant + Duration::hours(2)))
                    .is_none()
            );
        }
        // An overnight open in the repeated hour stays open when the clock moves back.
        for (zone, hour, open, close) in [
            (
                "America/New_York",
                1,
                "2024-11-03T05:30:00Z",
                "2024-11-04T06:00:00Z",
            ),
            (
                "Europe/Zurich",
                2,
                "2024-10-27T00:30:00Z",
                "2024-10-28T01:00:00Z",
            ),
        ] {
            let provider = ExchangeSessionProvider::new(
                TimezoneId::named(zone).unwrap(),
                Some(Session::new([]).with_extended_windows([SessionWindow::new(
                    [Mon],
                    clock(hour, 30),
                    clock(hour, 0),
                )])),
            );
            let open: DateTime<Utc> = open.parse().unwrap();
            let close: DateTime<Utc> = close.parse().unwrap();
            for (time, expected) in [
                (open - Duration::seconds(1), false),
                (open, true),
                (open + Duration::minutes(45), true),
                (close - Duration::seconds(1), true),
                (close, false),
            ] {
                assert_eq!(provider.is_extended(time), expected, "{zone} {time}");
            }
        }
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let session = Session::new([SessionWindow::new(weekdays, clock(9, 30), clock(16, 0))])
            .with_extended_windows([
                SessionWindow::new(weekdays, clock(4, 0), clock(9, 30)),
                SessionWindow::new(weekdays, clock(16, 0), clock(20, 0)),
            ])
            .with_holidays([chrono::NaiveDate::from_ymd_opt(2024, 7, 4).unwrap()]);
        let provider = ExchangeSessionProvider::new(
            TimezoneId::named("America/New_York").unwrap(),
            Some(session),
        );
        let times = [
            "2024-07-03T23:00:00Z",
            "2024-07-04T08:00:00Z",
            "2024-07-05T07:59:59Z",
            "2024-07-05T08:00:00Z",
            "2024-07-05T13:29:59Z",
            "2024-07-05T13:30:00Z",
            "2024-07-05T20:00:00Z",
            "2024-07-06T00:00:00Z",
        ];
        let bars: Vec<_> = times
            .iter()
            .map(|time| Bar::new(time.parse().unwrap(), 1.0, 1.0, 1.0, 1.0, 1.0))
            .collect();
        assert_eq!(
            find_session_breaks(&bars, &provider)
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            [3]
        );
        assert_eq!(
            bars.iter()
                .map(|bar| provider.is_extended(bar.time))
                .collect::<Vec<_>>(),
            [true, false, false, true, true, false, true, false]
        );
        // Split regular windows do not create a second trading day after lunch.
        let split = ExchangeSessionProvider::new(
            TimezoneId::utc(),
            Some(Session::new([
                SessionWindow::new(weekdays, clock(9, 0), clock(11, 0)),
                SessionWindow::new(weekdays, clock(13, 0), clock(16, 0)),
            ])),
        );
        assert!(
            split
                .session_break_between(
                    &"2024-07-05T10:00:00Z".parse().unwrap(),
                    &"2024-07-05T13:00:00Z".parse().unwrap()
                )
                .is_none()
        );
        let overnight = ExchangeSessionProvider::new(
            TimezoneId::named("America/New_York").unwrap(),
            Some(Session::new([SessionWindow::new(
                weekdays,
                clock(18, 0),
                clock(17, 0),
            )])),
        );
        let open = "2024-03-10T22:00:00Z".parse().unwrap();
        assert_eq!(
            overnight
                .session_break_between(&(open - Duration::seconds(1)), &open)
                .unwrap()
                .label
                .as_deref(),
            Some("2024-03-11")
        );
        let weekly = ExchangeSessionProvider::new(
            TimezoneId::utc(),
            Some(
                Session::new([SessionWindow::new([Mon], clock(9, 0), clock(16, 0))]).with_holidays(
                    [4, 11, 18].map(|day| NaiveDate::from_ymd_opt(2024, 3, day).unwrap()),
                ),
            ),
        );
        assert_eq!(
            weekly.trading_date("2024-03-25T08:00:00Z".parse().unwrap()),
            NaiveDate::from_ymd_opt(2024, 2, 26).unwrap()
        );
        for zone in ["America/New_York", "Europe/Zurich"] {
            let provider = ExchangeSessionProvider::new(TimezoneId::named(zone).unwrap(), None);
            for time in [DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC] {
                let _ = provider.session_break_between(&time, &time);
            }
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
