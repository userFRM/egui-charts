//! Builder pattern for constructing [`TradingChart`] instances.
//!
//! This module provides [`ChartBuilder`], a fluent API for creating fully-configured
//! trading charts, and [`TradingChart`], the runtime wrapper that owns the chart widget,
//! data source, indicator registry, and drawing manager.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use egui_charts::ChartBuilder;
//! use egui_charts::model::Timeframe;
//!
//! // Full-featured chart with drawing tools and indicators
//! let mut chart = ChartBuilder::extended()
//!     .with_symbol("BTCUSDT")
//!     .with_timeframe(Timeframe::Min1)
//!     .with_data_src(Box::new(my_data_source))
//!     .build();
//!
//! // In your egui update loop:
//! chart.update();   // polls data source, handles progressive loading
//! chart.show(ui);   // renders the chart widget
//! ```
//!
//! # Builder Variants
//!
//! | Constructor | Use Case |
//! |------------|----------|
//! | [`ChartBuilder::new()`] | Sensible defaults, auto-fetch 1000 bars |
//! | [`ChartBuilder::extended()`] | Full trading terminal (drawings + indicators) |
//! | [`ChartBuilder::price_chart()`] | Minimal sparkline / dashboard widget |
//! | [`ChartBuilder::options_chart()`] | Non-time-based displays (options chains) |

use crate::config::{ChartConfig, ChartOptions, RealtimeButtonPos};
use crate::data::DataSource;
use crate::drawings::DrawingManager;
use crate::model::BarData;
use crate::model::ChartType;
use crate::model::Timeframe;
use crate::studies::IndicatorRegistry;
use crate::theme::Theme;
use crate::widget::Chart;
use log;

/// Fluent builder for constructing a [`TradingChart`].
///
/// Use one of the constructor methods ([`new`](Self::new), [`extended`](Self::extended),
/// [`price_chart`](Self::price_chart), [`options_chart`](Self::options_chart)) and chain
/// configuration calls before calling [`build`](Self::build) to produce the final chart.
///
/// All setter methods return `Self` and are marked `#[must_use]` to prevent accidental
/// drops of the builder mid-chain.
///
/// # Examples
///
/// ```rust,ignore
/// use egui_charts::{ChartBuilder, Timeframe, ChartType, Theme};
///
/// let chart = ChartBuilder::new()
///     .with_symbol("ETHUSDT")
///     .with_timeframe(Timeframe::H1)
///     .with_theme(Theme::dark())
///     .with_chart_type(ChartType::Candles)
///     .with_visible_candles(120)
///     .build();
/// ```
pub struct ChartBuilder {
    data_src: Option<Box<dyn DataSource>>,
    symbol: Option<String>,
    timeframe: Timeframe,
    theme: Theme,
    chart_type: ChartType,
    config: ChartConfig,
    chart_options: ChartOptions,
    indicators: Option<IndicatorRegistry>,
    drawing_manager: Option<DrawingManager>,
    visible_candles: usize,
    /// Number of bars to fetch on initial load (None = no auto-fetch)
    initial_bars: Option<usize>,
    /// Auto-fetch on symbol change
    auto_fetch_on_symbol_change: bool,
    /// Auto-fetch on timeframe change
    auto_fetch_on_timeframe_change: bool,
}

impl ChartBuilder {
    /// Create a new chart builder with sensible defaults
    ///
    /// By default, charts will auto-fetch 1000 bars on initial load and when
    /// symbol/timeframe changes. Use `with_initial_bars(None)` to disable auto-fetch.
    pub fn new() -> Self {
        Self {
            data_src: None,
            symbol: None,
            timeframe: Timeframe::Min1,
            theme: Theme::dark(),
            chart_type: ChartType::Candles,
            config: ChartConfig::default(),
            chart_options: ChartOptions::default(),
            indicators: None,
            drawing_manager: None,
            visible_candles: 100,
            initial_bars: Some(1000), // Default: auto-fetch 1000 bars
            auto_fetch_on_symbol_change: true,
            auto_fetch_on_timeframe_change: true,
        }
    }

