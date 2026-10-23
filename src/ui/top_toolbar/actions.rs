//! Actions emitted by the top toolbar.

use super::components::layout_menu::LayoutAction;

/// Actions emitted by the top toolbar
#[derive(Debug, Clone, PartialEq)]
pub enum TopToolbarAction {
    /// No action taken
    None,
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
    /// Toggle fullscreen mode
    ToggleFullscreen,
    /// Toggle light/dark theme
    ThemeToggle,
    /// Layout management actions
    Layout(LayoutAction),
}
