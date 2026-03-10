//! Application state trait for UI synchronization.
//!
//! UI components need to read from centralized app state. Instead of
//! depending on a concrete AppState type, components accept any type
//! implementing [`ChartAppState`].

use crate::model::{ChartType, Timeframe};

/// Trait for application state that chart UI components can read from.
///
/// Library consumers implement this on their app state type to connect
/// the UI components to their data layer.
pub trait ChartAppState {
    /// Currently active trading symbol (e.g., "AAPL", "BTC/USD").
    fn active_symbol(&self) -> &str;
    /// Current chart timeframe.
    fn active_timeframe(&self) -> &Timeframe;
    /// Current chart visualization type.
    fn chart_type(&self) -> ChartType;
    /// Whether the data connection is active.
    fn is_connected(&self) -> bool;
    /// Whether replay mode is active.
    fn is_replay_active(&self) -> bool;
}
