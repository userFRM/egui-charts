//! Interactive financial charts for [egui](https://docs.rs/egui), using your historical and streaming data.
//!
//! [`Chart`] provides price and volume panes, pan and zoom, crosshairs and live bar
//! updates on native and WebAssembly targets. Optional features add built-in studies,
//! drawing tools, order flow visualizations and application UI such as toolbars,
//! replay controls, trading panels and dialogs. The host supplies market data,
//! external accounts and durable storage through the crate's contracts.
//!
//! # Quick start
//!
//! Enable `widget` for a chart without application UI or built-in studies:
//!
//! ```toml
//! egui-charts = { version = "0.3", default-features = false, features = ["widget"] }
//! ```
//!
//! Create the chart once with bars ordered from oldest to newest, then call
//! [`Chart::show`] in your egui UI callback each frame:
//!
//! ```
//! # #[cfg(feature = "widget")] {
//! use egui_charts::{Chart, model::{Bar, BarData}};
//!
//! let bars = vec![
//!     Bar::new("2024-01-01T00:00:00Z".parse().unwrap(),
//!              100.0, 102.0, 99.0, 101.0, 1_000.0),
//!     Bar::new("2024-01-01T00:01:00Z".parse().unwrap(),
//!              101.0, 103.0, 100.0, 102.0, 1_200.0),
//! ];
//! let mut chart = Chart::new(BarData::from_bars(bars));
//!
//! # egui::__run_test_ui(|ui| {
//! chart.show(ui);
//! # });
//! # }
//! ```
//!
//! For live data, merge bars with `Chart::update(widget::SeriesId::MAIN, bar)`.
//! [`data::Datafeed`] supports paged history and streaming bars, trades and depth;
//! attach a feed with [`ChartBuilder::with_feed`]. Feed methods return promptly,
//! answer requests through [`data::Reply`] and push updates through [`data::Sink`].
//! A symbol identifies the host's instrument; setting it does not fetch data without
//! a feed. See the [`guide`] for complete integration examples.
//!
//! # API map
//!
//! | Module | Use it for |
//! |---|---|
//! | [`widget`] | `Chart`, `ChartBuilder`, events, panes, chart layouts, replay and export |
//! | [`model`] | Bars, instruments, intervals, chart types, price transforms and host data records |
//! | [`data`] | Historical and streaming data contracts, aggregation and order flow data |
//! | [`studies`] | The `Indicator` extension point, calculation helpers and built-in studies |
//! | `drawings` | Drawing tools, selection, undo and timestamp-based persistence |
//! | [`scales`] | Price ranges, coordinate mapping, axis ticks and label formatting |
//! | [`config`] | Chart appearance and interaction options |
//! | [`services`] | Host storage, symbol search, quotes, alerts, signals and broker contracts |
//! | `strategies` | Strategies that trade on closed bars and the backtest engine that runs them |
//! | [`theme`], [`tokens`], [`styles`] | Theme presets, design tokens and UI styling helpers |
//! | `ui`, `ui_kit` | Chart controls and reusable UI components placed in the host's layout |
//! | `icons` | Embedded SVG icons |
//! | [`localization`] | Host translations scoped to an egui context |
//! | [`prelude`] | Common types and traits for chart integration |
//!
//! # Feature flags
//!
//! The defaults are `ui`, `locales` and `timezones`. With `default-features = false`,
//! the data model, scales, feed and service contracts, tokens and study extension
//! points are available without an egui dependency.
//!
//! | Feature | Default | Enables |
//! |---|---|---|
//! | `widget` | via `ui` | The chart widget, rendering, configuration, themes, styling and localization |
//! | `drawings` | via `ui` | Drawing tools, interaction, undo and persistence; enables `widget` |
//! | `studies` | via `ui` | Built-in studies, `IndicatorFactory` and study templates; usable without egui |
//! | `alert-engine` | off | `services::AlertEngine`, an `Alerts` service that evaluates alerts on the bars the host feeds it; needs no egui |
//! | `detectors` | off | `services::OrderFlowDetectors`, a `Signals` service that finds order-flow events in the trades and depth the host feeds it; needs no egui |
//! | `strategies` | off | `strategies::Backtest`, which runs a `Strategy` over bars and returns a `StrategyReport`; needs no egui |
//! | `icons` | via `ui` | Embedded SVG icons and their image loaders; enables `widget` |
//! | `ui` | on | Toolbars, panels, dialogs and `ui_kit`; enables `icons`, `studies` and `drawings` |
//! | `locales` | on | Localized month and weekday names and AM/PM markers through chrono; otherwise English |
//! | `timezones` | on | IANA zones and daylight-saving rules through chrono-tz; otherwise UTC, system local time and fixed offsets |
//! | `serde` | off | `Serialize` and `Deserialize` for `theme::Theme` |
//!
//! # Application integration
//!
//! Keep a chart across frames so its viewport, studies and drawings persist.
//! Application UI components return actions for the host to apply. The crate does
//! not connect to a market data provider on its own.
//!
//! Apply [`theme::apply_to_egui`] at startup and after changing the theme. It installs
//! the bundled font and applies UI colors and spacing. Use
//! [`localization::set_translator`] for UI text; the host owns translation resources
//! and additional fonts. Chart date and number labels use the chart's formatters.
//!
//! The [`guide`] covers historical and streaming data, custom studies, drawing
//! persistence, synchronized charts and host strategy and script results.
//!
//! # API compatibility
//!
//! The 0.3 release line uses egui 0.37. Use a compatible egui version in your host
//! so types such as `egui::Ui` agree across the dependency graph. Before 1.0,
//! breaking API changes require a minor release; afterward they require a major
//! release. An incompatible upgrade of exposed egui types is a breaking change.
//!
//! Construct extensible options with their constructors or `Default`, then assign
//! public fields. Include a fallback arm when matching a `#[non_exhaustive]` enum.

// ─── Crate-level lint configuration ──────────────────────────────────────────
#![warn(unnameable_types)]
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
#[cfg(feature = "strategies")]
#[cfg_attr(docsrs, doc(cfg(feature = "strategies")))]
#[warn(missing_docs)]
pub mod strategies;

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

#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
pub mod ui_kit;

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
