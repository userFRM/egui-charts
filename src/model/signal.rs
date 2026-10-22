//! What a host's analytics found, as the chart shows it.

use chrono::{DateTime, Utc};

/// One finding of the host's analytics: a marker, a level or a zone, which the chart draws in the
/// theme's colours for its direction and category.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    /// Identifies the signal within its subscription: an upsert with the same id replaces it.
    pub id: SignalId,
    /// Where the signal sits on the chart.
    pub shape: SignalShape,
    /// The group the user can hide signals by, such as "Large trades" or "Pivots".
    pub category: String,
    /// Short text drawn next to the signal.
    pub label: Option<String>,
    /// Text shown while the pointer is over the signal.
    pub tooltip: Option<String>,
}

/// Identifies a signal. The host chooses it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SignalId(
    /// The host's own identifier, unique within a subscription.
    pub String,
);

/// Where a signal sits on the chart, anchored by time so it keeps its place through zoom,
/// interval changes and history loads. Prices are finite and in the symbol's price units.
#[derive(Clone, Debug, PartialEq)]
pub enum SignalShape {
    /// A mark on the bar containing `time`, at `price` or, without one, beside the bar.
    Marker {
        time: DateTime<Utc>,
        price: Option<f64>,
        direction: Direction,
    },
    /// A horizontal line at `price`, starting at `from` or, without one, across the whole chart.
    Level {
        price: f64,
        from: Option<DateTime<Utc>>,
    },
    /// The prices from `low` to `high` between `from` and `to`, or from `from` onward without
    /// `to`. `low` is at most `high`.
    Zone {
        from: DateTime<Utc>,
        to: Option<DateTime<Utc>>,
        low: f64,
        high: f64,
    },
}

/// Whether a signal is bullish, bearish or neither, which picks its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Bullish: drawn in the theme's up colour.
    Up,
    /// Bearish: drawn in the theme's down colour.
    Down,
    /// Neither bullish nor bearish: drawn in a neutral colour.
    Neutral,
}
