//! Chart construction, configuration, and data management methods.
//!
//! This module contains the builder-pattern constructors and the mutable
//! configuration API for [`Chart`]. Use these methods to create a chart,
//! set its initial state, and update it across frames.
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
use crate::config::{ChartConfig, ChartOptions};
use crate::data::FeedState;
use crate::drawings::DrawingManager;
use crate::model::ChartType;
use crate::model::{BarData, ChartState};
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::IndicatorRegistry;

use super::Chart;
use super::state::{BoxZoomState, KineticScrollState};
use std::collections::BTreeSet;

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
        let (bars, rejected_bars) = super::series::clean(data.bars);
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
            chart_type: ChartType::Candles,
            renko_brick_size: 1.0,
            kagi_reversal_amount: 1.0,
            tracking_mode_active: false,
            mouse_in_chart: false,
            feed: FeedState {
                rejected_bars,
                ..FeedState::default()
            },
            from_ticks: BTreeSet::new(),
            studies: IndicatorRegistry::new(),
            study_ids: Vec::new(),
            pending_from: Vec::new(),
            // 0 and 1 are the main and volume series.
            next_series: 2,
            drawings: DrawingManager::new(),
            box_zoom: BoxZoomState::new(),
            zoom_mode_active: false,
            zoom_just_applied: false,
            symbol: String::new(),
            timeframe: String::new(),
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
