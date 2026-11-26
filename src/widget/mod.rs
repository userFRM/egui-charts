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

use crate::chart::rendering::ChartMapping;
use crate::chart::selection::SelectionState;
use crate::chart::series::SeriesSettings;
use crate::config::{ChartConfig, ChartOptions};
use crate::data::FeedState;
#[cfg(feature = "drawings")]
use crate::drawings::DrawingManager;
use crate::model::Timeframe;
use crate::model::{ChartType, ChartTypeParams};
use crate::studies::ChartState;
#[cfg(feature = "drawings")]
use crate::widget::tool_interaction::cursor_modes::CursorModeState;

pub use crate::chart::rendering::{
    Legend, LegendStatus, QuoteSide, StudyAction, StudyValue, ValueFormatters, Watermark,
    WatermarkLine,
};
pub use crate::chart::selection::{ChartElementId, SeriesId};
use crate::scales::{PriceMap, PriceScale};
use crate::studies::IndicatorRegistry;
pub use builder::ChartBuilder;
use chrono::{DateTime, Utc};
use egui::{Pos2, Rect, Vec2};
#[cfg(feature = "ui")]
pub(crate) use export::ColumnId;
pub use export::{ChartImage, DataTable, ExportError, ImageExportOptions, PixelDensity};
pub use feed::checkpoint::{
    TradeReplayCheckpoint, TradeReplayCheckpointError, TradeReplayCheckpointPlan,
};
pub use pane::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction};
#[cfg(feature = "ui")]
pub(crate) use readout::SeriesFingerprint;
use std::collections::BTreeMap;

// Re-export from logic layer
use pan_zoom::state;

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
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
pub use events::ImageRequest;
pub use events::{
    ChartEvent, ChartPointer, CursorDataPolicy, DataOperation, PointerKind, PointerTarget,
    StudyEventKind,
};
mod export;
mod format;
mod frame;
mod history;
pub(crate) mod interaction;
pub(crate) mod keyboard;
mod layout;
mod navigation;
pub(crate) mod palette;
pub(crate) mod pan_zoom;
mod pane;
#[doc(inline)]
pub use pane::{ChartLayoutRects, PaneId, PaneOptions, RenderedIndicatorPane};
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod posts;
mod primitives;
#[cfg(feature = "png-export")]
mod raster;
mod readout;
pub(crate) mod replay;
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
#[cfg(feature = "studies")]
mod trade_profile;

#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod drawing_api;
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
pub use drawing_api::DrawingError;
mod studies_data;
pub use crate::chart::primitives::{
    CustomPoint, PanePrimitive, PanePrimitiveView, PriceAxisLabel, Primitive, PrimitiveHit,
    PrimitiveId, PrimitivePass, PrimitiveView, TimeAxisLabel,
};
pub use pane::VisualOrder;
pub use series::{BarsInRange, MismatchDirection, PriceLineId, SeriesLastValue};
pub use series::{PaneScale, SeriesOptions};
#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
pub use snapshot::{
    ChartSnapshot, RestoreError, SavedAnnotations, SavedLayout, SavedPane, SavedSeries, SavedStudy,
};
mod volume;
pub use layout::{
    ChartLayout, ChartLayoutMode, ChartLayoutSizing, ChartLink, ChartLinkGroups, ChartSyncOptions,
    ChartTabRequest, TabSplit, TabSplitSide,
};
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub use layout::{LayoutCopy, LayoutCopyIssue, LayoutCopyIssueKind};
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
pub use layout::{SymbolDrawing, SymbolDrawings};
pub(crate) mod sync;
mod time_scale;
use feed::FeedLink;
pub use sync::VisibleTimeRange;
#[cfg(feature = "drawings")]
pub(crate) mod tool_interaction;

pub use crate::config::shortcut_map::ShortcutMap;
pub use keyboard::ShortcutRequest;
pub use state::{BoxZoomState, ElasticBounceState, KineticScrollState};

