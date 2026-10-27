//! Calendar periods of exchange trading days, at whose start anchored studies restart.

use super::vwap::trading_day;
use crate::model::{Bar, Instrument, SessionWindow, TimezoneId};
use chrono::{Datelike, NaiveDate};

/// A calendar period of trading days.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnchorPeriod {
    /// Each trading day.
    #[default]
    Session,
    /// Each ISO week, Monday to Sunday.
    Week,
    /// Each calendar month.
    Month,
    /// Each calendar quarter: January to March, April to June, July to September, October to December.
    Quarter,
    /// Each calendar year.
    Year,
    /// Each decade, 2020 to 2029 for example.
    Decade,
    /// Each century, 2000 to 2099 for example.
    Century,
}

impl AnchorPeriod {
    /// Identifies the period that holds trading day `day`.
    pub(super) fn key(self, day: NaiveDate) -> (i32, u32) {
        match self {
            Self::Session => (day.year(), day.ordinal()),
            Self::Week => (day.iso_week().year(), day.iso_week().week()),
            Self::Month => (day.year(), day.month()),
            Self::Quarter => (day.year(), (day.month() - 1) / 3),
            Self::Year => (day.year(), 0),
            Self::Decade => (day.year().div_euclid(10), 0),
            Self::Century => (day.year().div_euclid(100), 0),
        }
    }
}

/// The exchange time zone and trading session supplied by the chart instrument; UTC
/// without session windows until one is supplied.
#[derive(Clone, Default, PartialEq)]
pub(super) struct ExchangeDays(Option<(TimezoneId, Vec<SessionWindow>)>);

impl ExchangeDays {
    /// Takes the instrument's time zone and session. Returns whether they changed.
    pub(super) fn set(&mut self, instrument: Option<&Instrument>) -> bool {
        let next = instrument.map(|instrument| {
            (
                instrument.time_zone.clone().unwrap_or_else(TimezoneId::utc),
                instrument
                    .session
                    .as_ref()
                    .map_or_else(Vec::new, |s| s.windows.clone()),
            )
        });
        let changed = self.0 != next;
        self.0 = next;
        changed
    }

    /// The trading day of each bar, counted as `VolumeWeightedAvgPrice` counts them: the UTC
    /// date when no two consecutive bars share one (bars of a day or longer), otherwise the
    /// exchange date, a session window that spans midnight opening the next trading day.
    pub(super) fn of(&self, data: &[Bar]) -> Vec<NaiveDate> {
        let daily = data
            .windows(2)
            .all(|pair| pair[0].time.date_naive() != pair[1].time.date_naive());
        data.iter().map(|bar| self.day(bar, daily)).collect()
    }

    /// Trading day with the caller's already established daily/intraday classification.
    pub(super) fn day(&self, bar: &Bar, daily: bool) -> NaiveDate {
        if daily {
            return bar.time.date_naive();
        }
        let (zone, windows) = self.0.as_ref().map_or(
            (crate::model::TimezoneId::utc(), &[][..]),
            |(zone, windows)| (zone.clone(), windows.as_slice()),
        );
        trading_day(bar.time, &zone, windows)
    }
}

#[cfg(all(test, feature = "timezones"))]
mod tests {
    use super::*;
    use crate::model::Session;
    use chrono::{NaiveTime, Weekday};

    /// The instrument's zone and session move intraday bars to the exchange trading day,
    /// while bars a day or longer keep their UTC date.
    #[test]
    fn trading_days_follow_the_instrument_session() {
        let hour = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
        let chicago = Instrument::new("CHICAGO")
            .with_time_zone(TimezoneId::named("America/Chicago").unwrap())
            .with_session(Session::new([SessionWindow::new(
                [Weekday::Mon, Weekday::Tue],
                hour(17),
                hour(16),
            )]));
        let new_york = Instrument::new("NEW YORK")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap());
        let bars =
            |times: [&str; 2]| times.map(|t| Bar::new(t.parse().unwrap(), 1.0, 1.0, 1.0, 1.0, 1.0));
        let date = |d: &str| d.parse::<NaiveDate>().unwrap();
        // 15:00 and 17:00 in Chicago on Monday 2 March 2026; daily bars at 00:00 UTC.
        let intraday = bars(["2026-03-02T21:00:00Z", "2026-03-02T23:00:00Z"]);
        let daily = bars(["2024-01-04T00:00:00Z", "2024-01-05T00:00:00Z"]);
        for (instrument, data, want) in [
            (None, &intraday, ["2026-03-02", "2026-03-02"]),
            (Some(&chicago), &intraday, ["2026-03-02", "2026-03-03"]),
            (Some(&new_york), &daily, ["2024-01-04", "2024-01-05"]),
        ] {
            let mut days = ExchangeDays::default();
            assert_eq!(days.set(instrument), instrument.is_some());
            assert_eq!(
                days.of(data),
                want.map(date),
                "{:?}",
                instrument.map(|i| &i.symbol)
            );
        }
    }
}
