//! Exchange session provider: trading hours, DST and holidays.

use super::{SessionBreak, SessionBreakType, SessionProvider, local_instant, local_time};
use crate::model::{Bar, OverrideWindow, Session, Timeframe, TimezoneId};
use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

/// A window that closes on one trading date.
struct Hours {
    open: NaiveTime,
    close: NaiveTime,
    extended: bool,
}

impl Hours {
    fn new(open: NaiveTime, close: NaiveTime, extended: bool) -> Self {
        Self {
            open,
            close,
            extended,
        }
    }

    /// Whether the window opens on the day before it closes.
    fn overnight(&self) -> bool {
        self.close <= self.open && self.open != NaiveTime::MIN
    }
}

/// Daily session provider for an exchange: a day starts at each session open of its trading
/// hours, in its time zone, so bars outside the hours stay with the session before them.
/// Without trading hours a day starts at midnight in the time zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeSessionProvider {
    time_zone: TimezoneId,
    calendar_zone: Option<TimezoneId>,
    session: Option<Session>,
    timeframe: Timeframe,
    local_zone: bool,
}

impl ExchangeSessionProvider {
    /// Days of `session`, or calendar days without one, in `time_zone`.
    pub fn new(time_zone: TimezoneId, session: Option<Session>) -> Self {
        Self {
            calendar_zone: Some(time_zone.clone()),
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
            calendar_zone: None,
            ..Self::new(TimezoneId::utc(), session)
        }
    }

    /// The calendar that aligned daily and longer bars; `None` uses the system's local zone.
    pub(crate) fn with_calendar_zone(mut self, zone: Option<TimezoneId>) -> Self {
        self.calendar_zone = zone;
        self
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

    /// Draw week boundaries on daily bars, month boundaries on multi-day and weekly bars,
    /// and year boundaries on monthly bars shorter than a year. Yearly and longer bars have no dividers.
    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.timeframe = timeframe;
        self
    }

    /// The trading date containing `time`, using the earliest window of that date for intraday
    /// bars. Daily and longer bars carry their trading date in UTC, or in their clock's calendar
    /// or session zone when they open at its first instant of the date or at session midnight.
    pub fn trading_date(&self, time: DateTime<Utc>) -> NaiveDate {
        if self.timeframe.duration_ms() >= Timeframe::Day1.duration_ms() {
            self.bar_date(time)
        } else {
            self.trading_date_in(&self.opens([time]), time)
        }
    }

    /// The date of a daily or longer bar, recognizing skipped and repeated session midnights.
    fn bar_date(&self, open: DateTime<Utc>) -> NaiveDate {
        let (date, first) = match &self.calendar_zone {
            Some(zone) => {
                let date = local_time(zone.clone(), open).date();
                (
                    date,
                    local_instant(zone.clone(), date.and_time(NaiveTime::MIN)),
                )
            }
            None => {
                let date = local_time(chrono::Local, open).date();
                (
                    date,
                    local_instant(chrono::Local, date.and_time(NaiveTime::MIN)),
                )
            }
        };
        if first == Some(open) {
            return date;
        }
        let wall = self.wall_time(open);
        if wall.time() == NaiveTime::MIN
            || self.instant(wall.date().and_time(NaiveTime::MIN)) == Some(open)
        {
            wall.date()
        } else {
            open.date_naive()
        }
    }

    /// Whether `time` lies in extended hours and outside regular hours.
    pub fn is_extended(&self, time: DateTime<Utc>) -> bool {
        let closes = self.window_closes(time, true);
        closes.iter().any(|(extended, _)| *extended)
            && !closes.iter().any(|(extended, _)| !extended)
    }

    /// Whether the exchange is open, optionally including extended windows.
    pub fn is_open(&self, time: DateTime<Utc>, extended_hours: bool) -> bool {
        let Some(session) = &self.session else {
            return true;
        };
        let date = self.wall_time(time).date();
        if self.continuous_on(date) {
            return !session.holidays.contains(&date);
        }
        !self.window_closes(time, extended_hours).is_empty()
    }

