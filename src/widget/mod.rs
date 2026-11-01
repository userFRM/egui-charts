//! Interactive charts, events, panes, layouts and export.
//!
//! Keep a [`Chart`] across frames and call [`Chart::show`] from your egui UI callback.
//! Construct it from [`BarData`](crate::model::BarData), or use [`ChartBuilder`] to
//! configure an instrument, interval and optional [`Datafeed`](crate::data::Datafeed).
//!
//! # Updating data
//!
//! Merge each live bar into its series. The chart updates studies when it next draws;
//! use [`Chart::set_data`] to replace a series' history.
//!
//! ```
//! use egui_charts::{Chart, model::{Bar, BarData}, widget::SeriesId};
//!
//! let mut chart = Chart::new(BarData::default());
//! let bar = Bar::new("2024-01-01T00:00:00Z".parse().unwrap(),
//!                    100.0, 102.0, 99.0, 101.0, 50.0);
//! chart.update(SeriesId::MAIN, bar);
//! # egui::__run_test_ui(|ui| {
//! chart.show(ui);
//! # });
//! ```
//!
//! # Host integration
//!
//! Read [`ChartEvent`] values with [`Chart::drain_events`] for host legends,
//! inspection and application actions. [`ChartLayout`] arranges multiple charts
//! and synchronizes their viewports and crosshairs by timestamp.
//!
//! Studies added with [`Chart::add_indicator`] are calculated and drawn by the
//! chart. Built-in studies require the `studies` feature; drawing tools require
//! `drawings`. Toolbars and dialogs in `ui` are separate components placed and
//! connected by the host.

#[cfg(feature = "drawings")]
use crate::chart::cursor_modes::CursorModeState;
use crate::chart::renderers::ChartMapping;
use crate::chart::selection::SelectionState;
use crate::chart::series::SeriesSettings;
use crate::config::{ChartConfig, ChartOptions};
use crate::data::FeedState;
#[cfg(feature = "drawings")]
use crate::drawings::DrawingManager;
use crate::model::Timeframe;
use crate::model::{ChartType, ChartTypeParams};
use crate::studies::ChartState;

use crate::scales::{PriceMap, PriceScale};
use crate::studies::IndicatorRegistry;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
mod indicator_pane;
pub use crate::chart::renderers::{
    Legend, LegendStatus, QuoteSide, StudyAction, StudyValue, ValueFormatters, Watermark,
    WatermarkLine,
};
pub use crate::chart::selection::{ChartElementId, SeriesId};
pub use builder::ChartBuilder;
use egui::{Pos2, Rect, Vec2};
pub use export::{ChartImage, DataTable, ImageExportOptions};
pub use indicator_pane::{
    IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction,
};

// Re-export from logic layer
use crate::chart::state;

mod accessibility;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod alerts;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod anchors;
mod builder;
mod change_marks;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod clipboard;
mod crosshair;
pub(crate) use crosshair::menu_value;
mod events;
mod feed;
mod marks;
mod merge;
mod signals;
mod targets;
pub use events::{ChartEvent, ChartPointer, PointerKind, PointerTarget};
mod export;
mod frame;
mod history;
pub(crate) mod interaction;
pub(crate) mod keyboard;
mod layout;
mod navigation;
mod palette;
pub(crate) mod pan_zoom;
mod pane_layout;
pub use pane_layout::{PaneId, PaneOptions};
mod panes;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod posts;
mod price_series;
mod primitives;
mod raster;
mod readout;
mod replay;
mod selection;
mod series;
pub mod settings;
#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
mod snapshot;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod stored_drawings;
mod studies;
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
mod study_templates;
mod svg;