/// The main bars' revision, length and last high, low and close, and the pending sizes, that
/// the sizes derived from history were last resolved from.
type BoxSizesRead = (u64, usize, Option<[u64; 3]>, [bool; 4]);

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
    market_replay: Option<replay::MarketReplay>,
    /// Visual configuration controlling colors, padding, grid visibility, and more.
    pub(crate) config: ChartConfig,
    applied_config_scale_mode: crate::scales::PriceScaleMode,
    /// Whether the main price series and its volume are drawn.
    pub(crate) main_series_visible: bool,
    /// Whether every study is hidden as a group; see [`Self::set_indicators_hidden`].
    indicators_hidden: bool,
    main_series_present: bool,
    main_series_scale: crate::model::SeriesScale,
    price_series: Vec<series::PriceSeries>,
    heikin_ashi: crate::chart::rendering::candles::HeikinAshiCache,
    series_scales: std::collections::HashMap<crate::model::SeriesScale, PriceScale>,
    /// Chart behavior options (bar spacing, scroll/zoom constraints, time scale settings).
    pub(crate) chart_options: ChartOptions,
    pub(crate) keyboard: keyboard::KeyboardState,
    history: history::ChartHistory,
    pane_layout: pane::PaneLayout,
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
    /// Bar spacing when a time-axis drag began or was last rebased (UI state only)
    pub(crate) scroll_start_spacing: Option<f32>,
    /// Apply external start-index once to time scale (UI state only)
    pub(crate) pending_start_idx: Option<usize>,
    /// Timestamp to centre, retained while older history is loading.
    pub(crate) pending_scroll_time: Option<DateTime<Utc>>,
    /// Whether local navigation centres the first bar at or after the pending time.
    pub(crate) pending_scroll_next: bool,
    /// A range set before the first layout and the bar spacing, right offset and tracking mode
    /// its last fit left, fitted again at the plot width until the first laid-out frame is shown.
    pending_visible_range: Option<(time_scale::PendingRange, (f32, f64, bool))>,
    /// Animated scroll requested by the host, advanced each frame (UI state only)
    scroll_animation: Option<time_scale::ScrollAnimation>,
    /// Time axis below the plots in the last frame; zero height while its labels are hidden
    time_axis_rect: Rect,
    /// Whether the last price frame took a double-click on its plot, below its status line and
    /// away from the drawings, series and primitives there
    price_plot_double_click: bool,
    /// Chart type (candlestick, line, area, bar, Renko, Kagi)
    pub(crate) chart_type: ChartType,
    /// Parameters of the price-based chart types and the baseline
    pub(crate) chart_type_params: ChartTypeParams,
    #[cfg(feature = "studies")]
    trade_profiles: trade_profile::TradeProfiles,
    pending_box_sizes: [bool; 4],
    /// What the pending sizes were last resolved from: a size those bars could not give is not
    /// read again until the bars or the pending sizes change.
    box_sizes_read: Option<BoxSizesRead>,
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
    /// Changes when what the data table calculates from changes, unlike the steps of the chart's
    /// own refresh, whose results the table does not read.
    readout_revision: u64,
    /// Tells this chart from every other, also of another run: the data table keys its column
    /// choices by it, and the snapshots and drawings documents the chart writes name it.
    #[cfg(feature = "drawings")]
    data_table_identity: u64,
    study_data: studies_data::StudyDataCache,
    /// Whether the footprint statistics of the last frame had an open interest row, so that the
    /// study data requests the open interest they read.
    footprint_open_interest: bool,
    pub(crate) element_studies: crate::chart::rendering::candles::price_based::ElementStudies,
    /// Range bars built from the held trades, while the chart type is Range.
    execution_ranges: crate::chart::rendering::candles::price_based::ExecutionRanges,
    #[cfg(feature = "studies")]
    volume_setting: bool,
    #[cfg(feature = "studies")]
    volume_study: Option<SeriesId>,
    /// The colours last given to or taken from the Analyst price forecast studies, positive then
    /// negative; `None` gives them the configured colours on the next frame.
    #[cfg(feature = "studies")]
    price_target_colors: Option<[egui::Color32; 2]>,
    /// Id of each study in `studies`.
    pub(crate) study_ids: Vec<SeriesId>,
    /// The most studies the chart takes through the interface, if capped.
    study_limit: Option<usize>,
    /// The most studies reading another study's output the chart takes through the interface,
    /// if capped.
    chained_study_limit: Option<usize>,
    /// Whether each study in `studies` contributes to the main price auto-scale.
    pub(crate) study_autoscale: Vec<bool>,
    /// Host captions of the studies in `studies`, set with [`Chart::set_indicator_inputs`];
    /// empty where the host set none.
    pub(crate) study_inputs: Vec<String>,
    /// For each study in `studies`, the caption derived from its inputs, built on first read
    /// and dropped whenever the study may have changed.
    pub(crate) study_captions: Vec<std::sync::OnceLock<String>>,
    /// For each study in `studies`, the first bar it must be updated from on the next frame.
    pub(crate) pending_from: Vec<Option<usize>>,
    /// For each study in `studies`, whether it must be recomputed from the first bar while its
    /// values still hold: on the bars that remain after an eviction, after a drop of its
    /// order-flow input's oldest bars or after a live bar of a history it reads. It draws them
    /// until its recompute, which waits for `next_refresh` like a tail update.
    pub(crate) stale: Vec<bool>,
    /// For each study in `studies`, the input time in seconds from which it may be recomputed
    /// in full again.
    pub(crate) next_refresh: Vec<f64>,
    /// Id of the next study added.
    pub(crate) next_series: usize,
    /// Host primitives, in the order they were attached.
    primitives: Vec<primitives::Attached>,
    /// Id of the next primitive attached.
    next_primitive: u64,
    /// Omit the above-drawings primitive pass in an export snapshot.
    hide_foreground: bool,
    /// The chart's drawings and the active drawing tool.
    #[cfg(feature = "drawings")]
    pub(crate) drawings: DrawingManager,
    #[cfg(feature = "drawings")]
    symbol_drawings: std::collections::HashMap<
        String,
        (DrawingManager, marks::TimeOrdered<crate::model::Marker>),
    >,
    /// Counts the times [`Self::forget_symbol`] or [`Self::load_drawings`] dropped or replaced
    /// the drawings of a symbol at once, which `ChartLayout` tells from a last drawing deleted.
    #[cfg(feature = "drawings")]
    drawing_resets: u64,
    #[cfg(not(feature = "drawings"))]
    symbol_markers: std::collections::HashMap<String, marks::TimeOrdered<crate::model::Marker>>,
    /// Where each symbol's drawings are kept; see [`Chart::set_storage`].
    #[cfg(feature = "drawings")]
    drawing_store: Option<stored_drawings::DrawingStore>,
    /// The id each study a snapshot saved has since [`Chart::restore`] added it again.
    #[cfg(feature = "drawings")]
    restored_studies: std::collections::HashMap<usize, usize>,
    /// The chart that took the snapshot of the last restore, as its `saved_by` names it, or
    /// `None` before any restore and for a snapshot that names none. Only the drawings
    /// documents that chart wrote hold the saved ids `restored_studies` maps.
    #[cfg(feature = "drawings")]
    restored_from: Option<u64>,
    /// Original column ids of restored studies, retained for snapshots from earlier restores.
    #[cfg(feature = "drawings")]
    study_column_ids: std::collections::HashMap<usize, usize>,
    /// Box-zoom selection in progress
    pub(crate) box_zoom: BoxZoomState,
    /// Whether box-zoom mode is on: set by [`Chart::set_zoom_mode`], ended by a completed zoom
    /// and by an active drawing tool or the eraser.
    pub(crate) zoom_mode_active: bool,
    /// Whether a box zoom was completed in the last frame.
    pub(crate) zoom_just_applied: bool,
    /// For each entry of the zoom history of `state`, the scales its box zoom set in a study
    /// pane, as the zoom found them: empty for a zoom of the main series' pane, whose scale the
    /// history restores itself. Entries left by a cleared history go at the next zoom.
    zoom_scales: Vec<Vec<(PaneId, crate::model::SeriesScale, PriceScale)>>,
    /// Current symbol being displayed (for legend)
    pub(crate) symbol: String,
    /// Metadata supplied directly by a host without a feed.
    legend_instrument: Option<crate::model::Instrument>,
    currency_choices: Vec<String>,
    currency_choice: Option<String>,
    unit_choices: Vec<String>,
    unit_choice: Option<String>,
    legend_badge: Option<egui::ImageSource<'static>>,
    legend_statuses: Vec<(String, egui::Color32, egui::Color32)>,
    /// Current quote, independent of the historical readout.
    quote: Option<crate::model::Level1Quote>,
    quote_action: Option<QuoteSide>,
    /// Whether the last frame drew the Buy and Sell buttons, which the market order keys press.
    quote_buttons: bool,
    /// The quote panel's place as shares of its available horizontal and vertical room.
    quote_panel_at: Option<[f32; 2]>,
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
    /// Host formatters of single studies. Kept after a study's removal, so that undo restores
    /// it with its formatter; study ids are never reused.
    study_formatters:
        std::collections::HashMap<SeriesId, std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    extended_hours: bool,
    overnight_hours: bool,
    /// The futures data choices the chart settings applied, which the host's history follows.
    back_adjust_futures: bool,
    settlement_as_close: bool,
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
    /// Timeframe of the bars, for the legend, the session breaks and the feed
    pub(crate) timeframe: Option<Timeframe>,
    /// The feed the chart loads from, if it was built with one.
    pub(crate) feed_link: Option<FeedLink>,
    /// Host policy for the initial historical trade page, independent of the current binding.
    trade_history_on_demand: bool,
    /// Size the first prepared footprint page to its readers rather than candle history.
    footprint_history_by_view: bool,
    /// Cursor mode state (Demonstration, Magic, Eraser effects)
    #[cfg(feature = "drawings")]
    pub(crate) cursor_modes: CursorModeState,
    /// Shapes of drawings painted under a series or study this frame: the layer, the index they
    /// move before when the frame ends, and where they were painted.
    #[cfg(feature = "drawings")]
    pub(crate) layered_drawings: Vec<tool_interaction::LayeredDrawing>,
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
    /// X positions of the vertical grid lines of the last rendered price pane, which the study
    /// panes below it repeat; empty when the chart hides its vertical grid.
    pub(crate) last_vertical_grid_x: Vec<f32>,
    /// X positions of the session breaks of the last rendered price pane, which the other panes
    /// repeat; empty when the chart draws none.
    pub(crate) last_session_break_x: Vec<f32>,
    /// Last rendered indicator pane info for hit testing
    /// Each entry: (indicator_index, panel_rect, chart_rect, y_min, y_max, coords)
    pub(crate) last_rendered_indicator_panes: Vec<RenderedIndicatorPane>,
    // =========================================================================
    // Multi-Chart Sync State
    // =========================================================================
    /// This chart is an inactive, attached slot in a multi-chart layout for the current frame.
    pub(crate) layout_inactive: bool,
    /// The layout's field selectors occupy the right end of the status line's first row this frame.
    pub(crate) layout_link_controls: bool,
    /// `Control::Settings` is hidden: the legend's series settings button, the settings buttons
    /// of study and added series legend rows and the price axis corner's scale settings button
    /// are not drawn.
    pub(crate) settings_hidden: bool,
    /// Tells `ChartLayout` this is another chart: new for each chart and each restored snapshot,
    /// whose symbol and interval are no change for the layout to send to linked charts.
    pub(crate) layout_identity: u64,
    /// Host period, price and target, retained while a linked pointer takes precedence.
    host_crosshair: Option<(sync::CrosshairPeriod, Option<f64>, Option<SeriesId>)>,
    /// External crosshair position from synced chart (bar index)
    pub(crate) synced_crosshair_bar_idx: Option<f64>,
    /// Source period retained by time-based setters and resolved against the current bars.
    synced_crosshair_period: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Price of the external crosshair in its target's units; `None` draws the vertical guide
    /// alone.
    synced_crosshair_price: Option<f64>,
    /// Target bar series of a programmatic position; `None` uses the main scale.
    synced_crosshair_series: Option<SeriesId>,
    /// Time and target price of the crosshair last drawn, for exported images.
    last_crosshair: Option<(DateTime<Utc>, Option<f64>)>,
    /// Series whose units the last drawn host position used; `None` uses the main scale.
    last_crosshair_series: Option<SeriesId>,
    /// Last computed hover bar index (for sync emission to other charts)
    pub(crate) last_hover_bar_idx: Option<f64>,
    /// Time of the hovered whitespace slot after the last bar, sent to linked charts.
    pub(crate) last_hover_whitespace: Option<DateTime<Utc>>,
    /// Shown bar the status line, legends and data window read this frame: the one under the
    /// local pointer, else under a crosshair from a linked chart or the host; `None` reads the
    /// latest bar.
    pub(crate) readout_index: Option<usize>,

    // =========================================================================
    // Marks (Widget API)
    // =========================================================================
    /// Bar marks (annotations on chart bars, e.g., trade signals); see [`Chart::set_markers`].
    pub(crate) marks: marks::TimeOrdered<crate::model::Marker>,
    /// Fit and layer of the main series' marks; see [`Chart::set_marker_options`].
    marker_options: crate::model::MarkerOptions,
    /// Marks of the main series' recent changes; see [`Chart::set_change_markers`].
    change_marks: Option<change_marks::ChangeMarks>,
    /// Clock of the last frame that showed the chart, which dates change marks recorded while no
    /// frame shows it.
    frame_clock: Option<change_marks::FrameClock>,
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
    /// The host's text for `order_quantity`, set with it for the next frame only.
    #[cfg(feature = "ui")]
    pub(crate) order_quantity_text: Option<String>,
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
    /// Set by the context menus while they would answer a touch held on the price pane with
    /// a limit order, so the next frame's hold asks for one in place of its values. Cleared by
    /// every frame.
    #[cfg(feature = "ui")]
    pub(crate) touch_order_armed: bool,
    /// Where a touch held on the price pane asked for an order during the last frame, for the
    /// context menus to answer. It lives for one frame, as a right-click does.
    #[cfg(feature = "ui")]
    pub(crate) touch_order: Option<egui::Pos2>,
    /// Size to allocate instead of the available space; see [`Chart::set_size`].
    size: Option<Vec2>,
    /// The main bars in groups that fall within one pixel, while zoomed out that far.
    merge_groups: merge::Groups,
}

