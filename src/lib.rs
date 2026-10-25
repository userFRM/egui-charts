//! # egui-charts
//!
//! Financial charting engine for
//! [egui](https://docs.rs/egui). Render interactive charts (candlesticks, OHLC
//! bars, line, area, Renko, Kagi, Point & Figure, Market Profile (TPO)) with
//! drawing tools, technical indicators and a design-token theme system. The
//! chart is an egui widget and runs in any egui host, native or WebAssembly.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! # #[cfg(feature = "widget")] {
//! use egui_charts::ChartBuilder;
//! use egui_charts::model::Timeframe;
//! use egui_charts::theme::Theme;
//!
//! // Build a chart with the fluent API
//! let chart = ChartBuilder::new()
//!     .with_symbol("BTCUSDT")
//!     .with_timeframe(Timeframe::Hour1)
//!     .with_theme(Theme::dark())
//!     .build();
//!
//! // In `eframe::App::ui`, with `ui: &mut egui::Ui`:
//! // chart.show(ui); // load from the feed, if any, and render the chart
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
//! | **Data** | [`data`] | `Datafeed` contract for history and live data, `Reply`, `Sink`, bar resampling |
//! | **Host services** | [`services`] | Traits a host implements to supply services such as storage, symbol search, signals, alerts and a trading account |
//! | **Chart engine** | [`chart`] | Rendering, hit-testing, coordinate mapping, overlays |
//! | **Drawing tools** | [`drawings`] | 98 drawing tool types counting cursor modes (trend lines, Fibonacci, patterns, etc.), undo/redo |
//! | **Indicators** | [`studies`] | `Indicator` trait, `IndicatorRegistry`, `CustomIndicator`; *(feature `studies`)* built-in studies and 162 indicator names in `IndicatorFactory` |
//! | **Scales** | [`scales`] | Price scales (normal, log, percentage), time scales, tick generators, formatters |
//! | **Configuration** | [`config`] | `ChartConfig`, `ChartOptions`, crosshair, tooltip, keyboard, kinetic scroll |
//! | **Theme system** | [`theme`], [`tokens`], [`styles`] | Design tokens as Rust constants, `Theme` with dark, light and high-contrast presets |
//! | **Widget** | [`widget`] | `Chart` egui widget and `ChartBuilder`: pan and zoom, kinetic scrolling, keyboard and drawing input, indicator panes, loading from a feed |
//! | **Extensions** | `ext` | *(feature `ui`)* `UiExt`, `ResponseExt`, `ButtonWidgetExt`, `TextEditWidgetExt`, `RoundingExt` |
//! | **Icons** | [`icons`] | Compile-time embedded SVG icons |
//! | **App UI** | `ui`, `ui_kit` | *(feature `ui`)* Toolbars, panels, dialogs, reusable form/button primitives |
//!
//! ## Feature flags
//!
//! The default build includes the chart widget, the icons, the built-in
//! studies and the application-level UI components. An application that only
//! shows charts uses `default-features = false, features = ["widget"]`. With
//! `default-features = false` alone the crate is the headless engine, which
//! does not depend on egui.
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `widget` | **on** | The egui chart widget and the modules built on it: `chart`, `drawings`, `config`, `theme`, `styles` and `widget`. Without it the crate is the headless engine: [`model`], [`scales`], [`data`], [`services`], [`tokens`] and the `Indicator` trait layer of [`studies`]. Required by `icons` and `ui`. |
//! | `studies` | **on** | The built-in studies and `IndicatorFactory`. Needs no egui, so a headless build can compute the same studies. `Indicator`, `IndicatorValue`, `IndicatorRegistry` and `CustomIndicator` are in every build. Required by `ui`. |
//! | `icons` | **on** | Compile-time embedded SVG icons, drawn through egui_extras' SVG image loader, which the chart installs in the egui context. Required by `ui`. |
//! | `ui` | **on** | Application-level UI: toolbars, panels, sidebars, dialogs, the reusable `ui_kit` widget library they are built on, and the `ext` helpers. Enables `icons` and `studies`. |
//! | `serde` | off | `Serialize` and `Deserialize` for `theme::Theme`. |
//!
//! ## Embedding in an application
//!
//! A typical setup:
//!
//! 1. Add the crate to your dependencies:
//!
//!    ```toml
//!    [dependencies]
//!    egui-charts = "0.2"
//!    ```
//!
//! 2. Implement [`data::Datafeed`] to bridge your backend (IPC, a WebSocket
//!    or a database) to the chart: answer `history` through its
//!    [`data::Reply`] and push live updates into the [`data::Sink`] that
//!    `subscribe` receives, from any thread. `examples/live_feed.rs` shows one.
//!
//! 3. Build the chart once with [`ChartBuilder`] and
//!    [`ChartBuilder::with_feed`], then call `chart.show(ui)` inside an
//!    `egui::CentralPanel` in `eframe::App::ui`.
//!
//! 4. Apply a theme at startup with [`theme::apply_to_egui`] to propagate
//!    design tokens into egui's visual system.
//!
//! ## Crate-level re-exports
//!
//! The most commonly used types are re-exported at the crate root for
//! convenience:
//!
//! - [`ChartBuilder`]: fluent chart construction
//! - [`Chart`]: the egui chart widget
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

// ─── Theme & design system ───────────────────────────────────────────────────

#[cfg(feature = "ui")]
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

pub use model::ChartType;
#[cfg(feature = "widget")]
pub use widget::{Chart, ChartBuilder, RightClickTarget};

// Compiles and runs the README's Rust snippets as doctests.
#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

#[cfg(all(test, feature = "ui"))]
mod tests {
    use crate::drawings::DrawingToolType;
    use crate::studies::IndicatorFactory;

    /// Every number followed, directly or after one or two words, by "drawing tool...",
    /// "indicator..." or "chart type..." (ignoring surrounding punctuation) in the README, the
    /// package description, the crate docs and the ROADMAP must match the code.
    #[test]
    fn stated_counts_match_code() {
        let docs = [
            include_str!("../README.md"),
            include_str!("../Cargo.toml"),
            include_str!("lib.rs"),
            include_str!("../ROADMAP.md"),
        ];
        let counts: [(&[&str], usize); 3] = [
            (&["drawing", "tool"], DrawingToolType::all().len()),
            (&["indicator"], IndicatorFactory::new().count()),
            (&["chart", "type"], crate::model::ChartType::all().len()),
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
