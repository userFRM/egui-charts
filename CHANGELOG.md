# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to Semantic Versioning.

## [Unreleased]

### Added

- `data::Datafeed`, a callback contract for market data: a feed answers instrument and history requests once through a `data::Reply` (dropping it unanswered reports `FeedError::Unanswered`) and pushes live bars, trades, status and errors through a per-subscription `data::Sink`, from any thread, runtime or browser task. Pushes coalesce on arrival into one bar per time slot, so memory follows the bars that changed rather than the update rate. Supporting types: `HistoryRequest`, `HistoryPage`, `Subscription`, `SubscriptionId`, `FeedStatus`, `FeedError`, `FeedState` and `MaybeSend` in `data`, `Instrument` and `BarClock` in `model`. Charts do not consume it yet; `DataSource` is unchanged.
- `model::Bar` implements `PartialEq`.
- `ThemeContextExt::active_theme` returns the theme last passed to `apply_to_egui`, so components can resolve colours while painting. It has a default implementation returning `None`, so existing implementors of the trait still compile.
- `tokens::UiSemanticTokens` gains `panel_bg_floating_dark`, the fill of popups, menus and windows in dark themes. Struct literals of `UiSemanticTokens` must set it.
- A theme toggle in the top toolbar emits `TopToolbarAction::ThemeToggle`.
- `Indicator::update(data, from)` brings a study up to date after the bars from `from` onward changed or were appended, and `Indicator::is_incremental` reports whether a last-bar update or an append costs time independent of the series length (an earlier change still recomputes in full). `IndicatorRegistry::update_all` updates every study. Both methods have default implementations (a full `calculate`, and `false`), so existing implementors still compile. `SMA` and `EMA` update incrementally.
- Icons `CHECK`, `MINUS`, `CHEVRON_UP`, `CHEVRON_LEFT`, `CHEVRON_RIGHT`, `STAR_FILLED`, `CODE`, `USERS`, `CROWN`, `DATABASE`, `PENCIL` and `TEMPLATE` in `icons::icons`.
- `ThemeData::ui_accent_text`, the text and icon colour on an accent fill. Struct literals of `ThemeData` must set it.
- `ListItemContent::interactive` lets content take its row out of pointer interaction. It defaults to `true`; a disabled `ButtonContent` returns `false`, so its row is neither highlighted nor clickable.
- `data::Sink::push_tick` also keeps each trade, with its price, size, aggressor side and time, in a bounded store for order flow, and `Datafeed::trades` pages historical trades (`TradeRequest`, `TradePage`) so order flow can be rebuilt after a reload. The default answers `FeedError::NotSupported`. History and live trades join exactly when both carry the new `Tick::id` (an exchange trade id or sequence number); without ids, trades at the boundary instant may be dropped but are never counted twice.
- Order book depth on the `Datafeed` contract: `Sink::push_depth_snapshot` and `Sink::push_depth_delta` carry a sequence number; a gap, a delta before any snapshot or an invalid update clears the book until the feed pushes a new snapshot, which is asked for once through `Datafeed::request_depth_snapshot`. A level with size 0 is removed, and a book is limited to 10,000 levels per side. Rejected updates are counted in `FeedState::rejected_depth`.
- `model::Instrument` describes the instrument type, tick size, price decimals, currency, exchange time zone (`model::TimezoneId`) and trading hours (`model::Session`, `model::SessionWindow`), each unknown by default and set with a `with_` method.
- `model::Tick` implements `PartialEq`.
- `DialogFooter::danger` fills the primary action with the danger colour.
- `scales::TimezoneMode`, the time axis timezone mode, now lives next to the time formatters that use it. `config::TimezoneMode` remains as a re-export of the same type.

### Fixed

