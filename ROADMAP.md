# egui-charts Roadmap

Comprehensive feature audit and roadmap for reaching professional charting parity.
Each item is marked as complete, partial, or missing.

---

## Chart Rendering Engine

### Chart Types (17/20 complete)

#### Standard OHLC
- [x] **Candles** — Filled/hollow body, wicks, optional volume bars
- [x] **Bars** — OHLC bars with tick marks
- [x] **Hollow Candles** — Hollow body when bullish, filled when bearish
- [x] **Volume Candles** — Bar width scaled 30-100% by relative volume

#### Line-Based
- [x] **Line** — Close-price line with configurable `PriceSource`
- [x] **Line with Markers** — Line + circle markers at each data point
- [x] **Step Line** — Staircase-style step interpolation

#### Area-Based
- [x] **Area** — Filled polygon under close line (50% alpha)
- [x] **HLC Area** — Band fill between high and low, close line on top
- [x] **Baseline** — Colored segments above/below a reference baseline

#### Range-Based
- [x] **High-Low** — Thin rects from high to low per bar
- [x] **Range Bars** — Time-independent range bar transformation + rendering

#### Japanese
- [x] **Heikin-Ashi** — Full HA formula (HA_close, HA_open), rendered as candles
- [x] **Renko** — Brick rendering via `to_renko_bricks()`, max 2000 elements
- [x] **Kagi** — Thick/thin line segments with shoulder/elbow logic
- [x] **Line Break** — 3-line break bricks via `to_line_break_lines()`
- [x] **Point & Figure** — X/O column rendering, max 1000 columns

#### Advanced (require tick/order-flow data for full accuracy)
- [ ] **Volume Footprint** — Renders buyer/seller split from OHLCV approximation; needs real bid/ask tick data
- [ ] **Time Price Opportunity (TPO)** — Full `TpoRenderer` exists (value area, POC, initial balance, 3 display modes, 4 color modes) but is NOT wired into the main dispatch; pipeline still routes to placeholder
- [ ] **Session Volume Profile** — Session-chunked approximation, not per-price-level aggregation

### Pan / Zoom
- [x] Mouse wheel zoom (centered on cursor)
- [x] Shift+wheel horizontal pan
- [x] Click-drag pan (left button)
- [x] Pinch-to-zoom (multi-touch)
- [x] Two-finger pan
- [x] Kinetic scrolling with exponential damping
- [x] Price axis drag-scaling
- [x] Double-click to reset axes
- [x] Box zoom (selection rectangle)
- [ ] Box zoom Measure mode (TODO P1 in code)

### Crosshair / Cursor
- [x] Normal mode (follows raw cursor)
- [x] Magnet mode (snaps to nearest OHLC value)
- [x] Full crosshair (vertical + horizontal lines)
- [x] Dot crosshair
- [x] Arrow mode (no crosshair)
- [x] Line styles: solid, dashed, dotted
- [x] Price label on Y-axis with alert indicator
- [x] Time label on X-axis with adaptive format

### Scales
- [x] Linear (normal) price scale
- [x] Logarithmic price scale
- [x] Percentage price scale
- [x] Indexed-to-100 price scale
- [x] Auto-scale with configurable margins
- [x] Manual range override
- [x] Inverted scale
- [x] Dual price scale manager (left/right)
- [x] Hierarchical time tick generation with weight system
- [x] Price mark generation
- [x] Price formatters: default, currency, percentage, scientific, volume, custom
- [x] Time formatters: default, locale, relative, custom

### Series API
- [x] `ISeriesApi` trait defined with coordinate conversion
- [x] `SeriesApiImpl` with real coordinate math
- [ ] `bars_in_logical_range()` returns synthetic placeholder bars (hardcoded values)
- [ ] `current_price()` always returns `None`
- [ ] `symbol()` returns literal `"SYMBOL"`
- [ ] `ITimeScaleApi` trait defined but has no concrete implementation
- [ ] `PriceScaleApiImpl::width()` hardcoded to `60.0`

### Multi-Pane
- [x] `IndicatorPane` widget for separate-pane indicators
- [x] Predefined configs: RSI, MACD, etc.
- [x] X-axis synchronization with main chart
- [ ] No `PaneManager` — indicator sub-panes manually constructed
- [ ] `move_to_pane` / `merge_with_pane` / `detach_pane` are stubs on `ISeriesApi`

---

