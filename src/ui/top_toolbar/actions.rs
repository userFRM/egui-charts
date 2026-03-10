//! Actions emitted by the top toolbar.

use super::components::layout_menu::LayoutAction;
use crate::ui::multi_chart::ChartSyncOptions;
use crate::ui::stubs::ChartLayoutMode;

/// Actions emitted by the top toolbar
#[derive(Debug, Clone, PartialEq)]
pub enum TopToolbarAction {
    /// No action taken
    None,
    /// Open main menu (burger menu)
    OpenMenu,
    /// Open symbol search dialog
    OpenSymbolSearch,
    /// Open compare/add symbol dialog
    OpenCompare,
    /// Chart interval changed (e.g. "1D", "1W")
    IntervalSelected(String),
    /// Chart style changed (e.g. "Candles", "Line")
    ChartStyleSelected(String),
    /// Open indicators dialog
    OpenIndicators,
    /// Open indicator templates panel
    OpenIndicatorTemplates,
    /// Create a new price alert
    CreateAlert,
    /// Toggle replay mode on/off
    ToggleReplay,
    /// Open the trading panel
    OpenTradingPanel,
    /// Undo last action
    Undo,
    /// Redo last undone action
    Redo,
    /// Capture a chart screenshot
    TakeScreenshot,
    /// Open settings dialog
    OpenSettings,
    /// Save current chart layout
    Save,
    /// Publish chart idea
    Publish,
    /// Toggle fullscreen mode
    ToggleFullscreen,
    /// Toggle light/dark theme
    ThemeToggle,
    /// Layout management actions
    Layout(LayoutAction),
    /// Multi-chart grid layout changed
    ChartGridChanged(ChartLayoutMode),
    /// Multi-chart sync options changed
    SetSyncOptions(ChartSyncOptions),
}
