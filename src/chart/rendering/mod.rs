//! Chart painting grouped by concept.
//!
//! [`axes`] renders price and time labels, currency controls and study scales.
//! [`candles`] renders every chart type and its per-bar primitives. [`overlays`]
//! renders crosshairs, tooltips and the realtime button; [`grid`] paints
//! the grid. Supporting chart elements include legends, markers, session breaks,
//! trading lines, alerts, watermarks and study plots.
//!
//! The shared [`RenderContext`] and [`ChartMapping`] carry painting and coordinate
//! data. The chart widget composes these functions during a frame. Profile geometry
//! lives here, while shared painting lives in `crate::painting`.

pub mod axes;
pub mod candles;
mod context;
pub mod grid;
pub mod overlays;

mod alert_levels;
mod indicator;
mod legend;
mod markers;
mod session_breaks;
mod trading_lines;
pub(in crate::chart) mod volume_profile;
mod watermark;

// Re-export rendering functions
pub(crate) use crate::painting::dashed_line::{draw_dashed_line, draw_dashed_polyline};
pub use alert_levels::{alert_shortcut_price, render_alert_levels};
pub(crate) use axes::{
    fit_study_mapping, price_marks, render_study_axis, render_time_grid, study_mapping,
    time_axis_marks,
};
pub use axes::{
    render_axis_currency, render_price_labels, render_price_labels_at_pos, render_price_line,
    render_time_labels,
};
pub use candles::{
    BarDimensions, CandleDataContext, ChartTypeParams, CoordMapping, TradingColors, VolumeSettings,
    render_candle, render_chart_type, render_ohlc_bar, render_volume_bar,
};
pub use context::{BarRenderParams, ChartMapping, RenderContext, StyleColors};
pub use grid::{render_grid, render_vertical_grid};
pub use indicator::IndicatorRenderer;
pub(crate) use indicator::paint_study_plot;
pub use legend::{Legend, LegendStatus, QuoteSide, StudyAction};
pub(crate) use legend::{SeriesAction, controls_width, study_controls};
pub(crate) use markers::{
    marker_bounds, render_change_mark, render_event_badges, render_event_breaks,
};
pub use markers::{render_event_band, render_markers};
pub use overlays::{
    StudyValue, ValueFormatters, render_crosshair_full, render_crosshair_with_options,
    render_tooltip_with_options,
};
pub(crate) use overlays::{
    realtime_button, realtime_button_rect, render_crosshair_at_time, render_crosshair_lines,
    render_crosshair_price_tag, snap_to_values,
};
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
