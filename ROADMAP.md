# egui-charts Roadmap

Feature status and roadmap.
Each item is marked as complete, partial, or missing.

---

## Chart Rendering Engine

### Chart Types (18/18 complete)

#### Standard OHLC
- [x] **Candles**: Filled body with optional border, wicks, optional volume bars
- [x] **Bars**: OHLC bars with tick marks
- [x] **Hollow Candles**: Hollow body when bullish, filled when bearish
- [x] **Volume Candles**: Bar width scaled 30-100% by relative volume

#### Line-Based
- [x] **Line**: Close-price line with configurable `PriceSource`
- [x] **Line with Markers**: Line + circle markers at each data point
- [x] **Step Line**: Staircase-style step interpolation

#### Area-Based
- [x] **Area**: Filled polygon under the price line (about 20% alpha)
- [x] **HLC Area**: Band fill between high and low, close line on top
- [x] **Baseline**: Colored segments above/below a reference baseline

#### Range-Based
- [x] **High-Low**: Thin rects from high to low per bar
- [x] **Range Bars**: Time-independent range bar transformation + rendering

#### Japanese
- [x] **Heikin-Ashi**: Full HA formula (HA_close, HA_open), rendered as candles
- [x] **Renko**: Brick rendering via `to_renko_bricks()`, max 2000 elements
- [x] **Kagi**: Thick/thin line segments with shoulder/elbow logic
- [x] **Line Break**: 3-line break bricks via `to_line_break_lines()`
- [x] **Point & Figure**: X/O column rendering, max 1000 columns

#### Advanced
- [x] **Time Price Opportunity (TPO)**: Rendered by the chart type dispatch (value area, POC, initial balance); default 30-minute periods, sessions split on the UTC date, profiles built from the visible bars

### Pan / Zoom
- [x] Mouse wheel zoom (centered on cursor)
- [x] Shift+wheel horizontal pan
- [x] Click-drag pan (left button)
- [x] Pinch-to-zoom (multi-touch)
- [x] Two-finger pan
- [x] Kinetic scrolling with exponential damping
- [x] Price axis drag-scaling (the scale compounds per frame; needs a per-frame delta)
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
- [x] Price label on Y-axis with a decorative "+" glyph (not clickable)
- [x] Time label on X-axis with adaptive format

### Scales
- [x] Linear (normal) price scale
- [ ] Logarithmic price scale (conversion math in src/scales, not used by rendering)
- [ ] Percentage price scale (conversion math in src/scales, not used by rendering)
- [ ] Indexed-to-100 price scale (conversion math in src/scales, not used by rendering)
- [x] Auto-scale with a fixed 10% margin
- [x] Manual range override
- [ ] Inverted scale (setting exists, rendering ignores it)
- [ ] Dual price scale manager (left/right): type exists, not used by the chart
- [x] Hierarchical time tick generation with weight system
- [x] Price mark generation
- [ ] Price formatters (default, currency, percentage, scientific, volume, custom) defined but not used by axis labels
- [x] Default time formatter on the time axis (locale, relative, custom defined but not injectable; locale is a stub)

### Multi-Pane
- [x] `IndicatorPane` widget for separate-pane indicators
- [x] Predefined configs: RSI, MACD, etc.
- [x] X-axis synchronization with main chart
- [ ] No `PaneManager`: indicator sub-panes manually constructed

---

## Drawing Tools (74/88 working as described, 3 placeholders)

Note: tools with a fixed point count (parallel channel, pitchforks, triangle, harmonic and chart patterns, Elliott waves) currently complete one click early with the last anchor duplicated; drag that handle to set the final point.

### Lines (9/9)
- [x] Trend Line: Segment distance hit test, start/end/middle handles
- [x] Ray: extends to the right edge; a ray drawn leftward points the wrong way
- [x] Extended Line: Extends both directions
- [x] Horizontal Line: Y-proximity hit test
- [x] Horizontal Ray
- [x] Vertical Line: X-proximity hit test
- [x] Cross Line: Horizontal + vertical
- [x] Trend Angle: Line + dashed reference + arc + angle/slope label
- [x] Measure: colored rect, arrows and info box with price change, percent and bar count; pips assume a 0.01 tick, time assumes one-minute bars, volume not shown

