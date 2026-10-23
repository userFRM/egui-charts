//! Chart construction, configuration, and data management methods.
//!
//! This module contains [`ChartBuilder`], the builder-pattern constructors and
//! the mutable configuration API for [`Chart`]. Use these methods to create a
//! chart, set its initial state, and update it across frames.
//!
//! # Builder Pattern
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..100).map(|i| { let p = 100.0 + (i as f64 * 0.2).sin() * 5.0; egui_charts::model::Bar::new(chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! # let bar_data = egui_charts::model::BarData::from_bars(bars.clone());
//! use egui_charts::config::ChartOptions;
//! use egui_charts::widget::Chart;
//!
//! # let options = ChartOptions::default();
//! let chart = Chart::new(bar_data)
//!     .visible_bars(120)
//!     .start_idx(0)
//!     .with_chart_options(options);
//! ```
//!
//! # Persistent Chart (Recommended for Real-Time)
//!
//! ```rust
//! # use egui_charts::{ChartType, model::Bar, widget::{Chart, SeriesId}};
//! # struct App { chart: Chart }
//! # impl App {
//! # fn on_frame(&mut self, bar: Bar) {
//! // Store in your app struct, update each frame:
//! self.chart.update(SeriesId::MAIN, bar);
//! self.chart.set_chart_type(ChartType::Candles);
//! self.chart.enable_tracking_mode();
//! # }
//! # }
//! ```

use crate::chart::cursor_modes::CursorModeState;
use crate::chart::selection::SelectionState;
use crate::config::{ChartConfig, ChartOptions, RealtimeButtonPos};
use crate::data::{Datafeed, FeedState};
use crate::drawings::DrawingManager;
use crate::model::{BarClock, BarData, ChartState, Timeframe};
use crate::model::{ChartType, ChartTypeParams};
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::IndicatorRegistry;
use crate::theme::Theme;

use super::Chart;
use super::state::{BoxZoomState, KineticScrollState};
use std::collections::BTreeSet;
use std::sync::Arc;

impl Chart {
    /// Creates a new chart with default configuration.
    ///
    /// Initializes the chart with the provided OHLCV data and sensible defaults:
    /// candlestick chart type, default bar spacing and tracking mode off. The
    /// bars are cleaned as by [`Chart::set_data`].
    ///
    /// # Arguments
    ///
    /// * `data` -- The OHLCV bar data to display
    pub fn new(data: BarData) -> Self {
        let chart_options = ChartOptions::default();
        let (bars, rejected_bars) = super::series::clean(data.bars, &BarClock::exact());
        let mut state = ChartState::new(BarData::from_bars(bars));

        // Initialize TimeScale with defaults (apply all constraints)
        {
            let ts_opt = &chart_options.time_scale;
            state.time_scale_mut().apply_options(
                ts_opt.bar_spacing,
                ts_opt.min_bar_spacing,
                ts_opt.max_bar_spacing,
                ts_opt.fix_left_edge,
                ts_opt.fix_right_edge,
                ts_opt.right_offset,
                ts_opt.right_offset_pixels,
            );
        }

        Self {
            state,
            config: ChartConfig::default(),
            start_idx: 0,
            chart_options,
            kinetic_scroll: KineticScrollState::new(),
            scroll_start_pos: None,
            scroll_start_offset: None,
            prev_width: None,
            desired_visible_bars: None,
            last_visible_bars: 0,
            apply_visible_bars_once: false,
            price_axis_drag: None,
            pending_start_idx: None,
            pending_scroll_time: None,
            chart_type: ChartType::Candles,
            chart_type_params: ChartTypeParams::default(),
            older_bars_drawn: true,
            tracking_mode_active: false,
            mouse_in_chart: false,
            feed: FeedState {
                rejected_bars,
                ..FeedState::default()
            },
            from_ticks: BTreeSet::new(),
            studies: IndicatorRegistry::new(),
            main_series_visible: true,
            study_ids: Vec::new(),
            study_inputs: Vec::new(),
            study_autoscale: Vec::new(),
            pending_from: Vec::new(),
            next_refresh: Vec::new(),
            // 0 and 1 are the main and volume series.
            next_series: 2,
            drawings: DrawingManager::new(),
            box_zoom: BoxZoomState::new(),
            zoom_mode_active: false,
            zoom_just_applied: false,
            symbol: String::new(),
            legend_instrument: None,
            legend_badge: None,
            legend_statuses: Vec::new(),
            quote: None,
            quote_action: None,
            indicator_settings_request: None,
            session_quote: None,
            session_provider: None,
            extended_hours: false,
            axis_hover: None,
            scale_settings_requested: false,
            events: Vec::new(),
            event_click: None,
            timeframe: None,
            feed_link: None,
            cursor_modes: CursorModeState::new(),
            last_price_map: PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(0.0, 100.0),
                0.0,
                0.0,
                false,
            ),
            last_rendered_price_rect: egui::Rect::NOTHING,
            last_rendered_volume_rect: egui::Rect::NOTHING,
            last_rendered_indicator_panes: Vec::new(),
            // Multi-chart sync
            synced_crosshair_bar_idx: None,
            last_hover_bar_idx: None,
            // Marks (Widget API)
            marks: Vec::new(),
            price_lines: Vec::new(),
            // Selection (Widget API)
            selection: SelectionState::new(),
            right_click: None,
            menu_open: false,
        }
    }

    /// Creates a new chart with a custom visual configuration.
    ///
    /// Use this when you need to control colors, padding, grid visibility,
    /// volume display, and other visual aspects from the start.
    ///
    /// # Arguments
    ///
    /// * `data` -- The OHLCV bar data to display
    /// * `config` -- Custom visual configuration (colors, padding, feature flags)
    pub fn with_config(data: BarData, config: ChartConfig) -> Self {
        let mut chart = Self::new(data);
        chart.config = config;
        chart
    }

    /// Sets chart behavior options (builder pattern).
    ///
    /// Controls bar spacing, scroll/zoom constraints, right offset, and
    /// time scale behavior. Applies the time scale options immediately.
    pub fn with_chart_options(mut self, options: ChartOptions) -> Self {
        {
            let ts_opt = &options.time_scale;
            self.state.time_scale_mut().apply_options(
                ts_opt.bar_spacing,
                ts_opt.min_bar_spacing,
                ts_opt.max_bar_spacing,
                ts_opt.fix_left_edge,
                ts_opt.fix_right_edge,
                ts_opt.right_offset,
                ts_opt.right_offset_pixels,
            );
        }
        self.chart_options = options;
        self
    }

    /// Sets the desired number of visible bars (builder pattern).
    ///
    /// The actual bar spacing is computed during rendering to fit `count` bars
    /// in the available width. Also sets the start index to show the latest
    /// `count` bars by default.
    pub fn visible_bars(mut self, count: usize) -> Self {
        // Store the desired number so that we can compute the proper
        // bar spacing during `show_internal` when we know the widget width.
        self.desired_visible_bars = Some(count);
        self.apply_visible_bars_once = true;
        // Default to showing the latest `count` bars unless caller overrides later
        self.start_idx = self.state.data().len().saturating_sub(count);
        self
    }

    /// Sets the starting bar index for the visible range (builder pattern).
    ///
    /// On price-based types indices count elements; index `0` is the oldest kept element.
    /// Otherwise index `0` is the oldest bar in the data. Use this to start the chart
    /// at a specific point in history rather than at the latest data.
    pub fn start_idx(mut self, index: usize) -> Self {
        self.start_idx = index;
        self
    }

    /// Returns the number of visible slots, counting elements on price-based types.
    ///
    /// This value is computed from the bar spacing and widget width during
    /// rendering. Returns 100 as a fallback if the chart has not been rendered yet.
    pub fn get_visible_bars(&self) -> usize {
        // Return the last computed value (updated during `show_internal`).
        // Falls back to width/bar_spacing semantics if `show_internal` has
        // not been called yet.
        if self.last_visible_bars == 0 {
            100
        } else {
            self.last_visible_bars
        }
    }

    /// Returns the current starting index of the visible range, counting elements on price-based types.
    ///
    /// Useful for syncing chart position with external app state or persisting
    /// the user's scroll position across sessions.
    pub fn get_start_idx(&self) -> usize {
        self.start_idx
    }

    /// Replaces the visual configuration on a persistent chart instance.
    ///
    /// Use this when reconfiguring a long-lived chart (e.g., after the user
    /// changes settings). For initial construction prefer [`Chart::with_config`].
    pub fn update_config(&mut self, config: ChartConfig) {
        self.config = config;
    }

    /// Updates the desired number of visible bars on a persistent chart instance.
    ///
    /// Only triggers a recalculation if `count` differs from the current value.
    /// The new bar spacing is applied on the next frame.
    pub fn set_visible_bars(&mut self, count: usize) {
        // Only reapply if requested value differs from last computed
        if self.last_visible_bars == 0 || count != self.last_visible_bars {
            self.desired_visible_bars = Some(count);
            self.apply_visible_bars_once = true;
        }
    }

    /// Updates the starting bar index on a persistent chart instance.
    ///
    /// Only triggers an update if `index` differs from the current value.
    /// The viewport shift is applied on the next frame. On price-based types `index` counts
    /// elements, with zero the oldest kept element.
    pub fn set_start_idx(&mut self, index: usize) {
        if index != self.start_idx {
            self.start_idx = index;
            self.pending_start_idx = Some(index);
            self.pending_scroll_time = None;
        }
    }

    /// Scrolls the bar nearest `time` to the centre of the chart, keeping the zoom.
    ///
    /// Applied on the next frame. A time outside the data shows the nearest end; with a feed,
    /// older history is requested until the time is reached or history ends. Panning, zooming,
    /// [`Chart::set_start_idx`] or tracking cancel it.
    pub fn scroll_to_time(&mut self, time: chrono::DateTime<chrono::Utc>) {
        self.pending_scroll_time = Some(time);
        self.pending_start_idx = None;
        self.disable_tracking_mode();
        self.kinetic_scroll = KineticScrollState::new();
    }

    /// Sets the chart type (Candles, Bars, Line, Area, Renko, Kagi).
    ///
    /// Takes effect on the next rendered frame. Price-based chart types take their
    /// sizes from [`Chart::set_chart_type_params`]. A change clears the zoom history and the
    /// series and drawing selection, keeping bar spacing and the time at the right edge.
    /// A view near the live edge stays there. On Renko, Kagi, Line Break, Point and Figure and
    /// range bar charts, indices count elements instead of bars.
    pub fn set_chart_type(&mut self, chart_type: ChartType) {
        if self.chart_type != chart_type {
            self.state.clear_zoom_history();
            self.selection.deselect();
            self.drawings.deselect();
            self.chart_type = chart_type;
        }
    }

    /// Enables tracking mode, which automatically scrolls to keep the latest bar visible.
    ///
    /// When enabled, the chart immediately scrolls to the live edge and stays
    /// there as new data arrives. Disable with [`Chart::disable_tracking_mode`].
    pub fn enable_tracking_mode(&mut self) {
        self.tracking_mode_active = true;
        self.pending_scroll_time = None;
        // Immediately scroll to latest when enabling
        self.state.time_scale_mut().scroll_to_realtime();
    }

    /// Disables tracking mode, allowing the user to scroll freely through history.
    pub fn disable_tracking_mode(&mut self) {
        self.tracking_mode_active = false;
    }

    /// Toggles tracking mode on or off.
    pub fn toggle_tracking_mode(&mut self) {
        if self.tracking_mode_active {
            self.disable_tracking_mode();
        } else {
            self.enable_tracking_mode();
        }
    }

    /// Returns whether tracking mode is currently active
    pub fn is_tracking_mode_active(&self) -> bool {
        self.tracking_mode_active
    }

    /// Returns the current chart type (Candles, Bars, Line, Area, Renko, or Kagi).
    pub fn chart_type(&self) -> ChartType {
        self.chart_type
    }

    /// Sets the parameters of the price-based chart types (Renko, Kagi, Range, Point and Figure,
    /// Line Break) and of the baseline.
    ///
    /// Sizes are in price units and are used as given;
    /// `RenkoConfig::from_atr(chart.bars(SeriesId::MAIN), 14, 1.0).brick_size` gives an ATR-based
    /// brick. A size the chart cannot use is reported on the chart. On price-based types a change
    /// clears the zoom history and selection and keeps the right-edge item by time and rank.
    pub fn set_chart_type_params(&mut self, params: ChartTypeParams) {
        if self.chart_type.is_time_independent() && self.chart_type_params != params {
            self.state.clear_zoom_history();
            self.selection.deselect();
            self.drawings.deselect();
        }
        self.chart_type_params = params;
    }

    /// Returns the parameters of the price-based chart types and of the baseline.
    pub fn chart_type_params(&self) -> &ChartTypeParams {
        &self.chart_type_params
    }

    /// Calculates how many bars fit in the given pixel width at the current bar spacing.
    pub fn calculate_visible_bars(&self, width: f32) -> usize {
        (width / self.state.time_scale().bar_spacing()).floor() as usize
    }

    /// Calculates the bar spacing (pixels per bar) needed to fit the desired
    /// number of bars in the given pixel width.
    pub fn calculate_bar_spacing(&self, width: f32, visible_bars: usize) -> f32 {
        if visible_bars == 0 {
            self.chart_options.time_scale.bar_spacing
        } else {
            width / visible_bars as f32
        }
    }

    /// Get the price rect used for actual rendering
    ///
    /// This is the actual screen rect where candles are drawn, useful for
    /// external code that needs to draw overlays (e.g., selection dots).
    pub fn get_rendered_price_rect(&self) -> egui::Rect {
        self.last_rendered_price_rect
    }

    /// Get the volume rect used for actual rendering
    ///
    /// This is the actual screen rect where volume bars are drawn, useful for
    /// external code that needs to draw overlays (e.g., selection dots).
    pub fn get_rendered_volume_rect(&self) -> egui::Rect {
        self.last_rendered_volume_rect
    }

    /// Get the rendered indicator pane info for hit testing
    ///
    /// This returns information about each rendered indicator pane, useful for
    /// external code that needs to do hit testing on indicator lines.
    pub fn get_rendered_indicator_panes(&self) -> &[super::RenderedIndicatorPane] {
        &self.last_rendered_indicator_panes
    }
}

/// Fluent builder for a [`Chart`].
///
/// Use one of the constructor methods ([`new`](Self::new),
/// [`price_chart`](Self::price_chart), [`options_chart`](Self::options_chart)) and chain
/// configuration calls before calling [`build`](Self::build) to produce the chart.
///
/// All setter methods return `Self` and are marked `#[must_use]` to prevent accidental
/// drops of the builder mid-chain.
///
/// # Examples
///
/// ```rust
/// use egui_charts::{ChartBuilder, ChartType};
/// use egui_charts::model::Timeframe;
/// use egui_charts::theme::Theme;
///
/// let chart = ChartBuilder::new()
///     .with_symbol("ETHUSDT")
///     .with_timeframe(Timeframe::Hour1)
///     .with_theme(Theme::dark())
///     .with_chart_type(ChartType::Candles)
///     .with_visible_candles(120)
///     .build();
/// ```
pub struct ChartBuilder {
    feed: Option<Arc<dyn Datafeed>>,
    symbol: Option<String>,
    timeframe: Timeframe,
    theme: Theme,
    chart_type: ChartType,
    config: ChartConfig,
    chart_options: ChartOptions,
    indicators: Option<IndicatorRegistry>,
    visible_candles: usize,
    /// Bars asked for in the first page of history.
    initial_bars: usize,
}

impl ChartBuilder {
    /// Create a new chart builder with sensible defaults
    ///
    /// With a feed and a symbol, the chart asks for the latest 1000 bars at its
    /// first `show`, and again when the symbol or timeframe changes.
    pub fn new() -> Self {
        Self {
            feed: None,
            symbol: None,
            timeframe: Timeframe::Min1,
            theme: Theme::dark(),
            chart_type: ChartType::Candles,
            config: ChartConfig::default(),
            chart_options: ChartOptions::default(),
            indicators: None,
            visible_candles: 100,
            initial_bars: 1000,
        }
    }

    /// Creates a line chart with the time axis labels and ticks hidden.
    ///
    /// The data is still a time series of bars, and pan and zoom work as on any
    /// chart.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use egui_charts::ChartBuilder;
    ///
    /// // Line chart without time axis labels
    /// let chart = ChartBuilder::options_chart()
    ///     .with_symbol("SPY")
    ///     .build();
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
            .with_config(ChartConfig {
                show_time_labels: false,
                ..Default::default()
            })
            .with_chart_options(chart_options)
            .with_chart_type(ChartType::Line)
    }

    /// Creates a line chart without grid or crosshair, showing 50 bars.
    ///
    /// Drawing tools and studies are not added.
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
    /// For drawing tools and studies, use [`ChartBuilder::new()`] instead.
    pub fn price_chart() -> Self {
        use crate::config::CrosshairOptions;

        let config = ChartConfig {
            show_horizontal_grid: false,
            show_vertical_grid: false,
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

    /// Source of instrument, history and live updates. Without one, feed the chart with
    /// [`Chart::set_data`] and [`Chart::update`].
    ///
    /// [`build`](Self::build) calls no feed method: with a symbol, the chart's first
    /// [`show`](Chart::show) asks the feed for the instrument and the latest page of history,
    /// and subscribes once the instrument resolves.
    #[must_use]
    pub fn with_feed(mut self, feed: Arc<dyn Datafeed>) -> Self {
        self.feed = Some(feed);
        self
    }

    /// Set the trading symbol (e.g., `"BTCUSDT"`, `"AAPL"`).
    ///
    /// Without a symbol the chart shows none and requests nothing from the
    /// feed.
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

    /// Toggle session-break dividers (day/week/month boundaries) and shading.
    #[must_use]
    pub fn with_session_breaks(mut self, show: bool) -> Self {
        self.config.show_session_breaks = show;
        self
    }

    /// Add the studies of `registry` (SMA, RSI, MACD, etc.) to the chart, which computes,
    /// updates and draws them.
    #[must_use]
    pub fn with_indicators(mut self, registry: IndicatorRegistry) -> Self {
        self.indicators = Some(registry);
        self
    }

    /// Set the number of bars visible in the initial viewport (default: 100).
    ///
    /// This controls the initial bar spacing so that approximately `count` bars
    /// fit within the chart width. Use zero to keep the configured bar spacing.
    #[must_use]
    pub fn with_visible_candles(mut self, count: usize) -> Self {
        self.visible_candles = count;
        self
    }

    /// Bars asked for in the first page of history (default 1,000); scroll-back pages ask for
    /// at least 600.
    ///
    /// # Examples
    /// ```
    /// use egui_charts::ChartBuilder;
    ///
    /// let chart = ChartBuilder::new().with_initial_bars(500).build();
    /// ```
    #[must_use]
    pub fn with_initial_bars(mut self, bars: usize) -> Self {
        self.initial_bars = bars;
        self
    }

    /// Configure the right price scale
    ///
    /// The right price scale is the primary scale, shown by default. The chart
    /// starts with this visibility and scale mode.
    ///
    /// # Examples
    /// ```
    /// use egui_charts::ChartBuilder;
    /// use egui_charts::scales::PriceScaleMode;
    ///
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

    /// Consume the builder and produce the chart.
    ///
    /// This applies the theme to the config and sets up the chart. It never
    /// blocks and calls no feed method: with a feed and a symbol, the chart's
    /// first [`show`](Chart::show) asks the feed for the instrument and history.
    pub fn build(mut self) -> Chart {
        // Apply theme to config
        self.config = self.theme.apply_to_config(self.config);

        let right_scale_mode = self.config.right_axis_scale_mode;
        let mut chart =
            Chart::with_config(BarData::new(), self.config).with_chart_options(self.chart_options);
        chart.state.price_scale_mut().options_mut().mode = right_scale_mode;

        chart.set_chart_type(self.chart_type);
        chart.set_visible_bars(self.visible_candles);
        chart.set_symbol(self.symbol.unwrap_or_default());
        chart.set_timeframe(self.timeframe);
        if let Some(feed) = self.feed {
            chart.attach_feed(feed, self.initial_bars);
        }

        let mut studies = self.indicators.unwrap_or_default();
        while let Some(study) = studies.remove_indicator(0) {
            chart.add_indicator(study);
        }
        chart
    }
}

impl Default for ChartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Bar;
    use chrono::{Duration, TimeZone, Utc};

    /// Both entry points fit 50 slots once and leave the same view at the latest bar.
    #[test]
    fn visible_bars_matches_the_setter_on_initial_frames() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let data = BarData::from_bars(
            (0..600)
                .map(|i| Bar::new(start + Duration::days(i), 100.0, 200.0, 100.0, 200.0, 1.0))
                .collect(),
        );
        for width in [600.0, 900.0, 1440.0] {
            let mut built = Chart::new(data.clone()).visible_bars(50);
            let mut set = Chart::new(data.clone());
            set.set_visible_bars(50);
            let contexts = [egui::Context::default(), egui::Context::default()];
            built.set_timeframe(Timeframe::Day1);
            set.set_timeframe(Timeframe::Day1);
            for frame in 0..3 {
                for (chart, ctx) in [(&mut built, &contexts[0]), (&mut set, &contexts[1])] {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        time: Some(frame as f64 * 0.1),
                        ..Default::default()
                    };
                    ctx.run_ui(input, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            chart.show(ui);
                        });
                    })
                    .drop_without_applying_deltas();
                }
                assert_eq!(
                    built.state.time_scale().bar_spacing(),
                    set.state.time_scale().bar_spacing(),
                    "width {width}, frame {frame}"
                );
                assert_eq!(
                    built.state.time_scale().bar_spacing(),
                    built.get_rendered_price_rect().width() / 50.0
                );
                assert_eq!(built.get_visible_bars(), set.get_visible_bars());
                assert_eq!(built.get_visible_bars(), 49);
                assert_eq!(built.get_start_idx(), set.get_start_idx());
                assert!(built.get_start_idx() >= 548);
            }
            // A later zoom survives subsequent frames: the builder's count is applied once.
            built.state.time_scale_mut().set_bar_spacing(20.0);
            built.apply_timescale_config(width - 166.0);
            assert_eq!(built.state.time_scale().bar_spacing(), 20.0);
        }
    }
}
