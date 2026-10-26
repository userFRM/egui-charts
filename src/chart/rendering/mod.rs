//! Modern rendering pipeline for chart components.
//!
//! Provides standalone rendering functions (`axes`, `candles`, `grid`, `overlays`)
//! that accept a context struct and paint directly to an egui `Painter`. These are
//! the workhorse implementations the `Chart` widget calls during a frame.

pub mod axes;
pub mod candles;
mod dashed_line;
pub mod grid;
pub mod overlays;

// Re-export rendering functions
pub use super::renderers::{render_price_labels, render_time_labels, render_tooltip_with_options};
pub use axes::render_price_line;
pub use candles::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, TradingColors, VolumeSettings,
    render_chart_type,
};
#[cfg(feature = "drawings")]
pub(crate) use dashed_line::finite_clip;
pub(crate) use dashed_line::{draw_dashed_line, draw_dashed_polyline};
pub use grid::{render_grid, render_vertical_grid};
pub use overlays::{render_box_zoom, render_crosshair_with_options};

pub(crate) use overlays::{realtime_button, realtime_button_rect};