### Channels (4/4)
- [x] Parallel Channel: Two parallel lines + connectors, explicit hit test
- [x] Flat Top/Bottom: Trend line + horizontal + Support/Resistance labels
- [x] Disjoint Channel: Two parallel segments + dashed connectors + width label
- [x] Regression Trend: least-squares line through the closes between the anchors, 1σ and 2σ bands, R² and slope label

### Pitchforks (2/4)
- [x] Pitchfork (Andrews)
- [ ] Schiff Pitchfork: median origin not moved off the first anchor, so the geometry is wrong
- [x] Modified Schiff Pitchfork
- [ ] Inside Pitchfork: draws the same lines as the Andrews pitchfork

### Fibonacci (9/11)
- [x] Fibonacci Retracement: default levels with price labels; custom levels and extend only through the fib_config field set by host code; level 0 at the first anchor, Reverse not applied
- [x] Fibonacci Extension: two-point extension levels; no third anchor to project from
- [x] Fibonacci Channel: levels parallel to the base line; always drawn on one side regardless of the third point
- [ ] Pitchfan: two-point fan of 7 lines over a fixed 90 degree spread, not built from a pitchfork
- [x] Fibonacci Time Zones: Vertical lines at Fibonacci sequence intervals
- [x] Fibonacci Circles: Concentric circles at 8 ratios
- [x] Fibonacci Speed Resistance Arcs: semicircular arcs at 6 ratios, always below the second anchor
- [x] Fibonacci Speed Fan: 7 ratios with colored fills
- [x] Fibonacci Spiral: logarithmic golden-ratio spiral drawn with straight segments around the first anchor
- [ ] Fibonacci Wedge: placeholder of fixed-length rays at fractions of the anchor angle
- [x] Trend-Based Fib Time: vertical lines at ratio multiples of the two-anchor span; no third anchor

### Gann (3/4)
- [ ] Gann Fan: 9 lines drawn (lines more than 200 px off the chart are skipped), but the 8x1 to 1x8 labels are inverted
- [x] Gann Square: Grid with diagonals, inscribed circle, degree labels
- [x] Gann Box: 8x8 grid with diagonals; angle lines from the top-left corner only
- [x] Gann Fixed: 1:1 forced-square with 4x4 grid

### Patterns (3/6)
- [x] XABCD Pattern: Labels, filled triangles, Fibonacci ratio display
- [ ] Head and Shoulders: labels drawn, but the neckline joins the shoulders instead of the troughs, so the target is wrong
- [x] ABCD Pattern: Labels, fill, CD/AB ratio with color coding
- [x] Triangle Pattern: numbered points and fill; apex lines use only the first 4 of 7 points
- [ ] Three Drives: completes at 6 points, so the PRZ box (drawn at 7) never appears
- [ ] **Cypher Pattern**: Delegates entirely to XABCD renderer; Cypher-specific ratios not visually distinct

### Elliott Waves (5/5)
- [x] Elliott Impulse: 6 points, motive/corrective fill, Fibonacci 1.618 target
- [x] Elliott Correction: 4 points (5-A-B-C), corrective fill
- [x] Elliott Triangle: 5 points (A-E), converging trendlines
- [x] Elliott Double Combo: 5 points, only the first 3 labelled (W-X-Y)
- [x] Elliott Triple Combo: 7 points, only the first 5 labelled (W-X-Y-X-Z)

### Cycles (3/3)
- [x] Cyclic Lines: Repeating vertical lines, cycle numbers, period label
- [x] Time Cycles: Concentric semicircular arcs (0.5x-3x), mesh fills
- [x] Sine Line: Full sine wave with amplitude guides

