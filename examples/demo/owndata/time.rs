//! Reading the time of a row or record, and printing the bounds of a request.

use chrono::{
    DateTime, Datelike as _, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeDelta,
    TimeZone as _, Timelike as _, Utc,
};
use egui_charts::model::TimezoneId;
use serde_json::Value;

use super::profile::{
    ClockUnit, DateFormat, FieldPath, RangeFormat, TimeSpec, TimeUnit, at, number,
};

/// When a row or record happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stamp {
    /// An instant.
    At(DateTime<Utc>),
    /// A calendar date without a time of day: a bar dated on that day.
    On(NaiveDate),
}

/// The time of `record` as `spec` describes it. `zone` is the profile's time zone; it applies
/// to a date with a time of day and to ISO 8601 text without an offset. `None` when a field is
/// missing, is `null`, or does not read as the form `spec` names.
pub(crate) fn stamp(record: &Value, spec: &TimeSpec, zone: &TimezoneId) -> Option<Stamp> {
    match spec {
        TimeSpec::Instant { field, unit } => {
            let value = at(record, field)?;
            let per_second = match unit {
                TimeUnit::Seconds => 1,
                TimeUnit::Milliseconds => 1_000,
                TimeUnit::Microseconds => 1_000_000,
                TimeUnit::Nanoseconds => 1_000_000_000,
                TimeUnit::Iso8601 => return iso(value.as_str()?, zone),
            };
            unix(value, per_second).map(Stamp::At)
        }
        TimeSpec::Wall {
            date,
            format,
            clock,
        } => {
            let date = date_of(at(record, date)?, format)?;
            let Some(clock) = clock else {
                return Some(Stamp::On(date));
            };
            let time = time_of_day(at(record, &clock.field)?, &clock.unit)?;
            wall(date.and_time(time), zone).map(Stamp::At)
        }
    }
}

/// The instant `value` names in a Unix unit of which `per_second` make a second.
fn unix(value: &Value, per_second: i64) -> Option<DateTime<Utc>> {
    let whole = match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    };
    // An integer is read exactly: nanoseconds have 19 digits and a float holds 15 to 17.
    if let Some(whole) = whole {
        let nanos = whole.rem_euclid(per_second) * (1_000_000_000 / per_second);
        return DateTime::from_timestamp(whole.div_euclid(per_second), u32::try_from(nanos).ok()?);
    }
    let micros = number(value)? * 1e6 / per_second as f64;
    // The cast saturates, and a saturated value lies outside the range of `DateTime`.
    DateTime::from_timestamp_micros(micros.round() as i64)
}

