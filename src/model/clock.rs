//! Bar boundaries in time.

use super::Timeframe;
use chrono::{DateTime, Datelike, LocalResult, NaiveTime, Offset, TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;

const DAY_MS: i64 = 86_400_000;
/// 1970-01-05, the first Monday after the Unix epoch.
const FIRST_MONDAY_MS: i64 = 4 * DAY_MS;

/// Bar boundaries for one timeframe, around the clock in UTC by default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarClock {
    /// `None` for a clock that matches bars by their exact open time.
    timeframe: Option<Timeframe>,
    timezone: Option<Tz>,
    session: Option<super::ExchangeSessionProvider>,
}

impl BarClock {
    /// Clock for bars of `timeframe`.
    pub fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe: Some(timeframe),
            timezone: Some(chrono_tz::UTC),
            session: None,
        }
    }

    /// Align time windows to the local calendar of `timezone`. Repeated clock hours remain
    /// separate; a boundary skipped by a clock change starts at the first valid local instant.
    pub fn with_timezone(mut self, timezone: Tz) -> Self {
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
            session.extended_windows.clear();
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
            timezone: Some(chrono_tz::UTC),
            session: None,
        }
    }

    /// Open time of the bar containing `t`. Bars shorter than a day align to local midnight,
    /// weekly bars to Monday, monthly bars to the first day of the month, and other bars to the
    /// Unix epoch. Count bars retain their exact opening timestamp.
    pub fn bar_open(&self, t: DateTime<Utc>) -> DateTime<Utc> {
        let Some(timeframe) = self.timeframe else {
            return t;
        };
        if matches!(timeframe, Timeframe::Ticks(_)) {
            return t;
        }
        if timeframe.duration_ms() < DAY_MS
            && let Some(session) = &self.session
        {
            let opens = session.opens([t]);
            let n = opens.partition_point(|(open, _)| *open <= t);
            if let Some((open, _)) = n.checked_sub(1).and_then(|i| opens.get(i)) {
                let elapsed = (t - *open).num_milliseconds();
                return open
                    .checked_add_signed(TimeDelta::milliseconds(
                        elapsed - elapsed.rem_euclid(timeframe.duration_ms()),
                    ))
                    .unwrap_or(t);
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
            Timeframe::Week1 => (7 * DAY_MS, FIRST_MONDAY_MS),
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
        open
    }

    fn offset_at(&self, t: DateTime<Utc>) -> i32 {
        match self.timezone {
            Some(zone) => t.with_timezone(&zone).offset().fix().local_minus_utc(),
            None => t.with_timezone(&chrono::Local).offset().local_minus_utc(),
        }
    }

    fn local_boundary(&self, local: chrono::NaiveDateTime, at: DateTime<Utc>) -> DateTime<Utc> {
        let time = match self.timezone {
            Some(zone) => zone.from_local_datetime(&local).map(|time| time.to_utc()),
            None => chrono::Local
                .from_local_datetime(&local)
                .map(|time| time.to_utc()),
        };
        match time {
            LocalResult::Single(time) => time,
            LocalResult::Ambiguous(first, second) => {
                if second <= at {
                    second
                } else {
                    first
                }
            }
            LocalResult::None => match self.timezone {
                Some(zone) => super::session::local_instant(zone, local),
                None => super::session::local_instant(chrono::Local, local),
            }
            .unwrap_or(at),
        }
    }

    /// First boundary after an aligned open, including short days and calendar months.
    pub(crate) fn next_open(&self, open: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let timeframe = self.timeframe?;
        if matches!(timeframe, Timeframe::Ticks(_)) {
            return None;
        }
        if timeframe.duration_ms() < DAY_MS
            && let Some(session) = &self.session
        {
            let next = open.checked_add_signed(timeframe.duration())?;
            return Some(
                session
                    .session_close(open, true)
                    .map_or(next, |close| next.min(close)),
            );
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

    /// The timeframe of the bars, `None` for a clock that matches bars by their exact open time.
    pub(crate) fn timeframe(&self) -> Option<Timeframe> {
        self.timeframe
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
        ];
        for (timeframe, at, open) in cases {
            let clock = BarClock::new(timeframe);
            assert_eq!(clock.bar_open(t(at)), t(open), "{timeframe} at {at}");
        }
        for (timeframe, at, open, next) in [
            (
                "123s".parse().unwrap(),
                "2024-03-10T07:00:01Z",
                "2024-03-10T07:00:00Z",
                "2024-03-10T07:00:24Z",
            ),
            (
                "45min".parse().unwrap(),
                "2024-11-03T06:01:00Z",
                "2024-11-03T06:00:00Z",
                "2024-11-03T06:30:00Z",
            ),
            (
                "45min".parse().unwrap(),
                "2024-11-03T05:45:00Z",
                "2024-11-03T05:30:00Z",
                "2024-11-03T06:00:00Z",
            ),
            (
                "7min".parse().unwrap(),
                "2024-03-10T06:59:30Z",
                "2024-03-10T06:59:00Z",
                "2024-03-10T07:00:00Z",
            ),
            (
                Timeframe::Hour4,
                "2024-03-10T07:30:00Z",
                "2024-03-10T05:00:00Z",
                "2024-03-10T08:00:00Z",
            ),
            (
                Timeframe::Hour4,
                "2024-11-03T08:30:00Z",
                "2024-11-03T04:00:00Z",
                "2024-11-03T09:00:00Z",
            ),
            (
                Timeframe::Hour1,
                "2024-11-03T05:30:00Z",
                "2024-11-03T05:00:00Z",
                "2024-11-03T06:00:00Z",
            ),
            (
                Timeframe::Hour1,
                "2024-11-03T06:30:00Z",
                "2024-11-03T06:00:00Z",
                "2024-11-03T07:00:00Z",
            ),
            (
                Timeframe::Month1,
                "2024-03-15T12:00:00Z",
                "2024-03-01T05:00:00Z",
                "2024-04-01T04:00:00Z",
            ),
        ] {
            let clock = BarClock::new(timeframe).with_timezone(chrono_tz::America::New_York);
            assert_eq!(clock.bar_open(t(at)), t(open));
            assert_eq!(clock.next_open(t(open)), Some(t(next)));
        }
    }
}
