//! Legacy chart rendering components.
//!
//! Provides standalone rendering functions for individual chart elements:
//! candlesticks, OHLC bars, crosshairs, price/time labels, markers,
//! session break lines, tooltips, and volume bars.
//!
//! These functions are called directly by the chart widget and the modern
//! rendering layer in [`rendering`](super::rendering).

mod alert_levels;
mod crosshair;
mod indicator;
mod legend;
mod markers;
mod session_breaks;
mod tooltip;
mod trading_lines;
pub(crate) mod volume_profile;
mod watermark;

pub use super::rendering::{ChartMapping, RenderContext, StyleColors};
pub use alert_levels::{alert_shortcut_price, render_alert_levels};

pub use crosshair::{render_crosshair_full, render_crosshair_with_options};
pub(crate) use crosshair::{render_crosshair_lines, render_crosshair_price_tag, snap_to_values};
pub use indicator::IndicatorRenderer;
pub(crate) use indicator::paint_study_plot;
pub use legend::{Legend, LegendStatus, QuoteSide, StudyAction};
pub(crate) use legend::{SeriesAction, study_controls};
pub(crate) use markers::{render_change_mark, render_event_badges, render_event_breaks};
pub use markers::{render_event_band, render_markers};
pub use session_breaks::{
    SessionBackgroundRenderer, SessionBreakRenderConfig, SessionBreakRenderer,
};
pub(crate) use session_breaks::{exchange_session, render_extended_hours};
pub use tooltip::{StudyValue, ValueFormatters, render_tooltip_with_options};
#[cfg(feature = "ui")]
pub(crate) use trading_lines::entry_price;
pub use trading_lines::{
    PnlUnit, TradingLabelAlignment, TradingLineAction, TradingLineOptions, TradingLines,
    render_trading_lines,
};
pub(crate) use watermark::render_watermark;
pub use watermark::{Watermark, WatermarkLine};

pub(crate) use super::rendering::axes::{
    price_marks, render_study_axis, render_time_grid, study_mapping, time_axis_marks,
};
pub use super::rendering::axes::{
    render_axis_currency, render_price_labels, render_price_labels_at_pos, render_time_labels,
};
