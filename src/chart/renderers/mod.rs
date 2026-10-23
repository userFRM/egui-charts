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
#[cfg(feature = "icons")]
pub(crate) use crosshair::snap_to_nearest_ohlc;
pub use indicator::IndicatorRenderer;
pub use labels::{
    Legend, LegendStatus, QuoteSide, render_price_labels, render_price_labels_at_pos,
    render_time_labels,
};
pub(crate) use labels::{price_decimals, render_time_grid};
pub use markers::{render_event_band, render_markers};
pub use session_breaks::{
    SessionBackgroundRenderer, SessionBreakRenderConfig, SessionBreakRenderer,
};
pub(crate) use session_breaks::{exchange_session, render_extended_hours};
pub use tooltip::render_tooltip_with_options;
pub use volume::render_volume_bar;
