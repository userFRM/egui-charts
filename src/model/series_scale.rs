//! Price scale membership for series in one chart pane.

/// Series with the same assignment share a range and scale options. Relative modes
/// retain a separate first-visible-close reference for each series.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum SeriesScale {
    /// The pane's right axis.
    #[default]
    Right,
    /// The pane's left axis.
    Left,
    /// An independent scale without axis labels, identified by the host.
    Overlay(u64),
    /// An additional labelled scale, identified by the host.
    Separate(u64),
}

/// The edge of a pane where price axes are displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AxisSide {
    /// Before the plot.
    Left,
    /// After the plot.
    Right,
}

/// Layout of a pane's axes, independent of their units, ranges and source assignments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PaneAxisOptions {
    /// Override the usual side of every labelled scale. `None` follows scale assignments
    /// and the chart's axis visibility settings, which can still hide the pane's own axis.
    pub side: Option<AxisSide>,
    /// Omit axes without visible sources, retaining their options for when a source returns.
    /// Enabled by default; a visible source waiting for data still owns its axis.
    pub hide_unused: bool,
}

impl Default for PaneAxisOptions {
    fn default() -> Self {
        Self {
            side: None,
            hide_unused: true,
        }
    }
}
