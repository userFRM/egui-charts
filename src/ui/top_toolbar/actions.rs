//! Actions emitted by the top toolbar.

use super::components::layout_menu::LayoutAction;

/// Actions emitted by the top toolbar
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TopToolbarAction {
    /// A registered host button or dropdown item was chosen.
    Host(String),
    /// No action taken
    None,
    /// Open the command palette for actions, settings, drawing tools and studies.
    OpenQuickSearch,
    /// Open symbol search dialog
    OpenSymbolSearch,
    /// Open compare/add symbol dialog
    OpenCompare,
    /// Chart interval chosen in the interval menu, a preset or a custom interval
    IntervalSelected(crate::model::Timeframe),
    /// Chart style chosen in the chart-type menu.
    ChartStyleSelected(crate::model::ChartType),
    /// Open indicators dialog
    OpenIndicators,
    /// Ask the host to add the study registered under this name, chosen among
    /// [`TopToolbar::favorite_indicators`](super::TopToolbar::favorite_indicators).
    AddIndicator(String),
    /// Ask the host to save the current indicator set.
    SaveIndicatorTemplate,
    /// Ask the host to apply the saved indicator set with this id.
    ApplyIndicatorTemplate(String),
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
    /// Capture the chart with [`Chart::export_png`](crate::Chart::export_png) and deliver the
    /// decoded image to the clipboard, for example with `egui::Context::copy_image`.
    CopyImage,
    /// Capture the chart and open the rendered image in a new browser tab or native viewer.
    /// The host owns the image URL or local file and its lifetime.
    OpenImage,
    /// Publish the captured image through the host's service and copy its returned URL.
    /// Offered only with [`TopToolbarState::can_publish_image`](super::TopToolbarState::can_publish_image).
    CopyImageLink,
    /// Ask the host to publish the chart: capture it with
    /// [`Chart::export_image`](crate::widget::Chart::export_image) and share or post the image
    /// through the host's own service.
    Publish,
    /// Open or close the order ticket, for example with
    /// [`Trading::toggle_ticket`](crate::ui::trading::Trading::toggle_ticket).
    ToggleTrading,
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
