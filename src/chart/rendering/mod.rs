//! Modern rendering pipeline for chart components.
//!
//! Provides standalone rendering functions (`axes`, `candles`, `grid`, `overlays`)
//! that accept a context struct and paint directly to an egui `Painter`. These are
//! the workhorse implementations the `Chart` widget calls during a frame.

pub mod axes;
pub mod candles;
mod context;
pub use candles::{render_candle, render_ohlc_bar, render_volume_bar};
pub use context::{BarRenderParams, ChartMapping, RenderContext, StyleColors};
pub mod grid;
pub mod overlays;

// Re-export rendering functions
pub(crate) use crate::painting::dashed_line::{draw_dashed_line, draw_dashed_polyline};
pub use axes::render_price_line;
pub use candles::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, TradingColors, VolumeSettings,
    render_chart_type,
};
pub use grid::{render_grid, render_vertical_grid};
pub(crate) use overlays::{realtime_button, realtime_button_rect};
pub use overlays::{render_box_zoom, render_crosshair_with_options};

mod alert_levels;
mod indicator;
mod legend;
mod markers;
mod session_breaks;
mod trading_lines;
mod watermark;

pub use alert_levels::{alert_shortcut_price, render_alert_levels};

pub use indicator::IndicatorRenderer;
pub(crate) use indicator::paint_study_plot;
pub use legend::{Legend, LegendStatus, QuoteSide, StudyAction};
pub(crate) use legend::{SeriesAction, study_controls};
pub(crate) use markers::{render_change_mark, render_event_badges, render_event_breaks};
pub use markers::{render_event_band, render_markers};
pub use overlays::render_crosshair_full;
pub use overlays::{StudyValue, ValueFormatters, render_tooltip_with_options};
pub(crate) use overlays::{render_crosshair_lines, render_crosshair_price_tag, snap_to_values};
pub use session_breaks::{
    SessionBackgroundRenderer, SessionBreakRenderConfig, SessionBreakRenderer,
};
pub(crate) use session_breaks::{exchange_session, render_extended_hours};
#[cfg(feature = "ui")]
pub(crate) use trading_lines::entry_price;
pub use trading_lines::{
    PnlUnit, TradingLabelAlignment, TradingLineAction, TradingLineOptions, TradingLines,
    render_trading_lines,
};
pub(crate) use watermark::render_watermark;
pub use watermark::{Watermark, WatermarkLine};

pub(crate) use axes::{
    price_marks, render_study_axis, render_time_grid, study_mapping, time_axis_marks,
};
pub use axes::{
    render_axis_currency, render_price_labels, render_price_labels_at_pos, render_time_labels,
};
