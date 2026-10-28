//! Order flow visualizations for the price pane.
//!
//! Use the footprint renderer with executed volume at price, [`LiquidityHeatmap`]
//! with depth history, or [`HeatmapStrip`] with per-bar delta and volume. The chart
//! also exposes volume bubble configuration through [`VolumeBubblesConfig`].

mod footprint;
pub use footprint::{
    FootprintColor, FootprintConfig, FootprintMode, render_footprint, render_footprint_statistics,
};
mod heatmap_strip;
mod liquidity_heatmap;
pub use liquidity_heatmap::LiquidityHeatmap;
mod volume_bubbles;

pub use heatmap_strip::{HeatmapCell, HeatmapStrip, HeatmapStripConfig};
pub(crate) use volume_bubbles::VolumeBubbles;
pub use volume_bubbles::{BubbleColor, BubbleGrouping, BubbleScalingMode, VolumeBubblesConfig};

#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use footprint::render_footprint_studies;

#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
mod trade_profile;
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use trade_profile::render_trade_profiles;