### Projection / Trading (5/6)
- [x] Long Position: entry, target and independent stop zones; risk/reward from the levels, P&L from the bars after the entry
- [x] Short Position
- [x] Forecast: price change over the bar span, resolved against the bars up to the target bar
- [ ] Ghost Feed: faded polyline through the clicked points; draws no bars
- [x] Bars Pattern: copies the selected bars to the projection anchor
- [x] Projection Tool: two-point measured move with labels; the AB=CD start point is synthesized, not user placed

### Volume / VWAP (3/3)
- [x] Anchored VWAP: VWAP of the typical price from the anchor bar, with 1σ and 2σ volume-weighted bands
- [x] Fixed Range Volume Profile: each bar's volume spread over its high-low range, POC and 70% value area
- [x] Anchored Volume Profile: the same profile from the anchor bar to the latest bar

### Measurements (4/4)
- [x] Price Range: Two hlines + vertical connector + price diff label
- [x] Date Range: Two vlines + horizontal connector + bar count
- [x] Date and Price Range: Filled rect + combined label
- [x] Info Line: angled line and info box with price change, percent and bars; time assumes one-minute bars, angle sign inverted

### Brushes / Freehand (2/3)
- [x] Brush: Freehand stroke with connected segments
- [x] Highlighter: Wider semi-transparent stroke
- [ ] **Paintbrush**: Identical implementation to Brush (calls `render_brush()`)

### Arrows (4/4)
- [x] Arrow: Line + arrowhead
- [x] Arrow Marker: Colored filled triangle
- [x] Arrow Mark Up
- [x] Arrow Mark Down

### Shapes (8/10)
- [x] Rectangle: Fill, dashed midlines, dimension labels, 4-corner handles
- [x] Circle
- [x] Ellipse: Path approximation
- [x] Triangle: 3-point polygon
- [x] Arc: Arc/sector rendering
- [x] Polyline: Connected segments with vertex markers
- [x] Path: Freeform connected segments
- [x] Curve: Quadratic bezier
- [ ] **Rotated Rectangle**: Renders as axis-aligned only; rotation not implemented
- [ ] **Double Curve**: Uses sine offset approximation, not true cubic bezier

### Annotations (10/11)
- [x] Text Label: Background box + border + text + anchor marker
- [x] Anchored Text: Background box + anchor icon + text
- [x] Note: Sticky note with folded corner + tooltip
- [x] Anchored Note: Pin head + needle + expandable text box
- [x] Flag Note: Flag pole + pennant + optional text
- [x] Price Label: Arrow badge with price text
- [x] Price Note: Dashed hline + note box
- [x] Callout: Text box with auto-edge pointer triangle
- [x] Comment: Speech bubble + expandable text
- [ ] Table: fixed grid of 3 columns by 4 rows (header included) with placeholder headers and cell text
- [x] Signpost: Dashed vertical + sign plate + arrow

### Content / Media (0/3)
- [ ] **Image**: Decorative placeholder only (checkerboard + icon); no image loading
- [ ] **Tweet**: Decorative card only; no API integration
- [ ] **Idea**: Decorative card only; no service connection

### Icons (0/1)
- [ ] Font Icon: renderer exists (text glyph sized from stroke width) but no toolbar control selects the tool
- Note: Icons module is feature-gated behind `icons` feature (enabled by default)

### Cursors (4 modes, no rendering needed)
- [x] Cross, Dot, Arrow, Eraser

### Drawing System Infrastructure

#### Hit Testing
- [x] Geometry-accurate hit tests for: TrendLine, Measure, HorizontalLine, VerticalLine, Rect, Fibonacci (3 variants), GannFan, ParallelChannel
- [ ] 80+ tools use point-proximity fallback only (must click on anchor point, not on drawing body)

#### Handle System
- [x] Coordinate-aware handles for: TrendLine, Measure, Rect, HorizontalLine, VerticalLine, Fib Retracement/Extension/Channel, Gann Fan/Square/Box
- [x] Generic indexed handle fallback for all other tools (functional but basic)