    /// Create an extended chart with all advanced features enabled.
    ///
    /// This pre-configured variant creates a full-featured trading terminal chart
    /// with drawing tools and indicators registry enabled.
    ///
    /// # Features Enabled
    ///
    /// - **Drawing Tools**: Full suite of technical analysis drawing tools
    /// - **Indicators Registry**: Support for technical indicators (RSI, MACD, etc.)
    /// - **All Interactive Features**: Complete user interaction capabilities
    pub fn extended() -> Self {
        Self::new()
            .with_drawing_tools()
            .with_indicators(IndicatorRegistry::new())
    }

    /// Create an options chart optimized for non-time-based pricing displays.
    ///
    /// This variant creates a chart specifically designed for options pricing,
    /// volatility surfaces, and other financial instruments where time is not
    /// the primary axis. Time scale features are disabled.
    ///
    /// # Features Disabled
    ///
    /// - **Time Scale**: No time axis display
    /// - **Time Labels**: Time values hidden
    /// - **Time-Based Interactions**: Scroll/zoom based on price levels only
    ///
    /// # Chart Type
    ///
    /// - Uses `Line` chart by default (suitable for smooth price curves)
    ///
    /// # Use Cases
    ///
    /// - Options pricing chains
    /// - Volatility surface visualization
    /// - Greeks display
    /// - Implied volatility curves
    /// - Non-time-series financial data
    ///
    /// # Examples
    ///
    /// ```rust
    /// use egui_charts::ChartBuilder;
    ///
    /// // Create options chain chart
    /// let chart = ChartBuilder::options_chart()
    ///     .with_symbol("SPY")
    ///     .build();
    ///
    /// // Time scale is disabled, optimized for strike price vs premium
    /// ```
    ///
    /// # Note
    ///
    /// For traditional OHLC price charts, use [`ChartBuilder::new()`] instead.
    pub fn options_chart() -> Self {
        let mut chart_options = ChartOptions::default();
        // Disable time-based features
        chart_options.time_scale.time_visible = false;
        chart_options.time_scale.seconds_visible = false;
        chart_options.time_scale.ticks_visible = false;

        Self::new()
            .with_chart_options(chart_options)
            .with_chart_type(ChartType::Line)
    }

    /// Create a minimal, lightweight price chart for simple displays.
    ///
    /// This variant provides a clean, distraction-free price chart optimized
    /// for dashboards, tickers, and embedded widgets. Removes visual clutter
    /// and focuses purely on price movement.
    ///
    /// # Features Disabled
    ///
    /// - **Grid**: No grid lines
    /// - **Crosshair**: No crosshair lines or labels
    /// - **Drawing Tools**: Not included
    /// - **Indicators**: Not included
    ///
    /// # Configuration
    ///
    /// - **Chart Type**: Line (smooth, clean appearance)
    /// - **Visible Candles**: 50 (narrower view window)
    /// - **Interactions**: Basic only (no complex overlays)
    ///
    /// # Use Cases
    ///
    /// - Price tickers and widgets
    /// - Dashboard sparklines
    /// - Embedded mini-charts
    /// - Overview displays
    /// - Mobile-optimized views
    ///
    /// # Examples
    ///
    /// ```rust
    /// use egui_charts::ChartBuilder;
    ///
    /// // Create minimal price ticker
    /// let chart = ChartBuilder::price_chart()
    ///     .with_symbol("BTCUSDT")
    ///     .with_visible_candles(30)  // Even narrower view
    ///     .build();
    ///
    /// // Clean chart with no grid, crosshair, or tools
    /// ```
    ///
    /// ## Dashboard Example
    ///
    /// ```rust
    /// use egui_charts::ChartBuilder;
    ///
    /// // Multiple mini-charts for dashboard
    /// let btc_chart = ChartBuilder::price_chart()
    ///     .with_symbol("BTCUSDT")
    ///     .build();
    ///
    /// let eth_chart = ChartBuilder::price_chart()
    ///     .with_symbol("ETHUSDT")
    ///     .build();
    /// ```
    ///
    /// # Note
    ///
    /// For full-featured charts, use [`ChartBuilder::extended()`] instead.
    pub fn price_chart() -> Self {
        use crate::config::CrosshairOptions;

        let config = ChartConfig {
            show_grid: false,
            ..ChartConfig::default()
        };

        let chart_options = ChartOptions {
            crosshair: CrosshairOptions {
                vert_line_visible: false,
                horz_line_visible: false,
                label_visible: false,
                ..CrosshairOptions::default()
            },
            ..ChartOptions::default()
        };

        Self::new()
            .with_config(config)
            .with_chart_options(chart_options)
            .with_chart_type(ChartType::Line)
            .with_visible_candles(50)
    }

