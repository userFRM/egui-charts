//! # egui-charts
//!
//! High-performance financial charting engine for
//! [egui](https://docs.rs/egui). Render interactive, TradingView-quality
//! charts — candlesticks, OHLC bars, line, area, Renko, Kagi, Point & Figure,
//! Market Profile (TPO) — with 95 drawing tools, 130+ technical indicators,
//! and a full design-token theme system. Built to be embedded in
//! [Tauri](https://tauri.app) desktop apps or any egui host.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use egui_charts::{ChartBuilder, Chart};
//! use egui_charts::model::{Bar, BarData, Timeframe};
//! use egui_charts::theme::Theme;
//!
//! // Build a chart with the fluent API
//! let mut trading_chart = ChartBuilder::new()
//!     .with_symbol("BTCUSDT")
//!     .with_timeframe(Timeframe::H1)
//!     .with_theme(Theme::dark())
//!     .with_drawing_tools()
//!     .build();
//!
//! // In your egui update loop:
//! // trading_chart.update();           // poll data source, progressive loading
//! // trading_chart.show(&mut ui);      // render into an egui::Ui
//! ```
//!
//! For a minimal price ticker (no grid, no crosshair, no tools):
//!
//! ```rust,no_run
//! # use egui_charts::ChartBuilder;
//! let mini = ChartBuilder::price_chart()
//!     .with_symbol("ETHUSDT")
//!     .with_visible_candles(30)
//!     .build();
//! ```
//!
//! ## Architecture
//!
//! The crate is organized into a layered set of modules:
//!
//! | Layer | Modules | Purpose |
//! |---|---|---|
//! | **Domain model** | [`model`] | `Bar`, `BarData`, `Symbol`, `Timeframe`, `ChartType`, Renko/Kagi/P&F transforms |
//! | **Data** | [`data`] | `DataSource` trait, `DataUpdate`, historical/live data abstractions |
//! | **Chart engine** | [`chart`] | Pan/zoom, hit-testing, coordinate mapping, series rendering, interaction |
//! | **Drawing tools** | [`drawings`] | 95 tools (trend lines, Fibonacci, patterns, etc.), undo/redo, snapping |
//! | **Indicators** | [`studies`] | 130+ built-in indicators, `Indicator` trait, `IndicatorRegistry` |
//! | **Scales** | [`scales`] | Price scales (normal, log, percentage), time scales, tick generators, formatters |
//! | **Configuration** | [`config`] | `ChartConfig`, `ChartOptions`, crosshair, tooltip, keyboard, kinetic scroll |
//! | **Validation** | [`validation`] | OHLC integrity checks, timestamp ordering, data quality |
//! | **Theme system** | [`theme`], [`tokens`], [`styles`], [`theming`] | Design tokens (RON), semantic colors, presets (Classic, Dark, Light, Midnight, High Contrast) |
//! | **Widget** | [`widget`] | `Chart` egui widget, `ChartBuilder`, `TradingChart` |
//! | **Extensions** | [`ext`] | `UiExt`, `ContextExt`, `ResponseExt`, `HasDesignTokens` |
//! | **Icons** | [`icons`] | 280+ compile-time embedded SVG icons |
//! | **App UI** | `ui`, `ui_kit`, `templates` | *(feature `ui`)* Toolbars, panels, dialogs, reusable form/button primitives |
//! | **Backtest** | `backtest` | *(feature `backtest`)* Strategy backtesting on historical data |
//! | **Scripting** | `scripting` | *(feature `scripting`)* User-defined indicators and strategies |
//!
//! ## Feature flags
//!
//! The default build includes the core engine, theme system, chart widget, and
//! compile-time icons. Application-level UI is opt-in.
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `icons` | **on** | 280+ compile-time embedded SVG icons. Required by `ui`. |
//! | `ui` | off | Application-level UI: toolbars, panels, sidebars, dialogs, and the reusable `ui_kit` widget library they are built on. Enable this when you are building a full trading-terminal interface around the chart engine. |
//! | `backtest` | off | Backtesting framework for strategy evaluation on historical data. |
//! | `scripting` | off | Embedded scripting support for user-defined indicators and strategies. |
//!
//! ## Integrating with Tauri
//!
//! `egui-charts` is designed to be used as the rendering engine inside a Tauri
//! desktop application. A typical setup:
//!
//! 1. Add the crate to your Tauri frontend's dependencies:
//!
//!    ```toml
//!    [dependencies]
//!    egui-charts = { version = "0.1", features = ["ui"] }
//!    ```
//!
//! 2. Implement [`data::DataSource`] to bridge your Tauri backend (IPC /
//!    WebSocket) to the chart's data layer.
//!
//! 3. Create a [`chart::builder::ChartBuilder`] in your `eframe::App::update`
//!    and call `trading_chart.show(ui)` inside an `egui::CentralPanel`.
//!
//! 4. Apply a theme at startup with [`theme::apply_to_egui`] to propagate
//!    design tokens into egui's visual system.
//!
//! ## Crate-level re-exports
//!
//! The most commonly used types are re-exported at the crate root for
//! convenience:
//!
//! - [`ChartBuilder`] / [`TradingChart`] — fluent chart construction
//! - [`Chart`] — the low-level egui widget
//! - [`DataSource`] — data provider trait
//! - [`ChartType`] — candlestick, line, area, bar, etc.

// ─── Crate-level lint configuration ──────────────────────────────────────────
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::module_inception)]
#![allow(clippy::needless_range_loop)]

// ─── Core engine modules ─────────────────────────────────────────────────────

pub mod chart;
pub mod drawings;
pub mod model;
pub mod scales;
pub mod studies;

// ─── Configuration & data ────────────────────────────────────────────────────

pub mod config;
pub mod data;
pub mod validation;

// ─── Theme & design system ───────────────────────────────────────────────────

pub mod ext;
#[cfg(feature = "icons")]
pub mod icons;
pub mod styles;
pub mod theme;
pub mod theming;
pub mod tokens;

// ─── Widget layer ────────────────────────────────────────────────────────────

pub mod widget;

// ─── Application-level UI (feature-gated) ────────────────────────────────────

/// Reusable UI components (buttons, dialogs, forms, color pickers, etc.).
///
/// These are domain-agnostic building blocks consumed by the [`ui`] module.
/// Enable the `ui` feature to access them.
#[cfg(feature = "ui")]
pub mod ui_kit;

/// Application-level chart UI: drawing toolbar, top toolbar, timeframe bar,
/// replay controls, dialogs, and widget panels.
///
/// Built on top of [`ui_kit`] primitives and the core chart engine. Enable the
/// `ui` feature to access this module.
#[cfg(feature = "ui")]
pub mod ui;

/// Chart settings template management.
///
/// Provides `SettingsTemplate` and
/// `TemplateManager` for persisting and
/// restoring chart configurations. Requires the `ui` feature because it
/// re-exports types from [`ui::stubs`].
#[cfg(feature = "ui")]
pub mod templates;

// ─── Optional domain modules (feature-gated) ─────────────────────────────────

#[cfg(feature = "backtest")]
pub mod backtest;
#[cfg(feature = "scripting")]
pub mod scripting;

// ─── Convenience re-exports ──────────────────────────────────────────────────

pub use chart::builder::{ChartBuilder, TradingChart};
pub use data::DataSource;
pub use model::ChartType;
pub use widget::Chart;
