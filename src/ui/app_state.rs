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
/// method; every control starts visible. This controls presentation, not authorization:
/// hosts choose which services, commands and shortcut bindings they provide.
/// Chart behavior switches live in [`ChartOptions`](crate::config::ChartOptions);
/// component `config` fields override dimensions and [`Theme`](crate::theme::Theme) overrides
/// colours and text sizes. [`ChartBuilder::with_theme`](crate::widget::ChartBuilder::with_theme)
/// keeps those theme properties local to one chart.
///
/// ```
/// use egui_charts::ui::{Control, context_menu::ChartMenus, top_toolbar::TopToolbar};
/// let mut toolbar = TopToolbar::new();
/// let mut menus = ChartMenus::default();
/// for control in [Control::Alerts, Control::Trading, Control::Settings] {
///     toolbar.set_control_visible(control, false);
///     menus.set_control_visible(control, false);
/// }
/// toolbar.set_control_visible(Control::Snapshot, false);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Control {
    /// The top toolbar's menu button or host menu slot.
    Menu,
    /// The top toolbar's symbol search button.
    SymbolSearch,
    /// The top toolbar's interval selector.
    Interval,
    /// The top toolbar's chart type selector.
    ChartType,
    /// Study selection and removal entries in the toolbar and series/chart menus.
    Indicators,
    /// The top toolbar's undo button. Shortcut bindings remain host configurable.
    Undo,
    /// The top toolbar's redo button. Shortcut bindings remain host configurable.
    Redo,
    /// The top toolbar's theme toggle.
    Theme,
    /// The top toolbar's quick search button.
    QuickSearch,
    /// Chart and series settings buttons and menu entries.
    Settings,
    /// The top toolbar's fullscreen button.
    Fullscreen,
    /// The top toolbar's snapshot button and menu.
    Snapshot,
    /// Object Tree buttons and chart-menu entries.
    ObjectTree,
    /// The right rail's help button.
    Help,
    /// All chart, series, drawing and axis context menus.
    ContextMenus,
    /// Reset-view entries in the chart menu.
    ResetView,
    /// Copy-price entries in chart and series menus.
    CopyPrice,
    /// CSV export entries in the chart menu.
    ExportData,
    /// Alert creation: the top toolbar's Alert button, the right rail's Alerts button and the
    /// add alert rows of the chart and series menus.
    Alerts,
    /// Positions and orders: the top toolbar's Trade button, the drawing toolbar's hide rows for
    /// them and the chart menu's order rows.
    Trading,
    /// The top toolbar's Publish button.
    Publish,
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
