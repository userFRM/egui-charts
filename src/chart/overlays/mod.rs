//! Order flow visualizations for the price pane.
//!
//! Use the footprint renderer with executed volume at price, [`LiquidityHeatmap`]
//! with depth history, or [`HeatmapStrip`] with per-bar delta and volume. The chart
//! also exposes volume bubble configuration through [`VolumeBubblesConfig`].

mod book_profile;
pub use book_profile::{BookProfile, OrderBookView, render_book_profile};
mod footprint;
#[cfg(feature = "studies")]
pub(crate) use footprint::render_footprint_annotations;
#[cfg(feature = "ui")]
pub(crate) use footprint::step_color;
pub use footprint::{
    FootprintColor, FootprintConfig, FootprintIntensity, FootprintMode, pullback_range,
    render_footprint, render_footprint_statistics, render_footprint_statistics_rows,
};
pub(crate) use footprint::{
    OPEN_INTEREST_ROWS, automatic_row_height, fixed_row_group, footprint_column_width,
    open_interest_request, render_footprint_read, render_footprint_statistics_rows_read, row_edges,
    row_group, summary_room,
};
mod heatmap_strip;
mod liquidity_heatmap;
pub use liquidity_heatmap::{LiquidityHeatmap, LiquidityHeatmapConfig};
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
#[cfg(feature = "ui")]
pub(crate) use trade_profile::SessionColors;
#[cfg(feature = "studies")]
pub(crate) use trade_profile::render_session_volume_profiles;
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use trade_profile::{TradeProfileSettings, render_trade_profiles};