## Drawing Tools (80/88 real implementations, 3 placeholders)

### Lines (9/9)
- [x] Trend Line — Segment distance hit test, start/end/middle handles
- [x] Ray — Extends to right edge
- [x] Extended Line — Extends both directions
- [x] Horizontal Line — Y-proximity hit test
- [x] Horizontal Ray
- [x] Vertical Line — X-proximity hit test
- [x] Cross Line — Horizontal + vertical
- [x] Trend Angle — Line + dashed reference + arc + angle/slope label
- [x] Measure — Colored rect + arrows + info box (price%, pips, bars, time)

### Channels (3/4)
- [x] Parallel Channel — Two parallel lines + connectors, explicit hit test
- [x] Flat Top/Bottom — Trend line + horizontal + Support/Resistance labels
- [x] Disjoint Channel — Two parallel segments + dashed connectors + width label
- [ ] **Regression Trend** — Renders regression line + bands, but R-squared is hardcoded `0.95` (no actual regression from data)

### Pitchforks (4/4)
- [x] Pitchfork (Andrews)
- [x] Schiff Pitchfork
- [x] Modified Schiff Pitchfork
- [x] Inside Pitchfork

### Fibonacci (11/11)
- [x] Fibonacci Retracement — Configurable levels, extend left/right, price labels
- [x] Fibonacci Extension
- [x] Fibonacci Channel
- [x] Fibonacci Arc — Semicircular arcs at 6 ratios
- [x] Fibonacci Time Zones — Vertical lines at Fibonacci sequence intervals
- [x] Fibonacci Circles — Concentric circles at 8 ratios
- [x] Fibonacci Speed Resistance Arcs
- [x] Fibonacci Speed Fan — 7 ratios with colored fills
- [x] Fibonacci Spiral — Golden spiral via quadratic bezier segments
- [x] Fibonacci Wedge — Angle-based wedge fan
- [x] Trend-Based Fib Time — Vertical lines at ratio multiples

### Gann (4/4)
- [x] Gann Fan — 9 angles (8:1 to 1:8), colored lines + labels
- [x] Gann Square — Grid with diagonals, inscribed circle, degree labels
- [x] Gann Box — 8x8 grid with diagonals + corner angles
- [x] Gann Fixed — 1:1 forced-square with 4x4 grid

### Patterns (5/6)
- [x] XABCD Pattern — Labels, filled triangles, Fibonacci ratio display
- [x] Head and Shoulders — LS/Head/RS labels, neckline extension, target projection
- [x] ABCD Pattern — Labels, fill, CD/AB ratio with color coding
- [x] Triangle Pattern — Numbered points, fill, apex extension
- [x] Three Drives — 7-point pattern, PRZ box
- [ ] **Cypher Pattern** — Delegates entirely to XABCD renderer; Cypher-specific ratios not visually distinct

### Elliott Waves (5/5)
- [x] Elliott Impulse — 6 points, motive/corrective fill, Fibonacci 1.618 target
- [x] Elliott Correction — 4 points (5-A-B-C), corrective fill
- [x] Elliott Triangle — 5 points (A-E), converging trendlines
- [x] Elliott Double Combo — 3 points (W-X-Y)
- [x] Elliott Triple Combo — 5 points (W-X-Y-X2-Z)

### Cycles (3/3)
- [x] Cyclic Lines — Repeating vertical lines, cycle numbers, period label
- [x] Time Cycles — Concentric semicircular arcs (0.5x-3x), mesh fills
- [x] Sine Line — Full sine wave with amplitude guides

### Projection / Trading (6/6)
- [x] Long Position — Entry/target/stop zones, P&L calculation
- [x] Short Position
- [x] Forecast — Projected price trajectory
- [x] Ghost Feed — Projected future bars
- [x] Bars Pattern — Source region + pattern projection
- [x] Projection Tool

### Volume / VWAP (0/3 accurate)
- [ ] **Anchored VWAP** — Renders line + bands, but std_dev is a visual approximation (`chart_rect.height() * 0.1 * 0.15`), not computed from actual price/volume data
- [ ] **Fixed Range Volume Profile** — Horizontal bars approximated, not driven by real volume data
- [ ] **Anchored Volume Profile** — Same approximation issue

