//! Chart Overlays
//!
//! Overlay visualizations that sit on top of the main chart, including:
//! - Heatmap strips (volume flow visualization)
//!
//! # Example
//!
//! ```
//! use egui_charts::chart::overlays::HeatmapStrip;
//!
//! # fn example(ui: &mut egui::Ui, strip_rect: egui::Rect, deltas: &[f64], volumes: &[f64], visible_range: (usize, usize), bar_width: f32) {
//! // Create heatmap strip
//! let mut heatmap = HeatmapStrip::new();
//! heatmap.set_data(deltas.iter().enumerate().map(|(i, d)| (i, *d, volumes[i])));
//! heatmap.render(ui, strip_rect, visible_range, bar_width);
//! # }
//! ```

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
pub use volume_bubbles::{BubbleColor, BubbleScalingMode, VolumeBubblesConfig};

#[cfg(feature = "studies")]
pub use footprint::render_footprint_studies;

#[cfg(feature = "studies")]
mod trade_profile;
#[cfg(feature = "studies")]
pub use trade_profile::render_trade_profiles;