#### Persistence
- [x] Serializable `DrawingStorage` types (id, tool type, points, color, line width, timestamps)
- [ ] Restore does not work: `stored_to_drawing` leaves `chart_points` empty, so restored drawings vanish on the next frame; some tool types fail to parse back; nothing calls save or load
- [x] Version field with migration stub
- [ ] `FibonacciConfig` (custom levels) not serialized: user customizations lost on save/load
- [ ] `line_style`, `fill_color`, `font_size`, `extend_left/right`, `arrow_styles`, `z_order` not serialized

#### Properties Dialog
- [x] Multi-tab dialog: Style, Text, Visibility, Coordinates
- [x] Style controls for color (with alpha), stroke width, line style, fill and extend; only color and width are applied when drawing
- [ ] Per-timeframe visibility toggles: shown in the dialog, not applied when rendering
- [ ] Direct point coordinate editing: tab with blank fields, not loaded from or applied to the drawing

---

## Technical Indicators (131 registered)

All 131 registered indicators have calculation code: no `todo!()` or `unimplemented!()` stubs.
Some indicators appear in multiple categories below (e.g., CVD in both Volume and Order Flow).

### Rendering Limitation
All indicators use a **generic polyline renderer**. There is no indicator-specific rendering. This means:
- [ ] MACD histogram renders as a line instead of bars
- [ ] Ichimoku cloud renders as 5 separate lines instead of filled cloud regions
- [ ] Parabolic SAR renders as a connected line instead of individual dots
- [ ] Volume Profile outputs `IndicatorValue::Multiple` but there is no horizontal bar chart renderer
- [ ] Multi-line studies that keep the default line count of 1 draw only their first line (signal lines, second DI or VI line, lower bands, pivot levels)

### Moving Averages (22/22)
- [x] SMA, EMA, WMA, DEMA, TEMA, HMA, ALMA, VWMA, LSMA, SWMA
- [x] KAMA, Smoothed MA (Wilder), McGinley Dynamic, VIDYA, ZLEMA
- [x] Hamming MA
- [x] Guppy MA (GMMA): 12 EMAs (6 short, 6 long)
- [x] Multiple MA: SMA 20/50/100/200
- [x] MA Cross, EMA Cross
- [x] MA Channel, Envelopes

### Momentum Oscillators (37/37)
- [x] RSI, MACD, VW-MACD, Stochastic, Stochastic RSI
- [x] Williams %R, CCI, ROC, Momentum, Aroon, Awesome Oscillator
- [x] Ultimate Oscillator, Connors RSI, Coppock Curve
- [x] DPO, TRIX (signal line not drawn), Mass Index, Chande Momentum (CMO)
- [x] True Strength Index (TSI), KDJ, Know Sure Thing (KST): only the first line is drawn (no D/J or signal lines)
- [x] Schaff Trend Cycle, Double Stochastic, Elder Ray (Bear Power not drawn)
- [x] Fisher Transform (non-standard smoothing, signal not drawn), Ehlers Fisher (trigger not drawn), Force Index
- [x] Center of Gravity (signal not drawn), Pretty Good Oscillator, Rainbow Oscillator
- [x] Relative Vigor Index, Stochastic Momentum Index (signal lines not drawn)
- [x] Ultimate Momentum, Trend Intensity Index, Qstick
- [x] Elder Impulse System, Trend Detection Index

### Trend / Overlay (8/8)
- [x] ADX: ADX, +DI, -DI lines
- [x] SuperTrend: ATR-based trailing stop
- [x] Ichimoku Cloud: Tenkan and Kijun; Senkou A/B and Chikou computed but not displaced, all lines start at bar 52
- [x] Parabolic SAR: SAR values with acceleration factor, drawn as a connected line
- [x] Directional Movement (DMI): +DI and -DI computed; only +DI is drawn
- [x] Vortex Indicator: VI+ and VI- computed; only VI+ is drawn
- [x] Choppiness Index: ATR sum / range
- [x] Linear Regression: Rolling LinReg endpoint overlay

