# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to Semantic Versioning.

## [Unreleased]

### Added

- `data::Datafeed`, a callback contract for market data: a feed answers instrument and history requests once through a `data::Reply` (dropping it unanswered reports `FeedError::Unanswered`) and pushes live bars, trades, status and errors through a per-subscription `data::Sink`, from any thread, runtime or browser task. Pushes coalesce on arrival into one bar per time slot, so memory follows the bars that changed rather than the update rate. Supporting types: `HistoryRequest`, `HistoryPage`, `Subscription`, `SubscriptionId`, `FeedStatus`, `FeedError`, `FeedState` and `MaybeSend` in `data`, `Instrument` and `BarClock` in `model`. Charts do not consume it yet; `DataSource` is unchanged.
- `model::Bar` implements `PartialEq`.

### Fixed

- The crate builds warning-free on current stable Rust: float literals that relied on the deprecated `f32` inference fallback are now explicitly typed.
- The documentation examples for the `ui` module use the current crate name and module paths, and they compile and run as doctests.
- Order line labels are shortened by rendered width with an ellipsis instead of by byte count, so labels with multi-byte characters no longer panic.
- Cloning a `CustomIndicator` or `CustomTimeFormatter` now keeps its closure instead of replacing it with an empty or default one. Migration: `CustomIndicator::new` takes the closure directly (`CustomIndicator::new(name, |bars| ...)` instead of `Box::new(...)`), and the closure must be `Sync`.
- The indicator dialog's Momentum entry creates a new `Momentum` study (the close minus the close n bars earlier, in price units) instead of Rate of Change; `Momentum` is also registered in the indicator factory.
- The export dialog no longer offers actions it cannot perform: the save path is typed into an editable field instead of a Browse button that set a fixed name, the data export text lists only the bar fields, and `ExportDialog::with_formats` limits the offered formats to those the host supports. `Exportable::copy_to_clipboard`, which discarded the image and reported success, is removed; copy a rendered `egui::ColorImage` with `egui::Context::copy_image` instead. The demo copies the chart image to the clipboard and saves PNG exports to the chosen path.
- Dashed lines are clipped to the visible area before they are dashed, so a drawing anchored far off screen no longer hangs the UI thread or exhausts memory. Every dashed line in the crate (drawings, crosshair, order lines, last-price line, session breaks, indicator pane zones) now shares this routine.
- Auto-scaling flat price or indicator data now centres the series in a range proportional to the price level instead of collapsing to a near-zero range.
- The anchored VWAP drawing computes the volume-weighted average of the typical price from its anchor bar, with bands one and two volume-weighted standard deviations away.
- The fixed range and anchored volume profiles are built from bar volume, spreading each bar's volume over its high-low range, with the point of control and a 70% value area. The anchored volume profile is placed with a single click and runs to the latest bar.
- The regression trend fits the closes between its anchors by least squares and shows the resulting R² and slope.
- Long and short position tools have a stop of their own that moves independently of the target, so risk/reward reflects the levels set; P&L replays the bars after the entry and shows the position as closed once the stop or the target trades. Both read n/a while the stop or the target is on the wrong side of the entry.
- The forecast drawing shows the predicted change over its bar span and resolves to success once a bar reaches the target by the target bar, or to failure once the target bar has closed without that.
- The bars pattern projection draws shifted copies of the selected bars.
- The chart watermark falls back to the chart symbol and is hidden when there is none, instead of showing a placeholder.

### Changed

- Replaced the bundled icon set with original artwork drawn on a 24 px grid. Toolbar, chart-type and widget-bar icons now render at 24 px, and close buttons, the cross cursor, alert and toast states, move to pane, add to watchlist, the replay next-bar button, the price range tool, the arrow tool and the note, callout, comment, table, image and other annotation tools show their own icons.
- `Drawing::render`, `Drawing::render_original`, and `DrawingManager::render_all` take a `drawings::rendering::ChartView` holding the chart's bars and coordinate mapping. Migration: build `ChartView { bars, bar_to_x, price_to_y }` from the series and the closures passed to `update_all_screen_coords`, and pass it as the last argument.
- `Chart::handle_drawings` no longer takes the last close price.

### Removed

