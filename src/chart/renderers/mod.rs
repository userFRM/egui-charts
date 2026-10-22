//! Legacy chart rendering components.
//!
//! Provides standalone rendering functions for individual chart elements:
//! candlesticks, OHLC bars, crosshairs, price/time labels, markers,
//! session break lines, tooltips, and volume bars.
//!
//! These functions are called directly by the chart widget and the modern
//! rendering layer in [`rendering`](super::rendering).

mod bar;
mod candle;
mod context;
mod crosshair;
mod indicator;
mod labels;
mod markers;
mod session_breaks;
mod tooltip;
mod volume;

pub use bar::render_ohlc_bar;
pub use candle::render_candle;
pub use context::{BarRenderParams, ChartMapping, RenderContext, StyleColors};

pub use crosshair::render_crosshair_full;
pub use indicator::IndicatorRenderer;
pub use labels::{render_legend, render_ohlc_info, render_price_labels, render_time_labels};
pub use markers::render_markers;
pub use session_breaks::{
    SessionBackgroundRenderer, SessionBreakRenderConfig, SessionBreakRenderer,
    provider_for_timeframe,
};
pub use tooltip::render_tooltip_with_options;
pub use volume::render_volume_bar;
