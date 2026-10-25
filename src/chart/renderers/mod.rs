//! Legacy chart rendering components.
//!
//! Provides standalone rendering functions for individual chart elements:
//! candlesticks, OHLC bars, crosshairs, price/time labels, markers,
//! session break lines, tooltips, and volume bars.
//!
//! These functions are called directly by the chart widget and the modern
//! rendering layer in [`rendering`](super::rendering).

mod alert_levels;
mod axis_currency;
mod bar;
mod candle;
mod context;
mod crosshair;
mod indicator;
mod study_scale;
pub(crate) use study_scale::{render_study_axis, study_mapping};
mod labels;
mod markers;
mod session_breaks;
mod tooltip;
mod trading_lines;
mod volume;
mod watermark;

pub use alert_levels::{alert_shortcut_price, render_alert_levels};
pub use axis_currency::render_axis_currency;
pub use bar::render_ohlc_bar;
pub use candle::render_candle;
pub use context::{BarRenderParams, ChartMapping, RenderContext, StyleColors};

pub use crosshair::{render_crosshair_full, render_crosshair_with_options};
pub(crate) use crosshair::{render_crosshair_lines, render_crosshair_price_tag, snap_to_values};
pub use indicator::IndicatorRenderer;
pub(crate) use indicator::paint_study_plot;
pub use labels::{
    Legend, LegendStatus, QuoteSide, StudyAction, render_price_labels, render_price_labels_at_pos,
    render_time_labels,
};
pub(crate) use labels::{
    price_decimals, price_marks, render_time_grid, study_controls, time_axis_marks,
};
pub(crate) use markers::render_change_mark;
pub use markers::{render_event_band, render_markers};
pub use session_breaks::{
    SessionBackgroundRenderer, SessionBreakRenderConfig, SessionBreakRenderer,
};
pub(crate) use session_breaks::{exchange_session, render_extended_hours};
pub use tooltip::{StudyValue, render_tooltip_with_options};
pub use trading_lines::{
    PnlUnit, TradingLineAction, TradingLineOptions, TradingLines, render_trading_lines,
};
#[cfg(feature = "ui")]
pub(crate) use trading_lines::{decimals, to_step};
pub use volume::render_volume_bar;
pub(crate) use watermark::render_watermark;
pub use watermark::{Watermark, WatermarkLine};