### Measurements (4/4)
- [x] Price Range — Two hlines + vertical connector + price diff label
- [x] Date Range — Two vlines + horizontal connector + bar count
- [x] Date and Price Range — Filled rect + combined label
- [x] Info Line — Angled line + info box (price%, bars, time, angle)

### Brushes / Freehand (2/3)
- [x] Brush — Freehand stroke with connected segments
- [x] Highlighter — Wider semi-transparent stroke
- [ ] **Paintbrush** — Identical implementation to Brush (calls `render_brush()`)

### Arrows (4/4)
- [x] Arrow — Line + arrowhead
- [x] Arrow Marker — Colored filled triangle
- [x] Arrow Mark Up
- [x] Arrow Mark Down

### Shapes (8/10)
- [x] Rectangle — Fill, dashed midlines, dimension labels, 4-corner handles
- [x] Circle
- [x] Ellipse — Path approximation
- [x] Triangle — 3-point polygon
- [x] Arc — Arc/sector rendering
- [x] Polyline — Connected segments with vertex markers
- [x] Path — Freeform connected segments
- [x] Curve — Quadratic bezier
- [ ] **Rotated Rectangle** — Renders as axis-aligned only; rotation not implemented
- [ ] **Double Curve** — Uses sine offset approximation, not true cubic bezier

### Annotations (11/11)
- [x] Text Label — Background box + border + text + anchor marker
- [x] Anchored Text — Background box + anchor icon + text
- [x] Note — Sticky note with folded corner + tooltip
- [x] Anchored Note — Pin head + needle + expandable text box
- [x] Flag Note — Flag pole + pennant + optional text
- [x] Price Label — Arrow badge with price text
- [x] Price Note — Dashed hline + note box
- [x] Callout — Text box with auto-edge pointer triangle
- [x] Comment — Speech bubble + expandable text
- [x] Table — 3-column x 4-row grid with header, alternating rows
- [x] Signpost — Dashed vertical + sign plate + arrow

### Content / Media (0/3)
- [ ] **Image** — Decorative placeholder only (checkerboard + icon); no image loading
- [ ] **Tweet** — Decorative card only; no API integration
- [ ] **Idea** — Decorative card only; no service connection

### Icons (1/1)
- [x] Font Icon — Emoji/character rendering at configurable size
- Note: Icons module is feature-gated behind `icons` feature (enabled by default)

### Cursors (4 modes, no rendering needed)
- [x] Cross, Dot, Arrow, Eraser

### Drawing System Infrastructure

#### Hit Testing
- [x] Geometry-accurate hit tests for: TrendLine, Measure, HorizontalLine, VerticalLine, Rect, Fibonacci (3 variants), GannFan, ParallelChannel
- [ ] 80+ tools use point-proximity fallback only (must click on anchor point, not on drawing body)

#### Handle System
- [x] Coordinate-aware handles for: TrendLine, Measure, Fibonacci variants, Gann variants, Rect, HorizontalLine, VerticalLine
- [x] Generic indexed handle fallback for all other tools (functional but basic)

#### Persistence
- [x] JSON serialization via `DrawingStorage` (id, tool type, points, color, line width, timestamps)
- [x] Version field with migration stub
- [ ] `FibonacciConfig` (custom levels) not serialized — user customizations lost on save/load
- [ ] `line_style`, `fill_color`, `font_size`, `extend_left/right`, `arrow_styles`, `z_order` not serialized

#### Properties Dialog
- [x] Multi-tab dialog: Style, Text, Visibility, Coordinates
- [x] Color, stroke width, line style, fill, opacity controls
- [x] Per-timeframe visibility toggles
- [x] Direct point coordinate editing

---

## Technical Indicators (~130 unique, all calculated)

All ~130 indicators have **complete calculation logic** — zero `todo!()` or `unimplemented!()` stubs.
Some indicators appear in multiple categories below (e.g., CVD in both Volume and Order Flow).

### Rendering Limitation
All indicators use a **generic polyline renderer**. There is no indicator-specific rendering. This means:
- [ ] MACD histogram renders as a line instead of bars
- [ ] Ichimoku cloud renders as 5 separate lines instead of filled cloud regions
- [ ] Parabolic SAR renders as a connected line instead of individual dots
- [ ] Volume Profile outputs `IndicatorValue::Multiple` but there is no horizontal bar chart renderer

