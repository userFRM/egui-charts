//! # egui-charts
//!
//! Financial charting engine for
//! [egui](https://docs.rs/egui). Render interactive charts (candlesticks, OHLC
//! bars, line, area, Renko, Kagi, Point & Figure, Market Profile (TPO)) with
//! drawing tools, technical indicators and a design-token theme system.
//! Built to be embedded in [Tauri](https://tauri.app) desktop apps or any egui
//! host.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! # #[cfg(feature = "widget")] {
//! use egui_charts::{ChartBuilder, Chart};
//! use egui_charts::model::{Bar, BarData, Timeframe};
//! use egui_charts::theme::Theme;
//!
//! // Build a chart with the fluent API
//! let mut trading_chart = ChartBuilder::new()
//!     .with_symbol("BTCUSDT")
//!     .with_timeframe(Timeframe::Hour1)
//!     .with_theme(Theme::dark())
//!     .with_drawing_tools()
//!     .build();
//!
//! // In your egui update loop:
//! // trading_chart.update();           // poll data source, progressive loading
//! // trading_chart.show(&mut ui);      // render into an egui::Ui
//! # }
//! ```
//!
//! For a minimal price ticker (no grid, no crosshair, no tools):
//!
//! ```rust,no_run
//! # #[cfg(feature = "widget")] {
//! # use egui_charts::ChartBuilder;
//! let mini = ChartBuilder::price_chart()
//!     .with_symbol("ETHUSDT")
//!     .with_visible_candles(30)
//!     .build();
//! # }
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
//! | **Host services** | [`services`] | Traits a host implements to supply services such as storage, symbol search, signals and a trading account |
//! | **Chart engine** | [`chart`] | Pan/zoom, hit-testing, coordinate mapping, series rendering, interaction |
//! | **Drawing tools** | [`drawings`] | 95 drawing tool types counting cursor modes (trend lines, Fibonacci, patterns, etc.), undo/redo |
//! | **Indicators** | [`studies`] | 128 indicator names in `IndicatorFactory`, `Indicator` trait, `IndicatorRegistry` |
//! | **Scales** | [`scales`] | Price scales (normal, log, percentage), time scales, tick generators, formatters |
//! | **Configuration** | [`config`] | `ChartConfig`, `ChartOptions`, crosshair, tooltip, keyboard, kinetic scroll |
//! | **Validation** | [`validation`] | OHLC integrity checks, timestamp ordering, data quality |
//! | **Theme system** | [`theme`], [`tokens`], [`styles`] | Design tokens as Rust constants, `Theme` with dark, light and high-contrast presets |
//! | **Widget** | [`widget`] | `Chart` egui widget, `ChartBuilder`, `TradingChart` |
//! | **Extensions** | [`ext`] | `UiExt`, `ResponseExt`, `ButtonWidgetExt`, `TextEditWidgetExt` |
//! | **Icons** | [`icons`] | Compile-time embedded SVG icons |
//! | **App UI** | `ui`, `ui_kit` | *(feature `ui`)* Toolbars, panels, dialogs, reusable form/button primitives |
//!
//! ## Feature flags
//!
//! The default build includes the core engine, theme system, chart widget, and
//! compile-time icons. Application-level UI is opt-in. With
//! `default-features = false` the crate is the headless engine, which does not
//! depend on egui.
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `widget` | **on** | The egui chart widget and the modules built on it: `chart`, `drawings`, `config`, `theme`, `styles`, `ext` and `widget`. Without it the crate is the headless engine: [`model`], [`scales`], [`data`], [`services`], [`studies`], [`tokens`], [`validation`]. Required by `icons` and `ui`. |
//! | `icons` | **on** | Compile-time embedded SVG icons, drawn through egui_extras' SVG image loader, which the chart installs in the egui context. Required by `ui`. |
//! | `ui` | off | Application-level UI: toolbars, panels, sidebars, dialogs, and the reusable `ui_kit` widget library they are built on. Enable this when you are building a full trading-terminal interface around the chart engine. |
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
//!    egui-charts = { version = "0.2", features = ["ui"] }
//!    ```
//!
//! 2. Implement [`data::DataSource`] to bridge your Tauri backend (IPC /
//!    WebSocket) to the chart's data layer.
//!
//! 3. Build the chart once with [`chart::builder::ChartBuilder`], then call
//!    `trading_chart.update()` and `trading_chart.show(ui)` inside an
//!    `egui::CentralPanel` in `eframe::App::update`.
//!
//! 4. Apply a theme at startup with [`theme::apply_to_egui`] to propagate
//!    design tokens into egui's visual system.
//!
//! ## Crate-level re-exports
//!
//! The most commonly used types are re-exported at the crate root for
//! convenience:
//!
//! - [`ChartBuilder`] / [`TradingChart`]: fluent chart construction
//! - [`Chart`]: the low-level egui widget
//! - [`DataSource`]: data provider trait
//! - [`ChartType`]: candlestick, line, area, bar, etc.

