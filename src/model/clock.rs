//! Bar boundaries in time.

use super::Timeframe;
use chrono::{DateTime, Datelike, NaiveTime, Utc};

const DAY_MS: i64 = 86_400_000;
/// 1970-01-05, the first Monday after the Unix epoch.
const FIRST_MONDAY_MS: i64 = 4 * DAY_MS;

/// Bar boundaries for one timeframe, in UTC around the clock.
#[derive(Clone, Debug)]
pub struct BarClock {
    /// `None` for a clock that matches bars by their exact open time.
    timeframe: Option<Timeframe>,
}

impl BarClock {
    /// Clock for bars of `timeframe`.
    pub fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe: Some(timeframe),
        }
    }

    /// Clock in which every instant opens a bar of its own, for bars of an unknown timeframe.
    #[cfg_attr(
        not(feature = "widget"),
        expect(dead_code, reason = "only the chart widget edits bars by hand")
    )]
    pub(crate) fn exact() -> Self {
        Self { timeframe: None }
    }

    /// Open time of the bar containing `t`. Bars shorter than a day align to UTC midnight,
    /// weekly bars to Monday, monthly bars to the first day of the month, and other bars to the
    /// Unix epoch. Count bars retain their exact opening timestamp.
    pub fn bar_open(&self, t: DateTime<Utc>) -> DateTime<Utc> {
        let Some(timeframe) = self.timeframe else {
            return t;
        };
        let ms = t.timestamp_millis();
        let (period, anchor) = match timeframe {
            Timeframe::Ticks(_) => return t,
            Timeframe::Month1 => {
                return t
                    .date_naive()
                    .with_day(1)
                    .map_or(t, |first| first.and_time(NaiveTime::MIN).and_utc());
            }
            Timeframe::Week1 => (7 * DAY_MS, FIRST_MONDAY_MS),
            timeframe => {
                let period = timeframe.duration_ms().max(1);
                let midnight = ms - ms.rem_euclid(DAY_MS);
                (period, if period < DAY_MS { midnight } else { 0 })
            }
        };
        DateTime::from_timestamp_millis(ms - (ms - anchor).rem_euclid(period)).unwrap_or(t)
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
    }
}