### Moving Averages (22/22)
- [x] SMA, EMA, WMA, DEMA, TEMA, HMA, ALMA, VWMA, LSMA, SWMA
- [x] KAMA, Smoothed MA (Wilder), McGinley Dynamic, VIDYA, ZLEMA
- [x] Hamming MA
- [x] Guppy MA (GMMA) — 12 EMAs (6 short, 6 long)
- [x] Multiple MA — SMA 20/50/100/200
- [x] MA Cross, EMA Cross
- [x] MA Channel, Envelopes

### Momentum Oscillators (36/36)
- [x] RSI, MACD, VW-MACD, Stochastic, Stochastic RSI
- [x] Williams %R, CCI, ROC, Aroon, Awesome Oscillator
- [x] Ultimate Oscillator, Connors RSI, Coppock Curve
- [x] DPO, TRIX, Mass Index, Chande Momentum (CMO)
- [x] True Strength Index (TSI), KDJ, Know Sure Thing (KST)
- [x] Schaff Trend Cycle, Double Stochastic, Elder Ray
- [x] Fisher Transform, Ehlers Fisher, Force Index
- [x] Center of Gravity, Pretty Good Oscillator, Rainbow Oscillator
- [x] Relative Vigor Index, Stochastic Momentum Index
- [x] Ultimate Momentum, Trend Intensity Index, Qstick
- [x] Elder Impulse System, Trend Detection Index

### Trend / Overlay (8/8)
- [x] ADX — ADX, +DI, -DI lines
- [x] SuperTrend — ATR-based trailing stop
- [x] Ichimoku Cloud — Tenkan, Kijun, Chikou, Senkou A, Senkou B
- [x] Parabolic SAR — Acceleration factor dots
- [x] Directional Movement (DMI) — +DI and -DI
- [x] Vortex Indicator — VI+ and VI-
- [x] Choppiness Index — ATR sum / range
- [x] Linear Regression — Rolling LinReg endpoint overlay

### Volatility (20/20)
- [x] Bollinger Bands — SMA +/- k*StdDev
- [x] ATR — Wilder smoothed true range
- [x] Keltner Channels — EMA +/- ATR*multiplier
- [x] Donchian Channels — N-bar high/low
- [x] Standard Deviation
- [x] Bollinger Band Width (BBW), %B
- [x] Chandelier Exit — ATR-based trailing stop (long + short)
- [x] Squeeze Momentum — BB inside KC detection
- [x] Price Channel
- [x] Historical Volatility — Annualized log-return std dev
- [x] Chaikin Volatility
- [x] Acceleration Bands
- [x] Close-to-Close Vol, OHLC Volatility (Garman-Klass), Parkinson Vol
- [x] Volatility Index — ATR-based annualized %
- [x] Standard Error, Standard Error Bands

### Volume (23/23)
- [x] OBV, VWAP (session-resetting), Anchored VWAP
- [x] MFI, A/D Line, CMF, Klinger Oscillator
- [x] Ease of Movement, Net Volume, Volume ROC, Volume Oscillator
- [x] CVD — Cumulative Volume Delta (3 estimation modes)
- [x] Volume Profile Visible Range, Volume Profile Fixed Range
- [x] VWMA, Balance of Power
- [x] Volume Price Trend, NVI, PVI
- [x] Williams AD, PVT Enhanced
- [x] Market Facilitation Index (Bill Williams)
- [x] Commodity Selection Index

### Correlation / Statistics (9/9)
- [x] Correlation Coefficient (Pearson), Correlation Log Returns
- [x] Linear Regression, Linear Regression Slope
- [x] Rank Correlation Index (Spearman)
- [x] Standard Error, Standard Error Bands
- [x] Majority Rule, Accumulation Swing Index

### Price Derivatives (9/9)
- [x] Typical Price, Weighted Close, Median Price
- [x] OHLC4, HLC3, HL2
- [x] True Range
- [x] Spread Study, Ratio Study

### Market Structure (2/2)
- [x] Advance/Decline Line (simulated from single symbol)
- [x] 52-Week High/Low %

### Support / Resistance (2/2)
- [x] Pivot Points — Standard, Fibonacci, Woodie, Camarilla, DeMark formulas
- [x] ZigZag — Swing detection with percentage/points modes

### Order Flow (4/4)
- [x] Iceberg Detector — Volume anomaly + confidence score
- [x] Stop Run — ATR breakout + volume surge
- [x] Liquidity Tracker — Rolling liquidity ratio
- [x] CVD (also in Volume)

