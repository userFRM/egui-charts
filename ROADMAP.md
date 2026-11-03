# egui-charts Roadmap

[Feature status and progress](https://userfrm.github.io/egui-charts/docs/progress/)

The crate provides an engine without egui, an egui 0.37 chart widget, and terminal UI components. Checked items describe working capabilities, including public APIs that applications can compose. Unchecked items describe remaining library work. `ui::terminal::Terminal` assembles the components on the host's feed and services and lets the application intercept their actions; it returns those it does not perform yet, and the demo still routes the components by hand.

## Chart Rendering Engine

### Chart Types

`ChartType` offers 21 chart types, with the rendering limits listed below.

- [x] Candles, OHLC bars, hollow candles and volume candles, with configurable rising and falling colours.
- [x] Line, line with markers, step line, area, HLC area, baseline, columns and high-low charts; line and area charts accept a `PriceSource`. Baseline fills accept independent colours at the pane edge and the base on each price side, including point overrides, while older settings keep flat fills.
- [x] Main-series conflation has a configurable physical-pixel threshold or can be disabled. `Chart::prepare_conflation` prepares the shared render cache before a dense view; groups retain price extremes, gaps and per-bar colours and refresh after data, price-source or grouping changes.
- [x] Heikin-Ashi, Renko, Kagi, line break, point and figure, and range-bar transformations and rendering.
- [x] Renko box sizing and wicks, line-break lookback, and price-based chart parameters are configurable.
- [x] Market Profile (TPO) uses exchange sessions or calendar days, with letters or blocks, initial balance, point of control, value area and composite profiles; autoscale fits the full displayed distributions and cropped profiles retain their session and central-level readouts. POC, value-area and midpoint captions keep a gutter at the first or offscreen session opening, separate nearby levels and reveal their prices on hover.
- [x] Volume Footprint and Session Volume Profile aggregate supplied executions on time-based intervals with an instrument tick size; unsupported intervals and bars without retained trades use candles. Session profiles require `studies`; autoscale includes their full price rows after range edits, and manual crops retain session and POC/value-area readouts.
- [ ] Change Kagi line thickness at shoulder and waist crossings within a continuing leg.
- [ ] Expose the standalone point-and-figure close-only source in chart parameters and settings.
- [x] Range charts build their bars from the supplied executions, falling back to the OHLC approximation while the chart holds none; `Chart::range_bar_source` reports which one is shown.
- [ ] Offer proportional volume-candle widths without the current minimum width.
- [ ] Expose HLC fill opacity and close-line width, and Kagi stroke widths.
- [ ] Render the TPO single-print highlights already calculated by the engine.

### Pan, Zoom and Cursor

- [x] Cursor-centred wheel zoom, horizontal pan, drag pan, touch pan and pinch zoom.
- [x] Touch drags can start kinetic scrolling with configurable damping.
- [x] Axis drag scaling, double-click reset, box zoom and optional chart-owned navigation buttons.
- [x] Cross, dot and arrow cursor modes, styled crosshair lines, price/time labels and OHLC snapping. Independent time-scale filters exclude whitespace and hidden-only values from crosshair indices without removing stored data; visible series and study values keep their slots eligible.
- [x] Main-pane crosshair snapping includes visible studies on the main scale for time-based charts.
- [x] Hover readouts and the data window show bar and study values.
- [ ] Allow mouse drags to start kinetic scrolling.
- [ ] Include comparison series and price-based chart studies in main-pane crosshair snapping.
- [ ] Correct box-zoom overlay alpha and use theme colours.
- [ ] Add the instrument title to the floating historical readout.
- [ ] Provide a table view of loaded bars and study values.

### Scales and Panes

- [x] Linear, logarithmic, percentage, indexed-to-100 and inverted price scales, autoscale, manual ranges and scale options. Marker autoscaling reserves room for shapes, outlines and labels of visible markers positioned from bars or prices at their configured size on main and secondary panes; hidden markers and disabled autoscaling add no room.
- [x] Time-axis tick hierarchy, price marks that default to the instrument tick grid with host overrides, exchange time zones, session breaks and extended-hours shading.
- [x] Chart-wide time, price, percentage and volume formatters, plus per-series price formatters. Saved series styles declare price, abbreviated volume or raw percentage display with precision and minimum movement, independently of scale transforms. Explicit series and chart price formatters take precedence; a shared axis takes tick settings from its first visible series, and each series retains its own tags and readouts. Time tick hooks receive the current locale and display-time context and can defer individual ticks to the normal locale-aware text without changing crosshair formatting. Drawing prices and price changes use their pane's raw-price formatter, including measurements, forecasts, projections, position levels and rectangle heights.
- [x] Locale-aware date names and configurable date patterns. Forecast dates use the chart's display time zone and crosshair time formatter, including host formatting and clock visibility.
- [x] Chart-owned panes share the time axis and support resizing, reordering, stretch sizing, collapse and maximise controls.
- [x] Studies and added series can use separate panes and independent scales. The demo menu's Samples entries open synthetic strike and maturity charts with numeric crosshair readouts and preserve the price workspace on return.
- [x] Scale settings control currency and unit labels, scale-mode and price-action buttons, price-to-bar ratio, label overlap and symbol, study and extended-session value colours.
- [ ] Forward pan, wheel and pinch input to maximised study panes and support pane maximisation by double-click.
- [ ] Apply the main-series-only autoscale quick setting and the left-axis scale-mode configuration field.
- [x] Added-series value tags use configured scale precision when no formatter is supplied; saved numeric display declarations also control their tags and readouts.
- [ ] Draw independent symbol-name and study-name tags on the price scale.
- [ ] Apply scale font-size, scale-line colour and pane-button visibility settings.

## Drawing Tools

### Geometry and Annotations

- [x] Trend lines, rays, extended lines, horizontal and vertical levels, cross lines, trend angle and info line.
- [x] Parallel, flat-top/bottom, disjoint and regression channels; regression uses the selected closes and configurable deviation bands.
- [x] Andrews, Schiff, modified Schiff and inside pitchforks, with configurable levels and fills; three-anchor pitchfans.
- [x] Fibonacci retracement, three-anchor extension and channel, time zones, circles, arcs, speed fan, spiral, wedge and trend-based time projections.
- [x] Gann fan, square, box and fixed-square drawings, with editable grid settings where supported.
- [x] XABCD, ABCD, Cypher and head-and-shoulders drawings, with labels and price ratios or neckline projections as applicable.
- [x] Triangle and three-drives patterns accept their required anchors and render labelled paths.
- [x] Elliott impulse, correction, triangle and combination drawings render labelled waves.
- [x] Cyclic lines, repeating time cycles and sine lines.
- [x] Long/short position drawings expose entry, stop, target, instrument-aware risk sizing and bar-based profit/loss; forecasts, copied bars and three-anchor measured projections.
- [x] Anchored VWAP with deviation bands; fixed and anchored volume-profile drawings estimate distributions from chart bars.
- [x] Price, date and combined range measurements use actual bar timestamps; quick measure also reports volume and instrument ticks/pips when supplied.
- [x] Brush, highlighter, arrows, rectangles, rotated rectangles, circles, ellipses, triangles, arcs, sectors, polylines, paths and quadratic/cubic curves. Curved outlines and analysis arcs adapt to physical pixels and refine only near the view; freehand strokes interpolate their samples with centripetal Catmull-Rom curves, with matching selection outlines.
- [x] Editable text, notes, callouts, price labels, comments, signposts and tables with persistent cell contents.
- [x] Image annotations load an address through application-installed egui image loaders; local post and idea cards store editable supplied content.
- [x] Searchable bundled icons, emoji and stickers with size controls and recent selections.
- [x] Fibonacci retracement and extension fills share their level lines' horizontal span; channel levels extend to the pane edges.
- [ ] Build the triangle pattern's two boundaries and apex, and correct its polygon fill.
- [x] Three-drives ratios and the reversal zone are measured in price.
- [x] Elliott impulse targets project the first price leg in its own direction, independent of zoom and scale.
- [x] Elliott triangles collect an origin and waves A to E.
- [ ] Render projected bars for Ghost Feed, which currently draws a faded path.
- [ ] Connect fixed and anchored profile drawings to finer bar data or execution-based profiles.
- [ ] Expose configurable Gann-square fan and arc construction and visibility.

### Interaction, Properties and Persistence

- [x] Fixed-point placement keeps clicked anchors; free paths finish with Enter or double-click.
- [x] Selection uses rendered strokes, fills and text, with geometry and anchor fallbacks before painting.
- [x] Anchor handles, dragging, OHLC/study/drawing magnet snapping, repeated placement, locking and visibility controls.
- [x] Undoable drawing creation, edits, deletion, ordering, flat groups and moves between panes.
- [x] Properties edit style, tool inputs, text, timeframe visibility and validated price/time coordinates.
- [x] Public timestamp-based creation and anchor editing, drawing enumeration, per-tool defaults and custom drawing registration.
- [x] Per-drawing selection, saving and object-list flags; drawing change notifications and registry availability controls.
- [x] Versioned drawing documents retain timestamp anchors, settings and flat groups, with migration and skipped-record reporting.
- [x] `Chart::set_storage` loads and saves drawings per symbol and retains failed writes for retry.
- [x] Drawing templates save, apply and delete reusable styles through `TemplateStore` and `Storage`.
- [x] Demonstration cursor with a fading pointer trail.
- [ ] Add nested drawing folders and folder management to the object tree.
- [x] Sloping drawing alert levels agree with logarithmic rendering, and parallel, disjoint and flat top/bottom channels expose both lines.
- [ ] Add a separate magic cursor mode.

## Technical Indicators

### Calculations and Rendering

- [x] `IndicatorFactory` registers 204 indicators with default features and accepts downstream registrations; the study picker groups its built-in results on Indicators, Profiles, Patterns and Order flow pages, each offered only while it has an accepted study.
- [x] Moving-average families include SMA, EMA, WMA, HMA, ALMA, KAMA and configurable combinations, ribbons and crossing markers.
- [x] Momentum and trend studies include RSI, MACD, Stochastic, ADX, SuperTrend, Ichimoku and Parabolic SAR.
- [x] Volatility and channel studies include Bollinger Bands, ATR, Keltner, Donchian and historical volatility.
- [x] Volume studies include OBV, MFI, accumulation/distribution and VWAP; VWAP supports calendar/session and supplied event anchors with deviation or percentage bands.
- [x] Visible, fixed, session, periodic, anchored and auto-anchored volume profiles sum supplied executions in whole-tick rows, mark incomplete spans and fall back to finer-bar or chart-bar estimates, with translucent rows that keep candles readable, point of control and value area.
- [x] Regression, rank correlation, pivot levels, ZigZag, candlestick patterns, seasonality and market-structure studies.
- [x] Majority Rule reports +1, -1 or zero from the majority of rising versus falling closes over its lookback.
- [x] Automatic trend-line, channel, Fibonacci and pitchfork studies use ZigZag pivots with the `drawings` feature.
- [x] Shared renderers support lines, steps, histograms, columns, dots, crosses, areas, shapes, bar/background colours and filled regions.
- [x] MACD histogram columns, displaced Ichimoku cloud fills into visible future space, and separate Parabolic SAR dots.
- [ ] Correct RSI zero-loss handling for very small price changes and the dependent divergence calculations.
- [ ] Return the correct gain-only RSI values in Connors RSI's price and streak components.
- [ ] Tighten both Tasuki Gap detectors' third-candle opening and closing constraints.
- [ ] Correct Woodies CCI's price source and neutral/transition histogram colours.
- [ ] Give PPO, PVO and Net Volume histogram outputs their histogram defaults; expose PPO/PVO average-type inputs.
- [ ] Add selectable Bollinger basis averages and BandWidth expansion/contraction lookbacks.
- [ ] Expose VWAP band multipliers and event dates in applied-study settings.
- [x] Request finer profile data, from a feed's suggestion or a chosen interval, and adapt session profile rows to the zoom.
- [ ] Add dedicated two-symbol spread, ratio, Pearson correlation and log-return correlation studies.
- [ ] Emit rolling price-to-bar-index correlation as a Trend Strength Index study.
- [ ] Add automatic chart-pattern and key-level detection studies.
- [ ] Add composite technical ratings, lunar phases and a dedicated volatility-index study.
- [ ] Add the remaining specialised ADX-breakout, divergence, reversal and intraday/missed/ghost-pivot studies.

### Study API and Editing

- [x] `Indicator` and `IndicatorRegistry` calculate without egui; `CustomIndicator` accepts downstream calculation code.
- [x] Typed inputs with validation, mutable settings, plot styles, fills, offsets, precision, visibility and independent scale metadata. Multi-output legends identify each value and wrap in narrow panes; order-flow quantity readouts follow instrument precision.
- [x] Additional-symbol and additional-interval histories, source selection and confirmed/developing value alignment.
- [x] Incremental updates or scheduled recalculation as chart data changes.
- [x] Applied-study editor with Inputs, Style and Visibility pages and atomic validated changes.
- [x] Study templates capture inputs and presentation; built-in studies reconstruct through `StudyTemplate`.
- [ ] Offer interval and wait-for-close inputs for OHLC-based studies as well as source-aware studies.
- [ ] Expose every supported plot type in the Style editor.
- [ ] Add a configurable per-chart study limit and rejection feedback.
- [x] Restore custom registered studies and remap study-source dependencies through a library-owned template operation, including the terminal's indicator templates.
- [x] `StudyEditor` sends open, save and apply requests to the host and displays diagnostics. The demo applies the documented `close - close[N]` sample to live bars, keeps the last valid study on errors and stores named source documents locally.

## Order Flow

Execution-based calculations use supplied trades and their aggressor side. Unknown-side volume remains separate; chart-bar volume estimates do not reconstruct actual executions.

### Footprints, Delta and Profiles

- [x] `Footprint` aggregates bid, ask and unknown volume by bar and integer tick row, with execution-size filtering.
- [x] Footprint cells support bid/ask, total volume, delta and delta-plus-volume modes, up to four columns per bar, profile or boxed rendering, automatic row grouping and opening/closing price ticks in a separate gutter; their values have clearance from underlying price guides.
- [x] Per-bar point of control, value area and horizontal/diagonal imbalance highlighting; compact values fit detail columns and POC outlines recede in the overview.
- [x] Public calculations and renderers expose stacked diagonal imbalance zones, unrevisited POC levels, volume clusters and per-bar statistics; annotation captions avoid footprint values, with compact overview marks and hover details. Statistics use shared row labels, aligned compact values, explicit percentages and ratios, and a zoom-in or resize notice when detail cannot fit.
- [x] Execution-based delta histograms and cumulative-delta candles, including intrabar extremes and configurable reset boundaries, render in a supplied pane mapping. The cumulative execution study offers optional confirmed divergence and exhaustion price markers, with study outputs for bar-close alerts.
- [x] Trade profiles aggregate supplied ranges with bid/ask, volume or delta display, developing POC, value area and local volume nodes; public range editing supports merging and splitting.
- [ ] Bound live footprint retention when trades are evicted, including accumulated bars and price levels.
- [x] Size-filtered cumulative delta has a study pane with its own scale and lot-precision readouts.
- [x] Combined size-range delta labels its inclusive buckets; rolling volume/delta supports histograms.
- [ ] Add threshold highlights for delta percentage and bid/ask ratio, and a minimum filter for delta POC.
- [ ] Add fixed-cell, delta-fade and dominant-side colouring, abbreviated values, zero-display controls and value-scaled text.
- [ ] Add footprint extreme bid/ask-ratio annotations and pullback highlights.
- [ ] Extend imbalance zones with horizontal mode, configurable duration and fresh/triggered state.
- [ ] Correct unfinished-auction extreme tests and extend their levels beyond the originating bar.
- [ ] Draw value-area boundary guides for execution-based profiles and expose node sensitivity, smoothing and empty-row valleys.

### Depth, Tape and Heatmap

- [x] `OrderFlowPanel` edits detector rules and alert/sound choices, shows counts, and emits category visibility changes and sound intents; the demo connects its supplied executions and depth.
- [x] Order-flow detectors count new detections in feed-time windows, keep loaded history silent, and deliver live and rising-edge count alerts through the notification service.
- [x] Sequenced aggregated-depth snapshots/deltas and per-order add, modify, cancel and execute events, with gap detection and resynchronisation.
- [x] Public order book and order queue retain tick-aware prices, best quotes and queue priority.
- [x] `DepthHistory` retains sampled resting-size columns in bounded tiers; the heatmap uses retained texture tiles and partial updates.
- [x] Heatmap best bid/ask paths and a compact current-book profile of total or largest-order resting size, with host-supplied quantity units, a maximum-size key including offscreen levels and exact price, side and size on hover. The Order flow settings tab saves independent heatmap, book profile and volume bubble visibility and appearance; declared feed services and instrument tick size determine its controls.
- [x] Volume bubbles aggregate by chart bar and price, adapt automatic grouping, sizing and borders to screen density, and offer side/delta colours, linear/square-root/logarithmic scaling and outlier limits.
- [x] Time-and-sales panel with aggressor colours, size filters, sound intents, selectable milliseconds and full execution timestamps on hover.
- [x] Explicit aggressor identifiers combine split fills into parent orders with total size, price span and fill count.
- [x] Fixed stream studies calculate quoted spread, resting book totals/difference, execution rate and inside-quote movement. Execution and aggressor rates plot per-second windows by default and keep bar totals in labelled readouts.
- [ ] Add bubble time-window/count grouping, ungrouped execution timestamps, split bid/ask and additional delta/volume colour modes.
- [x] Dedicated ladder columns show observed size changes, traded bid/ask volume and order counts with bid/ask labels, complete values and horizontal scrolling in narrow tickets.
- [ ] Add session/reset controls and delta/bid-ask modes for the ladder profile.
- [ ] Expose largest-order side profiles and configurable resting-size highlight tiers in the ladder and heatmap.
- [ ] Connect order-size filtering and aggregated/per-order source selection to heatmap sampling, including metadata for participant exclusions.
- [ ] Benchmark and tune heatmap cadence, row count and update cost for large books.
- [ ] Add fresh/persistent size filtering to book totals, execution-consumed level counts and price-reversal resets.
- [ ] Add combined-parent-order counting to trade-rate studies and consumed passive-order details to reconstructed tape.
- [ ] Expose raw footprint and book inputs through the general study context.

## Trading and Strategy Reports

- [x] `Trading` connects order tickets, account tables and chart overlays to a supplied `Broker`.
- [x] Market, limit, stop and stop-limit tickets, units/value sizing, market-order risk sizing, time in force, presets and attached stop-loss/take-profit levels.
- [x] Validated one-click orders, chart context-menu entry and broker/account selection components.
- [x] Order and position lines show levels and profit/loss; confirmed actions modify/cancel orders, close/reverse positions and edit brackets.
- [x] A depth ladder supports order entry, drag modification and right-click cancellation through the broker interface.
- [x] Account summary, positions, orders, history, executions, balance and journal tables, with CSV export.
- [x] Execution markers, rejection/fill notifications and execution sound intents.
- [x] `StrategyReportPanel` accepts `StrategyReport` data and shows trade lists, equity/drawdown plots, performance metrics, supplied properties and chart navigation; trades export as CSV.
- [x] `Backtest::update` processes new closed bars incrementally, retains observed forming-bar fills when tick recalculation is enabled, and rebuilds corrected history.
- [x] `StrategyTester` selects factory strategies, reuses the study input editor, edits account and execution settings, requests history from the host and displays runs with labelled fill signals. Edits mark the report stale.
- [x] Parameter optimisation previews bounded numeric grids, yields between execution batches, sorts trial results and applies selected inputs on native and web builds.
- [x] `StrategyFactory` lists and creates built-in and host strategies with editable study inputs, defaults and bounds.
- [x] `strategies::optimise` lazily runs every input combination on a fresh strategy with the same bars and settings, yielding its inputs and report performance; dropping the iterator stops further trials.
- [x] Ten price-action strategies trade gaps, consecutive closes, inside/outside bars, channels, confirmed pivots and ATR expansion, with fill-relative tick brackets where needed.
- [x] Twelve study strategies use public indicator values for bands, momentum, averages, oscillators, trend flips and technical ratings; every built-in preserves its trades on historical prefixes.
- [ ] Extend risk-percent sizing to limit, stop and stop-limit tickets.
- [x] Export strategy report performance summaries and run properties as CSV.

## Alerts

- [x] Typed `Alert` conditions model price crossings/comparisons, moves over a bar span, study plots, supported drawing levels, watchlists and multiple conditions.
- [x] Shared creation/editing dialog validates conditions, name, message, frequency and expiry; presets use storage.
- [x] The terminal wires toolbar, context-menu, price-scale and configurable keyboard entry points, plus draggable chart alert levels. Alert display, toast duration and host sound playback follow the Alerts settings tab.
- [x] Strategy-run fill alerts carry execution prices and position messages, suppress repeated fill batches and run on the demo live feed.
- [x] `AlertsPanel` uses `services::Alerts` to list, edit, pause/enable and remove alerts, and to show supplied trigger events in toasts and an exportable log.
- [x] The alerts panel sorts by name, symbol and status, and stages copies in the shared editor.
- [x] Inline creation and series-level alert visibility, labels and colour work alongside the shared dialog.
- [x] Drawing alerts follow fork, pitchfan and regression lines on linear and logarithmic scales, with entry, exit, inside and outside conditions resolving both moving boundaries.
- [ ] Model persistent inside/outside-channel conditions and once-per-minute frequency.
- [ ] Expose message placeholder and per-alert notification-schedule configuration for host processing.

## UI Components

### Toolbars, Dialogs and Panels

- [x] Top toolbar with symbol search, comparison, interval/type selection, study, alert, replay, history, settings, save and layout actions.
- [x] Symbol search supports text, type, exchange and source filters, pagination, keyboard navigation and host results; the terminal loads comparisons from the chart feed on shared percentage scales, separate scales or panes, and displays arithmetic spreads as candles in their own panes.
- [x] Timeframe toolbar provides range presets, go-to-date, time zone, regular/extended session and adjustment requests. `Terminal` fits ranges to loaded history, navigates dates in the chart time zone, switches intervals and sessions, restores interval favourites, and requests host-supplied dividend adjustment.
- [x] Drawing toolbar categories, cursor modes, snapping, repeated placement, locking and template actions; favourites can be stored.
- [x] Chart settings apply backgrounds, grids, candle parts and colours, previous-close colouring, crosshair styling, session shading and watermarks.
- [x] Status settings control supplied logos, last day change and study backdrops; legend text and controls have opaque backing over profiles; Symbol settings request dividend-adjusted history from the host.
- [x] Object tree supports selection, filtering, visibility, locks, ordering, rename, duplication, pane moves and bulk actions; a data window shows OHLCV and study values.
- [x] Editable shortcut bindings are shared by chart input, help and menu hints; quick search uses a command palette. The demo showcase menu describes and opens the same populated samples without a search query.
- [x] Watchlists support named lists, sections, sorting, row reordering, flags, symbol navigation and text import/export.
- [x] Feed-backed panels render symbol details, news, calendars, hotlists, screener tables and grouped market heatmaps; `FinancialsPanel` charts and tabulates supplied financial figures.
- [ ] Assemble a terminal that owns component state, handles all actions and lets the application intercept them. `Terminal` also runs the widget bar and side panels, persists watchlists, routes panel symbol selections, derives performance and seasonality from chart bars, opens host-fed profile cards, trades through the supplied brokers and docks the account and strategy report. It also handles time controls, comparisons, spread selection and host-supplied dividend adjustment, and runs multi-chart grids, layout tabs, saved layouts, favourites, autosave and Ctrl+S. Remaining action groups and the demo migration are unfinished.
- [x] Dock the supplied broker account and host strategy report in shared bottom tabs, with collapse, restore and maximise controls.
- [ ] Open the study editor and replay-trading surfaces as tabs of the same bottom dock.
- [ ] Open the drawing toolbar's colour popup from a visible control and render its saved favourites bar.
- [ ] Persist and restore indicator favourites in the assembled UI.
- [ ] Show and route the standalone floating selection toolbar's settings, duplicate, lock, visibility, delete and colour actions.
- [ ] Connect the standalone quick settings menu, its scale-settings action and all of its configuration fields.
- [ ] Handle the series context menu's Copy price action.
- [ ] Complete default rectangle-tool and selected-chart-maximise shortcut routing. Interval entry and go-to-date shortcuts run in `Terminal`.
- [ ] Add main-symbol legend controls for visibility, settings and removal.
- [x] Watchlists duplicate lists, filter flags, add symbols from the chart menu and open an advanced view in the terminal. The host supplies its earnings, dividends and headlines.

### Replay

- [x] Bar replay hides future main-series bars, supports start-bar/date/random selection, stepping, autoplay, speed and seek, and restores live data on exit.
- [x] Main-series studies recalculate against replayed bars and drawing anchors remain stable.
- [x] Floating replay controls, keyboard shortcuts, saved replay sessions and synchronised layout replay are exercised by the demo.
- [x] `ReplayClock` merges timestamped trades, quotes and depth events across symbols, supports speed and seek, and routes through normal sinks.
- [x] `RecordingWriter` and `RecordingReader` encode/decode versioned trade and depth events through supplied I/O streams.
- [ ] Window additional-symbol/interval study histories to the replay cutoff.
- [ ] Connect the standalone `ReplayToolbar` actions to the replay controller.

### Export

- [x] `Chart::export_csv` exports OHLCV and study values with timestamps and missing-value handling.
- [x] `Chart::export_png` renders an isolated chart frame on the CPU after font initialisation; export controls include size, title, timestamp, crosshair and transparency.
- [x] Export dialog and demo actions save images/data or copy images through application facilities. The demo exports either the active chart or the visible arrangement as PNG or SVG, with shared image options and opted-in trading overlays. The terminal's file callback returns save errors and carries a host request to open an exported image.
- [ ] Capture composed panels, trading overlays, external textures and feed-backed order-flow visuals in a complete terminal image.
- [ ] Add an export watermark switch, vector image output and print/PDF output.
- [ ] Integrate a native save-file chooser and provide an `Exportable` adapter for direct chart export.

## Layouts and Persistence

- [x] `ChartLayout` arranges up to 16 charts in grids or tabs, tracks the active chart, supports detached views and groups, and synchronises selected symbol, interval, time range and crosshair time. The demo saves group memberships, detached slots and synchronization switches with each layout.
- [x] Main-pane drawings synchronise between grouped charts by timestamp.
- [x] `ChartSnapshot` saves symbol, interval, chart configuration, built-in studies, panes, scales, viewport and drawings in versioned documents; feed-backed comparisons and spreads reconnect on restore, and the dividend-adjustment choice persists.
- [x] The terminal creates, copies, renames, lists, saves, opens and deletes layouts through supplied storage, with debounced autosave and Ctrl+S. Layout tabs keep independent charts and undo history; Tab and Shift+Tab cycle the visible chart slots, and new slots inherit the study limit.
- [x] Chart, study and drawing template catalogues support named save, replace, apply and delete operations; the terminal wires indicator templates to host storage and applies their studies, links and shared panes as one undoable edit.
- [x] `SettingsStore` persists preferences with debouncing, failed-write retry and error reporting.
- [ ] Preserve added/custom series, markers, price lines and custom-study reconstruction in chart snapshots, with explicit reattachment of application services and formatters.
- [x] Layout favourites appear in the catalogue and menu, follow renames and deletions, and persist in user settings.
- [ ] Synchronise crosshair price as well as time in `ChartLayout`.
- [ ] Map study-pane drawings across charts and propagate mirrored updates, unlocking and deletion to locked receivers.
- [ ] Include symbol navigation in terminal undo/redo; comparison membership, placement and visibility are undoable and reconnect feeds when restored.

## Data and Builder API

- [x] `Chart::new` embeds static bars; `ChartBuilder` configures a chart, feed, symbol, interval, studies, theme and drawing registry. `Bar` accepts typed calendar dates for OHLCV, single-value and whitespace daily bars at midnight UTC, matching date-only imports.
- [x] `Datafeed`, `Reply` and `Sink` provide asynchronous metadata/history and streaming bars, executions, quotes and depth, with per-symbol service declarations, stale-response rejection and subscription cancellation.
- [x] Scroll-back pages bar history; transient failures retry with bounded backoff and expose errors and status through `FeedState`.
- [x] Shared validation sorts/deduplicates bars, rejects invalid OHLCV and counts rejected updates; out-of-order corrections update the affected history.
- [x] Live trade aggregation supports time, execution-count, volume, delta and range intervals; resampling helpers accept session rules.
- [x] Optional per-series bar limits bound retained history; feed queues coalesce updates and bound pending trades/depth.
- [x] Runtime symbol/interval changes reload the feed; explicit reload and reset paths invalidate old work.
- [x] Public events report pointer, range, symbol, interval and data changes; custom menu entries and toolbar controls return application identifiers.
- [x] Added price series, custom study-backed series and attached `Primitive` renderers support visibility, hit testing, scale participation and removal. Bar-series logical-range queries return stored slices and shared-scale distances, including gaps and fractional bounds outside the data. The demo menu's Samples entries demonstrate toggleable session backgrounds, axis labels and a custom range pane with pointer feedback.
- [ ] Reset a stale manual price range when changing symbols.
- [ ] Accept locale, formatters, storage and saved state as builder inputs.
- [ ] Add bounded time-range requests for trades/depth and page older trades during scroll-back.

## Infrastructure

- [x] Feature layers separate the engine, built-in studies, widget, drawings, icons and terminal UI; engine and study calculations need no egui.
- [x] `Theme` provides editable palettes with dark, light and high-contrast presets; shared tokens cover sizing, spacing, corners and colours.
- [x] Host translation lookup with English fallback, locale/date formatters and optional named time zones. The demo persists independent language and number-format samples and applies host formatting to its data window and exported chart. The demo selects light, dark, high contrast or system appearance, preserves explicit choices and follows later system changes.
- [x] The demo exposes floating, tracking and magnifier readouts, hover or hold activation, lens size and zoom; magnified candles stay inside the circular lens. Its annotation editor creates, edits and removes series markers and price lines, controls marker fitting and layering, and configures live change-marker duration.
- [x] Accessibility roles/states, keyboard focus and a textual bar/study readout.
- [x] Reusable `ui_kit` buttons, inputs, colour pickers, dialogs, lists, tabs, panels, command palette and toast components.
- [x] Native and browser examples and CI checks cover library, feature combinations and demo builds.
- [x] The guide and site landing page share light and dark navigation and a live sample chart with consistent text sizing and a compact readout at phone widths. The first-chart guide embeds an isolated widget scene beside its compiled setup source, with theme synchronization, keyboard exit and a loading fallback; the full demo stays at `/demo/`. Version labels distinguish main (0.3 in development, egui 0.37) from published 0.2.0 (egui 0.33.3), with grouped topic navigation, integration recipes and direct links to API members. The documentation build checks local guide and API pages and anchors, including after site assembly.
- [ ] Route remaining direct English UI labels through the translation lookup.
- [ ] Make session-quote/volume tags and marker outlines fully theme-configurable.
- [ ] Add a runnable desktop-webview integration example.

## Provided by the host

The crate renders supplied data and exposes interfaces for application services. The application supplies market data, broker execution, accounts and durable storage. These interfaces also allow services to run remotely.

- `Datafeed`, `SymbolSearch`, `Quotes` and `Marks` connect instrument discovery, history, live prices, executions, depth and chart events. Feed queries also supply metrics, options volume, listed option expiries and live or historical option chains with quotes, open interest, implied volatility and Greeks, analyst targets, symbol details, news, calendars, screeners, hotlists and market heatmaps. The headless `OptionChain` handle retains the latest validated snapshot and error; the host supplies all option data. `OptionExposure::gamma` computes exposure per 1% underlying move by expiry and strike, counting contracts with missing inputs and using positive call and negative put signs as a convention. `Datafeed::option_exposure` supplies gamma, delta, vanna or charm snapshots with their units and model label; `OptionExposureHandle` retains the latest valid observation and prefers it unchanged over computed gamma.
- `Broker` supplies real or paper-account state, trading rules, execution and account updates. Trading components submit actions and render the reported orders, positions, fills and history.
- `Storage` supplies layouts, drawings, settings and template documents; an application may use local or remote storage and manage accounts, synchronisation and sharing.
- `Alerts` stores and evaluates alert conditions and reports changes and triggers. The crate edits models and draws levels, dialogs, logs and toasts; the application supplies sound, schedules and external delivery.
- `Signals` supplies markers, levels and zones with category visibility and click callbacks. Captions yield to price bars, signal borders and other captions, prioritising recent markers; hover retains the labels and details of coincident events. Applications can deliver iceberg, absorption, spoofing, large-order and other interpreted order-flow events through it.
- `Notifications` and `Chats` supply notification records and conversation messages. Publishing and sharing actions are application intents; post/idea drawings display supplied local content.
- `StrategyReport` supplies the trade list, equity and performance inputs for the report panel; `CompanyFinancials` supplies financial tables. Applications also provide translations, additional fonts, image loaders and platform save/clipboard facilities.

## Next

The order below prioritises correctness and complete library workflows. It carries no delivery dates.

1. **Chart rendering and drawings:** correct Kagi transitions, Elliott targets, pattern geometry, Fibonacci bounds, logarithmic alert levels, pane input and scale settings.
2. **Indicators and order flow:** correct RSI, Connors RSI, Tasuki, Woodies CCI and unfinished auctions; bound footprint retention and replay histories; finish histogram defaults and integrated delta/profile panes.
3. **Terminal UI and alerts:** assemble component state and action handling with application interception; complete selection/favourites controls, alert configuration, watchlist actions and the study editor surface.
4. **Layouts and data API:** broaden snapshot/template restoration, drawing/crosshair synchronisation and undo coverage; finish trade/depth range requests, paging, symbol scaling and builder inputs.
5. **Trading and reports:** complete ticket sizing and report exports.
6. **Presentation and extensions:** finish export capture/formats, translation/theme coverage, advanced order-flow controls, additional studies and the desktop-webview example.