    /// Attach a [`DataSource`] for live and historical data.
    ///
    /// Without a data source the chart will render but remain empty until
    /// data is fed manually via `chart.update_data(...)`.
    #[must_use]
    pub fn with_data_src(mut self, source: Box<dyn DataSource>) -> Self {
        self.data_src = Some(source);
        self
    }

    /// Set the trading symbol (e.g., `"BTCUSDT"`, `"AAPL"`).
    ///
    /// Defaults to `"BTCUSDT"` if not specified.
    #[must_use]
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Set the bar aggregation timeframe (default: `Timeframe::Min1`).
    #[must_use]
    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.timeframe = timeframe;
        self
    }

    /// Set the visual theme (default: `Theme::dark()`).
    ///
    /// The theme is applied to the [`ChartConfig`] during [`build`](Self::build),
    /// controlling colors for background, candles, grid, axes, and overlays.
    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Set the chart visualization type (default: `ChartType::Candles`).
    ///
    /// Common types: `Candles`, `Line`, `Area`, `Bar`, `Baseline`, `Histogram`.
    #[must_use]
    pub fn with_chart_type(mut self, chart_type: ChartType) -> Self {
        self.chart_type = chart_type;
        self
    }

    /// Override the entire [`ChartConfig`] (axis visibility, grid, realtime button, etc.).
    #[must_use]
    pub fn with_config(mut self, config: ChartConfig) -> Self {
        self.config = config;
        self
    }

    /// Override the [`ChartOptions`] (crosshair, time scale, scroll/zoom behavior, etc.).
    #[must_use]
    pub fn with_chart_options(mut self, options: ChartOptions) -> Self {
        self.chart_options = options;
        self
    }

    /// Attach an [`IndicatorRegistry`] for technical studies (SMA, RSI, MACD, etc.).
    #[must_use]
    pub fn with_indicators(mut self, registry: IndicatorRegistry) -> Self {
        self.indicators = Some(registry);
        self
    }

    /// Enable the drawing tools subsystem (trend lines, Fibonacci, channels, etc.).
    ///
    /// Creates a fresh [`DrawingManager`]. Without this, the chart will not
    /// support user-drawn annotations.
    #[must_use]
    pub fn with_drawing_tools(mut self) -> Self {
        self.drawing_manager = Some(DrawingManager::new());
        self
    }

    /// Set the number of bars visible in the initial viewport (default: 100).
    ///
    /// This controls the initial bar spacing so that approximately `count` bars
    /// fit within the chart width.
    #[must_use]
    pub fn with_visible_candles(mut self, count: usize) -> Self {
        self.visible_candles = count;
        self
    }

    /// Set number of bars to fetch on initial load
    ///
    /// Pass `Some(n)` to auto-fetch n bars, or `None` to disable auto-fetch.
    /// Default: `Some(1000)`
    ///
    /// # Examples
    /// ```
    /// // Fetch 500 bars on initial load
    /// chart.with_initial_bars(Some(500));
    ///
    /// // Disable auto-fetch (manual control)
    /// chart.with_initial_bars(None);
    /// ```
    #[must_use]
    pub fn with_initial_bars(mut self, bars: Option<usize>) -> Self {
        self.initial_bars = bars;
        self
    }

    /// Enable or disable auto-fetch for both symbol and timeframe changes
    ///
    /// Default: `true`
    #[must_use]
    pub fn with_auto_fetch(mut self, enabled: bool) -> Self {
        self.auto_fetch_on_symbol_change = enabled;
        self.auto_fetch_on_timeframe_change = enabled;
        self
    }

    /// Enable or disable auto-fetch on symbol change
    ///
    /// Default: `true`
    #[must_use]
    pub fn with_auto_fetch_on_symbol_change(mut self, enabled: bool) -> Self {
        self.auto_fetch_on_symbol_change = enabled;
        self
    }

    /// Enable or disable auto-fetch on timeframe change
    ///
    /// Default: `true`
    #[must_use]
    pub fn with_auto_fetch_on_timeframe_change(mut self, enabled: bool) -> Self {
        self.auto_fetch_on_timeframe_change = enabled;
        self
    }

    /// Configure the left price scale
    ///
    /// The left price scale is typically used for indicators or secondary series.
    /// By default, it is hidden. Call this method to enable and configure it.
    ///
    /// # Examples
    /// ```
    /// use egui_charts::ChartBuilder;
    /// use egui_charts::scale::PriceScaleMode;
    ///
    /// // Enable left scale with percentage mode (good for indicators like RSI)
    /// let chart = ChartBuilder::new()
    ///     .with_left_price_scale(true, PriceScaleMode::Percentage)
    ///     .build();
    /// ```
    #[must_use]
    pub fn with_left_price_scale(
        mut self,
        visible: bool,
        mode: crate::scales::PriceScaleMode,
    ) -> Self {
        self.config.show_left_axis = visible;
        self.config.left_axis_scale_mode = mode;
        self
    }

    /// Configure the right price scale
    ///
    /// The right price scale is the primary scale, shown by default.
    ///
    /// # Examples
    /// ```
    /// use egui_charts::ChartBuilder;
    /// use egui_charts::scale::PriceScaleMode;
    ///
    /// // Use logarithmic scale for long-term price charts
    /// let chart = ChartBuilder::new()
    ///     .with_right_price_scale(true, PriceScaleMode::Logarithmic)
    ///     .build();
    /// ```
    #[must_use]
    pub fn with_right_price_scale(
        mut self,
        visible: bool,
        mode: crate::scales::PriceScaleMode,
    ) -> Self {
        self.config.show_right_axis = visible;
        self.config.right_axis_scale_mode = mode;
        self
    }

    /// Set the width of the left price axis in pixels
    ///
    /// Default: 70.0
    #[must_use]
    pub fn with_left_axis_width(mut self, width: f32) -> Self {
        self.config.left_axis_width = width;
        self
    }

    /// Set the width of the right price axis in pixels
    ///
    /// Default: 70.0
    #[must_use]
    pub fn with_right_axis_width(mut self, width: f32) -> Self {
        self.config.right_axis_width = width;
        self
    }

    /// Enable or disable the "Go to Realtime" button
    ///
    /// Default: true (visible)
    #[must_use]
    pub fn with_realtime_btn(mut self, visible: bool) -> Self {
        self.config.show_realtime_btn = visible;
        self
    }

    /// Set the position of the "Go to Realtime" button
    ///
    /// Default: TopCenter
    #[must_use]
    pub fn with_realtime_button_pos(mut self, position: RealtimeButtonPos) -> Self {
        self.config.realtime_button_pos = position;
        self
    }

    /// Set custom text for the "Go to Realtime" button
    ///
    /// Default: "Go to Realtime"
    #[must_use]
    pub fn with_realtime_button_text(mut self, text: impl Into<String>) -> Self {
        self.config.realtime_button_text = Some(text.into());
        self
    }

    /// Set the size of the "Go to Realtime" button (width, height)
    ///
    /// Default: (110.0, 28.0)
    #[must_use]
    pub fn with_realtime_button_size(mut self, width: f32, height: f32) -> Self {
        self.config.realtime_button_size = (width, height);
        self
    }

    /// Consume the builder and produce a ready-to-use [`TradingChart`].
    ///
    /// This applies the theme to the config, creates the underlying [`Chart`]
    /// widget, subscribes to the data source (if present), and fetches
    /// initial historical bars when configured.
    pub fn build(mut self) -> TradingChart {
        // Apply theme to config
        self.config = self.theme.apply_to_config(self.config);

        // Create chart
        let mut chart =
            Chart::with_config(BarData::new(), self.config).with_chart_options(self.chart_options);

        chart.set_chart_type(self.chart_type);
        chart.set_visible_bars(self.visible_candles);

        let symbol = self.symbol.unwrap_or_else(|| "BTCUSDT".to_string());

        // Set symbol and timeframe for legend display
        chart.set_symbol(&symbol);
        chart.set_timeframe_label(&self.timeframe.as_str());

        // Subscribe to symbol and fetch historical data if data source provided
        if let Some(ref mut source) = self.data_src {
            // Subscribe for real-time updates
            let _ = source.subscribe(symbol.clone(), self.timeframe);

            // Fetch historical data if configured and supported
            if let Some(bars_cnt) = self.initial_bars
                && source.supports_historical()
            {
                let request = crate::data::HistoricalDataRequest {
                    symbol: symbol.clone(),
                    timeframe: self.timeframe,
                    end_ts_millis: chrono::Utc::now().timestamp_millis(),
                    limit: bars_cnt,
                };

                match source.fetch_historical(request) {
                    Ok(bars) => {
                        if !bars.is_empty() {
                            log::info!(
                                "[ChartBuilder] Loaded {} historical bars for {}",
                                bars.len(),
                                symbol
                            );
                            let data = crate::model::BarData::from_bars(bars);
                            chart.update_data(data);
                        }
                    }
                    Err(e) => {
                        log::warn!("[ChartBuilder] Failed to fetch historical data: {e}");
                    }
                }
            }
        }

        TradingChart {
            chart,
            data_src: self.data_src,
            symbol,
            timeframe: self.timeframe,
            indicators: self.indicators.unwrap_or_default(),
            drawing_manager: self.drawing_manager,
            initial_bars: self.initial_bars.unwrap_or(1000),
            auto_fetch_on_symbol_change: self.auto_fetch_on_symbol_change,
            auto_fetch_on_timeframe_change: self.auto_fetch_on_timeframe_change,
            is_fetching_historical: false,
            historical_fetch_threshold: 150, // Prefetch when within 150 bars (anticipatory loading)
            has_more_historical_data: true,  // Assume more data available initially
        }
    }
}