#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod drawing_api;
mod studies_data;
pub use crate::chart::primitives::{
    PriceAxisLabel, Primitive, PrimitiveId, PrimitivePass, PrimitiveView, TimeAxisLabel,
};
pub use pane_layout::VisualOrder;
pub use price_series::SeriesOptions;
pub use series::{BarsInRange, MismatchDirection, PriceLineId};
#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
pub use snapshot::{
    ChartSnapshot, SavedAnnotations, SavedLayout, SavedPane, SavedSeries, SavedStudy,
};
mod volume;
pub use layout::{ChartLayout, ChartLayoutMode, ChartSyncOptions};
pub(crate) mod sync;
mod time_scale;
use feed::FeedLink;
pub use sync::VisibleTimeRange;
#[cfg(feature = "drawings")]
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
/// # Undo and redo
///
/// User drawing interactions and shortcuts share [`Chart::undo`] / [`Chart::redo`] with
/// host toolbar, menu and dialog edits wrapped in [`Chart::user_action`]. Direct programmatic
/// setters and feed/data updates do not enter this history. Use [`Chart::can_undo`] and
/// [`Chart::can_redo`] for toolbar availability, and [`Chart::take_history_changed`] to
/// synchronize host controls and data after restoring an interval or settings.
pub struct Chart {
    /// Backend state holding OHLCV data and coordinate system (time scale, price range).
    pub(crate) state: ChartState,
    replay: Option<replay::ReplayWindow>,
    /// Visual configuration controlling colors, padding, grid visibility, and more.
    pub(crate) config: ChartConfig,
    applied_config_scale_mode: crate::scales::PriceScaleMode,
    /// Whether the main price series and its volume are drawn.
    pub(crate) main_series_visible: bool,
    main_series_present: bool,
    main_series_scale: crate::model::SeriesScale,
    price_series: Vec<price_series::PriceSeries>,
    heikin_ashi: crate::chart::rendering::candles::HeikinAshiCache,
    series_scales: std::collections::HashMap<crate::model::SeriesScale, PriceScale>,
    /// Chart behavior options (bar spacing, scroll/zoom constraints, time scale settings).
    pub(crate) chart_options: ChartOptions,
    pub(crate) keyboard: keyboard::KeyboardState,
    history: history::ChartHistory,
    pane_layout: pane_layout::PaneLayout,
    pane_options: PaneOptions,
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
    pub(crate) scroll_start_offset: Option<f64>,
    /// Previous widget width for resize handling (UI state only)
    pub(crate) prev_width: Option<f32>,
    /// Price-axis drag this frame: anchor y where the drag began, and the y movement (UI state only)
    pub(crate) price_axis_drag: Option<(f32, f32)>,
    /// Apply external start-index once to time scale (UI state only)
    pub(crate) pending_start_idx: Option<usize>,
    /// Timestamp to centre, retained while older history is loading.
    pub(crate) pending_scroll_time: Option<DateTime<Utc>>,
    /// Animated scroll requested by the host, advanced each frame (UI state only)
    scroll_animation: Option<time_scale::ScrollAnimation>,
    /// Time axis below the plots in the last frame; zero height while its labels are hidden
    time_axis_rect: Rect,
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
    /// Origins of main bars built from ticks, distinguishing trades from empty heartbeats.
    pub(crate) from_ticks: BTreeMap<DateTime<Utc>, crate::data::merge::Origin>,
    /// Optional oldest-bar eviction limits, unlimited when absent.
    bar_limits: std::collections::HashMap<SeriesId, std::num::NonZeroUsize>,
    /// The chart's studies, in the order they were added.
    pub(crate) studies: IndicatorRegistry,
    study_data: studies_data::StudyDataCache,
    pub(crate) element_studies: crate::chart::rendering::candles::price_based::ElementStudies,
    /// Range bars built from the held trades, while the chart type is Range.
    execution_ranges: crate::chart::rendering::candles::price_based::ExecutionRanges,
    #[cfg(feature = "studies")]
    volume_setting: bool,
    #[cfg(feature = "studies")]
    volume_study: Option<SeriesId>,
    /// Id of each study in `studies`.
    pub(crate) study_ids: Vec<SeriesId>,
    /// The most studies the chart takes through the interface, if capped.
    study_limit: Option<usize>,
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
    /// Host primitives, in the order they were attached.
    primitives: Vec<primitives::Attached>,
    /// Id of the next primitive attached.
    next_primitive: u64,
    /// The chart's drawings and the active drawing tool.
    #[cfg(feature = "drawings")]
    pub(crate) drawings: DrawingManager,
    #[cfg(feature = "drawings")]
    symbol_drawings: std::collections::HashMap<
        String,
        (DrawingManager, marks::TimeOrdered<crate::model::Marker>),
    >,
    #[cfg(not(feature = "drawings"))]
    symbol_markers: std::collections::HashMap<String, marks::TimeOrdered<crate::model::Marker>>,
    /// Where each symbol's drawings are kept; see [`Chart::set_storage`].
    #[cfg(feature = "drawings")]
    drawing_store: Option<stored_drawings::DrawingStore>,
    /// The id each study a snapshot saved has since [`Chart::restore`] added it again.
    #[cfg(feature = "drawings")]
    restored_studies: std::collections::HashMap<usize, usize>,
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
    watermark: Option<Watermark>,
    legend_statuses: Vec<(String, egui::Color32, egui::Color32)>,
    /// Current quote, independent of the historical readout.
    quote: Option<crate::model::Level1Quote>,
    quote_action: Option<QuoteSide>,
    server_time_offset: Option<chrono::TimeDelta>,
    bar_close_time: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) indicator_settings_request: Option<SeriesId>,
    session_quote: Option<crate::model::quote::SessionQuote>,
    session_provider: Option<std::sync::Arc<dyn crate::model::SessionProvider>>,
    horizontal_scale: Option<std::sync::Arc<dyn crate::scales::HorizontalScale>>,
    locale: Option<String>,
    date_format: Option<String>,
    time_formatter: Option<std::sync::Arc<dyn crate::scales::TimeFormatter>>,
    price_formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    percentage_formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    volume_formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    series_formatters:
        std::collections::HashMap<SeriesId, std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    extended_hours: bool,
    conversion: Option<String>,
    pub(crate) measured_axis_width: f32,
    /// Last usable widget width and its first frame, including discarded sizing passes.
    last_layout: Option<(f32, u64)>,
    last_ratio_lock: bool,
    axis_hover: Option<(Pos2, Rect)>,
    scale_settings_requested: bool,
    pub(crate) theme_override: Option<crate::theme::Theme>,
    inherited_theme: Option<crate::theme::Theme>,
    /// Palettes the colours of a restored configuration may still follow, until the next frame.
    restored_palettes: Vec<ChartConfig>,
    host_events: events::HostEvents,
    pub(crate) indicator_source_request: Option<crate::studies::StudySource>,
    events: marks::TimeOrdered<crate::model::Marker>,
    time_scale_events: marks::TimeOrdered<crate::model::TimeScaleEvent>,
    /// How many of `time_scale_events`, at their end, an export froze from a mark source.
    frozen_source_events: usize,
    event_visibility: crate::model::EventVisibility,
    mark_source: Option<marks::MarkSource>,
    #[cfg(feature = "drawings")]
    post_source: Option<posts::PostSource>,
    signal_sources: Vec<signals::SignalSource>,
    targets: targets::TargetState,
    show_price_targets: bool,
    hidden_signal_categories: std::collections::BTreeSet<String>,
    event_click: Option<String>,
    /// Bar marks with host IDs painted this frame, and those of the previous frame, which
    /// clicks resolve against.
    mark_hits: std::cell::RefCell<Vec<marks::MarkHit>>,
    clickable_marks: Vec<marks::MarkHit>,
    /// Timeframe of the bars, for the legend, the session breaks and the feed
    pub(crate) timeframe: Option<Timeframe>,
    /// The feed the chart loads from, if it was built with one.
    pub(crate) feed_link: Option<FeedLink>,
    /// Cursor mode state (Demonstration, Magic, Eraser effects)
    #[cfg(feature = "drawings")]
    pub(crate) cursor_modes: CursorModeState,
    /// The main series' last value, price line, base line, autoscale and click tolerance.
    pub(crate) main_series_style: crate::model::SeriesStyle,
    /// The last bar's time and price when the last price pulse saw it, and the input time of
    /// that pulse's start, negative infinity before any update.
    pub(crate) last_price_pulse: std::cell::Cell<Option<(chrono::DateTime<chrono::Utc>, f64, f64)>>,
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
    /// This chart is an inactive, attached slot in a multi-chart layout for the current frame.
    pub(crate) layout_inactive: bool,
    /// External crosshair position from synced chart (bar index)
    pub(crate) synced_crosshair_bar_idx: Option<f64>,
    /// Source period retained by time-based setters and resolved against the current bars.
    synced_crosshair_period: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Main-pane price of the external crosshair; `None` centres its horizontal line.
    synced_crosshair_price: Option<f64>,
    /// Time and main-pane price of the crosshair last drawn, for exported images.
    last_crosshair: Option<(DateTime<Utc>, Option<f64>)>,
    /// Last computed hover bar index (for sync emission to other charts)
    pub(crate) last_hover_bar_idx: Option<f64>,

    // =========================================================================
    // Marks (Widget API)
    // =========================================================================
    /// Bar marks (annotations on chart bars, e.g., trade signals); see [`Chart::set_markers`].
    pub(crate) marks: marks::TimeOrdered<crate::model::Marker>,
    /// Fit and layer of the main series' marks; see [`Chart::set_marker_options`].
    marker_options: crate::model::MarkerOptions,
    /// Marks of the main series' recent changes; see [`Chart::set_change_markers`].
    change_marks: Option<change_marks::ChangeMarks>,
    /// Horizontal lines on the main series; see [`Chart::set_price_lines`].
    price_lines: Vec<(PriceLineId, crate::model::PriceLine)>,
    /// The number of price line handles issued.
    next_price_line: u64,
    /// Price axis tags of the trading lines drawn over the last frame, shown once on the next.
    #[cfg(feature = "ui")]
    pub(crate) trading_tags: Vec<(f64, egui::Color32)>,
    #[cfg(feature = "ui")]
    pub(crate) trading_outlined: Vec<usize>,
    #[cfg(feature = "ui")]
    pub(crate) trading_bid_ask: bool,
    /// The order quantity the legend shows under the quote buttons' spread, set by trading for
    /// the next frame only, so the box goes when trading stops drawing.
    #[cfg(feature = "ui")]
    pub(crate) order_quantity: Option<f64>,
    /// The legend's quantity box, when the user clicked it, for trading to open its calculator.
    #[cfg(feature = "ui")]
    pub(crate) quantity_clicked: Option<egui::Rect>,

    // =========================================================================
    // Selection State
    // =========================================================================
    /// Chart-wide click selection, shared by series, overlay indicators, and
    /// pane indicators. Populated by click hit testing during rendering and
    /// readable by host apps via [`Chart::selected_element`].
    pub(crate) selection: SelectionState,
    /// Right-click hit result captured during the last frame, drained by the
    /// host via [`Chart::take_right_click`] (or by `ChartMenus` with the `ui`
    /// feature). Holds no UI types so the core widget builds without the `ui`
    /// feature.
    pub(crate) right_click: Option<RightClickTarget>,
    /// Set by the context menus while one is open, so the next frame draws no crosshair or
    /// tooltip under it. Cleared by every frame.
    pub(crate) menu_open: bool,
    /// Size to allocate instead of the available space; see [`Chart::set_size`].
    size: Option<Vec2>,
    /// The main bars in groups that fall within one pixel, while zoomed out that far.
    merge_groups: merge::Groups,
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
#[non_exhaustive]
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
    /// drawing id.
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
#[non_exhaustive]
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
#[non_exhaustive]
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

    /// Supplies an image for the legend's symbol mark in place of
    /// [`Instrument::logo`](crate::model::Instrument::logo). `None` shows the instrument's logo,
    /// or the symbol's initial without one;
    /// [`ChartConfig::show_legend_badge`](crate::config::ChartConfig::show_legend_badge) hides the mark.
    pub fn set_legend_badge(&mut self, badge: Option<egui::ImageSource<'static>>) {
        self.legend_badge = badge;
    }

    /// Paints `watermark` behind the series of the price pane, in addition to the symbol
    /// watermark of [`ChartConfig::show_watermark`](crate::config::ChartConfig::show_watermark).
    /// `None` removes it.
    pub fn set_watermark(&mut self, watermark: Option<Watermark>) {
        self.watermark = watermark;
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
        if let Some(replay) = &mut self.replay {
            replay.quote = quote;
            return;
        }
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
        if let Some(replay) = &mut self.replay {
            replay.session_quote = quote;
            return;
        }
        self.session_quote = quote;
    }

    /// Supplies event badges at the time-axis boundary, separately from bar annotations.
    pub fn set_events(&mut self, events: Vec<crate::model::Marker>) {
        self.events = events.into();
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
        let padding = self
            .config
            .padding
            .max(0.0)
            .min(widget_rect.width().min(widget_rect.height()).max(0.0) / 2.0);
        let study_axes = if self.config.show_left_axis || self.config.show_right_axis {
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
        let series_axes = self.series_axis_keys();
        let series_left = series_axes
            .iter()
            .filter(|key| **key != crate::model::SeriesScale::Right)
            .count() as f32
            * self
                .measured_axis_width
                .max(self.config.right_axis_width)
                .max(self.config.left_axis_width);
        let series_right = if series_axes.contains(&crate::model::SeriesScale::Right) {
            self.measured_axis_width.max(self.config.right_axis_width)
        } else {
            0.0
        };
        let study_axes = study_axes + series_left;
        let left_axis = if self.config.show_left_axis {
            self.config.left_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let right_axis = if self.config.show_right_axis {
            self.config.right_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        } + series_right;
        let study_axes = study_axes
            .min((widget_rect.width() - 2.0 * padding - left_axis - right_axis).max(0.0) * 0.5);
        let left_axis = left_axis + study_axes;
        let bottom_padding = if show_time_labels {
            self.time_axis_height()
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

    /// Sets the crosshair rendering style (Full, Dot, Arrow, Demonstration or Magic).
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
        self.main_series_style.alerts = settings.alerts;
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
            alerts: self.main_series_style.alerts,
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
            self.state.time_scale().right_offset_precise(),
            self.last_price_map,
        )
    }

    /// Returns a mutable reference to the chart's visual configuration.
    ///
    /// Use this to edit visual options in place, e.g.
    /// `chart.config_mut().show_grid = false`, without rebuilding the
    /// whole [`ChartConfig`]. Changes take effect on the next rendered frame. Wrap a host UI
    /// submission in [`Self::user_action`] to include it in chart history.
    pub fn config_mut(&mut self) -> &mut ChartConfig {
        &mut self.config
    }

    /// Apply a theme to this chart and to the UI in `ctx`, without changing data or pan and zoom.
    ///
    /// Other charts sharing the context follow the active theme on their next frame, keeping
    /// colours the host has edited and any per-chart theme override.
    pub fn set_theme(&mut self, ctx: &egui::Context, theme: &crate::theme::Theme) {
        self.config = self.config.clone().with_theme(theme);
        self.theme_override = None;
        self.inherited_theme = Some(theme.clone());
        crate::theme::apply_to_egui(ctx, theme);
        ctx.request_repaint();
    }

    /// Overrides this chart's palette and text sizes without changing sibling charts or egui.
    ///
    /// `Some(theme)` also applies its series colours to the chart configuration. Explicit
    /// configuration edits made afterwards are kept. `None` follows the host's palette again,
    /// retaining host-edited colours. `set_theme` clears this override and themes egui.
    pub fn set_theme_override(&mut self, theme: Option<crate::theme::Theme>) {
        if let Some(theme) = &theme {
            self.config = self.config.clone().with_theme(theme);
            self.inherited_theme = Some(theme.clone());
        }
        self.theme_override = theme;
    }

    /// The chart's visual configuration.
    pub fn config(&self) -> &ChartConfig {
        &self.config
    }

    /// The chart's behaviour options, set with [`Chart::with_chart_options`] or
    /// [`Chart::set_chart_options`].
    pub fn chart_options(&self) -> &ChartOptions {
        &self.chart_options
    }

    /// Replaces the chart's behaviour options at runtime, taking effect on the next frame.
    ///
    /// To change some options and keep the rest, edit a copy of the current ones:
    ///
    /// ```rust
    /// # fn example(chart: &mut egui_charts::Chart) {
    /// let mut options = chart.chart_options().clone();
    /// options.crosshair.mode = egui_charts::config::CrosshairMode::Magnet;
    /// options.time_scale.min_bar_spacing = 0.1;
    /// chart.set_chart_options(options);
    /// # }
    /// ```
    ///
    /// The view is kept: the bar spacing changes only when `time_scale.bar_spacing` changes or
    /// the new spacing bounds exclude it, and the chart scrolls only when `time_scale.right_offset`
    /// or `right_offset_pixels` changes. A non-finite `bar_spacing`, `right_offset` or
    /// `right_offset_pixels` keeps its previous value, and a `kinetic_scroll.damping_coeff`
    /// outside `0.0..1.0` is replaced by the default.
    pub fn set_chart_options(&mut self, options: ChartOptions) {
        let options = self.normalize_chart_options(options);
        let (old, new) = (&self.chart_options.time_scale, &options.time_scale);
        if (
            old.bar_spacing,
            old.min_bar_spacing,
            old.max_bar_spacing,
            old.fix_left_edge,
            old.fix_right_edge,
            old.right_offset,
            old.right_offset_pixels,
        ) != (
            new.bar_spacing,
            new.min_bar_spacing,
            new.max_bar_spacing,
            new.fix_left_edge,
            new.fix_right_edge,
            new.right_offset,
            new.right_offset_pixels,
        ) {
            let scale = self.state.time_scale_mut();
            let offset = scale.right_offset_precise();
            let spacing = if new.bar_spacing == old.bar_spacing {
                scale.bar_spacing()
            } else {
                new.bar_spacing
            };
            scale.apply_options(
                spacing,
                new.min_bar_spacing,
                new.max_bar_spacing,
                new.fix_left_edge,
                new.fix_right_edge,
                new.right_offset,
                new.right_offset_pixels,
            );
            if (old.right_offset, old.right_offset_pixels)
                == (new.right_offset, new.right_offset_pixels)
            {
                scale.set_right_offset(offset);
            }
        }
        self.chart_options = options;
    }

    /// Normalize numeric options before storing or comparing them, since NaN never equals itself.
    fn normalize_chart_options(&self, mut options: ChartOptions) -> ChartOptions {
        if !(0.0..1.0).contains(&options.kinetic_scroll.damping_coeff) {
            options.kinetic_scroll.damping_coeff =
                crate::config::KineticScrollOptions::default().damping_coeff;
        }
        let (old, new) = (&self.chart_options.time_scale, &mut options.time_scale);
        if !new.bar_spacing.is_finite() {
            new.bar_spacing = old.bar_spacing;
        }
        if !new.right_offset.is_finite() {
            new.right_offset = old.right_offset;
        }
        if !new.right_offset_pixels.is_none_or(f32::is_finite) {
            new.right_offset_pixels = old.right_offset_pixels;
        }
        options
    }

    /// Gives the chart an explicit size in points instead of filling the space `show` is given;
    /// `None`, the default, fills it again. The size includes the axes and study panes.
    pub fn set_size(&mut self, size: Option<Vec2>) {
        self.size = size.filter(|size| size.is_finite() && size.min_elem() >= 0.0);
    }

    /// The explicit size set with [`Chart::set_size`].
    pub fn size(&self) -> Option<Vec2> {
        self.size
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
    #[cfg(feature = "drawings")]
    #[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
    pub fn drawings(&self) -> &DrawingManager {
        &self.drawings
    }

    /// The chart's drawings and drawing tools, to set the active tool or to save and load the
    /// drawings. Direct host edits do not enter chart history; wrap UI-originated edits in
    /// [`Self::user_action`]. The manager's own history is for standalone drawing use.
    #[cfg(feature = "drawings")]
    #[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
    pub fn drawings_mut(&mut self) -> &mut DrawingManager {
        &mut self.drawings
    }
}

/// Headless frames for widget tests: a fixed 800x600 screen and explicit input events.
#[cfg(test)]
pub(crate) mod test_frames {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    use super::Chart;
    use crate::model::{Bar, BarData};
    use chrono::{TimeZone, Utc};
    use egui::{Context, CursorIcon, Event, Pos2, RawInput, Rect, Shape, Vec2};

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

    fn run(
        ctx: &Context,
        chart: &mut Chart,
        time: f64,
        events: Vec<Event>,
    ) -> (Vec<Shape>, CursorIcon) {
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
        let cursor = output.platform_output.cursor_icon;
        output.drop_without_applying_deltas();
        (shapes, cursor)
    }

    /// Runs one frame at `time` seconds with `events` and returns the shapes it painted.
    pub(crate) fn shapes(
        ctx: &Context,
        chart: &mut Chart,
        time: f64,
        events: Vec<Event>,
    ) -> Vec<Shape> {
        run(ctx, chart, time, events).0
    }

    /// The heights of the candles in `color` among `shapes`.
    pub(crate) fn candle_ys(shapes: &[Shape], color: egui::Color32) -> Vec<f32> {
        shapes
            .iter()
            .flat_map(|shape| match shape {
                Shape::Vec(shapes) => candle_ys(shapes, color),
                Shape::Rect(rect) if rect.fill == color => {
                    vec![rect.rect.min.y, rect.rect.max.y]
                }
                Shape::LineSegment { points, stroke } if stroke.color == color => {
                    points.iter().map(|point| point.y).collect()
                }
                Shape::Mesh(mesh) => mesh
                    .vertices
                    .iter()
                    .filter(|vertex| vertex.color == color)
                    .map(|vertex| vertex.pos.y)
                    .collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    /// Runs one frame at `time` seconds with `events` and returns the cursor it asked for.
    pub(crate) fn frame(
        ctx: &Context,
        chart: &mut Chart,
        time: f64,
        events: Vec<Event>,
    ) -> CursorIcon {
        run(ctx, chart, time, events).1
    }
}