/// The time ISO 8601 `text` names. `YYYY-MM-DD` alone is a date. Followed by `T` or a space and
/// a time of day it is a wall time in `zone`, or an instant when `Z` or an offset written
/// `+HH:MM` or `+HHMM` ends it.
fn iso(text: &str, zone: &TimezoneId) -> Option<Stamp> {
    let text = text.trim();
    // Every form is ASCII, so a byte position below is a character position.
    if !text.is_ascii() || text.len() < 10 {
        return None;
    }
    let (date, rest) = text.split_at(10);
    let date = dashed(date)?;
    if rest.is_empty() {
        return Some(Stamp::On(date));
    }
    let rest = rest.strip_prefix(['T', ' '])?;
    let (time, offset) = rest.split_at(rest.find(['Z', '+', '-']).unwrap_or(rest.len()));
    let local = date.and_time(clock_text(time)?);
    let east = match offset {
        "" => return wall(local, zone).map(Stamp::At),
        "Z" => 0,
        offset => {
            let (hours, minutes) = match offset.len() {
                6 if offset.as_bytes()[3] == b':' => (&offset[1..3], &offset[4..]),
                5 => (&offset[1..3], &offset[3..]),
                _ => return None,
            };
            let (hours, minutes) = (digits(hours, 2)?, digits(minutes, 2)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            let seconds = i64::from(hours * 3600 + minutes * 60);
            if offset.starts_with('-') {
                -seconds
            } else {
                seconds
            }
        }
    };
    let instant = local
        .and_utc()
        .checked_sub_signed(TimeDelta::seconds(east))?;
    Some(Stamp::At(instant))
}

/// The number `text` holds when it is `width` digits and nothing else.
fn digits(text: &str, width: usize) -> Option<u32> {
    if text.len() != width || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The calendar day `YYYY-MM-DD` names, when it is one.
fn dashed(text: &str) -> Option<NaiveDate> {
    let mut parts = text.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let year = i32::try_from(digits(year, 4)?).ok()?;
    NaiveDate::from_ymd_opt(year, digits(month, 2)?, digits(day, 2)?)
}

/// The calendar day the eight digits of `YYYYMMDD` name, when they name one.
fn compact(number: u64) -> Option<NaiveDate> {
    let number = u32::try_from(number).ok()?;
    if !(10_000_000..100_000_000).contains(&number) {
        return None;
    }
    let year = i32::try_from(number / 10_000).ok()?;
    NaiveDate::from_ymd_opt(year, number / 100 % 100, number % 100)
}

/// The calendar day a date field holds in `format`.
fn date_of(value: &Value, format: &DateFormat) -> Option<NaiveDate> {
    match (format, value) {
        (DateFormat::Compact, Value::Number(number)) => compact(number.as_u64()?),
        (DateFormat::Compact, Value::String(text)) => compact(digits(text.trim(), 8)?.into()),
        (DateFormat::Dashed, Value::String(text)) => dashed(text.trim()),
        _ => None,
    }
}

/// The reading of the wall clock a time-of-day field holds in `unit`.
fn time_of_day(value: &Value, unit: &ClockUnit) -> Option<NaiveTime> {
    let per_second = match unit {
        ClockUnit::Milliseconds => 1e3,
        ClockUnit::Seconds => 1.0,
        ClockUnit::Text => return clock_text(value.as_str()?.trim()),
    };
    let micros = (number(value)? * 1e6 / per_second).round();
    if !(0.0..86_400e6).contains(&micros) {
        return None;
    }
    let micros = micros as u64;
    let (seconds, nanos) = (micros / 1_000_000, micros % 1_000_000 * 1_000);
    NaiveTime::from_num_seconds_from_midnight_opt(
        u32::try_from(seconds).ok()?,
        u32::try_from(nanos).ok()?,
    )
}

/// The time of day `HH:MM`, `HH:MM:SS` or `HH:MM:SS.fff` names.
fn clock_text(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M"))
        .ok()
}

/// The instant of the wall time `local` in `zone`: the first of two when clocks go back, the
/// instant of the change when clocks go forward over it.
pub(crate) fn wall(local: NaiveDateTime, zone: &TimezoneId) -> Option<DateTime<Utc>> {
    let instant = |local: &NaiveDateTime| {
        let first = zone.from_local_datetime(local).earliest()?;
        Some(first.to_utc())
    };
    if let Some(instant) = instant(&local) {
        return Some(instant);
    }
    // The wall time does not occur. The first second after it that does is the change.
    let mut local = local.with_nanosecond(0)?;
    for _ in 0..=24 * 60 * 60 {
        local = local.checked_add_signed(TimeDelta::seconds(1))?;
        if let Some(instant) = instant(&local) {
            return Some(instant);
        }
    }
    None
}

/// The reading of the time field at `field` that fits `value` best: ISO 8601 for text that
/// reads as it, a date for eight digits that are a calendar day between 1900 and 2100, and
/// otherwise the Unix unit whose magnitude fits (below 1e11 seconds, below 1e14 milliseconds,
/// below 1e17 microseconds, else nanoseconds). The dialog offers it and the user can change it.
pub(crate) fn guess(field: FieldPath, value: &Value) -> TimeSpec {
    let text = value.as_str();
    let format = match text.and_then(|text| iso(text, &TimezoneId::utc())) {
        Some(Stamp::At(_)) => {
            let unit = TimeUnit::Iso8601;
            return TimeSpec::Instant { field, unit };
        }
        Some(Stamp::On(_)) => Some(DateFormat::Dashed),
        None => {
            let day = date_of(value, &DateFormat::Compact);
            let recent = day.is_some_and(|day| (1900..=2100).contains(&day.year()));
            recent.then_some(DateFormat::Compact)
        }
    };
    if let Some(format) = format {
        return TimeSpec::Wall {
            date: field,
            format,
            clock: None,
        };
    }
    let unit = match number(value).map_or(0.0, f64::abs) {
        size if size < 1e11 => TimeUnit::Seconds,
        size if size < 1e14 => TimeUnit::Milliseconds,
        size if size < 1e17 => TimeUnit::Microseconds,
        _ => TimeUnit::Nanoseconds,
    };
    TimeSpec::Instant { field, unit }
}

/// `instant` as the text of `{from}` or `{to}`. The two date forms print the calendar day of
/// `instant` in `zone`.
pub(crate) fn print(instant: DateTime<Utc>, format: &RangeFormat, zone: &TimezoneId) -> String {
    match format {
        RangeFormat::Seconds => instant.timestamp().to_string(),
        RangeFormat::Milliseconds => instant.timestamp_millis().to_string(),
        RangeFormat::Microseconds => instant.timestamp_micros().to_string(),
        RangeFormat::Nanoseconds => {
            let seconds = i128::from(instant.timestamp()) * 1_000_000_000;
            (seconds + i128::from(instant.timestamp_subsec_nanos())).to_string()
        }
        RangeFormat::Iso8601 => instant.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        RangeFormat::Date | RangeFormat::CompactDate => {
            print_day(instant.with_timezone(zone).date_naive(), format)
        }
    }
}

/// `day` as a date form of `{from}` and `{to}` prints it: `20261111` in the compact form and
/// `2026-11-11` in every other.
pub(crate) fn print_day(day: NaiveDate, format: &RangeFormat) -> String {
    let form = match format {
        RangeFormat::CompactDate => "%Y%m%d",
        _ => "%Y-%m-%d",
    };
    day.format(form).to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::profile::{Clock, Segment};
    use super::*;

    #[test]
    fn every_time_form_reads_as_the_table_says() {
        use {ClockUnit as C, DateFormat as D, TimeUnit as U};
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let instant = |unit| TimeSpec::Instant {
            field: key("t"),
            unit,
        };
        let wall = |format, unit| TimeSpec::Wall {
            date: key("d"),
            format,
            clock: Some(Clock {
                field: key("c"),
                unit,
            }),
        };
        let day = |format| TimeSpec::Wall {
            date: key("d"),
            format,
            clock: None,
        };
        let at = |text: &str| Some(Stamp::At(text.parse().unwrap()));
        let on = |day: u32| Some(Stamp::On(NaiveDate::from_ymd_opt(2026, 11, day).unwrap()));
        const NY: &str = "America/New_York";

        // One row per form of time, numbered: the mapping, the record, the profile zone and the
        // result. Every result was calculated by hand. 2026-11-11 is day 20,768 after
        // 1970-01-01, so 14:30 UTC on it is second 20,768 * 86,400 + 52,200 = 1,794,407,400.
        // New York is five hours behind UTC on that day and four on 10 July. Its clocks go
        // forward on 8 March 2026 at 07:00 UTC, over 02:30, and back on 1 November at 06:00 UTC,
        // so 01:30 occurs at 05:30 UTC and again at 06:30 UTC.
        #[rustfmt::skip]
        let rows = [
            (1, instant(U::Seconds), json!({"t": 1794407400}), "UTC", at("2026-11-11T14:30:00Z")),
            (2, instant(U::Seconds), json!({"t": "1794407400"}), "UTC", at("2026-11-11T14:30:00Z")),
            (3, instant(U::Seconds), json!({"t": 1794407400.5}), "UTC", at("2026-11-11T14:30:00.5Z")),
            (4, instant(U::Milliseconds), json!({"t": 1794407400123_i64}), "UTC", at("2026-11-11T14:30:00.123Z")),
            (5, instant(U::Microseconds), json!({"t": 1794407400123456_i64}), "UTC", at("2026-11-11T14:30:00.123456Z")),
            (6, instant(U::Nanoseconds), json!({"t": 1794407400123456789_i64}), "UTC", at("2026-11-11T14:30:00.123456789Z")),
            (7, instant(U::Seconds), json!({"t": -2208988800_i64}), "UTC", at("1900-01-01T00:00:00Z")),
            (8, instant(U::Iso8601), json!({"t": "2026-11-11T14:30:00Z"}), NY, at("2026-11-11T14:30:00Z")),
            (9, instant(U::Iso8601), json!({"t": "2026-11-11T14:30:00.250Z"}), "UTC", at("2026-11-11T14:30:00.25Z")),
            (10, instant(U::Iso8601), json!({"t": "2026-11-11T09:30:00-05:00"}), "UTC", at("2026-11-11T14:30:00Z")),
            (11, instant(U::Iso8601), json!({"t": "2026-11-11T14:30:00+0000"}), "UTC", at("2026-11-11T14:30:00Z")),
            (12, instant(U::Iso8601), json!({"t": "2026-11-11 09:30:00"}), NY, at("2026-11-11T14:30:00Z")),
            (13, instant(U::Iso8601), json!({"t": "2026-11-11T09:30"}), NY, at("2026-11-11T14:30:00Z")),
            (14, instant(U::Iso8601), json!({"t": "2026-11-11"}), NY, on(11)),
            (15, wall(D::Compact, C::Milliseconds), json!({"d": 20261111, "c": 34200000}), NY, at("2026-11-11T14:30:00Z")),
            (16, wall(D::Compact, C::Milliseconds), json!({"d": 20260710, "c": 34200000}), NY, at("2026-07-10T13:30:00Z")),
            (17, wall(D::Compact, C::Milliseconds), json!({"d": 20260308, "c": 9000000}), NY, at("2026-03-08T07:00:00Z")),
            (18, wall(D::Compact, C::Milliseconds), json!({"d": 20261101, "c": 5400000}), NY, at("2026-11-01T05:30:00Z")),
            (19, wall(D::Compact, C::Milliseconds), json!({"d": 20261111, "c": 32400000}), "Asia/Tokyo", at("2026-11-11T00:00:00Z")),
            (20, wall(D::Dashed, C::Seconds), json!({"d": "2026-11-11", "c": 33300}), "+05:30", at("2026-11-11T03:45:00Z")),
            (21, wall(D::Compact, C::Text), json!({"d": "20261111", "c": "09:30:00"}), NY, at("2026-11-11T14:30:00Z")),
            (22, day(D::Compact), json!({"d": 20261111}), NY, on(11)),
            (23, wall(D::Compact, C::Milliseconds), json!({"d": 20261111, "c": 86400000}), "UTC", None),
            (24, wall(D::Compact, C::Milliseconds), json!({"d": 20261341, "c": 0}), "UTC", None),
            (25, instant(U::Seconds), json!({"t": "soon"}), "UTC", None),
            (25, instant(U::Seconds), json!({"t": null}), "UTC", None),
            (25, instant(U::Seconds), json!({"t": true}), "UTC", None),
            (25, instant(U::Seconds), json!({}), "UTC", None),
        ];
        for (row, spec, record, zone, expected) in rows {
            let zone: TimezoneId = zone.parse().unwrap();
            assert_eq!(
                stamp(&record, &spec, &zone),
                expected,
                "row {row}: {record}"
            );
        }

        // The bounds of a request, as each form prints them. 03:00 UTC on 11 November is still
        // the 10th in New York and already noon of the 11th in Tokyo.
        let afternoon: DateTime<Utc> = "2026-11-11T14:30:00Z".parse().unwrap();
        let night: DateTime<Utc> = "2026-11-11T03:00:00Z".parse().unwrap();
        for (format, instant, name, expected) in [
            (RangeFormat::Seconds, afternoon, "UTC", "1794407400"),
            (RangeFormat::Milliseconds, afternoon, "UTC", "1794407400000"),
            (
                RangeFormat::Microseconds,
                afternoon,
                "UTC",
                "1794407400000000",
            ),
            (
                RangeFormat::Nanoseconds,
                afternoon,
                "UTC",
                "1794407400000000000",
            ),
            (RangeFormat::Iso8601, afternoon, NY, "2026-11-11T14:30:00Z"),
            (RangeFormat::Date, afternoon, NY, "2026-11-11"),
            (RangeFormat::Date, night, NY, "2026-11-10"),
            (RangeFormat::CompactDate, night, NY, "20261110"),
            (RangeFormat::CompactDate, night, "Asia/Tokyo", "20261111"),
        ] {
            let zone: TimezoneId = name.parse().unwrap();
            assert_eq!(
                print(instant, &format, &zone),
                expected,
                "{format:?} in {name}"
            );
        }
    }
}
