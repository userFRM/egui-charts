<div align="center">

# egui-charts

Financial charts for [egui](https://github.com/emilk/egui), from a single chart widget to a full trading terminal.

**[Live demo](https://userfrm.github.io/egui-charts/demo/)** · [API docs](https://docs.rs/egui-charts) · [Examples](https://github.com/userFRM/egui-charts/tree/main/examples)

[![CI](https://github.com/userFRM/egui-charts/workflows/CI/badge.svg)](https://github.com/userFRM/egui-charts/actions)
[![crates.io](https://img.shields.io/crates/v/egui-charts.svg)](https://crates.io/crates/egui-charts)
[![docs.rs](https://docs.rs/egui-charts/badge.svg)](https://docs.rs/egui-charts)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.95-blue.svg)](#minimum-supported-rust-version)

</div>

Pure Rust, native and WebAssembly from the same code. Take it as far as you need: one chart in your own egui app, or the chart with studies, drawing tools, toolbars and dialogs around it.

## A chart in ten lines

The chart widget alone, without the toolbars, the built-in studies, the icons or `egui_extras`:

```toml
[dependencies]
egui-charts = { version = "0.2", default-features = false, features = ["widget"] }
eframe = "0.37"
```

This example uses eframe 0.37, which matches the egui version egui-charts builds on; another eframe version brings a second egui whose types do not match.

This README describes the `main` branch, which is not published yet. Until the next release, use `git = "https://github.com/userFRM/egui-charts"` in place of `version = "0.2"`, or `cargo add egui-charts --git https://github.com/userFRM/egui-charts`. The documentation of the published 0.2.0 is at <https://docs.rs/egui-charts/0.2.0>.

```rust,no_run
use eframe::egui;
use egui_charts::{Chart, model::{Bar, BarData}};

fn main() -> eframe::Result {
    let bars: Vec<Bar> = Vec::new(); // your bars, oldest first
    let mut chart = Chart::new(BarData::from_bars(bars));
    eframe::run_ui_native("My chart", Default::default(), move |ui, _| {
        egui::CentralPanel::default().show(ui, |ui| chart.show(ui));
    })
}
```

The chart pans, zooms and shows a crosshair out of the box. `set_chart_type` picks one of the chart types below, `set_data` replaces the bars of a series, and `update` merges a live bar into it. [`examples/bare_chart.rs`](examples/bare_chart.rs) streams sample ticks into this chart.

## A terminal around it

In the default build, the toolbars, panels and dialogs are components you place in your layout; each returns an action that your app applies to the chart. This one puts the drawing toolbar next to a chart that loads its bars from your `Datafeed`:

```sh
cargo add egui-charts
cargo add eframe@0.37
```

```rust,no_run
use std::sync::Arc;

use eframe::egui;
use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
use egui_charts::model::Timeframe;
use egui_charts::ui::drawing_toolbar::DrawingToolbar;
use egui_charts::{Chart, ChartBuilder};

/// Your market data: history on request, then live updates.
struct MyFeed;

impl Datafeed for MyFeed {
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        // Up to `request.count` bars that open before `request.before`, or the latest bars
        // when it is `None`. Fetch them on another thread or task and reply from there.
        let _ = request;
        reply.send(Ok(HistoryPage::default()));
    }

    fn subscribe(&self, subscription: Subscription) {
        // Push live bars, trades and depth into `subscription.sink` from your connection
        // until `Sink::is_active` turns false.
        let _ = subscription;
    }
}

struct Terminal {
    chart: Chart,
    tools: DrawingToolbar,
}

impl eframe::App for Terminal {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("tools").show(ui, |ui| {
            if let Some(tool) = self.tools.show(ui) {
                self.chart.drawings_mut().set_active_tool(Some(tool));
            }
        });
        egui::CentralPanel::default().show(ui, |ui| self.chart.show(ui));
    }
}

fn main() -> eframe::Result {
    let chart = ChartBuilder::new()
        .with_symbol("SAMPLE")
        .with_timeframe(Timeframe::Min1)
        .with_feed(Arc::new(MyFeed))
        .build();
    let tools = DrawingToolbar::default();
    eframe::run_native(
        "My terminal",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(Terminal { chart, tools }))),
    )
}
```

The [demo](examples/demo) connects the top, drawing and timeframe toolbars, the dialogs, the side panels and the context menus this way. Controls that need a backend the demo does not have show a notice, and some context menu entries do nothing yet. An assembled terminal that handles these actions itself, so an app only supplies its services, is planned ([#205](https://github.com/userFRM/egui-charts/issues/205)).

## Chart types

`ChartType` offers these types, in the groups of `ChartTypeCategory`:

| Group | Types |
|---|---|
| Standard | Candles, Bars, Hollow Candles, Volume Candles |
| Line | Line, Line with Markers, Step Line |
| Area | Area, HLC Area, Baseline, Columns |
| Japanese | Heikin Ashi, Renko, Kagi, Line Break, Point and Figure |
| Range | High-Low, Range Bars |
| Advanced | Market Profile (TPO) |

## Studies

A chart computes its studies, updates them as its bars change and draws them: overlays on the price pane, the others in panes below it. `IndicatorFactory` registers 158 indicator names, each a distinct study under a category such as Momentum or Volume, and the indicator dialog lists every study the factory registers, the host's own included. Six more types are built directly instead of registered: five compute the same study as a registered one under their defaults (`CVD` and A/D, `CloseToCloseVol` and Historical Volatility, `HL2` and Median Price, `HLC3` and Typical Price, `PriceChannel` and Donchian Channels), and `EhlersFisher` is a variant of the Fisher Transform.

| Group | Examples |
|---|---|
| Moving averages | SMA, EMA, WMA, DEMA, TEMA, HMA, ALMA, VWMA, LSMA, KAMA, VIDYA, McGinley Dynamic |
| Momentum | RSI, MACD, Stochastic, Williams %R, CCI, ROC, Aroon, Awesome Oscillator, Ultimate Oscillator, Connors RSI, TRIX, KDJ |
| Trend | ADX, SuperTrend, Ichimoku Cloud, Parabolic SAR, Vortex, Squeeze Momentum |
| Volatility | Bollinger Bands, ATR, Keltner Channels, Donchian Channels, Historical Volatility, Chandelier Exit, Parkinson Vol, Zero-Trend Vol |
| Volume | OBV, VWAP, MFI, A/D, Chaikin Money Flow, Klinger Oscillator, Volume Profile |
| Support and resistance | Pivot Points, ZigZag |
| Market structure | 52W HL%, 52W High/Low |
| Patterns | All Candlestick Patterns |
| Statistics | Linear Regression, Standard Error, Rank Correlation |

```rust
use egui_charts::studies::{BollingerBands, RSI, SMA};
# let mut chart = egui_charts::Chart::new(egui_charts::model::BarData::default());
let sma = chart.add_indicator(Box::new(SMA::new(20)));
chart.add_indicator(Box::new(RSI::new(14)));
chart.add_indicator(Box::new(BollingerBands::new(20, 2.0)));
chart.remove_series(sma);
```

The built-in studies and `IndicatorFactory` are in the `studies` feature, which the default build includes and which needs no egui. The `Indicator` trait and `IndicatorRegistry`, which computes studies on a slice of bars without a chart, are in every build, so a study of your own needs neither.

## Drawing tools

`DrawingToolType` defines 98 drawing tool types, counting four cursor modes, which the drawing toolbar groups into cursors, lines, Fibonacci, patterns, projections, shapes, annotations and icons. Drawings are anchored to bar and price, so they stay in place through pan and zoom, and undo and redo are built in. Some tools are placeholders or approximations; [ROADMAP.md](ROADMAP.md) lists the status of each.

Magnet snapping is not applied yet. `drawings::persistence` saves drawings to JSON, with their anchors as timestamps and every setting, and restores them; the chart does not call it, so the application decides when to save and load.

## Order flow

The Market Profile (TPO) chart type and study, fixed range and anchored volume profiles, and cumulative volume delta, estimated from each bar's prices. A `Datafeed` can already push trades and order book depth; a footprint chart ([#231](https://github.com/userFRM/egui-charts/issues/231)) and a liquidity heatmap of the order book ([#235](https://github.com/userFRM/egui-charts/issues/235)) are planned.

## Themes

A `Theme` is one value with public fields: UI colours (`theme.ui`), chart colours (`theme.chart`), and the type, spacing and corner radius scales. It comes in dark, light and high-contrast presets. Start from one, override what you need, and apply it at startup and after every change:

```rust
use eframe::egui::Color32;
use egui_charts::theme::{Theme, apply_to_egui};

# let ctx = &eframe::egui::Context::default();
// The accent's hover and pressed shades, the text on it and the selection follow it.
let mut theme = Theme::dark().with_accent(Color32::from_rgb(255, 120, 0));
theme.chart.candle_up = Color32::from_rgb(0, 160, 120);
theme.chart.candle_down = Color32::from_rgb(220, 60, 60);
apply_to_egui(ctx, &theme);

// Light UI chrome with the dark chart.
let classic = Theme { chart: Theme::dark().chart, ..Theme::light() };
```

With the `serde` feature, `Theme` implements `Serialize` and `Deserialize`, so a theme can be stored in any serde format; fields missing from the input take the dark theme's values. Some surfaces still keep dark colours in the light theme ([#275](https://github.com/userFRM/egui-charts/issues/275)).

<!-- section slot: multi-chart layouts, once #115 lands -->

## Layers and feature flags

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/userFRM/egui-charts/main/docs/layers-dark.svg">
  <img alt="Four nested layers, each containing the ones inside it. Innermost, the engine and chart widget, built with default features off and the widget feature: one chart in your egui app, fed by your Datafeed, with chart types, scales, pan and zoom, crosshair, markers, drawing tools, themes and the Indicator trait. Around it, studies and terminal, the default build (ui, which enables studies): the built-in studies, the toolbars, panels and dialogs, and the symbol search and storage traits. Then trading: every build has the Broker trait and a paper account; the ui feature adds a broker picker, an account panel, order tickets and order and position lines. Depth of market is planned. Outermost, order flow, with no feature yet: the default build has TPO, volume profile and cumulative delta, and footprint and depth heatmap are planned." src="https://raw.githubusercontent.com/userFRM/egui-charts/main/docs/layers-light.svg" width="880">
</picture>

| Layer | What it gives | Feature |
|---|---|---|
| Engine | Bars, symbols and timeframes, price and time scales, the `Datafeed` contract and the host service traits (broker, symbol search, storage, alerts, signals), and the `Indicator` trait with `IndicatorRegistry`. No egui dependency. | always on |
| Studies | The built-in studies and `IndicatorFactory`, also without egui | `studies` |
| Chart widget | `Chart` and `ChartBuilder`: rendering, pan and zoom, crosshair, chart types, drawings, studies on the chart, theme | `widget` |
| Icons | Embedded SVG icons, drawn through `egui_extras` | `icons` |
| Terminal UI | Top, drawing and timeframe toolbars, replay controls, side panels, broker picker, account panel, order tickets and trading lines, dialogs, and the `ui_kit` widgets they are built from | `ui` |

The default features are `ui`, which enables `icons`, `widget` and `studies`, and `locales`.

```toml
# The chart with toolbars, panels, dialogs, icons and built-in studies (default)
egui-charts = "0.2"

# The chart widget alone: no toolbars, built-in studies, icons or egui_extras
egui-charts = { version = "0.2", default-features = false, features = ["widget"] }

# The chart widget with the built-in studies
egui-charts = { version = "0.2", default-features = false, features = ["widget", "studies"] }

# The engine with the built-in studies, without egui
egui-charts = { version = "0.2", default-features = false, features = ["studies"] }
```

`locales` names months and weekdays and the AM/PM marker in the language given to `Chart::set_locale` or `LocaleTimeFormatter`, from chrono's locale tables; the builds above with `default-features = false` leave it out and write those names in English unless they list it. `serde` (off by default) adds `Serialize` and `Deserialize` to `Theme`. `theme::apply_to_egui` installs the bundled Roboto ahead of the application's proportional fonts. To replace it, call `egui::Context::set_fonts` after applying the theme. eframe enables egui's `default_fonts` as fallbacks by default. Since egui 0.37 those fonts have no emoji, which need egui's `monochrome_emoji_fonts` feature or fonts the application installs. The chart installs no image loader other than SVG; an application that shows PNG, JPEG, file or HTTP images calls `egui_extras::install_image_loaders` with the loader features it needs.

`cargo bench --bench bare_chart --no-default-features --features widget` measures the frame cost of the bare chart example.

<!-- performance slot: binary size and frame time of the bare-chart build, quoted only once CI tracks them -->

## Connect your data

Implement `Datafeed` to connect any market data provider, as in the [terminal example](#a-terminal-around-it); only `history` is required. The chart calls the feed from `show` on the UI thread and never waits for it: answer through the `Reply` and push into the `Sink` from any thread, runtime or browser task. The chart asks for older pages as the user scrolls back, and unsubscribes when the symbol or timeframe changes. [`examples/live_feed.rs`](examples/live_feed.rs) serves history from memory and streams synthetic trades.

## Platform support

| Target | Status |
|---|---|
| Linux x86_64 | Built and linted in CI on stable and the MSRV; tests run on both |
| macOS aarch64 | Built and linted in CI on stable and the MSRV; tests run on stable |
| Windows x86_64 | Built and linted in CI on stable and the MSRV; tests run on stable |
| WebAssembly (wasm32-unknown-unknown) | The demo is built for it in CI and runs as the [live demo](https://userfrm.github.io/egui-charts/demo/) |

Other targets are expected to work but are not built in CI. The chart is an egui widget, so it runs wherever egui runs: in an eframe window on Linux, macOS and Windows, or in the browser through eframe's web runner.

## Minimum supported Rust version

The MSRV is **1.95** (Rust edition 2024), set in `Cargo.toml` through `rust-version`.

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

Some interface icons under `assets/icons/` come from Lucide and are licensed under the ISC License (with a Feather-derived subset under the MIT License); see the [third-party notices](THIRD-PARTY-NOTICES.md) and the [Lucide licence](assets/icons/LICENSE-lucide.txt). The bundled Roboto Latin subset is licensed under the [SIL Open Font License 1.1](assets/fonts/LICENSE-roboto.txt).

## Contributing

Contributions are welcome: bug fixes, docs, examples, and new studies, drawing tools, chart types and themes. Start with [CONTRIBUTING.md](CONTRIBUTING.md), which covers the dev setup, the project layout, where each kind of addition lives, and the local checks that mirror CI.

Please also read the [Code of Conduct](CODE_OF_CONDUCT.md), and report security issues privately per the [Security Policy](SECURITY.md). Questions and ideas are welcome in [Discussions](https://github.com/userFRM/egui-charts/discussions).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
