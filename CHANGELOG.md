# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to Semantic Versioning.

## [Unreleased]

### Fixed

- The crate builds warning-free on current stable Rust: float literals that relied on the deprecated `f32` inference fallback are now explicitly typed.
- The documentation examples for the `ui` module use the current crate name and module paths, and they compile and run as doctests.
- Dashed lines are clipped to the visible area before they are dashed, so a drawing anchored far off screen no longer hangs the UI thread or exhausts memory. Every dashed line in the crate (drawings, crosshair, order lines, last-price line, session breaks, indicator pane zones) now shares this routine.
- Auto-scaling flat price or indicator data now centres the series in a range proportional to the price level instead of collapsing to a near-zero range.
- The anchored VWAP drawing computes the volume-weighted average of the typical price from its anchor bar, with bands one and two volume-weighted standard deviations away.
- The fixed range and anchored volume profiles are built from bar volume, spreading each bar's volume over its high-low range, with the point of control and a 70% value area. The anchored volume profile is placed with a single click and runs to the latest bar.
- The regression trend fits the closes between its anchors by least squares and shows the resulting R² and slope.
- Long and short position tools have a stop of their own that moves independently of the target, so risk/reward reflects the levels set; P&L replays the bars after the entry and shows the position as closed once the stop or the target trades.
- The forecast drawing shows the predicted change over its bar span and resolves to success or failure against the bars up to its target bar.
- The bars pattern projection draws shifted copies of the selected bars.
- The chart watermark falls back to the chart symbol and is hidden when there is none, instead of showing a placeholder.

### Changed

- `Drawing::render`, `Drawing::render_original`, and `DrawingManager::render_all` take a `drawings::rendering::ChartView` holding the chart's bars and coordinate mapping. Migration: build `ChartView { bars, bar_to_x, price_to_y }` from the series and the closures passed to `update_all_screen_coords`, and pass it as the last argument.
- `Chart::handle_drawings` no longer takes the last close price.

### Removed

- The Volume Footprint and Session Volume chart types, which approximated per-price volume from bar totals, and the unused `model::footprint` module.
- The `chart::series_api` module (`ISeriesApi`, `IPriceScaleApi`, `ITimeScaleApi`, `IStudyApi`, their implementations and option types), which nothing constructed.
- `TradingChart::toggle_main_series_visibility` and the series context menu "Hide" entry (`SeriesContextMenuAction::Hide`), which had no effect.
- `DrawingManager::update_pos_prices`, `Drawing::format_timestamp`, and the `Drawing` fields `is_closed`, `curr_price`, `forecast_start_price`, `forecast_target_price`, `forecast_start_time`, and `forecast_target_time`; position and forecast results come from the chart's bars.

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
