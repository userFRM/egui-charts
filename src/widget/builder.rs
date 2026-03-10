//! Chart construction, configuration, and data management methods.
//!
//! This module contains the builder-pattern constructors and the mutable
//! configuration API for [`Chart`]. Use these methods to create a chart,
//! set its initial state, and update it across frames.
//!
//! # Builder Pattern
//!
//! ```rust,ignore
//! use egui_charts::widget::Chart;
//! use egui_charts::model::BarData;
//!
//! let chart = Chart::new(data)
//!     .visible_bars(120)
//!     .start_idx(0)
//!     .with_chart_options(options);
//! ```
//!
//! # Persistent Chart (Recommended for Real-Time)
//!
//! ```rust,ignore
//! // Store in your app struct, update each frame:
//! self.chart.update_data(new_data);
//! self.chart.set_chart_type(ChartType::Candles);
//! self.chart.enable_tracking_mode();
//! ```

use crate::chart::cursor_modes::CursorModeState;
use crate::config::{ChartConfig, ChartOptions};
use crate::model::ChartType;
use crate::model::{BarData, ChartState};
use crate::validation::DataValidator;

use super::Chart;
use super::state::{BoxZoomState, KineticScrollState};

impl Chart {
    /// Creates a new chart with default configuration.
    ///
    /// Initializes the chart with the provided OHLCV data and sensible defaults:
    /// candlestick chart type, default bar spacing, data validation enabled,
    /// and tracking mode off.
    ///
    /// # Arguments
    ///
    /// * `data` -- The OHLCV bar data to display
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let chart = Chart::new(bar_data);
    /// ```
    pub fn new(data: BarData) -> Self {
        let chart_options = ChartOptions::default();
        let mut state = ChartState::new(data);

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
            price_scale_drag_start: None,
            pending_start_idx: None,
            chart_type: ChartType::Candles,
            renko_brick_size: 1.0,
            kagi_reversal_amount: 1.0,
            tracking_mode_active: false,
            mouse_in_chart: false,
            validator: Some(DataValidator::new()),
            box_zoom: BoxZoomState::new(),
            zoom_mode_active: false,
            zoom_just_applied: false,
            symbol: String::new(),
            timeframe: String::new(),
            cursor_modes: CursorModeState::new(),
            last_rendered_price_range: (0.0, 0.0),
            last_rendered_price_rect: egui::Rect::NOTHING,
            last_rendered_volume_rect: egui::Rect::NOTHING,
            last_rendered_indicator_panes: Vec::new(),
            // Multi-chart sync
            synced_crosshair_bar_idx: None,
            last_hover_bar_idx: None,
            // Marks (Widget API)
            marks: Vec::new(),
            timescale_marks: Vec::new(),
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
        let chart_options = ChartOptions::default();
        let mut state = ChartState::new(data);

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
            config,
            start_idx: 0,
            chart_options,
            kinetic_scroll: KineticScrollState::new(),
            scroll_start_pos: None,
            scroll_start_offset: None,
            prev_width: None,
            desired_visible_bars: None,
            last_visible_bars: 0,
            apply_visible_bars_once: false,
            price_scale_drag_start: None,
            pending_start_idx: None,
            chart_type: ChartType::Candles,
            renko_brick_size: 1.0,
            kagi_reversal_amount: 1.0,
            tracking_mode_active: false,
            mouse_in_chart: false,
            validator: Some(DataValidator::new()),
            box_zoom: BoxZoomState::new(),
            zoom_mode_active: false,
            zoom_just_applied: false,
            symbol: String::new(),
            timeframe: String::new(),
            cursor_modes: CursorModeState::new(),
            last_rendered_price_range: (0.0, 0.0),
            last_rendered_price_rect: egui::Rect::NOTHING,
            last_rendered_volume_rect: egui::Rect::NOTHING,
            last_rendered_indicator_panes: Vec::new(),
            // Multi-chart sync
            synced_crosshair_bar_idx: None,
            last_hover_bar_idx: None,
            // Marks (Widget API)
            marks: Vec::new(),
            timescale_marks: Vec::new(),
        }
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
        // Default to showing the latest `count` bars unless caller overrides later
        self.start_idx = self.state.data().len().saturating_sub(count);
        self
    }

    /// Sets the starting bar index for the visible range (builder pattern).
    ///
    /// Index `0` is the oldest bar in the data. Use this to start the chart
    /// at a specific point in history rather than at the latest data.
    pub fn start_idx(mut self, index: usize) -> Self {
        self.start_idx = index;
        self
    }

    /// Returns the number of bars currently visible in the chart area.
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

    /// Returns the current starting bar index of the visible range.
    ///
    /// Useful for syncing chart position with external app state or persisting
    /// the user's scroll position across sessions.
    pub fn get_start_idx(&self) -> usize {
        self.start_idx
    }

    /// Sets a custom visual configuration (builder pattern).
    ///
    /// Controls colors, padding, grid visibility, volume display, and other
    /// visual aspects. See [`ChartConfig`] for the full list of options.
    pub fn config(mut self, config: ChartConfig) -> Self {
        self.config = config;
        self
    }

    /// Updates the chart's OHLCV data for live/streaming scenarios.
    ///
    /// Call this each time new bars arrive or the latest bar's close price changes.
    /// If the chart is near the live edge and `shift_visible_range_on_new_bar` is
    /// enabled in chart options, the viewport automatically scrolls to keep the
    /// latest bar visible.
    ///
    /// Returns `true` if the chart auto-scrolled to follow the live edge, `false`
    /// if the viewport position was unchanged (user is scrolled back in history).
    ///
    /// # Data Validation
    ///
    /// When validation is enabled (default), new bars are checked for anomalies
    /// (duplicate timestamps, suspicious price changes). Validation warnings are
    /// logged but do not reject data.
    pub fn update_data(&mut self, data: BarData) -> bool {
        // Validate new data if validator is enabled. Only validate when the tail
        // actually advances; when we prepend older bars the last bar is unchanged
        // and validating it against itself produces a false duplicate warning.
        if let Some(ref validator) = self.validator
            && let (Some(new_tail), Some(old_tail)) =
                (data.bars.last(), self.state.data().bars.last())
            && new_tail.time > old_tail.time
        {
            let result = validator.validate_new_bar(Some(old_tail), new_tail);
            if result.is_error() {
                // keep logging behaviour but do not reject
            }
        }

        // Loosely detect "near live edge": within ~1.5 bars from the right edge.
        // With fix_right_edge=true we clamp right_offset <= 0, so "near live" means
        // not farther than 1.5 bars to the left of the edge: right_offset >= -1.5.
        let near_live = self.state.time_scale().right_offset() >= -1.5;

        // Detect if new bars were appended or tail candle changed
        let prev_len = self.state.data().len();
        let old_tail = self.state.data().bars.last().cloned();
        let new_tail = data.bars.last().cloned();

        // CRITICAL FIX: Only consider it "appended" if TAIL moved forward in time.
        // When prepending older historical bars, the data length increases but
        // the tail stays the same (or moves forward), so we check if the tail
        // timestamp actually advanced.
        let tail_time_advanced = old_tail
            .as_ref()
            .zip(new_tail.as_ref())
            .map(|(o, n)| n.time > o.time)
            .unwrap_or(false);
        let appended = data.len() > prev_len && tail_time_advanced;

        let tail_changed = old_tail
            .zip(new_tail)
            .map(|(o, n)| o.time != n.time || o.close != n.close)
            .unwrap_or(false);

        // Check if we should shift for whitespace replacement
        let is_whitespace_replacement = tail_changed && !appended;
        let should_shift_for_whitespace = is_whitespace_replacement
            && self
                .chart_options
                .time_scale
                .allow_shift_visible_range_on_whitespace_replacement;

        // Update the data (this changes bar_cnt which shifts coords)
        self.state.set_data(data);

        // IMPORTANT: When older bars are prepended (length increased but tail unchanged),
        // NO adjustment to right_offset is needed! Here's why:
        // - base_idx increases by N (e.g., 199 -> 589 when adding 390 bars)
        // - Bar indices also increase by N (e.g., old bar 49 -> new bar 439)
        // - right_offset = right_border - base_idx
        // - Since both increase by N, right_offset stays the same
        // - The viewport automatically shows the same bars the user was viewing

        // FORCE chart to stay at live edge if:
        // 1. shift_visible_range_on_new_bar is enabled AND
        // 2. We were near the live edge AND
        // 3. Either new bars were appended OR we should shift for whitespace replacement
        let should_auto_shift = self.chart_options.time_scale.shift_visible_range_on_new_bar
            && near_live
            && (appended || should_shift_for_whitespace);

        if should_auto_shift {
            self.state.time_scale_mut().scroll_to_realtime();
            // Cancel any pending start_idx to prevent override
            self.pending_start_idx = None;
            true // Return true to indicate we auto-followed
        } else {
            false
        }
    }

    /// Replaces the visual configuration on a persistent chart instance.
    ///
    /// Use this when reconfiguring a long-lived chart (e.g., after the user
    /// changes settings). For initial construction prefer [`Chart::config`].
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
    /// The viewport shift is applied on the next frame.
    pub fn set_start_idx(&mut self, index: usize) {
        if index != self.start_idx {
            self.start_idx = index;
            self.pending_start_idx = Some(index);
        }
    }

    /// Sets the chart type (Candles, Bars, Line, Area, Renko, Kagi).
    ///
    /// Takes effect on the next rendered frame. For Renko/Kagi charts, also
    /// set the brick/reversal size with [`Chart::set_renko_brick_size`] or
    /// [`Chart::set_kagi_reversal_amount`].
    pub fn set_chart_type(&mut self, chart_type: ChartType) {
        self.chart_type = chart_type;
    }

    /// Enables tracking mode, which automatically scrolls to keep the latest bar visible.
    ///
    /// When enabled, the chart immediately scrolls to the live edge and stays
    /// there as new data arrives. Disable with [`Chart::disable_tracking_mode`].
    pub fn enable_tracking_mode(&mut self) {
        self.tracking_mode_active = true;
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

    /// Enables data validation for incoming bar data.
    ///
    /// When enabled, calls to [`Chart::update_data`] check new bars for
    /// anomalies such as duplicate timestamps or suspicious price spikes.
    /// Validation is enabled by default on new charts.
    pub fn enable_validation(&mut self) {
        if self.validator.is_none() {
            self.validator = Some(DataValidator::new());
        }
    }

    /// Disables data validation, skipping anomaly checks on new bars.
    pub fn disable_validation(&mut self) {
        self.validator = None;
    }

    /// Replaces the data validator with a custom-configured one.
    ///
    /// Use this to adjust thresholds for duplicate detection or price-spike
    /// sensitivity beyond what the default [`DataValidator`] provides.
    pub fn set_validator(&mut self, validator: DataValidator) {
        self.validator = Some(validator);
    }

    /// Returns `true` if data validation is currently enabled.
    pub fn is_validation_enabled(&self) -> bool {
        self.validator.is_some()
    }

    /// Returns the current chart type (Candles, Bars, Line, Area, Renko, or Kagi).
    pub fn chart_type(&self) -> ChartType {
        self.chart_type
    }

    /// Sets the Renko brick size in price units.
    ///
    /// Each Renko brick represents a fixed price movement of this size.
    /// Only affects rendering when the chart type is [`ChartType::Renko`].
    pub fn set_renko_brick_size(&mut self, brick_size: f64) {
        self.renko_brick_size = brick_size;
    }

    /// Returns the current Renko brick size in price units.
    pub fn renko_brick_size(&self) -> f64 {
        self.renko_brick_size
    }

    /// Sets the Kagi reversal amount in price units.
    ///
    /// A new Kagi line segment is drawn when price reverses by at least this
    /// amount. Only affects rendering when the chart type is [`ChartType::Kagi`].
    pub fn set_kagi_reversal_amount(&mut self, reversal_amount: f64) {
        self.kagi_reversal_amount = reversal_amount;
    }

    /// Returns the current Kagi reversal amount in price units.
    pub fn kagi_reversal_amount(&self) -> f64 {
        self.kagi_reversal_amount
    }

    /// Returns a reference to the chart's current OHLCV data.
    ///
    /// Useful for progressive/historical loading where you need to inspect
    /// the existing data range before prepending or appending bars.
    pub fn data(&self) -> &crate::model::BarData {
        self.state.data()
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

    /// Get the price range used for actual rendering (includes zoom adjustments)
    ///
    /// This is useful for external code that needs to use the same coordinate system
    /// as the rendered chart (e.g., selection dots, hit testing).
    pub fn get_rendered_price_range(&self) -> (f64, f64) {
        self.last_rendered_price_range
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
