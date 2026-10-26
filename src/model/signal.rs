//! What a host's analytics found, in the form the chart is to show it.

use chrono::{DateTime, Utc};

/// One finding of the host's analytics: a marker, a level or a zone, for the chart to draw in the
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
#[non_exhaustive]
pub enum SignalShape {
    /// A mark on the bar containing `time`, at `price` or, without one, beside the bar.
    Marker {
        /// UTC time at which the marker is anchored.
        time: DateTime<Utc>,
        /// Marker price; `None` uses the direction's default placement.
        price: Option<f64>,
        /// Direction used for marker placement and color.
        direction: Direction,
    },
    /// A horizontal line at `price`, starting at `from` or, without one, across the whole chart.
    Level {
        /// Price of the horizontal level.
        price: f64,
        /// Start time, or `None` for a level across the whole chart.
        from: Option<DateTime<Utc>>,
    },
    /// The prices from `low` to `high` between `from` and `to`, or from `from` onward without
    /// `to`. `low` is at most `high`.
    Zone {
        /// Start time of the zone.
        from: DateTime<Utc>,
        /// End time, or `None` for an open-ended zone.
        to: Option<DateTime<Utc>>,
        /// Lower boundary of the zone.
        low: f64,
        /// Upper boundary of the zone.
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
