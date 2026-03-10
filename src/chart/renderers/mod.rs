//! Legacy chart rendering components.
//!
//! Provides standalone rendering functions for individual chart elements:
//! candlesticks, OHLC bars, crosshairs, price/time grids, labels, markers,
//! session break lines, tooltips, and volume bars.
//!
//! These functions are consumed by the modern rendering pipeline in
//! [`rendering::pipeline`](super::rendering::pipeline) via adapter structs,
//! and can also be called directly for custom rendering scenarios.

mod bar;
mod candle;
mod context;
mod crosshair;
mod grid;
mod indicator;
mod labels;
mod markers;
mod session_breaks;
mod tooltip;
mod volume;

pub use bar::render_ohlc_bar;
pub use candle::render_candle;
pub use context::{BarRenderParams, ChartMapping, PriceScale, RenderContext, StyleColors};
pub use crosshair::{render_crosshair, render_crosshair_full, render_crosshair_with_mode};
pub use indicator::IndicatorRenderer;
pub use labels::{render_legend, render_ohlc_info, render_price_labels, render_time_labels};
pub use markers::render_markers;
pub use volume::render_volume_bar;
