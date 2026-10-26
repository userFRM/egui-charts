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
//! | **Domain model** | [`model`] | `Bar`, `BarData`, `Instrument`, `Timeframe`, `ChartType`, Renko/Kagi/P&F transforms |
//! | **Data** | [`data`] | `Datafeed` contract for history and live data, `Reply`, `Sink`, bar resampling |
//! | **Host services** | [`services`] | Traits a host implements to supply services such as storage, symbol search, signals, alerts and a trading account |
//! | **Chart engine** | [`chart`] | Rendering, hit-testing, coordinate mapping, overlays |
//! | **Drawing tools** | `drawings` | *(feature `drawings`)* 98 drawing tool types counting cursor modes (trend lines, Fibonacci, patterns, etc.), undo/redo |
//! | **Indicators** | [`studies`] | `Indicator` trait, `IndicatorRegistry`, `CustomIndicator`; *(feature `studies`)* built-in studies and 177 indicator names in `IndicatorFactory` with default features; automatic drawing studies also require `drawings` |
//! | **Scales** | [`scales`] | Price scales (normal, log, percentage), time scales, tick generators, formatters |
//! | **Configuration** | [`config`] | `ChartConfig`, `ChartOptions`, crosshair, tooltip, keyboard, kinetic scroll |
//! | **Theme system** | [`theme`], [`tokens`], [`styles`] | Design tokens as Rust constants, `Theme` with dark, light and high-contrast presets |
//! | **Widget** | [`widget`] | `Chart` egui widget and `ChartBuilder`: pan and zoom, kinetic scrolling, keyboard and drawing input, indicator panes, loading from a feed |
//! | **Extensions** | `ui_kit::ext` | *(feature `ui`)* `UiExt`, `ResponseExt`, `ButtonWidgetExt`, `TextEditWidgetExt`, `RoundingExt` |
//! | **Icons** | `icons` | Compile-time embedded SVG icons |
//! | **App UI** | `ui`, `ui_kit` | *(feature `ui`)* Toolbars, panels, dialogs, reusable form/button primitives |
//!
//! ## Feature flags
//!
//! The default build includes the chart widget, the icons, the built-in
//! studies, the application-level UI components and the localized month and
//! weekday names and IANA time zones. An application that only shows charts uses
//! `default-features = false, features = ["widget"]`. With
//! `default-features = false` alone the crate is the headless engine, which
//! does not depend on egui.
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `widget` | **on** | The egui chart widget and the modules built on it: `chart`, `config`, `theme`, `styles` and `widget`. Without it the crate is the headless engine: [`model`], [`scales`], [`data`], [`services`], [`tokens`] and the `Indicator` trait layer of [`studies`]. Required by `drawings`, `icons` and `ui`. |
//! | `drawings` | **on** | Drawing tools, rendering, hit tests, undo and persistence. Implies `widget`; enabled by `ui`. |
//! | `studies` | **on** | The built-in studies and `IndicatorFactory`. Needs no egui, so a headless build can compute the same studies. `Indicator`, `IndicatorValue`, `IndicatorRegistry` and `CustomIndicator` are in every build. Required by `ui`. |
//! | `icons` | **on** | Compile-time embedded SVG icons, drawn through egui_extras' SVG image loader, which the chart installs in the egui context. Required by `ui`. |
//! | `ui` | **on** | Application-level UI: toolbars, panels, sidebars, dialogs, the reusable `ui_kit` widget library they are built on, and the `ui_kit::ext` helpers. Enables `icons`, `studies` and `drawings`. |
//! | `locales` | **on** | Month and weekday names and the AM/PM marker in the language of the locale given to `Chart::set_locale` or [`LocaleTimeFormatter`](scales::LocaleTimeFormatter), from chrono's locale tables. Without it those labels stay in English. |
//! | `timezones` | **on** | IANA zones and daylight-saving rules through optional chrono-tz. Without it UTC, system local time and fixed offsets remain available through [`model::TimezoneId`]; unavailable names are rejected. Named exchange presets require this feature. |
//! | `serde` | off | `Serialize` and `Deserialize` for `theme::Theme`. |
//!
//! ## Embedding in an application
//!
//! The [`guide`] covers historical and streaming data, custom studies,
//! drawing persistence, synchronized charts and host strategy or script results.
//!
//! A typical setup:
//!
//! 1. Add the crate to your dependencies:
//!
//!    ```toml
//!    [dependencies]
//!    egui-charts = "0.3"
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
//! UI labels use the crate's English text by default. Install a host lookup with
//! [`localization::set_translator`] before drawing to translate toolbars, dialogs,
//! panels and accessibility names in that egui context. Missing entries retain
//! English; replacing or clearing the lookup takes effect on the next frame.
//! The host supplies translation resources and any additional fonts. Date and
//! numeric formatting remain controlled by the chart's formatters and locale.
//!
//! ## API compatibility
//!
//! Documented public exports are supported API. Implementation modules and state are
//! private; `Chart`, `ChartBuilder`, `ChartState`, `BarData`, `TimeScale`, `PriceScale` and
//! `IndicatorRegistry` expose their state through methods. Configuration and host data records
//! have editable fields deliberately. Build extensible options from `Default` or their
//! constructors and include a fallback when matching a `#[non_exhaustive]` enum.
//!
//! Before 1.0, breaking API changes require the next minor version; patch releases preserve
//! compatibility within that minor line. From 1.0, breaking changes require a major version.
//! This includes an egui upgrade that changes types exposed by this crate: the 0.3 line uses
//! egui 0.37, and an incompatible egui upgrade requires a new breaking crate release. Compatible
//! egui patch upgrades may ship in patch releases. CI checks the API against the latest
//! published crate version, including when CI is called by the release workflow.
//!
//! ## Building for the web
//!
//! The download of a `wasm32` build shrinks with:
//!
//! - the `[profile.wasm-release]` of this crate's `Cargo.toml` (`opt-level = "s"`,
//!   `lto = "fat"`, `codegen-units = 1`) and `wasm-opt -Oz`;
//! - eframe's `glow` renderer instead of `wgpu`;
//! - eframe without its `default_fonts` feature: [`theme::apply_to_egui`] installs the
//!   one font the chart draws with;
//! - disabling `timezones` when UTC, system local time and fixed offsets suffice;
//! - with `timezones`, chrono-tz's `filter-by-regex` feature with a `CHRONO_TZ_TIMEZONE_FILTER` that
//!   keeps the zones the application uses and the ones this crate names: `UTC`,
//!   `America/New_York`, `Europe/London`, `Asia/Tokyo` and `Asia/Hong_Kong`.
//!
//! `scripts/web-size.sh` builds `examples/bare_chart.rs`, which takes the first three,
//! and the demo for the web and prints their sizes.
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
#![cfg_attr(docsrs, feature(doc_cfg))]