### Volatility (19/19)
- [x] Bollinger Bands: SMA +/- k*StdDev
- [x] ATR: Wilder smoothed true range
- [x] Keltner Channels: EMA +/- ATR*multiplier
- [x] Donchian Channels: N-bar high/low
- [x] Standard Deviation
- [x] Bollinger Band Width (BBW), %B
- [x] Chandelier Exit: trailing stop from a simple average of true range; only the long exit is drawn
- [x] Squeeze Momentum: momentum line drawn; BB-inside-KC flag computed but not shown
- [x] Price Channel (same values as Donchian; only the upper line is drawn)
- [x] Historical Volatility: Annualized log-return std dev
- [x] Chaikin Volatility
- [x] Acceleration Bands (only the upper band is drawn)
- [x] Close-to-Close Vol, OHLC Volatility (Garman-Klass), Parkinson Vol
- [x] Volatility Index: ATR-based annualized %
- [x] Standard Error, Standard Error Bands

### Volume (21/23)
- [x] OBV, VWAP (optional reset on UTC date change; the indicator dialog builds it without reset, so it is cumulative), Anchored VWAP
- [x] MFI, A/D Line, CMF, Klinger Oscillator (signal not drawn)
- [x] Ease of Movement, Net Volume, Volume ROC, Volume Oscillator
- [x] CVD: Cumulative Volume Delta (3 estimation modes)
- [ ] Volume Profile Visible Range, Volume Profile Fixed Range: bin volume over the whole history; bin 0 is plotted on the price scale, so nothing visible draws
- [x] VWMA, Balance of Power
- [x] Volume Price Trend, NVI, PVI
- [x] Williams AD, PVT Enhanced
- [x] Market Facilitation Index (Bill Williams)
- [x] Commodity Selection Index

### Correlation / Statistics (9/9)
- [x] Correlation with time (Pearson of close vs bar index), Correlation Log Returns (log returns vs bar index)
- [x] Linear Regression, Linear Regression Slope
- [x] Rank Correlation Index (Spearman)
- [x] Standard Error, Standard Error Bands
- [x] Majority Rule, Accumulation Swing Index

### Price Derivatives (9/9)
- [x] Typical Price, Weighted Close, Median Price
- [x] OHLC4, HLC3, HL2
- [x] True Range
- [x] Spread Study (bar high minus low), Ratio Study (close/open percent change); single symbol only

### Market Structure (2/2)
- [x] Advance/Decline Line (simulated from single symbol)
- [x] 52-Week High/Low %

### Support / Resistance (1/2)
- [x] Pivot Points: Standard, Fibonacci, Woodie, Camarilla, DeMark formulas over a rolling N-bar window; only the pivot line is drawn
- [ ] ZigZag: percentage swing detection computes pivots, but they are isolated values and nothing is drawn

### Order Flow (1/4)
- [ ] Iceberg Detector: volume anomaly and confidence score estimated from bars; the scores are plotted on the price scale, so nothing visible is drawn on a normal chart
- [ ] Stop Run: swing-break-and-reversal pattern on bars; emits rare confidence values on the price scale, so it seldom draws anything
- [ ] Liquidity Tracker: rolling liquidity ratio plotted on the price scale; the sweep test flags prices the bar never traded
- [x] CVD (also in Volume)

### Price Oscillators (1/1)
- [x] PPO: Price Oscillator (signal line computed, not drawn)

### Factory Coverage
- [x] `IndicatorFactory` registers all 131 built-in indicators by name
- [ ] The indicator dialog offers 24 fixed entries; the other indicators are reachable only from code

---

## UI Components

### Drawing Toolbar
- [x] Full sidebar with all tool categories
- [x] Submenu rendering with icons, section headers, shortcut labels
- [ ] Cursor submenu (Cross, Dot, Arrow, Eraser) renders; none of the four changes chart behaviour (Eraser sets a flag nothing reads)
- [ ] Color picker popup (4x5 grid exists in code but no control opens it)
- [x] Magnet, StayInDrawing, Lock toggles (magnet and lock have no effect yet)
- [ ] Drawing tool favorites (star is paint-only, no toggle, favorites bar not rendered)
- [x] Horizontal floating toolbar variant
- [ ] Drawing template persistence (struct exists but no save/load backend)