- Light theme text and control outlines are readable: secondary and muted text reach at least 4.5:1 on panels and windows, unchecked checkboxes and radios, text inputs and combo boxes have a 3:1 outline, and egui's weak text uses the muted colour instead of fading by alpha. Dark theme secondary and muted text are raised to the same bar.
- `apply_to_egui` sets `error_fg_color` and `warn_fg_color` from the theme, so error text no longer uses egui's default pure red, and the status colours (success, warning, error, info) are defined per theme with at least 4.5:1 on its panels.
- Hover and pressed fills in the light theme are visible against white (1.2:1 and 1.3:1, from 1.1:1), and the accent hover and pressed shades and the danger button reds keep white text at 4.5:1 or better.
- The crate builds warning-free on current stable Rust: float literals that relied on the deprecated `f32` inference fallback are now explicitly typed.
- The documentation examples across the crate use the current crate name and module paths, and they compile and run as doctests with all features; none are skipped. The `icon_from_path!` documentation now says its path is relative to the `assets/icons/` directory of the crate that invokes the macro.
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
- The OHLC header of a chart without a symbol prints prices with the same magnitude-based decimals as the legend instead of 8 decimals, and labels the last bar's volume as volume instead of 24h volume. The top toolbar's symbol pill shows no symbol instead of AAPL when none is set, the chart and series context menus leave out their alert, order, watchlist and note items until there is a symbol, and `SymbolHeader` leaves out empty info fields instead of drawing bare separators.
- Zooming the price axis in by hand stops at a span of one millionth of the price level instead of narrowing to a few nanounits, and repeated zooming out can no longer overflow the price range to infinite or NaN bounds.
- `Timeframe::duration_ms` saturates at `i64::MAX` for custom timeframes too large to represent instead of overflowing into a panic or a negative duration, and parsing a custom timeframe whose second count overflows is rejected.
- Built-in studies raise a length of 0 to 1 and accept any longer length, alone or mixed with short ones, so a zero, huge or swapped period no longer panics, exhausts memory or yields only NaN (for example `KAMA::new(0)`, `SmoothedMA::new(0)` and `MACross::new(50, 20)`).
- SMA, VWMA, LSMA, Bollinger Bands, RSI, MACD and VW-MACD return one `None` per bar instead of no values when there are fewer bars than their period.
- Stochastic no longer panics when all its periods are 1, Volume Profile Fixed Range no longer panics on a bar with a NaN price, and Rank Correlation no longer panics sorting NaN closes.
- Rainbow Oscillator keeps a running sum instead of every SMA level, so a huge level count no longer exhausts memory.
- Volume profiles refuse a row count above 10,000 and yield `None` for every bar instead of allocating that many rows for each bar.
- Iceberg and Liquidity key prices on one grid per series, the tick size widened to a whole number of ticks when the series spans more than 1,000 ticks, and skip bars with a non-finite price or an inverted range, so an infinite or 1e12 print no longer hangs the chart or exhausts memory. Because the grid comes from the whole series, a bar that extends its range can change earlier values. Liquidity scans its levels once per bar instead of three times, and Iceberg keeps its levels in a dense vector.
- Iceberg, Stop Run and Liquidity plot in their own pane instead of on the price scale, where their scores and ratios drew nothing. Iceberg and Stop Run plot 0 on the bars they do not flag, so flags show as spikes.
- Liquidity records a sweep only for heavy levels inside the bar's range that held their volume before the bar, as one sweep per side of the level holding the bar's open, covering the swept range and volume, instead of one sweep for every heavy level at or beyond the bar's low or high, prices the bar never traded. Sweep storage now grows with the number of bars: on 20,000 synthetic bars with a 0.01 tick and period 500 it went from 1,684,762 sweeps and 57 MB peak memory to 25,443 sweeps and 6 MB. Migration: `LiquiditySweep::swept_price` is replaced by `price_low` and `price_high`, and `is_clean` now means the bar closed back beyond the swept levels.
- Liquidity keeps a level touched in the first `period` bars for `period` bars, like any other level, instead of dropping levels touched on the first bar at once.
- Iceberg counts a level's volume over the same `period` bars as its touches; the volume used to grow without bound while the touches decayed, and the window held one bar more than `period`.
- `to_tpo_profiles` widens the tick size to a whole number of ticks when the bars span more than 1,000 ticks and skips bars with a non-finite or inverted range, so one wide or infinite bar no longer hangs the chart or exhausts memory. Migration: `TPOProfile` has a `tick_size` field holding the row height used, which `TPOProfile::new` takes as a second argument; pass it to `tpo_count_at`, `price_levels`, `letters_at` and `classify_shape`.
- The CVD, Iceberg, Stop Run and Liquidity documentation states that they are estimated from bars and use no trade or order book data.
- The keyboard focus ring uses the selection stroke, the colour egui outlines a focused text input with, instead of the selection fill at reduced alpha, so it reaches 3:1 on panels and windows in every preset (it was 1.2:1 in light themes and 2.0 to 2.3:1 in dark ones).
- Widgets that meant the accent read the theme accent instead of the selection fill, which is pale in light themes: tab underlines, the sync button and its checkboxes, the selected drawing colour swatch, the open colour picker, the layout name input, the date picker range and Primary list item buttons. Text on an accent fill uses the theme's on-accent colour, and Danger list item buttons fill from the danger button token.
- Selected sidebar items and active chart control toggles use the selection fill and text colour (the sidebar painted accent text on an accent tint, 2.4:1 in dark themes), active tab labels use the strong text colour, descriptions on selected timeframe, chart type and template rows are no longer faded to 3.7:1, and the success status message uses the theme's success colour instead of the accent.
- Selected rows, tabs, menu items and calendar days render readable text on the selection fill instead of an empty accent bar (the selection text colour equalled the fill): white on the accent in dark themes, accent text on a pale accent fill in light themes.
- Tooltips, popups and menus have padding, a hairline border, an elevated fill, a small radius and a tight shadow; windows get padding and a border.
- `apply_to_egui` sets item spacing, window padding and menu padding from the design tokens instead of zero, so labels no longer touch their controls; toolbars, dropdown rows and stacked chart panes keep their own explicit gaps.
- Switching the theme repaints everything: the bottom toolbar, date picker, range switcher, timeframe bar, right panel tabs, symbol header, chart context menus, OHLC tooltip, replay bar and command palette resolve their colours from the active theme instead of keeping the dark theme they were built with. Fields a caller sets on the public `config` of `TimeframeToolbar`, `TimeframeBar` or `RightPanelTabs` are kept; only the fields left at their theme-derived value follow the theme.
- Custom-painted buttons (top toolbar, drawing toolbar, widget bar and the `ui_kit` buttons) draw a focus ring when they have keyboard focus.
- The dialog close button no longer draws an accent outline at rest; it looks like an icon button, so only keyboard focus draws a ring around it.
- The command palette search icon follows the theme instead of staying white, so it is visible in the light theme.
- The native demo starts on Linux: it enables wgpu's default backends instead of none.
- The README, package description, crate docs and ROADMAP state what the code does: 95 drawing tool types counting cursor modes and duplicates, and 130 registered indicator names of which 124 are distinct calculations (a unit test keeps the stated counts in step with the code), no unproven icon count, and no claim of magnet snapping, drawing persistence or templates saved to disk, none of which works yet.
- ROADMAP ticks an item only when it works end to end: unwired widgets, scale modes the chart does not render, drawing tools with wrong geometry and indicator lines that are computed but not drawn are now unticked or carry their limits.
- The README's Rust snippets compile and run as doctests against the current API (including the `DataSource` example), and the install lines use version `0.2`.
- The `DataSource::fetch_historical` and `QuoteSource` docs describe when they are actually called: history is fetched on the UI thread, and nothing in the crate polls quote sources.
- Bar replay no longer panics when a frame is due more bars than remain, and `ReplayState::update` returns the number of bars actually advanced.
- `ListItem` allocates its row once: rows are 28 px apart with no gap, hover, pressed and selected fill the whole row, and every content type starts its icon and text 8 px inside the row. Custom `ListItemContent` implementations paint inside `ContentContext::rect` and return the row response instead of allocating space of their own. Row colours come from the active theme, with a pressed state distinct from hover and on-accent text on selected rows, and rows report their role, label and selection to accessibility tools.
- The `AutocompleteEdit` popup is exactly as wide as its input, has the popup border, aligns its suggestions with the input text and draws them in the theme's text colour.
- `FormRow` takes the height of its content instead of the remaining height of the panel, so consecutive rows and a following `FormGrid` stay visible. The label is centred on the widget's line, and the description sits below it in the theme's muted text colour.
- The `FilterWidget` search icon and clear button sit inside the field, 8 px from its edges, and the field no longer grows by a pixel when hovered or focused. Clicking the clear button now clears the query.
- The `ColorPicker` swatch is 28 px with a border that stays visible on dark colours and brightens on hover. Its popup is as wide as the swatch grid and uses the popup border, and the row below the grid shows the hex value and opacity of the selected colour.
- The compare symbols dialog draws its symbols, labels and background in the theme's colours when `CompareSymbolsConfig` leaves them at the default instead of drawing them transparent.
- The `ui_kit` buttons share one state model. Rest, hover and press are each a visible step: neutral fills move one step further from the surface, and Primary and Danger fills darken, so pressed no longer equals hover or looks paler than it; a selected `Button`, `IconButton` or `SplitButton` is drawn as Primary, filled with the accent; `enabled(false)` renders exactly like a button inside `add_enabled_ui(false)`; and a disabled Primary or Danger button is drawn as Secondary, so its faded label stays as readable as other disabled buttons, with an accent outline on a disabled Primary `Button` so a selected one still reads as selected.
- `ButtonVariant` colours follow the active theme: Primary is the theme accent with its hover and pressed shades and on-accent text, Secondary a neutral fill one step above the surface, Outlined a border in the secondary text colour, Ghost the primary text colour, and Danger the danger button reds with on-accent text.
- `Button` is as wide as its laid-out label plus 12 px on each side, instead of an estimate from the byte length, so every label gets the same padding and labels with multi-byte characters fit.
- `SplitButton` points its chevron down whether or not it is expanded.
- The `ui_kit` buttons report their role, label (the tooltip for icon-only buttons), and enabled and selected state to accessibility tools, and show their tooltip while disabled.
- The kit `Modal`, `ConfirmDialog`, `PromptDialog` and the command palette draw their dimming backdrop below their window instead of over it, so the dialog is shown at full contrast. Clicking the backdrop closes a closable modal.
- `PromptDialog` keeps the text typed into it between frames instead of resetting it to the initial value, so Enter and the submit button return what was typed.

