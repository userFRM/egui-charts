//! Typed alerts, as a host or a server stores and evaluates them.

use chrono::{DateTime, Datelike, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};

use super::{Bar, Timeframe, TimezoneId};

/// The host's identifier of an alert.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertId(
    /// The host's own identifier, unique among its alerts.
    pub String,
);

/// An alert as the host stores and evaluates it. The library creates and edits alerts through
/// [`Alerts::save`](crate::services::Alerts::save) and shows the ones the host pushes. It evaluates
/// them only through the host's service; `AlertEngine` (feature `alert-engine`) is one.
///
/// It is serialisable so that a server can store and evaluate the same type. Fields added later
/// will carry `#[serde(default)]`, so alerts stored by an earlier version keep loading. A
/// condition, frequency or status added later does not load in an earlier version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Alert {
    /// `None` for an alert the user is creating; the host assigns the id.
    #[serde(default)]
    pub id: Option<AlertId>,
    /// The symbol whose bars supply the sources, unless `watchlist` names the subject.
    pub symbol: String,
    /// User-facing name, independent of the notification message.
    #[serde(default)]
    pub name: String,
    /// Host watchlist identifier. When present, evaluate independently on each member symbol.
    #[serde(default)]
    pub watchlist: Option<String>,
    /// The timeframe whose bars the alert is evaluated on. `None` for an alert that has not been
    /// fixed to one yet; `AlertEngine` fixes it to the first timeframe it evaluates the alert on.
    #[serde(default)]
    pub timeframe: Option<Timeframe>,
    /// Value tested by the first condition.
    #[serde(default)]
    pub source: AlertSource,
    /// Additional conditions, all required on the same bar as the first condition.
    #[serde(default)]
    pub additional_conditions: Vec<AlertRule>,
    /// When the alert fires.
    pub condition: AlertCondition,
    /// Whether the alert fires once or on every occurrence.
    #[serde(default)]
    pub frequency: AlertFrequency,
    /// The alert expires at this time; `None` never expires.
    #[serde(default)]
    pub expires: Option<DateTime<Utc>>,
    /// When notifications may be delivered; `None` delivers at any time. The schedule restricts
    /// delivery only: the alert is still evaluated, and fires, outside it.
    #[serde(default)]
    pub schedule: Option<Box<NotificationSchedule>>,
    /// Text the host shows or sends when the alert fires. [`Alert::expand_message`] fills in the
    /// placeholders `{{symbol}}`, `{{close}}` and so on when it fires.
    #[serde(default)]
    pub message: String,
    /// Where the alert stands, as the host reports it.
    #[serde(default)]
    pub status: AlertStatus,
}

impl Alert {
    /// An active alert on `symbol` with no id, frequency `Once`, no expiry and no message.
    pub fn new(symbol: impl Into<String>, condition: AlertCondition) -> Self {
        Self {
            id: None,
            symbol: symbol.into(),
            condition,
            name: String::new(),
            watchlist: None,
            timeframe: None,
            source: AlertSource::Price,
            additional_conditions: Vec::new(),
            frequency: AlertFrequency::default(),
            expires: None,
            schedule: None,
            message: String::new(),
            status: AlertStatus::default(),
        }
    }
}

