//! Countdown labels from bar boundaries and a host clock.

use crate::model::Timeframe;
use chrono::{DateTime, Datelike, Days, Months, NaiveTime, TimeDelta, Utc};

/// Formats the time left in an open bar, using an exact host or session close when supplied.
/// Without an exact close, calendar months end at the next month's first day and intraday
/// bars end no later than UTC midnight, as in `BarClock`. Closed or future bars have no label.
pub fn bar_countdown(
    open: DateTime<Utc>,
    timeframe: Timeframe,
    now: DateTime<Utc>,
    close: Option<DateTime<Utc>>,
) -> Option<String> {
    let end = match timeframe {
        Timeframe::Month1 => open
            .date_naive()
            .with_day(1)?
            .checked_add_months(Months::new(1))?
            .and_time(NaiveTime::MIN)
            .and_utc(),
        _ => {
            let end = open.checked_add_signed(TimeDelta::milliseconds(timeframe.duration_ms()))?;
            if timeframe.duration_ms() < 86_400_000 {
                end.min(
                    open.date_naive()
                        .checked_add_days(Days::new(1))?
                        .and_time(NaiveTime::MIN)
                        .and_utc(),
                )
            } else {
                end
            }
        }
    };
    let end = close.unwrap_or(end);
    if now < open || now >= end {
        return None;
    }
    let remaining = end - now;
    let seconds = remaining.num_seconds() + i64::from(remaining.subsec_nanos() > 0);
    let clock = if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600 % 24,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    };
    Some(if seconds >= 86_400 {
        format!("{}d {clock}", seconds / 86_400)
    } else {
        clock
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_uses_calendar_boundaries_and_exact_closes() {
        for (open, timeframe, now, close, expected) in [
            (
                "2024-02-01T00:00:00Z",
                Timeframe::Month1,
                "2024-02-29T23:59:59Z",
                None,
                Some("00:01"),
            ),
            (
                "2024-02-29T23:55:00Z",
                "7min".parse().unwrap(),
                "2024-02-29T23:59:00Z",
                None,
                Some("01:00"),
            ),
            (
                "2024-03-01T00:00:00Z",
                Timeframe::Min1,
                "2024-03-01T00:00:59.999999999Z",
                None,
                Some("00:01"),
            ),
            (
                "2024-03-01T00:00:00Z",
                Timeframe::Min1,
                "2024-03-01T00:01:00Z",
                None,
                None,
            ),
            (
                "2024-03-01T00:00:00Z",
                Timeframe::Min1,
                "2024-02-29T23:59:59Z",
                None,
                None,
            ),
            (
                "2024-03-11T00:00:00Z",
                Timeframe::Day1,
                "2024-03-11T19:59:00Z",
                Some("2024-03-11T20:00:00Z"),
                Some("01:00"),
            ),
            (
                "2024-03-11T00:00:00Z",
                Timeframe::Week1,
                "2024-03-14T19:00:00Z",
                Some("2024-03-15T20:00:00Z"),
                Some("1d 01:00:00"),
            ),
        ] {
            assert_eq!(
                bar_countdown(
                    open.parse().unwrap(),
                    timeframe,
                    now.parse().unwrap(),
                    close.map(|value| value.parse().unwrap())
                )
                .as_deref(),
                expected,
                "{timeframe}: {now}"
            );
        }
    }
}
