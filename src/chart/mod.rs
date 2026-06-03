//! Core chart engine for high-performance financial charting.
//!
//! This module is the heart of the `egui-charts` crate, providing a TradingView-like
//! charting engine built on [`egui`]. It is organized into several sub-modules that
//! separate concerns cleanly:
//!
//! # Architecture
//!
//! ```text
//!                        ┌─────────────┐
//!                        │  builder.rs  │  ChartBuilder / TradingChart
//!                        └──────┬───────┘
//!                               │ owns
//!                        ┌──────▼───────┐
//!                        │  widget::Chart│  Core chart widget (state + data)
//!                        └──────┬───────┘
//!          ┌────────┬───────────┼───────────┬──────────┐
//!          │        │           │           │          │
//!     ┌────▼───┐┌───▼────┐┌────▼───┐┌──────▼───┐┌────▼────────┐
//!     │ coords ││pan_zoom││interact││ rendering ││tool_interact│
//!     │        ││        ││        ││           ││             │
//!     │idx ↔ x ││scroll  ││keys   ││candles    ││drawings     │
//!     │price↔y ││zoom    ││track  ││grid/axes  ││selection    │
//!     └────────┘│kinetic ││focus  ││overlays   ││effects      │
//!               └────────┘└───────┘│pipeline   │└─────────────┘
//!                                  └───────────┘
//! ```
//!
//! # Public Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`builder`] | `ChartBuilder` (fluent API) and `TradingChart` (runtime wrapper) |
//! | [`coords`] | Unified coordinate system — single source of truth for idx/price ↔ screen |
//! | [`series`] | Series types (Line, Area, Baseline, Histogram, Bar) with hit testing and selection |
//! | [`series_api`] | TradingView-compatible `ISeriesApi` / `ITimeScaleApi` / `IStudyApi` traits |
//! | [`overlays`] | Visual overlays — order lines, heatmap strips, volume bubbles |
//!
//! # Internal Modules (crate-only)
//!
//! The remaining sub-modules (`state`, `interaction`, `pan_zoom`, `helpers`,
//! `hit_test`, `indicators`, `selection`, `cursor_modes`, `tool_interaction`,
//! `renderers`, `rendering`) are implementation details and not part of the
//! public API.
//!
//! # Getting Started
//!
//! The primary entry point for consumers is [`builder::ChartBuilder`]:
//!
//! ```rust,ignore
//! use egui_charts::ChartBuilder;
//!
//! let mut trading_chart = ChartBuilder::extended()
//!     .with_symbol("BTCUSDT")
//!     .with_timeframe(Timeframe::Min1)
//!     .with_data_src(Box::new(my_data_source))
//!     .build();
//!
//! // In your update loop:
//! trading_chart.update();
//! trading_chart.show(ui);
//! ```

pub mod builder;
pub mod coords;
pub mod overlays;
pub mod selection;
pub mod series;
pub mod series_api;

// Internal engine modules — used within the crate but not part of the public API.
pub(crate) mod cursor_modes;
pub(crate) mod helpers;
pub(crate) mod hit_test;
pub(crate) mod indicators;
pub(crate) mod interaction;
pub(crate) mod pan_zoom;
pub(crate) mod renderers;
pub(crate) mod rendering;
pub(crate) mod state;
pub(crate) mod tool_interaction;