impl std::fmt::Debug for Chart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chart")
            .field("symbol", &self.symbol())
            .field("timeframe", &self.timeframe())
            .field("bars", &self.bars(SeriesId::MAIN).len())
            .finish_non_exhaustive()
    }
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

impl Chart {
    /// Supplies instrument metadata for a chart without a feed, including the price precision or
    /// fraction and the default tick-mark grid. An explicit `tick_marks.min_price_step` takes
    /// precedence, and so does a precision chosen in the chart settings or with
    /// [`PriceScale::set_price_precision`]. Pass `None` to clear the metadata. Feed metadata
    /// takes precedence when available.
    pub fn set_instrument(&mut self, instrument: Option<crate::model::Instrument>) {
        if self.legend_instrument != instrument {
            self.readout_revision = crate::model::next_revision();
        }
        self.legend_instrument = instrument;
        let precision = self
            .instrument()
            .and_then(crate::scales::instrument_price_precision);
        let instrument = self
            .feed_link
            .as_ref()
            .and_then(|link| link.instrument.as_ref())
            .or(self.legend_instrument.as_ref());
        let scale = self.state.price_scale_mut();
        scale.set_instrument_precision(precision);
        scale.set_instrument_ticks(instrument);
    }

    /// The decimals of the main series' prices in the status line, the tooltip, the data window
    /// and the data table: the precision chosen for the price scale, else the instrument's. `None`
    /// leaves each readout its own rule for prices without metadata.
    pub(crate) fn readout_price_decimals(&self) -> Option<u8> {
        self.state.price_scale().chosen_precision().or_else(|| {
            self.instrument()
                .and_then(crate::scales::instrument_price_precision)
        })
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

    /// Supplies the price units the host can convert the chart into, such as `tonne` for a
    /// symbol quoted per `barrel`. While
    /// [`ChartConfig::show_unit`](crate::config::ChartConfig::show_unit) is on, the menu of the
    /// currency and unit header above the price scale lists them under the currencies. Choosing
    /// one emits an intent; the host replaces bars and instrument metadata together after
    /// conversion.
    pub fn set_unit_choices(&mut self, units: Vec<String>) {
        self.unit_choices = units;
    }

    /// Takes the selected unit. No prices change until the host supplies converted data.
    pub fn take_unit_choice(&mut self) -> Option<String> {
        self.unit_choice.take()
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
    /// `None` removes it. This assigns the pane currently holding the main series; use
    /// [`Self::set_pane_watermark`] to address another pane. It stays with that pane when
    /// series move.
    pub fn set_watermark(&mut self, watermark: Option<Watermark>) {
        self.set_pane_watermark(self.main_series_pane(), watermark);
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
            self.history.clear_view_restore();
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

    /// Takes the last press of a quote button: a click, or the key of
    /// [`KeyboardAction::BuyMarket`](crate::config::KeyboardAction::BuyMarket) or `SellMarket`
    /// while the buttons are drawn. No order is submitted by the chart.
    pub fn take_quote_action(&mut self) -> Option<QuoteSide> {
        self.quote_action.take()
    }

    /// Activates or deactivates box-zoom mode.
    ///
    /// While active, a left-button drag that starts on a plot, in the price pane or another
    /// expanded pane, draws a selection rectangle and zooms into it: the selected bars fill the
    /// width and the selected values fill the pane the selection began in, while the other panes
    /// keep their scales. A press on an axis scales that axis, and a press on a drawing drags the
    /// drawing, as without the mode. The mode applies to one selection: it ends once a zoom is
    /// applied (see [`Chart::zoom_was_applied`]). A selection of 20 points or less zooms nothing
    /// and keeps it. A drawing tool or the eraser owns the plot's drags, so the next frame with
    /// either active ends the mode. [`Chart::is_zoom_mode_active`] reports it.
    pub fn set_zoom_mode(&mut self, active: bool) {
        self.zoom_mode_active = active;
    }

    /// Returns whether box-zoom mode is on, for a toolbar's Zoom in button.
    pub fn is_zoom_mode_active(&self) -> bool {
        self.zoom_mode_active
    }

    /// Restore the view preceding the last box zoom: the time range, the price pane's scale and
    /// the scales of the pane the selection began in. A scale that fitted its values before the
    /// zoom fits them again, and one with a manual range gets that range back. Drawing and chart
    /// edit history is unchanged.
    pub fn zoom_back(&mut self) -> bool {
        self.pending_scroll_time = None;
        self.tracking_mode_active = false;
        let popped = self.state.pop_zoom_state();
        if popped {
            self.history.clear_view_restore();
            self.restore_box_zoom_scales();
        }
        popped
    }

    /// Returns `true` if a box zoom was completed in the most recent frame. Box-zoom mode has
    /// ended by then, as [`Chart::is_zoom_mode_active`] reports.
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

    /// Shows or hides the chart's own settings buttons: the series settings button among the
    /// legend's title controls, the settings button of each study and added series legend row,
    /// and the scale settings button in the price axis corner. Hidden, they are not drawn and
    /// take no keyboard focus, while the legend's other controls stay, and an added series row's
    /// More button and right click, which open its settings, do nothing. Hosts call it with the
    /// visibility they give `Control::Settings`. All start visible.
    pub fn set_settings_buttons_visible(&mut self, visible: bool) {
        self.settings_hidden = !visible;
    }

    /// The rising and falling body, border and wick colors of a chart type that series settings
    /// offer from its own style, in that order; `None` for a type they keep in the chart
    /// configuration.
    fn candle_style_colors(
        styles: &mut crate::model::ChartTypeStyles,
        chart_type: ChartType,
    ) -> Option<[&mut egui::Color32; 6]> {
        match chart_type {
            ChartType::Heikin => {
                let style = &mut styles.heikin;
                Some([
                    &mut style.body_up,
                    &mut style.body_down,
                    &mut style.border_up,
                    &mut style.border_down,
                    &mut style.wick_up,
                    &mut style.wick_down,
                ])
            }
            ChartType::HollowCandles => {
                let style = &mut styles.hollow;
                Some([
                    &mut style.body_up,
                    &mut style.body_down,
                    &mut style.border_up,
                    &mut style.border_down,
                    &mut style.wick_up,
                    &mut style.wick_down,
                ])
            }
            ChartType::VolumeCandles => {
                let style = &mut styles.volume_candles;
                Some([
                    &mut style.body_up,
                    &mut style.body_down,
                    &mut style.border_up,
                    &mut style.border_down,
                    &mut style.wick_up,
                    &mut style.wick_down,
                ])
            }
            _ => None,
        }
    }

    /// The price the main series draws on its chart type: the own source of a Line, Step line,
    /// Line with markers, Area or Baseline chart, or the chart's shared one.
    fn main_price_source(&self) -> crate::model::PriceSource {
        self.chart_type_params
            .styles
            .line_source(self.chart_type, self.config.price_source)
    }

    /// Apply series settings to chart colors, price source and price scale.
    ///
    /// Copies candlestick colors (bullish/bearish fill, border, wick) from the given
    /// [`SeriesSettings`] into the chart's [`ChartConfig`], and its scale mode and inversion into
    /// the main price scale. Its price source becomes the own source of its chart type when that
    /// is a Line, Step line, Line with markers, Area or Baseline chart drawing another price;
    /// other chart types ignore it, and [`ChartConfig::price_source`] is left as it is.
    /// Settings of a Heikin-Ashi, hollow candles or volume candles chart keep their
    /// candle colors in that chart type's style instead, a `None` border or wick meaning the
    /// body color, and these colors win over the same colors in `chart_type_params`. A new
    /// scale mode returns the scale to auto-scaling and clears the zoom history, whose ranges
    /// are in the old mode's units. An ATR or percentage size whose method is unchanged keeps
    /// its resolved value. A changed TPO period length, row size or profile period drops the TPO
    /// merges and splits carried over from the chart, which were made for the old values;
    /// [`Chart::set_chart_type_params`] sets them as given. Call this when the user changes
    /// series appearance in a settings dialog.
    pub fn apply_series_settings(&mut self, settings: &SeriesSettings) {
        let mut params = settings.chart_type_params.clone();
        params.drop_stale_tpo_edits(&self.chart_type_params);
        if let Some(
            [
                body_up,
                body_down,
                border_up,
                border_down,
                wick_up,
                wick_down,
            ],
        ) = Self::candle_style_colors(&mut params.styles, settings.chart_type)
        {
            *body_up = settings.bullish_color;
            *body_down = settings.bearish_color;
            *border_up = settings.effective_bullish_border();
            *border_down = settings.effective_bearish_border();
            *wick_up = settings.effective_bullish_wick();
            *wick_down = settings.effective_bearish_wick();
        } else {
            self.config.bullish_color = settings.bullish_color;
            self.config.bearish_color = settings.bearish_color;
            self.config.bullish_border_color = settings.bullish_border_color;
            self.config.bearish_border_color = settings.bearish_border_color;
            self.config.bullish_wick_color = settings.bullish_wick_color;
            self.config.bearish_wick_color = settings.bearish_wick_color;
        }
        params.styles.set_line_source(
            settings.chart_type,
            settings.price_source,
            self.config.price_source,
        );
        self.config.show_price_line = settings.show_price_line;
        self.config.show_prev_close_line = settings.show_prev_close_line;
        self.main_series_style.alerts = settings.alerts;
        self.apply_chart_type_params(params);
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
    /// [`Self::apply_series_settings`]. The active chart type selects the dialog's controls and
    /// where the candle colors are read from and written to; applying keeps the chart's type.
    /// Its price source is the one the active chart type draws. Its `chart_alert_color` is the
    /// default alert colour; set it to the host's chart-wide alert colour before opening the dialog.
    pub fn series_settings(&self) -> SeriesSettings {
        let mut settings = SeriesSettings {
            chart_type: self.chart_type,
            chart_type_params: self.chart_type_params.clone(),
            bullish_color: self.config.bullish_color,
            bearish_color: self.config.bearish_color,
            bullish_border_color: self.config.bullish_border_color,
            bearish_border_color: self.config.bearish_border_color,
            bullish_wick_color: self.config.bullish_wick_color,
            bearish_wick_color: self.config.bearish_wick_color,
            price_source: self.main_price_source(),
            show_price_line: self.config.show_price_line,
            show_prev_close_line: self.config.show_prev_close_line,
            alerts: self.main_series_style.alerts,
            price_scale_mode: self.state.price_scale().options().mode,
            invert_scale: self.state.price_scale().options().invert_scale,
            ..Default::default()
        };
        if let Some(
            [
                body_up,
                body_down,
                border_up,
                border_down,
                wick_up,
                wick_down,
            ],
        ) = Self::candle_style_colors(&mut settings.chart_type_params.styles, self.chart_type)
        {
            settings.bullish_color = *body_up;
            settings.bearish_color = *body_down;
            settings.bullish_border_color = (*border_up != *body_up).then_some(*border_up);
            settings.bearish_border_color = (*border_down != *body_down).then_some(*border_down);
            settings.bullish_wick_color = (*wick_up != *body_up).then_some(*wick_up);
            settings.bearish_wick_color = (*wick_down != *body_down).then_some(*wick_down);
        }
        settings
    }

    /// Get a [`ChartMapping`] for coordinate conversions.
    ///
    /// Returns a mapping constructed from the last rendered frame's parameters
    /// (price rect, bar spacing, right offset, price range). This is used for
    /// converting between screen coordinates and data coordinates, particularly
    /// for drawing tool restoration and hit testing. On price-based chart types its indices
    /// count elements instead of source bars.
    pub fn get_chart_mapping(&self) -> ChartMapping {
        let mut mapping = ChartMapping::new(
            self.last_rendered_price_rect,
            self.state.time_scale().bar_spacing(),
            self.start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset_precise(),
            self.last_price_map,
        );
        mapping.numeric_positions = self.state.time_scale().numeric_positions().cloned();
        mapping
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
    /// Unedited chart type style colours follow the theme; edited ones stay. Other charts sharing
    /// the context follow the active theme on their next frame, keeping colours the host has
    /// edited and any per-chart theme override.
    pub fn set_theme(&mut self, ctx: &egui::Context, theme: &crate::theme::Theme) {
        self.config = self.config.clone().with_theme(theme);
        self.theme_override = None;
        self.follow_chart_theme(theme);
        crate::theme::apply_to_egui(ctx, theme);
        ctx.request_repaint();
    }

    /// Overrides this chart's palette and text sizes without changing sibling charts or egui.
    ///
    /// `Some(theme)` also applies its series colours to the chart configuration, and unedited
    /// chart type style colours follow it while edited ones stay. Explicit configuration edits
    /// made afterwards are kept. `None` follows the host's palette again, retaining host-edited
    /// colours. `set_theme` clears this override and themes egui.
    pub fn set_theme_override(&mut self, theme: Option<crate::theme::Theme>) {
        if let Some(theme) = &theme {
            self.config = self.config.clone().with_theme(theme);
            self.follow_chart_theme(theme);
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
    /// options.kinetic_scroll.damping_coeff = 0.9;
    /// chart.set_chart_options(options);
    /// # }
    /// ```
    ///
    /// The view is kept: the bar spacing changes only when `time_scale.bar_spacing` changes or
    /// the new spacing bounds exclude it, and the chart scrolls only when `time_scale.right_offset`
    /// or `right_offset_pixels` changes. A non-finite `bar_spacing`, `right_offset` or
    /// `right_offset_pixels` keeps its previous value, and a `kinetic_scroll.damping_coeff`
    /// outside `0.0..1.0`, a `min_scroll_speed` that is not a finite number above zero and a
    /// `max_scroll_speed` that is NaN or negative are replaced by their defaults.
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
            if new.bar_spacing != old.bar_spacing
                || (new.right_offset, new.right_offset_pixels)
                    != (old.right_offset, old.right_offset_pixels)
            {
                self.history.clear_view_restore();
            }
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
        let defaults = crate::config::KineticScrollOptions::default();
        let kinetic = &mut options.kinetic_scroll;
        if !(0.0..1.0).contains(&kinetic.damping_coeff) {
            kinetic.damping_coeff = defaults.damping_coeff;
        }
        // A coast ends only below a positive speed, and a release clamps to a limit that is a
        // number not below zero.
        if !kinetic.min_scroll_speed.is_finite() || kinetic.min_scroll_speed <= 0.0 {
            kinetic.min_scroll_speed = defaults.min_scroll_speed;
        }
        if kinetic.max_scroll_speed.is_nan() || kinetic.max_scroll_speed < 0.0 {
            kinetic.max_scroll_speed = defaults.max_scroll_speed;
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
        self.map_drawing_bars();
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

    /// Runs one frame at `time` seconds with `events` and returns the shapes it painted, each
    /// with its clip rect, and the cursor it asked for.
    pub(crate) fn run(
        ctx: &Context,
        chart: &mut Chart,
        time: f64,
        events: Vec<Event>,
    ) -> (Vec<(Rect, Shape)>, CursorIcon) {
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
        fn flatten(clip: Rect, shape: Shape, out: &mut Vec<(Rect, Shape)>) {
            match shape {
                Shape::Vec(shapes) => shapes
                    .into_iter()
                    .for_each(|shape| flatten(clip, shape, out)),
                shape => out.push((clip, shape)),
            }
        }
        let mut shapes = Vec::new();
        for clipped in std::mem::take(&mut output.shapes) {
            flatten(clipped.clip_rect, clipped.shape, &mut shapes);
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
        run(ctx, chart, time, events)
            .0
            .into_iter()
            .map(|(_, shape)| shape)
            .collect()
    }

    /// The fill and stroke colours of `shapes`, mesh vertices included.
    #[cfg(feature = "ui")]
    pub(crate) fn painted_colors(shapes: &[Shape]) -> Vec<egui::Color32> {
        shapes
            .iter()
            .flat_map(|shape| match shape {
                Shape::Vec(shapes) => painted_colors(shapes),
                Shape::Rect(rect) => vec![rect.fill, rect.stroke.color],
                Shape::LineSegment { stroke, .. } => vec![stroke.color],
                Shape::Path(path) => match path.stroke.color {
                    egui::epaint::ColorMode::Solid(color) => vec![path.fill, color],
                    _ => vec![path.fill],
                },
                Shape::Mesh(mesh) => mesh.vertices.iter().map(|vertex| vertex.color).collect(),
                _ => Vec::new(),
            })
            .collect()
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
