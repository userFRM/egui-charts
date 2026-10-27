//! Rendering and geometry helpers for chart composition.
//!
//! Use [`coords`] for coordinate conversions, [`series`] for hit testing and series
//! settings, [`overlays`] for order flow visualizations and [`primitives`] for custom
//! rendering. For a chart that manages data, layout and interaction, use
//! [`Chart`](crate::Chart).

pub mod coords;
pub mod overlays;
pub mod primitives;
pub(crate) mod selection;
pub mod series;

// Internal engine modules: used within the crate but not part of the public API.
#[cfg(feature = "drawings")]
pub(crate) mod cursor_modes;
pub(crate) mod hit_test;
pub(crate) mod indicators;
pub(crate) mod renderers;
pub(crate) mod rendering;
pub(crate) mod state;

// Axis composition for hosts that allocate their own pane and input regions.
pub use renderers::{
    PnlUnit, RenderContext, TradingLabelAlignment, TradingLineAction, TradingLineOptions,
    TradingLines, alert_shortcut_price, render_alert_levels, render_axis_currency,
    render_crosshair_full, render_event_band, render_trading_lines,
};
#[cfg(feature = "ui")]
pub(crate) use renderers::{decimals, to_step};
#[cfg(feature = "icons")]
#[cfg_attr(docsrs, doc(cfg(feature = "icons")))]
pub use rendering::axes::{axis_settings_button, crosshair_price_action};
pub use rendering::axes::{price_axis_width, render_quote_tags};