### Price Oscillators (1/1)
- [x] PPO — Price Oscillator with signal line

### Factory Coverage
- [ ] `IndicatorFactory` pre-registers only 20 of ~130 indicators — remaining ~110 must be instantiated directly and are not discoverable through the factory

---

## UI Components

### Drawing Toolbar
- [x] Full sidebar with all tool categories
- [x] Submenu rendering with icons, section headers, shortcut labels
- [x] Cursor mode selection
- [x] Color picker popup (4 rows x 5 swatches)
- [x] Magnet, StayInDrawing, Lock toggles
- [x] Favorites and star-toggle
- [x] Horizontal floating toolbar variant
- [ ] Drawing template persistence (struct exists but no save/load backend)

### Top Toolbar
- [x] Symbol search pill (custom-drawn with icons)
- [x] Compare/Add symbol button
- [x] Timeframe selector dropdown (grouped by unit)
- [x] Chart type selector dropdown (all 20 types with icons + descriptions)
- [x] Indicators button
- [x] Alert, Replay, Undo, Redo buttons
- [x] Right rail: Publish, Trade, Camera, Fullscreen, Settings, Save, Layout
- [x] Settings menu (chart types, appearance, scale mode, labels)
- [ ] Settings "Scales Properties..." button closes menu but opens nothing
- [ ] Save button has no dropdown/sub-options
- [ ] Layout menu / chart grid selector not rendered inline

### Indicator Dialog
- [x] Modal overlay with dim background
- [x] Left category sidebar
- [x] Search bar with fuzzy filtering
- [x] Tabs: Indicators, Strategies, Metrics, Community
- [x] Favorite toggling
- [x] Configuration panel for parameters

### Symbol Search Dialog
- [x] Real-time text filtering
- [x] Keyboard navigation (Up/Down/Enter/Escape)
- [x] Scrollable results with exchange badges
- [x] Compare symbols dialog with two-column layout

### Timeframe Toolbar
- [x] Date range buttons (5D, 1M, 3M, 6M, YTD, 1Y, 5Y, All)
- [x] Go-to-date calendar button
- [x] Timezone display button
- [x] RTH/Session toggle
- [x] ADJ toggle

### Floating Selection Toolbar
- [x] Drag handle with reposition logic
- [x] Color slot pickers (native egui `color_edit_button`)
- [x] Settings, Duplicate, Lock, Visibility, Delete buttons
- [x] Delete button with red hover effect
- [x] Viewport clamping

### Chart Controls
- [x] Zoom In / Out / Reset buttons
- [x] Auto-scale toggle
- [x] Log scale toggle
- [x] Percentage toggle

### Replay System
- [x] Play/Pause/Stop buttons
- [x] Step forward/backward
- [x] Speed control with presets
- [x] Progress bar with seek (click/drag)
- [x] Keyboard shortcuts (Space, arrows, Home/End, M for marker, Ctrl+R, Escape)
- [x] Compact variant
- [x] SVG icon-based toolbar variant

### Dialogs
- [x] **Alert Dialog** — 3 tabs (Price, Indicator, Volume) with form validation
- [x] **Series Settings** — 7 tabs (Symbol, Status Line, Scales, Canvas, Trading, Alerts, Events)
- [x] **Drawing Properties** — 4 tabs (Style, Coordinates, Visibility, Text)
- [x] **Pine Script Editor** — Syntax highlighting, compile & run, output panel with errors/metrics
- [x] **Keyboard Shortcuts** — Config-driven KeyboardAction system
- [x] **Drawing Context Menu**
- [x] **Series Context Menu**

### Widget Bar (Right Sidebar)
- [x] Vertical icon bar (Alerts, Object Tree, Help)
- [x] Notification count badge
- [x] Active state highlighting

### Object Tree Panel
- [x] Hierarchical tree with indicator/drawing rows
- [x] Filter controls (text + type dropdown)
- [x] Visibility/lock toggle icons per item
- [x] Bulk actions (Show All, Hide All, Lock All, Unlock All, Delete Selected)
- [x] Context menu with keyboard shortcuts
- [x] Selection model (single, additive, range, Ctrl+A)
- [x] Footer stats (total, indicators, drawings, visible, selected)
- [x] Collapsible OHLCV data window

### Alerts Panel
- [x] 4 sections: Price, Indicator, Volume, Event
- [x] Inline add forms with real inputs
- [x] Alert rows with status-colored indicator + remove/toggle
- [x] Alert notification popup

