//! Rendering and geometry helpers for chart composition.
//!
//! Use [`coords`] for coordinate conversions, [`series`] for hit testing and series
//! settings, [`overlays`] for order flow visualizations and [`primitives`] for custom
//! rendering. For a chart that manages data, layout and interaction, use
//! [`Chart`](crate::Chart).
//!
//! Internal painting lives in one `rendering` tree, grouped into axes, chart
//! types, overlays and supporting chart elements. Interaction state belongs to
//! the widget.

pub mod coords;
pub mod overlays;
pub mod primitives;
pub(crate) mod selection;
pub mod series;

// Internal engine modules: used within the crate but not part of the public API.
pub(crate) mod hit_test;
pub(crate) mod indicators;
pub(crate) mod rendering;

// Axis composition for hosts that allocate their own pane and input regions.
#[cfg(feature = "icons")]
#[cfg_attr(docsrs, doc(cfg(feature = "icons")))]
pub use rendering::axes::{axis_settings_button, crosshair_price_action};
pub use rendering::axes::{price_axis_width, render_quote_tags};
#[cfg(feature = "ui")]
pub(crate) use rendering::{
    COVERAGE_CELL, coverage_text, entry_price, exit_coverage, exit_request, grouped, held_exits,
    paint_coverage, pnl_text, quantity_decimals, quantity_text,
};
pub use rendering::{
    PnlUnit, RenderContext, TradingFormatter, TradingLabelAlignment, TradingLineAction,
    TradingLineAppearance, TradingLineOptions, TradingLines, render_alert_levels,
    render_axis_currency, render_crosshair_full, render_event_band, render_trading_lines,
};
