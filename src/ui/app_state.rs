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

/// A control the terminal or a host can hide. Hidden controls are not drawn and emit nothing.
///
/// Each component that draws one of these has a `set_control_visible(control, visible)`
/// method; every control starts visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Control {
    /// Alert creation: the top toolbar's Alert button, the right rail's Alerts button and the
    /// add alert rows of the chart and series menus.
    Alerts,
    /// Positions and orders: the drawing toolbar's hide rows for them and the chart menu's
    /// order rows.
    Trading,
    /// The top toolbar's Replay button.
    Replay,
    /// The top toolbar's Save and Layout setup buttons.
    Layouts,
    /// Templates: the top toolbar's indicator templates button, the drawing toolbar's templates
    /// button and menu, and the chart settings dialog's template button.
    Templates,
    /// The top toolbar's Compare button.
    Compare,
    /// The bottom toolbar's time zone menu.
    TimeZone,
    /// The bottom toolbar's session menu.
    Session,
    /// The bottom toolbar's dividend adjustment toggle.
    DividendAdjustment,
}

/// Records in `hidden` whether `control` is shown.
pub(crate) fn set_control_visible(
    hidden: &mut std::collections::HashSet<Control>,
    control: Control,
    visible: bool,
) {
    if visible {
        hidden.remove(&control);
    } else {
        hidden.insert(control);
    }
}
