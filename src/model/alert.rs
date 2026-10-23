//! Typed alerts, as a host or a server stores and evaluates them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The host's identifier of an alert.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertId(
    /// The host's own identifier, unique among its alerts.
    pub String,
);

/// An alert as the host stores and evaluates it. The library creates and edits alerts through
/// [`Alerts::save`](crate::services::Alerts::save) and shows the ones the host pushes; it never
/// evaluates them.
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
    /// Text the host shows or sends when the alert fires.
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
            source: AlertSource::Price,
            additional_conditions: Vec::new(),
            frequency: AlertFrequency::default(),
            expires: None,
            message: String::new(),
            status: AlertStatus::default(),
        }
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
            Self::EnteringChannel { lower, upper } | Self::ExitingChannel { lower, upper } => {
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
}

impl std::fmt::Display for AlertFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Once => "Only once",
            Self::EveryTime => "Every time",
            Self::OncePerBar => "Once per bar",
            Self::OncePerBarClose => "Once per bar close",
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
                additional_conditions: Vec::new(),
                symbol: "AAPL".into(),
                condition: AlertCondition::CrossingUp(101.5),
                frequency: AlertFrequency::Once,
                expires: None,
                message: String::new(),
                status: AlertStatus::Active,
            }
        );
    }
}