- The Volume Footprint and Session Volume chart types, which approximated per-price volume from bar totals, with their `icons::icons::CHART_VOLUME_FOOTPRINT` and `icons::icons::CHART_SESSION_VOLUME` icons, and the unused `model::footprint` module.
- The `chart::series_api` module (`ISeriesApi`, `IPriceScaleApi`, `ITimeScaleApi`, `IStudyApi`, their implementations and option types), which nothing constructed.
- `TradingChart::toggle_main_series_visibility` and the series context menu "Hide" entry (`SeriesContextMenuAction::Hide`), which had no effect.
- `DrawingManager::update_pos_prices`, `Drawing::format_timestamp`, and the `Drawing` fields `is_closed`, `curr_price`, `forecast_start_price`, `forecast_target_price`, `forecast_start_time`, and `forecast_target_time`; position and forecast results come from the chart's bars.
- Bar replay no longer panics when a frame is due more bars than remain, and `ReplayState::update` returns the number of bars actually advanced.
- Unused icon constants `AREA_CHART`, `LINE_CHART`, `ORDER_BOOK`, `TIME_AND_SALES`, `HIDE`, `RISK_REWARD_SHORT`, `TREND_BASED_FIB_EXTENSION`, `ZOOM_TOGGLE`, `EMOJI_EAGLE`, `EMOJI_ORNAMENT`, `WIDGET_BAR_NOTIFICATIONS`, `WATCHLIST_44`, `SCREENER_44`, `PINE_SCRIPT_44`, `COMMUNITY_44` and `PRODUCTS_44`. Migration: use `CHART_AREA`, `CHART_LINE`, `WATCHLIST`, `EYE_HIDE` (or `EYE` for the visible state), `SHORT_POSITION` and `FIB_EXTENSION`; the others have no replacement.
- The unused series renderers `chart::series::{LineSeries, AreaSeries, BaselineSeries, HistogramSeries, BarSeries}` and the `Series` trait with `SeriesData` and `SeriesType`, which the chart never drew through. No replacement; the chart renders its series from `ChartType`.
- The `model::enums` module of wire-format enums, which nothing used, with `ChartType::to_chart_style` and `From<ChartStyle> for ChartType`. Migration: use `scales::PriceScaleMode` for `PriceScaleMode`, `ChartType` for `ChartStyle`, and `drawings::LineStyle` for `LineStyle` (it has no large-dashed or sparse-dotted style); `CrosshairMode` and `ColorType` are unrelated to `config::CrosshairMode` and `model::ColorType` and, like `ChartMode`, `Timezone`, `MagnetMode`, `Orientation`, `Size`, `Resolution`, `ChartTheme`, `PriceLineSource`, `ScaleLocation`, `BackgroundType`, `OverrideLineStyle`, `OverridePriceAxisLastValueMode`, `ChartStyleParameters`, `ClearMarksMode`, `MarkLocation`, `StudyPlotType`, `StudyDisplayTarget`, `StudyInputType`, `StudyTargetPriceScale`, `LineStudyPlotStyle`, `OhlcStudyPlotStyle` and `FilledAreaType`, have no replacement.
- The `templates` module. Migration: import `SettingsTemplate` and `TemplateManager` from `ui::model`.

## [0.2.0] - 2026-06-04

### Added

- Click-to-select for indicators and series on a single unified selection model, with on-chart selection handles.
- Right-click context menus for the chart background, a series, and a drawing, opened directly from the chart.
- Hover tooltip with an OHLC readout, in floating, tracking, and magnifier modes.
- Session-break dividers and optional background shading, with a `ChartBuilder::with_session_breaks` toggle (timeframe-aware day, week, and month boundaries).
- TPO / Market Profile chart type with point-of-control and a contiguous 70% value-area computation, rendered as letters or blocks.
- Remove an indicator from its pane legend; `Chart::take_indicator_remove` surfaces the request to host applications.
- Host interaction plumbing: `Chart::take_right_click` and `RightClickTarget`, `TradingChart::take_context_action` and `ChartContextAction`.
- `TradingChart` controller helpers: `config_mut`, `apply_series_settings`, `remove_indicator`, `set_indicator_visible`, and `remove_drawing`.
- The bundled demo is now a complete application: every toolbar action, all dialogs, the object-tree and alerts panels, and bar replay are wired, and it is published as a live WebAssembly build.

### Fixed

- Backtest equity now signs short positions correctly, and the Sharpe, Sortino, and Calmar ratios are no longer inflated by a percent-versus-fraction unit mismatch.
- RSI output length now matches the bar count and is no longer misaligned by one bar.
- The indicator factory constructs every built-in indicator by name rather than a small subset.
- Heikin-Ashi candles are computed over the full series, so they no longer change shape on pan and zoom.
- Coordinate and formatter transforms guard against flat price ranges and zero denominators.
- Drawing hit-testing is unified through a single path, and an undone add now survives history pruning so redo still works.
- Selection handles render correctly when the chart is scrolled.
- Embedded SVG icons render because the image loaders are now installed automatically, and notification timing no longer panics on WebAssembly.
- The bundled examples compile again.
- Issue #1: the focus ring appeared when the pointer merely hovered the chart. Issue #2: the examples did not compile.

### Changed

- Renamed `ui::stubs` to `ui::model`.
- Corrected the documented metrics to match the code.
- CI now builds the examples and runs the documentation and feature-gated tests.

### Removed

- The unused `egui_plot` dependency.
- Superseded legacy renderers and the unused render pipeline.

Thanks to @intelligentnet and @sstscrypto for the reports that became issues #2 and #1.

[0.2.0]: https://github.com/userFRM/egui-charts/compare/v0.1.0...v0.2.0
