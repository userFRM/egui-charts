//! Time zones with optional IANA daylight-saving rules.

use std::{borrow::Cow, fmt, str::FromStr};

use chrono::{
    DateTime, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone,
    Timelike, Utc,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A validated time zone, independent of the optional time zone database.
///
/// UTC and fixed offsets (`+05:45`, `-03:30`, or `+00:19:32`) work in every
/// build. IANA names such as `America/New_York` need the `timezones` feature,
/// enabled by default. Unavailable names are rejected rather than approximated.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimezoneId {
    name: Cow<'static, str>,
    zone: Zone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Zone {
    Fixed(FixedOffset),
    #[cfg(feature = "timezones")]
    Named(chrono_tz::Tz),
}

impl TimezoneId {
    /// Coordinated Universal Time.
    pub fn utc() -> Self {
        Self {
            name: Cow::Borrowed("UTC"),
            zone: Zone::Fixed(FixedOffset::east_opt(0).unwrap()),
        }
    }

    /// The zone of an IANA name or fixed offset, if available in this build.
    pub fn named(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    /// A constant offset east of UTC, in seconds, strictly between -24 and +24 hours.
    /// Negative values are west of UTC; fixed offsets have no daylight-saving changes.
    pub fn fixed_offset(seconds: i32) -> Option<Self> {
        let offset = FixedOffset::east_opt(seconds)?;
        Some(Self {
            name: Cow::Owned(offset.to_string()),
            zone: Zone::Fixed(offset),
        })
    }

    /// The IANA name, `UTC`, or signed fixed offset.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn offset(&self, fixed: FixedOffset) -> TimezoneOffset {
        TimezoneOffset {
            zone: self.clone(),
            fixed,
        }
    }
}

impl Default for TimezoneId {
    fn default() -> Self {
        Self::utc()
    }
}

impl FromStr for TimezoneId {
    type Err = &'static str;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if name == "UTC" {
            return Ok(Self::utc());
        }
        let bytes = name.as_bytes();
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            let valid = matches!(bytes.len(), 6 | 9)
                && bytes[3] == b':'
                && (bytes.len() == 6 || bytes[6] == b':')
                && bytes.iter().enumerate().skip(1).all(|(i, b)| {
                    if i == 3 || i == 6 {
                        *b == b':'
                    } else {
                        b.is_ascii_digit()
                    }
                });
            if valid {
                let number = |i| i32::from(bytes[i] - b'0') * 10 + i32::from(bytes[i + 1] - b'0');
                let (hours, minutes) = (number(1), number(4));
                let seconds = if bytes.len() == 9 { number(7) } else { 0 };
                if hours < 24 && minutes < 60 && seconds < 60 {
                    let offset = hours * 3600 + minutes * 60 + seconds;
                    return Self::fixed_offset(if bytes[0] == b'-' { -offset } else { offset })
                        .ok_or("Invalid fixed time zone offset");
                }
            }
            return Err("Invalid fixed time zone offset");
        }
        #[cfg(feature = "timezones")]
        if let Ok(zone) = name.parse::<chrono_tz::Tz>() {
            return Ok(Self {
                name: Cow::Borrowed(zone.name()),
                zone: Zone::Named(zone),
            });
        }
        Err("Unknown time zone or timezones feature disabled")
    }
}

impl fmt::Display for TimezoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Serialize for TimezoneId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for TimezoneId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// An instant's UTC offset, retaining its zone for chrono calendar operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimezoneOffset {
    zone: TimezoneId,
    fixed: FixedOffset,
}

impl Offset for TimezoneOffset {
    fn fix(&self) -> FixedOffset {
        self.fixed
    }
}

impl fmt::Display for TimezoneOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fixed.fmt(f)
    }
}

impl TimeZone for TimezoneId {
    type Offset = TimezoneOffset;

    fn from_offset(offset: &Self::Offset) -> Self {
        offset.zone.clone()
    }

    fn offset_from_local_date(&self, _local: &NaiveDate) -> LocalResult<Self::Offset> {
        match self.zone {
            Zone::Fixed(offset) => LocalResult::Single(self.offset(offset)),
            #[cfg(feature = "timezones")]
            Zone::Named(zone) => zone
                .offset_from_local_date(_local)
                .map(|offset| self.offset(offset.fix())),
        }
    }

    fn offset_from_local_datetime(&self, _local: &NaiveDateTime) -> LocalResult<Self::Offset> {
        match self.zone {
            Zone::Fixed(offset) => LocalResult::Single(self.offset(offset)),
            #[cfg(feature = "timezones")]
            Zone::Named(zone) => zone
                .offset_from_local_datetime(_local)
                .map(|offset| self.offset(offset.fix())),
        }
    }

    fn offset_from_utc_date(&self, _utc: &NaiveDate) -> Self::Offset {
        match self.zone {
            Zone::Fixed(offset) => self.offset(offset),
            #[cfg(feature = "timezones")]
            Zone::Named(zone) => self.offset(zone.offset_from_utc_date(_utc).fix()),
        }
    }

    fn offset_from_utc_datetime(&self, _utc: &NaiveDateTime) -> Self::Offset {
        match self.zone {
            Zone::Fixed(offset) => self.offset(offset),
            #[cfg(feature = "timezones")]
            Zone::Named(zone) => self.offset(zone.offset_from_utc_datetime(_utc).fix()),
        }
    }
}

/// Local fields, falling back to UTC only beyond the representable date range.
pub(crate) fn local_time<T: TimeZone>(zone: T, time: DateTime<Utc>) -> NaiveDateTime {
    time.naive_utc()
        .checked_add_offset(zone.offset_from_utc_datetime(&time.naive_utc()).fix())
        .unwrap_or(time.naive_utc())
}

/// The first instant of a repeated wall time; a skipped time opens at the clock jump.
pub(crate) fn local_instant<T: TimeZone>(
    zone: T,
    mut local: NaiveDateTime,
) -> Option<DateTime<Utc>> {
    if let Some(time) = zone.from_local_datetime(&local).earliest() {
        return Some(time.to_utc());
    }
    local = local.with_nanosecond(0)?;
    for _ in 0..=24 * 60 * 60 {
        local = local.checked_add_signed(Duration::seconds(1))?;
        if let Some(time) = zone.from_local_datetime(&local).earliest() {
            return Some(time.to_utc());
        }
    }
    None
}
