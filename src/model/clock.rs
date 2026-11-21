//! Bar boundaries in time.

use super::{Timeframe, TimezoneId};
use chrono::{
    DateTime, Datelike, LocalResult, NaiveDate, NaiveTime, Offset, TimeDelta, TimeZone, Utc,
};
use std::ops::Range;
use std::sync::Arc;

const DAY_MS: i64 = 86_400_000;
/// 1970-01-05, the first Monday after the Unix epoch.
const FIRST_MONDAY_MS: i64 = 4 * DAY_MS;

/// The trading dates a bar of `timeframe` holds on a clock with a session: custom intervals of
/// two or more whole days count them; daily, weekly and monthly bars keep calendar periods.
pub(super) fn days_per_bar(timeframe: Timeframe) -> Option<i64> {
    match timeframe {
        Timeframe::Custom(seconds)
            if seconds.get() % 86_400 == 0 && seconds.get() >= 2 * 86_400 =>
        {
            i64::try_from(seconds.get() / 86_400).ok()
        }
        _ => None,
    }
}

/// Bar boundaries for one timeframe, around the clock in UTC by default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarClock {
    /// `None` for a clock that matches bars by their exact open time.
    timeframe: Option<Timeframe>,
    timezone: Option<TimezoneId>,
    /// Shared by the clock's copies, so that copying and comparing them stays cheap.
    session: Option<Arc<super::ExchangeSessionProvider>>,
}