### Export
- [x] Export dialog UI (format selection, size options, path input)
- [x] `Exportable` trait defined (`export_png`, `export_svg`, `copy_to_clipboard`)
- [ ] No `Exportable` implementation in-crate (host must implement)
- [ ] Browse button generates placeholder path (no native file dialog)
- [ ] No actual PNG/SVG rendering logic

### Other UI
- [x] Context menu — 20 entries with icons, shortcuts, disabled state, click-outside close
- [x] Watermark system — Text + Image watermarks, 6 anchor positions, tile mode, WatermarkManager
- [ ] Emoji picker — Structure exists but actual emoji grid rendering unverified
- [x] Multi-chart layout component
- [x] Symbol header display
- [x] Connection status indicator
- [x] Replay status overlay

---

## UI Kit (Reusable Components)

All ui_kit components are **fully implemented** generic building blocks:

- [x] Button (themed, variant-based: Primary/Secondary/Danger/Ghost)
- [x] IconButton (SVG icon + optional label)
- [x] SplitButton (icon + dropdown arrow)
- [x] ColorPicker (60 swatches: grayscale row + 10 main colors x 6 variations)
- [x] CommandPalette (fuzzy search, keyboard nav, command registry)
- [x] DialogFrame / DialogHeader / DialogFooter
- [x] EmptyState placeholder
- [x] Filter widget
- [x] FormGrid (2-column layout for settings forms)
- [x] Card frame
- [x] ListItem (LabelContent, ButtonContent, PropertyContent variants)
- [x] LoadingIndicator (spinner)
- [x] Modal + ConfirmDialog + PromptDialog
- [x] Notifications / Toast system (global toast manager, 4 severity levels)
- [x] PanelHeader (section header with right-side content slot)
- [x] Section (collapsible)
- [x] SidebarLayout (left nav + right content)
- [x] StatusMessage (inline status with icon)
- [x] TabBar (generic over `T: Display`)
- [x] ResponsiveToolbar (horizontal scrollable)
- [x] ButtonGroup (grouped toggle buttons)
- [x] TradingLabels (buy/sell/bid/ask label widgets)
- [x] Autocomplete (text field + dropdown suggestions)
- [x] Alert (inline banner)

---

## Infrastructure

### Data Layer
- [x] `DataSource` trait defined with error types (8 variants)
- [x] `DataSourceError` with `is_recoverable()` and `is_conn_error()` helpers
- [ ] No in-crate `DataSource` implementations (streaming, tick feeds, symbol search all in host)

### Configuration
- [x] `ChartConfig` — Comprehensive chart configuration
- [x] `ChartOptions` — User-facing options
- [x] `CrosshairOptions` — Crosshair style/mode/color
- [x] Config-driven keyboard shortcuts via `KeyboardAction`

### Theming
- [x] Full theme system with semantic tokens
- [x] Dark and light presets
- [x] Classic preset (formerly TradingView-style)
- [x] Design token system (RON-based, ~300+ tokens)
- [x] Token categories: sizing, spacing, rounding, stroke widths, semantic colors
- [x] Theme helpers: `toolbar_bg()`, `icon_color()`, `sel_color()`, `hover_color()`

### Scripting (Pine Script Interpreter)
- [x] Lexer, Parser, Runtime types
- [x] `register_builtins()` function
- [x] Syntax highlighting with full Pine v5 keyword set
- [x] Parse errors with line/column
- [x] Basic runtime execution with plot output
- [x] Strategy metrics: total_trades, win_rate, net_profit, max_drawdown
- [ ] No series/array operations beyond basic runtime
- [ ] No `input.*` UI-driven parameter integration
- [ ] No indicator overlay/pane placement logic from scripts
- [ ] Strategy trade simulation completeness unverified

### Backtesting
- [x] Config, Engine, Metrics, Portfolio, Strategy, Trade modules
- [x] Strategy metrics struct (total_trades, win_rate, net_profit, max_drawdown)
- [ ] Full trade execution loop completeness unverified
- [ ] No broker simulation (commission models, slippage)

### Data Validation
- [x] `DataValidator` — Timestamp ordering, OHLC sanity checks
- [x] `ValidationResult` variants: Valid, TsMismatch, DuplicateTs, InvalidOHLC
- [x] Builder-style configuration

