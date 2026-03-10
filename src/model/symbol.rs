//! Trading symbol identification.
//!
//! A lightweight symbol handle used throughout the chart engine to identify
//! which financial instrument is being displayed or subscribed to.

/// Represents a trading symbol (financial instrument identifier).
///
/// A `Symbol` is the canonical way to reference a tradable instrument in the chart
/// engine. It pairs a machine-readable `name` (e.g. `"BTCUSDT"`, `"AAPL"`) with a
/// human-readable `display_name` (e.g. `"Bitcoin / Tether"`, `"Apple Inc."`).
///
/// The `active` flag indicates whether the symbol is currently subscribed to a
/// live data feed. Symbols default to active on creation.
///
/// # Example
///
/// ```
/// use egui_charts::model::Symbol;
///
/// let sym = Symbol::new("BTCUSDT", "Bitcoin / Tether");
/// assert_eq!(sym.name, "BTCUSDT");
/// assert!(sym.active);
/// ```
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Machine-readable symbol identifier (e.g. `"AAPL"`, `"BTCUSDT"`).
    pub name: String,
    /// Human-readable display name (e.g. `"Apple Inc."`, `"Bitcoin / Tether"`).
    pub display_name: String,
    /// Whether this symbol is actively subscribed to a data feed.
    pub active: bool,
}

impl Symbol {
    /// Creates a new active symbol with the given name and display name.
    pub fn new(name: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            active: true,
        }
    }
}