### Changed

- `model::Tick` is `#[non_exhaustive]` and has an `id` field. Migration: build it with `Tick::new(ts, price, size, side)`, adding `.with_id(id)` when the feed has trade ids.
- `model::SymbolType` is `#[non_exhaustive]`. Migration: add a wildcard arm to matches on it.
- `tokens::UiSemanticTokens` replaces `warning` and `success` with per-theme status colours `{success,warning,error,info}_{light,dark}`, and gains `text_disabled_light`, `text_disabled_dark`, `border_control_light`, `border_control_dark`, `selection_bg_light` and `selection_text_light`; `theme::UiSemanticTokens` gains `border_control`, the outline `apply_to_egui` gives inactive and hovered widgets. Migration: read `DESIGN_TOKENS.semantic.ui.warning_dark` and `success_dark` for the old values, or the theme's `semantic.ui.warning` and `success` to follow the active theme; struct literals must set the new fields.
- The theme's `accent_active` (pressed accent) is a darker accent, (31, 78, 205), instead of the grey `buttons.primary_bg_active`, and `accent_hover` is (35, 88, 230) instead of the lighter `blue_600`. `tokens::BrandSemanticTokens` gains `accent_active`; struct literals must set it. The `buttons.danger_bg`, `danger_bg_hover` and `danger_bg_active` tokens are darker reds.
- Replaced the bundled icon set with original artwork drawn on a 24 px grid. Toolbar, chart-type and widget-bar icons now render at 24 px, and close buttons, the cross cursor, alert and toast states, move to pane, add to watchlist, the replay next-bar button, the price range tool, the arrow tool and the note, callout, comment, table, image and other annotation tools show their own icons.
- `Drawing::render`, `Drawing::render_original`, and `DrawingManager::render_all` take a `drawings::rendering::ChartView` holding the chart's bars and coordinate mapping. Migration: build `ChartView { bars, bar_to_x, price_to_y }` from the series and the closures passed to `update_all_screen_coords`, and pass it as the last argument.
- `Chart::handle_drawings` no longer takes the last close price.
- `TooltipOptions::default()` leaves `background_color`, `text_color` and `tracking_bar_background` as `Color32::PLACEHOLDER`, which resolves from the active theme; set them explicitly to keep fixed colours.
- `SymbolHeaderConfig::default()` text, label and separator colours are `Color32::PLACEHOLDER` and follow the egui style; `SymbolHeaderConfig::from_theme` now uses the theme it is given.
- `apply_to_egui` styles the selection per theme. Dark themes fill it with the accent and set `visuals.selection.stroke` to white; light themes fill it with a pale accent, (219, 229, 255), and set the stroke to a darker accent, (31, 78, 205), which also outlines a focused text input. `ThemeUiExt::theme_accent` and `theme_icon_active` return the theme's accent instead of the selection fill. Migration: code that read `selection.bg_fill` or `selection.stroke.color` as the accent colour should read `ThemeUiExt::theme_accent` (or the theme's `semantic.ui.accent`); text painted on `selection.bg_fill` should use `selection.stroke.color`, not white.
- `apply_to_egui` sets `item_spacing` to 8 x 4, `window_margin` to 12 and `menu_margin` to 8 x 4 instead of zero. Layouts that relied on zero spacing should set `ui.spacing_mut().item_spacing = Vec2::ZERO` locally.
- `apply_to_egui` sets `visuals.window_fill` to `UiSemanticTokens::panel_bg_floating`, which in dark themes is now (42, 46, 57), one step above `panel_bg`, instead of equal to it. Popups and windows also get a hairline border and a smaller shadow.
- The `text_on_primary`, `text_on_primary_hovered`, `icon_on_primary` and `icon_on_primary_hovered` tokens of `ListItemSemanticTokens` are white instead of the accent and near-black, the colour of content on an accent fill.
- `TradingChart` context menus follow the theme applied with `apply_to_egui` once there is one; the theme passed to `ChartBuilder::with_theme` only colours them before that.
- `ChartBuilder` no longer defaults the symbol to `BTCUSDT`: a chart built without `with_symbol` has an empty symbol, shows no symbol and does not subscribe to or fetch from its data source until `TradingChart::set_symbol` is called. Migration: call `with_symbol` to keep the previous behaviour.
- `SymbolInfo::default()` (in `ui::symbol_header`) has empty fields instead of AAPL, Apple Inc, NASDAQ and 1D. Migration: set the fields explicitly.
- Iceberg takes its tick size from `IcebergDetector::with_tick_size` instead of guessing it from the first 100 bars, and Liquidity no longer defaults to a tick size of 1.0. Without a tick size, both divide the series' price span into 1,000 levels; set the instrument's tick size to key levels on it.
- `CompareSymbolsDialog::show` takes the symbols to suggest (`show(ctx, &symbols)`, the same `&[model::Symbol]` that `SymbolSearchDialog::show` takes) and lists only those, with an empty state when there are none, instead of a built-in list of tickers the data source may not offer. Added symbols have an empty `CompareSymbol::exchange`, since `Symbol` carries no exchange. Migration: pass the symbols your data source offers.
- `ButtonVariant::bg_color`, `fg_color` and `border_color` resolve colours from the theme applied with `apply_to_egui` and the `Ui`'s visuals instead of fixed design tokens, and `apply_to_style` sets them on a `Ui` instead of a `Style`; `fg_color` returns a real colour for Ghost and Outlined instead of `Color32::PLACEHOLDER`. Migration: pass `ui` as the first argument (call `apply_to_style` inside `ui.scope`); `border_color` no longer takes `hovered`.
- `IconButton`, `SplitButton` and `Button` icons are 16, 20 or 24 px for small, medium and larger buttons (20 px at the default 28 px) instead of 60% of the button size; `icon_scale` still overrides this.
- `SplitButton` occupies the same square cell as an `IconButton` of its size, with the chevron in the cell's corner, instead of a cell 10 px wider.
- `dialog_close_button` is an `IconButton` showing `icons::CLOSE` with a "Close" tooltip.
- `DialogFrame`, `dialog_header`, `DialogFooter`, `Modal`, `ConfirmDialog`, `PromptDialog` and `ModalHandler` share one dialog chrome: the theme's window fill, border, radius and shadow with 16 px padding, a title row with a left-aligned 14 px title and an icon close button centred on it, and a footer whose buttons are 28 px high, 8 px apart, with a filled primary and an outlined secondary. The size passed to `DialogFrame::new` is the preferred content size inside the padding: the dialog is at least that large, grows with its content and is limited to the application window. A `DialogFooter` shown directly in a `DialogFrame::new` frame sits at the bottom of the frame, with any free space above its rule. Modals are centred and sized to their content, and `Modal::width` and `Modal::min_height` set the outer size including the padding (before, the content size); `ConfirmDialog` is cancelled by Escape, the close button or a backdrop click, and `ModalHandler` closes on Escape.

