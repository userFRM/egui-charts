//! Reading the time of a row or record, and printing the bounds of a request.

use chrono::{DateTime, NaiveDate, Utc};
use egui_charts::model::TimezoneId;
use serde_json::Value;

use super::profile::{FieldPath, RangeFormat, TimeSpec};

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
pub(crate) fn stamp(_record: &Value, _spec: &TimeSpec, _zone: &TimezoneId) -> Option<Stamp> {
    unimplemented!("part A")
}

/// The reading of the time field at `field` that fits `value` best: ISO 8601 for text that
/// reads as it, a date for eight digits that are a calendar day between 1900 and 2100, and
/// otherwise the Unix unit whose magnitude fits (below 1e11 seconds, below 1e14 milliseconds,
/// below 1e17 microseconds, else nanoseconds). The dialog offers it and the user can change it.
pub(crate) fn guess(_field: FieldPath, _value: &Value) -> TimeSpec {
    unimplemented!("part A")
}

/// `instant` as the text of `{from}` or `{to}`. The two date forms print the calendar day of
/// `instant` in `zone`.
pub(crate) fn print(_instant: DateTime<Utc>, _format: &RangeFormat, _zone: &TimezoneId) -> String {
    unimplemented!("part A")
}