impl Default for ChartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A fully assembled trading chart with data source, indicators, and drawing tools.
///
/// `TradingChart` is the runtime owner produced by [`ChartBuilder::build`]. It wraps
/// the core [`Chart`] widget and adds:
///
/// - **Live data polling** via the [`DataSource`] trait
/// - **Progressive historical loading** — automatically prefetches older bars as the
///   user scrolls left, similar to TradingView's infinite scroll
/// - **Indicator calculation** — recalculates all registered studies when data changes
/// - **Drawing tool management** — optional [`DrawingManager`] for annotations
///
/// # Usage
///
/// Call [`update`](Self::update) once per frame (before rendering) to poll the data
/// source and handle progressive loading.  Then call [`show`](Self::show) to paint
/// the chart into an egui `Ui`.
///
/// ```rust,ignore
/// // In your egui app's update function:
/// trading_chart.update();
/// trading_chart.show(ui);
/// ```
pub struct TradingChart {
    /// The underlying chart widget that handles rendering and state.
    pub chart: Chart,
    /// Optional data source for live/historical data.
    pub data_src: Option<Box<dyn DataSource>>,
    /// Current trading symbol (e.g., `"BTCUSDT"`).
    pub symbol: String,
    /// Current timeframe for bar aggregation.
    pub timeframe: Timeframe,
    /// Registry of technical indicators (SMA, EMA, RSI, etc.).
    pub indicators: IndicatorRegistry,
    /// Optional drawing tool manager for annotations and technical drawings.
    pub drawing_manager: Option<DrawingManager>,
    /// Number of bars to fetch when auto-fetching
    initial_bars: usize,
    /// Auto-fetch on symbol change
    auto_fetch_on_symbol_change: bool,
    /// Auto-fetch on timeframe change
    auto_fetch_on_timeframe_change: bool,
    /// Progressive loading: prevent multiple simultaneous fetches
    is_fetching_historical: bool,
    /// Progressive loading: how close to data edge before fetching more (bars)
    historical_fetch_threshold: usize,
    /// Progressive loading: whether more historical data is available
    has_more_historical_data: bool,
}

