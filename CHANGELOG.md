# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to Semantic Versioning.

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
