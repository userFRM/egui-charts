//! Actions emitted by the chart control bar.

/// Actions from the chart control bar
#[derive(Debug, Clone, PartialEq)]
pub enum ChartControlAction {
    /// No action taken
    None,
    /// Zoom in on the chart
    ZoomIn,
    /// Zoom out on the chart
    ZoomOut,
    /// Reset zoom to default level
    ResetZoom,
    /// Toggle auto-scale mode
    ToggleAutoScale,
    /// Toggle logarithmic scale
    ToggleLogScale,
    /// Toggle percentage display mode
    TogglePercentage,
}