impl Alert {
    /// The message with its placeholders filled in from the trigger: `{{symbol}}` and
    /// `{{timeframe}}` as given, `{{time}}` the open time of `bar` in RFC 3339, `{{open}}`,
    /// `{{high}}`, `{{low}}`, `{{close}}` and `{{volume}}` of `bar`, `{{value}}` the value of the
    /// alert's source and `{{output_N}}` the value of output `N` of the study the source names,
    /// empty if the source is not a study. `value` gives the value of a source at the bar. A value
    /// that is missing or not finite becomes empty text, a number is written in the shortest form
    /// that reads back the same, and a placeholder the alert does not know stays as written.
    ///
    /// ```
    /// use egui_charts::model::{Alert, AlertCondition, Bar, Timeframe};
    ///
    /// let mut alert = Alert::new("AAPL", AlertCondition::Above(100.0));
    /// alert.message = "{{symbol}} closed at {{close}} on {{timeframe}}".into();
    /// let time = chrono::DateTime::from_timestamp(0, 0).unwrap();
    /// let bar = Bar::new(time, 99.0, 102.0, 98.0, 101.5, 1000.0);
    /// assert_eq!(
    ///     alert.expand_message("AAPL", Timeframe::Min5, &bar, |_| None),
    ///     "AAPL closed at 101.5 on 5min"
    /// );
    /// ```
    pub fn expand_message(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        bar: &Bar,
        value: impl Fn(&AlertSource) -> Option<f64>,
    ) -> String {
        let number = |number: Option<f64>| {
            number
                .filter(|number| number.is_finite())
                .map(|number| number.to_string())
                .unwrap_or_default()
        };
        let field = |name: &str| -> Option<String> {
            Some(match name {
                "symbol" => symbol.to_owned(),
                "timeframe" => timeframe.to_string(),
                "time" => bar.time.to_rfc3339(),
                "open" => number(Some(bar.open)),
                "high" => number(Some(bar.high)),
                "low" => number(Some(bar.low)),
                "close" => number(Some(bar.close)),
                "volume" => number(Some(bar.volume)),
                "value" => number(value(&self.source)),
                _ => {
                    let output = name.strip_prefix("output_")?.parse().ok()?;
                    match &self.source {
                        AlertSource::Study { id, .. } => number(value(&AlertSource::Study {
                            id: id.clone(),
                            output,
                        })),
                        _ => String::new(),
                    }
                }
            })
        };
        let mut text = String::new();
        let mut rest = self.message.as_str();
        while let Some(start) = rest.find("{{")
            && let Some(length) = rest[start + 2..].find("}}")
        {
            let end = start + 2 + length;
            text.push_str(&rest[..start]);
            match field(&rest[start + 2..end]) {
                Some(filled) => text.push_str(&filled),
                None => text.push_str(&rest[start..end + 2]),
            }
            rest = &rest[end + 2..];
        }
        text.push_str(rest);
        text
    }
}

/// When an alert fires, judged on its source for each subject symbol.
/// Each variant states its rule so that a host and a server evaluate it alike. Levels are finite
/// and in the symbol's price units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertCondition {
    /// The previous value was strictly below or above the level and the current value reaches
    /// or passes it, in either direction.
    Crossing(f64),
    /// The previous price was below the level and the current price is at or above it.
    CrossingUp(f64),
    /// The previous price was above the level and the current price is at or below it.
    CrossingDown(f64),
    /// The price is above the level.
    Above(f64),
    /// The price is below the level.
    Below(f64),
    /// Previous value was outside the inclusive channel, current value is inside it.
    EnteringChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// Previous value was inside the inclusive channel, current value is outside it.
    ExitingChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// The current value is inside the inclusive channel, `lower <= value <= upper`, whatever the
    /// previous value was.
    InsideChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// The current value is outside the inclusive channel, below `lower` or above `upper`,
    /// whatever the previous value was.
    OutsideChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// Signed change from `bars` bars ago reaches `value`: at least for positive, at most for negative.
    MovingBy {
        /// Signed, nonzero change in source units.
        value: f64,
        /// Number of bars back, at least one.
        bars: u32,
    },
    /// Signed percentage change, 100 * (current - previous) / abs(previous), over `bars` bars.
    /// A zero previous value cannot satisfy this condition.
    MovingByPercent {
        /// Signed, nonzero percentage, with 1 meaning 1%.
        percent: f64,
        /// Number of bars back, at least one.
        bars: u32,
    },
    /// Compare against another output on the same bar, using its previous value for crossings.
    Compare {
        /// The second output on the same symbol and bar.
        other: AlertSource,
        /// The required relationship between the outputs.
        comparison: AlertComparison,
    },
    /// Current volume exceeds `multiplier` times the mean of the preceding `bars` completed volumes.
    VolumeAboveAverage {
        /// Positive, finite multiple of the preceding mean volume.
        multiplier: f64,
        /// Number of preceding bars in the mean, at least one.
        bars: u32,
    },
}

/// A host-resolved value on a bar. Identifiers remain meaningful to the host across sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSource {
    /// The symbol's closing or latest price.
    #[default]
    Price,
    /// A study instance and its zero-based output index.
    Study {
        /// Host's stable identifier for the study instance.
        id: String,
        /// Zero-based output line within the study.
        output: usize,
    },
    /// A drawing's price at the bar's time, resolved by the host.
    Drawing(String),
    /// The bar's volume.
    Volume,
}

/// Comparison of two outputs. Crossings require a strict previous side and include equality now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertComparison {
    /// Cross in either direction.
    Crossing,
    /// Cross from below.
    CrossingUp,
    /// Cross from above.
    CrossingDown,
    /// Strictly greater.
    Above,
    /// Strictly less.
    Below,
}

/// One additional condition of an alert; all rules must hold on the same bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    /// The value to test.
    pub source: AlertSource,
    /// The required relationship.
    pub condition: AlertCondition,
}

