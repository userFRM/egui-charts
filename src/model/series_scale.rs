//! Price scale membership for series in one chart pane.

/// Series with the same assignment share a range and scale options. Relative modes
/// retain a separate first-visible-close reference for each series.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
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

/// The edge of a pane where price axes are displayed. The set is closed: a pane has no other edge
/// for price axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AxisSide {
    /// Before the plot.
    Left,
    /// After the plot.
    Right,
}

/// Layout of a pane's axes, independent of their units, ranges and source assignments.
///
/// ```
/// use egui_charts::model::PaneAxisOptions;
///
/// let mut options = PaneAxisOptions::default();
/// options.hide_unused = true;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
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