### Removed

- The unused `panel_bg`, `panel_border` and `header_bg` fields of `tokens::ModalSemanticTokens`. Migration: use the theme's `semantic.ui.panel_bg_floating` and `semantic.ui.border`.
- `ui_kit::buttons::button_bg` and `ui_kit::buttons::icon_color`, which had no pressed state. Migration: use `ButtonVariant::Ghost.bg_color(ui, hovered, pressed)`, or `ButtonVariant::Primary` for a selected button, and `ButtonVariant::fg_color`.
- The Volume Footprint and Session Volume chart types, which approximated per-price volume from bar totals, with their `icons::icons::CHART_VOLUME_FOOTPRINT` and `icons::icons::CHART_SESSION_VOLUME` icons, and the unused `model::footprint` module.
- The `chart::series_api` module (`ISeriesApi`, `IPriceScaleApi`, `ITimeScaleApi`, `IStudyApi`, their implementations and option types), which nothing constructed.
- `TradingChart::toggle_main_series_visibility` and the series context menu "Hide" entry (`SeriesContextMenuAction::Hide`), which had no effect.
- `DrawingManager::update_pos_prices`, `Drawing::format_timestamp`, and the `Drawing` fields `is_closed`, `curr_price`, `forecast_start_price`, `forecast_target_price`, `forecast_start_time`, and `forecast_target_time`; position and forecast results come from the chart's bars.
- Unused icon constants `AREA_CHART`, `LINE_CHART`, `ORDER_BOOK`, `TIME_AND_SALES`, `HIDE`, `RISK_REWARD_SHORT`, `TREND_BASED_FIB_EXTENSION`, `ZOOM_TOGGLE`, `EMOJI_EAGLE`, `EMOJI_ORNAMENT`, `WIDGET_BAR_NOTIFICATIONS`, `WATCHLIST_44`, `SCREENER_44`, `PINE_SCRIPT_44`, `COMMUNITY_44` and `PRODUCTS_44`. Migration: use `CHART_AREA`, `CHART_LINE`, `WATCHLIST`, `EYE_HIDE` (or `EYE` for the visible state), `SHORT_POSITION` and `FIB_EXTENSION`; the others have no replacement.
- The unused series renderers `chart::series::{LineSeries, AreaSeries, BaselineSeries, HistogramSeries, BarSeries}` and the `Series` trait with `SeriesData` and `SeriesType`, which the chart never drew through. No replacement; the chart renders its series from `ChartType`.
- The `model::enums` module of wire-format enums, which nothing used, with `ChartType::to_chart_style` and `From<ChartStyle> for ChartType`. Migration: use `scales::PriceScaleMode` for `PriceScaleMode`, `ChartType` for `ChartStyle`, and `drawings::LineStyle` for `LineStyle` (it has no large-dashed or sparse-dotted style); `CrosshairMode` and `ColorType` are unrelated to `config::CrosshairMode` and `model::ColorType` and, like `ChartMode`, `Timezone`, `MagnetMode`, `Orientation`, `Size`, `Resolution`, `ChartTheme`, `PriceLineSource`, `ScaleLocation`, `BackgroundType`, `OverrideLineStyle`, `OverridePriceAxisLastValueMode`, `ChartStyleParameters`, `ClearMarksMode`, `MarkLocation`, `StudyPlotType`, `StudyDisplayTarget`, `StudyInputType`, `StudyTargetPriceScale`, `LineStudyPlotStyle`, `OhlcStudyPlotStyle` and `FilledAreaType`, have no replacement.
- `ui::top_toolbar::components::symbol_search::popular_symbols`, a fixed list of index, crypto, commodity and forex tickers. Migration: pass your own symbols to `CompareSymbolsDialog::show`.
- The `templates` module. Migration: import `SettingsTemplate` and `TemplateManager` from `ui::model`.
- `CVDMode::TickSplit`, which computed the same estimate as `CVDMode::ClosePosition` while claiming footprint data; use `ClosePosition`. `CVD::with_histogram`, which had no effect, with no replacement.
- `StopRunIndicator::with_reversal_window`, which only lengthened the warm-up; the reversal is tested on the breaking bar. Remove the call.

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