impl AlertSource {
    fn is_valid(&self) -> bool {
        match self {
            Self::Study { id, .. } | Self::Drawing(id) => !id.trim().is_empty(),
            _ => true,
        }
    }
}

impl AlertCondition {
    /// Whether the condition has finite, ordered levels and a nonzero lookback.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Crossing(v)
            | Self::CrossingUp(v)
            | Self::CrossingDown(v)
            | Self::Above(v)
            | Self::Below(v) => v.is_finite(),
            Self::EnteringChannel { lower, upper }
            | Self::ExitingChannel { lower, upper }
            | Self::InsideChannel { lower, upper }
            | Self::OutsideChannel { lower, upper } => {
                lower.is_finite() && upper.is_finite() && lower < upper
            }
            Self::MovingBy { value, bars } => value.is_finite() && *value != 0.0 && *bars > 0,
            Self::MovingByPercent { percent, bars } => {
                percent.is_finite() && *percent != 0.0 && *bars > 0
            }
            Self::Compare { other, .. } => other.is_valid(),
            Self::VolumeAboveAverage { multiplier, bars } => {
                multiplier.is_finite() && *multiplier > 0.0 && *bars > 0
            }
        }
    }

    /// The single horizontal threshold, if the condition has one.
    pub fn level(&self) -> Option<f64> {
        match self {
            Self::Crossing(v)
            | Self::CrossingUp(v)
            | Self::CrossingDown(v)
            | Self::Above(v)
            | Self::Below(v) => Some(*v),
            _ => None,
        }
    }
}

impl std::fmt::Display for AlertCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Crossing(v) => write!(f, "Crossing {v}"),
            Self::CrossingUp(v) => write!(f, "Crossing up {v}"),
            Self::CrossingDown(v) => write!(f, "Crossing down {v}"),
            Self::Above(v) => write!(f, "Greater than {v}"),
            Self::Below(v) => write!(f, "Less than {v}"),
            Self::EnteringChannel { lower, upper } => write!(f, "Entering {lower} to {upper}"),
            Self::ExitingChannel { lower, upper } => write!(f, "Exiting {lower} to {upper}"),
            Self::InsideChannel { lower, upper } => write!(f, "Inside {lower} to {upper}"),
            Self::OutsideChannel { lower, upper } => write!(f, "Outside {lower} to {upper}"),
            Self::MovingBy { value, bars } => write!(f, "Moving by {value} over {bars} bars"),
            Self::MovingByPercent { percent, bars } => {
                write!(f, "Moving by {percent}% over {bars} bars")
            }
            Self::Compare { comparison, .. } => write!(f, "{} another output", comparison),
            Self::VolumeAboveAverage { multiplier, bars } => write!(
                f,
                "Volume above {multiplier} times the preceding {bars}-bar mean"
            ),
        }
    }
}

impl Alert {
    /// Validate user input before handing it to host storage or evaluation.
    pub fn is_valid(&self) -> bool {
        (!self.symbol.trim().is_empty()
            || self
                .watchlist
                .as_ref()
                .is_some_and(|id| !id.trim().is_empty()))
            && self
                .watchlist
                .as_ref()
                .is_none_or(|id| !id.trim().is_empty())
            && self.source.is_valid()
            && self.condition.is_valid()
            && self
                .additional_conditions
                .iter()
                .all(|rule| rule.source.is_valid() && rule.condition.is_valid())
            && self
                .schedule
                .as_ref()
                .is_none_or(|schedule| schedule.is_valid())
    }
}

/// When an alert's notifications may be delivered: on the selected weekdays, inside the time
/// windows, both read on the clock of `timezone`, so a daylight-saving change moves the windows
/// with the local clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationSchedule {
    /// The zone whose weekday and clock decide.
    pub timezone: TimezoneId,
    /// Weekdays on which a window starts, at least one.
    pub days: Vec<Weekday>,
    /// Delivery windows, at least one.
    pub windows: Vec<NotificationWindow>,
}

/// A window of the day, from `start` up to but excluding `end`, in the schedule's zone. A window
/// whose `end` is before `start` crosses midnight and belongs to the weekday it starts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationWindow {
    /// First clock time inside the window.
    pub start: NaiveTime,
    /// First clock time after the window; different from `start`.
    pub end: NaiveTime,
}

impl NotificationSchedule {
    /// Whether at least one weekday and one window are selected and no window is empty.
    pub fn is_valid(&self) -> bool {
        !self.days.is_empty()
            && !self.windows.is_empty()
            && self.windows.iter().all(|w| w.start != w.end)
    }

