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
