//! Exchange session provider: trading hours, DST and holidays.

use super::{SessionBreak, SessionBreakType, SessionProvider, local_instant, local_time};
use crate::model::{Bar, Session, Timeframe, TimezoneId};
use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

/// Daily session provider for an exchange: a day starts at each session open of its trading
/// hours, in its time zone, so bars outside the hours stay with the session before them.
/// Without trading hours a day starts at midnight in the time zone.
#[derive(Debug, Clone, PartialEq, Eq)]
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
            local_time(self.time_zone.clone(), time)
        }
    }

    fn instant(&self, time: NaiveDateTime) -> Option<DateTime<Utc>> {
        if self.local_zone {
            local_instant(chrono::Local, time)
        } else {
            local_instant(self.time_zone.clone(), time)
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

    /// Whether the exchange is open, optionally including extended windows.
    pub fn is_open(&self, time: DateTime<Utc>, extended_hours: bool) -> bool {
        let Some(session) = &self.session else {
            return true;
        };
        if session.windows.is_empty() && session.extended_windows.is_empty() {
            return !session.holidays.contains(&self.wall_time(time).date());
        }
        session
            .windows
            .iter()
            .chain(session.extended_windows.iter().filter(|_| extended_hours))
            .any(|window| self.window_close(time, window).is_some())
    }

    /// Closing instant of the active trading window. Outside trading hours, including holidays,
    /// returns `None`. Continuous sessions end at the next local midnight.
    /// Extended windows participate only when requested.
    pub fn session_close(&self, time: DateTime<Utc>, extended: bool) -> Option<DateTime<Utc>> {
        let session = self.session.as_ref()?;
        if session.windows.is_empty() && session.extended_windows.is_empty() {
            let date = self.wall_time(time).date();
            return if session.holidays.contains(&date) {
                None
            } else {
                self.instant(date.succ_opt()?.and_time(NaiveTime::MIN))
            };
        }
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
        if session.windows.is_empty() && (!extended || session.extended_windows.is_empty()) {
            return None;
        }
        let date = open.date_naive();
        let end = match timeframe {
            Timeframe::Day1 => date.succ_opt()?,
            Timeframe::Week1 => date.checked_add_days(chrono::Days::new(7))?,
            Timeframe::Month1 => date
                .with_day(1)?
                .checked_add_months(chrono::Months::new(1))?,
            Timeframe::Weeks(count) => {
                date.checked_add_days(chrono::Days::new(u64::from(count.get()) * 7))?
            }
            Timeframe::Months(count) => date
                .with_day(1)?
                .checked_add_months(chrono::Months::new(count.get()))?,
            _ => return None,
        };
        let start = date;
        let mut date = end.pred_opt()?;
        // Only the final trading date is needed, even for long calendar intervals.
        while date >= start {
            let mut close = None;
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
            if close.is_some() {
                return close;
            }
            date = date.pred_opt()?;
        }
        None
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
    pub(crate) fn opens(
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

#[cfg(all(test, feature = "timezones"))]
mod tests {
    use super::*;
    use crate::model::session::find_session_breaks;
    use chrono::{Duration, Weekday};

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
        assert_eq!(
            bars.iter()
                .map(|bar| provider.is_open(bar.time, false))
                .collect::<Vec<_>>(),
            [false, false, false, false, false, true, false, false]
        );
        assert_eq!(
            bars.iter()
                .map(|bar| provider.is_open(bar.time, true))
                .collect::<Vec<_>>(),
            [true, false, false, true, true, true, true, false]
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
        let regular = ExchangeSessionProvider::new(
            TimezoneId::named("America/New_York").unwrap(),
            Some(Session::new([SessionWindow::new(
                weekdays,
                clock(9, 30),
                clock(16, 0),
            )])),
        );
        for (open, timeframe, expected) in [
            (
                "2024-03-08T00:00:00Z",
                Timeframe::Day1,
                Some("2024-03-08T21:00:00Z"),
            ),
            (
                "2024-03-11T00:00:00Z",
                Timeframe::Day1,
                Some("2024-03-11T20:00:00Z"),
            ),
            (
                "2024-03-11T00:00:00Z",
                Timeframe::Week1,
                Some("2024-03-15T20:00:00Z"),
            ),
            (
                "2024-03-01T00:00:00Z",
                Timeframe::Month1,
                Some("2024-03-29T20:00:00Z"),
            ),
            (
                "2024-03-04T00:00:00Z",
                "2w".parse().unwrap(),
                Some("2024-03-15T20:00:00Z"),
            ),
            (
                "2024-04-01T00:00:00Z",
                "3month".parse().unwrap(),
                Some("2024-06-28T20:00:00Z"),
            ),
            (
                "2024-03-11T19:59:00Z",
                Timeframe::Min1,
                Some("2024-03-11T20:00:00Z"),
            ),
            ("2024-03-11T20:00:00Z", Timeframe::Min1, None),
            ("2024-03-10T00:00:00Z", Timeframe::Day1, None),
        ] {
            assert_eq!(
                regular.bar_session_close(open.parse().unwrap(), timeframe, false),
                expected.map(|time| time.parse().unwrap())
            );
        }
        for zone in ["America/New_York", "Europe/Zurich"] {
            let provider = ExchangeSessionProvider::new(TimezoneId::named(zone).unwrap(), None);
            for time in [DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC] {
                let _ = provider.session_break_between(&time, &time);
            }
        }
    }
}