impl TradingChart {
    /// Poll the data source for updates and handle progressive historical loading.
    ///
    /// This method should be called **once per frame**, before [`show`](Self::show).
    /// It performs three key tasks:
    ///
    /// 1. **Polls real-time updates** — processes `FullDataset` and `NewBars` events
    ///    from the data source, merging live bars into the existing dataset.
    /// 2. **Recalculates indicators** — whenever data changes, all registered
    ///    indicators are recalculated on the updated bar slice.
    /// 3. **Progressive loading** — when the user scrolls close to the oldest bar
    ///    (within `historical_fetch_threshold`), automatically fetches more
    ///    historical data and prepends it, shifting drawing indices to maintain
    ///    alignment.
    pub fn update(&mut self) {
        if let Some(ref mut source) = self.data_src {
            // Poll for real-time updates
            let updates = source.poll();
            for update in updates {
                use crate::data::DataUpdate;
                match update {
                    DataUpdate::FullDataset { symbol, bars } if symbol == self.symbol => {
                        let data = BarData::from_bars(bars);
                        self.chart.update_data(data.clone());
                        self.indicators.calculate_all(&data.bars);
                        let _ = symbol; // consumed
                    }
                    DataUpdate::NewBars { symbol, bars } if symbol == self.symbol => {
                        // LIVE DATA: Append new bars to existing data
                        if !bars.is_empty() {
                            let mut existing_bars = self.chart.data().bars.clone();
                            let bars_before = existing_bars.len();

                            // Merge new bars: update last bar if same timestamp, otherwise append
                            for new_bar in bars {
                                if let Some(last) = existing_bars.last_mut() {
                                    if last.time == new_bar.time {
                                        // Update existing bar (live bar update)
                                        *last = new_bar;
                                    } else if new_bar.time > last.time {
                                        // Append new completed bar
                                        existing_bars.push(new_bar);
                                    }
                                    // Ignore bars older than last (out of order)
                                } else {
                                    // First bar
                                    existing_bars.push(new_bar);
                                }
                            }

                            let bars_after = existing_bars.len();
                            let data = BarData::from_bars(existing_bars);
                            self.chart.update_data(data.clone());

                            // Recalculate indicators with updated data
                            self.indicators.calculate_all(&data.bars);

                            // Log new bar activity
                            if bars_after > bars_before {
                                log::debug!(
                                    "[TradingChart] Live data: {} new bars (total: {})",
                                    bars_after - bars_before,
                                    bars_after
                                );
                            }

                            let _ = symbol; // consumed
                        }
                    }
                    _ => {}
                }
            }

            // PROGRESSIVE LOADING: Check if user has scrolled close to data edge
            // If so, automatically fetch more historical data
            let start_idx = self.chart.get_start_idx();
            let curr_bar_cnt = self.chart.data().len();

            // Fetch more data if:
            // 1. User is within threshold of data edge
            // 2. We're not already fetching
            // 3. Data source supports historical fetching
            // 4. We have data (avoid fetching on empty chart)
            // 5. We haven't reached the end of historical data yet
            let should_fetch = start_idx < self.historical_fetch_threshold
                && !self.is_fetching_historical
                && source.supports_historical()
                && curr_bar_cnt > 0
                && self.has_more_historical_data;

            if should_fetch {
                self.is_fetching_historical = true;

                // Get the oldest bar's ts to fetch data before it
                let curr_data = self.chart.data();
                if let Some(oldest_bar) = curr_data.bars.first() {
                    // Use larger batch size for progressive loading to reduce round trips
                    let progressive_batch_size = self.initial_bars.max(600);
                    log::info!(
                        "[TradingChart] Progressive loading: Fetching {} more bars before {}",
                        progressive_batch_size,
                        oldest_bar.time
                    );

                    let request = crate::data::HistoricalDataRequest {
                        symbol: self.symbol.clone(),
                        timeframe: self.timeframe,
                        end_ts_millis: oldest_bar.time.timestamp_millis(), // Fetch data BEFORE oldest bar
                        limit: progressive_batch_size,
                    };

                    match source.fetch_historical(request) {
                        Ok(mut older_bars) if !older_bars.is_empty() => {
                            log::info!(
                                "[TradingChart] Progressive loading: Loaded {} older bars",
                                older_bars.len()
                            );

                            // Filter out any bars that overlap with existing data (avoid duplicate ts)
                            if let Some(first_existing) = curr_data.bars.first() {
                                older_bars.retain(|bar| bar.time < first_existing.time);
                            }

                            if older_bars.is_empty() {
                                log::info!(
                                    "[TradingChart] Progressive loading: All fetched bars were duplicates, reached end of data"
                                );
                                self.has_more_historical_data = false;
                                self.is_fetching_historical = false;
                                return; // Exit early, no need to update
                            }

                            // Prepend older bars to existing data
                            older_bars.extend(curr_data.bars.iter().cloned());
                            let new_data = BarData::from_bars(older_bars);

                            // Calculate how many bars were actually prepended
                            let bars_added = new_data.len() - curr_bar_cnt;

                            // CRITICAL: Shift drawing bar indices BEFORE updating chart data.
                            // When N bars are prepended, all existing bar indices shift by +N.
                            // Drawings must be shifted to maintain alignment with their candles.
                            if let Some(ref mut dm) = self.drawing_manager {
                                dm.shift_bar_indices(bars_added as f32);
                            }

                            // Update chart with extended dataset
                            self.chart.update_data(new_data.clone());
                            self.indicators.calculate_all(&new_data.bars);
                        }
                        Ok(_) => {
                            log::info!(
                                "[TradingChart] Progressive loading: No more historical data available (empty response)"
                            );
                            self.has_more_historical_data = false;
                        }
                        Err(e) => {
                            log::warn!("[TradingChart] Progressive loading failed: {e}");
                            // Don't set has_more_historical_data to false on error - might be temporary
                        }
                    }
                }

                self.is_fetching_historical = false;
            }
        }
    }

