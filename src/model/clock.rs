//! Bar boundaries in time.

use super::{Timeframe, TimezoneId};
use chrono::{
    DateTime, Datelike, LocalResult, NaiveDate, NaiveTime, Offset, TimeDelta, TimeZone, Utc,
};

const DAY_MS: i64 = 86_400_000;
/// 1970-01-05, the first Monday after the Unix epoch.
const FIRST_MONDAY_MS: i64 = 4 * DAY_MS;

/// Bar boundaries for one timeframe, around the clock in UTC by default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarClock {
    /// `None` for a clock that matches bars by their exact open time.
    timeframe: Option<Timeframe>,
    timezone: Option<TimezoneId>,
    session: Option<super::ExchangeSessionProvider>,
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
    /// from intraday history. Extended windows participate only when requested.
    pub fn with_session(
        mut self,
        zone: super::TimezoneId,
        mut session: super::Session,
        extended_hours: bool,
    ) -> Self {
        if !extended_hours {
            session.clear_extended_windows();
        }
        self.session = Some(super::ExchangeSessionProvider::new(zone, Some(session)));
        self
    }

    /// Clock in which every instant opens a bar of its own, for bars of an unknown timeframe.
    #[cfg_attr(
        not(feature = "widget"),
        expect(dead_code, reason = "only the chart widget edits bars by hand")
    )]
    pub(crate) fn exact() -> Self {
        Self {
            timeframe: None,
            timezone: Some(TimezoneId::utc()),
            session: None,
        }
    }

    /// Open time of the bar containing `t`. Bars shorter than a day align to local midnight,
    /// weekly bars to Monday, monthly bars to the first day of the month, and other bars to the
    /// Unix epoch. Trade-based bars retain their exact opening timestamp.
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

    /// The timeframe of the bars, `None` for a clock that matches bars by their exact open time.
    pub(crate) fn timeframe(&self) -> Option<Timeframe> {
        self.timeframe
    }

    /// The trading days of this clock's bars: its exchange sessions, or calendar days in its
    /// time zone without them.
    pub(crate) fn sessions(&self) -> super::ExchangeSessionProvider {
        let sessions = match (&self.session, &self.timezone) {
            (Some(session), _) => session.clone(),
            (None, Some(zone)) => super::ExchangeSessionProvider::new(zone.clone(), None),
            (None, None) => super::ExchangeSessionProvider::local(None),
        };
        sessions.with_timeframe(self.timeframe.unwrap_or(Timeframe::Min1))
    }

    /// Whether `a` and `b` fall in the same bar, so a feed may stamp a bar with any instant
    /// inside it.
    pub fn same_bar(&self, a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
        self.bar_open(a) == self.bar_open(b)
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
    }
}
