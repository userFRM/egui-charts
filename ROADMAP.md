# egui-charts Roadmap

[Feature status and progress](https://userfrm.github.io/egui-charts/docs/progress/)

The crate provides an engine without egui, an egui 0.37 chart widget, and terminal UI components. Checked items describe working capabilities, including public APIs that applications can compose. Unchecked items describe remaining library work. The status page, computed from [`docs/progress/features.csv`](docs/progress/features.csv), measures completion; this roadmap describes capabilities and the order of remaining work. `egui_charts_terminal::Terminal`, in the unpublished `terminal/` package, assembles components on the host's feed and services and lets the application intercept their actions. It performs chart, study and drawing workflows and returns actions it does not perform yet; the demo still routes the components by hand.

## Chart Rendering Engine

### Chart Types

`ChartType` offers 21 chart types, with the rendering limits listed below.

- [x] Candles, OHLC bars, hollow candles and volume candles, with configurable rising and falling colours.
- [x] Line, line with markers, step line, area, HLC area, baseline, columns and high-low charts; line and area charts accept a `PriceSource`.
- [x] Heikin-Ashi, Renko, Kagi, line break, point and figure, and range-bar transformations and rendering.
- [x] Renko box sizing and wicks, line-break lookback, and price-based chart parameters are configurable.
- [x] Market Profile (TPO) uses exchange sessions or calendar days, with letters or blocks, initial balance, point of control, value area and composite profiles.
- [x] Volume Footprint and Session Volume Profile aggregate supplied executions on time-based intervals with an instrument tick size; unsupported intervals and bars without retained trades use candles. Session profiles require `studies`.
- [x] Kagi lines change thickness at shoulder and waist crossings within a continuing leg.
- [x] Point and figure accepts close-only or high/low input through chart parameters and settings.
- [x] Range charts build their bars from the supplied executions, falling back to the OHLC approximation while the chart holds none; `Chart::range_bar_source` reports which one is shown.
- [x] Volume-candle widths follow their share of the largest visible volume, with a wick-width floor.
- [x] HLC fill opacity and close-line width, and Kagi stroke widths, are editable in series settings.
- [x] TPO single-print highlights can be shown or hidden on the chart and study.

### Pan, Zoom and Cursor

- [x] Cursor-centred wheel zoom, horizontal pan, drag pan, touch pan and pinch zoom.
- [x] Touch drags can start kinetic scrolling with configurable damping.
- [x] Axis drag scaling, double-click reset, box zoom and optional chart-owned navigation buttons.
- [x] Cross, dot and arrow cursor modes, styled crosshair lines, price/time labels and OHLC snapping.
- [x] Main-pane crosshair snapping includes visible studies and comparison series through their own scales on time-based charts.
- [x] Hover readouts and the data window show bar and study values.
- [x] Mouse drags can start kinetic scrolling independently of touch inertia.
- [ ] Include studies and comparison series in main-pane crosshair snapping on price-based chart types.
- [x] Box-zoom overlays use translucent theme colours.
- [x] Floating, held and tracking readouts identify the instrument by symbol.
- [x] `Chart::data_table` and `DataTableDialog` expose loaded bars and study values, with column selection, copy and CSV export intents.

### Scales and Panes

- [x] Linear, logarithmic, percentage, indexed-to-100 and inverted price scales, autoscale, manual ranges and scale options.
- [x] Time-axis tick hierarchy, price marks, exchange time zones, session breaks and extended-hours shading.
- [x] Chart-wide time, price, percentage and volume formatters, plus per-series price formatters.
- [x] Locale-aware date names and configurable date patterns.
- [x] Chart-owned panes share the time axis and support resizing, reordering, stretch sizing, collapse and maximise controls.
- [x] Studies and added series can use separate panes and independent scales.
- [x] Scale settings control currency and unit labels, scale-mode and price-action buttons, price-to-bar ratio, label overlap and symbol, study and extended-session value colours.
- [x] Maximised study panes retain pan, wheel, pinch and axis scaling; double-click toggles pane maximisation.
- [x] Main-series-only autoscale and the active left/right axis scale-mode configuration apply to the chart.
- [x] Added-series value tags use their scale's precision unless a host formatter overrides it.
- [x] Symbol and study names can independently prefix their price-scale value tags.
- [x] Scale font size, scale-line colour and pane-button visibility settings apply and persist.

## Drawing Tools

### Geometry and Annotations

- [x] Trend lines, rays, extended lines, horizontal and vertical levels, cross lines, trend angle and info line.
- [x] Parallel, flat-top/bottom, disjoint and regression channels; regression uses the selected closes and configurable deviation bands.
- [x] Andrews, Schiff, modified Schiff and inside pitchforks, with configurable levels and fills; three-anchor pitchfans.
- [x] Fibonacci retracement, three-anchor extension and channel, time zones, circles, arcs, speed fan, spiral, wedge and trend-based time projections.
- [x] Gann fan, square, box and fixed-square drawings, with editable grid settings where supported.
- [x] XABCD, ABCD, Cypher and head-and-shoulders drawings, with labels and price ratios or a neckline as applicable.
- [x] Triangle and three-drives patterns accept their required anchors and render labelled paths.
- [x] Elliott impulse, correction, triangle and combination drawings render plain labelled legs with a selectable wave degree.
- [x] Cyclic lines, repeating time cycles and sine lines.
- [x] Long/short position drawings expose entry, stop, target, instrument-aware risk sizing and bar-based profit/loss; forecasts, copied bars and three-anchor measured projections.
- [x] Anchored VWAP with selectable deviation bands; fixed and anchored volume-profile drawings use complete executions, then available finer bars, then chart-bar estimates, and identify their source.
- [x] Price, date and combined range measurements use actual bar timestamps; quick measure also reports volume and instrument ticks/pips when supplied.
- [x] Brush, highlighter, arrows, rectangles, rotated rectangles, circles, ellipses, triangles, arcs, sectors, polylines, paths and quadratic/cubic curves.
- [x] Editable text, notes, callouts, price labels, comments, signposts and tables with persistent cell contents.
- [x] Image annotations accept a chosen or dropped file through host file handling and egui image loaders, with corner resizing; post and idea cards retain linked content resolved by the host's `Posts` service.
- [x] Searchable bundled icons, emoji and stickers with size controls and recent selections.
- [x] Fibonacci retracement and extension fills share their level lines' horizontal span; channel levels extend to the pane edges.
- [x] Triangle patterns build two boundaries towards their apex and fill only the enclosed region.
- [x] Three-drives drawings label each drive's ratio to its preceding correction, measured in price.
- [x] Elliott triangles collect an origin and waves A to E.
- [x] Ghost Feed projects candles along its clicked path with configurable spacing and wicks.
- [x] Fixed and anchored profile drawings request the feed's suggested finer bars and use supplied executions on every chart type.
- [x] Gann-square grid, fan and arcs have independent visibility and configurable ratios and fan direction.

### Interaction, Properties and Persistence

- [x] Fixed-point placement keeps clicked anchors; open paths finish with Enter, double-click or Escape, and polylines can close on their first point.
- [x] Selection uses rendered strokes, fills and text, with geometry and anchor fallbacks before painting.
- [x] Anchor handles, dragging, OHLC/study/drawing magnet snapping, repeated placement, locking and visibility controls.
- [x] Undoable drawing creation, edits, deletion, ordering, nested folders and moves between panes.
- [x] Properties edit style, tool inputs, text, timeframe visibility and validated price/time coordinates.
- [x] Public timestamp-based creation and anchor editing, drawing enumeration, per-tool defaults and custom drawing registration.
- [x] Per-drawing selection, saving and object-list flags; drawing change notifications and registry availability controls.
- [x] Versioned drawing documents retain timestamp anchors, settings and nested folders, with migration and typed skipped-record reporting.
- [x] `Chart::set_storage` loads and saves drawings per symbol and retains failed writes for retry.
- [x] Drawing templates save, apply and delete reusable styles through `TemplateStore` and `Storage`.
- [x] Demonstration cursor with a fading pointer trail.
- [x] The object tree creates, renames, reparents, orders, locks, hides and removes nested drawing folders.
- [x] Sloping drawing alert levels agree with logarithmic rendering, and parallel, disjoint and flat top/bottom channels expose both lines.
- [x] Magic cursor draws a fading trail while the primary button is held, independently of Demonstration mode and without panning.

## Technical Indicators

### Calculations and Rendering

- [x] `IndicatorFactory` registers 204 indicators with default features and accepts downstream registrations; the study picker uses that registry.
- [x] Moving-average families include SMA, EMA, WMA, HMA, ALMA, KAMA and configurable combinations, ribbons and crossing markers.
- [x] Momentum and trend studies include RSI, MACD, Stochastic, ADX, SuperTrend, Ichimoku and Parabolic SAR.
- [x] Volatility and channel studies include Bollinger Bands, ATR, Keltner, Donchian and historical volatility.
- [x] Volume studies include OBV, MFI, accumulation/distribution and VWAP; VWAP supports calendar/session and supplied event anchors with deviation or percentage bands.
- [x] Visible, fixed, session, periodic, anchored and auto-anchored volume profiles render chart-bar volume estimates with point of control and value area.
- [x] Regression, rank correlation, pivot levels, ZigZag, candlestick patterns, seasonality and market-structure studies.
- [x] Majority Rule reports +1, -1 or zero from the majority of rising versus falling closes over its lookback.
- [x] Automatic trend-line, channel, Fibonacci and pitchfork studies use ZigZag pivots with the `drawings` feature.
- [x] Shared renderers support lines, steps, histograms, columns, dots, crosses, areas, shapes, bar/background colours and filled regions.
- [x] MACD histogram columns, displaced Ichimoku cloud fills into visible future space, and separate Parabolic SAR dots.
- [x] RSI and its divergence calculations handle very small price changes without a price-unit floor; gain-only, loss-only and flat windows return 100, 0 and 50.
- [x] Connors RSI uses the same defined endpoints for its price and streak components.
- [x] Both Tasuki Gap detectors require the third candle to open inside the second body and close strictly inside the gap.
- [x] Woodies CCI uses closes and colours its columns by neutral, transition and established runs on one side of zero.
- [x] PPO, PVO and Net Volume default their histograms to columns; PPO/PVO oscillator and signal averages independently choose simple or exponential smoothing.
- [x] Bollinger Bands offer simple, exponential, weighted, smoothed and volume-weighted bases; BandWidth exposes expansion/contraction lookbacks.
- [x] Applied VWAP settings edit band multipliers and event dates in a chosen time zone and retain them in templates.
- [x] Request finer profile data, from a feed's suggestion or a chosen interval, and adapt session profile rows to the zoom.
- [x] Two-symbol spread, ratio and Pearson correlation studies align supplied histories; correlation also accepts log returns and leaves incomplete or undefined windows empty.
- [x] Trend Strength Index plots rolling price-to-bar-position correlation in its own pane.
- [x] Automatic continuation, reversal, wedge and Elliott-impulse studies distinguish developing and confirmed patterns; Key Levels clusters confirmed swing pivots.
- [x] Technical Ratings combines documented moving-average and oscillator votes with coverage; Lunar Phases marks new and full moons from 1900 to 2100.
- [ ] Add a dedicated volatility-index study.
- [x] ADX Breakout, Knoxville Divergence, Reversal, Intraday Pivot Points, Projected Pivot Points and Missed Pivot Levels are registered studies with documented rules.
- [ ] Add a dedicated Ziv Ghost Pivots study.

### Study API and Editing

- [x] `Indicator` and `IndicatorRegistry` calculate without egui; `CustomIndicator` accepts downstream calculation code.
- [x] Typed inputs with validation, mutable settings, plot styles, fills, offsets, precision, visibility and independent scale metadata.
- [x] Additional-symbol and additional-interval histories, source selection and confirmed/developing value alignment.
- [x] Incremental updates or scheduled recalculation as chart data changes.
- [x] Applied-study editor with Inputs, Style and Visibility pages and atomic validated changes.
- [x] Study templates capture inputs and presentation; built-in studies reconstruct through `StudyTemplate`.
- [x] OHLC-based and source-aware studies offer interval and wait-for-close inputs; studies with their own data requests keep their own inputs.
- [x] The Style editor lists every plot type and explains choices unavailable for the current pane or scale.
- [x] A per-chart study limit rejects additions through `try_add_indicator`, paste and the picker, with feedback; direct `add_indicator` remains unrestricted.
- [x] Chart study-template operations restore registered custom studies and remap study-source dependencies through a supplied factory.
- [x] `StudyEditor` edits a document and shows selectable diagnostics, with open, save and apply intents; the host supplies the language, files and compilation.

## Order Flow

Execution-based calculations use supplied trades and their aggressor side. Unknown-side volume remains separate; chart-bar volume estimates do not reconstruct actual executions.

### Footprints, Delta and Profiles

- [x] `Footprint` aggregates bid, ask and unknown volume by bar and integer tick row, with execution-size filtering.
- [x] Footprint cells support bid/ask, total volume, delta and delta-plus-volume modes, up to four columns per bar, profile or boxed rendering and automatic row grouping.
- [x] Per-bar point of control, value area and horizontal/diagonal imbalance highlighting.
- [x] Public calculations and renderers expose stacked diagonal imbalance zones, unrevisited POC levels, volume clusters and per-bar statistics.
- [x] Execution-based delta histograms and cumulative-delta candles, including intrabar extremes and configurable reset boundaries, render in a supplied pane mapping.
- [x] Trade profiles aggregate supplied ranges with bid/ask, volume or delta display, developing POC, value area and local volume nodes; public range editing supports merging and splitting.
- [x] Feed footprints discard evicted trade contributions and bound retained bars and price rows; incomplete bars are identified instead of presented as complete executions.
- [x] Size-Filtered CVD, Execution Delta Candles and Execution Cumulative Delta Candles use study panes with their own scales; cumulative outputs offer session and calendar resets.
- [x] Multi-Range Delta and Rolling Volume and Delta combine size ranges or rolling sums of supplied executions.
- [x] Footprint studies highlight delta-percentage and bid/ask-ratio thresholds and support a minimum delta POC.
- [x] Footprint cells support fixed, delta-fade and dominant-side colours, abbreviated values, zero visibility and value-scaled text.
- [x] Footprints render extreme bid/ask-ratio annotations and pullback highlights.
- [x] Horizontal or diagonal imbalance zones have configurable expiry and unvisited, revisited or expired state; rejection detection remains incomplete.
- [x] Unfinished-auction levels use the extreme-side tests and extend beyond their originating bar.
- [x] Execution profiles draw independently styled VAH/VAL guides; node calculations expose smoothing, prominence and empty-row valleys.

### Depth, Tape and Heatmap

- [x] Sequenced aggregated-depth snapshots/deltas and per-order add, modify, cancel and execute events, with gap detection and resynchronisation.
- [x] Public order book and order queue retain tick-aware prices, best quotes and queue priority.
- [x] `DepthHistory` retains sampled resting-size columns in bounded tiers; the heatmap uses retained texture tiles and partial updates.
- [x] Heatmap best bid/ask paths and a side profile of total resting size.
- [x] Volume bubbles aggregate by chart bar and price, with side/delta colours, linear/square-root/logarithmic sizing and outlier limits.
- [x] Time-and-sales panel with aggressor colours, size filters and sound intents.
- [x] Explicit aggressor identifiers combine split fills into parent orders with total size, price span and fill count.
- [x] Fixed stream studies calculate quoted spread, resting book totals/difference, execution rate and inside-quote movement.
- [x] Bubbles group by chart bar, time window, price step or execution count, or retain individual execution positions, with split bid/ask and delta/volume colour modes.
- [x] Ladder view controls expose observed size changes, traded bid/ask volume and supplied order counts as columns.
- [x] Ladder profiles offer total, delta and bid/ask modes over retained trades, the exchange session or a manual reset.
- [x] The heatmap side profile can show the largest individual order; configurable size tiers highlight resting orders in the profile, ladder and heatmap cells.
- [x] Heatmap sampling accepts aggregate depth or filtered individual orders, including supplied participant exclusions; unavailable order metadata is reported.
- [x] A heatmap benchmark and documented budget cover 1,000 levels per side at 50 ms, with bounded retention and texture updates.
- [x] Book totals filter fresh or persistent orders by supplied placement times; Levels Executed counts executed prices and can reset on price reversal.
- [x] Aggressor Rate counts explicit parent-order identifiers; reconstructed tape exposes supplied consumed passive-order details and their completeness.
- [x] Custom studies request time-aligned footprint and book inputs through `Indicator::order_flow_inputs`, with bounded retention and replay coverage metadata; these raw inputs currently require time-based intervals.

## Trading and Strategy Reports

- [x] `Trading` connects order tickets, account tables and chart overlays to a supplied `Broker`.
- [x] Market, limit, stop and stop-limit tickets, units/value sizing, market-order risk sizing, time in force, presets and attached stop-loss/take-profit levels.
- [x] Validated one-click orders, chart context-menu entry and broker/account selection components.
- [x] Order and position lines show levels and profit/loss; confirmed actions modify/cancel orders, close/reverse positions and edit brackets.
- [x] A depth ladder supports order entry, drag modification and right-click cancellation through the broker interface.
- [x] Account summary, positions, orders, history, executions, balance and journal tables, with CSV export.
- [x] Execution markers, rejection/fill notifications and execution sound intents.
- [x] `StrategyReportPanel` accepts `StrategyReport` data and shows trade lists, equity/drawdown plots, performance metrics, supplied properties and chart navigation; trades export as CSV.
- [x] Risk-percent sizing applies to market, limit, stop and stop-limit tickets; ladder entries validate the ticket's attached exits at their own entry price.
- [x] Strategy reports export the displayed performance summary and supplied properties as CSV, with units and unavailable values.

## Alerts

- [x] Typed `Alert` conditions model price crossings/comparisons, moves over a bar span, study plots, supported drawing levels, watchlists and multiple conditions.
- [x] Shared creation/editing dialog validates conditions, name, message, frequency and expiry; presets use storage.
- [x] Toolbar, context-menu, price-scale and keyboard entry points, plus draggable chart alert levels.
- [x] `AlertsPanel` uses `services::Alerts` to list, edit, pause/enable and remove alerts, and to show supplied trigger events in toasts and an exportable log.
- [x] The alerts panel sorts by name, symbol or status in either direction and duplicates an alert as a new draft.
- [x] Inline alert creation reuses validated dialog conditions; series settings control alert-level visibility, labels and colour per series.
- [x] Supported channel drawings expose both boundary lines as alert sources through the active price scale.
- [x] Alert models include persistent inside/outside-channel conditions and once-per-minute frequency.
- [x] Alert messages support bar-value placeholders, and notification schedules carry a time zone, weekdays and local time windows for service processing.

## UI Components

### Toolbars, Dialogs and Panels

- [x] Top toolbar with symbol search, comparison, interval/type selection, study, alert, replay, history, settings, save and layout actions.
- [x] Symbol search supports text, type, exchange and source filters, pagination, keyboard navigation and host results; comparisons use shared percentage scales, separate scales or panes.
- [x] Timeframe toolbar provides range presets, go-to-date, time zone, regular/extended session and adjustment requests.
- [x] Drawing toolbar categories, cursor modes, snapping, repeated placement, locking and template actions; favourites can be stored.
- [x] Chart settings apply backgrounds, grids, candle parts and colours, previous-close colouring, crosshair styling, session shading and watermarks.
- [x] Status settings control supplied logos, last day change and study backdrops; Symbol settings request dividend-adjusted history from the host.
- [x] Object tree supports selection, filtering, visibility, locks, ordering, rename, duplication, pane moves and bulk actions; a data window shows OHLCV and study values.
- [x] Editable shortcut bindings are shared by chart input, help and menu hints; quick search uses a command palette.
- [x] Watchlists support named lists, sections, sorting, row reordering, flags, symbol navigation and text import/export.
- [x] Feed-backed panels render symbol details, news, calendars, hotlists, screener tables and grouped market heatmaps; `FinancialsPanel` charts and tabulates supplied financial figures.
- [ ] Complete assembled terminal workflows for intervals, comparisons, layouts, replay, alerts, trading and feed-backed panels. `Terminal` already performs chart type, theme, undo/redo, fullscreen, symbol search, study and chart settings, drawing tools, selection controls and object-tree actions; it supports action interception and returns unhandled actions to the host.
- [ ] Provide a shared docked tab container for account, strategy report, study editor and replay-trading surfaces.
- [x] The drawing toolbar exposes its colour popup and a draggable favourite-tools strip with saved visibility and position.
- [x] The terminal persists and restores indicator favourites through user settings.
- [x] The floating selection toolbar follows the drawing, applies undoable colour and stroke edits and returns drawing-menu actions; the terminal routes settings, duplicate, lock, visibility and delete.
- [ ] Connect the standalone quick settings menu, its scale-settings action and all of its configuration fields.
- [x] The terminal handles the series context menu's Copy price action at the instrument's precision.
- [x] Default shortcuts route comma/digit interval entry and Alt+Shift+R rectangle selection; Alt+Enter maximises the selected tile in a multi-chart layout.
- [x] Main-symbol legend controls hide, show, configure or remove the main series.
- [x] Watchlists offer duplication, flag-filtered lists, chart-menu insertion and a wide view with host-supplied earnings, dividends and news.

### Replay

- [x] Bar replay hides future main-series bars, supports start-bar/date/random selection, stepping, autoplay, speed and seek, and restores live data on exit.
- [x] Main-series studies recalculate against replayed bars and drawing anchors remain stable.
- [x] Floating replay controls, keyboard shortcuts, saved replay sessions and synchronised layout replay are exercised by the demo.
- [x] `ReplayClock` merges timestamped trades, quotes and depth events across symbols, supports speed and seek, and routes through normal sinks.
- [x] `RecordingWriter` and `RecordingReader` encode/decode versioned trade and depth events through supplied I/O streams.
- [x] Additional-symbol/interval study histories stop at bars completed by the replay cutoff and restore their live histories on exit.
- [ ] Connect the standalone `ReplayToolbar` actions to the replay controller.

### Export

- [x] `Chart::export_csv` exports OHLCV and study values with timestamps and missing-value handling.
- [x] `Chart::export_png` and `export_svg` render an isolated chart frame after font initialisation; export controls include size, title, timestamp, watermark, crosshair and transparency.
- [x] Export dialog and demo actions save images/data or copy images through application facilities.
- [x] Layout image/PNG/SVG exports capture library panels, trading overlays and feed-backed order flow through a host drawing closure. Host textures require supplied pixels; unsupported callbacks and SVG content return named errors.
- [ ] Add print/PDF output.
- [ ] Integrate a native save-file chooser and provide an `Exportable` adapter for direct chart export.

## Layouts and Persistence

- [x] `ChartLayout` arranges up to 16 charts in grids or tabs, tracks the active chart, supports detached views and groups, and synchronises selected symbol, interval, time range and crosshair time and main-pane price.
- [x] Main-pane drawings synchronise between grouped charts by timestamp.
- [x] `ChartSnapshot` saves symbol, interval, configuration, studies, panes, scales, viewport, drawings, added bar-series options, markers and price lines. A supplied factory reconstructs custom studies and remaps source links; the host reloads added-series bars through restored identifiers and reattaches services and formatters.
- [x] Layout menus create, copy, rename, list, save, open and delete layouts, with debounced autosave through supplied storage.
- [x] Chart, study and drawing template catalogues support named save, replace, apply and delete operations.
- [x] `SettingsStore` persists preferences with debouncing, failed-write retry and error reporting.
- [ ] Preserve custom study-backed series in chart snapshots as well as added bar series.
- [x] Layout favourites filter the picker and follow rename/delete, with persistence through user settings.
- [x] Crosshair synchronisation includes main-pane price; hovering a study pane shares time only.
- [x] Equivalent study panes map drawings between charts; mirrored changes, unlocking and deletion reach locally locked receivers.
- [x] User actions record added-series membership/options and symbol navigation in chart undo/redo; chart events report series membership changes for host subscription management.

## Data and Builder API

- [x] `Chart::new` embeds static bars; `ChartBuilder` configures a chart, feed, symbol, interval, studies, theme and drawing registry.
- [x] `Datafeed`, `Reply` and `Sink` provide asynchronous metadata/history and streaming bars, executions, quotes and depth, with stale-response rejection and subscription cancellation.
- [x] Scroll-back pages bar history; transient failures retry with bounded backoff and expose errors and status through `FeedState`.
- [x] Shared validation sorts/deduplicates bars, rejects invalid OHLCV and counts rejected updates; out-of-order corrections update the affected history.
- [x] Live trade aggregation supports time, execution-count, volume, delta and range intervals; resampling helpers accept session rules.
- [x] Optional per-series bar limits bound retained history; feed queues coalesce updates and bound pending trades/depth.
- [x] Runtime symbol/interval changes reload the feed; explicit reload and reset paths invalidate old work.
- [x] Public events report pointer, range, symbol, interval and data changes; custom menu entries and toolbar controls return application identifiers.
- [x] Added price series, custom study-backed series and attached `Primitive` renderers support visibility, hit testing, scale participation and removal.
- [x] A symbol change resets a stale manual price range to autoscale.
- [x] `ChartBuilder` accepts locale, formatters, storage and a study factory; `build_restored` validates and applies a saved snapshot before returning the chart.
- [x] Bar, trade and depth history requests carry bounded time ranges and gap-continuation cursors; order-flow scroll-back pages older trades and reports completeness.

## Infrastructure

- [x] Feature layers separate the engine, built-in studies, widget, drawings, icons and terminal UI; engine and study calculations need no egui.
- [x] `Theme` provides editable palettes with dark, light and high-contrast presets; shared tokens cover sizing, spacing, corners and colours.
- [x] Host translation lookup with English fallback, locale/date formatters and optional named time zones.
- [x] Accessibility roles/states, keyboard focus and a textual bar/study readout.
- [x] Reusable `ui_kit` buttons, inputs, colour pickers, dialogs, lists, tabs, panels, command palette and toast components.
- [x] Native and browser examples and CI checks cover library, feature combinations and demo builds.
- [ ] Route remaining direct English UI labels through the translation lookup; menus, trading prompts, toasts and chart readouts already use it.
- [x] Session-quote and volume tags, marker outlines and event breaks follow the active theme.
- [ ] Add a runnable desktop-webview integration example.

## Provided by the host

The crate renders supplied data and exposes interfaces for application services. The application supplies market data, broker execution, accounts and durable storage. These interfaces also allow services to run remotely.

- `Datafeed`, `SymbolSearch`, `Quotes` and `Marks` connect instrument discovery, history, live prices, executions, depth and chart events. Feed queries also supply metrics, options volume, analyst targets, symbol details, news, calendars, screeners, hotlists and market heatmaps.
- `Broker` supplies real or paper-account state, trading rules, execution and account updates. Trading components submit actions and render the reported orders, positions, fills and history.
- `Storage` supplies layouts, drawings, settings and template documents; an application may use local or remote storage and manage accounts, synchronisation and sharing.
- `Alerts` stores and evaluates alert conditions and reports changes and triggers. The crate edits models and draws levels, dialogs, logs and toasts; the application supplies sound, schedules and external delivery.
- `Signals` supplies markers, levels and zones with category visibility and click callbacks. Applications can deliver iceberg, absorption, spoofing, large-order and other interpreted order-flow events through it.
- `Notifications` and `Chats` supply notification records and conversation messages. `Posts` resolves linked post/idea drawings, and `Sharing` supplies links and recipient copy policies. `IdeasPanel` emits publishing and browsing intents; accounts, delivery and durable storage stay with the application.
- `StrategyReport` supplies the trade list, equity and performance inputs for the report panel; `CompanyFinancials` supplies financial tables. Applications also provide translations, additional fonts, image loaders and platform save/clipboard facilities.

## Next

The order below prioritises correctness and complete library workflows. It carries no delivery dates.

1. **Terminal workflows:** finish the assembled terminal's remaining service and action handling, the shared bottom dock and replay-toolbar routing; connect the standalone quick settings menu.
2. **State and chart interaction:** preserve custom study-backed series in snapshots and extend crosshair snapping to studies and comparisons on price-based chart types.
3. **Studies:** add the dedicated volatility-index and Ziv Ghost Pivots studies that remain absent from the catalogue.
4. **Export and integration:** provide print/PDF output, native save-file selection and a direct chart `Exportable` adapter; finish translation coverage and the desktop-webview example.
