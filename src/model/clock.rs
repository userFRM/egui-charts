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
    timezone: Tz,
}

impl BarClock {
    /// Clock for bars of `timeframe`.
    pub fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe: Some(timeframe),
            timezone: chrono_tz::UTC,
        }
    }

    /// Align time windows to the local calendar of `timezone`. Repeated clock hours remain
    /// separate; a boundary skipped by a clock change starts at the first valid local minute.
    pub fn with_timezone(mut self, timezone: Tz) -> Self {
        self.timezone = timezone;
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
            timezone: chrono_tz::UTC,
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
        let offset = t
            .with_timezone(&self.timezone)
            .offset()
            .fix()
            .local_minus_utc();
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
        self.local_boundary(boundary, t)
    }

    fn local_boundary(&self, mut local: chrono::NaiveDateTime, at: DateTime<Utc>) -> DateTime<Utc> {
        loop {
            match self.timezone.from_local_datetime(&local) {
                LocalResult::Single(time) => return time.with_timezone(&Utc),
                LocalResult::Ambiguous(first, second) => {
                    return if second <= at { second } else { first }.with_timezone(&Utc);
                }
                LocalResult::None => {
                    let Some(next) = local.checked_add_signed(TimeDelta::minutes(1)) else {
                        return at;
                    };
                    local = next;
                }
            }
        }
    }

    /// First boundary after an aligned open, including short days and calendar months.
    pub(crate) fn next_open(&self, open: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let timeframe = self.timeframe?;
        if matches!(timeframe, Timeframe::Ticks(_)) {
            return None;
        }
        let mut at = open.checked_add_signed(timeframe.duration())?;
        loop {
            let next = self.bar_open(at);
            if next > open {
                return Some(next);
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