### Top Toolbar
- [x] Symbol search pill (custom-drawn with icons)
- [x] Compare/Add symbol button renders and emits OpenCompare (no compare overlay)
- [x] Timeframe selector dropdown grouped by unit (only 9 of 25 presets map to a Timeframe)
- [x] Chart type selector dropdown (all 18 types with icons + descriptions)
- [x] Indicators button
- [x] Alert, Replay, Undo, Redo buttons
- [x] Right rail buttons render: Quick Search, Publish, Trade, Camera, Fullscreen, Settings, Save, Layout (Publish, Trade and Layout have no backing feature)
- [ ] Quick settings menu (exists but not rendered by the toolbar; most fields it writes have no reader)
- [ ] Settings "Scales Properties..." button closes menu but opens nothing
- [ ] Save button has no dropdown/sub-options
- [ ] Layout menu / chart grid selector not rendered inline

### Indicator Dialog
- [x] Modal overlay with dim background
- [x] Left category sidebar
- [x] Search bar with case-insensitive substring filtering
- [x] Tabs: Indicators, Strategies, Profiles, Patterns
- [x] Favorite toggling
- [x] Configuration panel for parameters

### Symbol Search Dialog
- [x] Real-time text filtering
- [x] Keyboard navigation (Up/Down/Enter/Escape)
- [x] Scrollable results with symbol name and description (no exchange badges)
- [ ] Compare symbols dialog (two-column widget exists but is never shown; search does not filter)

### Timeframe Toolbar
- [x] Date range buttons (1D to All) render and emit DateRangeChanged; mapping to a time span is left to the host
- [ ] Go-to-date navigation (picker exists; choosing a date does not scroll the chart)
- [x] Clock display in UTC (timezone button opens nothing)
- [ ] RTH/Session toggle (label renders; no session menu and no effect on data)
- [ ] ADJ toggle (flips a flag nothing reads)

### Floating Selection Toolbar
- [x] FloatingSelectionToolbar widget with drag handle (not shown by chart or demo)
- [x] Color slot pickers in the FloatingSelectionToolbar widget (not wired)
- [ ] Settings, Duplicate, Lock, Visibility, Delete buttons (emit actions no code handles)
- [x] Delete button with red hover in the FloatingSelectionToolbar widget (not wired)
- [x] Viewport clamping in the FloatingSelectionToolbar widget (not wired)

### Chart Controls
- [ ] Chart control bar Zoom In/Out/Reset (widget exists, never rendered, actions unhandled)
- [ ] Auto-scale toggle in chart control bar (unwired)
- [ ] Log scale toggle in chart control bar (unwired)
- [ ] Percentage toggle in chart control bar (unwired)

### Replay System
- [x] Play/Pause/Stop buttons
- [x] Step forward/backward
- [x] Speed control with presets
- [x] Progress bar with seek (click/drag)
- [x] Keyboard shortcuts via `ReplayKeyboardHandler` (Space, arrows, Home/End, M for marker, Ctrl+R, Escape)
- [x] Compact variant
- [ ] SVG icon replay toolbar (exists; actions not mapped to ReplayController, never rendered)

### Dialogs
- [x] **Alert Dialog**: 3 tabs (Price, Indicator, Volume) with form validation
- [x] **Series Settings**: 7 tabs (Symbol, Status Line, Scales, Canvas, Trading, Alerts, Events)
- [x] **Drawing Properties**: 4 tabs (Style, Coordinates, Visibility, Text); only color, width, line style, extend and label are applied
- [x] **Script Editor**: syntax highlighting, parse and runtime errors with line/column; scripts run against an empty bar set, so no plot output or metrics are shown
- [x] **Keyboard Shortcuts**: help dialog listing the KeyboardAction table; chart and drawing keys are hardcoded in chart/interaction.rs and chart/tool_interaction/
- [x] **Drawing Context Menu**
- [x] **Series Context Menu** (Settings, Add indicator and Add alert work; the other 12 entries do nothing)

