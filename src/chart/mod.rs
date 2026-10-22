//! Core chart engine for financial charting.
//!
//! This module is the heart of the `egui-charts` crate, providing the interactive
//! financial charting engine built on [`egui`]. It is organized into several sub-modules that
//! separate concerns cleanly:
//!
//! # Architecture
//!
//! [`builder`] owns a [`Chart`](crate::widget::Chart), which keeps the chart state and data.
//! [`coords`] maps bar indices and prices to the screen, and the internal `rendering` and
//! `renderers` modules draw candles, lines, the grid, axes and overlays. Pan, zoom, keyboard
//! input and drawing input are in the [`widget`](crate::widget) module.
//!
//! # Public Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`builder`] | `ChartBuilder` (fluent API) and `TradingChart` (runtime wrapper) |
//! | [`coords`] | Unified coordinate system: the one mapping between bar index or price and the screen |
//! | [`series`] | Series hit testing, selection and display settings |
//! | [`overlays`] | Visual overlays: order lines, heatmap strips, volume bubbles |
//!
//! # Internal Modules (crate-only)
//!
//! The remaining sub-modules (`state`, `hit_test`, `indicators`,
//! `selection`, `cursor_modes`, `renderers`, `rendering`) are implementation
//! details and not part of the public API.
//!
//! # Getting Started
//!
//! The primary entry point for consumers is [`builder::ChartBuilder`]:
//!
//! ```rust
//! use egui_charts::ChartBuilder;
//! use egui_charts::model::Timeframe;
//!
//! # fn example(my_data_source: impl egui_charts::DataSource + 'static, ui: &mut egui::Ui) {
//! let mut trading_chart = ChartBuilder::extended()
//!     .with_symbol("BTCUSDT")
//!     .with_timeframe(Timeframe::Min1)
//!     .with_data_src(Box::new(my_data_source))
//!     .build();
//!
//! // In your update loop:
//! trading_chart.update();
//! trading_chart.show(ui);
//! # }
//! ```

pub mod builder;
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
