//! Serializable calculation inputs shared by studies and saved alerts.

use super::{PriceSource, Timeframe};
use chrono::NaiveTime;
use egui::Color32;

/// A price series or an output of another study on the chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum StudySource {
    /// A field or average of fields of the input bars.
    Price(PriceSource),
    /// A stable chart study id and its zero-based output column, without plot displacement.
    Study {
        /// Source series identifier assigned by the chart.
        id: usize,
        /// Zero-based plot index within the source study.
        plot: usize,
    },
}

/// A study input's current value.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum InputValue {
    /// A bar count or nonnegative integer parameter.
    Length(usize),
    /// A signed integer parameter.
    Integer(i32),
    /// A finite numeric parameter.
    Number(f64),
    /// Index in the input's choices.
    Choice(usize),
    /// An enabled or disabled input.
    Toggle(bool),
    /// A price field or another study's output.
    Source(StudySource),
    /// An RGBA colour.
    Color(Color32),
    /// A calculation interval; `None` follows the chart.
    Timeframe(Option<Timeframe>),
    /// A host-resolved symbol; an empty string follows the chart.
    Symbol(String),
    /// Text the study parses itself, such as a date and time.
    Text(String),
    /// A time of day in hours and minutes, saved as `"HH:MM"`. Seconds are not part of the
    /// value: studies refuse a time with seconds.
    TimeOfDay(#[serde(with = "hours_minutes")] NaiveTime),
    /// A time zone name, such as `America/New_York`, `UTC` or `+05:30`. An empty name follows
    /// the instrument's exchange zone where the input offers it.
    TimeZone(String),
}

/// `"HH:MM"`, the saved form of [`InputValue::TimeOfDay`].
mod hours_minutes {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        time: &NaiveTime,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&time.format("%H:%M"))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<NaiveTime, D::Error> {
        let text = String::deserialize(deserializer)?;
        NaiveTime::parse_from_str(&text, "%H:%M").map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A time of day saves as its hours and minutes and a zone as its name; a saved time past
    /// 23:59, with seconds or as a number is refused.
    #[test]
    fn time_and_zone_values_save_as_text() {
        for (value, json) in [
            (
                InputValue::TimeOfDay(NaiveTime::from_hms_opt(9, 30, 0).unwrap()),
                r#"{"TimeOfDay":"09:30"}"#,
            ),
            (
                InputValue::TimeOfDay(NaiveTime::MIN),
                r#"{"TimeOfDay":"00:00"}"#,
            ),
            (
                InputValue::TimeOfDay(NaiveTime::from_hms_opt(23, 59, 0).unwrap()),
                r#"{"TimeOfDay":"23:59"}"#,
            ),
            (
                InputValue::TimeZone("America/New_York".into()),
                r#"{"TimeZone":"America/New_York"}"#,
            ),
            (InputValue::TimeZone(String::new()), r#"{"TimeZone":""}"#),
        ] {
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert_eq!(serde_json::from_str::<InputValue>(json).unwrap(), value);
        }
        for json in [
            r#"{"TimeOfDay":"24:00"}"#,
            r#"{"TimeOfDay":"09:30:15"}"#,
            r#"{"TimeOfDay":570}"#,
        ] {
            assert!(serde_json::from_str::<InputValue>(json).is_err(), "{json}");
        }
    }
}