    /// Render the chart widget into the given egui `Ui`.
    ///
    /// This paints the full chart including candles/lines, indicators, overlays,
    /// drawing tools, crosshair, axes, and the legend. Call [`update`](Self::update)
    /// before this method each frame.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.chart
            .show_with_indicators(ui, self.drawing_manager.as_mut(), Some(&self.indicators));
    }

    /// Change the active trading symbol.
    ///
    /// This clears the progressive loading state, subscribes to the new symbol
    /// on the data source, optionally fetches historical bars (if
    /// `auto_fetch_on_symbol_change` is enabled), recalculates indicators, and
    /// updates the chart legend.
    pub fn set_symbol(&mut self, symbol: impl Into<String>) {
        let symbol = symbol.into();

        // Reset progressive loading state for new symbol
        self.has_more_historical_data = true;

        if let Some(ref mut source) = self.data_src {
            // Subscribe for real-time updates
            let _ = source.subscribe(symbol.clone(), self.timeframe);

            // Fetch historical data if configured and supported
            if self.auto_fetch_on_symbol_change && source.supports_historical() {
                let request = crate::data::HistoricalDataRequest {
                    symbol: symbol.clone(),
                    timeframe: self.timeframe,
                    end_ts_millis: chrono::Utc::now().timestamp_millis(),
                    limit: self.initial_bars,
                };

                match source.fetch_historical(request) {
                    Ok(bars) => {
                        if !bars.is_empty() {
                            log::info!(
                                "[TradingChart] Loaded {} historical bars for {}",
                                bars.len(),
                                symbol
                            );
                            let data = crate::model::BarData::from_bars(bars);
                            self.chart.update_data(data.clone());
                            self.indicators.calculate_all(&data.bars);
                        }
                    }
                    Err(e) => {
                        log::warn!("[TradingChart] Failed to fetch historical data: {e}");
                    }
                }
            }
        }

        self.symbol = symbol;
        // Update chart legend display
        self.chart.set_symbol(&self.symbol);
        self.chart.set_timeframe_label(&self.timeframe.as_str());
    }

    /// Change the bar aggregation timeframe.
    ///
    /// Old bars are cleared because they belong to the previous timeframe.
    /// A new subscription is created on the data source, historical data is
    /// fetched if `auto_fetch_on_timeframe_change` is enabled, and indicators
    /// are recalculated. No-ops if `timeframe` equals the current value.
    pub fn set_timeframe(&mut self, timeframe: Timeframe) {
        // Skip if same timeframe
        if self.timeframe == timeframe {
            return;
        }

        log::info!(
            "[TradingChart] Changing timeframe from {:?} to {:?}",
            self.timeframe,
            timeframe
        );

        // Reset progressive loading state for new timeframe
        self.has_more_historical_data = true;

        // Clear existing data - new timeframe needs fresh bars
        // This is critical: old bars at old timeframe are invalid for new timeframe
        let empty_data = BarData::default();
        self.chart.update_data(empty_data.clone());
        // Reset indicators with empty data
        self.indicators.calculate_all(&[]);

        if let Some(ref mut source) = self.data_src {
            // Subscribe for real-time updates with new timeframe
            // This creates a new TickAggregator with the new period
            let _ = source.subscribe(self.symbol.clone(), timeframe);

            // Fetch historical data if configured and supported
            if self.auto_fetch_on_timeframe_change && source.supports_historical() {
                let request = crate::data::HistoricalDataRequest {
                    symbol: self.symbol.clone(),
                    timeframe,
                    end_ts_millis: chrono::Utc::now().timestamp_millis(),
                    limit: self.initial_bars,
                };

                match source.fetch_historical(request) {
                    Ok(bars) => {
                        if !bars.is_empty() {
                            log::info!(
                                "[TradingChart] Loaded {} historical bars for {} timeframe",
                                bars.len(),
                                timeframe
                            );
                            let data = crate::model::BarData::from_bars(bars);
                            self.chart.update_data(data.clone());
                            self.indicators.calculate_all(&data.bars);
                        }
                    }
                    Err(e) => {
                        log::warn!("[TradingChart] Failed to fetch historical data: {e}");
                    }
                }
            }
        }

        self.timeframe = timeframe;
        // Update chart legend display
        self.chart.set_timeframe_label(&timeframe.as_str());
    }

    /// Change the chart visualization type (e.g., Candles, Line, Area).
    pub fn set_chart_type(&mut self, chart_type: ChartType) {
        self.chart.set_chart_type(chart_type);
    }

    /// Get the most recent bar in the dataset, useful for displaying current price in UI.
    pub fn get_last_bar(&self) -> Option<&crate::model::Bar> {
        self.chart.data().bars.last()
    }

    /// Get the current trading symbol string.
    pub fn get_symbol(&self) -> &str {
        &self.symbol
    }

    /// Toggle main series visibility
    ///
    /// This is used by the series context menu to hide/show the main price series.
    /// Currently logs the action; full implementation requires visibility infrastructure.
    pub fn toggle_main_series_visibility(&mut self) {
        // TODO(P2): Add a `visible: bool` field to the main series state and toggle it here.
        // Renderers (candlestick, line, area) must check this flag before painting.
        // Also update the series context menu eye-icon to reflect current visibility.
        log::info!("Toggle main series visibility requested (infrastructure pending)");
    }
}
