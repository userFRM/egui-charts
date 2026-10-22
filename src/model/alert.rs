//! Price alerts, as a host or a server stores and evaluates them.

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
    /// The symbol whose prices the condition is judged on.
    pub symbol: String,
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
            frequency: AlertFrequency::default(),
            expires: None,
            message: String::new(),
            status: AlertStatus::default(),
        }
    }
}

/// When an alert fires, judged on the prices of [`Alert::symbol`](crate::model::Alert::symbol).
/// Each variant states its rule so that a host and a server evaluate it alike. Levels are finite
/// and in the symbol's price units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertCondition {
    /// The price moves from one side of the level to the other, in either direction.
    Crossing(f64),
    /// The previous price was below the level and the current price is at or above it.
    CrossingUp(f64),
    /// The previous price was above the level and the current price is at or below it.
    CrossingDown(f64),
    /// The price is above the level.
    Above(f64),
    /// The price is below the level.
    Below(f64),
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
    fn a_minimal_stored_alert_loads() {
        let stored = r#"{"symbol":"AAPL","condition":{"CrossingUp":101.5}}"#;
        let alert: Alert = serde_json::from_str(stored).unwrap();
        assert_eq!(
            alert,
            Alert {
                id: None,
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