    /// Whether a notification raised at `at` may be delivered.
    pub fn allows(&self, at: DateTime<Utc>) -> bool {
        let local = at.with_timezone(&self.timezone);
        let (day, time) = (local.weekday(), local.time());
        self.windows.iter().any(|w| {
            if w.start < w.end {
                self.days.contains(&day) && (w.start..w.end).contains(&time)
            } else {
                (self.days.contains(&day) && time >= w.start)
                    || (self.days.contains(&day.pred()) && time < w.end)
            }
        })
    }
}

/// How often an alert fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertFrequency {
    /// Fires once; the host then reports the alert with status `Triggered`.
    #[default]
    Once,
    /// Stays active and fires on every new occurrence; how often a lasting condition fires again
    /// is the host's choice.
    EveryTime,
    /// At most one trigger per bar, identified by its opening time.
    OncePerBar,
    /// Evaluate only completed bars, at most once per bar.
    OncePerBarClose,
    /// At most one trigger per clock minute of UTC, identified by the minute's start, whatever the
    /// chart's interval.
    OncePerMinute,
}

impl std::fmt::Display for AlertFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Once => "Only once",
            Self::EveryTime => "Every time",
            Self::OncePerBar => "Once per bar",
            Self::OncePerBarClose => "Once per bar close",
            Self::OncePerMinute => "Once per minute",
        })
    }
}

impl std::fmt::Display for AlertComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Crossing => "Crossing",
            Self::CrossingUp => "Crossing up",
            Self::CrossingDown => "Crossing down",
            Self::Above => "Greater than",
            Self::Below => "Less than",
        })
    }
}