impl BarClock {
    /// Clock for bars of `timeframe`.
    pub fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe: Some(timeframe),
            timezone: Some(TimezoneId::utc()),
            session: None,
        }
    }

    /// Align time windows to the local calendar of `timezone`. Repeated clock hours remain
    /// separate; a boundary skipped by a clock change starts at the first valid local instant.
    pub fn with_timezone(mut self, timezone: TimezoneId) -> Self {
        self.timezone = Some(timezone);
        self
    }

    /// Align time windows to the system's local calendar.
    pub fn with_local_timezone(mut self) -> Self {
        self.timezone = None;
        self
    }

    /// Align intraday buckets to the first selected exchange-session window. Calendar bars
    /// keep their trading-date labels; use `data::resample_with_session` to derive those labels
    /// from intraday history. Live trades and heartbeats join the calendar bar of their trading
    /// date. Bars of two or more whole days, such as `3D`, hold that many trading dates of the
    /// selected windows, counted from 1970-01-01, and carry the date of the first. Extended and
    /// overnight windows participate only when requested.
    pub fn with_session(
        self,
        zone: super::TimezoneId,
        session: super::Session,
        extended_hours: bool,
    ) -> Self {
        self.with_session_hours(zone, session, extended_hours, extended_hours)
    }

    /// As [`Self::with_session`], with overnight windows only when both flags are set, as a
    /// [`HistoryRequest`](crate::data::HistoryRequest) selects them.
    pub fn with_session_hours(
        mut self,
        zone: super::TimezoneId,
        mut session: super::Session,
        extended_hours: bool,
        overnight_hours: bool,
    ) -> Self {
        session.select_hours(extended_hours, extended_hours && overnight_hours);
        let provider = super::ExchangeSessionProvider::new(zone, Some(session));
        self.session = Some(Arc::new(provider));
        self
    }

    /// This clock counting the trading dates of `session`'s selected hours in `zone` when its bars
    /// hold two or more whole days and it has no session of its own, as
    /// `data::resample_with_session` dates such bars.
    pub(crate) fn with_trading_dates_of(
        self,
        zone: &TimezoneId,
        session: &super::Session,
        extended_hours: bool,
        overnight_hours: bool,
    ) -> Self {
        if self.session.is_some() || self.timeframe.and_then(days_per_bar).is_none() {
            return self;
        }
        self.with_session_hours(
            zone.clone(),
            session.clone(),
            extended_hours,
            overnight_hours,
        )
    }

    /// Clock in which every instant opens a bar of its own, for bars of an unknown timeframe.
    pub(crate) fn exact() -> Self {
        Self {
            timeframe: None,
            timezone: Some(TimezoneId::utc()),
            session: None,
        }
    }

    /// Open time of the bar containing `t`. Bars shorter than a day align to local midnight,
    /// weekly bars to Monday, monthly bars to the first day of the month, and other bars to the
    /// Unix epoch. Trade-based bars retain their exact opening timestamp. On a clock with a
    /// session, bars of two or more whole days instead hold that many consecutive trading dates,
    /// counted from 1970-01-01, and open at the first one's local midnight; a day without trading
    /// belongs to the bar of the trading date before it.
    pub fn bar_open(&self, t: DateTime<Utc>) -> DateTime<Utc> {
        let Some(timeframe) = self.timeframe else {
            return t;
        };
        if timeframe.is_trade_based() {
            return t;
        }
        if timeframe.duration_ms() < DAY_MS
            && let Some(session) = &self.session
        {
            let opens = session.opens([t]);
            let n = opens.partition_point(|(open, _)| *open <= t);
            if let Some((open, _)) = n.checked_sub(1).and_then(|i| opens.get(i)) {
                let elapsed = (t - *open).num_milliseconds();
                let bucket = open
                    .checked_add_signed(TimeDelta::milliseconds(
                        elapsed - elapsed.rem_euclid(timeframe.duration_ms()),
                    ))
                    .unwrap_or(t);
                // Touching selected windows share a bucket; only a gap closes it.
                let mut end = session.session_close(bucket, true);
                while let Some(close) = end.filter(|close| *close <= t) {
                    end = session.session_close(close, true);
                    if end.is_none() {
                        return close;
                    }
                }
                return bucket;
            }
        }
        let offset = self.offset_at(t);
        let local = t
            .naive_utc()
            .checked_add_signed(TimeDelta::seconds(i64::from(offset)))
            .unwrap_or(t.naive_utc());
        if let Some(days) = days_per_bar(timeframe)
            && let Some(session) = &self.session
            && let Some(first) = session.trading_run(local.date(), days, 0)
        {
            // A date without trading stays in the bar of the trading date before it.
            return self.local_boundary(first.and_time(NaiveTime::MIN), t);
        }
        let ms = local.and_utc().timestamp_millis();
        let (period, anchor) = match timeframe {
            Timeframe::Ticks(_) => return t,
            Timeframe::Month1 => {
                let first = local.date().with_day(1).unwrap().and_time(NaiveTime::MIN);
                return self.local_boundary(first, t);
            }
            Timeframe::Months(count) => {
                let month = i64::from(local.year() - 1970) * 12 + i64::from(local.month0());
                let month = month - month.rem_euclid(i64::from(count.get()));
                let first = i32::try_from(1970 + month.div_euclid(12))
                    .ok()
                    .and_then(|year| {
                        NaiveDate::from_ymd_opt(year, month.rem_euclid(12) as u32 + 1, 1)
                    });
                return first.map_or(t, |date| {
                    self.local_boundary(date.and_time(NaiveTime::MIN), t)
                });
            }
            Timeframe::Week1 => (7 * DAY_MS, FIRST_MONDAY_MS),
            Timeframe::Weeks(count) => (i64::from(count.get()) * 7 * DAY_MS, FIRST_MONDAY_MS),
            timeframe => {
                let period = timeframe.duration_ms().max(1);
                let midnight = ms - ms.rem_euclid(DAY_MS);
                (period, if period < DAY_MS { midnight } else { 0 })
            }
        };
        let boundary = DateTime::from_timestamp_millis(ms - (ms - anchor).rem_euclid(period))
            .map_or(local, |time| time.naive_utc());
        let open = self.local_boundary(boundary, t);
        // A repeated hour can begin inside a short bucket whose nominal open is before it.
        if timeframe.duration() <= TimeDelta::hours(1) && t - open >= timeframe.duration() {
            let mut lo = t
                .checked_sub_signed(timeframe.duration())
                .unwrap_or(t)
                .timestamp_millis();
            let mut hi = t.timestamp_millis();
            if self.offset_at(DateTime::from_timestamp_millis(lo).unwrap_or(t)) != offset {
                while hi - lo > 1 {
                    let mid = lo + (hi - lo) / 2;
                    if self.offset_at(DateTime::from_timestamp_millis(mid).unwrap_or(t)) == offset {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return DateTime::from_timestamp_millis(hi).unwrap_or(open);
            }
        }
        if period > TimeDelta::hours(1).num_milliseconds()
            && period < DAY_MS
            && let Some(next) = boundary.checked_add_signed(timeframe.duration())
        {
            // The first pass of the next boundary may already have opened a bar.
            let next = self.local_boundary(next, open);
            if next > open && next <= t {
                return next;
            }
        }
        open
    }

    fn offset_at(&self, t: DateTime<Utc>) -> i32 {
        match &self.timezone {
            Some(zone) => t.with_timezone(zone).offset().fix().local_minus_utc(),
            None => t.with_timezone(&chrono::Local).offset().local_minus_utc(),
        }
    }

    fn local_boundary(&self, local: chrono::NaiveDateTime, at: DateTime<Utc>) -> DateTime<Utc> {
        let time = match &self.timezone {
            Some(zone) => zone.from_local_datetime(&local).map(|time| time.to_utc()),
            None => chrono::Local
                .from_local_datetime(&local)
                .map(|time| time.to_utc()),
        };
        match time {
            LocalResult::Single(time) => time,
            LocalResult::Ambiguous(first, second) => {
                // Calendar periods keep one open when midnight repeats.
                if second <= at && self.timeframe.is_some_and(|tf| tf.duration_ms() < DAY_MS) {
                    second
                } else {
                    first
                }
            }
            LocalResult::None => match &self.timezone {
                Some(zone) => super::local_instant(zone.clone(), local),
                None => super::local_instant(chrono::Local, local),
            }
            .unwrap_or(at),
        }
    }

    /// First boundary after an aligned open, including short days and calendar months.
    pub(crate) fn next_open(&self, open: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let timeframe = self.timeframe?;
        if timeframe.is_trade_based() {
            return None;
        }
        if timeframe.duration_ms() < DAY_MS
            && let Some(session) = &self.session
        {
            let opens = session.opens([open]);
            let n = opens.partition_point(|(start, _)| *start <= open);
            if let Some((start, _)) = n.checked_sub(1).and_then(|i| opens.get(i)) {
                let remaining = timeframe.duration_ms()
                    - (open - *start)
                        .num_milliseconds()
                        .rem_euclid(timeframe.duration_ms());
                let mut next = open.checked_add_signed(TimeDelta::milliseconds(remaining))?;
                if let Some((start, _)) = opens.get(n) {
                    next = next.min(*start);
                }
                // Touching selected windows keep the bucket open until the grid or a gap.
                let mut end = session.session_close(open, true);
                while let Some(close) = end.filter(|close| *close < next) {
                    end = session.session_close(close, true);
                    if end.is_none() {
                        next = close;
                    }
                }
                return Some(next);
            }
        }
        if let Some(days) = days_per_bar(timeframe)
            && let Some(session) = &self.session
        {
            let date = (open.naive_utc())
                .checked_add_signed(TimeDelta::seconds(i64::from(self.offset_at(open))))?
                .date();
            if session.trading_run(date, days, 0).is_some() {
                // Past the last trading date no bar follows; the hourly search below would not end.
                let first = session.trading_run(date, days, 1)?;
                return Some(self.local_boundary(first.and_time(NaiveTime::MIN), open));
            }
        }
        if let Timeframe::Months(count) = timeframe {
            let local = open
                .naive_utc()
                .checked_add_signed(TimeDelta::seconds(i64::from(self.offset_at(open))))?;
            let next = local
                .date()
                .checked_add_months(chrono::Months::new(count.get()))?
                .and_time(NaiveTime::MIN);
            return Some(self.local_boundary(next, open));
        }
        let mut at = open.checked_add_signed(timeframe.duration())?;
        loop {
            let next = self.bar_open(at);
            if next > open {
                if self.bar_open(next - TimeDelta::milliseconds(1)) == open {
                    return Some(next);
                }
                let mut lo = open.timestamp_millis();
                let mut hi = at.timestamp_millis();
                while hi - lo > 1 {
                    let mid = lo + (hi - lo) / 2;
                    let time = DateTime::from_timestamp_millis(mid)?;
                    if self.bar_open(time) > open {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return Some(self.bar_open(DateTime::from_timestamp_millis(hi)?));
            }
            at = at.checked_add_signed(TimeDelta::hours(1))?;
        }
    }

    #[cfg(feature = "studies")]
    pub(crate) fn for_timeframe(&self, timeframe: Timeframe) -> Self {
        Self {
            timeframe: Some(timeframe),
            ..self.clone()
        }
    }

    /// Label a trading date in this clock's calendar before finding its containing period.
    pub(crate) fn open_on_date(&self, date: chrono::NaiveDate) -> Option<DateTime<Utc>> {
        let midnight = date.and_time(NaiveTime::MIN);
        let time = match &self.timezone {
            Some(zone) => super::local_instant(zone.clone(), midnight),
            None => super::local_instant(chrono::Local, midnight),
        }?;
        Some(self.bar_open(time))
    }

    /// The session of a clock whose bars are trading dates: a daily or longer clock with one.
    fn date_session(&self) -> Option<&super::ExchangeSessionProvider> {
        (self.session.as_deref())
            .filter(|_| self.timeframe.is_some_and(|tf| tf.duration_ms() >= DAY_MS))
    }

    /// Open time of the bar that a trade or another update at `t` joins: on a daily or longer
    /// clock with a session the bar of its trading date, as `data::resample_with_session` dates
    /// intraday history, so an evening session opens the next date's bar and a post-market hour
    /// after midnight stays in its date's bar; elsewhere [`bar_open`](Self::bar_open). Bar stamps
    /// lie inside their calendar period, so `bar_open` places them.
    pub(crate) fn instant_open(&self, t: DateTime<Utc>) -> DateTime<Utc> {
        (self.date_session())
            .and_then(|session| self.open_on_date(session.trading_date(t)))
            .unwrap_or_else(|| self.bar_open(t))
    }

    /// The instants around `t` that share its trading date on a daily or longer clock with a
    /// session, and the open of their bar: from the date's open to the next date's, at most to
    /// the end of `t`'s UTC day, through which the session lists every open. `None` elsewhere
    /// and before the first open listed.
    fn trading_span(&self, t: DateTime<Utc>) -> Option<(Range<DateTime<Utc>>, DateTime<Utc>)> {
        let opens = self.date_session()?.opens([t]);
        let n = opens.partition_point(|(open, _)| *open <= t);
        let (from, date) = *opens.get(n.checked_sub(1)?)?;
        let day_end = DateTime::from_timestamp((t.timestamp().div_euclid(86_400) + 1) * 86_400, 0)?;
        let until = opens.get(n).map_or(day_end, |(next, _)| day_end.min(*next));
        Some((from..until, self.open_on_date(date)?))
    }

    /// First instant of the bar that `t` joins by [`instant_open`](Self::instant_open). On a
    /// daily or longer clock with a session it is the open of the bar's first trading date, which
    /// can come before the bar's own time.
    pub(crate) fn instant_start(&self, t: DateTime<Utc>) -> DateTime<Utc> {
        let open = self.instant_open(t);
        let Some(session) = self.date_session() else {
            return open;
        };
        let (mut start, mut at) = (open.min(t), t);
        // A bar of several days, a week or a month holds several trading dates.
        while let Some(date_open) = session.session_open(at, false)
            && self.instant_open(date_open) == open
        {
            start = date_open;
            let Some(before) = date_open.checked_sub_signed(TimeDelta::nanoseconds(1)) else {
                break;
            };
            at = before;
        }
        start
    }

    /// First instant that joins the bar opening at `open` or a later one. On a daily or longer
    /// clock with a session it is the open of the bar's first trading date, which can come before
    /// `open`, as an evening session's does, or after it, as on UTC days for a New York session,
    /// whose midnight still belongs to the post-market of the date before; for a bar without
    /// trading, such as a weekend's, it is that of the next bar with trading. Elsewhere, and when
    /// no window opens within a year, `open`.
    pub(crate) fn bar_start(&self, open: DateTime<Utc>) -> DateTime<Utc> {
        let Some(session) = self.date_session() else {
            return open;
        };
        if self.instant_open(open) == open {
            return self.instant_start(open);
        }
        // The bar's time lies in an earlier date: step from window to window into the bar.
        let mut at = self.open_at_or_after(open);
        while let Some(t) = at
            && self.instant_open(t) < open
        {
            at = session
                .session_close(t, true)
                .and_then(|close| self.open_at_or_after(close));
        }
        at.map_or(open, |t| self.instant_start(t))
    }

    /// Open of the first bar after the one opening at `open` that a trade inside this clock's
    /// windows can join: that of the first window instant from [`next_open`](Self::next_open)
    /// on, and on a daily or longer clock with a session the bar of the next trading date.
    pub(crate) fn next_trading_open(&self, open: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let Some(session) = self.date_session() else {
            return Some(self.bar_open(self.open_at_or_after(self.next_open(open)?)?));
        };
        // A date's windows can close before its bar's time, as a Tokyo session's do on Los
        // Angeles days, so the walk starts in the bar rather than at the next calendar boundary.
        let mut at = self.open_at_or_after(self.instant_start(open))?;
        while self.instant_open(at) <= open {
            at = self.open_at_or_after(session.session_close(at, true)?)?;
        }
        Some(self.instant_open(at))
    }

    /// The zone of a clock with no exchange-session rules. `Some(None)` is local time.
    #[cfg(feature = "widget")]
    pub(crate) fn sessionless_zone(&self) -> Option<Option<&TimezoneId>> {
        self.session.is_none().then_some(self.timezone.as_ref())
    }

    /// The timeframe of the bars, `None` for a clock that matches bars by their exact open time.
    pub(crate) fn timeframe(&self) -> Option<Timeframe> {
        self.timeframe
    }

    /// The trading days of this clock's bars: its exchange sessions, or calendar days in its
    /// time zone without them.
    pub(crate) fn sessions(&self) -> super::ExchangeSessionProvider {
        let sessions = match (&self.session, &self.timezone) {
            (Some(session), _) => super::ExchangeSessionProvider::clone(session),
            (None, Some(zone)) => super::ExchangeSessionProvider::new(zone.clone(), None),
            (None, None) => super::ExchangeSessionProvider::local(None),
        };
        sessions
            .with_calendar_zone(self.timezone.clone())
            .with_timeframe(self.timeframe.unwrap_or(Timeframe::Min1))
    }

    /// The trading hours this clock keeps, as [`with_session_hours`](Self::with_session_hours)
    /// selected them.
    #[cfg(feature = "studies")]
    pub(crate) fn session(&self) -> Option<&super::Session> {
        self.session.as_deref()?.session()
    }

    /// Whether this clock restricts bars to exchange trading windows.
    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// Whether `t` is inside a window of this clock's session, always without a session.
    /// [`with_session`](Self::with_session) keeps only the windows the subscription selected.
    pub(crate) fn is_open(&self, t: DateTime<Utc>) -> bool {
        self.session.as_ref().is_none_or(|s| s.is_open(t, true))
    }

    /// The first instant at or after `t` inside a window of this clock's session: `t` itself
    /// without a session, `None` when no window opens within a year.
    pub(crate) fn open_at_or_after(&self, t: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match &self.session {
            Some(session) => session.open_at_or_after(t),
            None => Some(t),
        }
    }

    /// Whether `a` and `b` fall in the same bar, so a feed may stamp a bar with any instant
    /// inside it.
    pub fn same_bar(&self, a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
        self.bar_open(a) == self.bar_open(b)
    }
}

/// The bar that [`BarClock::instant_open`] gave a recent update, with its clock and the instants
/// that share the update's trading date, so that a stream of trades, quotes and heartbeats finds
/// the session once per date rather than once per update.
#[derive(Clone, Debug, Default)]
pub(crate) struct Placement(Option<(BarClock, Range<DateTime<Utc>>, DateTime<Utc>)>);

impl Placement {
    /// Whether there is no time-based placement to account for in a trade-bar checkpoint.
    #[cfg(feature = "widget")]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// [`BarClock::instant_open`] of `t` on `clock`.
    pub(crate) fn open(&mut self, clock: &BarClock, t: DateTime<Utc>) -> DateTime<Utc> {
        if clock.date_session().is_none() {
            return clock.bar_open(t);
        }
        if let Some((placed, span, open)) = &self.0
            && span.contains(&t)
            && placed == clock
        {
            return *open;
        }
        let Some((span, open)) = clock.trading_span(t) else {
            return clock.instant_open(t);
        };
        self.0 = Some((clock.clone(), span, open));
        open
    }
}

impl From<Timeframe> for BarClock {
    fn from(timeframe: Timeframe) -> Self {
        Self::new(timeframe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(rfc3339: &str) -> DateTime<Utc> {
        rfc3339.parse().unwrap()
    }

    /// Calendar timeframes must not fall back to fixed durations from the epoch: that gives
    /// Thursday weeks, 30-day months and intraday bars that ignore midnight.
    #[test]
    fn bars_open_at_calendar_boundaries() {
        #[rustfmt::skip]
        let cases = [
            (Timeframe::Min1, "2024-03-15T10:07:42.5Z", "2024-03-15T10:07:00Z"),
            (Timeframe::Hour4, "2024-03-15T23:30:00Z", "2024-03-15T20:00:00Z"),
            // Seven-minute bars restart at midnight, so the last one of the day is short.
            ("7min".parse().unwrap(), "2024-03-15T23:58:00Z", "2024-03-15T23:55:00Z"),
            (Timeframe::Day1, "2024-03-15T23:59:59Z", "2024-03-15T00:00:00Z"),
            (Timeframe::Day1, "1969-07-20T20:17:40Z", "1969-07-20T00:00:00Z"),
            // A Sunday belongs to the week that opened on the Monday before.
            (Timeframe::Week1, "2024-03-17T12:00:00Z", "2024-03-11T00:00:00Z"),
            (Timeframe::Month1, "2024-02-29T13:00:00Z", "2024-02-01T00:00:00Z"),
            ("3month".parse().unwrap(), "2024-04-15T12:00:00Z", "2024-04-01T00:00:00Z"),
            ("3month".parse().unwrap(), "2024-02-29T12:00:00Z", "2024-01-01T00:00:00Z"),
            ("3month".parse().unwrap(), "2024-07-15T12:00:00Z", "2024-07-01T00:00:00Z"),
            ("3month".parse().unwrap(), "2024-12-31T23:59:59Z", "2024-10-01T00:00:00Z"),
            ("6month".parse().unwrap(), "2024-12-31T23:59:59Z", "2024-07-01T00:00:00Z"),
            ("12month".parse().unwrap(), "2024-12-31T23:59:59Z", "2024-01-01T00:00:00Z"),
            ("3month".parse().unwrap(), "1969-12-31T23:59:59Z", "1969-10-01T00:00:00Z"),
            ("2w".parse().unwrap(), "1970-01-04T12:00:00Z", "1969-12-22T00:00:00Z"),
        ];
        for (timeframe, at, open) in cases {
            let clock = BarClock::new(timeframe);
            assert_eq!(clock.bar_open(t(at)), t(open), "{timeframe} at {at}");
        }
        for (timeframe, at, open, next, zone) in [
            (
                "90min".parse().unwrap(),
                "2024-11-03T06:10:00Z",
                "2024-11-03T05:30:00Z",
                "2024-11-03T06:30:00Z",
                "America/New_York",
            ),
            (
                "3month".parse().unwrap(),
                "2024-04-01T03:59:59Z",
                "2024-01-01T05:00:00Z",
                "2024-04-01T04:00:00Z",
                "America/New_York",
            ),
            (
                "3month".parse().unwrap(),
                "2024-04-15T12:00:00Z",
                "2024-04-01T04:00:00Z",
                "2024-07-01T04:00:00Z",
                "America/New_York",
            ),
            (
                "2w".parse().unwrap(),
                "2024-03-15T12:00:00Z",
                "2024-03-04T05:00:00Z",
                "2024-03-18T04:00:00Z",
                "America/New_York",
            ),
            (
                "123s".parse().unwrap(),
                "2024-03-10T07:00:01Z",
                "2024-03-10T07:00:00Z",
                "2024-03-10T07:00:24Z",
                "America/New_York",
            ),
            (
                "45min".parse().unwrap(),
                "2024-11-03T06:01:00Z",
                "2024-11-03T06:00:00Z",
                "2024-11-03T06:30:00Z",
                "America/New_York",
            ),
            (
                "45min".parse().unwrap(),
                "2024-11-03T05:45:00Z",
                "2024-11-03T05:30:00Z",
                "2024-11-03T06:00:00Z",
                "America/New_York",
            ),
            (
                "7min".parse().unwrap(),
                "2024-03-10T06:59:30Z",
                "2024-03-10T06:59:00Z",
                "2024-03-10T07:00:00Z",
                "America/New_York",
            ),
            (
                Timeframe::Hour4,
                "2024-03-10T07:30:00Z",
                "2024-03-10T05:00:00Z",
                "2024-03-10T08:00:00Z",
                "America/New_York",
            ),
            (
                Timeframe::Hour4,
                "2024-11-03T08:30:00Z",
                "2024-11-03T04:00:00Z",
                "2024-11-03T09:00:00Z",
                "America/New_York",
            ),
            (
                Timeframe::Hour1,
                "2024-11-03T05:30:00Z",
                "2024-11-03T05:00:00Z",
                "2024-11-03T06:00:00Z",
                "America/New_York",
            ),
            (
                Timeframe::Hour1,
                "2024-11-03T06:30:00Z",
                "2024-11-03T06:00:00Z",
                "2024-11-03T07:00:00Z",
                "America/New_York",
            ),
            (
                Timeframe::Month1,
                "2024-03-15T12:00:00Z",
                "2024-03-01T05:00:00Z",
                "2024-04-01T04:00:00Z",
                "America/New_York",
            ),
            (
                "2month".parse().unwrap(),
                "2020-11-01T05:30:00Z",
                "2020-11-01T04:00:00Z",
                "2021-01-01T05:00:00Z",
                "America/Havana",
            ),
        ] {
            let Some(zone) = TimezoneId::named(zone) else {
                continue;
            };
            let clock = BarClock::new(timeframe).with_timezone(zone);
            assert_eq!(clock.bar_open(t(at)), t(open));
            assert_eq!(clock.next_open(t(open)), Some(t(next)));
        }
        let clock = BarClock::new("7min".parse().unwrap()).with_session(
            super::super::TimezoneId::utc(),
            super::super::Session::new([]),
            false,
        );
        assert_eq!(
            clock.next_open(t("2024-03-15T23:55:00Z")),
            Some(t("2024-03-16T00:00:00Z"))
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn intraday_bars_partition_repeated_hours() {
        for (zone, transition, intervals) in [
            (
                "America/New_York",
                "2024-11-03T06:00:00Z",
                &[61, 75, 80, 90, 100][..],
            ),
            (
                "America/Chicago",
                "2024-11-03T07:00:00Z",
                &[61, 75, 80, 100][..],
            ),
            ("Europe/London", "2024-10-27T01:00:00Z", &[90][..]),
            ("America/St_Johns", "2024-11-03T04:30:00Z", &[90][..]),
            (
                "Europe/Berlin",
                "2024-10-27T01:00:00Z",
                &[61, 75, 80, 150][..],
            ),
            (
                "Australia/Sydney",
                "2024-04-06T16:00:00Z",
                &[61, 75, 80, 150][..],
            ),
            (
                "Europe/Chisinau",
                "2024-10-27T00:00:00Z",
                &[61, 75, 80, 150][..],
            ),
            (
                "Pacific/Chatham",
                "2024-04-06T14:00:00Z",
                &[61, 100, 180][..],
            ),
            (
                "America/Sao_Paulo",
                "2018-02-18T02:00:00Z",
                &[61, 75, 100][..],
            ),
        ] {
            for &minutes in intervals {
                let clock = BarClock::new(format!("{minutes}min").parse().unwrap())
                    .with_timezone(TimezoneId::named(zone).unwrap());
                let start = t(transition) - TimeDelta::hours(14);
                let mut previous = clock.bar_open(start);
                let mut trades = Vec::new();
                for step in 0..=240 {
                    let at = start + TimeDelta::minutes(step * 10);
                    let open = clock.bar_open(at);
                    let next = clock.next_open(open).unwrap();
                    assert!(
                        previous <= open && open <= at && at < next,
                        "{zone} {minutes}min {at}: {open}..{next}"
                    );
                    assert_eq!(clock.bar_open(open), open);
                    assert_eq!(clock.bar_open(next), next);
                    assert_eq!(clock.bar_open(next - TimeDelta::milliseconds(1)), open);
                    previous = open;
                    trades.push(crate::model::Tick::new(at, 100.0, 1.0, None));
                }
                assert!(crate::data::aggregate_trades(&trades, clock).is_ok());
            }
        }
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn closed_session_ticks_open_separate_bars() {
        use crate::model::{Session, SessionWindow};
        use chrono::Weekday::*;
        for (close_minute, split, after, expected) in [
            (0, false, "2024-03-12T20:10:00Z", "2024-03-12T20:00:00Z"),
            (15, false, "2024-03-12T20:20:00Z", "2024-03-12T20:15:00Z"),
            (15, true, "2024-03-12T20:20:00Z", "2024-03-12T20:15:00Z"),
        ] {
            let mut session = Session::new([SessionWindow::new(
                [Mon, Tue, Wed, Thu, Fri],
                NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
                NaiveTime::from_hms_opt(16, close_minute, 0).unwrap(),
            )]);
            if split {
                let boundary = NaiveTime::from_hms_opt(11, 15, 0).unwrap();
                session.windows[0].close = boundary;
                session.windows.push(SessionWindow::new(
                    [Mon, Tue, Wed, Thu, Fri],
                    boundary,
                    NaiveTime::from_hms_opt(16, close_minute, 0).unwrap(),
                ));
            }
            let clock = BarClock::new(Timeframe::Hour1).with_session(
                TimezoneId::named("America/New_York").unwrap(),
                session,
                false,
            );
            assert_eq!(clock.bar_open(t(after)), t(expected));
            let inbox = std::sync::Arc::new(crate::data::inbox::Inbox::default());
            let sink = inbox.subscribe("SAMPLE".into(), clock.clone(), 100, None);
            for time in ["2024-03-12T19:59:00Z", after] {
                sink.push_tick(crate::model::Tick::new(t(time), 100.0, 1.0, None));
            }
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            assert_eq!(
                updates.bars.keys().copied().collect::<Vec<_>>(),
                [t("2024-03-12T19:30:00Z"), t(expected)]
            );
            assert!(updates.bars.values().all(|(bar, _)| bar.volume == 1.0));
            // Include the opening, the shortened last bar and the following off-session bar.
            for step in 0..=600 {
                let at = t("2024-03-12T13:00:00Z") + TimeDelta::minutes(step);
                let open = clock.bar_open(at);
                let next = clock.next_open(open).unwrap();
                assert!(open <= at && at < next, "{at}: {open}..{next}");
                assert_eq!(clock.bar_open(next - TimeDelta::milliseconds(1)), open);
            }
        }
        // Tuesday's overnight window opens before Monday's late daytime window.
        let clock = BarClock::new(Timeframe::Hour4).with_session(
            TimezoneId::utc(),
            Session::new([
                SessionWindow::new(
                    [Mon],
                    NaiveTime::from_hms_opt(23, 0, 0).unwrap(),
                    NaiveTime::from_hms_opt(23, 30, 0).unwrap(),
                ),
                SessionWindow::new(
                    [Tue],
                    NaiveTime::from_hms_opt(1, 0, 0).unwrap(),
                    NaiveTime::MIN,
                ),
            ]),
            false,
        );
        for (at, open, next) in [
            (
                "2024-07-08T22:00:00Z",
                "2024-07-08T21:00:00Z",
                "2024-07-08T23:00:00Z",
            ),
            (
                "2024-07-08T23:15:00Z",
                "2024-07-08T23:00:00Z",
                "2024-07-09T00:00:00Z",
            ),
        ] {
            assert_eq!(clock.bar_open(t(at)), t(open));
            assert_eq!(clock.next_open(t(open)), Some(t(next)));
            assert_eq!(
                clock.bar_open(t(next) - TimeDelta::milliseconds(1)),
                t(open)
            );
        }
    }

    /// On a daily or longer clock with a session a trade joins the bar of its trading date, as
    /// `data::resample_with_session` dates history, not the calendar period it falls in.
    #[cfg(feature = "timezones")]
    #[test]
    fn trades_join_the_bar_of_their_trading_date() {
        use crate::model::{Session, SessionWindow, Tick};
        use chrono::Weekday::*;
        let hm = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let chicago = TimezoneId::named("America/Chicago").unwrap();
        let futures = |timeframe| {
            BarClock::new(timeframe)
                .with_timezone(chicago.clone())
                .with_session(
                    chicago.clone(),
                    Session::new([SessionWindow::new(weekdays, hm(17, 0), hm(16, 0))])
                        .with_holidays([NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()]),
                    false,
                )
        };
        // On UTC days, with the post-market hour after midnight in winter.
        let equities = BarClock::new(Timeframe::Day1).with_session(
            TimezoneId::named("America/New_York").unwrap(),
            Session::new([SessionWindow::new(weekdays, hm(9, 30), hm(16, 0))])
                .with_extended_windows([
                    SessionWindow::new(weekdays, hm(4, 0), hm(9, 30)),
                    SessionWindow::new(weekdays, hm(16, 0), hm(20, 0)),
                ]),
            true,
        );
        // A Tokyo session on Los Angeles days closes before its date begins there.
        let tokyo = BarClock::new(Timeframe::Day1)
            .with_timezone(TimezoneId::named("America/Los_Angeles").unwrap())
            .with_session(
                TimezoneId::named("Asia/Tokyo").unwrap(),
                Session::new([SessionWindow::new(weekdays, hm(9, 0), hm(15, 0))]),
                false,
            );
        let day = futures(Timeframe::Day1);
        let three_days = futures("3D".parse().unwrap());
        // The instant, its bar, the bar's first instant and the next bar with trading.
        #[rustfmt::skip]
        let cases = [
            // 18:30 in Chicago trades the next date, whose session opened at 17:00.
            (&day, "2024-01-09T00:30:00Z", "2024-01-09T06:00:00Z", "2024-01-08T23:00:00Z", "2024-01-10T06:00:00Z"),
            // Sunday's open trades Monday.
            (&day, "2024-01-07T23:00:00Z", "2024-01-08T06:00:00Z", "2024-01-07T23:00:00Z", "2024-01-09T06:00:00Z"),
            (&day, "2024-01-08T22:00:00Z", "2024-01-08T06:00:00Z", "2024-01-07T23:00:00Z", "2024-01-09T06:00:00Z"),
            // The weekend and the holiday on Monday 15 January hold no bar.
            (&day, "2024-01-12T21:00:00Z", "2024-01-12T06:00:00Z", "2024-01-11T23:00:00Z", "2024-01-16T06:00:00Z"),
            // Daylight saving starts on Sunday 10 March.
            (&day, "2024-03-10T22:00:00Z", "2024-03-11T05:00:00Z", "2024-03-10T22:00:00Z", "2024-03-12T05:00:00Z"),
            // The post-market hour after midnight UTC trades the previous date.
            (&equities, "2024-01-09T00:30:00Z", "2024-01-08T00:00:00Z", "2024-01-08T09:00:00Z", "2024-01-09T00:00:00Z"),
            (&equities, "2024-01-09T01:00:00Z", "2024-01-08T00:00:00Z", "2024-01-08T09:00:00Z", "2024-01-09T00:00:00Z"),
            (&equities, "2024-01-09T09:00:00Z", "2024-01-09T00:00:00Z", "2024-01-09T09:00:00Z", "2024-01-10T00:00:00Z"),
            // Friday's post-market runs into Saturday UTC.
            (&equities, "2024-01-06T00:30:00Z", "2024-01-05T00:00:00Z", "2024-01-05T09:00:00Z", "2024-01-08T00:00:00Z"),
            (&equities, "2024-06-11T00:00:00Z", "2024-06-10T00:00:00Z", "2024-06-10T08:00:00Z", "2024-06-11T00:00:00Z"),
            // The next bar is Friday's, whose session closes at 06:00 UTC, before the bar opens.
            (&tokyo, "2024-01-04T00:30:00Z", "2024-01-04T08:00:00Z", "2024-01-04T00:00:00Z", "2024-01-05T08:00:00Z"),
            (&futures(Timeframe::Week1), "2024-01-08T00:00:00Z", "2024-01-08T06:00:00Z", "2024-01-07T23:00:00Z", "2024-01-15T06:00:00Z"),
            (&futures(Timeframe::Week1), "2024-01-11T16:00:00Z", "2024-01-08T06:00:00Z", "2024-01-07T23:00:00Z", "2024-01-15T06:00:00Z"),
            (&futures(Timeframe::Month1), "2024-02-01T00:00:00Z", "2024-02-01T06:00:00Z", "2024-01-31T23:00:00Z", "2024-03-01T06:00:00Z"),
            // A three-day bar holds Friday 5, Monday 8 and Tuesday 9 January; Sunday's open trades Monday.
            (&three_days, "2024-01-07T23:00:00Z", "2024-01-05T06:00:00Z", "2024-01-04T23:00:00Z", "2024-01-10T06:00:00Z"),
        ];
        for (clock, at, open, start, next) in cases {
            assert_eq!(clock.instant_open(t(at)), t(open), "{at}");
            assert_eq!(clock.instant_start(t(at)), t(start), "{at}");
            assert_eq!(clock.bar_start(t(open)), t(start), "{at}");
            assert_eq!(clock.next_trading_open(t(open)), Some(t(next)), "{at}");
        }
        // A bar without trading starts where the next one with trading does.
        let saturday = day.bar_start(t("2024-01-13T06:00:00Z"));
        assert_eq!(saturday, t("2024-01-15T23:00:00Z"));
        // Through the feed: Sunday's heartbeat opens Monday's candle, an evening trade Tuesday's.
        let inbox = std::sync::Arc::new(crate::data::inbox::Inbox::default());
        let sink = inbox.subscribe("SAMPLE".into(), day, 100, None);
        sink.push_tick(Tick::new(t("2024-01-05T21:00:00Z"), 100.0, 1.0, None));
        sink.advance_time(t("2024-01-07T23:30:00Z"));
        for (time, size) in [("2024-01-08T21:00:00Z", 1.0), ("2024-01-09T00:30:00Z", 2.0)] {
            sink.push_tick(Tick::new(t(time), 101.0, size, None));
        }
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let bars: Vec<_> = (updates.bars.iter())
            .map(|(open, (bar, _))| (*open, bar.volume))
            .collect();
        assert_eq!(
            bars,
            [
                (t("2024-01-05T06:00:00Z"), 1.0),
                (t("2024-01-08T06:00:00Z"), 1.0),
                (t("2024-01-09T06:00:00Z"), 2.0),
            ]
        );
    }
}