// Engine code names colours as `egui::Color32`; without the widget that path
// resolves to `ecolor`, the crate egui re-exports them from.
#[cfg(not(feature = "widget"))]
extern crate ecolor as egui;

// ─── Core engine modules ─────────────────────────────────────────────────────

#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub mod chart;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
#[warn(missing_docs)]
pub mod drawings;
pub mod model;
#[cfg(feature = "widget")]
mod painting;
pub mod prelude;
pub mod scales;
#[warn(missing_docs)]
pub mod studies;

// ─── Configuration & data ────────────────────────────────────────────────────

#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub mod config;
pub mod data;
pub mod services;

// ─── Theme & design system ───────────────────────────────────────────────────

#[cfg(feature = "icons")]
#[cfg_attr(docsrs, doc(cfg(feature = "icons")))]
pub mod icons;
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub mod styles;
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub mod theme;
pub mod tokens;

// ─── Widget layer ────────────────────────────────────────────────────────────

#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub mod localization;
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
#[warn(missing_docs)]
pub mod widget;

#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
#[doc = include_str!("../docs/builder-guide.md")]
#[warn(missing_docs)]
pub mod guide {}

// ─── Application-level UI (feature-gated) ────────────────────────────────────

/// Reusable UI components (buttons, dialogs, forms, color pickers, etc.).
///
/// These are domain-agnostic building blocks consumed by the [`ui`] module.
/// Enable the `ui` feature to access them.
#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
pub mod ui_kit;

/// Application-level chart UI: drawing toolbar, top toolbar, timeframe bar,
/// replay controls, dialogs, and widget panels.
///
/// Built on top of [`ui_kit`] primitives and the core chart engine. Enable the
/// `ui` feature to access this module.
#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
pub mod ui;

// ─── Convenience re-exports ──────────────────────────────────────────────────

pub use model::ChartType;
#[cfg(feature = "widget")]
#[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
pub use widget::{Chart, ChartBuilder, RightClickTarget};

// Compiles and runs the README's Rust snippets as doctests.
#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

// Compile the documentation pages alongside their shared source guides.
#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/site/src/content/docs/chart-types.mdx")]
pub struct SiteChartTypesDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/site/src/content/docs/data-model.mdx")]
pub struct SiteDataModelDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/site/src/content/docs/data.mdx")]
pub struct SiteDataDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/site/src/content/docs/first-chart.mdx")]
pub struct SiteFirstChartDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/site/src/content/docs/studies.mdx")]
pub struct SiteStudiesDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/site/src/content/docs/theming.mdx")]
pub struct SiteThemingDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/study-data.md")]
pub struct StudyDataGuideDoctests;

#[cfg(all(doctest, feature = "ui"))]
#[doc = include_str!("../docs/strategy-report.md")]
pub struct StrategyReportGuideDoctests;

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