### Templates
- [ ] `TemplateManager` is a stub type (re-exported from `ui/stubs.rs`)
- [ ] No template persistence or serialization
- [ ] No user-facing template management UI (beyond drawing toolbar placeholder)

---

## Known Issues

| Priority | Issue | Location |
|----------|-------|----------|
| High | Regression Trend R-squared hardcoded to 0.95 | `drawings/rendering/channels.rs` |
| High | Anchored VWAP std_dev is visual approximation | `drawings/rendering/trading/volume_profile.rs` |
| High | Volume profile tools use approximate bar lengths | `drawings/rendering/trading/volume_profile.rs` |
| High | TPO full renderer not wired into dispatch | `chart/rendering/candles/advanced.rs` |
| Medium | MACD histogram renders as line, not bars | `chart/renderers/indicator.rs` |
| Medium | Ichimoku cloud renders as lines, not filled regions | `chart/renderers/indicator.rs` |
| Medium | Parabolic SAR renders as line, not dots | `chart/renderers/indicator.rs` |
| Medium | 80+ drawing tools use point-proximity hit test only | `drawings/services/interaction.rs` |
| Medium | Drawing styling properties not fully serialized | `drawings/persistence.rs` |
| Medium | FibonacciConfig not serialized | `drawings/persistence.rs` |
| Medium | IndicatorFactory only registers 20 of ~130 indicators | `studies/` |
| Medium | Rotated Rectangle renders as axis-aligned | `drawings/rendering/shapes.rs` |
| Low | Double Curve uses sine offset, not bezier | `drawings/rendering/shapes.rs` |
| Low | Paintbrush identical to Brush | `drawings/rendering/shapes.rs` |
| Low | Cypher Pattern identical to XABCD | `drawings/rendering/patterns.rs` |
| Low | Export browse button uses placeholder path | `ui/export.rs` |

---

## Feature Gap Summary

### What's built (ready to ship)
- 17 of 20 chart types with full rendering
- Complete pan/zoom/crosshair/scale system
- 80 of 88 drawing tools with real geometry (95 total incl. 4 cursor modes + 3 placeholders)
- ~130 technical indicators with complete calculation logic
- Full UI component library (toolbars, dialogs, panels, controls)
- 24 reusable ui_kit building blocks
- Theme system with 300+ design tokens and multiple presets
- Pine Script basic interpreter with syntax highlighting
- Data validation, configuration

### What needs work (P1 — core functionality gaps)
- [ ] Wire TPO full renderer into chart type dispatch
- [ ] Implement proper Volume Footprint with bid/ask data support
- [ ] Fix Regression Trend to compute actual R-squared from data
- [ ] Fix Anchored VWAP to compute std_dev from price/volume data
- [ ] Fix Volume Profile drawing tools to use real volume data
- [ ] Add indicator-specific rendering (histograms, filled clouds, dot plots)
- [ ] Register all ~130 indicators in `IndicatorFactory`
- [ ] Complete `ISeriesApi` and `ITimeScaleApi` implementations
- [ ] Build multi-pane layout manager (`PaneManager`)

### What needs work (P2 — polish and completeness)
- [ ] Improve drawing hit testing (line-body hit tests for all tools)
- [ ] Serialize all drawing styling properties (line_style, fill_color, font_size, etc.)
- [ ] Serialize `FibonacciConfig` custom levels
- [ ] Implement Rotated Rectangle with actual rotation
- [ ] Implement Double Curve with proper bezier geometry
- [ ] Differentiate Cypher Pattern from XABCD
- [ ] Differentiate Paintbrush from Brush
- [ ] Implement Box Zoom Measure mode
- [ ] Export: integrate native file dialog, implement `Exportable`
- [ ] Template system: persistence, serialization, user management UI

### What's missing entirely (P3 — new features for full parity)
- [ ] Localization / i18n
- [ ] Accessibility (a11y)
- [ ] Image drawing tool (load and display user images on chart)
- [ ] Social features (Tweet embed, Idea sharing)
- [ ] Real-time data streaming protocol
- [ ] Server-side rendering for chart snapshots
- [ ] Chart comparison / overlay mode
- [ ] Advanced Pine Script v5 (series ops, arrays, inputs, strategy simulation)
- [ ] Multi-symbol data correlation
- [ ] Alerts engine (evaluation, triggering, notification delivery)
- [ ] Print / PDF export