    /// Closing instant of the active trading window. Outside trading hours, including holidays,
    /// returns `None`. Continuous sessions end at the next local midnight.
    /// Extended windows participate only when requested.
    pub fn session_close(&self, time: DateTime<Utc>, extended: bool) -> Option<DateTime<Utc>> {
        let session = self.session.as_ref()?;
        let date = self.wall_time(time).date();
        if self.continuous_on(date) {
            return if session.holidays.contains(&date) {
                None
            } else {
                self.instant(date.succ_opt()?.and_time(NaiveTime::MIN))
            };
        }
        self.window_closes(time, extended)
            .into_iter()
            .map(|(_, close)| close)
            .max()
    }

    /// Close of the last trading window in a daily, weekly or monthly bar. Daily and longer
    /// bars are dated as [`trading_date`](Self::trading_date) dates them; intraday bars use their
    /// active exchange window.
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
        if session.windows.is_empty()
            && session.overrides.is_empty()
            && (!extended || session.extended_windows.is_empty())
        {
            return None;
        }
        let date = self.bar_date(open);
        let end = match timeframe {
            _ if timeframe.duration_ms() == Timeframe::Day1.duration_ms() => date.succ_opt()?,
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
            let close = self
                .hours_on(date)
                .iter()
                .filter(|hours| extended || !hours.extended)
                .filter_map(|hours| self.window_span(date, hours))
                .map(|(_, close)| close)
                .max();
            if close.is_some() {
                return close;
            }
            date = date.pred_opt()?;
        }
        None
    }

    /// The windows that close on `date`, tagged extended or regular: those of its override, else
    /// none on a holiday, else the weekly windows of its weekday.
    fn hours_on(&self, date: NaiveDate) -> Vec<Hours> {
        let Some(session) = &self.session else {
            return Vec::new();
        };
        if let Some(day) = session.overrides.iter().find(|day| day.date == date) {
            let tag = |extended| move |w: &OverrideWindow| Hours::new(w.open, w.close, extended);
            return day
                .windows
                .iter()
                .map(tag(false))
                .chain(day.extended_windows.iter().map(tag(true)))
                .collect();
        }
        if session.holidays.contains(&date) {
            return Vec::new();
        }
        let tag = |extended| {
            move |w: &crate::model::SessionWindow| {
                w.days
                    .contains(&date.weekday())
                    .then(|| Hours::new(w.open, w.close, extended))
            }
        };
        session
            .windows
            .iter()
            .filter_map(tag(false))
            .chain(session.extended_windows.iter().filter_map(tag(true)))
            .collect()
    }

    /// Whether `date` has no windows and is open all day unless it is a holiday: a session
    /// without weekly windows and without an override for the date.
    fn continuous_on(&self, date: NaiveDate) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session.windows.is_empty()
                && session.extended_windows.is_empty()
                && !session.overrides.iter().any(|day| day.date == date)
        })
    }

    /// Whether anything trades on `date`.
    fn trades_on(&self, date: NaiveDate) -> bool {
        if self.continuous_on(date) {
            self.session
                .as_ref()
                .is_some_and(|session| !session.holidays.contains(&date))
        } else {
            !self.hours_on(date).is_empty()
        }
    }

    /// The opening instant of a window that closes on `close_date`.
    fn window_open(&self, close_date: NaiveDate, hours: &Hours) -> Option<DateTime<Utc>> {
        let open_date = if hours.overnight() {
            close_date.pred_opt()?
        } else {
            close_date
        };
        self.instant(open_date.and_time(hours.open))
    }

    /// The opening and closing instants of a window that closes on `close_date`.
    fn window_span(
        &self,
        close_date: NaiveDate,
        hours: &Hours,
    ) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let end_date = if hours.open == hours.close && !hours.overnight() {
            close_date.succ_opt()?
        } else {
            close_date
        };
        // The later close retains the full repeated hour in an overnight window.
        let close = self.close_instant(end_date.and_time(hours.close))?;
        Some((self.window_open(close_date, hours)?, close))
    }

    /// The closing instants of the windows containing `time`, each tagged extended or regular.
    /// Extended windows are left out unless `extended`.
    fn window_closes(&self, time: DateTime<Utc>, extended: bool) -> Vec<(bool, DateTime<Utc>)> {
        let date = self.wall_time(time).date();
        let mut closes = Vec::new();
        for (close_date, next_day) in [(Some(date), false), (date.succ_opt(), true)] {
            let Some(close_date) = close_date else {
                continue;
            };
            for hours in self.hours_on(close_date) {
                if (hours.extended && !extended) || (next_day && !hours.overnight()) {
                    continue;
                }
                if let Some((open, close)) = self.window_span(close_date, &hours)
                    && time >= open
                    && time < close
                {
                    closes.push((hours.extended, close));
                }
            }
        }
        closes
    }

    fn period(&self, date: NaiveDate) -> (i32, u32) {
        if self.timeframe.duration_ms() >= 12 * Timeframe::Month1.duration_ms() {
            (0, 0)
        } else if self.timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
            (date.year(), date.ordinal())
        } else if self.timeframe.duration_ms() == Timeframe::Day1.duration_ms() {
            (date.iso_week().year(), date.iso_week().week())
        } else if matches!(self.timeframe, Timeframe::Month1 | Timeframe::Months(_)) {
            (date.year(), 0)
        } else {
            (date.year(), date.month())
        }
    }

    /// The instants trading days start around `times`, sorted, each with its trading date: the
    /// session opens whose windows close on dates from at least 9 days before the UTC date
    /// of a time to 2 days after it (the local date is within a day of the UTC one, a weekly schedule opens
    /// at least once a week, and a window open since the evening closes the next day), or local
    /// midnights without trading hours. The range extends back through closures to a valid
    /// trading date. For times on one UTC day, older opens are omitted once the containing
    /// session and any overlapping trading date have been visited. Days
    /// far from every time are skipped, so the work follows the times, not the span between
    /// them. An open that falls in a repeated hour is its first instant, and one that falls in
    /// a skipped hour is the instant the clocks jump.
    pub(crate) fn opens(
        &self,
        times: impl IntoIterator<Item = DateTime<Utc>>,
    ) -> Vec<(DateTime<Utc>, NaiveDate)> {
        // UTC days since the epoch: whole numbers, cheaper per bar than calendar dates.
        let mut days: Vec<i64> = Vec::new();
        let mut earliest = DateTime::<Utc>::MAX_UTC;
        for time in times {
            earliest = earliest.min(time);
            let day = time.timestamp().div_euclid(86_400);
            if days.last() != Some(&day) {
                days.push(day);
            }
        }
        days.sort_unstable();
        days.dedup();
        let cutoff = (days.len() == 1).then_some(earliest);
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
                    while !self.trades_on(first) {
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
        let days = ranges.into_iter().flat_map(|(first, last)| {
            (0..=(last - first).num_days()).map(move |offset| last - Days::new(offset as u64))
        });
        let midnight = |date: NaiveDate| Some((self.instant(date.and_time(NaiveTime::MIN))?, date));
        let opening = |date| match &self.session {
            None => midnight(date),
            Some(session) => {
                if self.continuous_on(date) {
                    return if session.holidays.contains(&date) {
                        None
                    } else {
                        midnight(date)
                    };
                }
                // All windows of one trading date share its earliest open, including split sessions.
                let open = self
                    .hours_on(date)
                    .iter()
                    .filter_map(|hours| self.window_open(date, hours))
                    .min()?;
                Some((open, date))
            }
        };
        let mut opens: Vec<_> = days
            .scan(None, |stop, date| {
                if stop.is_some_and(|stop| date < stop) {
                    return None;
                }
                let entry = opening(date);
                if stop.is_none()
                    && let Some((open, _)) = entry
                    && cutoff.is_some_and(|time| open <= time)
                {
                    // An overnight window may open before the preceding date's daytime window.
                    *stop = Some(self.wall_time(open).date());
                }
                Some(entry)
            })
            .flatten()
            .collect();
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
            } else if self.timeframe.duration_ms() == Timeframe::Day1.duration_ms() {
                SessionBreakType::Weekly
            } else if matches!(self.timeframe, Timeframe::Month1 | Timeframe::Months(_)) {
                SessionBreakType::Yearly
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
            let date = self.trading_date(*to);
            return (self.period(self.trading_date(*from)) != self.period(date))
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
            bars.iter().map(|bar| self.trading_date(bar.time)).collect()
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
    fn calendar_dividers_follow_bar_dates_and_coarser_periods() {
        for (timeframe, dates, expected, break_type) in [
            (
                "12month".parse().unwrap(),
                &[
                    (2015, 1, 1),
                    (2016, 1, 1),
                    (2017, 1, 1),
                    (2018, 1, 1),
                    (2019, 1, 1),
                    (2020, 1, 1),
                    (2021, 1, 1),
                    (2022, 1, 1),
                    (2023, 1, 1),
                    (2024, 1, 1),
                ][..],
                &[][..],
                SessionBreakType::Yearly,
            ),
            (
                Timeframe::Day1,
                &[
                    (2024, 3, 7),
                    (2024, 3, 8),
                    (2024, 3, 11),
                    (2024, 3, 12),
                    (2024, 3, 13),
                ][..],
                &[(2, "2024-03-11")][..],
                SessionBreakType::Weekly,
            ),
            // Monday July 8 starts week 28, after the Thursday and Friday bars.
            (
                Timeframe::Day1,
                &[
                    (2024, 7, 4),
                    (2024, 7, 5),
                    (2024, 7, 8),
                    (2024, 7, 9),
                    (2024, 7, 10),
                ][..],
                &[(2, "2024-07-08")][..],
                SessionBreakType::Weekly,
            ),
            // The second Monday bar opens July, even east of UTC.
            (
                Timeframe::Week1,
                &[(2024, 6, 24), (2024, 7, 1), (2024, 7, 8)][..],
                &[(1, "2024-07-01")][..],
                SessionBreakType::Monthly,
            ),
            // Amman repeats midnight on October 29; all three dates are in week 43.
            (
                Timeframe::Day1,
                &[(2021, 10, 28), (2021, 10, 29), (2021, 10, 30)][..],
                &[][..],
                SessionBreakType::Weekly,
            ),
            // Tehran starts Monday at 01:00 after skipping midnight on March 22.
            (
                Timeframe::Day1,
                &[
                    (2021, 3, 19),
                    (2021, 3, 20),
                    (2021, 3, 21),
                    (2021, 3, 22),
                    (2021, 3, 23),
                ][..],
                &[(3, "2021-03-22")][..],
                SessionBreakType::Weekly,
            ),
            (
                Timeframe::Week1,
                &[(2024, 1, 22), (2024, 1, 29), (2024, 2, 5)][..],
                &[(2, "2024-02-05")][..],
                SessionBreakType::Monthly,
            ),
            (
                "2d".parse().unwrap(),
                &[(2024, 1, 30), (2024, 2, 1), (2024, 2, 3)][..],
                &[(1, "2024-02-01")][..],
                SessionBreakType::Monthly,
            ),
            (
                Timeframe::Month1,
                &[(2024, 1, 1), (2024, 2, 1), (2024, 12, 1), (2025, 1, 1)][..],
                &[(3, "2025-01-01")][..],
                SessionBreakType::Yearly,
            ),
            (
                "3month".parse().unwrap(),
                &[
                    (2024, 1, 1),
                    (2024, 4, 1),
                    (2024, 7, 1),
                    (2024, 10, 1),
                    (2025, 1, 1),
                ][..],
                &[(4, "2025-01-01")][..],
                SessionBreakType::Yearly,
            ),
            (
                "24h".parse().unwrap(),
                &[(2024, 3, 14), (2024, 3, 18)][..],
                &[(1, "2024-03-18")][..],
                SessionBreakType::Weekly,
            ),
        ] {
            for (display, exchange) in [
                ("UTC", "UTC"),
                ("America/New_York", "America/New_York"),
                ("Pacific/Chatham", "Pacific/Chatham"),
                ("Europe/London", "Europe/Berlin"),
                ("Europe/Berlin", "Asia/Tokyo"),
                ("Asia/Kolkata", "Asia/Tokyo"),
                ("Asia/Tokyo", "Australia/Sydney"),
                ("Asia/Tehran", "Asia/Tehran"),
                ("UTC", "Asia/Amman"),
                ("Asia/Amman", "Asia/Amman"),
            ] {
                // Bars stamped at UTC, display or exchange midnight.
                let zone = TimezoneId::named(display).unwrap();
                let exchange_zone = TimezoneId::named(exchange).unwrap();
                let instrument = crate::model::Instrument::new("CALENDAR")
                    .with_time_zone(exchange_zone.clone())
                    .with_session(Session::new([]));
                let clock = crate::model::BarClock::new(timeframe)
                    .with_timezone(zone.clone())
                    .with_session(
                        instrument.time_zone.clone().unwrap(),
                        instrument.session.clone().unwrap(),
                        false,
                    );
                let providers = [
                    clock.sessions(),
                    #[cfg(feature = "widget")]
                    crate::chart::renderers::exchange_session(
                        Some(&instrument),
                        None,
                        &crate::scales::TimezoneMode::Timezone(zone),
                        timeframe,
                    ),
                ];
                for provider in providers {
                    let dates: Vec<_> = dates
                        .iter()
                        .map(|&(year, month, day)| {
                            NaiveDate::from_ymd_opt(year, month, day).unwrap()
                        })
                        .collect();
                    for stamp in ["utc", "display", "exchange"] {
                        let bars: Vec<_> = dates
                            .iter()
                            .map(|&date| {
                                let midnight = date.and_time(NaiveTime::MIN);
                                let open = match stamp {
                                    "utc" => midnight.and_utc(),
                                    "display" => clock.open_on_date(date).unwrap(),
                                    // Include the second midnight when the exchange repeats it.
                                    _ => exchange_zone
                                        .from_local_datetime(&midnight)
                                        .latest()
                                        .map(|time| time.to_utc())
                                        .or_else(|| local_instant(exchange_zone.clone(), midnight))
                                        .unwrap(),
                                };
                                Bar::new(open, 1.0, 1.0, 1.0, 1.0, 1.0)
                            })
                            .collect();
                        let breaks = find_session_breaks(&bars, &provider);
                        assert!(breaks.iter().all(|(_, item)| item.break_type == break_type));
                        assert_eq!(
                            breaks
                                .iter()
                                .map(|(i, item)| (*i, item.label.as_deref().unwrap()))
                                .collect::<Vec<_>>(),
                            expected,
                            "{timeframe} {display}/{exchange} {stamp}"
                        );
                        for (pair, date) in bars.windows(2).zip(&dates[1..]) {
                            assert_eq!(
                                provider.trading_date(pair[1].time),
                                *date,
                                "{timeframe} {display}/{exchange} {stamp} {}",
                                pair[1].time
                            );
                            let item = provider.session_break_between(&pair[0].time, &pair[1].time);
                            let batch = breaks.iter().find(|(_, item)| item.ts == pair[1].time);
                            assert_eq!(
                                item.as_ref().map(|item| (&item.label, item.break_type)),
                                batch.map(|(_, item)| (&item.label, item.break_type))
                            );
                        }
                    }
                }
            }
        }
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
                "2024-03-11T00:00:00Z",
                "24h".parse().unwrap(),
                Some("2024-03-11T20:00:00Z"),
            ),
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

    #[test]
    fn date_specific_hours_replace_weekly_hours_and_holidays_across_dst() {
        use crate::model::{BarClock, OverrideWindow, SessionOverride, SessionWindow};
        use Weekday::*;
        let clock = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let date = |text: &str| text.parse::<NaiveDate>().unwrap();
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        // Mon-Fri 09:30-16:00 New York time. Fri 1 Nov closes early at 13:00, with post-market
        // hours until 17:00. Sun 3 Nov, when clocks go back, holds an exceptional session
        // 10:00-12:00. Tue 5 Nov is a listed holiday with exceptional hours that win, and Wed 6
        // Nov is closed by an override without windows.
        let session = Session::new([SessionWindow::new(
            [Mon, Tue, Wed, Thu, Fri],
            clock(9, 30),
            clock(16, 0),
        )])
        .with_holidays([date("2024-11-05")])
        .with_overrides([
            SessionOverride::new(date("2024-11-01"))
                .with_windows([OverrideWindow::new(clock(9, 30), clock(13, 0))])
                .with_extended_windows([OverrideWindow::new(clock(13, 0), clock(17, 0))]),
            SessionOverride::new(date("2024-11-03"))
                .with_windows([OverrideWindow::new(clock(10, 0), clock(12, 0))]),
            SessionOverride::new(date("2024-11-05"))
                .with_windows([OverrideWindow::new(clock(9, 30), clock(11, 0))]),
            SessionOverride::new(date("2024-11-06")),
        ]);
        let provider = ExchangeSessionProvider::new(
            TimezoneId::named("America/New_York").unwrap(),
            Some(session),
        );
        // (instant, open regular, open with extended, extended shading, close with extended)
        for (time, regular, extended, shaded, close) in [
            // Fri 1 Nov, UTC-4: 12:59 is open, 13:00 closes the regular window.
            (
                "2024-11-01T16:59:00Z",
                true,
                true,
                false,
                "2024-11-01T17:00:00Z",
            ),
            (
                "2024-11-01T17:00:00Z",
                false,
                true,
                true,
                "2024-11-01T21:00:00Z",
            ),
            ("2024-11-01T21:00:00Z", false, false, false, ""),
            // Sun 3 Nov, UTC-5 from 06:00Z: 10:00 is 15:00Z and 12:00 is 17:00Z.
            ("2024-11-03T14:59:00Z", false, false, false, ""),
            (
                "2024-11-03T15:00:00Z",
                true,
                true,
                false,
                "2024-11-03T17:00:00Z",
            ),
            ("2024-11-03T17:00:00Z", false, false, false, ""),
            // Mon 4 Nov follows the weekly hours, now in UTC-5.
            ("2024-11-04T21:00:00Z", false, false, false, ""),
            (
                "2024-11-04T20:59:00Z",
                true,
                true,
                false,
                "2024-11-04T21:00:00Z",
            ),
            // Tue 5 Nov is a holiday, yet its override opens 09:30-11:00.
            (
                "2024-11-05T14:30:00Z",
                true,
                true,
                false,
                "2024-11-05T16:00:00Z",
            ),
            ("2024-11-05T16:00:00Z", false, false, false, ""),
            // Wed 6 Nov is closed although the weekly hours are open.
            ("2024-11-06T15:00:00Z", false, false, false, ""),
            (
                "2024-11-07T15:00:00Z",
                true,
                true,
                false,
                "2024-11-07T21:00:00Z",
            ),
        ] {
            let time = at(time);
            assert_eq!(provider.is_open(time, false), regular, "{time}");
            assert_eq!(provider.is_open(time, true), extended, "{time}");
            assert_eq!(provider.is_extended(time), shaded, "{time}");
            assert_eq!(
                provider.session_close(time, true),
                (!close.is_empty()).then(|| at(close)),
                "{time}"
            );
        }
        // History boundaries: each exceptional day starts a trading day of its own at its open,
        // and a closed day has none, so its hours stay with the day before.
        for (time, trading_date) in [
            ("2024-11-03T15:30:00Z", "2024-11-03"),
            ("2024-11-04T15:00:00Z", "2024-11-04"),
            ("2024-11-05T15:00:00Z", "2024-11-05"),
            ("2024-11-06T15:00:00Z", "2024-11-05"),
            ("2024-11-07T15:00:00Z", "2024-11-07"),
        ] {
            assert_eq!(
                provider.trading_date(at(time)),
                date(trading_date),
                "{time}"
            );
        }
        // Daily bars take the close of their own trading date.
        for (open, close) in [
            ("2024-11-01T00:00:00Z", "2024-11-01T21:00:00Z"),
            ("2024-11-03T00:00:00Z", "2024-11-03T17:00:00Z"),
        ] {
            assert_eq!(
                provider.bar_session_close(at(open), Timeframe::Day1, true),
                Some(at(close))
            );
        }
        // Bar clock: hourly bars of the exceptional Sunday start at its open.
        let bars = BarClock::new(Timeframe::Hour1).with_session(
            TimezoneId::named("America/New_York").unwrap(),
            Session::new([SessionWindow::new([Mon], clock(9, 30), clock(16, 0))])
                .with_overrides([SessionOverride::new(date("2024-11-03"))
                    .with_windows([OverrideWindow::new(clock(10, 0), clock(12, 0))])]),
            false,
        );
        assert_eq!(
            bars.bar_open(at("2024-11-03T16:30:00Z")),
            at("2024-11-03T16:00:00Z")
        );
        assert_eq!(
            bars.bar_open(at("2024-11-03T15:30:00Z")),
            at("2024-11-03T15:00:00Z")
        );
    }
}