/// Where an alert stands. The host sets it; the library only shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertStatus {
    /// Evaluated and waiting for its condition.
    #[default]
    Active,
    /// Fired and no longer evaluated, as an alert with frequency `Once` is after it fires.
    Triggered,
    /// Kept but not evaluated until the user enables it again.
    Disabled,
    /// Its expiry time passed before it fired.
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_named_compound_alert_round_trips() {
        let stored = serde_json::json!({
            "symbol": "AAPL", "name": "Momentum", "message": "Check momentum",
            "watchlist": "technology", "source": {"Study": {"id": "rsi", "output": 0}},
            "condition": {"Above": 70.0}, "frequency": "OncePerBarClose",
            "additional_conditions": [{"source": "Price", "condition": {"EnteringChannel": {"lower": 100.0, "upper": 110.0}}}]
        });
        let alert: Alert = serde_json::from_value(stored.clone()).unwrap();
        let saved = serde_json::to_value(alert).unwrap();
        for (key, value) in stored.as_object().unwrap() {
            assert_eq!(saved.get(key), Some(value), "{key}");
        }
    }

    #[test]
    fn messages_fill_placeholders_from_the_evaluated_bar() {
        let time = DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let bar = Bar::new(time, 99.0, 102.5, f64::NAN, 101.5, 1000.0);
        let study = AlertSource::Study {
            id: "3".into(),
            output: 0,
        };
        // Output 0 of the study is 70.25 and output 1 is 3; no other output has a value.
        let value = |source: &AlertSource| match source {
            AlertSource::Study { id, output: 0 } if id == "3" => Some(70.25),
            AlertSource::Study { id, output: 1 } if id == "3" => Some(3.0),
            AlertSource::Price => Some(101.5),
            _ => None,
        };
        for (message, source, expected) in [
            (
                "{{symbol}} closed at {{close}}",
                AlertSource::Price,
                "AAPL closed at 101.5",
            ),
            (
                "{{timeframe}} {{time}} {{open}} {{high}} [{{low}}] {{volume}}",
                AlertSource::Price,
                "5min 2024-01-01T00:00:00+00:00 99 102.5 [] 1000",
            ),
            ("{{value}}/{{output_1}}", study.clone(), "70.25/3"),
            ("{{output_2}}.", study.clone(), "."),
            ("{{value}}{{output_1}}", AlertSource::Price, "101.5"),
            (
                "{{ close }} {{nope}} {{output_x}} {{close",
                AlertSource::Price,
                "{{ close }} {{nope}} {{output_x}} {{close",
            ),
            ("{{{{close}}}}", AlertSource::Price, "{{{{close}}}}"),
        ] {
            let mut alert = Alert::new("AAPL", AlertCondition::Above(100.0));
            alert.source = source;
            alert.message = message.into();
            assert_eq!(
                alert.expand_message("AAPL", Timeframe::Min5, &bar, value),
                expected,
                "{message}"
            );
        }
    }

    #[test]
    fn saved_alerts_require_finite_levels_ordered_channels_and_positive_lookbacks() {
        type Edit = fn(&mut Alert);
        let rows: [(Edit, bool); 18] = [
            (|a| a.condition = AlertCondition::Crossing(f64::NAN), false),
            (
                |a| a.condition = AlertCondition::Above(f64::INFINITY),
                false,
            ),
            (|a| a.condition = AlertCondition::Below(-12.5), true),
            (
                |a| {
                    a.condition = AlertCondition::InsideChannel {
                        lower: 1.0,
                        upper: 2.0,
                    }
                },
                true,
            ),
            (
                |a| {
                    a.condition = AlertCondition::OutsideChannel {
                        lower: 2.0,
                        upper: 2.0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.schedule = Some(Box::new(NotificationSchedule {
                        timezone: TimezoneId::utc(),
                        days: Vec::new(),
                        windows: vec![NotificationWindow {
                            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                            end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
                        }],
                    }))
                },
                false,
            ),
            (
                |a| {
                    a.schedule = Some(Box::new(NotificationSchedule {
                        timezone: TimezoneId::utc(),
                        days: vec![Weekday::Mon],
                        windows: vec![NotificationWindow {
                            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                            end: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                        }],
                    }))
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::EnteringChannel {
                        lower: 2.0,
                        upper: 1.0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::ExitingChannel {
                        lower: 1.0,
                        upper: 1.0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::MovingBy {
                        value: 1.0,
                        bars: 0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::MovingByPercent {
                        percent: 0.0,
                        bars: 1,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::VolumeAboveAverage {
                        multiplier: -2.0,
                        bars: 20,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::VolumeAboveAverage {
                        multiplier: 2.0,
                        bars: 0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.source = AlertSource::Study {
                        id: " ".into(),
                        output: 0,
                    }
                },
                false,
            ),
            (|a| a.symbol.clear(), false),
            (
                |a| {
                    a.symbol.clear();
                    a.watchlist = Some("technology".into());
                },
                true,
            ),
            (|a| a.watchlist = Some(" ".into()), false),
            (
                |a| {
                    a.additional_conditions.push(AlertRule {
                        source: AlertSource::Drawing(String::new()),
                        condition: AlertCondition::Above(1.0),
                    })
                },
                false,
            ),
        ];
        for (index, (edit, valid)) in rows.into_iter().enumerate() {
            let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
            edit(&mut alert);
            assert_eq!(alert.is_valid(), valid, "row {index}");
        }
    }

    #[test]
    fn a_minimal_stored_alert_loads() {
        let stored = r#"{"symbol":"AAPL","condition":{"CrossingUp":101.5}}"#;
        let alert: Alert = serde_json::from_str(stored).unwrap();
        assert_eq!(
            alert,
            Alert {
                id: None,
                name: String::new(),
                source: AlertSource::Price,
                watchlist: None,
                timeframe: None,
                additional_conditions: Vec::new(),
                symbol: "AAPL".into(),
                condition: AlertCondition::CrossingUp(101.5),
                frequency: AlertFrequency::Once,
                expires: None,
                schedule: None,
                message: String::new(),
                status: AlertStatus::Active,
            }
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn a_stored_schedule_round_trips_and_a_window_may_cross_midnight() {
        let stored = serde_json::json!({
            "symbol": "AAPL", "condition": {"InsideChannel": {"lower": 1.0, "upper": 2.0}},
            "frequency": "OncePerMinute",
            "schedule": {"timezone": "America/New_York", "days": ["Fri"],
                "windows": [{"start": "22:00:00", "end": "02:00:00"}]}
        });
        let alert: Alert = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&alert).unwrap()["schedule"],
            stored["schedule"]
        );
        let schedule = alert.schedule.unwrap();
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        // Friday 2026-03-06 22:30 EST is 03:30 UTC on Saturday: inside. Saturday 01:59 EST
        // still belongs to Friday's window; 02:00 does not.
        assert!(schedule.allows(at("2026-03-07T03:30:00Z")));
        assert!(schedule.allows(at("2026-03-07T06:59:00Z")));
        assert!(!schedule.allows(at("2026-03-07T07:00:00Z")));
        // Thursday 22:30 EST: its weekday is not selected.
        assert!(!schedule.allows(at("2026-03-06T03:30:00Z")));
        // After the clocks go forward on Sunday 2026-03-08, Friday 22:30 is 02:30 UTC.
        assert!(schedule.allows(at("2026-03-14T02:30:00Z")));
        assert!(schedule.allows(at("2026-03-14T05:59:00Z")));
        assert!(!schedule.allows(at("2026-03-14T06:00:00Z")));
    }
}