### Widget Bar (Right Sidebar)
- [x] Vertical icon bar (Alerts, Object Tree, Help)
- [x] Notification count badge
- [x] Active state highlighting

### Object Tree Panel
- [x] Hierarchical tree with indicator/drawing rows
- [x] Filter controls (text + type dropdown)
- [x] Visibility/lock toggle icons per item
- [x] Bulk actions: Show All, Hide All (Lock All, Unlock All, Delete Selected emitted but not handled)
- [x] Object tree context menu with keyboard shortcuts (move, zoom to, rename, hide selected, duplicate selected, delete selected and lock all are not handled)
- [x] Selection model (single, additive, range, Ctrl+A)
- [x] Footer stats (total, indicators, drawings, visible, selected)
- [x] Collapsible OHLCV data window

### Alerts Panel
- [ ] Alerts panel with 4 sections renders, but AlertManager cannot be populated and the demo does not use it
- [ ] Inline add forms (inputs render, but Create clears the form before the host can read it)
- [ ] Alert rows with status color and remove/toggle (unreachable: AlertManager has no add API)
- [ ] Alert notification popup (defined, never shown)

### Export
- [x] Export dialog UI (format selection, size options, path input)
- [x] `Exportable` trait defined (`export`, `export_png`, `export_svg`)
- [ ] No `Exportable` implementation in-crate (host must implement)
- [ ] Save path is typed by hand (no native file dialog)
- [ ] No actual PNG/SVG rendering logic

### Other UI
- [x] Context menu: 20 entries with icons, shortcuts, disabled state, click-outside close
- [ ] Watermark system (Text/Image, 5 anchors plus custom, tile mode, WatermarkManager) defined but never drawn by the chart
- [ ] Emoji picker: Structure exists but actual emoji grid rendering unverified
- [ ] Multi-chart layout component (only ChartSyncOptions exists; module is a stub)
- [x] Symbol header display
- [x] Connection status indicator
- [x] Replay status indicator widget (inline label, not an overlay; not used by the replay UI or demo)

---

## UI Kit (Reusable Components)

All ui_kit components are **fully implemented** generic building blocks:

- [x] Button (themed, variant-based: Primary/Secondary/Outlined/Danger/Ghost)
- [x] IconButton (SVG icon + optional label)
- [x] SplitButton (icon + dropdown arrow)
- [x] ColorPicker (80 swatches: grayscale row, main row, 10 colors x 6 variations)
- [x] CommandPalette (fuzzy search, keyboard nav, command registry)
- [x] DialogFrame / DialogHeader / DialogFooter
- [x] EmptyState placeholder
- [x] Filter widget
- [x] FormGrid (2-column layout for settings forms)
- [x] Card frame
- [x] ListItem (LabelContent, ButtonContent, PropertyContent variants)
- [x] LoadingIndicator (spinner)
- [x] Modal + ConfirmDialog + PromptDialog
- [x] Notifications / Toast system (caller-owned Toasts collection, 4 severity levels)
- [x] PanelHeader (section header with right-side content slot)
- [x] Section (collapsible)
- [x] SidebarLayout (left nav + right content)
- [x] StatusMessage (inline status with icon)
- [x] TabBar (generic over `T: TabLabel`)
- [x] ResponsiveToolbar (horizontal scrollable)
- [x] ButtonGroup (grouped toggle buttons)
- [x] TradingLabels (PnlLabel, SideLabel for buy/sell/long/short)
- [x] Autocomplete (text field + dropdown suggestions)
- [x] Alert (inline banner)

---

## Infrastructure

### Data Layer
- [x] `DataSource` trait defined with error types (8 variants)
- [x] `DataSourceError` with `is_recoverable()` and `is_conn_error()` helpers
- [ ] No in-crate `DataSource` implementations (streaming, tick feeds, symbol search all in host)

