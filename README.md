<div align="center">

# egui-charts

Interactive financial charts for [egui](https://github.com/emilk/egui), using your historical and streaming data.

[Documentation](https://userfrm.github.io/egui-charts/docs/) · **[Live demo](https://userfrm.github.io/egui-charts/demo/)** · [API docs](https://docs.rs/egui-charts) · [Examples](https://github.com/userFRM/egui-charts/tree/main/examples)

[![CI](https://github.com/userFRM/egui-charts/workflows/CI/badge.svg)](https://github.com/userFRM/egui-charts/actions)
[![crates.io](https://img.shields.io/crates/v/egui-charts.svg)](https://crates.io/crates/egui-charts)
[![docs.rs](https://docs.rs/egui-charts/badge.svg)](https://docs.rs/egui-charts)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.95-blue.svg)](#minimum-supported-rust-version)

</div>

`Chart` provides price and volume panes, pan and zoom, crosshairs and live bar updates on native and WebAssembly targets. Optional features add built-in studies, drawing tools, order flow visualizations and application UI such as toolbars, replay controls, trading panels and dialogs. The host supplies market data, external accounts and durable storage through the crate's contracts.

## Status

[![chart](https://userfrm.github.io/egui-charts/docs/progress/badge-chart.svg)](https://userfrm.github.io/egui-charts/docs/progress/) [![drawings](https://userfrm.github.io/egui-charts/docs/progress/badge-drawings.svg)](https://userfrm.github.io/egui-charts/docs/progress/) [![studies](https://userfrm.github.io/egui-charts/docs/progress/badge-studies.svg)](https://userfrm.github.io/egui-charts/docs/progress/) [![order flow](https://userfrm.github.io/egui-charts/docs/progress/badge-order-flow.svg)](https://userfrm.github.io/egui-charts/docs/progress/) [![terminal](https://userfrm.github.io/egui-charts/docs/progress/badge-terminal.svg)](https://userfrm.github.io/egui-charts/docs/progress/) [![builder api](https://userfrm.github.io/egui-charts/docs/progress/badge-builder-api.svg)](https://userfrm.github.io/egui-charts/docs/progress/)

[![progress map](https://userfrm.github.io/egui-charts/docs/progress/progress.svg)](https://userfrm.github.io/egui-charts/docs/progress/)

## Quick start

The chart widget alone, without the toolbars, the built-in studies, the icons or `egui_extras`:

```toml
[dependencies]
egui-charts = { version = "0.3", default-features = false, features = ["widget"] }
eframe = "0.37"
```

This example uses eframe 0.37, which matches the egui version egui-charts builds on; another eframe version brings a second egui whose types do not match.

```rust,no_run
use eframe::egui;
use egui_charts::{Chart, model::{Bar, BarData}};

fn main() -> eframe::Result {
    let bars = vec![
        Bar::new("2024-01-01T00:00:00Z".parse().unwrap(),
                 100.0, 102.0, 99.0, 101.0, 1_000.0),
        Bar::new("2024-01-01T00:01:00Z".parse().unwrap(),
                 101.0, 103.0, 100.0, 102.0, 1_200.0),
    ];
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

The [demo](examples/demo) connects the top, drawing and timeframe toolbars, the dialogs, the side panels and the context menus this way. The components return actions for the host to apply; the library does not assemble an application or connect to a market data provider on its own.

## Chart types

`ChartType` offers these types, in the groups of `ChartTypeCategory`:

| Group | Types |
|---|---|
| Standard | Candles, Bars, Hollow Candles, Volume Candles |
| Line | Line, Line with Markers, Step Line |
| Area | Area, HLC Area, Baseline, Columns |
| Japanese | Heikin Ashi, Renko, Kagi, Line Break, Point and Figure |
| Range | High-Low, Range Bars |
| Advanced | Market Profile (TPO), Volume Footprint, Session Volume Profile |

## Studies

A chart computes its studies, updates them as its bars change and draws them: overlays on the price pane, the others in panes below it. `IndicatorFactory` registers 179 indicator names with default features (automatic drawing studies also require `drawings`), each a distinct study under a category such as Momentum or Volume, and the indicator dialog lists every study the factory registers, the host's own included. Other types are built directly instead of registered: some compute the same study as a registered one under their defaults (`CVD` and A/D, `CloseToCloseVol` and Historical Volatility, `HL2` and Median Price, `HLC3` and Typical Price, `PriceChannel` and Donchian Channels), and `EhlersFisher` is a variant of the Fisher Transform.

| Group | Examples |
|---|---|
| Moving averages | SMA, EMA, WMA, DEMA, TEMA, HMA, ALMA, VWMA, LSMA, KAMA, VIDYA, McGinley Dynamic |
| Momentum | RSI, MACD, Stochastic, Williams %R, CCI, ROC, Aroon, Awesome Oscillator, Ultimate Oscillator, Connors RSI, TRIX, KDJ |
| Trend | ADX, SuperTrend, Ichimoku Cloud, Parabolic SAR, Vortex, Squeeze Momentum |
| Volatility | Bollinger Bands, ATR, Keltner Channels, Donchian Channels, Historical Volatility, Chandelier Exit, Parkinson Vol, Zero-Trend Vol |
| Volume | OBV, VWAP, MFI, A/D, Chaikin Money Flow, Klinger Oscillator |
| Profiles | Volume Profile, Time Price Opportunity |
| Support and resistance | Pivot Points, ZigZag |
| Market structure | 52W HL%, 52W High/Low |
| Patterns | All Candlestick Patterns, Continuation Chart Patterns, Reversal Chart Patterns |
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

The `drawings` feature enables drawing tools and is included by `ui`. Widget-only applications add `drawings` to use these APIs. `DrawingToolType` defines 98 drawing tool types, including cursor modes, which the drawing toolbar groups into cursors, lines, Fibonacci, patterns, projections, shapes, annotations and icons. Drawings are anchored to bar and price, so they stay in place through pan and zoom, and undo and redo are built in. `DrawingToolRegistry` configures built-in availability and supports host-defined renderers; inspect availability before exposing a tool in a host picker.

Magnet mode snaps drawing anchors to nearby OHLC prices, visible study values and drawing anchors; strong magnet mode selects a bar value without a pixel threshold. `drawings::persistence` saves drawings to JSON, with their anchors as timestamps and every setting, and restores them; hosts can save and load explicitly through `Chart::save_drawings` and `load_drawings`, or attach a host storage service with `Chart::set_storage`.

## Order flow

The library renders Market Profile (TPO), volume profiles, footprints, volume bubbles and liquidity heatmaps. `Datafeed` accepts host-supplied executions and depth updates; `Footprint` aggregates executed volume at price and `DepthHistory` retains the book over time. Execution-based studies require trade data from the host. Bar-based volume estimates do not recover the actual aggressor side of trades.

`ChartType::VolumeFootprint` and `ChartType::SessionVolumeProfile` use retained and incoming feed executions on time-based intervals, with the instrument's tick size. They draw candles on trade-based intervals or without a tick size, and for bars before the first retained execution. Session profiles also require `studies`.

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

With the `serde` feature, `Theme` implements `Serialize` and `Deserialize`, so a theme can be stored in any serde format. The `dark` flag and complete UI and chart palettes are required; omitted text, spacing and radius scales use their defaults.

## Layers and feature flags

| Layer | What it gives | Feature |
|---|---|---|
| Engine | Bars, symbols and timeframes, price and time scales, the `Datafeed` contract and the host service traits (broker, symbol search, storage, alerts, signals), and the `Indicator` trait with `IndicatorRegistry`. No egui dependency. | always on |
| Studies | The built-in studies and `IndicatorFactory`, also without egui | `studies` |
| Chart widget | `Chart` and `ChartBuilder`: rendering, pan and zoom, crosshair, chart types, studies on the chart, theme | `widget` |
| Drawing tools | Drawing geometry, interaction, undo and persistence | `drawings` |
| Icons | Embedded SVG icons, drawn through `egui_extras` | `icons` |
| Terminal UI | Top, drawing and timeframe toolbars, replay controls, side panels, broker picker, account panel, order tickets with a depth ladder and trading lines, dialogs, and the `ui_kit` widgets they are built from | `ui` |

The default features are `ui`, `locales` and `timezones`. `ui` enables `icons`, `widget`, `studies` and `drawings`.

```toml
# The chart with toolbars, panels, dialogs, icons and built-in studies (default)
egui-charts = "0.3"

# The chart widget alone: no toolbars, built-in studies, icons or egui_extras
egui-charts = { version = "0.3", default-features = false, features = ["widget"] }

# The chart widget with the built-in studies
egui-charts = { version = "0.3", default-features = false, features = ["widget", "studies"] }

# The engine with the built-in studies, without egui
egui-charts = { version = "0.3", default-features = false, features = ["studies"] }
```

`timezones` (on by default) supplies IANA zones and daylight-saving rules through the optional `chrono-tz` dependency. Without it, UTC, system local time and fixed offsets remain available: `TimezoneMode::Timezone("+05:45".parse()?)` and `TimezoneId::fixed_offset(5 * 3600 + 45 * 60)` need no database. Named zones are rejected when the feature is off; Trading Sessions uses fixed standard-time offsets (+09:00, +00:00 and -05:00) for its default sessions, which the host can override with `with_session`; the exchange presets (`TimezoneMode::{nyse,lse,jse,hkex}` and `DailySessionProvider::nyse`) require it. Session configuration presets and the settings time zone list use fixed offsets when the feature is off. Add `"timezones"` to the feature list above when the host supplies named exchange zones. Time zone APIs use the crate's `model::TimezoneId`; callers no longer depend on a particular `chrono-tz` version. Persisted named-zone strings keep their format.

`locales` names months and weekdays and the AM/PM marker in the language given to `Chart::set_locale` or `LocaleTimeFormatter`, from chrono's locale tables; the builds above with `default-features = false` leave it out and write those names in English unless they list it. `serde` (off by default) adds `Serialize` and `Deserialize` to `Theme`. `theme::apply_to_egui` installs the bundled Roboto ahead of the application's proportional fonts. To replace it, call `egui::Context::set_fonts` after applying the theme. eframe enables egui's `default_fonts` as fallbacks by default. Since egui 0.37 those fonts have no emoji, which need egui's `monochrome_emoji_fonts` feature or fonts the application installs. The chart installs no image loader other than SVG; an application that shows PNG, JPEG, file or HTTP images calls `egui_extras::install_image_loaders` with the loader features it needs.

## Performance

`cargo bench --bench bare_chart --no-default-features --features widget` measures the frame cost of the bare chart example. `cargo bench --bench frame_time` covers large histories and multiple charts with studies.

On Linux with glibc, freeing large tessellation buffers can cause heap trimming and page faults on every frame. In the reported 5,000,000-bar workload, setting both `M_MMAP_THRESHOLD` to 64 MiB and `M_TRIM_THRESHOLD` to 256 MiB reduced the frame from about 5.8 ms to 3.9 ms, within 2% of egui 0.33. Setting either threshold alone did not remove the overhead. Applications can instead raise `M_TOP_PAD` to 16 MiB or choose another global allocator. The library leaves process-wide allocation policy to the host. See [performance notes](docs/performance.md) for the `GLIBC_TUNABLES` commands, measurements and memory tradeoffs.

## Connect your data

The [builder guide](docs/builder-guide.md), also included in the
[API documentation](https://docs.rs/egui-charts/latest/egui_charts/guide/), covers
history, streaming, custom studies, drawing persistence, synchronized charts and
host strategy and script integration. docs.rs builds every feature; module badges
show the feature needed to use each optional part.

Implement `Datafeed` to connect any market data provider, as in the [terminal example](#a-terminal-around-it); only `history` is required. The chart calls the feed from `show` on the UI thread and never waits for it: answer through the `Reply` and push into the `Sink` from any thread, runtime or browser task. The chart asks for older pages as the user scrolls back, and unsubscribes when the symbol or timeframe changes. [`examples/live_feed.rs`](examples/live_feed.rs) serves history from memory and streams synthetic trades.

## Workflow examples

Run each with `cargo run --example <name>`. These examples use generated market data.

| Example | Workflow |
|---|---|
| [basic_chart](examples/basic_chart.rs) | Historical bars, a price line and click events |
| [live_feed](examples/live_feed.rs) | Paged history and a cancellable stream of executions |
| [with_indicators](examples/with_indicators.rs) | Built-in overlays and study panes |
| [custom_indicator](examples/custom_indicator.rs) | A calculation closure with explicit warm-up values |
| [drawing_tools](examples/drawing_tools.rs) | Programmatic and interactive annotations |
| [drawing_persistence](examples/drawing_persistence.rs) | Save and load timestamped drawing JSON, reporting errors |
| [backtesting](examples/backtesting.rs) | A host buy-and-hold run, fees, equity and strategy metrics; also runs with `--no-default-features` |
| [scripting](examples/scripting.rs) | A minimal host expression evaluator connected to a custom study |
| [synchronized_charts](examples/synchronized_charts.rs) | Crosshairs and viewports shared by UTC across 1-hour and 4-hour charts |

The host owns market data, durable storage, strategy execution and its scripting
runtime. The scripting example accepts only `close - close[N]`; it demonstrates
the adapter and error handling, and adds no language support to the library.

## Platform support

| Target | Status |
|---|---|
| Linux x86_64 | Built and linted in CI on stable and the MSRV; tests run on both |
| macOS aarch64 | Built and linted in CI on stable and the MSRV; tests run on both |
| Windows x86_64 | Built and linted in CI on stable and the MSRV; tests run on both |
| WebAssembly (wasm32-unknown-unknown) | Widget tests run in headless Chrome; the demo is built in CI and runs as the [live demo](https://userfrm.github.io/egui-charts/demo/) |

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