// ─── Crate-level lint configuration ──────────────────────────────────────────
#![allow(clippy::too_many_arguments)]
#![allow(clippy::module_inception)]
#![allow(clippy::needless_range_loop)]

// Engine code names colours as `egui::Color32`; without the widget that path
// resolves to `ecolor`, the crate egui re-exports them from.
#[cfg(not(feature = "widget"))]
extern crate ecolor as egui;

// ─── Core engine modules ─────────────────────────────────────────────────────

#[cfg(feature = "widget")]
pub mod chart;
#[cfg(feature = "widget")]
pub mod drawings;
pub mod model;
pub mod scales;
pub mod studies;

// ─── Configuration & data ────────────────────────────────────────────────────

#[cfg(feature = "widget")]
pub mod config;
pub mod data;
pub mod services;
pub mod validation;

// ─── Theme & design system ───────────────────────────────────────────────────

#[cfg(feature = "widget")]
pub mod ext;
#[cfg(feature = "icons")]
pub mod icons;
#[cfg(feature = "widget")]
pub mod styles;
#[cfg(feature = "widget")]
pub mod theme;
pub mod tokens;

// ─── Widget layer ────────────────────────────────────────────────────────────

#[cfg(feature = "widget")]
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

// ─── Convenience re-exports ──────────────────────────────────────────────────

#[cfg(feature = "widget")]
pub use chart::builder::{ChartBuilder, TradingChart};
pub use data::DataSource;
pub use model::ChartType;
#[cfg(feature = "widget")]
pub use widget::{Chart, RightClickTarget};

// Compiles and runs the README's Rust snippets as doctests.
#[cfg(all(doctest, feature = "widget"))]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

#[cfg(all(test, feature = "widget"))]
mod tests {
    use crate::drawings::DrawingToolType;
    use crate::studies::IndicatorFactory;

    /// Every number followed, directly or after one or two words, by "drawing tool..." or
    /// "indicator..." (ignoring surrounding punctuation) in the README, the package description
    /// and the crate docs must match the code.
    #[test]
    fn stated_counts_match_code() {
        let docs = [
            include_str!("../README.md"),
            include_str!("../Cargo.toml"),
            include_str!("lib.rs"),
        ];
        let counts: [(&[&str], usize); 2] = [
            (&["drawing", "tool"], DrawingToolType::all().len()),
            (&["indicator"], IndicatorFactory::new().count()),
        ];
        for (noun, count) in counts {
            let mut stated = 0;
            for doc in docs {
                let words: Vec<&str> = doc
                    .split(|c: char| c.is_whitespace() || c == '>')
                    .filter(|w| !w.is_empty())
                    .collect();
                fn bare(w: &str) -> &str {
                    w.trim_start_matches(['*', '_', '(', '['])
                }
                for (i, word) in words.iter().enumerate() {
                    // Markdown emphasis and brackets around a number are not part of it,
                    // but a trailing "+" is, so "130+" still fails.
                    let number =
                        bare(word).trim_end_matches(['*', '_', ',', '.', ':', ';', ')', ']']);
                    if !number.starts_with(|c: char| c.is_ascii_digit())
                        || !number.chars().all(|c| c.is_ascii_digit() || c == '+')
                    {
                        continue;
                    }
                    let names_noun = (i + 1..=i + 3).any(|k| {
                        noun.iter()
                            .enumerate()
                            .all(|(j, n)| words.get(k + j).is_some_and(|w| bare(w).starts_with(n)))
                    });
                    if names_noun {
                        assert_eq!(number, count.to_string(), "stated count of {noun:?}");
                        stated += 1;
                    }
                }
            }
            assert!(stated > 0, "no stated count of {noun:?} found");
        }
    }
}
