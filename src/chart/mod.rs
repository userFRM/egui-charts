//! Core chart engine for financial charting.
//!
//! This module is the heart of the `egui-charts` crate, providing the interactive
//! financial charting engine built on [`egui`]. It is organized into several sub-modules that
//! separate concerns cleanly:
//!
//! # Architecture
//!
//! [`Chart`](crate::widget::Chart) keeps the chart state and data.
//! [`coords`] maps bar indices and prices to the screen, and the internal `rendering` and
//! `renderers` modules draw candles, lines, the grid, axes and overlays. Pan, zoom, keyboard
//! input and drawing input are in the [`widget`](crate::widget) module.
//!
//! # Public Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`coords`] | Unified coordinate system: the one mapping between bar index or price and the screen |
//! | [`series`] | Series hit testing, selection and display settings |
//! | [`overlays`] | Visual overlays: heatmap strips, volume bubbles |
//!
//! # Internal Modules (crate-only)
//!
//! The remaining sub-modules (`state`, `hit_test`, `indicators`,
//! `selection`, `cursor_modes`, `renderers`, `rendering`) are implementation
//! details and not part of the public API.

pub mod coords;
pub mod overlays;
pub mod selection;
pub mod series;

// Internal engine modules: used within the crate but not part of the public API.
pub(crate) mod cursor_modes;
pub(crate) mod hit_test;
pub(crate) mod indicators;
pub(crate) mod renderers;
pub(crate) mod rendering;
pub(crate) mod state;

// Axis composition for hosts that allocate their own pane and input regions.
pub use renderers::{
    RenderContext, TradingLineAction, TradingLineOptions, TradingLines, alert_shortcut_price,
    render_alert_levels, render_axis_currency, render_crosshair_full, render_event_band,
    render_trading_lines,
};
#[cfg(feature = "icons")]
pub use rendering::axes::{axis_settings_button, crosshair_price_action};
pub use rendering::axes::{price_axis_width, render_quote_tags};
