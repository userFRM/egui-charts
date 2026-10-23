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
