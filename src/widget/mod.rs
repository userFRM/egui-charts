//! The primary Chart widget for embedding interactive financial charts in egui.
//!
//! This module provides [`Chart`], the main entry point for rendering OHLCV
//! (Open-High-Low-Close-Volume) financial data as an interactive egui widget.
//! It supports multiple chart types (candlestick, line, area, bar, Renko, Kagi),
//! real-time data streaming, drawing tools, technical indicators, and
//! multi-chart synchronization.
//!
//! # Quick Start
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..100).map(|i| { let p = 100.0 + (i as f64 * 0.2).sin() * 5.0; egui_charts::model::Bar::new(chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::model::BarData;
//! use egui_charts::widget::Chart;
//!
//! let bar_data = BarData::from_bars(bars);
//!
//! // Create chart with OHLCV data
//! let mut chart = Chart::new(bar_data);
//!
//! // Show the chart in your egui update loop
//! # egui::__run_test_ui(|ui| {
//! egui::CentralPanel::default().show(ui, |ui| {
//!     chart.show(ui);
//! });
//! # });
//! ```
//!
//! # Builder Pattern
//!
//! Configure the chart before showing it:
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..100).map(|i| { let p = 100.0 + (i as f64 * 0.2).sin() * 5.0; egui_charts::model::Bar::new(chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! # let bar_data = egui_charts::model::BarData::from_bars(bars.clone());
//! # use egui_charts::{config::{ChartConfig, ChartOptions}, widget::Chart};
//! # let my_chart_config = ChartConfig::default();
//! # let opts = ChartOptions::default();
//! let mut chart = Chart::with_config(bar_data, my_chart_config) // Custom visual config
//!     .visible_bars(100)          // Show 100 bars at a time
//!     .with_chart_options(opts);  // Scroll/zoom behavior
//! ```
//!
//! # Real-Time Updates
//!
//! For live data feeds, reuse the chart instance across frames and merge each
//! bar as it arrives:
//!
//! ```rust
//! # use egui_charts::{model::Bar, widget::{Chart, SeriesId}};
//! // In your app struct:
//! struct MyApp {
//!     chart: Chart,
//! }
//!
//! impl MyApp {
//!     fn on_new_bar(&mut self, bar: Bar, ui: &mut egui::Ui) {
//!         self.chart.update(SeriesId::MAIN, bar);
//!         self.chart.show(ui);
//!     }
//! }
//! ```
//!
//! # Drawing Tools and Indicators
//!
//! The chart computes, updates and draws its studies and drawings itself:
//!
//! ```rust
//! # use egui_charts::{drawings::DrawingToolType, studies::{EMA, RSI}, widget::Chart};
//! # fn example(chart: &mut Chart, ui: &mut egui::Ui) {
//! let ema = chart.add_indicator(Box::new(EMA::new(20))); // on the main pane
//! chart.add_indicator(Box::new(RSI::new(14))); // in a pane below it
//! chart.drawings_mut().set_active_tool(Some(DrawingToolType::TrendLine));
//! chart.show(ui);
//! chart.remove_series(ema);
//! # }
//! ```
//!
//! # Multi-Chart Sync
//!
//! Synchronize crosshairs and time scales between multiple charts:
//!
//! ```rust
//! # fn example(chart_a: &egui_charts::Chart, chart_b: &mut egui_charts::Chart) {
//! chart_b.set_synced_crosshair_time(chart_a.hover_time());
//! if let Some(range) = chart_a.visible_time_range() {
//!     chart_b.apply_synced_time_range(range);
//! }
//! # }
//! ```
//!
//! # Sub-modules
//!
//! - [`builder`] -- Constructor and configuration methods for [`Chart`]
//! - [`indicator_pane`] -- Separate indicator panels (RSI, MACD, Stochastic)

use crate::chart::cursor_modes::CursorModeState;
use crate::chart::renderers::ChartMapping;
use crate::chart::selection::SelectionState;
use crate::chart::series::SeriesSettings;
use crate::config::{ChartConfig, ChartOptions};
use crate::data::FeedState;
use crate::drawings::DrawingManager;
use crate::model::ChartState;
use crate::model::Timeframe;
use crate::model::{ChartType, ChartTypeParams};
use crate::scales::{PriceMap, PriceScale};
use crate::studies::IndicatorRegistry;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
pub mod indicator_pane;
pub use crate::chart::renderers::{Legend, LegendStatus, QuoteSide, StudyAction, StudyValue};
pub use crate::chart::selection::{ChartElementId, SeriesId};
pub use builder::ChartBuilder;
use egui::{Pos2, Rect, Vec2};
pub use indicator_pane::{
    IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction,
};

// Re-export from logic layer
use crate::chart::state;

mod anchors;
pub mod builder;
mod clipboard;
mod crosshair;
mod feed;
mod frame;
mod history;
pub(crate) mod interaction;
pub mod keyboard;
pub mod layout;
mod navigation;
pub(crate) mod pan_zoom;
mod pane_layout;
mod panes;
mod readout;
mod selection;
mod series;
mod volume;
pub use layout::{ChartLayout, ChartLayoutMode, ChartSyncOptions};
pub(crate) mod sync;
use feed::FeedLink;
pub use sync::VisibleTimeRange;
pub(crate) mod tool_interaction;

pub use keyboard::{ShortcutMap, ShortcutRequest};
pub use state::{BoxZoomState, ElasticBounceState, KineticScrollState};

/// Interactive financial chart widget for egui.
///
/// `Chart` is the core rendering and interaction engine for displaying OHLCV
/// financial data. It handles all aspects of chart visualization including
/// candlestick/bar/line rendering, pan/zoom interactions, crosshair display,
/// drawing tool integration, and indicator overlays.
///
/// # Creating a Chart
///
/// Use [`Chart::new`] or [`Chart::with_config`] to construct, then call
/// [`Chart::show`] each frame to render:
///
/// ```rust
/// # let bars: Vec<egui_charts::model::Bar> = (0..100).map(|i| { let p = 100.0 + (i as f64 * 0.2).sin() * 5.0; egui_charts::model::Bar::new(chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
/// use egui_charts::model::BarData;
/// use egui_charts::widget::Chart;
///
/// let bar_data = BarData::from_bars(bars);
///
/// let mut chart = Chart::new(bar_data);
///
/// # egui::__run_test_ui(|ui| {
/// egui::CentralPanel::default().show(ui, |ui| {
///     let response = chart.show(ui);
///     // response can be used for additional interaction handling
/// });
/// # });
/// ```
///
/// # Supported Chart Types
///
/// - **Candles** -- Standard Japanese candlestick chart
/// - **Bars** -- OHLC bar chart
/// - **Line** -- Close-price line chart
/// - **Area** -- Filled area under the close-price line
/// - **Renko** -- Fixed-size brick chart (set brick size with [`Chart::set_chart_type_params`])
/// - **Kagi** -- Reversal-based chart (set reversal amount with [`Chart::set_chart_type_params`])
///
/// # Interaction Features
///
/// - **Pan**: Click and drag to scroll through history (with kinetic scrolling)
/// - **Zoom**: Mouse wheel to zoom in/out, pinch-to-zoom on trackpads
/// - **Box zoom**: Drag-select a region to zoom into (when zoom mode is active)
/// - **Price scale drag**: Drag the price axis to scale vertically
/// - **Crosshair**: Hover to see price/time at cursor position
/// - **Keyboard shortcuts**: Arrow keys, Home/End, +/- for navigation
/// - **Double-click**: Reset zoom on price or time axis
///
/// # Architecture
///
/// User drawing interactions and shortcuts share [`Chart::undo`] / [`Chart::redo`] with
/// host toolbar, menu and dialog edits wrapped in [`Chart::user_action`]. Direct programmatic
/// setters and feed/data updates do not enter this history. Use [`Chart::can_undo`] and
/// [`Chart::can_redo`] for toolbar availability, and [`Chart::take_history_changed`] to
/// synchronize host controls and data after restoring an interval or settings.
///
/// `Chart` owns a [`ChartState`] (data + coordinate systems) and a
/// [`ChartConfig`] (visual styling). Rendering is delegated to specialized
/// modules in `crate::chart::rendering`, while interaction logic lives in
/// `crate::chart::state`.
pub struct Chart {
    /// Backend state holding OHLCV data and coordinate system (time scale, price range).
    pub(crate) state: ChartState,
    /// Visual configuration controlling colors, padding, grid visibility, and more.
    pub(crate) config: ChartConfig,
    applied_config_scale_mode: crate::scales::PriceScaleMode,
    /// Whether the main price series and its volume are drawn.
    pub(crate) main_series_visible: bool,
    /// Chart behavior options (bar spacing, scroll/zoom constraints, time scale settings).
    pub(crate) chart_options: ChartOptions,
    pub(crate) keyboard: keyboard::KeyboardState,
    history: history::ChartHistory,
    pane_layout: pane_layout::PaneLayout,
    /// Starting index of visible range (for backward compatibility)
    pub(crate) start_idx: usize,
    /// Desired number of visible bars (from app state)
    pub(crate) desired_visible_bars: Option<usize>,
    /// Cache of last computed visible bars for external syncing
    pub(crate) last_visible_bars: usize,
    /// Whether to apply `desired_visible_bars` on next frame only
    pub(crate) apply_visible_bars_once: bool,
    /// Kinetic scroll animation state (UI state only)
    pub(crate) kinetic_scroll: KineticScrollState,
    /// Last scroll position for drag tracking (UI state only)
    pub(crate) scroll_start_pos: Option<Pos2>,
    /// Initial right offset when starting scroll (for drag) (UI state only)
    pub(crate) scroll_start_offset: Option<f32>,
    /// Previous widget width for resize handling (UI state only)
    pub(crate) prev_width: Option<f32>,
    /// Price-axis drag this frame: anchor y where the drag began, and the y movement (UI state only)
    pub(crate) price_axis_drag: Option<(f32, f32)>,
    /// Apply external start-index once to time scale (UI state only)
    pub(crate) pending_start_idx: Option<usize>,
    /// Timestamp to centre, retained while older history is loading.
    pub(crate) pending_scroll_time: Option<DateTime<Utc>>,
    /// Chart type (candlestick, line, area, bar, Renko, Kagi)
    pub(crate) chart_type: ChartType,
    /// Parameters of the price-based chart types and the baseline
    pub(crate) chart_type_params: ChartTypeParams,
    pending_box_sizes: [bool; 4],
    /// Whether older source bars can contribute to the shown series.
    pub(crate) older_bars_drawn: bool,
    /// Whether tracking mode is currently active
    pub(crate) tracking_mode_active: bool,
    /// Mouse entered chart area (for tracking mode exit detection)
    pub(crate) mouse_in_chart: bool,
    /// Feed state of the main series; see [`Chart::feed_state`].
    pub(crate) feed: FeedState,
    /// Times of main bars built from ticks, which a history bar of the same slot replaces.
    pub(crate) from_ticks: BTreeSet<DateTime<Utc>>,
    /// The chart's studies, in the order they were added.
    pub(crate) studies: IndicatorRegistry,
    #[cfg(feature = "studies")]
    volume_setting: bool,
    #[cfg(feature = "studies")]
    volume_study: Option<SeriesId>,
    /// Id of each study in `studies`.
    pub(crate) study_ids: Vec<SeriesId>,
    /// Whether each study in `studies` contributes to the main price auto-scale.
    pub(crate) study_autoscale: Vec<bool>,
    pub(crate) study_inputs: Vec<String>,
    /// For each study in `studies`, the first bar it must be updated from on the next frame.
    pub(crate) pending_from: Vec<Option<usize>>,
    /// For each study in `studies`, the input time in seconds from which it may be recomputed
    /// in full again.
    pub(crate) next_refresh: Vec<f64>,
    /// Id of the next study added.
    pub(crate) next_series: usize,
    /// The chart's drawings and the active drawing tool.
    pub(crate) drawings: DrawingManager,
    symbol_drawings: std::collections::HashMap<String, (DrawingManager, Vec<crate::model::Marker>)>,
    /// Right-click box zoom state
    pub(crate) box_zoom: BoxZoomState,
    /// Whether zoom mode is currently active (controlled by zoom toolbar button)
    pub(crate) zoom_mode_active: bool,
    /// Whether zoom was just applied in the last frame (for auto-deactivation)
    pub(crate) zoom_just_applied: bool,
    /// Current symbol being displayed (for legend)
    pub(crate) symbol: String,
    /// Metadata supplied directly by a host without a feed.
    legend_instrument: Option<crate::model::Instrument>,
    currency_choices: Vec<String>,
    currency_choice: Option<String>,
    legend_badge: Option<egui::ImageSource<'static>>,
    legend_statuses: Vec<(String, egui::Color32, egui::Color32)>,
    /// Current quote, independent of the historical readout.
    quote: Option<crate::model::Level1Quote>,
    quote_action: Option<QuoteSide>,
    server_time_offset: Option<chrono::TimeDelta>,
    bar_close_time: Option<chrono::DateTime<chrono::Utc>>,
    indicator_settings_request: Option<SeriesId>,
    session_quote: Option<crate::model::quote::SessionQuote>,
    session_provider: Option<std::sync::Arc<dyn crate::model::SessionProvider>>,
    extended_hours: bool,
    pub(crate) measured_axis_width: f32,
    /// Last usable widget width and its first frame, including discarded sizing passes.
    last_layout: Option<(f32, u64)>,
    last_ratio_lock: bool,
    axis_hover: Option<(Pos2, Rect)>,
    scale_settings_requested: bool,
    events: Vec<crate::model::Marker>,
    event_click: Option<String>,
    /// Timeframe of the bars, for the legend, the session breaks and the feed
    pub(crate) timeframe: Option<Timeframe>,
    /// The feed the chart loads from, if it was built with one.
    pub(crate) feed_link: Option<FeedLink>,
    /// Cursor mode state (Demonstration, Magic, Eraser effects)
    pub(crate) cursor_modes: CursorModeState,
    /// Price mapping of the last rendered frame (includes zoom adjustments)
    pub(crate) last_price_map: PriceMap,
    /// Last rendered price rect (actual rect used for candle rendering); see
    /// [`get_rendered_price_rect`](Chart::get_rendered_price_rect).
    pub(crate) last_rendered_price_rect: Rect,
    /// Last rendered volume rect (actual rect used for volume rendering)
    pub(crate) last_rendered_volume_rect: Rect,
    /// Last rendered indicator pane info for hit testing
    /// Each entry: (indicator_index, panel_rect, chart_rect, y_min, y_max, coords)
    pub(crate) last_rendered_indicator_panes: Vec<RenderedIndicatorPane>,
    // =========================================================================
    // Multi-Chart Sync State
    // =========================================================================
    /// External crosshair position from synced chart (bar index)
    pub(crate) synced_crosshair_bar_idx: Option<f64>,
    /// Last computed hover bar index (for sync emission to other charts)
    pub(crate) last_hover_bar_idx: Option<f64>,

    // =========================================================================
    // Marks (Widget API)
    // =========================================================================
    /// Bar marks (annotations on chart bars, e.g., trade signals); see [`Chart::set_markers`].
    pub(crate) marks: Vec<crate::model::Marker>,
    /// Horizontal lines on the main series; see [`Chart::set_price_lines`].
    price_lines: Vec<crate::model::PriceLine>,

    // =========================================================================
    // Selection State
    // =========================================================================
    /// Chart-wide click selection, shared by series, overlay indicators, and
    /// pane indicators. Populated by click hit testing during rendering and
    /// readable by host apps via [`Chart::selected_element`].
    pub(crate) selection: SelectionState<ChartElementId>,
    /// Right-click hit result captured during the last frame, drained by the
    /// host via [`Chart::take_right_click`] (or by `ChartMenus` with the `ui`
    /// feature). Holds no UI types so the core widget builds without the `ui`
    /// feature.
    pub(crate) right_click: Option<RightClickTarget>,
    /// Set by the context menus while one is open, so the next frame draws no crosshair or
    /// tooltip under it. Cleared by every frame.
    pub(crate) menu_open: bool,
}

/// The object a right-click landed on, with the data needed to position and
/// populate a context menu.
///
/// Captured during [`Chart::show`] when the user
/// right-clicks inside the chart area. Drain it once per frame with
/// [`Chart::take_right_click`]. Right-clicking also selects the hit object (the
/// same as a left click) so selection handles and the menu stay consistent.
///
/// This type intentionally carries no `ui`-feature types, so the core chart can
/// surface right-clicks even when built without the `ui` feature; the host
/// (or `ChartMenus` with the `ui` feature) maps it to a concrete menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RightClickTarget {
    /// A series or overlay/pane indicator line was hit.
    Element {
        /// The selected chart element (series or indicator).
        id: ChartElementId,
        /// Data-space index under the cursor, counting elements on price-based chart types.
        bar_idx: usize,
        /// Screen position of the click (menu anchor).
        pos: Pos2,
        /// Price under the cursor at the click position.
        price: f64,
    },
    /// A drawing object was hit. The drawing is identified by its
    /// [`crate::drawings::DrawingManager`] id.
    Drawing {
        /// Id of the hit drawing within the drawing manager.
        drawing_id: usize,
        /// Screen position of the click (menu anchor).
        pos: Pos2,
    },
    /// Empty chart area was hit (no series, indicator, or drawing).
    Background {
        /// Screen position of the click (menu anchor).
        pos: Pos2,
        /// Price under the cursor at the click position.
        price: f64,
    },
}

/// Information about a rendered indicator pane, used for hit testing and coordinate mapping.
///
/// After calling [`Chart::show`], each visible separate-pane indicator
/// (RSI, MACD, etc.) produces a `RenderedIndicatorPane` entry stored in the chart.
/// Platform code can use these to implement click-on-indicator-line selection,
/// tooltip display, or other interactive features.
///
/// Retrieve with [`Chart::get_rendered_indicator_panes`].
#[derive(Clone, Debug)]
pub struct RenderedIndicatorPane {
    /// Position of the study in [`Chart::indicators`] when the pane was drawn. After a pane's
    /// close button removed its study, that pane stays in the list until the next frame, and it
    /// and the panes after it point one position too far.
    pub indicator_idx: usize,
    /// Full panel rect (including y-axis labels)
    pub panel_rect: Rect,
    /// Chart drawing area rect (excluding y-axis labels)
    pub chart_rect: Rect,
    /// Y-axis minimum value
    pub y_min: f64,
    /// Y-axis maximum value
    pub y_max: f64,
    /// Coordinate parameters for x-axis calculation
    pub coords: IndicatorCoordParams,
    /// Full mapping, including manual ranges, scale modes and inversion.
    pub mapping: crate::chart::coords::ChartMapping,
}

/// Pre-computed layout rectangles for the chart's sub-regions.
///
/// Use [`Chart::calculate_layout_rects`] to obtain these rects for a given
/// widget area. They are useful for external hit-testing (e.g., determining
/// whether a click landed on the price area, volume area, or legend).
#[derive(Clone, Copy, Debug)]
pub struct ChartLayoutRects {
    /// The overall widget rect (entire chart area including axes and padding).
    pub widget_rect: Rect,
    /// The main price/candle area where OHLC data is rendered.
    pub price_rect: Rect,
    /// The volume area inside the price pane (empty if volume is hidden).
    pub volume_rect: Rect,
    /// The legend/OHLC info area at the top of the chart.
    pub legend_rect: Rect,
}

impl Default for ChartLayoutRects {
    fn default() -> Self {
        Self {
            widget_rect: Rect::NOTHING,
            price_rect: Rect::NOTHING,
            volume_rect: Rect::NOTHING,
            legend_rect: Rect::NOTHING,
        }
    }
}

impl Chart {
    /// Supplies instrument metadata for a chart without a feed, including price precision.
    /// Pass `None` to clear the metadata. Feed metadata takes precedence when available.
    pub fn set_instrument(&mut self, instrument: Option<crate::model::Instrument>) {
        self.legend_instrument = instrument;
        let precision = self
            .instrument()
            .and_then(crate::scales::instrument_price_precision);
        self.state.price_scale_mut().set_price_precision(precision);
    }

    /// Supplies currencies the host can convert the chart into. Choosing one emits an intent;
    /// the host replaces bars and instrument metadata together after conversion.
    pub fn set_currency_choices(&mut self, currencies: Vec<String>) {
        self.currency_choices = currencies;
    }

    /// Takes the selected currency. No prices change until the host supplies converted data.
    pub fn take_currency_choice(&mut self) -> Option<String> {
        self.currency_choice.take()
    }

    /// Supplies a host-owned instrument image for the legend. `None` hides it.
    pub fn set_legend_badge(&mut self, badge: Option<egui::ImageSource<'static>>) {
        self.legend_badge = badge;
    }

    /// Supplies status marks and their host-defined meaning and colours.
    pub fn set_legend_statuses(&mut self, statuses: &[LegendStatus<'_>]) {
        self.legend_statuses = statuses
            .iter()
            .map(|status| (status.text.to_owned(), status.color, status.background))
            .collect();
    }

    /// Supplies the current bid and ask, independently of the hovered historical bar.
    /// Pass `None` when quotes are unavailable or the symbol changes.
    pub fn set_quote(&mut self, quote: Option<crate::model::Level1Quote>) {
        self.quote = quote;
    }

    /// Supplies the current server time for a chart without a feed. The clock advances between
    /// calls; update it when a replay jumps. `None` restores the feed clock or latest quote time.
    pub fn set_server_time(&mut self, now: Option<chrono::DateTime<chrono::Utc>>) {
        self.server_time_offset = now.map(|time| time - chrono::Utc::now());
    }

    /// Supplies an exact closing instant for the newest bar, for irregular sessions or replays.
    /// `None` derives the close from the timeframe and the instrument's session.
    pub fn set_bar_close_time(&mut self, close: Option<chrono::DateTime<chrono::Utc>>) {
        self.bar_close_time = close;
    }

    /// Sets the right margin in bars and moves to that margin when it changes.
    pub fn set_right_margin(&mut self, bars: f32) {
        let options = &mut self.chart_options.time_scale;
        if bars.is_finite()
            && bars >= 0.0
            && (options.right_offset != bars || options.right_offset_pixels.is_some())
        {
            options.right_offset = bars;
            options.right_offset_pixels = None;
            let scale = self.state.time_scale_mut();
            scale.apply_options(
                scale.bar_spacing(),
                options.min_bar_spacing,
                options.max_bar_spacing,
                options.fix_left_edge,
                options.fix_right_edge,
                bars,
                None,
            );
        }
    }

    /// Supplies an independent extended-session price and qualifier. `None` hides the tag.
    pub fn set_session_quote(&mut self, quote: Option<crate::model::quote::SessionQuote>) {
        self.session_quote = quote;
    }

    /// Supplies event badges at the time-axis boundary, separately from bar annotations.
    pub fn set_events(&mut self, events: Vec<crate::model::Marker>) {
        self.events = events;
    }

    /// Takes the ID of the last clicked event badge.
    pub fn take_event_click(&mut self) -> Option<String> {
        self.event_click.take()
    }

    /// Takes a request to open the host's price-scale settings.
    pub fn take_scale_settings_request(&mut self) -> bool {
        std::mem::take(&mut self.scale_settings_requested)
    }

    /// Takes the last legend quote click. No order is submitted by the chart.
    pub fn take_quote_action(&mut self) -> Option<QuoteSide> {
        self.quote_action.take()
    }

    /// Calculate the layout sub-rects for a given widget rect.
    ///
    /// Given the overall widget area, this computes where the price chart,
    /// volume bars, and legend/OHLC header will be drawn. Useful for
    /// external hit-testing, overlay placement, or custom drawing on top
    /// of specific chart regions.
    ///
    /// The layout respects current config flags like `show_ohlc_info`,
    /// `show_time_labels`, and `show_volume`.
    pub fn calculate_layout_rects(&self, widget_rect: Rect) -> ChartLayoutRects {
        self.pane_layout_rects(widget_rect, self.config.show_time_labels)
    }

    fn pane_layout_rects(&self, widget_rect: Rect, show_time_labels: bool) -> ChartLayoutRects {
        use crate::styles::sizing::chart as sizing;

        let padding = self
            .config
            .padding
            .max(0.0)
            .min(widget_rect.width().min(widget_rect.height()).max(0.0) / 2.0);
        let study_axes = if !self.chart_type.is_time_independent()
            && (self.config.show_left_axis || self.config.show_right_axis)
        {
            self.studies
                .indicators()
                .iter()
                .filter(|study| {
                    study.is_visible()
                        && study.is_overlay()
                        && !study.is_volume()
                        && study.scale() == crate::studies::StudyScale::Independent
                })
                .count() as f32
                * self
                    .measured_axis_width
                    .max(self.config.right_axis_width)
                    .max(self.config.left_axis_width)
        } else {
            0.0
        };
        let left_axis = if self.config.show_left_axis {
            self.config.left_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let right_axis = if self.config.show_right_axis {
            self.config.right_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let study_axes = study_axes
            .min((widget_rect.width() - 2.0 * padding - left_axis - right_axis).max(0.0) * 0.5);
        let left_axis = left_axis + study_axes;
        let bottom_padding = if show_time_labels {
            sizing::BOTTOM_PADDING_WITH_TIME
        } else {
            0.0
        };
        let price_rect = Rect::from_min_size(
            widget_rect.min + Vec2::new((padding + left_axis).min(widget_rect.width()), padding),
            Vec2::new(
                (widget_rect.width() - 2.0 * padding - left_axis - right_axis).max(0.0),
                (widget_rect.height() - 2.0 * padding - bottom_padding).max(0.0),
            ),
        );
        let legend_rect = if self.config.show_ohlc_info {
            price_rect.shrink(8.0).intersect(Rect::from_min_size(
                price_rect.min + Vec2::splat(8.0),
                Vec2::new(price_rect.width(), 24.0),
            ))
        } else {
            Rect::NOTHING
        };
        let volume_rect = if self.shows_volume() {
            volume::overlay_rect(price_rect, &self.config)
        } else {
            Rect::ZERO
        };

        ChartLayoutRects {
            widget_rect,
            price_rect,
            volume_rect,
            legend_rect,
        }
    }

    /// Activates or deactivates box-zoom mode.
    ///
    /// When active, left-click drag draws a selection rectangle and zooms into
    /// that region. The mode auto-deactivates after a successful zoom operation
    /// (check with [`Chart::zoom_was_applied`]).
    pub fn set_zoom_mode(&mut self, active: bool) {
        self.zoom_mode_active = active;
    }

    /// Restore the view preceding the last box zoom. Drawing and chart edit history is unchanged.
    pub fn zoom_back(&mut self) -> bool {
        self.pending_scroll_time = None;
        self.tracking_mode_active = false;
        self.state.pop_zoom_state()
    }

    /// Returns `true` if a box-zoom was completed in the most recent frame.
    ///
    /// Use this to auto-deactivate zoom mode in your toolbar after the user
    /// completes a zoom selection.
    pub fn zoom_was_applied(&self) -> bool {
        self.zoom_just_applied
    }

    /// Sets the crosshair rendering style (Full, Dot, Arrow or Demonstration).
    ///
    /// Use this to connect a toolbar cursor-type selector to the chart.
    /// The style controls how the crosshair lines and labels are drawn
    /// when the user hovers over the chart area.
    pub fn set_crosshair_style(&mut self, style: crate::config::CrosshairStyle) {
        self.chart_options.crosshair.style = style;
    }

    /// Enables the configured values readout without changing pan or zoom.
    pub fn set_tooltip_enabled(&mut self, enabled: bool) {
        self.chart_options.tooltip.enabled = enabled;
    }

    /// Apply series settings to chart colors, price source and price scale.
    ///
    /// Copies candlestick colors (bullish/bearish fill, border, wick) and the
    /// price source field from the given [`SeriesSettings`] into the chart's
    /// [`ChartConfig`], and its scale mode and inversion into the main price
    /// scale. A new scale mode returns the scale to auto-scaling and clears the
    /// zoom history, whose ranges are in the old mode's units. Call this when
    /// the user changes series appearance in a settings dialog.
    pub fn apply_series_settings(&mut self, settings: &SeriesSettings) {
        self.config.bullish_color = settings.bullish_color;
        self.config.bearish_color = settings.bearish_color;
        self.config.bullish_border_color = settings.bullish_border_color;
        self.config.bearish_border_color = settings.bearish_border_color;
        self.config.bullish_wick_color = settings.bullish_wick_color;
        self.config.bearish_wick_color = settings.bearish_wick_color;
        self.config.price_source = settings.price_source;
        self.config.show_price_line = settings.show_price_line;
        self.config.show_prev_close_line = settings.show_prev_close_line;
        self.set_chart_type_params(settings.chart_type_params.clone());
        if self.state.price_scale().options().mode != settings.price_scale_mode {
            self.state.price_scale_mut().reset();
            self.state.clear_zoom_history();
        }
        let options = self.state.price_scale_mut().options_mut();
        options.mode = settings.price_scale_mode;
        options.invert_scale = settings.invert_scale;
    }

    /// Read the current appearance and chart type parameters for the series settings dialog.
    /// Pass the result to `SeriesSettingsDialog::open`, then apply its accepted settings with
    /// [`Self::apply_series_settings`]. The active chart type is dialog context only.
    pub fn series_settings(&self) -> SeriesSettings {
        SeriesSettings {
            chart_type: self.chart_type,
            chart_type_params: self.chart_type_params.clone(),
            bullish_color: self.config.bullish_color,
            bearish_color: self.config.bearish_color,
            bullish_border_color: self.config.bullish_border_color,
            bearish_border_color: self.config.bearish_border_color,
            bullish_wick_color: self.config.bullish_wick_color,
            bearish_wick_color: self.config.bearish_wick_color,
            price_source: self.config.price_source,
            show_price_line: self.config.show_price_line,
            show_prev_close_line: self.config.show_prev_close_line,
            price_scale_mode: self.state.price_scale().options().mode,
            invert_scale: self.state.price_scale().options().invert_scale,
            ..Default::default()
        }
    }

    /// Get a [`ChartMapping`] for coordinate conversions.
    ///
    /// Returns a mapping constructed from the last rendered frame's parameters
    /// (price rect, bar spacing, right offset, price range). This is used for
    /// converting between screen coordinates and data coordinates, particularly
    /// for drawing tool restoration and hit testing. On price-based chart types its indices
    /// count elements instead of source bars.
    pub fn get_chart_mapping(&self) -> ChartMapping {
        ChartMapping::new(
            self.last_rendered_price_rect,
            self.state.time_scale().bar_spacing(),
            self.start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset(),
            self.last_price_map,
        )
    }

    /// Returns a mutable reference to the chart's visual configuration.
    ///
    /// Use this to apply settings produced by a dialog in place, e.g.
    /// `settings.apply_to_config(chart.config_mut())`, without rebuilding the
    /// whole [`ChartConfig`]. Changes take effect on the next rendered frame. Wrap a host UI
    /// submission in [`Self::user_action`] to include it in chart history.
    pub fn config_mut(&mut self) -> &mut ChartConfig {
        &mut self.config
    }

    /// Apply a theme to this chart and to the UI in `ctx`, without changing data or pan and zoom.
    ///
    /// Other charts sharing the context keep their configured series colours until updated.
    pub fn set_theme(&mut self, ctx: &egui::Context, theme: &crate::theme::Theme) {
        self.config = theme.apply_to_config(self.config.clone());
        crate::theme::apply_to_egui(ctx, theme);
        ctx.request_repaint();
    }

    /// The chart's visual configuration.
    pub fn config(&self) -> &ChartConfig {
        &self.config
    }

    /// The chart's behaviour options, set with [`Chart::with_chart_options`].
    pub fn chart_options(&self) -> &ChartOptions {
        &self.chart_options
    }

    /// The chart's bars, time scale, price scale and zoom history, to read them.
    pub fn state(&self) -> &ChartState {
        &self.state
    }

    /// The main price scale, to zoom it or change its mode and options.
    pub fn price_scale_mut(&mut self) -> &mut PriceScale {
        self.state.price_scale_mut()
    }

    /// The chart's drawings and drawing tools.
    pub fn drawings(&self) -> &DrawingManager {
        &self.drawings
    }

    /// The chart's drawings and drawing tools, to set the active tool or to save and load the
    /// drawings. Direct host edits do not enter chart history; wrap UI-originated edits in
    /// [`Self::user_action`]. The manager's own history is for standalone drawing use.
    pub fn drawings_mut(&mut self) -> &mut DrawingManager {
        &mut self.drawings
    }
}

/// Headless frames for widget tests: a fixed 800x600 screen and explicit input events.
#[cfg(test)]
pub(crate) mod test_frames {
    use super::Chart;
    use crate::model::{Bar, BarData};
    use chrono::{TimeZone, Utc};
    use egui::{Context, Event, Pos2, RawInput, Rect, Shape, Vec2};

    /// 100 daily bars; bar `i` opens at `lows[i % len]` and closes at `highs[i % len]`.
    pub(crate) fn chart(lows: &[f64], highs: &[f64]) -> Chart {
        let bars = (0..100)
            .map(|i| {
                let t = Utc
                    .timestamp_opt(1_700_000_000 + i as i64 * 86_400, 0)
                    .unwrap();
                let (lo, hi) = (lows[i % lows.len()], highs[i % highs.len()]);
                Bar::new(t, lo, hi, lo, hi, 1.0)
            })
            .collect();
        Chart::new(BarData::from_bars(bars))
    }

    /// Runs one frame at `time` seconds with `events` and returns the shapes it painted.
    pub(crate) fn shapes(
        ctx: &Context,
        chart: &mut Chart,
        time: f64,
        events: Vec<Event>,
    ) -> Vec<Shape> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
        });
        fn flatten(shape: Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => shapes.into_iter().for_each(|shape| flatten(shape, out)),
                shape => out.push(shape),
            }
        }
        let mut shapes = Vec::new();
        for clipped in std::mem::take(&mut output.shapes) {
            flatten(clipped.shape, &mut shapes);
        }
        output.drop_without_applying_deltas();
        shapes
    }

    /// Runs one frame at `time` seconds with `events`.
    pub(crate) fn frame(ctx: &Context, chart: &mut Chart, time: f64, events: Vec<Event>) {
        shapes(ctx, chart, time, events);
    }
}