### Configuration
- [x] `ChartConfig`: Comprehensive chart configuration
- [x] `ChartOptions`: User-facing options
- [x] `CrosshairOptions`: Crosshair style/mode/color
- [x] `KeyboardOptions` (enable, pan amount, zoom step); key bindings are fixed, `KeyboardAction` is a display table for the help dialog

### Theming
- [x] Full theme system with semantic tokens
- [x] Dark and light presets
- [x] Classic preset
- [x] Design token system (RON-based, ~300+ tokens)
- [x] Token categories: sizing, spacing, rounding, stroke widths, semantic colors
- [x] Theme helpers: `toolbar_bg()`, `icon_color()`, `sel_color()`, `hover_color()`

### Scripting (built-in scripting language interpreter)
- [x] Lexer, Parser, Runtime types
- [x] `register_builtins()` function
- [x] Syntax highlighting for the scripting language's common keywords, builtins and namespaces
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
- [x] `DataValidator`: Timestamp ordering, OHLC sanity checks
- [x] `ValidationResult` variants: Valid, TsMismatch, DuplicateTs, InvalidOHLC
- [x] Builder-style configuration

### Templates
- [ ] `TemplateManager` keeps templates in memory only (`ui/model.rs`)
- [ ] No template persistence or serialization
- [ ] No user-facing template management UI (beyond drawing toolbar placeholder)

---

## Known Issues

| Priority | Issue | Location |
|----------|-------|----------|
| Medium | MACD histogram renders as line, not bars | `chart/renderers/indicator.rs` |
| Medium | Ichimoku cloud renders as lines, not filled regions | `chart/renderers/indicator.rs` |
| Medium | Parabolic SAR renders as line, not dots | `chart/renderers/indicator.rs` |
| Medium | 80+ drawing tools use point-proximity hit test only | `drawings/services/interaction.rs` |
| Medium | Drawing styling properties not fully serialized | `drawings/persistence.rs` |
| Medium | FibonacciConfig not serialized | `drawings/persistence.rs` |
| Medium | Restored drawings vanish on the next frame | `drawings/persistence.rs` |
| Medium | Magnet snapping never applied to drawings | `drawings/manager.rs` |
| Medium | Rotated Rectangle renders as axis-aligned | `drawings/rendering/shapes.rs` |
| Low | Double Curve uses sine offset, not bezier | `drawings/rendering/shapes.rs` |
| Low | Paintbrush identical to Brush | `drawings/rendering/shapes.rs` |
| Low | Cypher Pattern identical to XABCD | `drawings/rendering/patterns.rs` |
| Low | Export browse button uses placeholder path | `ui/export.rs` |

---

## Feature Gap Summary

### What's built
- 18 of 18 chart types render
- Pan/zoom and crosshair; the price axis renders on a linear scale only
- 74 of 88 drawing tools work as described, several with limits noted above (95 total incl. 4 cursor modes + 3 placeholders)
- 131 registered indicators with calculation code; multi-line studies draw only their first line
- UI component library (toolbars, dialogs, panels, controls); several widgets are not wired yet, see above
- 24 reusable ui_kit building blocks
- Theme system with 300+ design tokens and multiple presets
- Basic interpreter for the crate's scripting language, with syntax highlighting
- Data validation, configuration

### What needs work (P1: core functionality gaps)
- [ ] Add a Volume Footprint chart type driven by bid/ask data
- [ ] Add indicator-specific rendering (histograms, filled clouds, dot plots)
- [ ] Build multi-pane layout manager (`PaneManager`)

### What needs work (P2: polish and completeness)
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

### What's missing entirely (P3: new features)
- [ ] Localization / i18n
- [ ] Accessibility (a11y)
- [ ] Image drawing tool (load and display user images on chart)
- [ ] Social features (Tweet embed, Idea sharing)
- [ ] Real-time data streaming protocol
- [ ] Server-side rendering for chart snapshots
- [ ] Chart comparison / overlay mode
- [ ] Advanced scripting language features (series ops, arrays, inputs, strategy simulation)
- [ ] Multi-symbol data correlation
- [ ] Alerts engine (evaluation, triggering, notification delivery)
- [ ] Print / PDF export
