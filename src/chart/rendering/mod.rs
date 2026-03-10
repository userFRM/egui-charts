//! Modern rendering pipeline for chart components.
//!
//! This module contains two complementary rendering approaches:
//!
//! 1. **Functions** (`axes`, `candles`, `grid`, `overlays`) -- standalone rendering
//!    functions that accept a context struct and paint directly to an egui `Painter`.
//!    These are the workhorse implementations.
//!
//! 2. **Pipeline** (`pipeline`) -- a trait-based [`ChartRenderer`] architecture with
//!    [`RenderLayer`] ordering and adapter structs (e.g., `CandleRenderer`,
//!    `GridRenderer`) that delegate to the functions above. This enables composable,
//!    reorderable render passes.
//!
//! Most users interact with this module indirectly through the `Chart` widget, which
//! assembles the pipeline internally.

pub mod axes;
pub mod candles;
pub mod grid;
pub mod overlays;
pub mod pipeline;

// Re-export rendering functions
pub use super::renderers::{render_price_labels, render_time_labels};
pub use axes::render_last_price_line;
pub use candles::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, JapaneseChartSettings,
    TradingColors, VolumeSettings, render_chart_type,
};
pub use grid::{render_grid, render_vertical_grid};
pub use overlays::{
    render_box_zoom, render_crosshair_with_options, render_legend, render_ohlc_info,
    render_realtime_btn,
};

// Re-export pipeline types
