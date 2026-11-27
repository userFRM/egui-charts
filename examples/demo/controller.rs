//! Application controller: owns every dialog, panel, and the replay engine, and
//! routes the typed action each library widget emits into a real chart mutation.
//!
//! The chart engine is pure: widgets return `Action` enums and never touch the
//! chart themselves. This controller is the host half that consumes those
//! actions. Layouts live in host memory; templates use application storage. Controls that need
//! a trading account are hidden.

#[path = "annotations.rs"]
mod annotations;
#[path = "audio.rs"]
mod audio;
#[path = "commands.rs"]
mod commands;
#[path = "drawing_tools.rs"]
mod drawing_tools;
#[path = "formatting.rs"]
mod formatting;
#[path = "ideas.rs"]
mod ideas;
#[path = "images.rs"]
mod images;
#[path = "interval.rs"]
mod interval;
#[path = "layouts.rs"]
mod layouts;
#[path = "numeric_scenes.rs"]
mod numeric_scenes;
#[path = "primitive_scene.rs"]
mod primitive_scene;
use egui_charts::widget::ChartLayout;
use layouts::{DemoChart, LayoutTab};

#[path = "comparison.rs"]
mod comparison;
#[path = "marks.rs"]
mod marks;
#[path = "owndata/mod.rs"]
mod owndata;
#[path = "replay.rs"]
mod replay;
#[path = "storage.rs"]
mod storage;
#[path = "strategy.rs"]
mod strategy;
#[path = "strategy_live.rs"]
mod strategy_live;
#[path = "study_editor.rs"]
mod study_editor;
#[path = "templates.rs"]
mod templates;
use egui_charts::services::StoreKind;
use egui_charts::services::{ChatUser, ProfileDirectory, ProfileId};
use egui_charts::studies::StudyTemplateSet;
use egui_charts::ui::dialogs::templates::TemplateDialog;
use egui_charts::ui::profile::{ProfileAction, ProfileCard};
#[path = "feeds.rs"]
mod feeds;
#[cfg(target_arch = "wasm32")]
#[path = "landing.rs"]
pub(crate) mod landing;
#[path = "live.rs"]
mod live;
#[path = "market.rs"]
mod market;
#[path = "orders.rs"]
mod orders;

use eframe::egui;
use egui_charts::Chart;
use egui_charts::chart::overlays::{BookProfile, OrderBookView};
use egui_charts::data::{DepthHistory, DepthHistoryConfig, OrderFilter, OrderSizeTiers};
use egui_charts::model::{Alert, AlertCondition, DateRange, Watchlist};
use egui_charts::model::{Bar, BarData, BoxSizeMethod, ChartType, Tick, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::ui::app_state::ChartAppState;
use egui_charts::ui::context_menu::{
    ChartContextAction, ChartMenus, ContextMenuAction, HostMenuEntry,
};
use egui_charts::ui::dialogs::DrawingContextMenuAction;
use egui_charts::ui::dialogs::{
    AlertDialog, AlertDialogAction, DrawingId, DrawingPropertiesAction, DrawingPropertiesDialog,
    KeyboardShortcutsAction, KeyboardShortcutsDialog, SeriesContextMenuAction,
    StudyContextMenuAction,
};
use egui_charts::ui::drawing_toolbar::{DrawingToolbar, DrawingToolbarAction};
use egui_charts::ui::export::{ExportDialog, ExportFormat, ExportResponse, ExportScope};
use egui_charts::ui::floating_replay_control::{FloatingReplayControl, ReplayControlAction};
use egui_charts::ui::floating_toolbar::FloatingSelectionToolbar;
use egui_charts::ui::replay::{
    ReplayAction, ReplayController, ReplayScope, ReplaySession, ReplayStartPicker, ReplayToolbar,
    ReplayToolbarAction, ReplayToolbarState,
};
use egui_charts::ui::timeframe_toolbar::{
    DatePicker, DatePickerAction, SessionType, TimeframeToolbar, TimeframeToolbarAction,
    default_time_zones,
};
#[cfg(test)]
use egui_charts::ui::top_toolbar::CompareAction;
use egui_charts::ui::top_toolbar::components::SymbolSearchDialog;
use egui_charts::ui::top_toolbar::components::indicator_dialog::{
    IndicatorDialog, IndicatorDialogAction, IndicatorSettingsDialog,
};
use egui_charts::ui::top_toolbar::components::settings_menu::{SettingsAction, SettingsDialog};
use egui_charts::ui::top_toolbar::{TopToolbar, TopToolbarAction};
use egui_charts::ui::widget_bar::{AlertPanelAction, AlertsPanel};
use egui_charts::ui::widget_bar::{
    CalendarAction, CalendarPanel, ChatsPanel, HotlistsAction, HotlistsPanel, NewsAction,
    NewsPanel, NotificationsPanel, Product, ProductsAction, ProductsPanel, ScreenerAction,
    ScreenerPanel, SymbolDetailsPanel, TimeAndSalesPanel, WatchlistAction, WatchlistPanel,
    WidgetBarSection, WidgetBarWidget,
};
use egui_charts::ui::widget_bar::{DataWindowInfo, ObjectTreeAction, ObjectTreePanel, SourceItem};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBar, WidgetBarAction};
use egui_charts::ui_kit::EmptyState;
use egui_charts::ui_kit::notifications::{NotificationPanel, Toasts};
use egui_charts::widget::settings::ChartSettingsState;
use egui_charts::widget::{ChartLayoutMode, LayoutCopy, SeriesId};
use live::LiveFeed;
use std::sync::Arc;

use crate::sampledata::{
    DemoSymbol, SAMPLE_LOGO, SAMPLE_LOGO_SVG, us_equity_session, visible_bars_for_range,
};

/// Object-tree ids for studies and added series are offset into a high range so a single
/// `usize` id space cleanly distinguishes series from drawings (which use
/// small monotonic ids from the drawing manager). The main series reserves the last id.
const MAIN_SERIES_ID: usize = usize::MAX;
/// Local tones offered by the settings dialog.
const DEMO_SOUNDS: [&str; 3] = ["Chime", "Bell", "Beep"];
const INDICATOR_ID_BASE: usize = usize::MAX / 2;

/// Resting order sizes the demo marks: the block orders of the order-level instrument reach them,
/// its ordinary orders do not.
const ORDER_TIERS: OrderSizeTiers = OrderSizeTiers::new(100.0, 250.0);

/// The largest watchlist file the demo reads, far more than the 1,000 entries a list keeps.
const MAX_WATCHLIST_BYTES: u64 = 1024 * 1024;

/// A chosen watchlist file's name and bytes, `None` when the choice was cancelled, or why the
/// file could not be read.
type WatchlistFile = Result<Option<(String, Vec<u8>)>, String>;

/// The heatmap's history of `symbol`'s book: of its individual orders when its feed sends them, so
/// the cells mark the large ones, otherwise of the aggregate depth.
fn depth_history(symbol: &str) -> DepthHistory {
    let mut config = DepthHistoryConfig::default();
    config.orders = (symbol == live::ORDER_LEVEL_SYMBOL).then(OrderFilter::default);
    DepthHistory::new(config).expect("default depth history")
}

/// How the live book is shown: highlighted at [`ORDER_TIERS`].
fn book_view() -> OrderBookView {
    let mut view = OrderBookView::default();
    view.tiers = Some(ORDER_TIERS);
    view
}

fn indicator_tree_id(id: SeriesId) -> usize {
    INDICATOR_ID_BASE + id.0
}

fn indicator_from_tree_id(id: usize) -> Option<SeriesId> {
    if id == MAIN_SERIES_ID {
        None
    } else {
        id.checked_sub(INDICATOR_ID_BASE).map(SeriesId)
    }
}

/// How much of the window the account panel takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountPanelSize {
    Closed,
    Normal,
    Minimized,
    Maximized,
}

/// Example tiles beside the paper account in the broker picker. They show what a host's tiles
/// carry; connecting to them only reports that the demo has no broker behind them.
fn example_brokers() -> Vec<egui_charts::ui::trading::BrokerTile> {
    use egui_charts::ui::trading::BrokerTile;
    [
        ("example-stocks", "Example Stocks", 4.8, true),
        ("example-futures", "Example Futures", 4.6, false),
        ("example-crypto", "Example Crypto", 4.5, false),
    ]
    .into_iter()
    .map(|(id, name, rating, featured)| {
        let mut tile = BrokerTile::new(id, name);
        tile.rating = Some(rating);
        tile.featured = featured;
        tile.description = format!("{name} is an example tile: a host lists its brokers here.");
        tile.facts = broker_facts(id);
        tile
    })
    .collect()
}

/// The comparison rows of a broker in the picker: the demo paper account's terms, and made-up
/// ones for the example tiles, one of which gives no leverage.
fn broker_facts(id: &str) -> Vec<(String, egui_charts::ui::trading::BrokerFact)> {
    use egui_charts::ui::trading::BrokerFact::{Supported, Value};
    let (commission, leverage, stocks, futures, crypto) = match id {
        egui_charts::ui::trading::BrokerTile::PAPER => ("None", Some("10:1"), true, true, true),
        "example-stocks" => ("0.005 USD per share", Some("4:1"), true, false, false),
        "example-futures" => ("1.25 USD per contract", Some("20:1"), false, true, false),
        _ => ("0.10% per trade", None, false, false, true),
    };
    let mut facts = vec![("Commission".to_owned(), Value(commission.to_owned()))];
    facts.extend(leverage.map(|leverage| ("Leverage".to_owned(), Value(leverage.to_owned()))));
    for (label, supported) in [
        ("Stocks", stocks),
        ("Futures", futures),
        ("Crypto", crypto),
        ("Bracket orders", id != "example-crypto"),
    ] {
        facts.push((label.to_owned(), Supported(supported)));
    }
    facts
}

/// Which right-hand side panel is open, if any.
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum SidePanel {
    ObjectTree,
    Alerts,
    DataWindow,
    Watchlist,
    TimeAndSales,
    Depth,
    OrderFlow,
    Highlights,
    Screener,
    Hotlists,
    MarketHeatmap,
    Calendar,
    News,
    Notifications,
    Chats,
    Ideas,
    Products,
}

impl SidePanel {
    /// Panels the demo renders behind a built-in rail entry, with the entry.
    const BUILT_IN: [(SidePanel, RightBarIcon); 12] = [
        (SidePanel::Watchlist, RightBarIcon::Watchlist),
        (SidePanel::Alerts, RightBarIcon::Alerts),
        (SidePanel::ObjectTree, RightBarIcon::ObjectTree),
        (SidePanel::Highlights, RightBarIcon::Highlights),
        (SidePanel::Screener, RightBarIcon::Screener),
        (SidePanel::Hotlists, RightBarIcon::Hotlists),
        (SidePanel::MarketHeatmap, RightBarIcon::MarketHeatmap),
        (SidePanel::Calendar, RightBarIcon::Calendar),
        (SidePanel::News, RightBarIcon::News),
        (SidePanel::Notifications, RightBarIcon::Notifications),
        (SidePanel::Chats, RightBarIcon::Chats),
        (SidePanel::Products, RightBarIcon::Products),
    ];

    /// Panels the demo renders behind a host rail entry, with the entry's id.
    const HOSTED: [(SidePanel, u64); 1] = [(SidePanel::TimeAndSales, 4)];

    fn icon(self) -> Option<RightBarIcon> {
        Self::BUILT_IN
            .iter()
            .find(|(panel, _)| *panel == self)
            .map(|(_, icon)| *icon)
    }

    fn from_icon(icon: RightBarIcon) -> Option<Self> {
        Self::BUILT_IN
            .iter()
            .find(|(_, entry)| *entry == icon)
            .map(|(panel, _)| *panel)
    }

    fn widget(self) -> Option<u64> {
        Self::HOSTED
            .iter()
            .find(|(panel, _)| *panel == self)
            .map(|(_, id)| *id)
    }

    fn from_widget(id: u64) -> Option<Self> {
        Self::HOSTED
            .iter()
            .find(|(_, widget)| *widget == id)
            .map(|(panel, _)| *panel)
    }
}

/// A products launcher entry.
fn product(id: &str, icon: egui_charts::icons::Icon, title: &str, description: &str) -> Product {
    let mut product = Product::new(id, icon, title);
    product.description = description.into();
    product
}

/// Export formats the demo can perform, with browser downloads on the web.
const EXPORT_FORMATS: &[ExportFormat] = &[
    ExportFormat::Png,
    ExportFormat::Svg,
    ExportFormat::Csv,
    ExportFormat::Clipboard,
];

/// Where a rendered chart image is delivered.
enum ScreenshotTarget {
    File(Option<std::path::PathBuf>),
    Clipboard,
    NewTab,
    Post { copy_link: bool },
}

/// App state surfaced to the toolbars via `ChartAppState`.
#[derive(Clone)]
pub struct DemoAppState {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub chart_type: ChartType,
    pub replay_active: bool,
}

impl ChartAppState for DemoAppState {
    fn active_symbol(&self) -> &str {
        &self.symbol
    }
    fn active_timeframe(&self) -> &Timeframe {
        &self.timeframe
    }
    fn chart_type(&self) -> ChartType {
        self.chart_type
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn is_replay_active(&self) -> bool {
        self.replay_active
    }
}

/// A preset waits for its interval and bars, then confirms the fit across a plot layout.
#[derive(Clone, Copy)]
enum RangeFit {
    Interval,
    Bars,
    Fitting,
    Confirming(u64),
}

/// The full demo application.
///
/// Members over 1 KiB are boxed: `new_at` assembles this struct on its stack before boxing it.
pub struct DemoApp {
    pub chart: Box<Chart>,
    pub(crate) layout: Box<ChartLayout>,
    layout_slot: usize,
    images: images::Images,
    extra_charts: Vec<DemoChart>,
    layout_tabs: Vec<LayoutTab>,
    layout_tab: usize,
    live: Arc<LiveFeed>,
    /// Whether an object tree colour picker is editing a drawing, as one undo step.
    colour_drag: bool,
    menus: Box<ChartMenus>,
    last_chart_click: Option<egui_charts::widget::ChartPointer>,
    /// The latest crosshair payload of the active chart, from which the data window reads.
    crosshair: Option<egui_charts::widget::ChartPointer>,
    /// The last pointer event stays readable while the pointer is over the host legend.
    pointer_readout: Option<egui_charts::widget::ChartPointer>,
    pub state: DemoAppState,
    pub bars: Vec<Bar>,
    loading_bars: bool,
    /// The preset and displayed interval, with its load and plot-fitting state.
    range_fit: Option<(DateRange, Timeframe, RangeFit)>,
    pub theme: Theme,
    /// No explicit choice follows the system appearance without saving its current value.
    theme_choice: Option<String>,
    formatting: formatting::Formatting,

    // Persistent toolbars.
    pub(crate) top_toolbar: Box<TopToolbar>,
    drawing_toolbar: DrawingToolbar,
    selection_toolbar: FloatingSelectionToolbar,
    pub(crate) timeframe_toolbar: TimeframeToolbar,
    widget_bar: WidgetBar,

    // Dialogs (each owns its open/closed state).
    indicator_dialog: Box<IndicatorDialog>,
    indicator_settings: Box<IndicatorSettingsDialog>,
    settings_dialog: Box<SettingsDialog>,
    series_settings: Box<egui_charts::ui::dialogs::SeriesSettingsDialog>,
    settings_state: Box<ChartSettingsState>,
    symbol_search: Box<SymbolSearchDialog>,
    compare_dialog: egui_charts::ui::top_toolbar::CompareSymbolsDialog,
    comparison_settings: Option<comparison::ComparisonSettings>,
    comparisons: Vec<(String, SeriesId, Arc<LiveFeed>)>,
    /// Spreads picked in symbol search, with feeds of their symbols.
    spreads: Vec<comparison::Spread>,
    storage: Arc<storage::DemoStorage>,
    workspace_recovery: storage::WorkspaceRecovery,
    settings: Box<egui_charts::services::SettingsStore>,
    /// Changes of the chart arrangement, which autosave counts with the charts' edits.
    grid_edits: u64,
    study_templates: TemplateDialog<StudyTemplateSet>,
    quick_search: egui_charts::ui_kit::command_palette::CommandPaletteHandler,
    sample_scene: Option<Box<commands::SampleScene>>,
    sample_study: study_editor::SampleStudyEditor,
    alert_dialog: Box<AlertDialog>,
    drawing_props: Box<DrawingPropertiesDialog>,
    keyboard_shortcuts: KeyboardShortcutsDialog,
    /// The key bindings the user saved in the shortcuts dialog, for the charts of every layout
    /// tab; `None` keeps each chart's defaults.
    shortcuts: Option<egui_charts::widget::ShortcutMap>,
    data_table: egui_charts::ui::dialogs::DataTableDialog,
    /// The table kept on the chart of a layout slot, whichever chart is active.
    fixed_data_table: (egui_charts::ui::dialogs::DataTableDialog, usize),
    interval_entry: Option<String>,
    interval_focus: bool,
    /// The saved-layout chooser of the Open layout shortcut is open.
    open_layout: bool,
    copied_drawing: Option<String>,
    // A study copied in this session, kept across symbols and intervals.
    copied_study: Option<String>,
    /// A copy across the layout waiting for the user to accept what its charts lack.
    layout_copy_prompt: Option<(LayoutCopy, String)>,
    export_dialog: ExportDialog,
    date_picker: DatePicker,

    // A finished host-run sample, independent of subsequent chart navigation.
    strategy_report: Box<egui_charts::ui::strategy_report::StrategyTester>,
    /// Whether the tester shows before it has a report, as after a strategy script is applied.
    strategy_open: bool,
    live_strategy: Option<strategy_live::LiveStrategy>,
    strategy_trade: Option<usize>,
    strategy_session: SessionType,
    /// The symbol and tick size the tester's price increment last followed.
    strategy_instrument: Option<(String, Option<f64>)>,

    // Right-hand side panel.
    object_tree: ObjectTreePanel,
    open_panel: Option<SidePanel>,
    data_window_expanded: bool,

    // Host-owned session-only alerts, shown through the library service connection.
    pub(crate) alerts: Box<AlertsPanel>,
    alert_host: Arc<crate::panel_alerts::DemoAlerts>,
    audio: audio::AudioPlayer,
    alert_evaluator: egui_charts::services::AlertEvaluator,
    alert_live: Arc<LiveFeed>,

    // Paper account kept across restarts, connected from the broker picker and traded through
    // the library's order ticket and account panel.
    pub(crate) trading: Box<egui_charts::ui::trading::Trading>,
    pub(crate) paper: Arc<egui_charts::services::PaperAccount>,
    broker_picker: egui_charts::ui::trading::BrokerPicker,
    account_panel: AccountPanelSize,
    /// The height of the account panel the user dragged its top edge to, kept across restarts.
    /// A saved height that is not a finite number restores as 320 points, and one below the
    /// drag's 42 point minimum as 42.
    account_panel_height: f32,
    // The account settings ask whether to reset the paper account.
    paper_reset: bool,
    // The account of the running replay session, built from the replay bar's start menu. The
    // account above is not traded, priced or reset while it exists.
    pub(crate) replay_paper: Option<Arc<egui_charts::services::PaperAccount>>,
    // Asks before a replay move leaves orders or trades of the replay account behind, and
    // holds that move until the answer.
    replay_prompt: egui_charts::ui::replay::ReplayTradingPrompt,
    replay_move: Option<replay::ReplayMove>,
    // Keep the report data off the controller's construction stack.
    pub(crate) replay_results: Box<egui_charts::ui::replay::ReplayResults>,
    // Watchlists and time and sales, fed by synthetic quotes and the live feed's executions.
    watchlist: WatchlistPanel,
    quotes: Arc<feeds::DemoQuotes>,
    // The watchlist file the user is choosing, until it is read.
    watchlist_import: Option<std::sync::mpsc::Receiver<WatchlistFile>>,
    /// Whether the wide watchlist window is open.
    watchlist_wide: bool,
    // Symbol search is open for the watchlist's add button: the pick joins the list.
    watchlist_search: bool,
    time_and_sales: TimeAndSalesPanel,
    depth: egui_charts::ui::trading::DepthPanel,
    order_flow: egui_charts::ui::widget_bar::OrderFlowPanel,
    // Below the watchlist: the chart symbol's synthetic details and notes kept in app storage.
    symbol_details: Box<SymbolDetailsPanel>,
    /// The chart symbol's financials, shown in place of the chart while they are open.
    financials: Option<(String, egui_charts::ui::financials::FinancialsPanel)>,
    /// The symbol details are shown in place of the chart, chosen with their Expand button.
    details_expanded: bool,
    // Synthetic highlights and headlines from the live feed.
    highlights: NewsPanel,
    news: NewsPanel,
    /// The simulation hour last requested by the headline panels.
    news_hour: Option<i64>,
    calendar: CalendarPanel,
    screener: ScreenerPanel,
    hotlists: HotlistsPanel,
    hotlist_favorites: Vec<String>,
    market_heatmap: egui_charts::ui::widget_bar::MarketHeatmapPanel,
    // The demo's event log: its messages and alert triggers.
    notifications: NotificationsPanel,
    notification_host: Arc<feeds::DemoNotifications>,
    chats: ChatsPanel,
    /// The chat authors' profiles, and the one whose card is open.
    profiles: ProfileDirectory,
    profile_open: Option<ProfileId>,
    ideas: ideas::DemoIdeas,
    products: ProductsPanel,

    // Replay engine and its floating transport. `None` until first toggled.
    recording_files: replay::RecordingFiles,
    replay: Option<ReplayController>,
    replay_updated: Option<chrono::DateTime<chrono::Utc>>,
    replay_applied: Option<chrono::DateTime<chrono::Utc>>,
    replay_start: ReplayStartPicker,
    replay_selecting: bool,
    replay_interval: Timeframe,
    replay_scope: ReplayScope,
    replay_session: Option<(ReplaySession, Timeframe)>,
    replay_slots: Vec<usize>,
    live_was_paused: bool,
    replay_control: FloatingReplayControl,
    replay_toolbar: bool,

    // Pending offscreen image options and delivery destination.
    screenshot_requested: Option<(
        ScreenshotTarget,
        egui_charts::widget::ImageExportOptions,
        ExportScope,
    )>,

    toasts: Toasts,
    /// Handle of the demo's price line, moved and removed from the chart menu.
    demo_level: Option<egui_charts::widget::PriceLineId>,
    /// Whether the showcase panels and overlays are enabled for this session.
    api_samples: bool,
    annotation_editor: bool,
    annotation_series: SeriesId,
    /// Set while the chart shows a visitor's own data.
    own_data: Option<owndata::app::OwnData>,
    /// The Data source dialog.
    data_source: Box<owndata::dialog::DataSourceDialog>,
}

fn sample_widgets() -> [WidgetBarWidget; 2] {
    [
        WidgetBarWidget::new(
            4,
            egui_charts::icons::icons::SETTINGS_EVENTS,
            "Time and sales",
            WidgetBarSection::Bottom,
        ),
        WidgetBarWidget::new(
            2,
            egui_charts::icons::icons::TEXT,
            "Session notes",
            WidgetBarSection::Bottom,
        ),
    ]
}

impl DemoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Box<Self> {
        Self::new_at(cc, chrono::Utc::now(), false)
    }

    pub(crate) fn new_at(
        cc: &eframe::CreationContext<'_>,
        now: chrono::DateTime<chrono::Utc>,
        paused: bool,
    ) -> Box<Self> {
        // Register egui's image loaders so the embedded SVG toolbar icons decode.
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.include_bytes(SAMPLE_LOGO, SAMPLE_LOGO_SVG);

        // Earlier releases saved active palettes without an explicit choice.
        let theme_choice = formatting::load_theme_choice(cc.storage);
        let theme =
            formatting::selected_theme(theme_choice.as_deref(), cc.egui_ctx.system_theme(), false);
        let timeframe = Timeframe::Min1;
        let symbol = "SAMPLE";
        let mut options = egui_charts::config::ChartOptions::default();
        options.time_scale.bar_spacing = 6.1;
        options.tooltip.on_long_press = true;
        options.tooltip.show_time = false;
        options.tooltip.font_size = 14.0;
        // A full-history range must also fit in a narrow window.
        options.time_scale.min_bar_spacing = 0.01;
        // The scale reserves half a slot and one pixel after the last centre.
        options.time_scale.right_offset_pixels = Some(65.0 - 6.1 * 0.5 - 1.0);
        options.time_scale.time_visible = true;
        options.time_scale.seconds_visible = true;
        let live = Arc::new(LiveFeed::new(symbol, now));
        live.set_paused(paused);
        let quotes = Arc::new(feeds::DemoQuotes::new(live.clone()));
        let drawing_tools = drawing_tools::registry();
        let mut chart = Box::new(
            egui_charts::ChartBuilder::new()
                .with_feed(live.clone())
                // The generated week of minutes, so the first reply holds the whole history.
                .with_initial_bars(10_080)
                .with_symbol(symbol)
                .with_timeframe(timeframe)
                .with_theme(theme.clone())
                .with_visible_candles(0)
                .with_chart_options(options)
                .with_drawing_registry(drawing_tools.clone())
                .build(),
        );
        chart.config_mut().volume_height_fraction = 0.25;
        chart.config_mut().volume_baseline_inset = 0.0;
        chart.config_mut().candle_width = 0.75;
        chart.config_mut().right_axis_width = 59.0;
        chart.config_mut().skip_background = true;
        size_by_atr(&mut chart);
        chart.set_event_visibility(
            cc.storage
                .and_then(|storage| eframe::get_value(storage, "event_visibility"))
                .unwrap_or_else(|| {
                    let mut visibility = egui_charts::model::EventVisibility::default();
                    visibility.show_dividends = false;
                    visibility.show_splits = false;
                    visibility.show_news = false;
                    visibility.show_ideas = false;
                    visibility
                }),
        );
        chart.set_order_book_view(book_view());
        chart.set_symbol(symbol);
        chart.set_timeframe(timeframe);
        let storage = Arc::new(storage::DemoStorage::restore(cc.storage));
        chart.set_storage(storage.clone());
        chart.set_post_source(Some(Arc::new(feeds::DemoPosts(live.clone()))));
        let notification_host: Arc<feeds::DemoNotifications> = Arc::default();

        let mut top_toolbar = Box::new(TopToolbar::new().with_config(
            egui_charts::ui::top_toolbar::TopToolbarConfig::default().with_height(38.0),
        ));
        top_toolbar.timeframe_selector.current = Timeframe::Min1;
        // A host's own call to action sits at the right end; the demo only reports it.
        top_toolbar.call_to_action = Some(("upgrade".into(), "Upgrade".into()));
        top_toolbar.builtin_indicator_templates = StudyTemplateSet::builtin()
            .into_iter()
            .map(|(name, _)| (name.to_owned(), name.to_owned()))
            .collect();
        let mut menus = Box::<ChartMenus>::default();
        menus.host_entries.extend([
            HostMenuEntry::new("research", "Open in research")
                .with_icon(&egui_charts::icons::icons::QUICK_SEARCH),
            HostMenuEntry::new("demo-level", "Move demo level here"),
            HostMenuEntry::new("demo-level-remove", "Remove demo level"),
            HostMenuEntry::new("demo-fixed-data-table", "Open fixed data table"),
        ]);
        let mut widget_bar = WidgetBar::default();
        widget_bar.config.width = 45.0;
        // The default rail contains the built-in panels supplied by the demo.
        widget_bar.icons = SidePanel::BUILT_IN
            .iter()
            .map(|(_, icon)| *icon)
            .chain([RightBarIcon::Help])
            .collect();
        let mut notes = String::new();
        widget_bar.register_panel(sample_widgets()[1].clone(), move |ui| {
            ui.label("Host panel · notes stay in this session");
            ui.add(
                egui::TextEdit::multiline(&mut notes)
                    .hint_text("Write your research notes…")
                    .desired_width(f32::INFINITY)
                    .desired_rows(12),
            );
        });
        widget_bar.widgets.clear();
        if let Some(preferences) = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "interval_preferences"))
        {
            top_toolbar.timeframe_selector.set_preferences(preferences);
        }
        if let Some(preferences) = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "chart_type_preferences"))
        {
            top_toolbar.chart_type_selector.set_preferences(preferences);
        }
        let mut timeframe_toolbar = TimeframeToolbar::default();
        timeframe_toolbar.config.active_bg = egui::Color32::TRANSPARENT;
        timeframe_toolbar.state.timeframe = timeframe;
        timeframe_toolbar.set_session(SessionType::Full24x7);

        let alert_live = Arc::new(live.fork());
        alert_live.set_view(SessionType::RTH, false);
        let sample_study = study_editor::SampleStudyEditor::new(storage.clone(), &cc.egui_ctx);
        let indicator_dialog = Box::new(
            IndicatorDialog::new().with_factory(sample_study.study_factory(theme.ui.accent)),
        );
        // Saved strategy scripts are listed from start-up.
        let strategy_report = Box::new(egui_charts::ui::strategy_report::StrategyTester::new(
            sample_study.strategy_factory(),
        ));
        // `Some(None)`: a saved account that no longer reads, or whose balance is not finite.
        let saved_paper = cc
            .storage
            .filter(|storage| storage.get_string("paper_account").is_some())
            .map(|storage| {
                eframe::get_value(storage, "paper_account")
                    .and_then(egui_charts::services::PaperAccount::restore)
            });
        let paper_unreadable = matches!(saved_paper, Some(None));
        // Keep the controller off callers' stacks, including tests that restart it.
        let mut app = Box::new(Self {
            chart,
            order_flow: egui_charts::ui::widget_bar::OrderFlowPanel::new(live.detectors()),
            live,
            layout: Box::default(),
            layout_slot: 0,
            images: images::Images::default(),
            extra_charts: Vec::new(),
            layout_tabs: vec![LayoutTab::unnamed()],
            layout_tab: 0,
            colour_drag: false,
            menus,
            last_chart_click: None,
            crosshair: None,
            pointer_readout: None,
            state: DemoAppState {
                symbol: symbol.into(),
                timeframe,
                chart_type: ChartType::Candles,
                replay_active: false,
            },
            bars: Vec::new(),
            loading_bars: false,
            range_fit: None,
            theme,
            theme_choice,
            formatting: cc
                .storage
                .and_then(|storage| eframe::get_value(storage, "demo_formatting"))
                .unwrap_or_default(),
            top_toolbar,
            drawing_toolbar: {
                let mut toolbar = DrawingToolbar::default();
                toolbar.state.preferences = cc
                    .storage
                    .and_then(|storage| eframe::get_value(storage, "drawing_toolbar_preferences"))
                    .unwrap_or_default();
                toolbar.registry = drawing_tools;
                toolbar
            },
            selection_toolbar: FloatingSelectionToolbar::new(),
            timeframe_toolbar,
            widget_bar,
            indicator_dialog,
            indicator_settings: Box::default(),
            series_settings: Box::default(),
            settings_dialog: Box::new({
                let mut dialog = SettingsDialog::new();
                dialog.execution_sounds = DEMO_SOUNDS.map(str::to_owned).to_vec();
                dialog
            }),
            settings_state: Box::default(),
            symbol_search: Box::new(SymbolSearchDialog::new()),
            compare_dialog: egui_charts::ui::top_toolbar::CompareSymbolsDialog::new(),
            comparison_settings: None,
            comparisons: Vec::new(),
            spreads: Vec::new(),
            settings: Box::new(egui_charts::services::SettingsStore::new(storage.clone())),
            own_data: None,
            data_source: Box::new(owndata::dialog::DataSourceDialog::new(
                storage.clone(),
                cc.storage.is_some(),
            )),
            storage,
            workspace_recovery: storage::WorkspaceRecovery::default(),
            grid_edits: 0,
            study_templates: TemplateDialog::new(StoreKind::StudyTemplates),
            // Filled from the chart's bindings each time it opens.
            quick_search: egui_charts::ui_kit::command_palette::CommandPaletteHandler::new(
                Vec::new(),
            ),
            sample_scene: None,
            sample_study,
            alert_dialog: Box::new(AlertDialog::new()),
            drawing_props: Box::new(DrawingPropertiesDialog::new()),
            keyboard_shortcuts: {
                let mut dialog = KeyboardShortcutsDialog::new();
                dialog.editable = true;
                dialog
            },
            // Read with the other user settings.
            shortcuts: None,
            data_table: egui_charts::ui::dialogs::DataTableDialog::new(),
            fixed_data_table: (
                egui_charts::ui::dialogs::DataTableDialog::new()
                    .with_id(egui::Id::unique("fixed_data_table"))
                    .with_fixed_symbol(),
                0,
            ),
            interval_entry: None,
            interval_focus: false,
            open_layout: false,
            copied_drawing: None,
            copied_study: None,
            layout_copy_prompt: None,
            export_dialog: ExportDialog::new().with_formats(EXPORT_FORMATS),
            date_picker: DatePicker::new(),
            strategy_report,
            strategy_open: false,
            live_strategy: None,
            strategy_trade: None,
            strategy_session: SessionType::Full24x7,
            strategy_instrument: None,
            object_tree: ObjectTreePanel::new(),
            open_panel: cc
                .storage
                .and_then(|storage| eframe::get_value(storage, "open_panel"))
                .flatten()
                .filter(|panel| *panel != SidePanel::TimeAndSales),
            data_window_expanded: true,
            alerts: Box::new({
                let mut panel = AlertsPanel::default();
                if let Some(options) = cc
                    .storage
                    .and_then(|storage| eframe::get_value(storage, "alert_list_options"))
                {
                    panel.set_options(options);
                }
                panel
            }),
            alert_host: {
                let mut alerts: Vec<Alert> = cc
                    .storage
                    .and_then(|storage| eframe::get_value(storage, "alerts"))
                    .unwrap_or_default();
                for alert in &mut alerts {
                    alert.symbol = DemoSymbol::current(&alert.symbol);
                }
                let mut triggers: Vec<egui_charts::model::AlertTrigger> = cc
                    .storage
                    .and_then(|storage| eframe::get_value(storage, "alert_triggers"))
                    .unwrap_or_default();
                for trigger in &mut triggers {
                    trigger.symbol = DemoSymbol::current(&trigger.symbol);
                }
                let notifications = notification_host.clone();
                Arc::new(crate::panel_alerts::DemoAlerts::new(
                    alerts,
                    triggers,
                    move |symbol| {
                        notifications.notify(format!("Alert triggered: {symbol}"), Some(symbol));
                    },
                ))
            },
            audio: Default::default(),
            alert_evaluator: egui_charts::services::AlertEvaluator::new(alert_live.clone()),
            alert_live,
            trading: Default::default(),
            paper: Arc::new(saved_paper.flatten().unwrap_or_else(|| {
                egui_charts::services::PaperAccount::new("USD", 100_000.0).leverage(10.0)
            })),
            broker_picker: {
                let mut picker = egui_charts::ui::trading::BrokerPicker::new(example_brokers());
                picker.tiles[0].facts = broker_facts(egui_charts::ui::trading::BrokerTile::PAPER);
                picker
            },
            account_panel: AccountPanelSize::Normal,
            account_panel_height: cc
                .storage
                .and_then(|storage| eframe::get_value::<f32>(storage, "account_panel_height"))
                .filter(|height| height.is_finite())
                .map_or(320.0, |height| height.max(42.0)),
            paper_reset: false,
            replay_paper: None,
            replay_prompt: Default::default(),
            replay_move: None,
            replay_results: Default::default(),
            watchlist: WatchlistPanel::new(
                cc.storage
                    .and_then(|storage| eframe::get_value(storage, "watchlists"))
                    .map(|mut lists: Vec<Watchlist>| {
                        for item in lists.iter_mut().flat_map(|list| &mut list.items) {
                            if let egui_charts::model::WatchlistItem::Symbol { symbol, .. } = item {
                                *symbol = DemoSymbol::current(symbol);
                            }
                        }
                        lists
                    })
                    .unwrap_or_else(|| {
                        vec![Watchlist::from_text(
                            "Watchlist",
                            "###Crypto,TOKENA,TOKENB,###Stocks,STOCKA,STOCKB,STOCKC,STOCKD,\
                             ###Indices and forex,INDEXA,FOREXA,SAMPLE",
                        )]
                    }),
            ),
            quotes,
            watchlist_import: None,
            watchlist_wide: false,
            watchlist_search: false,
            time_and_sales: TimeAndSalesPanel::default(),
            depth: cc
                .storage
                .and_then(|storage| eframe::get_value(storage, "depth_panel"))
                .unwrap_or_default(),
            symbol_details: Box::new(SymbolDetailsPanel::new()),
            financials: None,
            details_expanded: false,
            highlights: NewsPanel::highlights(),
            news: NewsPanel::new(),
            news_hour: None,
            calendar: CalendarPanel::new(),
            screener: ScreenerPanel::new(feeds::screener_sets()),
            hotlists: HotlistsPanel::new(feeds::HOTLISTS.map(String::from).to_vec()),
            hotlist_favorites: Vec::new(),
            market_heatmap: {
                let mut panel = egui_charts::ui::widget_bar::MarketHeatmapPanel::default();
                panel.universes = feeds::heatmap_universes();
                panel.metrics = feeds::heatmap_metrics();
                panel
            },
            notifications: NotificationsPanel::new(),
            notification_host,
            chats: ChatsPanel::new(),
            profiles: ProfileDirectory::new(),
            profile_open: None,
            ideas: ideas::DemoIdeas::new(),
            products: ProductsPanel::new(vec![
                product(
                    "screener",
                    egui_charts::icons::icons::WIDGET_BAR_SCREENER,
                    "Screener",
                    "Filter the catalog symbols",
                ),
                product(
                    "calendar",
                    egui_charts::icons::icons::WIDGET_BAR_CALENDAR,
                    "Calendars",
                    "Economic and corporate events",
                ),
                product(
                    "news",
                    egui_charts::icons::icons::WIDGET_BAR_NEWS,
                    "News",
                    "Headlines for the chart's symbol",
                ),
                product(
                    "shortcuts",
                    egui_charts::icons::icons::WIDGET_BAR_HELP,
                    "Keyboard shortcuts",
                    "Every key the chart understands",
                ),
            ]),
            recording_files: Default::default(),
            replay: None,
            replay_updated: None,
            replay_applied: None,
            replay_start: ReplayStartPicker::default(),
            replay_selecting: false,
            replay_interval: Timeframe::Hour1,
            replay_scope: ReplayScope::AllCharts,
            replay_session: cc.storage.and_then(|storage| {
                let (mut session, interval): (ReplaySession, Timeframe) =
                    eframe::get_value::<Option<_>>(storage, "replay_session")??;
                // Older demo sessions replayed all charts.
                if !storage.get_string("replay_session")?.contains("scope:") {
                    session.scope = ReplayScope::AllCharts;
                }
                session.symbol = DemoSymbol::current(&session.symbol);
                for chart in &mut session.charts {
                    chart.symbol = DemoSymbol::current(&chart.symbol);
                }
                Some((session, interval))
            }),
            replay_slots: Vec::new(),
            live_was_paused: false,
            replay_control: FloatingReplayControl::new(),
            replay_toolbar: false,
            screenshot_requested: None,
            toasts: Toasts::new(),
            demo_level: None,
            api_samples: false,
            annotation_editor: false,
            annotation_series: SeriesId::MAIN,
        });
        // The demo applies the requests of the tab strip's controls.
        app.layout.tab_controls = true;
        app.settings_dialog
            .templates
            .set_storage(app.storage.clone());
        app.study_templates
            .templates
            .set_storage(app.storage.clone());
        app.apply_formatting();
        app.drawing_props.templates.set_storage(app.storage.clone());
        let search: Arc<dyn egui_charts::services::SymbolSearch> =
            Arc::new(feeds::search_catalog(&mut app.symbol_search));
        app.symbol_search.set_search_service(Some(search.clone()));
        app.compare_dialog.set_search_service(Some(search));
        app.symbol_search.set_storage(app.storage.clone());
        app.compare_dialog.set_storage(app.storage.clone());
        let mut sources = Vec::new();
        for exchange in DemoSymbol::symbols()
            .into_iter()
            .filter_map(|row| row.exchange)
        {
            if !sources.contains(&exchange) {
                sources.push(exchange);
            }
        }
        app.symbol_search.set_sources(sources);
        if let Some(row) = DemoSymbol::symbols()
            .into_iter()
            .find(|row| row.symbol == symbol)
        {
            app.symbol_search.record_symbol(&row);
        }
        app.alerts.set_service(app.alert_host.clone());
        app.alerts.set_popup_actions(true);
        app.stop_restored_strategy_alerts();
        app.alerts.set_mark("SAMPLE", SAMPLE_LOGO.into());
        app.top_toolbar.state.can_trade = true;
        app.top_toolbar.state.can_publish_ideas = true;
        app.top_toolbar.state.can_publish_image = true;
        app.watchlist.set_selected(symbol);
        if let Some(active) = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "watchlist_active"))
        {
            app.watchlist.set_active_list(active);
        }
        if let Some(flags) = cc.storage.and_then(|storage| {
            eframe::get_value::<Vec<(String, egui_charts::model::WatchlistFlag)>>(
                storage,
                "watchlist_unlisted_flags",
            )
        }) {
            app.watchlist.set_unlisted_flags(
                flags
                    .into_iter()
                    .map(|(symbol, flag)| (DemoSymbol::current(&symbol), flag)),
            );
        }
        app.watchlist.set_service(app.quotes.clone());
        let sharing = Arc::new(feeds::DemoSharing::default());
        app.watchlist.set_sharing(sharing.clone());
        app.watchlist.set_mark("SAMPLE", SAMPLE_LOGO.into());
        app.alert_evaluator.set_storage(app.storage.clone());
        app.alert_dialog.set_storage(app.alert_host.clone());
        app.symbol_details.set_feed(app.live.clone());
        app.symbol_details.set_storage(app.storage.clone());
        app.symbol_details
            .set_detail_sections(egui_charts::ui::widget_bar::DetailSection::ALL);
        app.watchlist.set_add_note_visible(true);
        app.menus.set_add_note_visible(true);
        if let Some(hidden) = cc
            .storage
            .and_then(|storage| eframe::get_value::<Vec<String>>(storage, "symbol_details_hidden"))
        {
            app.symbol_details.set_hidden_sections(hidden);
        }
        app.top_toolbar.layout_menu.set_storage(app.storage.clone());
        app.top_toolbar.layout_menu.set_sharing(sharing);
        app.highlights.set_feed(app.live.clone());
        app.news.set_feed(app.live.clone());
        for panel in [&mut app.news, &mut app.highlights] {
            panel.set_storage(app.storage.clone());
            panel.set_countries(feeds::COUNTRIES.map(String::from));
            panel.set_topics(["Markets".into()]);
            panel.set_providers(["Demo Wire".into()]);
        }
        app.calendar.set_feed(app.live.clone());
        app.screener.set_feed(app.live.clone());
        app.hotlists
            .set_refresh_interval(Some(std::time::Duration::from_secs(60)));
        app.hotlists.set_exchanges(feeds::exchanges());
        app.hotlists
            .set_countries(feeds::COUNTRIES.map(String::from));
        app.hotlists.set_feed(app.live.clone());
        app.notifications.set_service(app.notification_host.clone());
        app.chats.set_service(Arc::new(feeds::DemoChats::new(
            chrono::Utc::now(),
            app.notification_host.clone(),
        )));
        app.chats.set_profiles(true);
        app.profiles
            .set_service(Arc::new(feeds::DemoProfiles::new()));
        app.calendar
            .set_countries(feeds::COUNTRIES.iter().map(|code| code.to_string()));
        app.calendar
            .set_categories(feeds::CATEGORIES.map(String::from));
        app.apply_session();
        app.chart.drawings_mut().tool_defaults = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "drawing_defaults"))
            .unwrap_or_default();
        *app.settings_state = ChartSettingsState::from_chart(&app.chart);
        // The demo's storage answers at once, so the settings apply before the first frame.
        let current = app.user_settings();
        let recover_workspace = if let Some(settings) = app.settings.poll(&cc.egui_ctx, &current) {
            let unnamed = settings.layout.is_none();
            app.apply_user_settings(&cc.egui_ctx, settings);
            unnamed
        } else {
            true
        };
        // Keep the old key as a fallback for settings saved before cursor favourites.
        if app.drawing_toolbar.state.cursor_favorites.is_empty() {
            app.drawing_toolbar.state.cursor_favorites = cc
                .storage
                .and_then(|storage| eframe::get_value(storage, "cursor_favorites"))
                .unwrap_or_default();
        }
        if recover_workspace {
            app.restore_workspace();
        }
        if let Some(settings) = cc.storage.and_then(|storage| {
            eframe::get_value::<egui_charts::services::DetectorSettings>(storage, "detectors")
        }) {
            match settings.validate() {
                Ok(()) => app.live.detectors().set_settings(settings),
                Err(error) => app
                    .toasts
                    .error(format!("Could not restore detectors: {error}")),
            }
        }
        app.trading.presets = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "order_presets"))
            .unwrap_or_default();
        if cc
            .storage
            .and_then(|storage| eframe::get_value::<bool>(storage, "paper_connected"))
            == Some(true)
        {
            app.connect_paper();
        }
        if paper_unreadable {
            app.toasts.error("Could not restore the paper account");
        }
        app
    }

    fn play_sound(&mut self, ctx: &egui::Context, name: &str, volume: f32, activate: bool) {
        if let Err(error) = self.audio.play(name, volume, activate, ctx) {
            self.toasts.error(error);
        }
    }

    /// Surface a short-lived, honest status message in the UI.
    fn notify(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.notification_host.notify(msg.clone(), None);
        self.toasts.info(msg);
    }

    pub(super) fn user_action<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        self.chart.begin_user_action();
        let result = edit(self);
        self.chart.end_user_action();
        result
    }

    fn sync_chart_history(&mut self) {
        if !self.chart.take_history_changed() {
            return;
        }
        self.range_fit = None;
        self.state.chart_type = self.chart.chart_type();
        let clock = self.timeframe_toolbar.state.timezone.clone();
        self.timeframe_toolbar
            .state
            .set_display_timezone(&self.chart.display_timezone());
        let clock_changed = self.timeframe_toolbar.state.timezone != clock;
        // A bar reload must keep the session restored by history.
        let extended = self.chart.extended_hours();
        self.follow_chart_symbol(clock_changed);
        self.chart.set_extended_hours(extended);
        self.follow_chart_session(false);
    }

    /// Loads the bars of the symbol and interval the chart shows after it changed them itself.
    pub(super) fn follow_chart_symbol(&mut self, clock_changed: bool) {
        let symbol_changed = self.state.symbol != self.chart.symbol();
        if symbol_changed {
            self.sync_symbol();
            self.stop_replay();
        }
        if let Some(tf) = self.chart.timeframe()
            && (tf != self.state.timeframe || clock_changed || symbol_changed)
        {
            // Reloads the bars, for the restored symbol too.
            self.switch_timeframe(tf);
        }
    }

    // --- data helpers ------------------------------------------------------

    fn switch_timeframe(&mut self, tf: Timeframe) {
        if let Some(own) = &self.own_data {
            if own.profile.service_interval(tf).is_none() {
                self.toasts
                    .error(format!("The data source has no interval {tf}"));
                return;
            }
            self.copied_drawing = None;
            self.state.timeframe = tf;
            self.chart.set_timeframe(tf);
            self.timeframe_toolbar.state.timeframe = tf;
            self.refresh_interval_control();
            return;
        }
        self.copied_drawing = None;
        self.state.timeframe = tf;
        self.chart.set_timeframe(tf);
        if matches!(tf, Timeframe::Range(_)) {
            self.state.chart_type = ChartType::Range;
            self.chart.set_chart_type(ChartType::Range);
        }
        self.regenerate_bars();
        self.timeframe_toolbar.state.timeframe = tf;
        self.refresh_interval_control();
        self.follow_replay_timeframe();
    }

    pub(crate) fn switch_symbol(&mut self, symbol: &str) {
        if let Some(own) = &self.own_data {
            // The profile lists what the visitor chose to offer. Nothing else reaches the service.
            if !own.profile.symbols.iter().any(|listed| listed == symbol) {
                self.toasts
                    .error(format!("{symbol} is not a symbol of this data source"));
                return;
            }
            if symbol != self.chart.symbol() {
                self.user_action(|app| {
                    app.chart.set_symbol(symbol);
                    app.sync_symbol();
                });
            }
            return;
        }
        if let Some(row) = DemoSymbol::symbols()
            .into_iter()
            .find(|row| row.symbol == symbol)
        {
            self.symbol_search.record_symbol(&row);
        }
        let symbol = DemoSymbol::find(symbol).symbol;
        // A reload would clear the bars and the heatmap and stop a replay of the shown symbol.
        if symbol == self.chart.symbol() {
            return;
        }
        self.user_action(|app| {
            app.chart.set_symbol(symbol);
            app.sync_symbol();
            app.regenerate_bars();
            // A symbol change invalidates an in-flight replay.
            app.stop_replay();
        });
    }

    fn sync_symbol(&mut self) {
        self.copied_drawing = None;
        self.time_and_sales.clear();
        if self.own_data.is_some() {
            self.state.symbol = self.chart.symbol().to_owned();
            return;
        }
        let spec = DemoSymbol::find(self.chart.symbol());
        self.live.set_symbol(spec.symbol);
        self.watchlist.set_selected(spec.symbol);
        self.timeframe_toolbar.state.session_menu_open = false;
        self.state.symbol = spec.symbol.to_string();
        for control in [
            egui_charts::ui::Control::Session,
            egui_charts::ui::Control::DividendAdjustment,
        ] {
            self.timeframe_toolbar
                .set_control_visible(control, DemoSymbol::has_hours(spec.symbol));
        }
        self.timeframe_toolbar
            .set_session(opening_session(spec.symbol));
        self.sync_depth_history();
    }

    fn regenerate_bars(&mut self) {
        self.apply_session();
        self.size_range_interval();
    }

    /// A range interval names the size of its bars, in ticks of the instrument. The other
    /// price-based sizes stay as the chart settings have them.
    fn size_range_interval(&mut self) {
        let Timeframe::Range(ticks) = self.state.timeframe else {
            return;
        };
        let size = ticks.get() as f64 * self.live.instrument().tick_size.unwrap_or(0.01);
        let params = self.chart.chart_type_params();
        if params.range_size != size || params.range_size_method != BoxSizeMethod::Fixed {
            let mut params = params.clone();
            params.range_size = size;
            params.range_size_method = BoxSizeMethod::Fixed;
            self.chart.set_chart_type_params(params);
        }
    }

    /// Follows a session chosen by the chart settings or undo with the toolbar and demo feed,
    /// reloading the bars when it changed or `reload` asks for it.
    fn follow_chart_session(&mut self, reload: bool) {
        let extended = self.chart.extended_hours();
        let changed = self.own_data.is_none()
            && DemoSymbol::has_hours(&self.state.symbol)
            && extended != (self.timeframe_toolbar.state.session_type != SessionType::RTH);
        if changed {
            self.timeframe_toolbar.set_session(if extended {
                SessionType::ETH
            } else {
                SessionType::RTH
            });
        }
        if changed || reload {
            self.stop_replay();
            self.apply_session();
        }
    }

    fn apply_session(&mut self) {
        if self.own_data.is_some() {
            return;
        }
        // Recorded prices and metadata already have their final session and price basis.
        if self.chart.replay_driver().is_some() {
            return;
        }
        load_view(
            &mut self.chart,
            &self.live,
            self.timeframe_toolbar.state.session_type,
            self.timeframe_toolbar.state.adjust_for_dividends,
        );
        self.loading_bars = true;
        // New prices may keep the bar times, such as after a dividend adjustment: rate them anew.
        self.symbol_details.set_bars(self.state.timeframe, &[]);
        self.bars = self.live.snapshot(
            self.state.timeframe,
            self.chart.bar_clock(),
            self.chart.extended_hours(),
        );
        self.chart.set_data(SeriesId::MAIN, self.bars.clone());
        self.chart.set_quote(None);
        self.chart.set_events(Vec::new());
        self.chart.set_mark_source(None);
        self.chart.set_signal_source(None);
        if !self.live.loading() {
            self.loading_bars = false;
            self.annotate();
        }
    }

    // --- alerts ------------------------------------------------------------

    fn open_alert(&mut self, price: Option<f64>) {
        if self.synthetic_only("Alerts") {
            return;
        }
        self.list_alert_sources();
        let price = price
            .or_else(|| self.chart.bars(SeriesId::MAIN).last().map(|b| b.close))
            .unwrap_or(0.0);
        // The demo has no account service; a host supplies the zone of the user's account.
        self.alert_dialog.account_timezone =
            egui_charts::model::TimezoneId::named("America/New_York");
        self.alert_dialog.open(self.state.symbol.clone(), price);
        self.alert_dialog.alert.timeframe = Some(self.state.timeframe);
        self.chart.prepare_alert(&mut self.alert_dialog.alert);
    }

    /// Offers study outputs, drawing levels and live strategy runs as alert sources.
    fn list_alert_sources(&mut self) {
        let inputs = self.chart.alert_inputs();
        self.alert_dialog.sources = inputs.sources.clone();
        self.alerts.set_sources(inputs.sources.clone());
        self.alerts.set_inputs(inputs, self.chart.timeframe());
        self.alert_dialog.watchlists = self
            .watchlist
            .lists()
            .iter()
            .map(|list| (list.name.clone(), list.name.clone()))
            .collect();
        self.alert_dialog.footprint_conditions = true;
        self.alert_dialog.strategy_runs = self
            .live_strategy
            .iter()
            .map(|live| (live.id.clone(), live.run.name.clone()))
            .collect();
    }

    fn quick_price_alert(&mut self, symbol: String, price: f64) {
        if self.synthetic_only("Alerts") {
            return;
        }
        let mut alert = Alert::new(symbol, AlertCondition::Crossing(price));
        alert.timeframe = Some(self.state.timeframe);
        self.chart.prepare_alert(&mut alert);
        self.alerts.save(alert);
    }

    fn monitor_alerts(&mut self, ctx: &egui::Context) {
        if self.own_data.is_some() {
            return;
        }
        let now = chrono::Utc::now();
        let paused = if self.replay.is_some() {
            self.live_was_paused
        } else {
            self.live.paused()
        };
        self.alert_live.set_paused(paused);
        let delay = self.alert_live.advance(now);
        let engine = self.alert_host.engine();
        for list in self.watchlist.lists() {
            engine.set_watchlist(&list.name, list.symbols().map(str::to_owned).collect());
        }
        let live: Vec<_> = std::iter::once((&self.chart, &self.live))
            .chain(
                self.extra_charts
                    .iter()
                    .map(|slot| (&slot.chart, &slot.live)),
            )
            .filter(|(_, feed)| !feed.loading())
            .filter_map(|(chart, _)| chart.alert_data())
            .collect();
        for error in
            self.alert_evaluator
                .poll(engine, self.indicator_dialog.factory(), &live, ctx, now)
        {
            self.toasts.error(format!("Alert data: {error}"));
        }
        // Footprint alerts follow the detectors whose rule the active chart sets.
        let footprint = self.live.detectors().take_footprint_events();
        if self.replay.is_none() {
            engine.footprint_events(&footprint, now);
        }
        if engine
            .alerts()
            .iter()
            .any(|alert| alert.status == egui_charts::model::AlertStatus::Active)
        {
            ctx.request_repaint_after(delay.min(std::time::Duration::from_secs(1)));
        }
    }

    /// Hands the alerts panel the chosen sound, zone and price, delivers the triggers of the
    /// host's engine and opens what a trigger popup asked for.
    fn poll_alerts(&mut self, ctx: &egui::Context) {
        self.alerts
            .set_sound(Some(self.settings_state.alerts.sound.clone()));
        self.alerts.set_timezone(
            self.timeframe_toolbar
                .state
                .timezone
                .parse()
                .unwrap_or_default(),
        );
        self.alerts.set_price(
            self.chart.symbol(),
            self.chart
                .bars(SeriesId::MAIN)
                .iter()
                .rev()
                .find(|bar| !bar.is_whitespace())
                .map(|bar| bar.close),
            self.chart
                .instrument()
                .and_then(|instrument| instrument.tick_size),
        );
        self.alerts.poll(ctx, &mut self.toasts);
        self.monitor_alerts(ctx);
        self.alerts.poll(ctx, &mut self.toasts);
        match self.alerts.take_popup_action() {
            AlertPanelAction::SelectSymbol(symbol) => self.open_catalog_symbol(&symbol),
            AlertPanelAction::EditAlert(alert) => self.edit_alert(*alert),
            _ => {}
        }
    }

    // --- selection / context helpers --------------------------------------

    fn chart_time_zones(&self) -> Vec<egui_charts::model::TimezoneId> {
        let mut zones = default_time_zones();
        // On own data the zone of the profile is offered too, to return to it.
        if let Some(own) = &self.own_data
            && !zones.contains(&own.profile.time_zone)
        {
            zones.insert(0, own.profile.time_zone.clone());
        }
        zones
    }

    /// The chart settings as the dialog shows them, with the settings the demo keeps itself.
    fn chart_settings_state(&self) -> ChartSettingsState {
        let mut state = ChartSettingsState::from_chart(&self.chart);
        state.alerts = self.settings_state.alerts.clone();
        state.trading = self.trading.display.clone();
        state.adjust_for_dividends = self.timeframe_toolbar.state.adjust_for_dividends;
        state
    }

    fn open_chart_settings_dialog(&mut self) {
        *self.settings_state = self.chart_settings_state();
        self.settings_dialog
            .offer_apply_to_all(self.layout_has_other_charts());
        // Only the synthetic contracts have futures history to reload.
        self.settings_dialog.offer_futures_data(
            self.own_data.is_none() && DemoSymbol::find(self.chart.symbol()).exchange == "Futures",
        );
        self.settings_dialog.set_depth_history_config(
            self.chart
                .depth_history(SeriesId::MAIN)
                .map(DepthHistory::config),
        );
        self.settings_dialog.open((*self.settings_state).clone());
    }

    fn open_drawing_props_for(&mut self, drawing_id: usize) {
        self.drawing_props
            .open_for_chart(&self.chart, DrawingId(drawing_id));
    }

    fn open_indicator_settings(&mut self, id: SeriesId) {
        if id == SeriesId::MAIN {
            self.open_chart_settings_dialog();
            return;
        }
        self.indicator_settings
            .set_time_zones(self.chart_time_zones());
        if !self.indicator_settings.open_for_chart(&self.chart, id)
            && self.chart.series().any(|(series, _)| series == id)
        {
            self.open_comparison_settings(id);
        }
    }

    // --- dialog wiring -----------------------------------------------------

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.show_quick_search(ctx) {
            self.execute_search_command(ctx, &id);
        }
        // The metrics of the study dialog are those of the synthetic feed.
        let metrics: Option<Arc<dyn egui_charts::data::Datafeed>> = match self.own_data {
            Some(_) => None,
            None => Some(self.live.clone()),
        };
        self.indicator_dialog
            .set_metric_source(metrics, self.chart.symbol());
        // Indicators: add the study the dialog created.
        if let IndicatorDialogAction::AddIndicator(study) =
            self.indicator_dialog.show(ctx, &self.theme)
        {
            self.chart.begin_user_action();
            self.chart.add_indicator(study);
            self.chart.end_user_action();
        }

        if let Some(edit) =
            self.indicator_settings
                .show_preview_for_chart(ctx, &self.theme, &mut self.chart)
        {
            self.chart.begin_user_action();
            let result = self
                .chart
                .indicator_mut(edit.id)
                .filter(|study| study.user_edit_enabled())
                .ok_or_else(|| "Study is unavailable for editing".to_owned())
                .and_then(|study| {
                    study
                        .apply_settings(&edit.inputs, edit.settings)
                        .map_err(|error| format!("Study settings not applied: {error}"))
                });
            if let Err(error) = result {
                self.notify(error);
            }
            self.chart.end_user_action();
        }

        self.show_comparison_settings(ctx);
        self.show_templates(ctx);

        // Chart settings.
        match self.settings_dialog.show(ctx, &self.theme) {
            SettingsAction::Apply(state) => self.apply_chart_settings(*state),
            SettingsAction::ApplyToAll(state) => {
                self.request_layout_copy(LayoutCopy::Settings(state))
            }
            SettingsAction::PreviewSound { name, volume } => {
                self.play_sound(ctx, &name, volume, true)
            }
            SettingsAction::ClearDepthHistory => self.chart.clear_depth_history(),
            SettingsAction::Cancel => {}
            _ => {}
        }
        if let egui_charts::ui::dialogs::SeriesSettingsAction::Apply(_, settings) =
            self.series_settings.show(ctx)
        {
            self.user_action(|app| app.chart.apply_series_settings(&settings));
        }
        self.show_layout_copy_prompt(ctx);

        // On own data the search offers the symbols of the profile and no others.
        let symbols = match &self.own_data {
            Some(own) => own
                .profile
                .symbols
                .iter()
                .map(|symbol| egui_charts::model::Instrument::new(symbol.as_str()))
                .collect(),
            None => DemoSymbol::symbols(),
        };
        self.sync_comparisons();
        let action = self.compare_dialog.show(ctx, &symbols);
        self.handle_comparison(action);
        match self.data_table.show(ctx, &self.chart) {
            egui_charts::ui::dialogs::DataTableAction::Export(csv) => {
                self.save_text("chart_data.csv", &csv);
            }
            egui_charts::ui::dialogs::DataTableAction::SearchSymbol(query) => {
                self.watchlist_search = false;
                if query.is_empty() {
                    self.symbol_search
                        .open_with_selected_query(self.chart.symbol().to_owned());
                } else {
                    self.symbol_search.open_with_query(query);
                }
            }
            _ => {}
        }
        let (table, slot) = &mut self.fixed_data_table;
        let chart = if *slot == self.layout_slot {
            Some(&*self.chart)
        } else {
            self.extra_charts
                .get(*slot - usize::from(*slot > self.layout_slot))
                .map(|extra| &*extra.chart)
        };
        // A layout that no longer holds the table's chart closes it.
        match chart.map(|chart| table.show(ctx, chart)) {
            Some(egui_charts::ui::dialogs::DataTableAction::Export(csv)) => {
                self.save_text("chart_data.csv", &csv);
            }
            None => table.close(),
            _ => {}
        }
        if let Some(picked) = self.symbol_search.show(ctx, &symbols) {
            if self.watchlist_search {
                self.watchlist.add_symbol(&picked.symbol);
            } else if picked.symbol_type == Some(egui_charts::model::SymbolType::Spread) {
                self.add_spread(&picked.symbol);
            } else {
                self.switch_symbol(&picked.symbol);
            }
        }
        self.watchlist_search &= self.symbol_search.is_open();

        // Alert creation.
        match self.alert_dialog.show(ctx) {
            AlertDialogAction::Create(mut alert) => {
                self.chart.prepare_alert(&mut alert);
                self.alerts.save(*alert);
                self.strategy_alert_closed(true);
            }
            AlertDialogAction::Cancel => self.strategy_alert_closed(false),
            AlertDialogAction::None => {}
            _ => {}
        }

        // Drawing properties.
        match self.drawing_props.show_and_apply(ctx, &mut self.chart) {
            DrawingPropertiesAction::Apply(..) => {}
            DrawingPropertiesAction::Delete(id) => {
                self.chart
                    .user_action(|chart| chart.drawings_mut().delete_drawing(id.0));
            }
            DrawingPropertiesAction::Clone(id) => self.clone_drawing(id.0),
            DrawingPropertiesAction::Cancel | DrawingPropertiesAction::None => {}
            _ => {}
        }

        // Keyboard shortcuts help, whose Save gives the edited bindings to every chart.
        if let KeyboardShortcutsAction::Apply(map) =
            self.keyboard_shortcuts.show_for_chart(ctx, &self.chart)
        {
            self.shortcuts = Some(map);
        }
        self.show_interval_entry(ctx);
        self.show_open_layout(ctx);
        self.show_idea(ctx);

        // Export dialog (screenshot / CSV).
        match self.export_dialog.show(ctx) {
            ExportResponse::Export { config, save_path } => self.run_export(ctx, config, save_path),
            ExportResponse::Closed | ExportResponse::None => {}
        }

        // Date picker (opened from the bottom toolbar): jump the visible window.
        match self.date_picker.show(ctx) {
            DatePickerAction::GoToDate { date, time } => self.go_to_date(date, time),
            DatePickerAction::SetRange {
                start,
                start_time,
                end,
                end_time,
            } => {
                self.set_date_range(start, start_time, end, end_time);
            }
            DatePickerAction::Cancel | DatePickerAction::None => {}
            _ => {}
        }
        self.show_data_source(ctx);
    }

    fn apply_chart_settings(&mut self, state: ChartSettingsState) {
        let futures = |chart: &Chart| {
            let settings = ChartSettingsState::from_chart(chart);
            (settings.back_adjust_futures, settings.settlement_as_close)
        };
        let futures_before = futures(&self.chart);
        let news_changed = state.events.show_news_notifications
            != self.chart.event_visibility().show_news_notifications;
        self.chart.begin_user_action();
        *self.settings_state = state.clone();
        self.trading.display = state.trading.clone();
        let alerts = &state.alerts;
        self.alerts
            .set_notification_seconds(if alerts.close_toasts {
                alerts.notification_seconds as f32
            } else {
                0.0
            });
        self.alerts.set_desktop(
            alerts
                .desktop_notifications
                .then(|| self.notification_host.clone() as _),
        );
        apply_settings_state(&state, &mut self.chart, &mut self.timeframe_toolbar);
        self.chart.end_user_action();
        if news_changed {
            self.annotate();
        }
        let dividends_changed =
            state.adjust_for_dividends != self.timeframe_toolbar.state.adjust_for_dividends;
        self.timeframe_toolbar.state.adjust_for_dividends = state.adjust_for_dividends;
        let futures_changed = futures(&self.chart) != futures_before;
        // shortcut: the synthetic contracts have no settlement prices or rolls, so the reload
        // serves the same bars; model both if the demo gains a continuous contract.
        self.follow_chart_session(dividends_changed || futures_changed);
    }

    /// Whether the current preset shows other charts to copy to.
    fn layout_has_other_charts(&self) -> bool {
        !self.extra_charts.is_empty() && self.layout.mode.chart_count() > 1
    }

    /// Applies `copy` when every chart can take it, and otherwise first lists what they lack.
    fn request_layout_copy(&mut self, copy: LayoutCopy) {
        let mut charts: Vec<_> = self
            .extra_charts
            .iter_mut()
            .map(|c| &mut *c.chart)
            .collect();
        charts.insert(self.layout_slot, &mut self.chart);
        let issues = self.layout.copy_issues(
            &charts,
            self.layout_slot,
            &copy,
            self.indicator_dialog.factory(),
        );
        if issues.is_empty() {
            self.apply_layout_copy(copy);
            return;
        }
        let any_reached = (0..self.layout.mode.chart_count().min(charts.len()))
            .filter(|slot| *slot != self.layout_slot)
            .any(|slot| {
                !issues
                    .iter()
                    .any(|issue| issue.slot == slot && issue.kind.skips_chart())
            });
        let mut lines: Vec<_> = issues
            .iter()
            .map(|issue| {
                let (slot, symbol) = (issue.slot + 1, charts[issue.slot].symbol());
                format!("Chart {slot} ({symbol}): {}", issue.kind)
            })
            .collect();
        if !any_reached {
            self.toasts.error(lines.join("\n"));
            return;
        }
        // The dialog does not scroll: its buttons stay on screen with many charts and studies.
        if lines.len() > 12 {
            let more = lines.len() - 12;
            lines.truncate(12);
            lines.push(format!("and {more} more"));
        }
        self.layout_copy_prompt = Some((copy, lines.join("\n")));
    }

    fn apply_layout_copy(&mut self, copy: LayoutCopy) {
        self.chart.begin_user_action();
        if let LayoutCopy::Settings(state) = &copy {
            self.apply_chart_settings((**state).clone());
        }
        let mut charts: Vec<_> = self
            .extra_charts
            .iter_mut()
            .map(|c| &mut *c.chart)
            .collect();
        charts.insert(self.layout_slot, &mut self.chart);
        self.layout.apply_copy(
            &mut charts,
            self.layout_slot,
            &copy,
            self.indicator_dialog.factory(),
        );
        self.chart.end_user_action();
    }

    fn show_layout_copy_prompt(&mut self, ctx: &egui::Context) {
        let Some((_, message)) = &self.layout_copy_prompt else {
            return;
        };
        let mut open = true;
        let result = egui_charts::ui_kit::modal::dialogs::ConfirmDialog::new(
            "layout_copy",
            "Apply to all charts",
            message,
        )
        .confirm_text("Apply")
        .show(ctx, &mut open);
        if result.is_open() {
            return;
        }
        let Some((copy, _)) = self.layout_copy_prompt.take() else {
            return;
        };
        if result.is_confirmed() {
            self.apply_layout_copy(copy);
        } else if let LayoutCopy::Settings(state) = copy {
            // The edits came from the dialog, which closed for this prompt; hand them back.
            self.settings_dialog.open(*state);
        }
    }

    fn remove_all_indicators(&mut self) {
        let chart = &mut self.chart;
        let ids: Vec<SeriesId> = chart
            .indicators()
            .filter(|(_, study)| study.user_edit_enabled())
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            chart.remove_series(id);
        }
    }

    fn clone_drawing(&mut self, id: usize) {
        self.chart.user_action(|chart| {
            if chart.drawings().is_selected(id) {
                let copies: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| chart.drawings().is_selected(d.id))
                    .cloned()
                    .collect();
                chart.drawings_mut().paste_drawings(&copies);
            } else {
                chart.drawings_mut().duplicate_drawing(id);
            }
        });
    }

    /// Shows the days `start` to `end`, both included, or the times on them, in the active
    /// chart's zone.
    fn set_date_range(
        &mut self,
        start: chrono::NaiveDate,
        start_time: Option<chrono::NaiveTime>,
        end: chrono::NaiveDate,
        end_time: Option<chrono::NaiveTime>,
    ) {
        let mut charts: Vec<_> = self
            .extra_charts
            .iter_mut()
            .map(|c| &mut *c.chart)
            .collect();
        charts.insert(self.layout_slot, &mut self.chart);
        self.layout
            .apply_local_date_range(&mut charts, start, start_time, end, end_time);
    }

    /// Goes to `time` on `date`, or to its start, on the active chart and the charts that share
    /// date ranges with it.
    fn go_to_date(&mut self, date: chrono::NaiveDate, time: Option<chrono::NaiveTime>) {
        let mut charts: Vec<_> = self
            .extra_charts
            .iter_mut()
            .map(|c| &mut *c.chart)
            .collect();
        charts.insert(self.layout_slot, &mut self.chart);
        self.layout.go_to_local_time(&mut charts, date, time);
    }

    fn run_export(
        &mut self,
        ctx: &egui::Context,
        config: egui_charts::ui::export::ExportConfig,
        save_path: Option<std::path::PathBuf>,
    ) {
        let target = match config.format {
            ExportFormat::Png => ScreenshotTarget::File(save_path),
            ExportFormat::Clipboard => ScreenshotTarget::Clipboard,
            ExportFormat::Csv => return self.export_csv(&config.data, save_path),
            ExportFormat::Svg => {
                return self.export_svg(ctx, &config.image, config.scope, save_path);
            }
        };
        self.screenshot_requested = Some((target, config.image, config.scope));
        ctx.request_repaint();
    }

    fn export_csv(
        &mut self,
        options: &egui_charts::widget::DataExportOptions,
        save_path: Option<std::path::PathBuf>,
    ) {
        let out = match self.chart.export_csv_with(options) {
            Ok(csv) => csv,
            Err(error) => return self.export_failed(save_path.is_some(), error.to_string()),
        };
        self.write_export(save_path, "csv", out.as_bytes());
    }

    fn export_svg(
        &mut self,
        ctx: &egui::Context,
        image: &egui_charts::widget::ImageExportOptions,
        scope: ExportScope,
        save_path: Option<std::path::PathBuf>,
    ) {
        match self.export_image_bytes(ctx, image, ExportFormat::Svg, scope == ExportScope::Layout) {
            Ok(svg) => self.write_export(save_path, "svg", &svg),
            Err(error) => self.export_failed(save_path.is_some(), error.to_string()),
        }
    }

    fn export_failed(&mut self, from_dialog: bool, error: String) {
        if from_dialog {
            self.export_dialog.mark_error(error.clone());
        }
        self.notify(format!("Export failed: {error}"));
    }

    fn write_export(&mut self, save_path: Option<std::path::PathBuf>, extension: &str, out: &[u8]) {
        let from_dialog = save_path.is_some();
        let path = save_path.unwrap_or_else(|| default_export_path(&self.state.symbol, extension));
        #[cfg(not(target_arch = "wasm32"))]
        let result = std::fs::write(&path, out).map_err(|e| e.to_string());
        #[cfg(target_arch = "wasm32")]
        let result = browser_download(out, &path);
        match result {
            Ok(()) => {
                if from_dialog {
                    self.export_dialog.mark_success();
                }
                self.notify(format!("Export successful: {}", path.display()));
            }
            Err(error) => self.export_failed(from_dialog, error),
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App: per-frame update that drives the panels, toolbars and dialogs.
// ---------------------------------------------------------------------------

impl eframe::App for DemoApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            "theme_choice",
            &self.theme_choice.as_deref().unwrap_or("system"),
        );
        eframe::set_value(storage, "demo_formatting", &self.formatting);
        let current = self.user_settings();
        let settings_saved = self.settings.flush(&current);
        eframe::set_value(storage, "detectors", &self.live.detectors().settings());
        storage.remove_string("api_samples");
        eframe::set_value(storage, "event_visibility", self.chart.event_visibility());
        eframe::set_value(storage, "open_panel", &self.open_panel);
        eframe::set_value(storage, "depth_panel", &self.depth);
        eframe::set_value(
            storage,
            "symbol_details_hidden",
            &self.symbol_details.hidden_sections(),
        );
        eframe::set_value(
            storage,
            "drawing_defaults",
            &self.chart.drawings().tool_defaults,
        );
        eframe::set_value(storage, "watchlists", &self.watchlist.lists());
        eframe::set_value(storage, "watchlist_active", &self.watchlist.active_list());
        eframe::set_value(
            storage,
            "watchlist_unlisted_flags",
            &self.watchlist.unlisted_flags(),
        );
        eframe::set_value(storage, "alerts", &self.alert_host.engine().alerts());
        eframe::set_value(
            storage,
            "alert_triggers",
            &self.alert_host.engine().triggers(),
        );
        eframe::set_value(storage, "alert_list_options", self.alerts.options());
        // The live account: a replay session trades an account of its own, which is not kept.
        eframe::set_value(storage, "paper_account", &self.paper.save());
        eframe::set_value(storage, "order_presets", &self.trading.presets);
        eframe::set_value(storage, "paper_connected", &self.trading.has_broker());
        eframe::set_value(storage, "account_panel_height", &self.account_panel_height);
        self.checkpoint_replay();
        eframe::set_value(storage, "replay_session", &self.replay_session);
        self.checkpoint_workspace();
        self.storage.persist(storage);
        eframe::set_value(
            storage,
            "interval_preferences",
            &self.top_toolbar.timeframe_selector.preferences(),
        );
        eframe::set_value(
            storage,
            "drawing_toolbar_preferences",
            &self.drawing_toolbar.state.preferences,
        );
        if settings_saved {
            storage.remove_string("cursor_favorites");
        }
        eframe::set_value(
            storage,
            "chart_type_preferences",
            &self.top_toolbar.chart_type_selector.preferences(),
        );
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.update_theme(&ctx);
        egui_charts::theme::apply_to_egui(&ctx, &self.theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ctx.global_style());

        // Fulfil requests from the previous frame, before a dialog can submit another.
        self.handle_screenshot(&ctx);
        for error in self.images.poll(&ctx, &mut self.chart, self.layout_slot) {
            self.notify(error);
        }
        self.poll_alerts(&ctx);
        self.symbol_details.poll(&ctx);
        self.highlights.poll(&ctx);
        self.news.poll(&ctx);
        for notification in self
            .news
            .take_notifications()
            .into_iter()
            .chain(self.highlights.take_notifications())
        {
            self.notification_host.publish(notification);
        }
        self.calendar.poll(&ctx);
        self.screener.poll(&ctx);
        self.hotlists.poll(&ctx);
        self.notifications.poll(&ctx);
        self.chats.poll(&ctx);
        self.profiles.poll(&ctx);
        if let Some(user) = self.chats.take_profile_request() {
            self.profile_open = Some(ProfileId(user.id));
        }
        self.widget_bar.set_notification_cnt(
            RightBarIcon::Notifications,
            self.notifications.unread() as u32,
        );
        self.trading.poll(&ctx, &mut self.toasts);
        if let Some(sound) = self.trading.take_execution_sound() {
            self.play_sound(
                &ctx,
                &sound,
                self.trading.display.execution_sound_volume,
                false,
            );
        }
        if let Some(name) = self.alerts.take_sound() {
            self.play_sound(&ctx, &name, self.settings_state.alerts.sound_volume, false);
        }
        if let Some(error) = self.audio.take_error() {
            self.toasts.error(error);
        }
        self.poll_storage(&ctx);

        // Drive replay before laying out the chart so its window is current.
        self.poll_recording(&ctx);
        self.poll_watchlist_file();
        self.advance_replay();
        if self.replay.is_none() || self.live.loading() {
            let now = chrono::Utc::now();
            let delay = self.live.advance(now);
            self.advance_live_strategy_source(now);
            if !self.live.paused() || delay.is_zero() {
                ctx.request_repaint_after(delay);
            }
        }
        let detectors = self.live.detectors();
        let mut settings = detectors.settings();
        let before = settings.clone();
        // Footprint alerts read the imbalances and rows the active chart shows.
        let rows = self.chart.footprint_row_ticks();
        settings
            .footprint
            .rule
            .follow_chart(self.chart.chart_type_params(), rows);
        if settings != before {
            detectors.set_settings(settings);
        }
        self.order_flow.set_service(detectors);
        self.notification_host
            .watch_detectors(self.live.detectors());
        let sounds = self.order_flow.poll(&ctx, &mut self.toasts);
        if !sounds.is_empty() {
            let alerts = &self.settings_state.alerts;
            let (name, volume) = (alerts.sound.clone(), alerts.sound_volume);
            self.play_sound(&ctx, &name, volume, false);
        }
        self.feed_market_panels(&ctx);

        for (_, _, feed) in &self.comparisons {
            feed.set_paused(self.live.paused() || self.replay.is_some());
            feed.advance(chrono::Utc::now());
        }

        for (index, extra) in self.extra_charts.iter_mut().enumerate() {
            if !slot_shown(&self.layout, self.layout_slot, index) {
                continue;
            }
            extra.live.set_symbol(extra.chart.symbol());
            let delay = extra.live.advance(chrono::Utc::now());
            for (_, id, feed) in &extra.comparisons {
                feed.set_paused(extra.live.paused());
                feed.advance(chrono::Utc::now());
                comparison::refresh(&mut extra.chart, *id, feed);
            }
            comparison::refresh_spreads(&mut extra.chart, &mut extra.spreads, extra.live.paused());
            if !extra.live.paused() || delay.is_zero() {
                ctx.request_repaint_after(delay);
            }
        }

        // A colour picker drag ends, as one edit, when the pointer is released.
        if self.colour_drag && !ctx.input(|i| i.pointer.any_down()) {
            self.colour_drag = false;
            self.chart
                .user_action(|chart| chart.drawings_mut().end_drag_handle());
        }

        let panel_bg = ui.style().visuals.panel_fill;
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, self.theme.ui.border);

        // Dialogs consume their keys before chart commands and the replay transport.
        if self.sample_scene.is_none() && !self.keyboard_dialog_open() {
            let action = self.replay_control.shortcut(&ctx);
            self.handle_replay_control(action);
            self.handle_keyboard(&ctx);
        }
        if self.sample_scene.is_none() && self.keyboard_dialog_open() {
            self.chart.suppress_shortcuts(&ctx);
        }
        if self.show_samples(ui) {
            self.storage.flush(frame.storage_mut());
            return;
        }
        self.show_layout_tabs(ui);
        self.refresh_layout_menu();
        self.show_top_toolbar(ui, panel_bg);
        self.layout.sync = self.top_toolbar.chart_grid_selector.sync.clone();
        let removal = egui_charts::ui::drawing_toolbar::utilities::RemoveMenu::show_confirmation(
            &ctx,
            &mut self.drawing_toolbar.state,
        );
        self.handle_drawing_toolbar(removal);
        self.show_drawing_toolbar(ui, panel_bg);
        self.show_widget_bar(ui, panel_bg);
        self.trading.show_ticket(ui, &self.chart);
        let narrow_account = ui.available_width() < 700.0 && self.trading.has_broker();
        if narrow_account {
            self.show_account_panel(ui, panel_bg);
        }
        self.show_side_panel(ui);
        if !narrow_account {
            self.show_account_panel(ui, panel_bg);
        }
        self.show_strategy_report(ui, panel_bg);
        self.show_sample_study(ui);
        self.show_bottom_toolbar(ui, panel_bg);
        self.reserve_replay_strip(ui, panel_bg);
        let groups = self.layout_groups();
        self.show_chart(ui, panel_bg);
        if self.layout_groups() != groups {
            self.grid_edits = self.grid_edits.wrapping_add(1);
        }
        self.show_replay_control(&ctx);
        self.show_replay_start(&ctx);
        self.show_replay_trading(&ctx);
        self.show_annotation_editor(&ctx);
        self.show_dialogs(&ctx);
        self.show_wide_watchlist(&ctx);
        self.show_profile(&ctx);

        // Transient status notice.
        self.show_notice(ui);
        if let Some(storage) = frame.storage_mut()
            && eframe::get_value::<String>(storage, "theme_choice").as_deref()
                != Some(self.theme_choice.as_deref().unwrap_or("system"))
        {
            eframe::set_value(
                storage,
                "theme_choice",
                &self.theme_choice.as_deref().unwrap_or("system"),
            );
            storage.flush();
        }
        if let Some(storage) = frame.storage_mut()
            && eframe::get_value::<formatting::Formatting>(storage, "demo_formatting").as_ref()
                != Some(&self.formatting)
        {
            eframe::set_value(storage, "demo_formatting", &self.formatting);
            storage.flush();
        }
        self.poll_workspace(&ctx);
        self.storage.flush(frame.storage_mut());

        // Repaint continuously while replay is playing.
        if self
            .replay
            .as_ref()
            .map(|r| r.state.is_playing())
            .unwrap_or(false)
        {
            ctx.request_repaint();
        }
    }
}

// ---------------------------------------------------------------------------
// Per-region rendering + action routing.
// ---------------------------------------------------------------------------

impl DemoApp {
    fn sync_depth_history(&mut self) {
        if self.own_data.is_some() {
            return;
        }
        // Replay hides the live history, which must survive for its exit.
        if self.chart.replay_time().is_some()
            || !ChartSettingsState::from_chart(&self.chart)
                .order_flow
                .show_heatmap
        {
            return;
        }
        let history = depth_history(self.chart.symbol());
        if self
            .chart
            .depth_history(SeriesId::MAIN)
            .is_none_or(|current| current.config().orders != history.config().orders)
        {
            self.chart.set_depth_history(Some(history));
        }
    }

    fn show_top_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        use commands::{menu_item, menu_toggle};
        use egui_charts::icons::icons;

        self.sync_depth_history();
        self.top_toolbar.sync_from_app_state(&self.state);
        self.top_toolbar.state.can_undo = self.chart.can_undo();
        self.top_toolbar.state.can_redo = self.chart.can_redo();
        egui::Panel::top("toolbar")
            .exact_size(self.top_toolbar.config.height + 4.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                bottom: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                let mut menu_action = TopToolbarAction::None;
                let mut sample = None;
                let mut panel_action = None;
                let mut widget_action = None;
                let mut show_help = false;
                let order_flow = ChartSettingsState::from_chart(&self.chart).order_flow;
                let mut heatmap_enabled = order_flow.show_heatmap;
                let mut largest_profile =
                    order_flow.order_book.profile == BookProfile::LargestOrder;
                let mut paused = self
                    .own_data
                    .as_ref()
                    .map_or(self.live.paused(), |own| own.paused);
                let mut study_permission = None;
                let mut toolbar = std::mem::take(&mut self.top_toolbar);
                let state = self.state.clone();
                let action = toolbar.show_with_menu(ui, &state, |ui| {
                    if let Some(action) =
                        egui_charts::ui::drawing_toolbar::utilities::HideMenu::show_toolbar_toggle(
                            ui,
                            &self.drawing_toolbar.state,
                        )
                    {
                        self.handle_drawing_toolbar(action);
                    }
                    if ui.ctx().content_rect().width() < 600.0 {
                        let response = menu_item(
                            ui,
                            "Panels",
                            &icons::WIDGET_BAR_LAYERS,
                            Some(&icons::CHEVRON_RIGHT),
                        );
                        egui::containers::menu::SubMenu::new()
                            .config(
                                egui::containers::menu::MenuConfig::new()
                                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                            )
                            .show(ui, &response, |ui| {
                                ui.set_width(
                                    egui_charts::tokens::DESIGN_TOKENS
                                        .sizing
                                        .dialog
                                        .submenu_width,
                                );
                                let height = (ui.ctx().content_rect().height() - 40.0).max(80.0);
                                // A popup starts shorter than a long panel list.
                                ui.set_max_height(height);
                                egui::ScrollArea::vertical()
                                    .max_height(height)
                                    .show(ui, |ui| {
                                        for (panel, icon) in SidePanel::BUILT_IN {
                                            if self.widget_bar.icons.contains(&icon)
                                                && menu_item(
                                                    ui,
                                                    icon.tooltip(),
                                                    icon.icon(),
                                                    (self.open_panel == Some(panel))
                                                        .then_some(&icons::CHECK),
                                                )
                                                .clicked()
                                            {
                                                panel_action = Some(panel);
                                                ui.close();
                                            }
                                        }
                                        if menu_item(
                                            ui,
                                            "Data Window",
                                            &icons::TABLE,
                                            (self.open_panel == Some(SidePanel::DataWindow))
                                                .then_some(&icons::CHECK),
                                        )
                                        .clicked()
                                        {
                                            panel_action = Some(SidePanel::DataWindow);
                                            ui.close();
                                        }
                                        for widget in &self.widget_bar.widgets {
                                            let icon = if widget.id == 2 {
                                                &icons::NOTE
                                            } else {
                                                &widget.icon
                                            };
                                            let panel = SidePanel::from_widget(widget.id);
                                            let open = match panel {
                                                Some(_) => self.open_panel == panel,
                                                None => {
                                                    self.widget_bar.active_widget == Some(widget.id)
                                                }
                                            };
                                            if menu_item(
                                                ui,
                                                &widget.tooltip,
                                                icon,
                                                open.then_some(&icons::CHECK),
                                            )
                                            .clicked()
                                            {
                                                if panel.is_some() {
                                                    panel_action = panel;
                                                } else {
                                                    widget_action = Some(widget.id);
                                                }
                                                ui.close();
                                            }
                                        }
                                        if menu_item(
                                            ui,
                                            "Keyboard shortcuts",
                                            &icons::WIDGET_BAR_HELP,
                                            None,
                                        )
                                        .clicked()
                                        {
                                            show_help = true;
                                            ui.close();
                                        }
                                    });
                            });
                        ui.separator();
                    }
                    sample = self.show_demo_controls(ui);
                    menu_toggle(
                        ui,
                        "Replay toolbar",
                        &icons::REPLAY,
                        &mut self.replay_toolbar,
                    );
                    let editing = menu_item(
                        ui,
                        "Study editing",
                        &icons::LOCK,
                        Some(&icons::CHEVRON_RIGHT),
                    );
                    egui::containers::menu::SubMenu::new()
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .show(ui, &editing, |ui| {
                            for (id, study) in self.chart.indicators() {
                                let mut enabled = study.user_edit_enabled();
                                menu_toggle(ui, study.name(), &icons::SETTINGS, &mut enabled);
                                if enabled != study.user_edit_enabled() {
                                    study_permission = Some((id, enabled));
                                }
                            }
                        });
                    ui.separator();
                    // The mapped feed of own data has no depth to draw.
                    ui.add_enabled_ui(self.own_data.is_none(), |ui| {
                        menu_toggle(
                            ui,
                            "Liquidity heatmap",
                            &icons::LIQUIDITY_HEATMAP,
                            &mut heatmap_enabled,
                        );
                    });
                    ui.add_enabled_ui(heatmap_enabled || order_flow.show_order_book, |ui| {
                        menu_toggle(
                            ui,
                            "Largest order profile",
                            &icons::CHART_SESSION_VOLUME_PROFILE,
                            &mut largest_profile,
                        );
                    });
                    ui.separator();
                    ui.add_enabled_ui(self.replay.is_none(), |ui| {
                        let label = if self.own_data.is_some() {
                            "Pause data feed"
                        } else {
                            "Pause synthetic feed"
                        };
                        menu_toggle(ui, label, &icons::MEDIA_PAUSE, &mut paused);
                    });
                    if menu_item(ui, "Data source", &icons::DATABASE, None).clicked() {
                        self.data_source.open();
                        ui.close();
                    }
                    ui.separator();
                    if menu_item(ui, "Order flow", &icons::CHART_VOLUME_FOOTPRINT, None).clicked() {
                        panel_action = Some(SidePanel::OrderFlow);
                        ui.close();
                    }
                    if egui_charts::ui::trading::DepthPanel::is_available(&self.chart)
                        && menu_item(ui, "Depth", &icons::FIXED_RANGE_VOLUME_PROFILE, None)
                            .clicked()
                    {
                        panel_action = Some(SidePanel::Depth);
                        ui.close();
                    }
                    // Host actions live in the menu so the default strip keeps the
                    // built-in controls only.
                    for (label, icon, action) in [
                        (
                            "Research",
                            &icons::QUICK_SEARCH,
                            TopToolbarAction::Host("research".into()),
                        ),
                        (
                            "Copy symbol",
                            &icons::DUPLICATE,
                            TopToolbarAction::Host("copy-symbol".into()),
                        ),
                    ] {
                        if menu_item(ui, label, icon, None).clicked() {
                            menu_action = action;
                            ui.close();
                        }
                    }
                });
                self.top_toolbar = toolbar;
                if let Some((id, enabled)) = study_permission {
                    // Undo keeps the host's permission, so autosave counts the change here.
                    self.chart.set_indicator_user_edit_enabled(id, enabled);
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
                if let Some(panel) = panel_action {
                    self.open_panel = (self.open_panel != Some(panel)).then_some(panel);
                    self.widget_bar.active_widget = None;
                    match self.open_panel {
                        Some(SidePanel::Hotlists) => self.hotlists.refresh(),
                        Some(SidePanel::Screener) => self.screener.refresh(),
                        _ => {}
                    }
                }
                if let Some(id) = widget_action {
                    self.open_panel = None;
                    self.widget_bar.active_widget =
                        (self.widget_bar.active_widget != Some(id)).then_some(id);
                }
                if show_help {
                    self.keyboard_shortcuts.toggle();
                }
                if let Some(own) = &mut self.own_data {
                    // Pausing the chart ends its subscription, which closes the socket.
                    if own.paused != paused {
                        own.paused = paused;
                        self.chart.set_feed_paused(paused);
                    }
                } else {
                    self.live.set_paused(paused);
                }
                if heatmap_enabled != order_flow.show_heatmap {
                    self.chart.user_action(|chart| {
                        chart.set_liquidity_heatmap(heatmap_enabled);
                        chart.set_volume_bubbles(
                            heatmap_enabled.then(|| order_flow.volume_bubbles.clone()),
                        );
                    });
                }
                if largest_profile != (order_flow.order_book.profile == BookProfile::LargestOrder) {
                    let mut view = order_flow.order_book;
                    view.profile = if largest_profile {
                        BookProfile::LargestOrder
                    } else {
                        BookProfile::Total
                    };
                    self.chart
                        .user_action(|chart| chart.set_order_book_view(view));
                }
                self.handle_top_toolbar(ui.ctx(), action);
                self.handle_top_toolbar(ui.ctx(), menu_action);
                if let Some(id) = sample {
                    self.execute_search_command(ui.ctx(), id);
                }
            });
    }

    pub(crate) fn handle_top_toolbar(&mut self, ctx: &egui::Context, action: TopToolbarAction) {
        match action {
            TopToolbarAction::None => return,
            TopToolbarAction::Host(id) => {
                if id == "copy-symbol" {
                    ctx.copy_text(self.chart.symbol().to_string());
                }
                if id == "research" {
                    if let Some(click) = &self.last_chart_click
                        && let Some(time) = click.time
                    {
                        self.toasts.info(format!(
                            "Host action: {id}, last chart click: {}, price: {:.4}",
                            time.to_rfc3339(),
                            click.price
                        ));
                    } else {
                        self.toasts
                            .info("Click the chart to choose a research time and price");
                    }
                } else {
                    self.toasts
                        .info(format!("Host action: {id} ({})", self.chart.symbol()));
                }
                return;
            }
            TopToolbarAction::Save => {
                self.handle_layout_action(egui_charts::ui::top_toolbar::LayoutAction::Save);
                return;
            }
            TopToolbarAction::ToggleTrading => {
                if !self.trading.has_broker() {
                    self.broker_picker.open();
                } else if self.account_panel == AccountPanelSize::Closed {
                    self.account_panel = AccountPanelSize::Normal;
                } else {
                    self.account_panel = AccountPanelSize::Closed;
                }
                return;
            }
            TopToolbarAction::Layout(action) => {
                self.handle_layout_action(action);
                return;
            }
            TopToolbarAction::Undo => {
                self.chart.undo();
                self.sync_chart_history();
                return;
            }
            TopToolbarAction::Redo => {
                self.chart.redo();
                self.sync_chart_history();
                return;
            }
            // The theme is the interface's, on every chart: switching it is no chart's edit.
            TopToolbarAction::ThemeToggle => {
                self.toggle_theme(ctx);
                return;
            }
            _ => {}
        }

        self.user_action(|app| app.handle_top_toolbar_edit(ctx, action));
    }

    fn handle_top_toolbar_edit(&mut self, ctx: &egui::Context, action: TopToolbarAction) {
        match action {
            TopToolbarAction::ChartStyleSelected(ct) => {
                self.state.chart_type = ct;
                self.chart.set_chart_type(ct);
                self.size_range_interval();
            }
            TopToolbarAction::IntervalSelected(tf) => self.switch_timeframe(tf),
            TopToolbarAction::OpenSymbolSearch => self
                .symbol_search
                .open_with_selected_query(self.chart.symbol().to_owned()),
            TopToolbarAction::OpenCompare => {
                if self.synthetic_only("Compare") {
                    return;
                }
                self.compare_dialog.open();
            }
            TopToolbarAction::OpenQuickSearch => self.open_quick_search(),
            TopToolbarAction::OpenIndicators => {
                self.indicator_dialog.open_for_chart(&self.chart, None);
            }
            TopToolbarAction::CreateAlert => self.open_alert(None),
            TopToolbarAction::ToggleReplay => self.toggle_replay(),
            TopToolbarAction::OpenSettings => {
                self.open_chart_settings_dialog();
            }
            TopToolbarAction::Publish => {
                self.open_panel =
                    (self.open_panel != Some(SidePanel::Ideas)).then_some(SidePanel::Ideas);
            }
            TopToolbarAction::TakeScreenshot
            | TopToolbarAction::CopyImage
            | TopToolbarAction::OpenImage
            | TopToolbarAction::PostImage
            | TopToolbarAction::CopyImageLink => {
                self.export_dialog.close();
                self.toasts.clear();
                let target = match action {
                    TopToolbarAction::CopyImage => ScreenshotTarget::Clipboard,
                    TopToolbarAction::OpenImage => ScreenshotTarget::NewTab,
                    TopToolbarAction::PostImage | TopToolbarAction::CopyImageLink => {
                        ScreenshotTarget::Post {
                            copy_link: action == TopToolbarAction::CopyImageLink,
                        }
                    }
                    _ => ScreenshotTarget::File(None),
                };
                self.screenshot_requested = Some((
                    target,
                    {
                        let mut options = egui_charts::widget::ImageExportOptions::default();
                        options.include_crosshair = true;
                        options
                    },
                    ExportScope::Layout,
                ));
                ctx.request_repaint();
            }
            TopToolbarAction::ToggleFullscreen => {
                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
            }
            TopToolbarAction::Undo | TopToolbarAction::Redo => {}
            TopToolbarAction::AddIndicator(name) => {
                // A study with symbols to choose opens the dialog's prompt and arrives from `show`.
                if let Some(IndicatorDialogAction::AddIndicator(study)) =
                    self.indicator_dialog.add(&name)
                {
                    self.chart.begin_user_action();
                    self.chart.add_indicator(study);
                    self.chart.end_user_action();
                }
            }
            TopToolbarAction::SaveIndicatorTemplate => {
                self.study_templates.open(self.chart.study_template_set());
            }
            TopToolbarAction::ApplyIndicatorTemplate(id) => {
                if let Some(template) = self.study_templates.templates.entries().get(&id).cloned() {
                    self.load_study_template(&template);
                }
            }
            TopToolbarAction::ApplyBuiltinIndicatorTemplate(id) => {
                if let Some((_, template)) = StudyTemplateSet::builtin()
                    .into_iter()
                    .find(|(name, _)| *name == id)
                {
                    self.load_study_template(&template);
                }
            }
            TopToolbarAction::Layout(_)
            | TopToolbarAction::ThemeToggle
            | TopToolbarAction::Save
            | TopToolbarAction::ToggleTrading
            | TopToolbarAction::Host(_)
            | TopToolbarAction::None => {}
            _ => {}
        }
    }

    fn show_drawing_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        self.drawing_toolbar.state.zoom_mode_active = self.chart.is_zoom_mode_active();
        self.drawing_toolbar.state.has_zoom_history = self.chart.state().has_zoom_history();
        use egui_charts::config::CrosshairStyle;
        use egui_charts::ui::drawing_toolbar::{
            DrawingToolbarDomainState, MagnetType, categories::cursors::CursorType,
        };
        let dm = self.chart.drawings();
        let magnet_type = if dm.options.strong_magnet {
            MagnetType::Strong
        } else {
            MagnetType::Weak
        };
        self.drawing_toolbar
            .sync_from_domain_state(&DrawingToolbarDomainState {
                sel_tool: dm.active_tool(),
                sel_custom_tool: dm.active_custom_tool().map(str::to_owned),
                magnet_mode: dm.options.magnet_mode,
                snap_to_indicators: dm.options.snap_to_indicators,
                magnet_type,
                current_cursor_type: match self.chart.chart_options().crosshair.style {
                    CrosshairStyle::Full => CursorType::Cross,
                    CrosshairStyle::Dot => CursorType::Dot,
                    CrosshairStyle::Arrow | CrosshairStyle::Hidden => CursorType::Arrow,
                    CrosshairStyle::Demonstration => CursorType::Demonstration,
                    CrosshairStyle::Magic => CursorType::Magic,
                    _ => CursorType::Cross,
                },
                eraser_mode_enabled: dm.is_eraser_mode(),
                stay_in_drawing_mode: dm.stay_in_drawing_mode,
                drawing_color: dm
                    .new_drawing(
                        0,
                        dm.active_tool()
                            .unwrap_or(egui_charts::drawings::DrawingToolType::TrendLine),
                    )
                    .color,
            });
        self.drawing_toolbar.state.hide_indicators = self.chart.indicators_hidden();
        self.drawing_toolbar.state.hide_drawings = self.chart.drawings().is_hidden();
        // Show readout also sets the readout, so the cursor menu switch reads it from the chart.
        self.drawing_toolbar.state.values_tooltip_on_long_press =
            self.chart.chart_options().tooltip.enabled;
        self.drawing_toolbar.state.crosshair_close_snap = self.chart.chart_options().crosshair.mode
            == egui_charts::config::CrosshairMode::MagnetClose;
        // The trading rows show while a broker is connected; their checks follow the Trading tab.
        self.drawing_toolbar
            .set_control_visible(egui_charts::ui::Control::Trading, self.trading.has_broker());
        self.trading.sync_hide_menu(&mut self.drawing_toolbar.state);
        self.drawing_toolbar.state.lock_all = self.chart.drawings().is_locked();
        if self.drawing_toolbar.state.preferences.hidden {
            let action = self.drawing_toolbar.show_with_action(ui);
            self.handle_drawing_toolbar(action);
            return;
        }
        egui::Panel::left("drawing_toolbar")
            .resizable(false)
            .exact_size(56.0)
            .show_separator_line(false)
            .frame(
                panel_frame(bg)
                    .corner_radius(egui::CornerRadius {
                        ne: 4,
                        ..egui::CornerRadius::ZERO
                    })
                    .outer_margin(egui::Margin {
                        right: 4,
                        ..egui::Margin::ZERO
                    }),
            )
            .show(ui, |ui| {
                let action = self.drawing_toolbar.show_with_action(ui);
                self.handle_drawing_toolbar(action);
            });
    }

    pub(crate) fn handle_drawing_toolbar(&mut self, action: DrawingToolbarAction) {
        if action == DrawingToolbarAction::None {
            return;
        }
        let action = egui_charts::ui::drawing_toolbar::utilities::RemoveMenu::request(
            action,
            {
                let dm = self.chart.drawings();
                dm.drawings
                    .iter()
                    .any(|d| d.user_edit_enabled && (d.locked || dm.is_locked()))
            },
            &mut self.drawing_toolbar.state,
        );
        self.trading
            .apply_hide_action(&action, &self.drawing_toolbar.state);
        match action {
            DrawingToolbarAction::HideAllPoss
            | DrawingToolbarAction::HideAllOrders
            | DrawingToolbarAction::HideAllExecutions => return,
            DrawingToolbarAction::SetToolbarVisible(visible) => {
                self.drawing_toolbar.state.preferences.hidden = !visible;
                return;
            }
            DrawingToolbarAction::SetLockedDrawingRemoval(include) => {
                self.drawing_toolbar.state.preferences.remove_locked = Some(include);
                return;
            }
            DrawingToolbarAction::None => return,
            DrawingToolbarAction::ZoomIn => {
                self.chart
                    .set_zoom_mode(self.drawing_toolbar.state.zoom_mode_active);
                return;
            }
            DrawingToolbarAction::ZoomOut => {
                self.chart.zoom_back();
                return;
            }
            _ => {}
        }
        self.user_action(|app| app.handle_drawing_toolbar_edit(action));
    }

    /// Sets the crosshair mode that the cursor menu's close switch and the drawing magnet
    /// choose together: closes only while the switch is on, else the magnet's snapping or none.
    fn set_crosshair_snapping(&mut self, close_only: bool) {
        use egui_charts::config::CrosshairMode;
        self.chart.set_crosshair_mode(if close_only {
            CrosshairMode::MagnetClose
        } else if self.chart.drawings().options.magnet_mode {
            CrosshairMode::Magnet
        } else {
            CrosshairMode::Normal
        });
    }

    fn handle_drawing_toolbar_edit(&mut self, action: DrawingToolbarAction) {
        // Variants that touch the indicator registry (not the drawing manager)
        // or surface a notice are handled first, before the `dm` borrow.
        match action {
            // Each tool's templates are loaded and deleted from its properties' Template menu.
            DrawingToolbarAction::ManageTemplates => {
                match self.chart.drawings().sel_drawing() {
                    Some(id) => self.open_drawing_props_for(id),
                    None => self.notify("Select a drawing first"),
                }
                return;
            }
            DrawingToolbarAction::SaveTemplate { name } => {
                let Some(template) = self.current_drawing_template() else {
                    self.notify("Select a drawing first");
                    return;
                };
                if let Err(error) = self.drawing_props.templates.save_for_tool(&name, template) {
                    self.notify(error);
                }
                return;
            }
            DrawingToolbarAction::LoadTemplate { template_id } => {
                if let Some(template) = self
                    .drawing_props
                    .templates
                    .entries()
                    .get(&template_id)
                    .cloned()
                {
                    self.load_drawing_template(template);
                }
                return;
            }
            DrawingToolbarAction::RemoveAllIndicators | DrawingToolbarAction::RemoveAllStudies => {
                self.remove_all_indicators();
                return;
            }
            DrawingToolbarAction::HideAllIndicators => {
                self.chart
                    .set_indicators_hidden(self.drawing_toolbar.state.hide_indicators);
                return;
            }
            DrawingToolbarAction::HideAll => {
                self.chart
                    .drawings_mut()
                    .set_hidden(self.drawing_toolbar.state.hide_drawings);
                self.chart
                    .set_indicators_hidden(self.drawing_toolbar.state.hide_indicators);
                return;
            }
            DrawingToolbarAction::RemoveDrawings {
                include_locked,
                include_indicators,
            } => {
                self.chart
                    .drawings_mut()
                    .clear_all_with_locked(include_locked);
                if include_indicators {
                    self.remove_all_indicators();
                }
                return;
            }
            _ => {}
        }

        match action {
            DrawingToolbarAction::SetCursorType(cursor) => {
                use egui_charts::config::CrosshairStyle;
                use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
                self.chart.set_crosshair_style(match cursor {
                    CursorType::Cross => CrosshairStyle::Full,
                    CursorType::Dot => CrosshairStyle::Dot,
                    CursorType::Arrow | CursorType::Eraser => CrosshairStyle::Arrow,
                    CursorType::Demonstration => CrosshairStyle::Demonstration,
                    CursorType::Magic => CrosshairStyle::Magic,
                });
                self.chart.drawings_mut().set_active_tool(None);
                self.chart
                    .drawings_mut()
                    .set_eraser_mode(cursor == CursorType::Eraser);
                self.chart.set_zoom_mode(false);
                return;
            }
            DrawingToolbarAction::ToggleValuesTooltip => {
                self.chart
                    .set_tooltip_enabled(self.drawing_toolbar.state.values_tooltip_on_long_press);
                return;
            }
            DrawingToolbarAction::SetCrosshairCloseSnap(on) => {
                self.set_crosshair_snapping(on);
                return;
            }
            _ => {}
        }

        let dm = self.chart.drawings_mut();
        match action {
            DrawingToolbarAction::SelectTool(tool) => {
                dm.set_active_tool(Some(tool));
            }
            // The toolbar offers only registered tools that are enabled.
            DrawingToolbarAction::SelectCustomTool(id) => {
                let _ = dm.set_custom_tool(&id);
            }
            DrawingToolbarAction::ToggleMagnet => {
                dm.options.magnet_mode = !dm.options.magnet_mode;
                dm.options.strong_magnet = self.drawing_toolbar.state.magnet_type
                    != egui_charts::ui::drawing_toolbar::MagnetType::Weak;
                dm.update_snap_options();
                let close_only = self.chart.chart_options().crosshair.mode
                    == egui_charts::config::CrosshairMode::MagnetClose;
                self.set_crosshair_snapping(close_only);
            }
            DrawingToolbarAction::SetSnapToIndicators(enabled) => {
                dm.options.snap_to_indicators = enabled;
            }
            DrawingToolbarAction::SetMagnetType(kind) => {
                dm.options.magnet_mode = true;
                dm.options.strong_magnet =
                    kind != egui_charts::ui::drawing_toolbar::MagnetType::Weak;
                dm.update_snap_options();
                let close_only = self.chart.chart_options().crosshair.mode
                    == egui_charts::config::CrosshairMode::MagnetClose;
                self.set_crosshair_snapping(close_only);
            }
            DrawingToolbarAction::ToggleEraserMode => {
                let on = !dm.is_eraser_mode();
                dm.set_eraser_mode(on);
            }
            DrawingToolbarAction::ToggleStayInDrawingMode => {
                dm.stay_in_drawing_mode = !dm.stay_in_drawing_mode;
            }
            DrawingToolbarAction::ColorChanged(rgba) => dm.options.default_color = Some(rgba),
            DrawingToolbarAction::ClearAllDrawings | DrawingToolbarAction::RemoveAllDrawings => {
                dm.clear_all()
            }
            DrawingToolbarAction::HideAllDrawings => {
                dm.set_hidden(self.drawing_toolbar.state.hide_drawings);
            }
            DrawingToolbarAction::LockAllDrawings => {
                dm.set_locked(self.drawing_toolbar.state.lock_all)
            }
            DrawingToolbarAction::InsertGlyph { icon_name, size } => {
                dm.start_icon_insertion(icon_name.to_string());
                if let Some(drawing) = dm.curr_drawing_mut() {
                    drawing.font_size = size;
                }
            }
            DrawingToolbarAction::InsertEmoji { icon_name } => {
                dm.start_icon_insertion(icon_name.to_string())
            }
            DrawingToolbarAction::None
            | DrawingToolbarAction::OpenTemplateMenu
            | DrawingToolbarAction::ManageTemplates
            | DrawingToolbarAction::SaveTemplate { .. }
            | DrawingToolbarAction::LoadTemplate { .. }
            | DrawingToolbarAction::RemoveAllIndicators
            | DrawingToolbarAction::RemoveAllStudies
            | DrawingToolbarAction::HideAllIndicators
            | DrawingToolbarAction::SetCursorType(_)
            | DrawingToolbarAction::ToggleValuesTooltip
            | DrawingToolbarAction::ZoomIn
            | DrawingToolbarAction::ZoomOut => {}
            _ => {}
        }
    }

    fn show_widget_bar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        if ui.ctx().content_rect().width() < 600.0 {
            return;
        }
        self.widget_bar.active_icon = self.open_panel.and_then(SidePanel::icon);
        if let Some(id) = self.open_panel.and_then(SidePanel::widget) {
            self.widget_bar.active_widget = Some(id);
        } else if self.open_panel.is_some()
            || self
                .widget_bar
                .active_widget
                .is_some_and(|id| SidePanel::from_widget(id).is_some())
        {
            self.widget_bar.active_widget = None;
        }
        egui::Panel::right("widget_bar")
            .resizable(false)
            .exact_size(46.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                left: 1,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                let action = self.widget_bar.show(ui);
                match action {
                    WidgetBarAction::Clicked(icon) => self.toggle_panel(icon),
                    WidgetBarAction::WidgetClicked(id) => {
                        let panel = SidePanel::from_widget(id);
                        self.open_panel = panel.filter(|panel| self.open_panel != Some(*panel));
                    }
                    _ => {}
                }
            });
    }

    pub(crate) fn toggle_panel(&mut self, icon: RightBarIcon) {
        if icon == RightBarIcon::Help {
            self.keyboard_shortcuts.toggle();
        }
        if let Some(target) = SidePanel::from_icon(icon) {
            self.open_panel = if self.open_panel == Some(target) {
                None
            } else {
                Some(target)
            };
            // Quotes move: list current figures each time a ranking opens.
            match self.open_panel {
                Some(SidePanel::Hotlists) => self.hotlists.refresh(),
                Some(SidePanel::Screener) => self.screener.refresh(),
                _ => {}
            }
        }
    }

    fn show_side_panel(&mut self, ui: &mut egui::Ui) {
        if self.open_panel.is_some() {
            self.widget_bar.active_widget = None;
        }
        self.widget_bar.show_panel(ui);
        if self.open_panel == Some(SidePanel::Depth)
            && !egui_charts::ui::trading::DepthPanel::is_available(&self.chart)
        {
            return;
        }
        let Some(which) = self.open_panel else {
            return;
        };
        egui::Panel::right("detail_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(220.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::side_top_panel(ui.style()).outer_margin(egui::Margin {
                    left: 4,
                    ..egui::Margin::ZERO
                }),
            )
            .show(ui, |ui| match which {
                SidePanel::TimeAndSales
                    if self
                        .own_data
                        .as_ref()
                        .is_some_and(|own| !own.profile.has_trades()) =>
                {
                    EmptyState::new("No trades")
                        .description(
                            "The data source maps no trades. A trades route in its Live step \
                             lists them here.",
                        )
                        .show(ui);
                }
                SidePanel::Alerts
                | SidePanel::Watchlist
                | SidePanel::OrderFlow
                | SidePanel::Highlights
                | SidePanel::Screener
                | SidePanel::Hotlists
                | SidePanel::MarketHeatmap
                | SidePanel::Calendar
                | SidePanel::News
                    if self.own_data.is_some() =>
                {
                    EmptyState::new("Synthetic data only")
                        .description(
                            "This panel shows the demo's synthetic market. Open Data source in \
                             the menu to switch back.",
                        )
                        .show(ui);
                }
                SidePanel::ObjectTree => self.show_object_tree(ui),
                SidePanel::Alerts => self.show_alerts_panel(ui),
                SidePanel::Watchlist => {
                    // The expanded details draw in place of the chart instead.
                    if ui.available_height() >= 400.0 && !self.details_expanded {
                        egui::Panel::bottom("symbol_details")
                            .resizable(true)
                            .default_size(ui.available_height() / 2.0)
                            .min_size(120.0)
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| self.symbol_details.show(ui));
                    }
                    let action = self.symbol_details.take_news_action();
                    self.handle_news(ui.ctx(), action);
                    self.show_watchlist(ui);
                }
                SidePanel::TimeAndSales => {
                    self.chart.request_trade_history();
                    self.time_and_sales.show(ui);
                }
                SidePanel::Depth => self.depth.show(ui, &mut self.chart),
                SidePanel::OrderFlow => {
                    for (kind, visible) in
                        self.order_flow
                            .show(ui, self.chart.symbol(), self.live.now())
                    {
                        self.chart
                            .set_signal_category_visible(kind.category(), visible);
                    }
                }
                SidePanel::Highlights => {
                    let action = self.highlights.show(ui);
                    self.handle_news(ui.ctx(), action);
                }
                SidePanel::Screener => match self.screener.show(ui) {
                    ScreenerAction::SelectSymbol(symbol) => self.open_catalog_symbol(&symbol),
                    ScreenerAction::ExportCsv(text) => self.save_text("screener.csv", &text),
                    ScreenerAction::SendToWatchlist(symbols) => {
                        self.watchlist.import("Screener", &symbols.join(","));
                        self.open_panel = Some(SidePanel::Watchlist);
                    }
                    _ => {}
                },
                SidePanel::MarketHeatmap => {
                    if let Some(symbol) = self.market_heatmap.show(ui) {
                        self.open_catalog_symbol(&symbol);
                    }
                }
                SidePanel::Hotlists => match self.hotlists.show(ui) {
                    HotlistsAction::SelectSymbol(symbol) => {
                        self.open_catalog_symbol(&symbol);
                    }
                    HotlistsAction::AddToWatchlist { list, symbol } => self.handle_context_action(
                        ui.ctx(),
                        ChartContextAction::Chart(ContextMenuAction::AddToWatchlist {
                            list,
                            symbol,
                        }),
                    ),
                    HotlistsAction::Starred(names) => self.hotlist_favorites = names,
                    HotlistsAction::CopyToWatchlist {
                        hotlist,
                        list: None,
                        symbols,
                    } => {
                        self.watchlist.import(hotlist, &symbols.join(","));
                        self.open_panel = Some(SidePanel::Watchlist);
                    }
                    HotlistsAction::CopyToWatchlist {
                        list: Some(list),
                        symbols,
                        ..
                    } if list < self.watchlist.lists().len() => {
                        let mut added = 0;
                        for symbol in &symbols {
                            added += usize::from(self.watchlist.add_symbol_to(list, symbol));
                        }
                        if added == 0 {
                            self.notify("These symbols are already in that watchlist");
                        }
                        self.watchlist.set_active_list(list);
                        self.open_panel = Some(SidePanel::Watchlist);
                    }
                    _ => {}
                },
                SidePanel::Notifications => {
                    let action = self.notifications.show(ui);
                    self.handle_notification(action);
                }
                SidePanel::Ideas => {
                    self.ideas
                        .panel
                        .set_context(&self.state.symbol, self.state.timeframe.as_str());
                    let action = self.ideas.panel.show(ui);
                    self.handle_ideas(ui.ctx(), action);
                }
                SidePanel::Chats => self.chats.show(ui),
                SidePanel::Products => {
                    if let ProductsAction::Open(id) = self.products.show(ui) {
                        match id.as_str() {
                            "screener" => self.open_panel = Some(SidePanel::Screener),
                            "calendar" => self.open_panel = Some(SidePanel::Calendar),
                            "news" => self.open_panel = Some(SidePanel::News),
                            _ => self.keyboard_shortcuts.toggle(),
                        }
                    }
                }
                SidePanel::Calendar => {
                    if let CalendarAction::SelectSymbol(symbol) = self.calendar.show(ui) {
                        self.open_catalog_symbol(&symbol);
                    }
                }
                SidePanel::News => {
                    let action = self.news.show(ui);
                    self.handle_news(ui.ctx(), action);
                }
                SidePanel::DataWindow => {
                    self.data_window_expanded = egui_charts::ui::widget_bar::show_data_window(
                        ui,
                        &self.build_data_window(),
                        self.data_window_expanded,
                    );
                }
            });
    }

    fn handle_news(&mut self, ctx: &egui::Context, action: NewsAction) {
        match action {
            NewsAction::ShowOnChart(time) => self.chart.scroll_to_time(time),
            NewsAction::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            NewsAction::None => {}
            _ => {}
        }
    }

    fn show_object_tree(&mut self, ui: &mut egui::Ui) {
        // Build the source list from live chart state each frame.
        let mut sources = self.build_source_items();
        let data_window = self.build_data_window();
        self.refresh_symbol_drawings();
        let action = self.object_tree.show(ui, Some(&data_window), &mut sources);
        if !self.handle_symbol_drawings(&action) {
            self.handle_object_tree(action, &sources);
        }
    }

    fn build_source_items(&self) -> Vec<SourceItem> {
        let mut items = Vec::new();

        // Main data source row.
        let tf = self.state.timeframe.as_str();
        let mut main =
            SourceItem::data_source(MAIN_SERIES_ID, format!("{} · {}", self.state.symbol, tf))
                .with_series_id(SeriesId::MAIN);
        main.visible = self.chart.is_main_series_visible();
        items.push(main);

        // Added bar series, including separate panes and spread expressions.
        for (id, options) in self.chart.series() {
            let mut item =
                SourceItem::data_source(indicator_tree_id(id), &options.name).with_series_id(id);
            item.visible = self.chart.is_series_visible(id);
            item.color = options.color;
            items.push(item);
        }

        // The tree lists higher layers first, so each pane reads from its front series to its back.
        let panes = self.chart.panes();
        items.sort_by_key(|item| {
            let id = item.series_id.expect("bar series row");
            (
                panes
                    .iter()
                    .position(|pane| Some(*pane) == self.chart.series_pane(id)),
                std::cmp::Reverse(self.chart.series_order(id)),
            )
        });
        let rows = items.len() as i32;
        for (row, item) in (0..).zip(&mut items) {
            item.z_index = rows - row;
        }

        // Live indicators.
        for (id, ind) in self.chart.indicators() {
            let color = ind.colors().first().copied().unwrap_or(egui::Color32::GRAY);
            let mut item =
                SourceItem::indicator(indicator_tree_id(id), ind.name(), color).with_series_id(id);
            item.visible = ind.is_visible();
            items.push(item);
        }

        // Live drawings.
        let all_locked = self.chart.drawings().is_locked();
        for d in self.chart.drawings().listed_drawings() {
            let mut item =
                SourceItem::drawing(d.id, d.tool_type, d.color32()).with_panes(&self.chart);
            item.visible = d.visible;
            item.locked = d.locked;
            item.user_edit_enabled = d.user_edit_enabled && !all_locked;
            item.z_index = d.z_order;
            item.label = d.label.clone();
            item.folder = d.folder.clone();
            item.folder_sync = d.folder_sync_mode();
            items.push(item);
        }
        items
    }

    /// Reads the hovered values from the crosshair payload; off the bars, the chart's own
    /// readout shows the latest bar.
    fn build_data_window(&self) -> DataWindowInfo {
        self.crosshair.as_ref().map_or_else(
            || DataWindowInfo::from_chart(&self.chart),
            |pointer| DataWindowInfo::from_pointer(&self.chart, pointer),
        )
    }

    fn handle_object_tree(&mut self, action: ObjectTreeAction, sources: &[SourceItem]) {
        self.user_action(|app| app.handle_object_tree_edit(action, sources));
    }

    fn handle_object_tree_edit(&mut self, action: ObjectTreeAction, sources: &[SourceItem]) {
        if self
            .object_tree
            .apply_to_chart(&mut self.chart, &action, sources)
        {
            return;
        }
        match action {
            ObjectTreeAction::ChangeColor(id, color) => {
                let chart = &mut self.chart;
                if let Some(study) = indicator_from_tree_id(id) {
                    if let Some(ind) = chart
                        .indicator_mut(study)
                        .filter(|study| study.user_edit_enabled())
                    {
                        ind.set_color(0, color);
                    } else {
                        let options = chart
                            .series()
                            .find(|(id, _)| *id == study)
                            .map(|(_, options)| options.clone());
                        if let Some(mut options) = options {
                            options.color = color;
                            chart.set_series_options(study, options);
                        }
                    }
                } else if chart
                    .drawings()
                    .drawings
                    .iter()
                    .any(|d| d.id == id && chart.drawings().accepts_user_edits(d))
                {
                    let _ = chart
                        .drawings_mut()
                        .drag_edit_drawing(id, |d| d.color = color.to_srgba_unmultiplied());
                    self.colour_drag = true;
                }
            }
            ObjectTreeAction::OpenProperties(id) | ObjectTreeAction::OpenIndicatorSettings(id) => {
                if id == MAIN_SERIES_ID {
                    self.open_chart_settings_dialog();
                } else if let Some(study) = indicator_from_tree_id(id) {
                    self.open_indicator_settings(study);
                } else {
                    self.open_drawing_props_for(id);
                }
            }
            _ => {}
        }
    }

    /// Publishes the quotes the watchlist subscribed to through the demo's quote service, and
    /// passes the chart symbol's quote on to the details panel and the order ticket. Feeds time
    /// and sales the executions of the live feed and plays the alert sound for matching size
    /// filters with sound.
    fn feed_market_panels(&mut self, ctx: &egui::Context) {
        let timezone: egui_charts::model::TimezoneId = self
            .timeframe_toolbar
            .state
            .timezone
            .parse()
            .unwrap_or_default();
        self.calendar.set_timezone(timezone.clone());
        self.news.set_timezone(timezone.clone());
        self.chats.set_timezone(timezone.clone());
        self.highlights.set_timezone(timezone.clone());
        self.symbol_details.set_timezone(timezone.clone());
        self.hotlists.set_timezone(timezone);
        if let Some(own) = &self.own_data {
            let (symbol, timeframe) = (self.chart.symbol(), self.chart.timeframe());
            let tape = own.feed.drain_tape(symbol, timeframe);
            // The details show synthetic figures, so they leave the chart's place.
            self.details_expanded = false;
            self.feed_time_and_sales(ctx, tape);
            return;
        }
        // The simulation clock rather than the wall clock, which the pause freezes: a paused
        // demo must be able to go idle instead of repainting for a market that stands still.
        let now = self.live.now();
        for entry in self
            .market_heatmap
            .universes
            .iter_mut()
            .flat_map(|universe| &mut universe.entries)
        {
            if let Some(quote) = self.live.market_quote(&entry.symbol, now) {
                let change = (quote.last - quote.prev_close) / quote.prev_close * 100.0;
                entry.values.insert("change".into(), change);
                entry.values.insert("price".into(), quote.last);
            }
        }
        let chart_symbol = self.chart.symbol().to_owned();
        let symbols: Vec<String> = self.watchlist.symbols().map(str::to_owned).collect();
        let mut selected = chart_symbol.clone();
        for symbol in symbols {
            if symbol.eq_ignore_ascii_case(&chart_symbol) {
                selected.clone_from(&symbol);
            }
            if let Some(decimals) = live::price_decimals(&symbol) {
                self.watchlist.set_price_decimals(&symbol, decimals);
            }
            if let Some(spec) = DemoSymbol::catalog()
                .iter()
                .find(|spec| spec.symbol.eq_ignore_ascii_case(&symbol))
            {
                self.watchlist.set_description(&symbol, spec.company_name);
            }
        }
        self.watchlist.set_selected(&selected);
        self.feed_paper_prices();
        if self.quotes.set_live(self.live.clone()) {
            self.news.set_feed(self.live.clone());
            self.highlights.set_feed(self.live.clone());
            self.screener.set_feed(self.live.clone());
            self.hotlists.set_feed(self.live.clone());
        }
        self.quotes.publish(now);
        self.watchlist.poll(ctx);
        self.symbol_details.set_instrument(
            &self
                .chart
                .instrument()
                .cloned()
                .unwrap_or_else(|| egui_charts::model::Instrument::new(&chart_symbol)),
        );
        if let Some(timeframe) = self.chart.timeframe()
            && !self.loading_bars
        {
            self.symbol_details
                .set_bars(timeframe, self.chart.bars(SeriesId::MAIN));
        }
        if let Some(quote) = self.live.quote(now) {
            self.symbol_details.set_quote(quote.clone());
            self.trading.set_quote(&chart_symbol, quote);
        }
        self.symbol_details
            .set_average_volume(self.live.daily_volume().1);
        // Period returns and seasonality look back over the whole history, so wait for it.
        let requests = if self.loading_bars {
            Vec::new()
        } else {
            self.symbol_details.take_market_requests()
        };
        for request in requests {
            use egui_charts::ui::widget_bar::{MarketDetail, MarketDetailRequest};
            let bars = self.market_history();
            match request {
                MarketDetailRequest::PeriodReturns(symbol) => self
                    .symbol_details
                    .set_period_returns(&symbol, MarketDetail::Ready(feeds::period_returns(&bars))),
                MarketDetailRequest::Seasonality(symbol) => self
                    .symbol_details
                    .set_seasonality(&symbol, MarketDetail::Ready(feeds::seasonality(&bars))),
                MarketDetailRequest::Section(section, symbol) => self
                    .symbol_details
                    .set_detail_rows(&symbol, section, feeds::detail_rows(section, &symbol)),
                _ => {}
            }
        }
        if let Some(url) = self
            .symbol_details
            .take_opened_row()
            .and_then(|row| row.url)
        {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        self.feed_financials();
        if self.symbol_details.take_expand_request().is_some() {
            self.details_expanded = true;
            self.financials = None;
            self.open_panel = None;
        }
        // Hidden before the larger view first draws, so it never offers itself.
        self.symbol_details
            .set_expand_visible(!self.details_expanded);
        self.highlights.set_symbol(&chart_symbol);
        self.news.set_symbol(&chart_symbol);
        let hour = now.timestamp().div_euclid(3600);
        if self.news_hour != Some(hour) {
            self.news_hour = Some(hour);
            self.news.refresh();
            self.highlights.refresh();
        }
        let tape = self.live.drain_tape();
        self.feed_time_and_sales(ctx, tape);
    }

    /// Lists `tape`, the trades the chart's feed received since the last frame, in time and
    /// sales, plays the alert sound once when a size filter with sound matched, and formats the
    /// panel for the chart's instrument and the chosen zone.
    fn feed_time_and_sales(&mut self, ctx: &egui::Context, tape: Vec<Tick>) {
        let instrument = self.chart.instrument();
        // As the price scale writes it: the instrument's decimals, else those of its tick size.
        let decimals = instrument.and_then(egui_charts::scales::instrument_price_precision);
        let mut matched = false;
        for trade in tape {
            matched |= self.time_and_sales.push(trade).is_some();
        }
        let config = &mut self.time_and_sales.config;
        config.price_decimals = decimals;
        config.size_decimals = instrument.and_then(|instrument| instrument.volume_decimals);
        config.timezone = self
            .timeframe_toolbar
            .state
            .timezone
            .parse()
            .unwrap_or_default();
        if matched {
            let alerts = &self.settings_state.alerts;
            let (name, volume) = (alerts.sound.clone(), alerts.sound_volume);
            self.play_sound(ctx, &name, volume, false);
        }
    }

    /// Saves exported `text` to `file` in the temporary directory, or downloads it on the web.
    fn save_text(&mut self, file: &str, text: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        let path = std::env::temp_dir().join(file);
        #[cfg(target_arch = "wasm32")]
        let path = std::path::PathBuf::from(file);
        #[cfg(not(target_arch = "wasm32"))]
        let result = std::fs::write(&path, text).map_err(|e| e.to_string());
        #[cfg(target_arch = "wasm32")]
        let result = browser_download(text.as_bytes(), &path);
        self.notify(match result {
            Ok(()) => format!("Export successful: {}", path.display()),
            Err(error) => format!("Export failed: {error}"),
        });
    }

    /// Shows `symbol` on the chart when the demo has data for it.
    fn open_catalog_symbol(&mut self, symbol: &str) {
        if self.own_data.is_some() {
            self.switch_symbol(symbol);
            return;
        }
        if DemoSymbol::catalog()
            .iter()
            .any(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
        {
            self.switch_symbol(symbol);
        } else {
            self.notify(format!("{symbol} has no synthetic data in this demo"));
        }
    }

    /// Shows the notes of `symbol` with their field focused, beneath the watchlist or in the
    /// expanded details, and keeps the chart on its symbol.
    fn add_note(&mut self, symbol: &str) {
        if self.synthetic_only("Symbol notes") {
            return;
        }
        self.financials = None;
        if !self.details_expanded {
            self.open_panel = Some(SidePanel::Watchlist);
        }
        self.symbol_details.focus_notes(symbol);
    }

    fn show_watchlist(&mut self, ui: &mut egui::Ui) {
        let action = self.watchlist.show(ui);
        self.handle_watchlist_action(ui.ctx(), action);
    }

    /// What the compact and the wide watchlist ask of the host.
    fn handle_watchlist_action(&mut self, ctx: &egui::Context, action: WatchlistAction) {
        match action {
            WatchlistAction::SelectSymbol(symbol) => self.open_catalog_symbol(&symbol),
            WatchlistAction::AddNote(symbol) => self.add_note(&symbol),
            WatchlistAction::AddSymbol => {
                self.watchlist_search = true;
                self.symbol_search.open();
            }
            WatchlistAction::AdvancedView => self.watchlist_wide = true,
            WatchlistAction::CompactView => self.watchlist_wide = false,
            WatchlistAction::Import => self.pick_watchlist_file(ctx),
            WatchlistAction::Export { text, .. } => self.save_text("watchlist.txt", &text),
            WatchlistAction::None => {}
            _ => {}
        }
    }

    /// The wide watchlist in a window, with the synthetic earnings, dividends and news of its
    /// symbols. It shares the compact panel's selection and sorting.
    fn show_wide_watchlist(&mut self, ctx: &egui::Context) {
        use egui_charts::data::{CalendarEvent, CalendarKind, NewsStory};
        use egui_charts::ui::widget_bar::{MarketDetail, WatchlistDetails};
        if !self.watchlist_wide {
            return;
        }
        let now = chrono::Utc::now();
        let mut details = WatchlistDetails::default();
        let (mut earnings, mut dividends, mut news) = (Vec::new(), Vec::new(), Vec::new());
        for (n, symbol) in self.watchlist.symbols().enumerate() {
            let days = chrono::Duration::days(n as i64 % 20 + 1);
            let event = |kind, time, title: String| {
                let mut event = CalendarEvent::new(format!("{symbol}-{kind:?}"), kind, time, title);
                event.symbol = Some(symbol.to_owned());
                event
            };
            let mut report = event(
                CalendarKind::Earnings,
                now + days,
                format!("{symbol} quarterly results"),
            );
            (report.forecast, report.prior) = (Some("1.24".into()), Some("1.17".into()));
            earnings.push(report);
            let mut payout = event(
                CalendarKind::Dividends,
                now + days * 2,
                format!("{symbol} dividend"),
            );
            payout.forecast = Some("0.42".into());
            dividends.push(payout);
            let mut story = NewsStory::new(
                format!("{symbol}-story"),
                now - chrono::Duration::hours(n as i64 + 1),
                format!("{symbol} extends its range as volume rises"),
            );
            story.source = "Demo wire".into();
            story.symbols = vec![symbol.to_owned()];
            news.push(story);
        }
        details.earnings = MarketDetail::Ready(earnings);
        details.dividends = MarketDetail::Ready(dividends);
        details.news = MarketDetail::Ready(news);
        let mut open = true;
        let mut action = WatchlistAction::None;
        egui::Window::new("Watchlist")
            .open(&mut open)
            .default_size([900.0, 520.0])
            .show(ctx, |ui| action = self.watchlist.show_wide(ui, &details));
        self.handle_watchlist_action(ctx, action);
        self.watchlist_wide &= open;
    }

    /// Asks for a `.txt` watchlist file, which [`Self::poll_watchlist_file`] imports.
    fn pick_watchlist_file(&mut self, ctx: &egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.watchlist_import = Some(rx);
        let ctx = ctx.clone();
        let too_large = |name: &str| format!("{name} exceeds the 1 MiB watchlist limit");
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            use std::io::Read;
            let result = (|| {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Text", &["txt"])
                    .pick_file()
                else {
                    return Ok(None);
                };
                let name = path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .and_then(|file| file.take(MAX_WATCHLIST_BYTES + 1).read_to_end(&mut bytes))
                    .map_err(|error| format!("Could not read {name}: {error}"))?;
                if bytes.len() as u64 > MAX_WATCHLIST_BYTES {
                    return Err(too_large(&name));
                }
                Ok(Some((name, bytes)))
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = match rfd::AsyncFileDialog::new()
                .add_filter("Text", &["txt"])
                .pick_file()
                .await
            {
                None => Ok(None),
                Some(file) if file.inner().size() > MAX_WATCHLIST_BYTES as f64 => {
                    Err(too_large(&file.file_name()))
                }
                Some(file) => Ok(Some((file.file_name(), file.read().await))),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    /// Imports the chosen watchlist file as a list named after the file without its extension.
    fn poll_watchlist_file(&mut self) {
        let Some(rx) = self.watchlist_import.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(Some((file, bytes)))) => match String::from_utf8(bytes) {
                Ok(text) => {
                    let name = std::path::Path::new(&file)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::trim)
                        .filter(|stem| !stem.is_empty())
                        .unwrap_or("Imported");
                    self.watchlist.import(name, &text);
                }
                Err(_) => self.notify(format!("{file} is not UTF-8 text")),
            },
            Ok(Ok(None)) => {}
            Ok(Err(error)) => self.notify(error),
            Err(std::sync::mpsc::TryRecvError::Empty) => self.watchlist_import = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.notify("Watchlist selection failed")
            }
        }
    }

    /// The card of the chat author the user asked for; Message opens a direct chat with them.
    fn show_profile(&mut self, ctx: &egui::Context) {
        let Some(id) = self.profile_open.clone() else {
            return;
        };
        let mut open = true;
        let mut action = ProfileAction::None;
        egui::Window::new("Profile")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                action = ProfileCard::new(&id)
                    .messaging(true)
                    .show(ui, &mut self.profiles);
            });
        if action == ProfileAction::Message {
            let name = self
                .profiles
                .profile(&id)
                .map_or_else(|| id.0.clone(), |profile| profile.name.clone());
            self.chats.start_direct(ChatUser::new(id.0, name));
            self.open_panel = Some(SidePanel::Chats);
            open = false;
        }
        if !open {
            self.profile_open = None;
        }
    }

    fn show_alerts_panel(&mut self, ui: &mut egui::Ui) {
        self.list_alert_sources();
        match self.alerts.show(ui) {
            AlertPanelAction::CreateAlert => self.open_alert(None),
            AlertPanelAction::EditAlert(alert) | AlertPanelAction::DuplicateAlert(alert) => {
                self.edit_alert(*alert)
            }
            AlertPanelAction::ExportLog(csv) => {
                #[cfg(not(target_arch = "wasm32"))]
                let path = std::env::temp_dir().join("alerts.csv");
                #[cfg(target_arch = "wasm32")]
                let path = std::path::PathBuf::from("alerts.csv");
                #[cfg(not(target_arch = "wasm32"))]
                let result = std::fs::write(&path, csv).map_err(|e| e.to_string());
                #[cfg(target_arch = "wasm32")]
                let result = browser_download(csv.as_bytes(), &path);
                self.notify(match result {
                    Ok(()) => format!("Export successful: {}", path.display()),
                    Err(error) => format!("Export failed: {error}"),
                });
            }
            AlertPanelAction::None => {}
            _ => {}
        }
    }

    fn edit_alert(&mut self, mut alert: Alert) {
        self.list_alert_sources();
        self.chart.prepare_alert(&mut alert);
        self.alert_dialog.edit(alert);
    }

    /// Advance the sample clock explicitly while preparing a paused session.
    #[allow(dead_code, reason = "used by the documentation captures")]
    pub(crate) fn advance_sample_to(&mut self, time: chrono::DateTime<chrono::Utc>) {
        self.live.set_paused(false);
        let mut cursor = self.live.now();
        while cursor < time {
            cursor = (cursor + chrono::Duration::seconds(1)).min(time);
            self.live.advance(cursor);
            self.advance_live_strategy_source(cursor);
        }
        self.live.set_paused(true);
    }

    /// The paper account that trades: the replay session's during replay, else the live one.
    pub(crate) fn active_paper(&self) -> &Arc<egui_charts::services::PaperAccount> {
        self.replay_paper.as_ref().unwrap_or(&self.paper)
    }

    /// Connects the paper account and opens its panel, as its tile in the broker picker does.
    pub(crate) fn connect_paper(&mut self) {
        self.trading.set_service(self.active_paper().clone());
        self.trading.account_panel.broker = "Paper Trading".to_owned();
        self.trading.account_panel.accounts = vec!["Paper 1".to_owned()];
        self.account_panel = AccountPanelSize::Normal;
    }

    /// The account panel of the connected broker, under the chart and its interval bar.
    fn show_account_panel(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        use egui_charts::ui::trading::{AccountPanelAction, BrokerPickerAction, BrokerTile};

        match self.broker_picker.show(ui.ctx()) {
            Some(BrokerPickerAction::Connect(id)) if id == BrokerTile::PAPER => {
                self.connect_paper()
            }
            Some(BrokerPickerAction::Connect(_)) => {
                self.notify("Example brokers have no account behind them in this demo");
            }
            Some(BrokerPickerAction::NeedBroker) => {
                self.notify("Host action: show how to open a broker account");
            }
            _ => {}
        }
        if self.paper_reset {
            let result = egui_charts::ui_kit::modal::dialogs::ConfirmDialog::new(
                "paper_reset",
                "Reset paper account",
                "Close every position, cancel every order and return the balance to its starting amount?",
            )
            .confirm_text("Reset")
            .show(ui.ctx(), &mut self.paper_reset);
            if result.is_confirmed() {
                self.active_paper().reset();
                self.trading.set_service(self.active_paper().clone());
                self.paper_reset = false;
            }
        }
        if !self.trading.has_broker() || self.account_panel == AccountPanelSize::Closed {
            return;
        }
        let available = ui.available_height();
        let height = match self.account_panel {
            AccountPanelSize::Minimized => 42.0,
            AccountPanelSize::Maximized => (available - 160.0).max(self.account_panel_height),
            _ => self.account_panel_height,
        };
        let height = height.min((available - 96.0).max(42.0));
        let symbol = self.chart.symbol().to_owned();
        let trading = &mut self.trading;
        let panel = egui::Panel::bottom("account_panel")
            .exact_size(height)
            // The dock's own edge below resizes it; the panel's would take the drag.
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(bg).corner_radius(4).outer_margin(egui::Margin {
                top: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| trading.show_account_panel(ui, &symbol));
        if self.account_panel == AccountPanelSize::Normal {
            // The 4 point gap over the panel is its top edge; its middle stands for the height.
            let rect = panel.response.rect;
            let edge = egui::Rect::from_x_y_ranges(rect.x_range(), rect.top()..=rect.top() + 4.0);
            let edge = ui
                .interact(
                    edge,
                    ui.scope_id().with("account_panel_edge"),
                    egui::Sense::drag(),
                )
                .on_hover_and_drag_cursor(egui::CursorIcon::ResizeVertical);
            edge.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Splitter, true, "Resize panel")
            });
            if let Some(pointer) = edge.interact_pointer_pos().filter(|_| edge.dragged()) {
                let most = (available - 96.0).max(42.0);
                self.account_panel_height = (rect.bottom() + 2.0 - pointer.y).clamp(42.0, most);
            }
        }
        match panel.inner {
            Some(AccountPanelAction::SelectSymbol(symbol)) => self.open_catalog_symbol(&symbol),
            Some(AccountPanelAction::Disconnect) => self.trading.disconnect(),
            Some(AccountPanelAction::ChangeBroker) => self.broker_picker.open(),
            Some(AccountPanelAction::Settings) => self.paper_reset = true,
            Some(AccountPanelAction::Minimize) => {
                self.account_panel = if self.account_panel == AccountPanelSize::Minimized {
                    AccountPanelSize::Normal
                } else {
                    AccountPanelSize::Minimized
                };
            }
            Some(AccountPanelAction::Maximize) => {
                self.account_panel = if self.account_panel == AccountPanelSize::Maximized {
                    AccountPanelSize::Normal
                } else {
                    AccountPanelSize::Maximized
                };
            }
            Some(AccountPanelAction::Export { name, csv }) => {
                let file = format!("{}.csv", name.to_lowercase().replace(' ', "_"));
                self.save_text(&file, &csv);
            }
            _ => {}
        }
    }

    fn show_bottom_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        egui::Panel::bottom("bottom")
            .exact_size(39.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).corner_radius(4).outer_margin(egui::Margin {
                top: 1,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                self.timeframe_toolbar.state.date_picker_open = self.date_picker.is_open;
                // The clock reads the zone of the time axis, however it was chosen.
                self.timeframe_toolbar
                    .state
                    .set_display_timezone(&self.chart.display_timezone());
                let action = self.timeframe_toolbar.show(ui);
                self.handle_timeframe_toolbar(action);
                self.show_timeframe_menus(ui.ctx());
            });
    }

    /// Switches to the preset's interval when offered, then fits its newly loaded bars.
    fn show_date_range(&mut self, range: DateRange) {
        let timeframe = range.timeframe();
        let offered = self
            .own_data
            .as_ref()
            .is_none_or(|own| own.profile.service_interval(timeframe).is_some());
        self.range_fit = None;
        if offered && self.state.timeframe != timeframe {
            self.switch_timeframe(timeframe);
            self.range_fit = Some((range, timeframe, RangeFit::Interval));
        } else {
            self.fit_date_range(range);
            if self.own_data.is_none() {
                self.explain_history_limit(range);
            }
            self.range_fit = Some((range, self.state.timeframe, RangeFit::Fitting));
        }
    }

    /// Fits the lookback to the plot and sends its span to the linked charts.
    fn fit_date_range(&mut self, range: DateRange) {
        let visible = visible_bars_for_range(range, self.chart.bars(SeriesId::MAIN));
        let width = self.chart.get_rendered_price_rect().width();
        // Include the latest slot in the scale's exclusive data range.
        let spacing = (width - 1.0).max(1.0) / (visible.max(1) as f32 + 0.5);
        self.chart.disable_tracking_mode();
        self.chart.apply_synced_time_scale(spacing, 0.5);
        if (self.layout.sync.sync_date_range || self.layout.sync.sync_time_axis)
            && let Some(range) = self.chart.visible_time_range()
        {
            let mut charts: Vec<_> = self
                .extra_charts
                .iter_mut()
                .map(|c| &mut *c.chart)
                .collect();
            charts.insert(self.layout_slot, &mut self.chart);
            self.layout.apply_date_range(&mut charts, range);
        }
    }

    fn handle_timeframe_toolbar(&mut self, action: TimeframeToolbarAction) {
        match action {
            TimeframeToolbarAction::DateRangeChanged(range) => {
                self.user_action(|app| app.show_date_range(range));
            }
            TimeframeToolbarAction::OpenDatePicker => self.date_picker.open_for_chart(&self.chart),
            TimeframeToolbarAction::OpenTimezoneMenu => {
                self.timeframe_toolbar
                    .set_time_zones(self.chart_time_zones());
                self.timeframe_toolbar.state.timezone_menu_open = true;
            }
            TimeframeToolbarAction::OpenSessionMenu => {
                self.timeframe_toolbar.state.session_menu_open = true;
            }
            TimeframeToolbarAction::AdjustDividendsToggled(adjusted) => {
                self.timeframe_toolbar.state.adjust_for_dividends = adjusted;
                self.stop_replay();
                self.apply_session();
            }
            TimeframeToolbarAction::None => {}
            _ => {}
        }
    }

    fn show_timeframe_menus(&mut self, ctx: &egui::Context) {
        if self.timeframe_toolbar.state.session_menu_open {
            let mut open = true;
            egui::Window::new("Synthetic session")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    for session in [SessionType::RTH, SessionType::ETH] {
                        if ui
                            .selectable_label(
                                self.timeframe_toolbar.state.session_type == session,
                                session.tooltip(),
                            )
                            .clicked()
                        {
                            self.timeframe_toolbar.set_session(session);
                            self.timeframe_toolbar.state.session_menu_open = false;
                            self.stop_replay();
                            self.apply_session();
                        }
                    }
                    let hours = us_equity_session();
                    let (regular, extended) = (&hours.windows[0], &hours.extended_windows);
                    ui.label(format!(
                        "Weekdays in New York time: regular {}–{}; extended {}–{}.",
                        regular.open.format("%H:%M"),
                        regular.close.format("%H:%M"),
                        extended[0].open.format("%H:%M"),
                        extended[1].close.format("%H:%M"),
                    ));
                });
            self.timeframe_toolbar.state.session_menu_open &= open;
        }
        let current = self.chart.chart_options().time_scale.timezone.clone();
        if let Some(timezone) = self.timeframe_toolbar.show_timezone_menu(ctx, &current) {
            self.user_action(|app| {
                apply_timezone_mode(&mut app.chart, &mut app.timeframe_toolbar, timezone)
            });
        }
    }

    fn show_chart(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        if let Some((_, financials)) = &mut self.financials {
            self.export_dialog.set_chart_size(None);
            self.export_dialog.set_layout_size(None);
            let frame = panel_frame(bg).corner_radius(4).inner_margin(16);
            let back = egui::CentralPanel::default()
                .frame(frame)
                .show(ui, |ui| financials.show(ui));
            if back.inner {
                self.financials = None;
            }
            return;
        }
        if self.details_expanded {
            self.export_dialog.set_chart_size(None);
            self.export_dialog.set_layout_size(None);
            let frame = panel_frame(bg).corner_radius(4).inner_margin(16);
            let back = egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                // A centred column stays readable where rows across the whole chart area do not.
                let width = ui.available_width().min(760.0);
                ui.horizontal_top(|ui| {
                    ui.add_space((ui.available_width() - width) / 2.0);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        let back = ui
                            .horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| ui.button("Back to chart").clicked(),
                                )
                                .inner
                            })
                            .inner;
                        self.symbol_details.show(ui);
                        back
                    })
                    .inner
                })
                .inner
            });
            let action = self.symbol_details.take_news_action();
            self.handle_news(ui.ctx(), action);
            if back.inner {
                self.details_expanded = false;
            }
            return;
        }
        let chart = egui::CentralPanel::default()
            .frame(panel_frame(bg).corner_radius(4))
            .show(ui, |ui| {
                let loading = self.live.loading()
                    || self.comparisons.iter().any(|(_, id, feed)| {
                        self.chart.series_order(*id).is_some() && feed.loading()
                    })
                    || self
                        .spreads
                        .iter()
                        .any(|spread| spread.loading(&self.chart))
                    || self.extra_charts.iter().enumerate().any(|(index, extra)| {
                        slot_shown(&self.layout, self.layout_slot, index)
                            && (extra.live.loading()
                                || extra.comparisons.iter().any(|(_, id, feed)| {
                                    extra.chart.series_order(*id).is_some() && feed.loading()
                                })
                                || extra
                                    .spreads
                                    .iter()
                                    .any(|spread| spread.loading(&extra.chart)))
                    });
                // Paused charts still need frames to finish their comparison and spread feeds.
                if loading {
                    ui.ctx().request_repaint();
                }
                if self.api_samples
                    && let Some(pointer) = &self.pointer_readout
                {
                    Self::show_pointer_readout(ui, pointer);
                }
                let symbols: Vec<_> = self
                    .extra_charts
                    .iter()
                    .map(|c| c.chart.symbol().to_owned())
                    .collect();
                let mut charts: Vec<_> = self
                    .extra_charts
                    .iter_mut()
                    .map(|c| &mut *c.chart)
                    .collect();
                charts.insert(self.layout_slot, &mut self.chart);
                // Only this symbol's feed prepares footprint rows, so its tape can wait for a reader
                // and its first footprint page can hold only the bars on view.
                for chart in &mut charts {
                    let prepared = chart.symbol() == live::PREPARED_FOOTPRINT_SYMBOL;
                    chart.set_trade_history_on_demand(prepared);
                    chart.set_footprint_history_by_view(prepared);
                }
                // Each layout tab has a layout of its own: the one on view takes the bindings.
                if self.layout.shortcuts() != self.shortcuts.as_ref() {
                    self.layout.set_shortcuts(self.shortcuts.clone());
                }
                let rect = self.layout.show(ui, &mut charts).rect;
                let visible = self.layout.visible_chart_rects(rect, charts.len());
                self.export_dialog.set_chart_size(
                    visible
                        .iter()
                        .find(|(slot, _)| *slot == self.layout.active())
                        .map(|(_, rect)| rect.size()),
                );
                self.export_dialog
                    .set_layout_size((visible.len() > 1).then_some(rect.size()));
                // A chart that follows a linked symbol change shows the bars it would show had
                // the symbol been chosen on it.
                for (extra, symbol) in self.extra_charts.iter_mut().zip(symbols) {
                    if extra.chart.symbol() != symbol && extra.chart.replay_driver().is_none() {
                        extra.live.set_symbol(extra.chart.symbol());
                        let session = opening_session(extra.chart.symbol());
                        let (_, adjusted) = extra.live.view(false);
                        load_view(&mut extra.chart, &extra.live, session, adjusted);
                    }
                }
                for request in self.layout.take_tab_requests() {
                    self.apply_tab_request(request);
                }
                self.highlight_strategy_trade(ui);
                let mapping = self.chart.get_chart_mapping();
                if self.own_data.is_none() {
                    let series = self.chart.series_settings().alerts;
                    let chart_wide = &self.settings_state.alerts;
                    if chart_wide.show_alerts && series.visible {
                        let color = series.color.unwrap_or(chart_wide.alert_color);
                        let decimals = self
                            .chart
                            .instrument()
                            .and_then(egui_charts::scales::instrument_price_precision)
                            .map(usize::from)
                            .unwrap_or(2);
                        let currency = self
                            .chart
                            .instrument()
                            .and_then(|i| i.currency.as_deref())
                            .unwrap_or("");
                        if let Some(alert) = egui_charts::chart::render_alert_levels(
                            ui,
                            &mapping,
                            self.alerts.alerts().iter().filter(|a| {
                                a.symbol == self.chart.symbol() && a.watchlist.is_none()
                            }),
                            color,
                            chart_wide.show_alert_labels && series.labels,
                            chart_wide.show_active_only,
                            |price| format!("{price:.decimals$} {currency}"),
                        ) {
                            self.alerts.save(alert);
                        }
                    }
                }
                if !self.keyboard_dialog_open() {
                    self.handle_keyboard(ui.ctx());
                }
                if let Some(id) = self.chart.take_indicator_settings_request() {
                    self.open_indicator_settings(id);
                }
                if self.chart.take_scale_settings_request() {
                    self.open_chart_settings_dialog();
                }
                if let Some(id) = self.chart.take_event_click() {
                    if let Some(story) = marks::news_story(&id) {
                        self.news.open_story(story);
                        self.open_panel = Some(SidePanel::News);
                    } else {
                        self.notify("Synthetic session: generated example data");
                    }
                }
                if (self.replay.is_some() || self.own_data.is_some())
                    && let Some(bar) = self
                        .chart
                        .bars(SeriesId::MAIN)
                        .iter()
                        .rfind(|bar| !bar.is_whitespace())
                {
                    self.active_paper()
                        .set_price(self.chart.symbol(), bar.close);
                    // Replay has no historical spread; its close prices both broker and ticket.
                    self.trading.set_quote(
                        self.chart.symbol(),
                        egui_charts::model::Quote::new(bar.close, bar.close, bar.time),
                    );
                }
                if let Some(side) = self.chart.take_quote_action() {
                    if !self.trading.has_broker() {
                        self.broker_picker.open();
                    } else {
                        match side {
                            egui_charts::widget::QuoteSide::Ticket => {
                                self.trading
                                    .open_ticket(egui_charts::model::OrderRequest::new(
                                        self.chart.symbol(),
                                        egui_charts::model::OrderSide::Buy,
                                        self.trading.quantity,
                                        egui_charts::model::OrderType::Market,
                                    ));
                            }
                            egui_charts::widget::QuoteSide::Buy => self.trading.trade(
                                self.chart.symbol().to_owned(),
                                egui_charts::model::OrderSide::Buy,
                            ),
                            egui_charts::widget::QuoteSide::Sell => self.trading.trade(
                                self.chart.symbol().to_owned(),
                                egui_charts::model::OrderSide::Sell,
                            ),
                            // A side added later has no action in the demo yet.
                            _ => {}
                        }
                    }
                }
                self.trading.show(ui, &mut self.chart);
                self.show_sample_fill(ui);
            });
        // The chart's rectangular background covers the frame; restore its top-left cutout.
        let corner = chart.response.rect.min;
        let radius = 4.0;
        let points = (0..=8)
            .map(|step| {
                let angle = std::f32::consts::PI + step as f32 * std::f32::consts::FRAC_PI_2 / 8.0;
                let x = corner.x + radius * (1.0 + angle.cos());
                let y = corner.y + radius * (1.0 + angle.sin());
                egui::epaint::BandPoint::new(x, corner.y..=y.max(corner.y))
            })
            .collect();
        ui.painter()
            .add(egui::Shape::Band(egui::epaint::BandShape::filled(
                points,
                self.theme.ui.border,
            )));
        if !self.replay_selecting && self.replay.is_none() {
            self.activate_chart_slot(self.layout.active());
        }
        self.drain_chart_events(ui.ctx());
        if self.loading_bars
            && self.replay.is_none()
            && !self.live.loading()
            && !self.chart.bars(SeriesId::MAIN).is_empty()
        {
            self.bars = self.chart.bars(SeriesId::MAIN).to_vec();
            self.loading_bars = false;
            self.size_range_interval();
            self.annotate();
        }
        if !self.keyboard_dialog_open()
            && let Some(action) = self.selection_toolbar.show_and_apply(ui, &mut self.chart)
        {
            self.handle_drawing_context(ui.ctx(), action);
        }
        self.menus
            .set_order_entries(self.trading.quantity, self.trading.capabilities());
        self.menus
            .set_watchlists(self.watchlist.lists().iter().map(|list| list.name.as_str()));
        self.menus.set_active_watchlist(
            self.watchlist
                .flag_filter()
                .is_none()
                .then_some(self.watchlist.active_list()),
        );
        self.hotlists
            .set_watchlists(self.watchlist.lists().iter().map(|list| list.name.as_str()));
        self.menus.set_layout_copies(self.layout_has_other_charts());
        if let Some(action) = self.menus.show(ui.ctx(), &mut self.chart) {
            self.handle_context_action(ui.ctx(), action);
        }
    }

    fn drain_chart_events(&mut self, ctx: &egui::Context) {
        let mut replay_click = None;
        let mut open_settings = false;
        let mut drawing_settings = None;
        let mut size = false;
        let mut fit = false;
        for event in self.chart.drain_events() {
            match event {
                egui_charts::widget::ChartEvent::ImageRequested(request) => {
                    self.images.request(ctx, self.layout_slot, request);
                }
                egui_charts::widget::ChartEvent::CrosshairMoved(pointer) => {
                    if pointer.is_some() {
                        self.pointer_readout = pointer.clone();
                    }
                    self.crosshair = pointer;
                    // The data window was drawn before the chart; show the new values now.
                    ctx.request_repaint();
                }
                egui_charts::widget::ChartEvent::Click { position, button } => {
                    if button == egui::PointerButton::Primary {
                        replay_click.get_or_insert(position.pos);
                    }
                    self.last_chart_click = Some(position);
                }
                egui_charts::widget::ChartEvent::DoubleClick { position }
                    if matches!(
                        position.target,
                        Some(egui_charts::widget::PointerTarget::Element(
                            egui_charts::widget::ChartElementId::Series(SeriesId::MAIN)
                        ))
                    ) =>
                {
                    open_settings = true;
                }
                egui_charts::widget::ChartEvent::DoubleClick { position } => {
                    if let Some(egui_charts::widget::PointerTarget::Drawing(id)) = position.target {
                        drawing_settings = Some(id);
                    }
                }
                egui_charts::widget::ChartEvent::TimeframeChanged(timeframe) => {
                    // Skip older queued switches; after ours, another interval cancels the fit.
                    self.range_fit = self
                        .range_fit
                        .filter(|(_, interval, stage)| {
                            matches!(stage, RangeFit::Interval) || *interval == timeframe
                        })
                        .map(|(range, interval, stage)| {
                            let stage =
                                if interval == timeframe && matches!(stage, RangeFit::Interval) {
                                    RangeFit::Bars
                                } else {
                                    stage
                                };
                            (range, interval, stage)
                        });
                    self.last_chart_click = None;
                    self.pointer_readout = None;
                    if let Some(own) = &mut self.own_data {
                        own.sizing = true;
                    }
                }
                egui_charts::widget::ChartEvent::SymbolChanged(_) => {
                    self.last_chart_click = None;
                    self.pointer_readout = None;
                    if let Some(own) = &mut self.own_data {
                        own.sizing = true;
                    }
                }
                // The first bars of a symbol or interval of own data size a range interval, as
                // the end of a synthetic load does.
                egui_charts::widget::ChartEvent::DataLoaded {
                    series: SeriesId::MAIN,
                    bars,
                } if bars > 0 => {
                    fit |= matches!(self.range_fit, Some((_, _, RangeFit::Bars)));
                    if let Some(own) = &mut self.own_data
                        && std::mem::take(&mut own.sizing)
                    {
                        size = true;
                    }
                }
                egui_charts::widget::ChartEvent::LinkClick { link, .. } => {
                    // Any typed text can be a link; open web addresses only.
                    let web = link.split_once("://").is_some_and(|(scheme, _)| {
                        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
                    });
                    if web {
                        ctx.open_url(egui::OpenUrl::new_tab(link));
                    }
                }
                egui_charts::widget::ChartEvent::LockedDrawingsKept(kept) => {
                    self.toasts.info(locked_drawings_kept(kept));
                }
                _ => {}
            }
        }
        if size {
            self.size_range_interval();
        }
        if let Some((range, interval, stage)) = self.range_fit
            && !self.chart.bars(SeriesId::MAIN).is_empty()
            && (fit || matches!(stage, RangeFit::Fitting | RangeFit::Confirming(_)))
        {
            let before = self.chart.get_time_scale_state();
            self.fit_date_range(range);
            if fit && self.own_data.is_none() {
                self.explain_history_limit(range);
            }
            let unchanged = before == self.chart.get_time_scale_state();
            if unchanged
                && matches!(stage, RangeFit::Confirming(previous) if previous != ctx.cumulative_frame_nr())
            {
                self.range_fit = None;
            } else {
                // Fitting can widen the price-axis gutter on the next frame.
                let stage = if unchanged {
                    match stage {
                        RangeFit::Confirming(frame) => RangeFit::Confirming(frame),
                        _ => RangeFit::Confirming(ctx.cumulative_frame_nr()),
                    }
                } else {
                    RangeFit::Fitting
                };
                self.range_fit = Some((range, interval, stage));
                ctx.request_repaint();
            }
        }
        if let Some(index) = replay_click.and_then(|pos| self.replay_start_bar(pos)) {
            let time = self.chart.state().shown()[index].time;
            self.handle_replay_bar_click(ctx, time);
        }
        if open_settings && !self.replay_selecting {
            self.open_chart_settings_dialog();
        }
        if let Some(id) = drawing_settings
            && !self.replay_selecting
            && self.chart.drawings().drawings.iter().any(|d| {
                d.id == id
                    && (!self.chart.drawings().accepts_user_edits(d)
                        || !d.tool_type.edits_text_inline())
            })
        {
            self.open_drawing_props_for(id);
        }
    }

    /// A host legend and tooltip built entirely from the last pointer event.
    fn show_pointer_readout(ui: &mut egui::Ui, pointer: &egui_charts::widget::ChartPointer) {
        let Some(bar) = pointer.bar.as_ref().filter(|bar| !bar.is_whitespace()) else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "O {:.2} H {:.2} L {:.2} C {:.2} V {:.0}",
                bar.open, bar.high, bar.low, bar.close, bar.volume
            ))
            .on_hover_ui(|ui| {
                if let Some(time) = pointer.time {
                    ui.label(time.to_rfc3339());
                }
                ui.label(format!("Bar {} · Pane {}", pointer.index, pointer.pane));
                for (id, bar) in &pointer.series_bars {
                    ui.label(format!("Series {}: {:.2}", id.0, bar.close));
                }
                for (_, row) in &pointer.studies {
                    ui.colored_label(
                        row.color.to_opaque(),
                        format!("{} {}", row.label(), row.formatted_value()),
                    );
                }
            });
            for (_, row) in &pointer.studies {
                ui.colored_label(
                    row.color.to_opaque(),
                    format!("{} {}", row.label(), row.formatted_value()),
                );
            }
        });
    }

    fn handle_context_action(&mut self, ctx: &egui::Context, action: ChartContextAction) {
        match action {
            ChartContextAction::Host { id, .. } if id == "demo-level-remove" => {
                self.remove_demo_level();
            }
            ChartContextAction::Host { id, .. } if id == "demo-fixed-data-table" => {
                self.fixed_data_table.1 = self.layout_slot;
                self.fixed_data_table.0.open();
            }
            ChartContextAction::Host { id, context } if id == "demo-level" => {
                self.set_demo_level(context.price);
            }
            ChartContextAction::Host { id, context } => {
                self.toasts.info(format!(
                    "Host action: {id}, time: {}, price: {:.4}, target: {:?}",
                    context
                        .time
                        .map_or_else(|| "outside loaded data".into(), |time| time.to_rfc3339()),
                    context.price,
                    context.target
                ));
            }
            ChartContextAction::Series(SeriesContextMenuAction::CopyPrice) => self
                .handle_context_action(
                    ctx,
                    ChartContextAction::Chart(ContextMenuAction::CopyPrice(
                        self.menus.series_click_price(),
                    )),
                ),
            ChartContextAction::Chart(ContextMenuAction::CopyPrice(price)) => {
                use egui_charts::scales::PriceFormatter;
                let decimals = self
                    .chart
                    .instrument()
                    .and_then(egui_charts::scales::instrument_price_precision)
                    .unwrap_or(2);
                ctx.copy_text(
                    egui_charts::scales::DefaultPriceFormatter::with_precision(decimals as usize)
                        .format(price),
                );
            }
            ChartContextAction::Chart(ContextMenuAction::AddNote(symbol))
            | ChartContextAction::Series(SeriesContextMenuAction::AddNote(symbol)) => {
                self.add_note(&symbol)
            }
            ChartContextAction::Chart(a) => self.handle_chart_context(a),
            ChartContextAction::Series(a) => self.handle_series_context(a),
            ChartContextAction::Study { id, action } => self.handle_study_context(ctx, id, action),
            ChartContextAction::Drawing(a) => self.handle_drawing_context(ctx, a),
            _ => {}
        }
    }

    /// The study menu applied its chart actions; these are the host's.
    fn handle_study_context(
        &mut self,
        ctx: &egui::Context,
        id: SeriesId,
        action: StudyContextMenuAction,
    ) {
        match action {
            StudyContextMenuAction::AddAlert(mut alert) => {
                if self.synthetic_only("Alerts") {
                    return;
                }
                self.chart.prepare_alert(&mut alert);
                self.alerts.save(*alert);
            }
            StudyContextMenuAction::AddIndicator => {
                let source = self
                    .chart
                    .take_indicator_source_request()
                    .unwrap_or(egui_charts::studies::StudySource::Study { id: id.0, plot: 0 });
                self.indicator_dialog
                    .open_for_chart(&self.chart, Some(source));
            }
            StudyContextMenuAction::AddToFavorites(name) => {
                self.indicator_dialog.favorites.insert(name);
            }
            StudyContextMenuAction::AddToLayout => self.request_layout_copy(LayoutCopy::Study(id)),
            StudyContextMenuAction::Help(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            StudyContextMenuAction::Copy => self.copy_study(ctx, id),
            StudyContextMenuAction::ObjectTree => {
                self.open_panel = Some(SidePanel::ObjectTree);
                self.object_tree.state_mut().select(indicator_tree_id(id));
            }
            _ => {}
        }
    }

    fn copy_study(&mut self, ctx: &egui::Context, id: SeriesId) {
        if let Some(text) = self.chart.copy_indicator(id) {
            ctx.copy_text(text.clone());
            self.copied_drawing = None;
            self.copied_study = Some(text);
        }
    }

    fn handle_chart_context(&mut self, action: ContextMenuAction) {
        self.user_action(|app| app.handle_chart_context_edit(action));
    }

    fn handle_chart_context_edit(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::OpenSettings => {
                self.open_chart_settings_dialog();
            }
            ContextMenuAction::OpenSeriesSettings => {
                let mut settings = self.chart.series_settings();
                settings.chart_alert_color = self.settings_state.alerts.alert_color;
                self.series_settings.open(SeriesId::MAIN, settings);
            }
            ContextMenuAction::ObjectTree => self.open_panel = Some(SidePanel::ObjectTree),
            ContextMenuAction::RemoveAllIndicators => self.remove_all_indicators(),
            ContextMenuAction::ApplyIndicatorsToLayout => {
                self.request_layout_copy(LayoutCopy::Studies)
            }
            ContextMenuAction::AddAlert { symbol, price } => self.quick_price_alert(symbol, price),
            ContextMenuAction::AddToWatchlist { list, symbol } => {
                if self.synthetic_only("Watchlists") {
                    return;
                }
                if !self.watchlist.add_symbol_to(list, &symbol) {
                    self.notify(format!("{symbol} is already in that watchlist"));
                }
            }
            ContextMenuAction::ResetChart => self.chart.reset_chart_view(),
            ContextMenuAction::OpenDataTable => self.data_table.open(),
            ContextMenuAction::ExportCsv => self.export_csv(&Default::default(), None),
            ContextMenuAction::Order(request) => self.trading.open_ticket(request),
            _ => {}
        }
    }

    fn handle_series_context(&mut self, action: SeriesContextMenuAction) {
        match action {
            SeriesContextMenuAction::OpenSettings => {
                let id = match self.chart.selected_element() {
                    // Volume is a selectable series too, and its settings are the chart's.
                    Some(egui_charts::widget::ChartElementId::Series(id))
                        if self.chart.series().any(|(series, _)| series == id) =>
                    {
                        id
                    }
                    _ => SeriesId::MAIN,
                };
                self.open_indicator_settings(id);
            }
            SeriesContextMenuAction::AddIndicator => {
                self.indicator_dialog.open_for_chart(&self.chart, None);
            }
            SeriesContextMenuAction::AddAlert => {
                let price = self.menus.series_alert_price();
                self.open_alert(price.is_finite().then_some(price));
            }
            _ => {}
        }
    }

    fn handle_drawing_context(&mut self, ctx: &egui::Context, action: DrawingContextMenuAction) {
        self.user_action(|app| app.handle_drawing_context_edit(ctx, action));
    }

    fn handle_drawing_context_edit(
        &mut self,
        ctx: &egui::Context,
        action: DrawingContextMenuAction,
    ) {
        // The library selects the right-clicked drawing, so the selected id is
        // the menu's target.
        let Some(id) = self.chart.drawings().sel_drawing() else {
            return;
        };
        let dm = self.chart.drawings_mut();
        match action {
            DrawingContextMenuAction::Delete => self.chart.delete_selected_drawings(),
            DrawingContextMenuAction::Edit => self.open_drawing_props_for(id),
            DrawingContextMenuAction::Clone => self.clone_drawing(id),
            DrawingContextMenuAction::Copy => match self.chart.copy_selected_drawings() {
                Ok(Some(text)) => {
                    ctx.copy_text(text.clone());
                    self.copied_drawing = Some(text);
                    self.copied_study = None;
                }
                Ok(None) => {}
                Err(error) => self
                    .toasts
                    .error(format!("Could not copy drawings: {error}")),
            },
            DrawingContextMenuAction::BringForward | DrawingContextMenuAction::SendBackward => {
                let action = if action == DrawingContextMenuAction::BringForward {
                    ObjectTreeAction::MoveUp(id)
                } else {
                    ObjectTreeAction::MoveDown(id)
                };
                self.object_tree
                    .apply_to_chart(&mut self.chart, &action, &[]);
            }
            // The menu and the toolbar show the state of the drawing they opened on.
            DrawingContextMenuAction::Lock => {
                let locked = dm.drawings.iter().any(|d| d.id == id && d.locked);
                dm.set_selected_locked(!locked);
            }
            DrawingContextMenuAction::Hide => {
                let visible = dm.drawings.iter().any(|d| d.id == id && d.visible);
                dm.set_selected_visible(!visible);
            }
            DrawingContextMenuAction::BringToFront => dm.bring_to_front(id),
            DrawingContextMenuAction::SendToBack => dm.send_to_back(id),
            DrawingContextMenuAction::MoveTo(pane) => {
                if dm
                    .drawings
                    .iter()
                    .any(|d| d.id == id && dm.accepts_user_edits(d))
                {
                    self.chart.move_drawing_to_pane(id, pane);
                }
            }
            DrawingContextMenuAction::AddAlert(mut alert) => {
                self.list_alert_sources();
                self.chart.prepare_alert(&mut alert);
                self.alert_dialog.edit(*alert);
            }
            DrawingContextMenuAction::None => {}
            _ => {}
        }
    }

    /// Keeps the replay bar out of the chart's way: between its time axis and the bottom bar.
    fn reserve_replay_strip(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        if self.replay_control.is_visible() {
            let strip = egui::Panel::bottom("replay_bar")
                .exact_size(FloatingReplayControl::STRIP_HEIGHT)
                .show_separator_line(false)
                .frame(panel_frame(bg).corner_radius(4).outer_margin(egui::Margin {
                    top: 1,
                    ..egui::Margin::ZERO
                }))
                .show(ui, |_| {})
                .response
                .rect;
            self.replay_control.set_dock_strip(strip);
        }
        if self.replay_toolbar
            && let Some(replay) = &self.replay
        {
            let state = ReplayToolbarState::from(replay);
            let action = egui::Panel::bottom("replay_toolbar")
                .frame(panel_frame(bg))
                .show(ui, |ui| ReplayToolbar::new(&state).show(ui))
                .inner;
            if let Some(action) = action {
                self.handle_replay_toolbar(action);
            }
        }
    }

    fn show_replay_control(&mut self, ctx: &egui::Context) {
        if self.replay.is_none() && !self.replay_selecting {
            return;
        }
        self.replay_control.set_selecting(self.replay_selecting);
        self.replay_control.set_scope(
            (self.layout.mode.chart_count() > 1 && !self.recording_files.opened)
                .then_some(self.replay_scope),
        );
        self.replay_control
            .set_can_resume(self.replay.is_none() && self.replay_session.is_some());
        match self.replay.as_ref() {
            Some(replay) => self.replay_control.sync_from(
                replay.state.is_active(),
                replay.state.is_playing(),
                replay.state.speed.multiplier(),
                replay.state.curr_bar,
                replay.state.total_bars,
            ),
            None => {
                let speed = self.replay_control.state.speed;
                self.replay_control.sync_from(false, false, speed, 0, 0);
            }
        }
        let action = self.replay_control.show(ctx);
        if action != ReplayControlAction::None {
            ctx.request_repaint();
        }
        self.handle_replay_control(action);
    }

    fn handle_replay_control(&mut self, action: ReplayControlAction) {
        match action {
            ReplayControlAction::Start(start) => return self.apply_replay_start(start),
            ReplayControlAction::SetScope(scope) => return self.set_replay_scope(scope),
            // Without a replay the bar is only there to select its start.
            ReplayControlAction::Stop | ReplayControlAction::Close if self.replay.is_none() => {
                return self.stop_replay();
            }
            ReplayControlAction::SetSpeed(speed) if self.replay.is_none() => {
                return self.replay_control.set_speed(speed);
            }
            _ => {}
        }
        // A move back ends on the bar before the cursor's, or on the first bar.
        let back = self.replay.as_ref().and_then(|replay| match action {
            ReplayControlAction::StepBackward | ReplayControlAction::PreviousBar => {
                replay.visible_bars().iter().rev().nth(1)
            }
            ReplayControlAction::GoToStart => replay.visible_bars().first(),
            _ => None,
        });
        if let Some(time) = back.map(|bar| bar.time)
            && self.hold_replay_move(replay::ReplayMove::Control(action.clone()), Some(time))
        {
            return;
        }
        let play = matches!(action, ReplayControlAction::Play);
        let endpoint = match action {
            ReplayControlAction::GoToStart => Some(false),
            ReplayControlAction::GoToEnd => Some(true),
            _ => None,
        };
        let transport = matches!(
            action,
            ReplayControlAction::Play
                | ReplayControlAction::Pause
                | ReplayControlAction::SetSpeed(_)
        );
        let Some(replay) = self.replay.as_mut() else {
            return;
        };
        match action {
            ReplayControlAction::Play => replay.handle_action(ReplayAction::Play),
            ReplayControlAction::Pause => replay.handle_action(ReplayAction::Pause),
            ReplayControlAction::StepForward | ReplayControlAction::NextBar => {
                replay.handle_action(ReplayAction::StepForward);
            }
            ReplayControlAction::StepBackward | ReplayControlAction::PreviousBar => {
                replay.handle_action(ReplayAction::StepBackward);
            }
            ReplayControlAction::GoToStart => {
                replay.handle_action(ReplayAction::JumpToStart);
            }
            ReplayControlAction::GoToEnd => {
                replay.handle_action(ReplayAction::JumpToEnd);
            }
            ReplayControlAction::SetSpeed(s) => replay.handle_action(ReplayAction::SetSpeed(s)),
            ReplayControlAction::SetUpdateInterval(interval) => self.set_replay_interval(interval),
            ReplayControlAction::SetAutomaticUpdateInterval => self.set_automatic_replay_interval(),
            ReplayControlAction::Stop => self.stop_replay(),
            ReplayControlAction::Close => {
                self.replay_selecting = false;
                self.replay_control.dismiss();
                return;
            }
            ReplayControlAction::None => return,
            _ => {}
        }
        if let Some(end) = endpoint
            && let Some(driver) = self.chart.replay_driver().cloned()
            && let Some((first, last)) = driver.time_range()
        {
            driver.seek(if end { last } else { first });
            self.push_market_replay_window();
        } else if !transport {
            self.push_replay_window_to_chart();
        }
        self.sync_replay_driver(play);
    }

    fn handle_replay_toolbar(&mut self, action: ReplayToolbarAction) {
        match action {
            ReplayToolbarAction::Stop => return self.stop_replay(),
            ReplayToolbarAction::OpenSettings => return,
            _ => {}
        }
        let back = self.replay.as_ref().and_then(|replay| {
            match action {
                ReplayToolbarAction::StepBackward => replay.visible_bars().iter().rev().nth(1),
                ReplayToolbarAction::Play
                    if replay.state.playback_state
                        == egui_charts::ui::replay::PlaybackState::Finished =>
                {
                    replay.visible_bars().first()
                }
                _ => None,
            }
            .map(|bar| bar.time)
        });
        if let Some(time) = back
            && self.hold_replay_move(replay::ReplayMove::Toolbar(action.clone()), Some(time))
        {
            return;
        }
        let play = matches!(action, ReplayToolbarAction::Play);
        let transport = matches!(
            action,
            ReplayToolbarAction::Play
                | ReplayToolbarAction::Pause
                | ReplayToolbarAction::SetSpeed(_)
        );
        let restart = play && back.is_some();
        let Some(replay) = &mut self.replay else {
            return;
        };
        replay.handle_action(action.into());
        if !transport || restart {
            self.push_replay_window_to_chart();
        }
        self.sync_replay_driver(play);
    }

    fn sync_replay_driver(&mut self, play: bool) {
        if let Some(driver) = self.chart.replay_driver() {
            let replay = self.replay.as_mut().expect("active replay");
            if play && !driver.is_finished() {
                replay.state.playback_state = egui_charts::ui::replay::PlaybackState::Playing;
            }
            let speed = if replay.state.is_playing() {
                replay.state.speed.multiplier()
            } else {
                0.0
            };
            driver
                .set_speed(f64::from(speed))
                .expect("playback control speed");
            self.replay_updated = Some(chrono::Utc::now());
        }
    }

    fn keyboard_dialog_open(&self) -> bool {
        self.keyboard_shortcuts.is_open
            || self.data_table.is_open
            || self.fixed_data_table.0.is_open
            || self.annotation_editor
            || self.quick_search.is_open()
            || self.symbol_search.is_open()
            || self.compare_dialog.is_open
            || self.comparison_settings.is_some()
            || self.indicator_dialog.is_open()
            || self.settings_dialog.is_open
            || self.series_settings.is_open
            || self.layout_copy_prompt.is_some()
            || self.drawing_toolbar.state.pending_removal.is_some()
            || self.drawing_props.is_open
            || self.export_dialog.is_open()
            || self.alert_dialog.is_open
            || self.study_templates.is_open
            || self.indicator_settings.is_open()
            || self.interval_entry.is_some()
            || self.open_layout
            || self.date_picker.is_open
            || self.replay_start.is_open
            || self.replay_selecting
            || self.replay_prompt.is_open
            || self.replay_results.is_open
            || self.data_source.is_open()
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        use egui_charts::widget::ShortcutRequest;
        self.chart.dispatch_shortcuts(ctx);
        self.sync_chart_history();
        for request in self.chart.take_shortcut_requests() {
            match request {
                ShortcutRequest::PasteDrawing(text) => {
                    self.paste_drawing_text(&text);
                }
                ShortcutRequest::SymbolSearch(text) => {
                    self.symbol_search.open_with_query(text);
                }
                ShortcutRequest::IntervalEntry(text) => {
                    self.interval_entry = Some(text);
                    self.interval_focus = true;
                }
                ShortcutRequest::PasteImage(picture) => {
                    if let Err(error) = self.images.paste(ctx, &mut self.chart, &picture) {
                        self.toasts
                            .error(format!("Could not paste the picture: {error}"));
                    }
                }
                ShortcutRequest::Action(action) => self.handle_shortcut_action(ctx, action),
                _ => {}
            }
        }
    }

    /// Runs a key binding the chart leaves to the host, as its key press would.
    fn handle_shortcut_action(
        &mut self,
        ctx: &egui::Context,
        action: egui_charts::config::KeyboardAction,
    ) {
        use egui_charts::config::KeyboardAction as A;
        match action {
            A::OpenQuickSearch => self.open_quick_search(),
            A::ShowKeyboardHelp => self.keyboard_shortcuts.toggle(),
            A::OpenIndicatorSearch => {
                self.indicator_dialog.open_for_chart(&self.chart, None);
            }
            A::OpenSymbolSearch => self
                .symbol_search
                .open_with_selected_query(self.chart.symbol().to_owned()),
            A::OpenIntervalSearch => {
                self.interval_entry = Some(String::new());
                self.interval_focus = true;
            }
            A::GoToDate => self.date_picker.open_for_chart(&self.chart),
            A::Snapshot => {
                // Run the entry the snapshot menu shows the shortcut beside.
                let action = if self.top_toolbar.state.can_publish_image {
                    TopToolbarAction::CopyImageLink
                } else {
                    TopToolbarAction::TakeScreenshot
                };
                self.handle_top_toolbar(ctx, action);
            }
            A::SaveLayout => self.handle_top_toolbar(ctx, TopToolbarAction::Save),
            A::OpenLayout => {
                self.handle_layout_action(egui_charts::ui::top_toolbar::LayoutAction::OpenLayout)
            }
            A::OpenDataWindow => self.open_panel = Some(SidePanel::DataWindow),
            A::AddToWatchlist => {
                if !self.synthetic_only("Watchlists") {
                    let symbol = self.chart.symbol().to_owned();
                    if !self.watchlist.add_symbol(&symbol) {
                        self.notify(format!("{symbol} is already in that watchlist"));
                    }
                }
            }
            A::AddNote => {
                let symbol = self.chart.symbol().to_owned();
                self.add_note(&symbol);
            }
            A::NextWatchlistSymbol | A::PreviousWatchlistSymbol => {
                let step = if action == A::NextWatchlistSymbol {
                    self.watchlist.select_next_symbol()
                } else {
                    self.watchlist.select_previous_symbol()
                };
                if let WatchlistAction::SelectSymbol(symbol) = step {
                    self.open_catalog_symbol(&symbol);
                }
            }
            A::ToggleSession if DemoSymbol::has_hours(&self.state.symbol) => {
                let session = if self.timeframe_toolbar.state.session_type == SessionType::RTH {
                    SessionType::ETH
                } else {
                    SessionType::RTH
                };
                self.timeframe_toolbar.set_session(session);
                self.stop_replay();
                self.apply_session();
            }
            A::CreateAlert => {
                let mapping = self.chart.get_chart_mapping();
                if let Some(pos) = ctx
                    .pointer_hover_pos()
                    .filter(|pos| mapping.rect.contains(*pos))
                {
                    if ctx
                        .layer_id_at(pos)
                        .is_some_and(|layer| layer.order == egui::Order::Background)
                        && let Some(price) = self.chart.alert_price_at(pos)
                    {
                        self.quick_price_alert(self.chart.symbol().to_owned(), price);
                    }
                } else {
                    self.open_alert(None);
                }
            }
            A::MaximizeChart => self.handle_top_toolbar(ctx, TopToolbarAction::ToggleFullscreen),
            A::CopyDrawing => match self.chart.copy_selected_drawings() {
                Ok(Some(text)) => {
                    ctx.copy_text(text.clone());
                    self.copied_drawing = Some(text);
                    self.copied_study = None;
                }
                Ok(None) => {
                    if let Some(egui_charts::widget::ChartElementId::Indicator(id)) =
                        self.chart.selected_element()
                    {
                        self.copy_study(ctx, id);
                    }
                }
                Err(error) => {
                    self.toasts
                        .error(format!("Could not copy drawings: {error}"));
                }
            },
            A::PasteDrawing => self.paste_drawing(),
            _ => {}
        }
    }

    fn paste_drawing(&mut self) {
        if let Some(text) = self.copied_drawing.clone().or(self.copied_study.clone()) {
            self.paste_drawing_text(&text);
        }
    }

    fn paste_drawing_text(&mut self, text: &str) {
        if text.starts_with("egui-charts-study\n") {
            if let Err(error) = self
                .chart
                .paste_indicator(text, self.indicator_dialog.factory())
            {
                self.toasts
                    .error(format!("Could not paste the study: {error}"));
            }
            return;
        }
        if !text.starts_with("egui-charts-drawings\n") {
            return;
        }
        match self.chart.paste_drawings(text) {
            Ok(()) => {}
            Err(error) => {
                self.toasts
                    .error(format!("Could not paste drawings: {error}"));
            }
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some((target, options, scope)) = self.screenshot_requested.take() else {
            return;
        };
        let result = self
            .export_image_bytes(
                ctx,
                &options,
                ExportFormat::Png,
                scope == ExportScope::Layout,
            )
            .map_err(|error| error.to_string())
            .and_then(|png| match target {
                ScreenshotTarget::File(path) => self.save_screenshot(&png, path),
                ScreenshotTarget::NewTab => open_image(ctx, &png),
                ScreenshotTarget::Post { copy_link } => Ok(self.post_image(ctx, png, copy_link)),
                ScreenshotTarget::Clipboard => {
                    let image = image::load_from_memory(&png)
                        .map_err(|error| error.to_string())?
                        .into_rgba8();
                    ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ));
                    Ok("Image sent to clipboard".to_owned())
                }
            });
        if self.export_dialog.is_open() {
            match &result {
                Ok(_) => self.export_dialog.mark_success(),
                Err(error) => self.export_dialog.mark_error(error.clone()),
            }
        }
        self.notify(result.unwrap_or_else(|error| format!("Export failed: {error}")));
    }

    fn show_notice(&mut self, ui: &egui::Ui) {
        let spacing = self.theme.spacing;
        let width = (ui.clip_rect().right() - 2.0 * (spacing.xxl + spacing.lg) - 2.0).min(340.0);
        // Trading notices reserve 80 points for the icon, padding and close button.
        if width <= 80.0 {
            return;
        }
        NotificationPanel::new()
            .width(width)
            .show_with_toasts(ui.ctx(), &mut self.toasts);
    }

    fn save_screenshot(
        &self,
        png: &[u8],
        path: Option<std::path::PathBuf>,
    ) -> Result<String, String> {
        let path = path.unwrap_or_else(|| default_export_path(&self.state.symbol, "png"));
        #[cfg(not(target_arch = "wasm32"))]
        let result = std::fs::write(&path, png).map_err(|error| error.to_string());
        #[cfg(target_arch = "wasm32")]
        let result = browser_download(png, &path);
        result.map(|()| format!("Export successful: {}", path.display()))
    }
}

// ---------------------------------------------------------------------------
// Free helpers.
// ---------------------------------------------------------------------------

/// Default export destination, named from a path-safe symbol and the local time to the second,
/// such as `SAMPLE_2026-11-20_06-50-12.png`. Native hosts save in the temporary directory;
/// browsers download the file. A visitor's symbol must not carry a path into the file name.
fn default_export_path(symbol: &str, extension: &str) -> std::path::PathBuf {
    let stem = owndata::store::file_stem(symbol);
    let time = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("{stem}_{time}.{extension}");
    #[cfg(not(target_arch = "wasm32"))]
    return std::env::temp_dir().join(filename);
    #[cfg(target_arch = "wasm32")]
    std::path::PathBuf::from(filename)
}

#[cfg(not(target_arch = "wasm32"))]
fn open_image(ctx: &egui::Context, png: &[u8]) -> Result<String, String> {
    let path = std::env::temp_dir().join(format!(
        "egui-charts-image-{}.png",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&path, png).map_err(|error| error.to_string())?;
    ctx.open_url(egui::OpenUrl::new_tab(path.to_string_lossy()));
    Ok("Image opened in viewer".into())
}

#[cfg(target_arch = "wasm32")]
fn open_image(_ctx: &egui::Context, png: &[u8]) -> Result<String, String> {
    use wasm_bindgen::{JsCast, closure::Closure};
    use web_sys::{Blob, BlobPropertyBag, Url, js_sys};
    let open = || -> Result<(), wasm_bindgen::JsValue> {
        let window = web_sys::window()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("No browser window"))?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(png));
        let properties = BlobPropertyBag::new();
        properties.set_type("image/png");
        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &properties)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        let opened = window.open_with_url_and_target(&url, "_blank");
        if !matches!(opened, Ok(Some(_))) {
            Url::revoke_object_url(&url)?;
            return Err(wasm_bindgen::JsValue::from_str(
                "The browser blocked the image tab",
            ));
        }
        // Leave time for the new tab to read the image before releasing its URL.
        let release = Closure::once_into_js(move || {
            let _ = Url::revoke_object_url(&url);
        });
        window.set_timeout_with_callback_and_timeout_and_arguments_0(
            release.unchecked_ref(),
            60_000,
        )?;
        Ok(())
    };
    open().map_err(|error| format!("Could not open image: {error:?}"))?;
    Ok("Image opened in new tab".into())
}

#[cfg(target_arch = "wasm32")]
fn browser_download(bytes: &[u8], path: &std::path::Path) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Blob, HtmlAnchorElement, Url, js_sys};

    let download = || -> Result<(), wasm_bindgen::JsValue> {
        let document = web_sys::window().unwrap().document().unwrap();
        let anchor = document
            .create_element("a")?
            .dyn_into::<HtmlAnchorElement>()?;
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let blob = Blob::new_with_u8_array_sequence(&parts)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        anchor.set_href(&url);
        anchor.set_download(&path.file_name().unwrap_or_default().to_string_lossy());
        anchor.click();
        Url::revoke_object_url(&url)
    };
    download().map_err(|e| format!("Browser download failed: {e:?}"))
}

/// Whether the layout paints the extra chart at `index`: inside the arrangement, and among tabs
/// only when detached or while a tab is split beside another.
fn slot_shown(layout: &ChartLayout, active: usize, index: usize) -> bool {
    let slot = if index >= active { index + 1 } else { index };
    // shortcut: a split runs every tab's feed, as the layout does not name the tab beside the
    // split one; name it if many tabs with costly feeds make a split slow.
    slot < layout.mode.chart_count()
        && (layout.mode != ChartLayoutMode::Tabs
            || layout.is_detached(slot)
            || layout.tab_split().is_some())
}

fn panel_frame(bg: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .corner_radius(4)
        .fill(bg)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::ZERO)
}

/// The toast after a delete that kept `kept` locked drawings.
fn locked_drawings_kept(kept: usize) -> String {
    if kept == 1 {
        "The locked drawing was kept. Unlock it to delete it.".to_owned()
    } else {
        format!("{kept} locked drawings were kept. Unlock them to delete them.")
    }
}

/// Gives the price-based chart types of `chart` half the 14-bar average true range of the bars
/// on show, which suits any symbol, until a size is entered in the chart settings.
fn size_by_atr(chart: &mut Chart) {
    let mut params = chart.chart_type_params().clone();
    let half_atr = BoxSizeMethod::Atr {
        period: 14,
        multiplier: 0.5,
    };
    params.renko_size_method = half_atr;
    params.kagi_size_method = half_atr;
    params.range_size_method = half_atr;
    params.pnf_size_method = half_atr;
    chart.set_chart_type_params(params);
}

/// The session a symbol opens in: around the clock where its market never closes, otherwise
/// the regular session.
fn opening_session(symbol: &str) -> SessionType {
    if matches!(DemoSymbol::find(symbol).exchange, "Crypto" | "Forex") {
        SessionType::Full24x7
    } else {
        SessionType::RTH
    }
}

/// Reloads `chart` from `live` in `session`, with prices adjusted for dividends or not, in the
/// price unit the chart shows.
fn load_view(chart: &mut Chart, live: &LiveFeed, session: SessionType, adjusted: bool) {
    live.set_view(session, adjusted);
    let cents = chart.conversion().is_some_and(|c| c.starts_with("cents"));
    live.set_price_unit(cents);
    chart.set_conversion(replay::conversion(cents, adjusted));
    chart.reload_feed();
    chart.set_instrument(Some(live.instrument()));
    chart.set_extended_hours(session != SessionType::RTH);
}

fn apply_timezone(chart: &mut Chart, toolbar: &mut TimeframeToolbar, name: &str) {
    use egui_charts::config::TimezoneMode;
    let timezone = if name == "UTC" {
        TimezoneMode::Utc
    } else if let Ok(zone) = name.parse() {
        TimezoneMode::Timezone(zone)
    } else {
        return;
    };
    apply_timezone_mode(chart, toolbar, timezone);
}

/// Shows the chart in `timezone` and the clock in the zone the chart resolves for it.
fn apply_timezone_mode(
    chart: &mut Chart,
    toolbar: &mut TimeframeToolbar,
    timezone: egui_charts::config::TimezoneMode,
) {
    if chart.chart_options().time_scale.timezone != timezone {
        let mut options = chart.chart_options().clone();
        options.time_scale.timezone = timezone;
        // Preserve the current pan and zoom when applying display options.
        let (spacing, offset) = chart.get_time_scale_state();
        let previous = std::mem::replace(chart, Chart::new(BarData::new()));
        *chart = previous.with_chart_options(options);
        chart.apply_synced_time_scale(spacing, offset);
    }
    toolbar
        .state
        .set_display_timezone(&chart.display_timezone());
}

/// Apply display settings to the live chart and its clock.
fn apply_settings_state(
    state: &ChartSettingsState,
    chart: &mut Chart,
    toolbar: &mut TimeframeToolbar,
) {
    if state.apply_to_chart(chart).is_ok() {
        toolbar
            .state
            .set_display_timezone(&chart.display_timezone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::model::DateRange;
    use egui_charts::model::{AlertSource, LineStyle};
    use egui_charts::ui::dialogs::DrawingProps;
    use egui_kittest::{Harness, kittest::Queryable};

    pub(super) fn calendar_news_harness(panel: SidePanel) -> Harness<'static, Box<DemoApp>> {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            chrono::Utc::now(),
            true,
        );
        finish_feed(&app.live);
        app.open_panel = Some(panel);
        Harness::builder()
            .with_size(egui::vec2(1440.0, 1100.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                app,
            )
    }

    #[test]
    fn calendar_news_demo_follows_the_toolbar_timezone() {
        use chrono::Offset;
        let mut h = calendar_news_harness(SidePanel::Calendar);
        let app = h.state_mut();
        apply_timezone(
            &mut app.chart,
            &mut app.timeframe_toolbar,
            "America/New_York",
        );
        h.run_steps(5);
        h.get_by_label("Times in America/New_York");
        let mut query = egui_charts::data::NewsQuery::new(1);
        query.symbols = vec![h.state().chart.symbol().into()];
        let story = feeds::news(&query, h.state().live.now()).remove(0);
        let zone: egui_charts::model::TimezoneId = "America/New_York".parse().unwrap();
        let local = story.time.with_timezone(&zone);
        let offset = local.offset().fix().local_minus_utc() / 3600;
        let meta = format!(
            "{} GMT{offset:+} · Demo Wire",
            local.format("%b %-d, %H:%M")
        );
        for panel in [SidePanel::News, SidePanel::Highlights] {
            h.state_mut().open_panel = Some(panel);
            h.run_steps(5);
            h.get_by_label(&meta);
        }
    }

    #[test]
    fn calendar_news_demo_filters_categories_opens_descriptions_and_changes_ranges() {
        use chrono::Datelike;
        let mut h = calendar_news_harness(SidePanel::Calendar);
        h.run_steps(5);
        h.get_by_label("All categories").click();
        h.run_steps(3);
        h.get_by_label("Inflation").click();
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        let now = chrono::Utc::now();
        let today = now.date_naive();
        let monday =
            today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()));
        for (range, start, end) in [
            (
                "Last week",
                (monday - chrono::Duration::weeks(1))
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
                monday.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            ),
            ("Upcoming", now, now + chrono::Duration::weeks(4)),
        ] {
            h.get_by_label(if range == "Last week" {
                "This week"
            } else {
                "Last week"
            })
            .click();
            h.run_steps(3);
            h.get_by_label(range).click();
            h.run_steps(5);
            let query = egui_charts::data::CalendarQuery::new(
                egui_charts::data::CalendarKind::Economic,
                start,
                end,
            );
            let events = feeds::calendar(&query, now);
            let inflation: Vec<_> = events
                .iter()
                .filter(|event| event.title == "CPI (YoY)")
                .collect();
            assert!(!inflation.is_empty());
            let event = inflation[0];
            h.query_all_by_role_and_label(egui::Role::Button, &event.title)
                .next()
                .unwrap()
                .click();
            h.run_steps(3);
            h.get_by_label("Annual change in consumer prices.");
            h.get_by_label(&event.time.format("%A, %b %-d").to_string());
            for title in [
                "Manufacturing PMI",
                "Initial jobless claims (K)",
                "Retail sales (MoM)",
                "Interest rate decision",
            ] {
                assert!(
                    h.query_by_role_and_label(egui::Role::Button, title)
                        .is_none(),
                    "{title}"
                );
            }
        }
    }

    #[test]
    fn calendar_news_demo_supplies_company_figures() {
        let mut h = calendar_news_harness(SidePanel::Calendar);
        h.run_steps(5);
        h.get_by_label("This week").click();
        h.run_steps(3);
        h.get_by_label("Last week").click();
        h.run_steps(5);
        // The complete table is rendered at the host's available width.
        let app = h.into_state();
        let mut h = Harness::builder()
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                egui_charts::theme::apply_to_egui(ctx, &egui_charts::theme::Theme::dark());
            })
            .with_size(egui::vec2(1400.0, 1100.0))
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    app.calendar.poll(ui.ctx());
                    app.calendar.show(ui);
                },
                app,
            );
        use chrono::Datelike;
        let today = chrono::Utc::now().date_naive();
        let payment = (today
            - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()))
            + chrono::Duration::weeks(1))
        .format("%Y-%m-%d")
        .to_string();
        for (kind, figures) in [
            (
                "Earnings",
                vec![
                    ("Revenue", "12.0B"),
                    ("Revenue estimate", "11.8B"),
                    ("Market cap", "120.0B"),
                    ("Surprise", "+3.0%"),
                ],
            ),
            (
                "Dividends",
                vec![("Yield", "3.2%"), ("Payment date", payment.as_str())],
            ),
            (
                "IPOs",
                vec![
                    ("Deal amount", "500M"),
                    ("Shares offered", "25M"),
                    ("Exchange", "XNAS"),
                ],
            ),
        ] {
            h.get_by_label(kind).click();
            h.run_steps(5);
            for (heading, value) in figures {
                assert!(
                    h.query_all_by_role_and_label(egui::Role::Label, heading)
                        .next()
                        .is_some(),
                    "{heading}"
                );
                assert!(h.query_all_by_label(value).next().is_some(), "{value}");
            }
        }
    }

    #[test]
    fn calendar_news_demo_filters_countries_and_reads_with_the_keyboard() {
        let mut h = calendar_news_harness(SidePanel::News);
        h.run_steps(5);
        h.get_by_label("Market").click();
        h.run_steps(5);
        h.get_by_label("All countries").click();
        h.run_steps(3);
        h.get_by_label("JP").click();
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(5);
        let query = egui_charts::data::NewsQuery::new(200);
        let all = feeds::news(&query, h.state().live.now());
        let mut query = query;
        query.countries = vec!["JP".into()];
        let stories = feeds::news(&query, h.state().live.now());
        assert!(stories.len() >= 2);
        assert!(stories.len() < all.len());
        for story in all
            .iter()
            .filter(|story| !stories.iter().any(|selected| selected.id == story.id))
        {
            assert!(
                h.query_by_role_and_label(egui::Role::Button, &story.headline)
                    .is_none()
            );
        }
        h.get_by_role_and_label(egui::Role::Button, &stories[0].headline)
            .focus();
        h.run_steps(3);
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(3);
        assert!(
            h.get_by_role_and_label(egui::Role::Button, &stories[1].headline)
                .is_focused()
        );
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        h.get_by_label(&stories[1].body);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        h.get_by_role_and_label(egui::Role::Button, &stories[1].headline);
    }

    #[test]
    fn calendar_news_demo_restores_saved_views() {
        let mut h = calendar_news_harness(SidePanel::News);
        h.run_steps(5);
        h.get_by_role(egui::Role::TextInput).focus();
        h.run_steps(2);
        h.get_by_role(egui::Role::TextInput).type_text("Rates");
        h.run_steps(3);
        h.get_by_label("Save").click();
        h.run_steps(5);
        h.get_by_role_and_label(egui::Role::Button, "Rates");
        let mut storage = AppStorage::default();
        eframe::App::save(&mut **h.state_mut(), &mut storage);
        let ctx = egui::Context::default();
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new_at(&cc, chrono::Utc::now(), true);
        restored.open_panel = Some(SidePanel::News);
        finish_feed(&restored.live);
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .with_size(egui::vec2(1440.0, 1100.0))
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                restored,
            );
        h.run_steps(5);
        h.get_by_role_and_label(egui::Role::Button, "Rates");
    }

    /// Guards the demo's news alerts: a new story of the alert whose selection the open News
    /// panel shows stays quiet, the next one notifies while the panel is closed, and its Open
    /// story button reopens News on that headline among the alert's stories.
    #[test]
    fn calendar_news_demo_alert_quiet_while_shown_opens_its_story() {
        let mut h = calendar_news_harness(SidePanel::News);
        h.run_steps(5);
        h.get_by_role(egui::Role::TextInput).focus();
        h.run_steps(2);
        h.get_by_role(egui::Role::TextInput)
            .type_text("Rates alert");
        h.run_steps(3);
        h.get_by_label("Create alert").click();
        h.run_steps(5);
        h.get_by_label("Alert: Rates alert");
        finish_feed(&h.state().live);
        let now = chrono::Utc::now();
        let advance = |h: &mut Harness<'static, Box<DemoApp>>, hours| {
            h.state().live.set_paused(false);
            h.state().live.advance(now + chrono::Duration::hours(hours));
            assert_eq!(h.state().live.now(), now + chrono::Duration::hours(hours));
            h.state().live.set_paused(true);
            h.run_steps(8);
        };
        advance(&mut h, 1);
        h.state_mut().open_panel = Some(SidePanel::Notifications);
        h.run_steps(5);
        assert!(h.query_by_label("Rates alert").is_none());
        advance(&mut h, 2);
        h.get_by_label("Rates alert");
        h.get_by_label("Open story").click();
        h.run_steps(5);
        assert!(h.state().open_panel == Some(SidePanel::News));
        let mut query = egui_charts::data::NewsQuery::new(1);
        query.symbols = vec![h.state().chart.symbol().into()];
        let story = feeds::news(&query, h.state().live.now()).remove(0);
        h.get_by_role_and_label(egui::Role::Label, &story.headline);
        h.get_by_role_and_label(egui::Role::Button, "Next");
    }

    /// Guards the demo's chat author menu: Reply answers the message through the demo host, View
    /// profile opens the author's card, and its Message opens a direct chat with them.
    #[test]
    fn chat_author_profile_messages_them_in_the_demo() {
        use egui::Role;
        let mut h = calendar_news_harness(SidePanel::Chats);
        h.run_steps(5);
        let author_menu = |h: &mut Harness<'static, Box<DemoApp>>, entry: &str| {
            h.get_all_by_role_and_label(Role::Button, "Ana")
                .last()
                .unwrap()
                .click();
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, entry).click();
            h.run_steps(3);
        };
        author_menu(&mut h, "Reply");
        h.get_by_label("Replying to Ana");
        h.get_by(|node| node.placeholder() == Some("Message General"))
            .focus();
        h.run_steps(2);
        h.get_by(|node| node.placeholder() == Some("Message General"))
            .type_text("Agreed");
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Send").click();
        h.run_steps(5);
        h.get_by_label("Ana: Volume looks light ahead of the CPI release.");

        author_menu(&mut h, "View profile");
        h.get_by_label("Index futures, mostly around the open.");
        h.get_by_role_and_label(Role::Button, "Message").click();
        h.run_steps(5);
        assert!(h.state().open_panel == Some(SidePanel::Chats));
        assert!(h.state().profile_open.is_none());
        h.get_by_role_and_label(Role::Button, "Ana");
        assert!(h.query_by_label("Agreed").is_none());
    }

    #[test]
    fn calendar_news_demo_badge_opens_its_headline() {
        use egui_charts::model::{EventKind, TimeScaleEvent};
        let mut h = calendar_news_harness(SidePanel::Calendar);
        h.run_steps(5);
        let time = h
            .state()
            .chart
            .bars(SeriesId::MAIN)
            .iter()
            .rev()
            .nth(70)
            .unwrap()
            .time;
        let id = format!(
            "demo-news-{}-{}",
            h.state().chart.symbol(),
            time.timestamp_millis()
        );
        let mut query = egui_charts::data::NewsQuery::new(1);
        query.symbols = vec![h.state().chart.symbol().into()];
        let story = feeds::news(&query, time).remove(0);
        let mut visibility = h.state().chart.event_visibility().clone();
        visibility.show_news = true;
        h.state_mut().chart.set_event_visibility(visibility);
        h.state_mut().chart.set_mark_source(None);
        h.state_mut()
            .chart
            .set_time_scale_events(vec![TimeScaleEvent::new(
                id,
                time,
                EventKind::News,
                "Read this news",
            )]);
        h.run_steps(5);
        h.get_by_label_contains("Read this news").click();
        h.run_steps(5);
        assert!(h.state().open_panel == Some(SidePanel::News));
        h.get_by_role_and_label(egui::Role::Label, &story.headline);
        h.get_by_label(&story.body);
        h.get_by_label(&format!("{} UTC · Demo Wire", time.format("%b %-d, %H:%M")));
    }

    /// The watchlist's details use the chart's bars and feed, list the demo's detail sections,
    /// open headline and document links and honour the three section choices through the demo's
    /// menus and action handler, and the sections hidden there stay hidden after a restart on
    /// the same storage. Expand shows the details in place of the chart until Back to chart.
    #[test]
    fn watchlist_details_sections_reach_the_demo() {
        use eframe::App as _;
        use egui::accesskit::Role;
        use egui_charts::studies::{Indicator, StudyCell, TechnicalRatings};

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            chrono::Utc::now(),
            true,
        );
        app.switch_symbol("STOCKA");
        settle(&mut app, &ctx);
        // The tall viewport fits all the details; the narrow toolbar offers its Panels menu.
        let frame = |app: &mut DemoApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(560.0, 6000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
        };
        let click = |app: &mut DemoApp, role, label: &str| {
            let output = frame(app, vec![]);
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.role() == role && node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
                .unwrap_or_else(|| panic!("missing control: {label}"));
            let pos = egui::pos2(
                (bounds.x0 + bounds.x1) as f32 * 0.5,
                (bounds.y0 + bounds.y1) as f32 * 0.5,
            );
            output.drop_without_applying_deltas();
            frame(app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
            frame(
                app,
                [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .to_vec(),
            )
        };
        click(&mut app, Role::Button, "Menu").drop_without_applying_deltas();
        click(&mut app, Role::MenuItem, "Panels").drop_without_applying_deltas();
        click(&mut app, Role::MenuItem, "Watchlist and details").drop_without_applying_deltas();
        assert!(app.open_panel == Some(SidePanel::Watchlist));
        egui::Popup::close_all(&ctx);
        // Let the side panel's initial sizing and feed replies settle.
        for _ in 0..3 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        let painted = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let output = frame(&mut app, vec![]);
        let shown = painted(&output);
        output.drop_without_applying_deltas();
        let bars = app.chart.bars(SeriesId::MAIN);
        assert!(!bars.is_empty());
        let mut study = TechnicalRatings::new();
        study.set_timeframe(app.chart.timeframe().unwrap());
        study.calculate(bars);
        let table = study.table().unwrap();
        for (label, column) in [("Summary", 3), ("Moving averages", 1), ("Oscillators", 2)] {
            let StudyCell::Text(word) = &table.rows[0][column] else {
                panic!("rating word");
            };
            assert!(
                shown.windows(2).any(|row| row == [label, word]),
                "{label}: {shown:?}"
            );
        }
        for title in [
            "Technicals",
            "Upcoming events",
            "Earnings",
            "Ex-dividend",
            "News",
            "Analyst estimates",
            "Options",
            "Earnings calls",
            "Documents",
            "Fund holdings",
            "Credit ratings",
            "Bonds",
        ] {
            assert!(shown.iter().any(|text| text == title), "{title}: {shown:?}");
        }
        let mut query = egui_charts::data::NewsQuery::new(10);
        query.symbols.push("STOCKA".into());
        let stories = feeds::news(&query, app.live.now());
        assert!(stories.len() > 3);
        let headlines: Vec<_> = shown
            .iter()
            .filter(|text| stories.iter().any(|story| story.headline == **text))
            .collect();
        assert_eq!(
            headlines,
            stories[..3]
                .iter()
                .map(|story| &story.headline)
                .collect::<Vec<_>>()
        );
        let output = click(&mut app, Role::Button, &stories[0].headline);
        assert!(output.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::OpenUrl(url) if Some(&url.url) == stories[0].url.as_ref())
        }));
        output.drop_without_applying_deltas();
        // The demo opens a clicked row's link when it next answers the panel.
        click(&mut app, Role::Link, "Annual report 2025").drop_without_applying_deltas();
        let output = frame(&mut app, vec![]);
        assert!(output.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::OpenUrl(url)
                if url.url == "https://demo.invalid/documents/STOCKA/annual-2025")
        }));
        output.drop_without_applying_deltas();

        // Expand shows the details in place of the chart, without its own button, until Back.
        click(&mut app, Role::Button, "Expand").drop_without_applying_deltas();
        let output = frame(&mut app, vec![]);
        let shown = painted(&output);
        let offered = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Expand"));
        output.drop_without_applying_deltas();
        assert!(app.details_expanded);
        assert!(app.open_panel.is_none(), "the details leave the side panel");
        assert!(!offered, "the larger view offers no Expand");
        for text in ["Back to chart", "Technicals"] {
            assert!(shown.iter().any(|shown| shown == text), "{text}: {shown:?}");
        }
        click(&mut app, Role::Button, "Back to chart").drop_without_applying_deltas();
        let output = frame(&mut app, vec![]);
        let shown = painted(&output);
        output.drop_without_applying_deltas();
        assert!(!app.details_expanded);
        assert!(
            !shown.iter().any(|text| text == "Back to chart"),
            "{shown:?}"
        );
        app.open_panel = Some(SidePanel::Watchlist);
        for _ in 0..3 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        click(&mut app, Role::Button, "Layout").drop_without_applying_deltas();
        for title in ["Technicals", "Upcoming events", "News"] {
            click(&mut app, Role::CheckBox, title).drop_without_applying_deltas();
        }
        egui::Popup::close_all(&ctx);
        let output = frame(&mut app, vec![]);
        let shown = painted(&output);
        output.drop_without_applying_deltas();
        for title in ["Technicals", "Upcoming events", "News"] {
            assert!(
                !shown.iter().any(|text| text == title),
                "{title}: {shown:?}"
            );
        }

        let mut storage = AppStorage::default();
        app.save(&mut storage);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new_at(&cc, chrono::Utc::now(), true);
        assert!(app.open_panel == Some(SidePanel::Watchlist));
        settle(&mut app, &ctx);
        for _ in 0..3 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        let output = frame(&mut app, vec![]);
        let shown = painted(&output);
        output.drop_without_applying_deltas();
        assert!(shown.iter().any(|text| text == "Volume"), "{shown:?}");
        for title in ["Technicals", "Upcoming events", "News"] {
            assert!(
                !shown.iter().any(|text| text == title),
                "{title}: {shown:?}"
            );
        }
        assert_eq!(
            app.symbol_details.hidden_sections(),
            ["Technicals", "Upcoming events", "News"]
        );
    }

    /// Notes open from the chart menu, a watchlist row's menu, Alt+N and All notes, each on its
    /// symbol with the field focused, while the chart keeps its symbol. A regression drops a
    /// route, leaves the field unfocused or types into another symbol's notes.
    #[test]
    fn notes_open_from_the_menus_the_key_and_all_notes() {
        use eframe::App;
        use egui::{Event, Key, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        // The demo builds its bars from the clock; a fixed time gives every run the same chart.
        let now = "2026-11-22T02:05:02Z".parse().unwrap();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, now, true);
        app.open_catalog_symbol("STOCKA");
        settle(&mut app, &ctx);
        app.open_panel = Some(SidePanel::Watchlist);
        // Tall enough for every details section above the notes.
        let size = egui::vec2(1440.0, 6000.0);
        let frame = |app: &mut DemoApp, events| {
            ui_frame(&ctx, size, events, |ui| {
                app.ui(ui, &mut eframe::Frame::_new_kittest())
            })
        };
        let press = |app: &mut DemoApp, pos, button| {
            frame(app, vec![Event::PointerMoved(pos)]);
            frame(
                app,
                [true, false]
                    .map(|pressed| Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    })
                    .to_vec(),
            );
        };
        // The topmost painted `text`, so a menu's row wins over the text beneath it.
        let click_text = |app: &mut DemoApp, text: &str, button| {
            let shown = frame(app, vec![]);
            let pos = shown
                .iter()
                .rev()
                .find(|(shown, _)| shown == text)
                .unwrap_or_else(|| panic!("missing {text}: {shown:?}"))
                .1
                .center();
            press(app, pos, button);
        };
        // The text typed once the field has its focus, and the painted notes after it.
        let typed = |app: &mut DemoApp, text: &str| {
            frame(app, vec![]);
            frame(app, vec![]);
            frame(app, vec![Event::Text(text.into())]);
            frame(app, vec![])
        };
        let shows = |shown: &[(String, egui::Rect)], words: &[&str]| {
            shown
                .iter()
                .any(|(text, _)| words.iter().all(|word| text.contains(word)))
        };

        frame(&mut app, vec![]);
        let chart = app.chart.get_chart_mapping().rect.center();
        press(&mut app, chart, PointerButton::Secondary);
        click_text(&mut app, "Add note on STOCKA…", PointerButton::Primary);
        let shown = typed(&mut app, "chart note");
        assert!(shows(&shown, &["chart note"]), "{shown:?}");

        click_text(&mut app, "STOCKB", PointerButton::Secondary);
        click_text(&mut app, "Add note", PointerButton::Primary);
        let shown = typed(&mut app, "row note");
        assert!(shows(&shown, &["row note"]), "{shown:?}");
        assert!(!shows(&shown, &["chart note"]), "{shown:?}");

        // Clicks this soon after the menu's would be a double click on the chart.
        for _ in 0..20 {
            frame(&mut app, vec![]);
        }
        press(&mut app, chart, PointerButton::Primary);
        let key = |pressed| Event::Key {
            key: Key::N,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::ALT,
        };
        frame(
            &mut app,
            vec![
                Event::ModifiersChanged(Modifiers::ALT),
                key(true),
                key(false),
            ],
        );
        let shown = typed(&mut app, "key");
        assert!(shows(&shown, &["chart note", "key"]), "{shown:?}");

        click_text(&mut app, "All notes", PointerButton::Primary);
        click_text(&mut app, "STOCKB", PointerButton::Primary);
        let shown = frame(&mut app, vec![]);
        assert!(shows(&shown, &["row note"]), "{shown:?}");
        assert_eq!(app.chart.symbol(), "STOCKA");
        assert!(app.open_panel == Some(SidePanel::Watchlist));
    }

    /// One frame of the demo's dialogs with `events`, returning its AccessKit nodes.
    fn draw(
        ctx: &egui::Context,
        app: &mut DemoApp,
        events: Vec<egui::Event>,
    ) -> Vec<(egui::accesskit::NodeId, egui::accesskit::Node)> {
        ctx.enable_accesskit();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_dialogs(ui.ctx()),
        );
        let nodes = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .clone();
        output.drop_without_applying_deltas();
        nodes
    }

    /// The control among `nodes` named `label`, by its own name or by the label it is tied to.
    fn control<'a>(
        nodes: &'a [(egui::accesskit::NodeId, egui::accesskit::Node)],
        label: &str,
    ) -> &'a (egui::accesskit::NodeId, egui::accesskit::Node) {
        use egui::accesskit::Role;
        nodes
            .iter()
            .find(|(_, node)| {
                !matches!(node.role(), Role::Label | Role::TextRun)
                    && (node.label() == Some(label)
                        || node.labelled_by().iter().any(|id| {
                            nodes.iter().any(|(other, label_node)| {
                                other == id
                                    && label_node.label().or(label_node.value()) == Some(label)
                            })
                        }))
            })
            .unwrap_or_else(|| panic!("missing control: {label}"))
    }

    /// The demo's series settings reach the TPO chart type's statistics rows, hours, blocks,
    /// volume profile settings, profile length and price line, and the study settings the Time
    /// Price Opportunity study's statistics rows, profile period, value area edges and volume
    /// profile levels.
    #[test]
    fn tpo_settings_reach_the_demo_chart() {
        use egui::accesskit::{Action, ActionData, ActionRequest, TreeId};
        use egui_charts::model::{ChartType, PriceSource, ProfileSession, TPOProfilePeriod};
        use egui_charts::studies::{IndicatorFactory, InputValue};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T17:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        // Clicks through AccessKit, which reaches rows below the fold of the dialog.
        let click = |app: &mut DemoApp, label: &str| {
            let nodes = draw(&ctx, app, vec![]);
            let target_node = control(&nodes, label).0;
            draw(
                &ctx,
                app,
                vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target_tree: TreeId::ROOT,
                    target_node,
                    data: None,
                })],
            );
        };
        app.chart.set_chart_type(ChartType::TimePriceOpportunity);
        app.handle_chart_context(ContextMenuAction::OpenSeriesSettings);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        for label in [
            "Statistics",
            "Statistics: Total volume",
            "Hours",
            "Regular hours",
            "Ok",
        ] {
            click(&mut app, label);
        }
        assert!(!app.series_settings.is_open);
        let params = app.chart.chart_type_params();
        assert!(params.tpo_statistics && params.tpo_statistics_rows.total_volume);
        assert_eq!(params.tpo_session, ProfileSession::Regular);

        // Numbers are typed through AccessKit's set value.
        let set = |app: &mut DemoApp, label: &str, value: f64| {
            let nodes = draw(&ctx, app, vec![]);
            let target_node = control(&nodes, label).0;
            draw(
                &ctx,
                app,
                vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::SetValue,
                    target_tree: TreeId::ROOT,
                    target_node,
                    data: Some(ActionData::NumericValue(value)),
                })],
            );
        };
        app.handle_chart_context(ContextMenuAction::OpenSeriesSettings);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        for label in [
            "Blocks",
            "Volume profile",
            "Volume point of control",
            "Volume placement",
            "Left",
            "Price source",
            "High",
            "Profile period",
            "Weeks",
        ] {
            click(&mut app, label);
        }
        set(&mut app, "Profile length", 2.0);
        set(&mut app, "Line width", 3.0);
        click(&mut app, "Ok");
        assert!(!app.series_settings.is_open);
        let params = app.chart.chart_type_params();
        assert!(!params.tpo_blocks && params.tpo_volume_profile);
        assert!(params.tpo_volume.poc && params.tpo_volume.left);
        assert_eq!(
            (params.tpo_profile_period, params.tpo_profile_periods),
            (TPOProfilePeriod::Week, 2)
        );
        assert_eq!(params.styles.tpo_price_source, PriceSource::High);
        assert_eq!(params.styles.tpo_line.width, 3.0);

        let study = IndicatorFactory::new()
            .create("Time Price Opportunity")
            .unwrap();
        let id = app.chart.add_indicator(study);
        app.open_indicator_settings(id);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        for label in [
            "Statistics",
            "Total volume",
            "Profile period",
            "Weeks",
            "Value area high",
            "Volume profile",
            "Volume point of control",
            "Apply",
        ] {
            click(&mut app, label);
        }
        assert!(!app.indicator_settings.is_open());
        let inputs = app.chart.indicator_mut(id).unwrap().inputs();
        for (input, expected) in [
            ("statistics", InputValue::Toggle(true)),
            ("statistics_total_volume", InputValue::Toggle(true)),
            ("profile_period", InputValue::Choice(1)),
            ("value_area_high", InputValue::Toggle(false)),
            ("volume_poc", InputValue::Toggle(true)),
        ] {
            assert_eq!(
                inputs.iter().find(|i| i.id == input).map(|i| &i.value),
                Some(&expected),
                "{input}"
            );
        }
    }

    /// The demo's series and study settings switch on extensions of TPO levels, which the alert
    /// dialog then offers as sources, and switching them off withdraws them.
    #[test]
    fn tpo_level_extensions_reach_the_demo_alert_dialog() {
        use egui::accesskit::{Action, ActionRequest, TreeId};
        use egui_charts::model::{AlertSource, ChartType, TPOLevel};
        use egui_charts::studies::{IndicatorFactory, InputValue};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T17:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        let click = |app: &mut DemoApp, label: &str| {
            let nodes = draw(&ctx, app, vec![]);
            let target_node = control(&nodes, label).0;
            draw(
                &ctx,
                app,
                vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target_tree: TreeId::ROOT,
                    target_node,
                    data: None,
                })],
            );
        };
        // The TPO levels the alert dialog offers, closing it again.
        let offered = |app: &mut DemoApp| {
            app.open_alert(None);
            let levels: Vec<_> = (app.alert_dialog.sources.iter())
                .filter_map(|(source, name)| match source {
                    AlertSource::TPOLevel(level) => Some((level.level, name.to_string())),
                    _ => None,
                })
                .collect();
            app.alert_dialog.close();
            levels
        };
        app.chart.set_chart_type(ChartType::TimePriceOpportunity);
        app.handle_chart_context(ContextMenuAction::OpenSeriesSettings);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        for label in ["Extend point of control until touched", "Ok"] {
            click(&mut app, label);
        }
        assert!(!app.series_settings.is_open);
        assert!(app.chart.chart_type_params().tpo_extend_poc);
        let levels = offered(&mut app);
        assert!(
            !levels.is_empty()
                && levels.iter().all(|(level, name)| {
                    *level == TPOLevel::PointOfControl && name.starts_with("TPO ")
                }),
            "{levels:?}"
        );

        let study = IndicatorFactory::new()
            .create("Time Price Opportunity")
            .unwrap();
        let id = app.chart.add_indicator(study);
        app.open_indicator_settings(id);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        for label in ["Extend value area high until touched", "Apply"] {
            click(&mut app, label);
        }
        assert!(!app.indicator_settings.is_open());
        let levels = offered(&mut app);
        assert!(
            levels.iter().any(|(level, name)| {
                *level == TPOLevel::ValueAreaHigh && name.starts_with("Time Price Opportunity ")
            }),
            "{levels:?}"
        );

        let mut params = app.chart.chart_type_params().clone();
        params.tpo_extend_poc = false;
        app.chart.set_chart_type_params(params);
        (app.chart.indicator_mut(id).unwrap())
            .set_input("extend_vah", InputValue::Toggle(false))
            .unwrap();
        let levels = offered(&mut app);
        assert!(levels.is_empty(), "{levels:?}");
    }

    /// The dialog saves a footprint condition and frequency, then the demo forwards the
    /// execution that completes its stack exactly once to the alert engine.
    #[test]
    fn footprint_alerts_reach_the_demo_alert_dialog() {
        use egui::accesskit::{Action, ActionRequest, TreeId};
        use egui_charts::data::MarketEvent;
        use egui_charts::model::{
            AlertFrequency, ChartTypeParams, FootprintCondition,
            OrderSide::{Buy, Sell},
            Tick,
        };
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-04T17:30:00Z".parse().unwrap(),
            false,
        );
        app.live.set_paused(true);
        finish_feed(&app.live);
        settle(&mut app, &ctx);
        app.open_alert(None);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        for label in [
            "Condition",
            "Footprint",
            "Footprint condition",
            "Stacked buy imbalance",
            "Frequency",
            "Once per bar",
            "Save alert",
        ] {
            let nodes = draw(&ctx, &mut app, vec![]);
            let target_node = control(&nodes, label).0;
            draw(
                &ctx,
                &mut app,
                vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target_tree: TreeId::ROOT,
                    target_node,
                    data: None,
                })],
            );
        }
        let engine = app.alert_host.engine();
        let saved = engine.alerts();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].condition,
            AlertCondition::Footprint(FootprintCondition::StackedBuyImbalance)
        );
        assert_eq!(saved[0].frequency, AlertFrequency::OncePerBar);
        let detectors = app.live.detectors();
        let mut settings = detectors.settings();
        settings
            .footprint
            .rule
            .follow_chart(&ChartTypeParams::default(), 1);
        detectors.set_settings(settings);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Min1));
        let bars = app.chart.bars(SeriesId::MAIN);
        let last = bars.last().unwrap();
        let b = last.time + chrono::TimeDelta::minutes(1);
        let tick = app.chart.instrument().unwrap().tick_size.unwrap();
        let p = (last.close / tick).round() * tick;
        let symbol = app.chart.symbol().to_owned();
        let fired = engine.triggers().len();
        for (second, row, size, side) in [
            (1, 0.0, 1.0, Sell),
            (2, 1.0, 1.0, Sell),
            (3, 2.0, 1.0, Sell),
            (4, 1.0, 12.0, Buy),
            (5, 2.0, 9.0, Buy),
            (6, 3.0, 15.0, Buy),
        ] {
            let time = b + chrono::TimeDelta::seconds(second);
            detectors.push(
                &symbol,
                time,
                &MarketEvent::Trade(Tick::new(time, p + row * tick, size, Some(side))),
            );
        }
        app.monitor_alerts(&ctx);
        let triggers = app.alert_host.engine().triggers();
        assert_eq!(triggers.len(), fired + 1);
        let trigger = triggers.last().unwrap();
        assert_eq!(trigger.id, saved[0].id.clone().unwrap());
        assert_eq!(trigger.time, b + chrono::TimeDelta::seconds(6));
        assert_eq!(trigger.price, Some(p + 3.0 * tick));
        assert_eq!(trigger.message, "Stacked buy imbalance of 3 rows");
        let time = b + chrono::TimeDelta::seconds(7);
        app.live.detectors().push(
            &symbol,
            time,
            &MarketEvent::Trade(Tick::new(time, p + 3.0 * tick, 15.0, Some(Buy))),
        );
        app.monitor_alerts(&ctx);
        assert_eq!(app.alert_host.engine().triggers().len(), fired + 1);
    }

    /// Study settings use the own-data chart's zones and apply the edited session clock.
    #[test]
    fn study_settings_edit_session_time_and_offer_the_own_data_zone() {
        use egui::accesskit::{Action, ActionData, ActionRequest, TreeId};
        use egui_charts::model::TimezoneId;
        use egui_charts::studies::{Indicator, InputValue, TradingSessions};
        use owndata::feed::tests::{Server, pages, profile};

        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        let server = Server::start(pages(vec!["[]".to_owned()]));
        let mut own = profile(&server.origin);
        own.time_zone = TimezoneId::fixed_offset(19_800).unwrap();
        assert!(!default_time_zones().contains(&own.time_zone));
        app.connect_own_data(own, Vec::new());
        assert!(app.own_data.is_some());
        let mut study = TradingSessions::new();
        study
            .set_input("start [0]", InputValue::Length(570))
            .unwrap();
        let id = app.chart.add_indicator(Box::new(study));
        app.open_indicator_settings(id);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        let nodes = draw(&ctx, &mut app, vec![]);
        let hours = control(&nodes, "First session start hours");
        assert_eq!(hours.1.value(), Some("09"));
        assert_eq!(
            control(&nodes, "First session start minutes").1.value(),
            Some("30")
        );
        draw(
            &ctx,
            &mut app,
            vec![egui::Event::AccessKitActionRequest(ActionRequest {
                action: Action::SetValue,
                target_tree: TreeId::ROOT,
                target_node: hours.0,
                data: Some(ActionData::NumericValue(10.0)),
            })],
        );
        let nodes = draw(&ctx, &mut app, vec![]);
        assert_eq!(
            control(&nodes, "First session start hours").1.value(),
            Some("10")
        );
        for label in ["First session time zone", "+05:30", "Apply"] {
            let nodes = draw(&ctx, &mut app, vec![]);
            let bounds = control(&nodes, label).1.bounds().unwrap();
            let at = egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            );
            ui_click(&ctx, egui::vec2(1440.0, 900.0), at, |ui| {
                app.show_dialogs(ui.ctx());
            });
        }
        assert!(!app.indicator_settings.is_open());
        let inputs = app.chart.indicator_mut(id).unwrap().inputs();
        for (input, expected) in [
            (
                "start [0]",
                InputValue::TimeOfDay(chrono::NaiveTime::from_hms_opt(10, 30, 0).unwrap()),
            ),
            ("zone [0]", InputValue::TimeZone("+05:30".to_owned())),
        ] {
            assert_eq!(
                inputs.iter().find(|i| i.id == input).map(|i| &i.value),
                Some(&expected),
                "{input}"
            );
        }
    }

    /// The demo reaches delta dashes, the retained custom interval, interval anchors and ticks.
    #[test]
    fn lower_interval_volume_studies_offer_their_new_settings() {
        use egui::accesskit::{Action, ActionRequest, Role, Toggled, TreeId};
        use egui_charts::studies::{IndicatorFactory, InputValue};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        let click = |app: &mut DemoApp, node: &egui::accesskit::Node| {
            let b = node.bounds().unwrap();
            ui_click(
                &ctx,
                egui::vec2(1440.0, 900.0),
                egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
                |ui| {
                    app.show_dialogs(ui.ctx());
                },
            );
        };
        let factory = IndicatorFactory::new();
        let up_down = app
            .chart
            .add_indicator(factory.create("Up/Down Volume").unwrap());
        app.open_indicator_settings(up_down);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Style").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        let mut pickers: Vec<_> = nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::ComboBox && n.label() == Some("Plot type"))
            .collect();
        pickers.sort_by(|a, b| {
            a.1.bounds()
                .unwrap()
                .y0
                .total_cmp(&b.1.bounds().unwrap().y0)
        });
        assert_eq!(pickers.len(), 3);
        assert_eq!(pickers[2].1.value(), Some("Dashes"));
        click(&mut app, &control(&nodes, "Apply").1);
        assert!(!app.indicator_settings.is_open());
        let cvd = app
            .chart
            .add_indicator(factory.create("Cumulative Volume Delta").unwrap());
        app.open_indicator_settings(cvd);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        let nodes = draw(&ctx, &mut app, vec![]);
        assert_eq!(
            control(&nodes, "Use custom timeframe").1.toggled(),
            Some(Toggled::False)
        );
        assert!(
            !nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Lower interval"))
        );
        click(&mut app, &control(&nodes, "Use custom timeframe").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        control(&nodes, "Lower interval");
        click(&mut app, &control(&nodes, "Anchor period").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        draw(
            &ctx,
            &mut app,
            vec![egui::Event::AccessKitActionRequest(ActionRequest {
                action: Action::ScrollIntoView,
                target_tree: TreeId::ROOT,
                target_node: control(&nodes, "Interval").0,
                data: None,
            })],
        );
        draw(&ctx, &mut app, vec![]);
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Interval").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        control(&nodes, "Anchor interval (minutes)");
        click(&mut app, &control(&nodes, "Data aggregation").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Ticks").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Apply").1);
        assert!(!app.indicator_settings.is_open());
        let study = app.chart.indicator_mut(cvd).unwrap();
        for (id, want) in [
            ("custom_interval", InputValue::Toggle(true)),
            ("anchor", InputValue::Choice(7)),
            ("aggregation", InputValue::Choice(1)),
        ] {
            assert_eq!(
                study.inputs().iter().find(|i| i.id == id).map(|i| &i.value),
                Some(&want),
                "{id}"
            );
        }
        assert_eq!(study.order_flow_inputs().len(), 1);
    }

    /// The Style tab offers a plot the character its study declares, also for a plot saved as
    /// another type: a Ziv Ghost Pivots plot saved as crosses switches to its "8". The Inputs
    /// tab takes a Booker Reversal level with decimals.
    #[test]
    fn study_settings_offer_ziv_characters_and_decimal_booker_levels() {
        use egui::accesskit::{Action, ActionData, ActionRequest, Role, TreeId};
        use egui_charts::studies::{IndicatorFactory, InputValue, PlotType};

        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        // Clicks the middle of a control.
        let click = |app: &mut DemoApp, node: &egui::accesskit::Node| {
            let bounds = node.bounds().unwrap();
            let at = egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            );
            ui_click(&ctx, egui::vec2(1440.0, 900.0), at, |ui| {
                app.show_dialogs(ui.ctx());
            });
        };
        let factory = IndicatorFactory::new();
        let id = app
            .chart
            .add_indicator(factory.create("Ziv Ghost Pivots").unwrap());
        // The first plot as a study saved while the pivots were crosses holds it.
        let study = app.chart.indicator_mut(id).unwrap();
        let mut settings = study.settings();
        settings.plots[0].plot_type = PlotType::Crosses;
        study.apply_settings(&[], settings).unwrap();
        app.open_indicator_settings(id);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Style").1);
        let nodes = draw(&ctx, &mut app, vec![]);
        // AccessKit lists the nodes in no layout order: the first plot's picker is the topmost.
        let top = |node: &egui::accesskit::Node| node.bounds().unwrap().y0;
        let picker = nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::ComboBox && node.label() == Some("Plot type"))
            .min_by(|a, b| top(&a.1).total_cmp(&top(&b.1)))
            .expect("a plot type picker");
        assert_eq!(picker.1.value(), Some("Crosses"));
        click(&mut app, &picker.1);
        // The list is longer than its popup, so the entry is scrolled into view first: the list
        // moves in the frame after the one that asks.
        let nodes = draw(&ctx, &mut app, vec![]);
        let request = egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::ScrollIntoView,
            target_tree: TreeId::ROOT,
            target_node: control(&nodes, "Character 8").0,
            data: None,
        });
        draw(&ctx, &mut app, vec![request]);
        draw(&ctx, &mut app, vec![]);
        for label in ["Character 8", "Apply"] {
            let nodes = draw(&ctx, &mut app, vec![]);
            click(&mut app, &control(&nodes, label).1);
        }
        assert!(!app.indicator_settings.is_open());
        let study = app.chart.indicator_mut(id).unwrap();
        assert_eq!(study.plot_style(0).plot_type, PlotType::Character('8'));

        let id = app
            .chart
            .add_indicator(factory.create("Booker Reversal").unwrap());
        app.open_indicator_settings(id);
        for _ in 0..3 {
            draw(&ctx, &mut app, vec![]);
        }
        let nodes = draw(&ctx, &mut app, vec![]);
        let request = egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: control(&nodes, "Stochastic Upper").0,
            data: Some(ActionData::NumericValue(72.5)),
        });
        draw(&ctx, &mut app, vec![request]);
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Apply").1);
        assert!(!app.indicator_settings.is_open());
        let inputs = app.chart.indicator_mut(id).unwrap().inputs();
        let upper = inputs.iter().find(|input| input.id == "stoch_upper");
        assert_eq!(
            upper.map(|input| &input.value),
            Some(&InputValue::Number(72.5))
        );
    }

    /// The menu actions reach the other chart, which lacks nothing, without a prompt; a chart
    /// without bars is named first, and the copy waits for the answer.
    #[test]
    fn layout_copy_actions_reach_other_charts_and_prompt_for_what_they_lack() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        let id = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(5)));
        let studies = |chart: &Chart| chart.indicators().count();
        let before = studies(&app.extra_charts[0].chart);

        app.handle_study_context(&ctx, id, StudyContextMenuAction::AddToLayout);
        assert_eq!(studies(&app.extra_charts[0].chart), before + 1);
        assert!(app.layout_copy_prompt.is_none());

        app.handle_chart_context(ContextMenuAction::ApplyIndicatorsToLayout);
        assert_eq!(studies(&app.extra_charts[0].chart), studies(&app.chart));

        app.extra_charts[0]
            .chart
            .set_data(SeriesId::MAIN, Vec::new());
        app.handle_study_context(&ctx, id, StudyContextMenuAction::AddToLayout);
        let (_, message) = app.layout_copy_prompt.as_ref().expect("prompt");
        assert!(message.starts_with("Chart 2 ("), "{message}");
        assert_eq!(studies(&app.extra_charts[0].chart), studies(&app.chart));

        // Nothing could take this one, so there is no Apply to offer.
        app.layout_copy_prompt = None;
        let linked = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(3)));
        let source = egui_charts::model::StudySource::Study { id: id.0, plot: 0 };
        app.chart.set_indicator_source(linked, source).unwrap();
        app.handle_study_context(&ctx, linked, StudyContextMenuAction::AddToLayout);
        assert!(app.layout_copy_prompt.is_none());
    }

    pub(super) fn ui_frame(
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
        draw: impl FnMut(&mut egui::Ui),
    ) -> Vec<(String, egui::Rect)> {
        ui_frame_input(
            ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            draw,
        )
    }

    fn ui_frame_input(
        ctx: &egui::Context,
        input: egui::RawInput,
        mut draw: impl FnMut(&mut egui::Ui),
    ) -> Vec<(String, egui::Rect)> {
        let output = ctx.run_ui(input, |ui| draw(ui));
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_owned(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                )),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    fn hotlist_frame(
        app: &mut DemoApp,
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        ui_frame_input(
            ctx,
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
        )
    }

    fn hotlist_click(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let texts = hotlist_frame(app, ctx, 10.0, vec![]);
        let point = texts
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label}: {texts:?}"))
            .1
            .center();
        hotlist_pointer_click(app, ctx, point, egui::PointerButton::Primary);
    }

    fn hotlist_pointer_click(
        app: &mut DemoApp,
        ctx: &egui::Context,
        point: egui::Pos2,
        button: egui::PointerButton,
    ) {
        hotlist_frame(app, ctx, 10.0, vec![egui::Event::PointerMoved(point)]);
        for pressed in [true, false] {
            hotlist_frame(
                app,
                ctx,
                10.0,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        hotlist_frame(app, ctx, 10.0, vec![]);
    }

    /// The open demo panel narrows the real catalog, shows its ranking volume and asks for
    /// moving quotes at the configured interval in the chart's zone.
    #[test]
    fn demo_hotlist_filters_figures_and_interval_are_reachable() {
        let ctx = egui::Context::default();
        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.open_panel = Some(SidePanel::Hotlists);
        apply_timezone(&mut app.chart, &mut app.timeframe_toolbar, "Asia/Tokyo");
        settle(&mut app, &ctx);
        hotlist_click(&mut app, &ctx, "All exchanges");
        hotlist_click(&mut app, &ctx, "Stocks");
        egui::Popup::close_all(&ctx);
        hotlist_click(&mut app, &ctx, "All countries");
        hotlist_click(&mut app, &ctx, "EU");
        egui::Popup::close_all(&ctx);
        let empty = hotlist_frame(&mut app, &ctx, 10.0, vec![]);
        assert!(
            DemoSymbol::catalog().iter().all(|spec| !empty
                .iter()
                .any(|(text, rect)| rect.left() > 1000.0 && text == spec.symbol)),
            "{empty:?}"
        );
        hotlist_click(&mut app, &ctx, "EU");
        hotlist_click(&mut app, &ctx, "US");
        hotlist_click(&mut app, &ctx, "2 countries");
        hotlist_click(&mut app, &ctx, "EU");
        egui::Popup::close_all(&ctx);
        hotlist_click(&mut app, &ctx, "Top gainers");
        hotlist_click(&mut app, &ctx, "Most active");
        egui::Popup::close_all(&ctx);
        let texts = hotlist_frame(&mut app, &ctx, 10.0, vec![]);
        let has = |label: &str| {
            texts
                .iter()
                .any(|(text, rect)| rect.left() > 1000.0 && text == label)
        };
        assert!(has("Stocks") && has("US") && has("Volume"), "{texts:?}");
        let mut volumes = Vec::new();
        for spec in DemoSymbol::catalog() {
            let shown = has(spec.symbol);
            assert_eq!(
                shown,
                spec.exchange == "Stocks",
                "{}: {texts:?}",
                spec.symbol
            );
            if shown {
                let quote = app.live.market_quote(spec.symbol, now).unwrap();
                let volume = quote.volume.unwrap();
                let value = format!("{volume:.0}");
                let symbol_rect = texts
                    .iter()
                    .find(|(text, _)| text == spec.symbol)
                    .unwrap()
                    .1;
                assert!(
                    texts.iter().any(|(text, rect)| text == &value
                        && (rect.center().y - symbol_rect.center().y).abs() < 2.0),
                    "{texts:?}"
                );
                volumes.push((symbol_rect.top(), volume));
            }
        }
        volumes.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(volumes.windows(2).all(|pair| pair[0].1 >= pair[1].1));
        assert!(
            texts
                .iter()
                .any(|(text, _)| text.starts_with("Last update at ") && text.ends_with("GMT+9")),
            "{texts:?}"
        );
        let before = format!("{:.2}", app.live.market_quote("STOCKA", now).unwrap().last);
        app.advance_sample_to(now + chrono::Duration::minutes(5));
        let after = format!(
            "{:.2}",
            app.live
                .market_quote("STOCKA", app.live.now())
                .unwrap()
                .last
        );
        assert_ne!(before, after);
        let early = hotlist_frame(&mut app, &ctx, 69.9, vec![]);
        assert!(early.iter().any(|(text, _)| text == &before), "{early:?}");
        hotlist_frame(&mut app, &ctx, 70.0, vec![]);
        let refreshed = hotlist_frame(&mut app, &ctx, 70.1, vec![]);
        assert!(
            refreshed.iter().any(|(text, _)| text == &after),
            "{refreshed:?}"
        );
    }

    /// Clicks the screener text `label` right of the chart. The filter row paints before the table
    /// and its popups after it, so a control is the first panel text of its label and a popup
    /// choice the last.
    fn screener_click(app: &mut DemoApp, ctx: &egui::Context, label: &str, last: bool) {
        let texts = hotlist_frame(app, ctx, 10.0, vec![]);
        let mut found = texts
            .iter()
            .filter(|(text, rect)| text == label && rect.left() > 1000.0);
        let rect = if last { found.last() } else { found.next() };
        let point = rect
            .unwrap_or_else(|| panic!("missing {label}: {texts:?}"))
            .1
            .center();
        hotlist_pointer_click(app, ctx, point, egui::PointerButton::Primary);
    }

    /// The demo screener filters its catalog by exchange: the host declares the text column and
    /// its values, the filter row sends "is" and "is not" with the ticked values, and the feed
    /// answers the matching symbols.
    #[test]
    fn demo_screener_filters_the_catalog_by_exchange() {
        let ctx = egui::Context::default();
        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.open_panel = Some(SidePanel::Screener);
        settle(&mut app, &ctx);
        let click =
            |app: &mut DemoApp, label: &str, last: bool| screener_click(app, &ctx, label, last);
        let tick = |app: &mut DemoApp, shown: &str, value: &str| {
            egui::Popup::close_all(&ctx);
            click(app, shown, false);
            click(app, value, true);
            egui::Popup::close_all(&ctx);
        };
        // The demo feed answers screens at once; the next frame shows the rows.
        let shown = |app: &mut DemoApp| {
            hotlist_frame(app, &ctx, 10.0, vec![]);
            let texts = hotlist_frame(app, &ctx, 10.0, vec![]);
            DemoSymbol::catalog()
                .iter()
                .filter(|spec| {
                    texts
                        .iter()
                        .any(|(text, rect)| rect.left() > 1000.0 && text == spec.symbol)
                })
                .map(|spec| spec.symbol)
                .collect::<Vec<_>>()
        };
        // Exchange is a Valuation column, so the filter offers it after switching sets.
        click(&mut app, "Overview", false);
        click(&mut app, "Valuation", true);
        click(&mut app, "Price", false);
        click(&mut app, "Exchange", true);
        tick(&mut app, "Choose values", "Crypto");
        tick(&mut app, "Crypto", "Forex");
        click(&mut app, "Add filter", false);
        let crypto_or_forex: Vec<_> = DemoSymbol::catalog()
            .iter()
            .filter(|spec| matches!(spec.exchange, "Crypto" | "Forex"))
            .map(|spec| spec.symbol)
            .collect();
        assert_eq!(shown(&mut app), crypto_or_forex);

        click(&mut app, "Exchange is Crypto, Forex  ×", false);
        click(&mut app, "is", false);
        click(&mut app, "is not", true);
        tick(&mut app, "Choose values", "Crypto");
        click(&mut app, "Add filter", false);
        let others: Vec<_> = DemoSymbol::catalog()
            .iter()
            .filter(|spec| spec.exchange != "Crypto")
            .map(|spec| spec.symbol)
            .collect();
        assert_eq!(shown(&mut app), others);
    }

    /// The demo screener's "+", heading menu, chart view and refresh choice are reachable: an added
    /// column heads its values, a heading's menu removes its column, the chart view lays the
    /// screened symbols out in taller rows, and a picked refresh interval replaces the dropdown's
    /// label, which the panel shows whole.
    #[test]
    fn demo_screener_columns_charts_and_refresh_are_reachable() {
        let ctx = egui::Context::default();
        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.open_panel = Some(SidePanel::Screener);
        settle(&mut app, &ctx);
        // The demo feed answers at once; the next frame shows the answer.
        let panel = |app: &mut DemoApp| {
            hotlist_frame(app, &ctx, 10.0, vec![]);
            let mut texts = hotlist_frame(app, &ctx, 10.0, vec![]);
            texts.retain(|(_, rect)| rect.left() > 1000.0);
            texts
        };
        screener_click(&mut app, &ctx, "+", false);
        screener_click(&mut app, &ctx, "Mkt cap", true);
        let texts = panel(&mut app);
        let heading = texts
            .iter()
            .find(|(text, _)| text == "Mkt cap")
            .unwrap_or_else(|| panic!("no Mkt cap heading: {texts:?}"))
            .1;
        assert!(
            texts
                .iter()
                .any(|(text, rect)| rect.top() > heading.bottom()
                    && (rect.right() - heading.right()).abs() < 2.0
                    && text.ends_with(['K', 'M', 'B', 'T'])),
            "{texts:?}"
        );

        // A right-aligned heading is painted from its right end, which the frame helper reports
        // as the left edge, so the click goes just inside that end.
        let change = texts
            .iter()
            .find(|(text, _)| text == "Chg %")
            .unwrap_or_else(|| panic!("no Chg % heading: {texts:?}"))
            .1
            .left_center()
            - egui::vec2(2.0, 0.0);
        hotlist_pointer_click(&mut app, &ctx, change, egui::PointerButton::Secondary);
        screener_click(&mut app, &ctx, "Remove column", true);
        let texts = panel(&mut app);
        assert!(
            texts.iter().any(|(text, _)| text == "Mkt cap")
                && !texts.iter().any(|(text, _)| text == "Chg %"),
            "{texts:?}"
        );

        screener_click(&mut app, &ctx, "Chart", false);
        let texts = panel(&mut app);
        let mut tops: Vec<f32> = DemoSymbol::catalog()
            .iter()
            .filter_map(|spec| texts.iter().find(|(text, _)| text == spec.symbol))
            .map(|(_, rect)| rect.top())
            .collect();
        tops.sort_by(f32::total_cmp);
        // Table rows are 26 points apart, chart rows 48.
        assert!(
            tops.len() > 1 && tops.windows(2).all(|pair| pair[1] - pair[0] > 47.0),
            "{tops:?}"
        );

        screener_click(&mut app, &ctx, "No auto refresh", false);
        screener_click(&mut app, &ctx, "Refresh every 10 seconds", true);
        let texts = panel(&mut app);
        let painted = |label: &str| texts.iter().any(|(text, _)| text == label);
        assert!(
            painted("Refresh every 10 seconds") && !painted("No auto refresh"),
            "{texts:?}"
        );
        // A cut label still carries its whole text, so ask the galley whether it was cut.
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(10.0),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| eframe::App::ui(&mut *app, ui, &mut eframe::Frame::_new_kittest()),
        );
        let cut = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text() == "Refresh every 10 seconds" && text.galley.elided)
        });
        output.drop_without_applying_deltas();
        assert!(!cut, "the refresh choice is cut short");
    }

    /// The list menu copies the shown hotlist, in its order, into a new watchlist named after it
    /// and into an existing one, appending only the symbols that list lacks, and shows the list.
    #[test]
    fn demo_hotlist_copies_into_a_new_and_a_given_watchlist() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.open_panel = Some(SidePanel::Hotlists);
        settle(&mut app, &ctx);
        hotlist_click(&mut app, &ctx, "Top gainers");
        hotlist_click(&mut app, &ctx, "Most volatile");
        egui::Popup::close_all(&ctx);
        let texts = hotlist_frame(&mut app, &ctx, 10.0, vec![]);
        let heading = texts
            .iter()
            .find(|(text, _)| text == "Symbol")
            .unwrap_or_else(|| panic!("no Symbol heading: {texts:?}"))
            .1;
        let mut rows: Vec<&(String, egui::Rect)> = texts
            .iter()
            .filter(|(text, rect)| {
                DemoSymbol::catalog().iter().any(|spec| spec.symbol == text)
                    && (rect.left() - heading.left()).abs() < 1.0
                    && rect.top() > heading.top()
            })
            .collect();
        rows.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        let shown: Vec<String> = rows.into_iter().map(|(text, _)| text.clone()).collect();
        assert_eq!(shown.len(), DemoSymbol::catalog().len(), "{texts:?}");

        hotlist_click(&mut app, &ctx, "Most volatile");
        hotlist_click(&mut app, &ctx, "Copy to new watchlist");
        let lists = app.watchlist.lists();
        let copy = lists.last().unwrap();
        assert_eq!(copy.name, "Most volatile");
        assert_eq!(copy.symbols().collect::<Vec<_>>(), shown);
        assert_eq!(app.watchlist.active_list(), lists.len() - 1);
        assert!(app.open_panel == Some(SidePanel::Watchlist));

        let old: Vec<String> = lists[0].symbols().map(str::to_owned).collect();
        let mut expected = old.clone();
        expected.extend(shown.iter().filter(|symbol| !old.contains(symbol)).cloned());
        assert!(expected.len() > old.len(), "nothing left to append");
        app.open_panel = Some(SidePanel::Hotlists);
        hotlist_click(&mut app, &ctx, "Most volatile");
        hotlist_click(&mut app, &ctx, "Copy to Watchlist");
        assert_eq!(
            app.watchlist.lists()[0].symbols().collect::<Vec<_>>(),
            expected
        );
        assert_eq!(app.watchlist.active_list(), 0);
    }

    /// Row menus update the demo watchlist, and starring a list survives the real settings save
    /// and restart, including the menu order.
    #[test]
    fn demo_hotlist_watchlists_and_stars_survive_restart() {
        use eframe::App;
        let ctx = egui::Context::default();
        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.open_panel = Some(SidePanel::Hotlists);
        settle(&mut app, &ctx);
        hotlist_click(&mut app, &ctx, "Top gainers");
        hotlist_click(&mut app, &ctx, "Most volatile");
        egui::Popup::close_all(&ctx);
        let texts = hotlist_frame(&mut app, &ctx, 10.0, vec![]);
        let (symbol, rect) = texts
            .iter()
            .find(|(text, _)| {
                DemoSymbol::catalog().iter().any(|spec| {
                    spec.symbol == text
                        && !app.watchlist.lists()[0]
                            .symbols()
                            .any(|symbol| symbol == text)
                })
            })
            .unwrap();
        let symbol = symbol.clone();
        let point = rect.center();
        hotlist_pointer_click(&mut app, &ctx, point, egui::PointerButton::Secondary);
        hotlist_click(&mut app, &ctx, &format!("Add {symbol} to watchlist"));
        let destination = app.watchlist.lists()[0].name.clone();
        hotlist_click(&mut app, &ctx, &destination);
        assert!(
            app.watchlist.lists()[0]
                .symbols()
                .any(|entry| entry == symbol)
        );
        egui::Popup::close_all(&ctx);
        hotlist_click(&mut app, &ctx, "Most volatile");
        let texts = hotlist_frame(&mut app, &ctx, 10.0, vec![]);
        let title = texts
            .iter()
            .find(|(text, _)| text == "Top losers")
            .unwrap()
            .1;
        let star = egui::pos2(title.left() - 20.0, title.center().y);
        hotlist_pointer_click(&mut app, &ctx, star, egui::PointerButton::Primary);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        let documents: std::collections::HashMap<String, String> =
            eframe::get_value(&storage, "templates").unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&documents["Settings/settings"]).unwrap();
        assert_eq!(
            settings["hotlist_favorites"],
            serde_json::json!(["Top losers"])
        );
        egui::Popup::close_all(&ctx);
        let ctx = egui::Context::default();
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new_at(&cc, now, true);
        restored.open_panel = Some(SidePanel::Hotlists);
        settle(&mut restored, &ctx);
        assert_eq!(restored.user_settings().hotlist_favorites, ["Top losers"]);
        egui::Popup::close_all(&ctx);
        hotlist_click(&mut restored, &ctx, "Top gainers");
        let mut texts = hotlist_frame(&mut restored, &ctx, 10.0, vec![]);
        if !texts.iter().any(|(text, _)| text == "Top losers") {
            hotlist_click(&mut restored, &ctx, "Top gainers");
            texts = hotlist_frame(&mut restored, &ctx, 10.0, vec![]);
        }
        let row_y = |label| {
            texts
                .iter()
                .filter(|(text, _)| text == label)
                .map(|(_, rect)| rect.top())
                .max_by(f32::total_cmp)
                .unwrap_or_else(|| panic!("missing {label}: {texts:?}"))
        };
        assert!(row_y("Top losers") < row_y("Top gainers"), "{texts:?}");
    }

    pub(super) fn ui_click(
        ctx: &egui::Context,
        size: egui::Vec2,
        point: egui::Pos2,
        mut draw: impl FnMut(&mut egui::Ui),
    ) {
        ui_frame(ctx, size, vec![egui::Event::PointerMoved(point)], &mut draw);
        ui_frame(
            ctx,
            size,
            [true, false]
                .map(|pressed| egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
                .to_vec(),
            draw,
        );
    }

    /// The gap above the account panel resizes it, preserves the chosen height across both
    /// size toggles, and stops before hiding either the chart or the account header.
    #[test]
    fn account_panel_edge_drag_resizes_restores_and_clamps_the_height() {
        use eframe::App;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.connect_paper();
        let size = egui::vec2(1440.0, 900.0);
        let frame = |app: &mut DemoApp, events| {
            ui_frame(&ctx, size, events, |ui| {
                app.ui(ui, &mut eframe::Frame::_new_kittest())
            })
        };
        let positions_y = |texts: &[(String, egui::Rect)]| {
            texts
                .iter()
                .find(|(text, rect)| text == "Positions" && rect.top() > 100.0)
                .expect("account Positions tab")
                .1
                .top()
        };
        let drag = |app: &mut DemoApp, from: f32, to: f32| {
            let from = egui::pos2(700.0, from);
            let to = egui::pos2(700.0, to);
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(app, vec![egui::Event::PointerMoved(from)]);
            frame(app, vec![button(from, true)]);
            frame(app, vec![egui::Event::PointerMoved(from.lerp(to, 0.5))]);
            frame(app, vec![egui::Event::PointerMoved(to)]);
            frame(app, vec![button(to, false)]);
            frame(app, vec![])
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let before = positions_y(&frame(&mut app, vec![]));
        // At 320 points high, the middle of the 4 point gap is y = 582.
        let moved = positions_y(&drag(&mut app, 582.0, 432.0));
        assert_eq!(before - moved, 150.0);
        assert_eq!(app.account_panel_height, 470.0);

        for state in [AccountPanelSize::Minimized, AccountPanelSize::Maximized] {
            app.account_panel = state;
            frame(&mut app, vec![]);
            frame(&mut app, vec![]);
            app.account_panel = AccountPanelSize::Normal;
            frame(&mut app, vec![]);
            assert_eq!(positions_y(&frame(&mut app, vec![])), moved, "{state:?}");
        }

        let upper = size.y - app.top_toolbar.config.height - 4.0 - 96.0;
        let high = positions_y(&drag(&mut app, 432.0, 0.0));
        assert_eq!(app.account_panel_height, upper);
        assert_eq!(before - high, upper - 320.0);
        let texts = drag(&mut app, size.y + 2.0 - upper, 899.0);
        assert_eq!(app.account_panel_height, 42.0);
        let header = texts
            .iter()
            .find(|(text, _)| text == "Paper Trading")
            .expect("account header remains visible")
            .1;
        assert!(header.top() >= size.y - 42.0 && header.bottom() <= size.y);
    }

    /// Changing brokers through the account menu opens the demo's picker while keeping the
    /// paper account connected until another broker is chosen.
    #[test]
    fn account_panel_change_broker_opens_the_picker_without_disconnecting() {
        use eframe::App;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.connect_paper();
        let size = egui::vec2(1440.0, 900.0);
        let draw =
            |app: &mut DemoApp, ui: &mut egui::Ui| app.ui(ui, &mut eframe::Frame::_new_kittest());
        for _ in 0..3 {
            ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        }
        assert!(!app.broker_picker.is_open());
        assert!(app.trading.has_broker());
        let texts = ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        // The broker selector is below the account header's copy of the same name.
        let broker = texts
            .iter()
            .filter(|(text, _)| text == "Paper Trading")
            .max_by(|(_, a), (_, b)| a.top().total_cmp(&b.top()))
            .expect("paper broker selector")
            .1
            .center();
        ui_click(&ctx, size, broker, |ui| draw(&mut app, ui));
        for _ in 0..3 {
            ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        }
        let texts = ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        let change = texts
            .iter()
            .find(|(text, _)| text == "Change broker")
            .expect("Change broker menu row")
            .1
            .center();
        ui_click(&ctx, size, change, |ui| draw(&mut app, ui));
        ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        assert!(app.broker_picker.is_open());
        assert!(app.trading.has_broker());
    }

    /// The paper account with its working order, the order presets, the broker connection and
    /// the account panel's height come back after a restart, and storage saved without them
    /// opens with a new account as before.
    #[test]
    fn paper_account_presets_connection_and_panel_height_survive_a_restart() {
        use egui_charts::model::{OrderId, OrderRequest, OrderSide, OrderType, TimeInForce};
        use egui_charts::services::PaperAccount;
        use egui_charts::ui::trading::OrderPreset;

        let ctx = egui::Context::default();
        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let start = |storage: Option<&AppStorage>| {
            let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
            cc.storage = storage.map(|storage| storage as &dyn eframe::Storage);
            DemoApp::new_at(&cc, now, true)
        };
        let mut app = start(None);
        app.connect_paper();
        let symbol = app.chart.symbol().to_owned();
        app.paper.set_price(&symbol, 100.0);
        // A buy limit below the price keeps working.
        let mut request = OrderRequest::new(&symbol, OrderSide::Buy, 2.0, OrderType::Limit(99.0));
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let (answer, reply) = egui_charts::data::ReplyReader::<OrderId>::new();
        egui_charts::services::Broker::place(&*app.paper, request, reply);
        answer.take().expect("limit reply").expect("working limit");
        let mut two = OrderPreset::new("Two", 2.0);
        two.applied = true;
        app.trading.presets.insert(String::new(), vec![two]);
        app.account_panel_height = 470.0;
        let mut storage = AppStorage::default();
        eframe::App::save(&mut *app, &mut storage);

        // No frame runs, so no price reaches either account between the two saves.
        let restored = start(Some(&storage));
        assert_eq!(restored.account_panel_height, 470.0);
        assert_eq!(restored.trading.presets, app.trading.presets);
        assert!(restored.trading.has_broker());
        let saved = restored.paper.save();
        assert_eq!(saved, app.paper.save());
        assert_eq!(
            serde_json::to_value(&saved).unwrap()["working"][0]["request"]["order_type"]["Limit"],
            99.0
        );

        // Storage saved before these keys existed opens as before.
        for key in [
            "paper_account",
            "order_presets",
            "paper_connected",
            "account_panel_height",
        ] {
            storage.0.remove(key);
        }
        let old = start(Some(&storage));
        assert_eq!(old.account_panel_height, 320.0);
        assert!(old.trading.presets.is_empty());
        assert!(!old.trading.has_broker());
        assert_eq!(
            old.paper.save(),
            PaperAccount::new("USD", 100_000.0).leverage(10.0).save()
        );
    }

    /// A binding saved in the shortcuts dialog reaches every chart of the layout, and it is in
    /// effect again after a restart on the same storage until settings without bindings arrive.
    #[test]
    fn shortcuts_dialog_bindings_reach_every_chart_and_survive_a_restart() {
        use egui::accesskit::Role;
        use egui_charts::config::KeyboardAction;

        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let alt_b = egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::B);
        let page_left = |app: &DemoApp| -> Vec<Option<egui::KeyboardShortcut>> {
            std::iter::once(&*app.chart)
                .chain(app.extra_charts.iter().map(|extra| &*extra.chart))
                .map(|chart| chart.shortcuts().shortcut(KeyboardAction::PanLeftFast))
                .collect()
        };
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        finish_feed(&app.live);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.keyboard_shortcuts.open();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                app,
            );
        h.run_steps(3);
        assert_ne!(page_left(h.state())[0], Some(alt_b));
        h.get_by_role_and_label(Role::Button, "Edit")
            .click_accesskit();
        // The edit mode is a window of its own, which sizes itself for a frame before it shows.
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Change Pan left by a page")
            .click_accesskit();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::B);
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Save")
            .click_accesskit();
        h.run_steps(3);
        assert_eq!(page_left(h.state()), [Some(alt_b); 2]);
        let mut storage = AppStorage::default();
        eframe::App::save(&mut **h.state_mut(), &mut storage);

        let ctx = egui::Context::default();
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new_at(&cc, now, true);
        settle(&mut restored, &ctx);
        assert_eq!(page_left(&restored), [Some(alt_b); 2]);

        // Settings from another client that hold no bindings bring back the defaults.
        let mut settings = restored.user_settings();
        settings.shortcuts = None;
        restored.apply_user_settings(&ctx, settings);
        settle(&mut restored, &ctx);
        let default =
            egui_charts::widget::ShortcutMap::default().shortcut(KeyboardAction::PanLeftFast);
        assert_eq!(page_left(&restored), [default; 2]);
    }

    /// Bulk removal waits for a lock decision, saves only remembered decisions, and restores
    /// the exact drawings and study ids together in one undo step without removing price series.
    #[test]
    fn toolbar_bulk_removal_resolves_locks_before_editing_either_group() {
        use eframe::App;
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        use egui_charts::ui::drawing_toolbar::utilities::RemoveMenu;

        for (combined, include, remember, cancel) in [
            (false, false, false, false),
            (false, true, true, false),
            (true, false, true, false),
            (true, true, false, false),
            (true, false, true, true),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            settle(&mut app, &ctx);
            app.chart
                .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
            let protected = app
                .chart
                .add_indicator(Box::new(egui_charts::studies::SMA::new(9)));
            app.chart.set_indicator_user_edit_enabled(protected, false);
            let study_ids: Vec<_> = app.chart.indicators().map(|(id, _)| id).collect();
            let added = app.chart.add_series(Default::default());
            let main_bars = app.chart.bars(SeriesId::MAIN).to_vec();
            for (id, locked) in [(41, false), (42, true)] {
                let mut drawing = Drawing::new(id, DrawingToolType::HorizontalLine);
                drawing.chart_points = vec![ChartPoint::new(20.0, 100.0)];
                drawing.completed = true;
                drawing.locked = locked;
                app.chart.drawings_mut().drawings.push(drawing);
            }
            let originals = app.chart.drawings().drawings.clone();
            let size = egui::vec2(1440.0, 900.0);
            let draw_menu = |app: &mut DemoApp, ui: &mut egui::Ui| {
                let action = RemoveMenu::show_submenu(ui, &app.drawing_toolbar.state);
                if let Some(action) = action {
                    app.handle_drawing_toolbar(action);
                }
            };
            let texts = ui_frame(&ctx, size, vec![], |ui| draw_menu(&mut app, ui));
            let row = if combined {
                "Remove drawings and indicators"
            } else {
                "Remove all drawings"
            };
            let at = texts
                .iter()
                .find(|(text, _)| text == row)
                .unwrap()
                .1
                .center();
            ui_click(&ctx, size, at, |ui| draw_menu(&mut app, ui));
            assert_eq!(app.chart.drawings().drawings.len(), 2);
            assert_eq!(
                app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                study_ids
            );
            let draw_prompt = |app: &mut DemoApp, ui: &mut egui::Ui| {
                let action =
                    RemoveMenu::show_confirmation(ui.ctx(), &mut app.drawing_toolbar.state);
                app.handle_drawing_toolbar(action);
            };
            for label in
                remember
                    .then_some("Remember my choice")
                    .into_iter()
                    .chain(std::iter::once(if cancel {
                        "Cancel"
                    } else if include {
                        "Remove including locked"
                    } else {
                        "Keep locked"
                    }))
            {
                let mut texts = Vec::new();
                for _ in 0..3 {
                    texts = ui_frame(&ctx, size, vec![], |ui| draw_prompt(&mut app, ui));
                }
                let at = texts
                    .iter()
                    .find(|(text, _)| text == label)
                    .expect("removal dialog control")
                    .1
                    .center();
                ui_click(&ctx, size, at, |ui| draw_prompt(&mut app, ui));
            }
            assert_eq!(
                app.chart.drawings().drawings.len(),
                if cancel { 2 } else { usize::from(!include) }
            );
            let expected_studies = if combined && !cancel {
                vec![protected]
            } else {
                study_ids.clone()
            };
            assert_eq!(
                app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                expected_studies
            );
            assert!(app.chart.is_series_visible(SeriesId::MAIN));
            assert_eq!(app.chart.bars(SeriesId::MAIN), main_bars);
            assert!(app.chart.series().any(|(id, _)| id == added));
            if !cancel {
                assert!(app.chart.undo());
                assert_eq!(
                    app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                    study_ids
                );
                let restored = &app.chart.drawings().drawings;
                assert_eq!(
                    serde_json::to_value(restored).unwrap(),
                    serde_json::to_value(&originals).unwrap()
                );
                assert!(app.chart.redo());
                assert_eq!(
                    app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                    expected_studies
                );
            }
            let mut storage = AppStorage::default();
            app.save(&mut storage);
            let mut cc = eframe::CreationContext::_new_kittest(egui::Context::default());
            cc.storage = Some(&storage);
            let mut reopened = DemoApp::new(&cc);
            reopened.chart.drawings_mut().drawings = originals;
            reopened.handle_drawing_toolbar(DrawingToolbarAction::RemoveAllDrawings);
            assert_eq!(
                reopened.chart.drawings().drawings.len(),
                if remember && !cancel {
                    usize::from(!include)
                } else {
                    2
                }
            );
        }
    }

    /// The menu remains reachable while the drawing side panel is hidden and after reopening.
    #[test]
    fn drawing_toolbar_visibility_survives_reopening() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.drawing_toolbar
            .state
            .favorites
            .push(egui_charts::drawings::DrawingToolType::TrendLine);
        let size = egui::vec2(1440.0, 900.0);
        let toggle = |app: &mut DemoApp, ctx: &egui::Context| {
            ui_click(ctx, size, egui::pos2(20.0, 20.0), |ui| {
                app.show_top_toolbar(ui, app.theme.ui.panel_bg)
            });
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ui_frame(ctx, size, vec![], |ui| {
                    app.show_top_toolbar(ui, app.theme.ui.panel_bg)
                });
            }
            let at = texts
                .iter()
                .find(|(text, _)| text == "Drawing tools")
                .expect("toolbar toggle")
                .1
                .center();
            ui_click(ctx, size, at, |ui| {
                app.show_top_toolbar(ui, app.theme.ui.panel_bg)
            });
        };
        toggle(&mut app, &ctx);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        let restored_ctx = egui::Context::default();
        let mut cc = eframe::CreationContext::_new_kittest(restored_ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        for hidden in [true, false] {
            let mut consumed = 0.0;
            ui_frame(&restored_ctx, size, vec![], |ui| {
                let width = ui.available_width();
                app.show_drawing_toolbar(ui, app.theme.ui.panel_bg);
                consumed = width - ui.available_width();
            });
            assert_eq!(consumed == 0.0, hidden);
            assert!(app.drawing_toolbar.state.floating_pos.is_some());
            if hidden {
                toggle(&mut app, &restored_ctx);
            }
        }
    }

    /// The real Hide menu follows the broker and settings, and each row changes only its layer.
    #[test]
    fn hide_menu_rows_toggle_trading_layers() {
        use egui_charts::model::{OrderId, OrderRequest, OrderSide, OrderType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        let size = egui::vec2(1440.0, 900.0);
        let menu = |app: &mut DemoApp| {
            app.drawing_toolbar.state.expanded_category = Some("Hide".into());
            egui::Popup::open_id(&ctx, egui::Id::unique(("drawing_toolbar_flyout", "Hide")));
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ui_frame(&ctx, size, vec![], |ui| {
                    app.show_drawing_toolbar(ui, app.theme.chart.bg)
                });
            }
            texts
        };
        let choose = |app: &mut DemoApp, label: &str| {
            let texts = menu(app);
            let point = texts
                .iter()
                .find(|(text, _)| text == label)
                .expect("hide menu row")
                .1
                .center();
            ui_click(&ctx, size, point, |ui| {
                app.show_drawing_toolbar(ui, app.theme.chart.bg)
            });
        };
        let texts = menu(&mut app);
        for name in ["Hide positions", "Hide orders", "Hide executions"] {
            assert!(!texts.iter().any(|(text, _)| text == name));
        }
        egui::Popup::close_all(&ctx);
        let symbol = app.chart.symbol().to_owned();
        let last = app.chart.bars(SeriesId::MAIN).last().unwrap().close;
        let limit = last * 0.99;
        app.chart
            .price_scale_mut()
            .set_range(last * 0.97, last * 1.03);
        app.paper.set_price(&symbol, last);
        app.trading.set_service(app.paper.clone());
        app.trading.poll(&ctx, &mut app.toasts);
        ui_frame(&ctx, size, vec![], |ui| {
            app.chart.show(ui);
            app.trading.show(ui, &mut app.chart);
        });
        app.trading.poll(&ctx, &mut app.toasts);
        app.trading.display.instant_orders = true;
        app.trading.trade(symbol.clone(), OrderSide::Buy);
        app.trading.poll(&ctx, &mut app.toasts);
        let mut request = OrderRequest::new(&symbol, OrderSide::Buy, 1.0, OrderType::Limit(limit));
        request.time_in_force = egui_charts::model::TimeInForce::GoodTillCancelled;
        let (answer, reply) = egui_charts::data::ReplyReader::<OrderId>::new();
        egui_charts::services::Broker::place(&*app.paper, request, reply);
        answer.take().expect("limit reply").expect("working limit");
        app.trading.poll(&ctx, &mut app.toasts);
        // Settle the pane mapping before checking the rendered labels.
        for _ in 0..3 {
            ui_frame(&ctx, size, vec![], |ui| {
                app.chart.show(ui);
                app.trading.show(ui, &mut app.chart);
            });
        }
        for (label, expected) in [
            (None, (true, true, true)),
            (Some("Hide positions"), (false, true, true)),
            (Some("Hide orders"), (false, false, true)),
            (Some("Hide executions"), (false, false, false)),
            (Some("Hide positions"), (true, false, false)),
            (Some("Hide orders"), (true, true, false)),
            (Some("Hide executions"), (true, true, true)),
            (Some("Hide all"), (false, false, false)),
            (Some("Hide all"), (true, true, true)),
        ] {
            if let Some(label) = label {
                choose(&mut app, label);
            }
            let texts = ui_frame(&ctx, size, vec![], |ui| {
                app.chart.show(ui);
                app.trading.show(ui, &mut app.chart);
            });
            let display = &app.trading.display;
            assert_eq!(
                (
                    display.show_positions,
                    display.show_orders,
                    display.show_executions
                ),
                expected,
                "{label:?}"
            );
            let mapping = app.chart.get_chart_mapping();
            let has_label = |caption: &str, price| {
                texts.iter().any(|(text, rect)| {
                    text == caption
                        && mapping.rect.contains(rect.center())
                        && (rect.center().y - mapping.price_to_y(price)).abs() < 2.0
                })
            };
            assert_eq!(has_label("1", last), expected.0, "position: {label:?}");
            assert_eq!(has_label("Limit", limit), expected.1, "order: {label:?}");
            assert_eq!(
                texts.iter().any(|(text, _)| text == "Buy 1"),
                expected.2,
                "execution: {label:?}"
            );
            if label == Some("Hide all") {
                assert_eq!(app.chart.drawings().is_hidden(), !expected.0);
            }
        }
        let mut state = app.chart_settings_state();
        state.trading.show_orders = false;
        app.apply_chart_settings(state);
        menu(&mut app);
        assert!(app.drawing_toolbar.state.hide_orders);
        app.trading.disconnect();
        let texts = menu(&mut app);
        assert!(!texts.iter().any(|(text, _)| text == "Hide orders"));
        assert!(!app.trading.display.show_orders);
        app.trading.set_service(app.paper.clone());
        menu(&mut app);
        assert!(app.drawing_toolbar.state.hide_orders);
    }

    /// The order label menu separates same-price orders and updates the settings the demo saves.
    #[test]
    fn no_overlap_in_a_label_menu_moves_orders_at_one_price_apart() {
        use egui_charts::model::{OrderId, OrderRequest, OrderSide, OrderType, TimeInForce};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        let size = egui::vec2(1440.0, 900.0);
        let symbol = app.chart.symbol().to_owned();
        let last = app.chart.bars(SeriesId::MAIN).last().unwrap().close;
        let limit = last * 0.99;
        app.chart
            .price_scale_mut()
            .set_range(last * 0.97, last * 1.03);
        app.paper.set_price(&symbol, last);
        app.trading.set_service(app.paper.clone());
        app.trading.poll(&ctx, &mut app.toasts);
        ui_frame(&ctx, size, vec![], |ui| {
            app.chart.show(ui);
            app.trading.show(ui, &mut app.chart);
        });
        app.trading.poll(&ctx, &mut app.toasts);
        for quantity in [1.0, 3.0] {
            let mut request =
                OrderRequest::new(&symbol, OrderSide::Buy, quantity, OrderType::Limit(limit));
            request.time_in_force = TimeInForce::GoodTillCancelled;
            let (answer, reply) = egui_charts::data::ReplyReader::<OrderId>::new();
            egui_charts::services::Broker::place(&*app.paper, request, reply);
            answer.take().expect("limit reply").expect("working limit");
        }
        app.trading.poll(&ctx, &mut app.toasts);
        let draw = |app: &mut DemoApp, ui: &mut egui::Ui| {
            app.chart.show(ui);
            app.trading.show(ui, &mut app.chart);
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        }
        let mapping = app.chart.get_chart_mapping();
        let y0 = mapping.price_to_y(limit);
        let centres = |texts: &[(String, egui::Rect)]| {
            ["1", "3"].map(|quantity| {
                texts
                    .iter()
                    .find(|(text, rect)| {
                        text == quantity
                            && mapping.rect.contains(rect.center())
                            && (rect.center().y - y0).abs() < 44.0
                    })
                    .expect("order quantity")
                    .1
                    .center()
            })
        };
        let [one, three] = centres(&texts);
        assert!((one.y - y0).abs() < 2.0 && (three.y - y0).abs() < 2.0);
        assert!((one.y - three.y).abs() < 0.5);
        ui_frame(&ctx, size, vec![egui::Event::PointerMoved(three)], |ui| {
            draw(&mut app, ui)
        });
        ui_frame(
            &ctx,
            size,
            [true, false]
                .map(|pressed| egui::Event::PointerButton {
                    pos: three,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
                .to_vec(),
            |ui| draw(&mut app, ui),
        );
        let texts = ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        let switch = texts
            .iter()
            .find(|(text, _)| text == "No overlap")
            .expect("No overlap switch")
            .1
            .center();
        ui_click(&ctx, size, switch, |ui| draw(&mut app, ui));
        assert!(app.trading.display.separate_labels);
        assert!(app.chart_settings_state().trading.separate_labels);
        let texts = ui_frame(&ctx, size, vec![], |ui| draw(&mut app, ui));
        let [one, three] = centres(&texts);
        assert!((one.y - three.y).abs() > 20.0);
        assert!((one.y - y0).abs() < 2.0 || (three.y - y0).abs() < 2.0);
    }

    /// The toolbar's real flyout toggles studies in both directions, including after undo.
    #[test]
    fn hide_indicator_menu_restores_studies_and_follows_undo() {
        let is_visible = |chart: &Chart, id| {
            assert!(
                chart
                    .indicators()
                    .find(|(study, _)| *study == id)
                    .unwrap()
                    .1
                    .is_visible()
            );
            chart.is_series_visible(id)
        };
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        let id = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
        let size = egui::vec2(1440.0, 900.0);
        let choose = |app: &mut DemoApp, label: &str| {
            app.drawing_toolbar.state.expanded_category = Some("Hide".into());
            egui::Popup::open_id(&ctx, egui::Id::unique(("drawing_toolbar_flyout", "Hide")));
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ui_frame(&ctx, size, vec![], |ui| {
                    app.show_drawing_toolbar(ui, app.theme.chart.bg)
                });
            }
            let point = texts
                .iter()
                .find(|(text, _)| text == label)
                .expect("hide menu row")
                .1
                .center();
            ui_click(&ctx, size, point, |ui| {
                app.show_drawing_toolbar(ui, app.theme.chart.bg)
            });
        };
        for label in [
            "Hide indicators",
            "Hide indicators",
            "Hide all",
            "Hide all",
            "Hide all",
            "Hide indicators",
        ] {
            let visible = !is_visible(&app.chart, id);
            choose(&mut app, label);
            assert_eq!(is_visible(&app.chart, id), visible, "{label}");
        }
        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        ui_frame(&ctx, size, vec![], |ui| {
            app.show_drawing_toolbar(ui, app.theme.chart.bg)
        });
        assert!(app.drawing_toolbar.state.hide_indicators);
        choose(&mut app, "Hide indicators");
        assert!(is_visible(&app.chart, id));
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.activate_chart_slot(1);
        app.chart.set_indicators_hidden(true);
        ui_frame(&ctx, size, vec![], |ui| {
            app.show_drawing_toolbar(ui, app.theme.chart.bg)
        });
        assert!(app.drawing_toolbar.state.hide_indicators);
        app.activate_chart_slot(0);
        ui_frame(&ctx, size, vec![], |ui| {
            app.show_drawing_toolbar(ui, app.theme.chart.bg)
        });
        assert!(!app.drawing_toolbar.state.hide_indicators);
        choose(&mut app, "Hide indicators");
        settle(&mut app, &ctx);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Save);
        app.poll_storage(&ctx);
        choose(&mut app, "Hide indicators");
        app.handle_layout_action(egui_charts::ui::top_toolbar::LayoutAction::Open(
            "Unnamed".into(),
        ));
        app.poll_storage(&ctx);
        let restored = app.chart.indicators().next().unwrap().0;
        choose(&mut app, "Hide indicators");
        assert!(!is_visible(&app.chart, restored));
    }

    /// The cursor menu switches show the chart's readout, which Show readout also sets, and its
    /// close-only snapping, which the magnet keeps and the layout saves; one click turns either on
    /// or off.
    #[test]
    fn cursor_menu_switches_follow_the_chart() {
        use egui::accesskit::{Role, Toggled};
        use egui_charts::config::CrosshairMode;
        use egui_charts::ui::drawing_toolbar::MagnetType;

        // One frame of `draw`: the toggled state and centre of the control named `label`.
        fn control(
            ctx: &egui::Context,
            label: &str,
            draw: impl FnMut(&mut egui::Ui),
        ) -> (Option<Toggled>, egui::Pos2) {
            ctx.enable_accesskit();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                draw,
            );
            let found = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.role() != Role::Label && node.label() == Some(label)).then(|| {
                        let bounds = node.bounds().expect("control bounds");
                        let centre = egui::pos2(
                            (bounds.x0 + bounds.x1) as f32 * 0.5,
                            (bounds.y0 + bounds.y1) as f32 * 0.5,
                        );
                        (node.toggled(), centre)
                    })
                });
            output.drop_without_applying_deltas();
            found.unwrap_or_else(|| panic!("missing control: {label}"))
        }

        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        // Opens the cursor menu and reads its switch named `label`; a click closes the menu.
        let cursor_switch = |app: &mut DemoApp, label: &str| {
            app.drawing_toolbar.state.expanded_category = Some("Cursors".into());
            egui::Popup::open_id(
                &ctx,
                egui::Id::unique(("drawing_toolbar_flyout", "Cursors")),
            );
            for _ in 0..2 {
                ui_frame(&ctx, size, vec![], |ui| {
                    app.show_drawing_toolbar(ui, app.theme.chart.bg)
                });
            }
            control(&ctx, label, |ui| {
                app.show_drawing_toolbar(ui, app.theme.chart.bg)
            })
        };
        let values = "Values tooltip on long press";

        assert!(app.chart.chart_options().tooltip.enabled);
        let (_, point) = control(&ctx, "Show readout", |ui| app.show_readout_controls(ui));
        ui_click(&ctx, size, point, |ui| app.show_readout_controls(ui));
        assert!(!app.chart.chart_options().tooltip.enabled);

        let (state, point) = cursor_switch(&mut app, values);
        assert_eq!(state, Some(Toggled::False));
        ui_click(&ctx, size, point, |ui| {
            app.show_drawing_toolbar(ui, app.theme.chart.bg)
        });
        assert!(app.chart.chart_options().tooltip.enabled);

        let (state, point) = cursor_switch(&mut app, values);
        assert_eq!(state, Some(Toggled::True));
        ui_click(&ctx, size, point, |ui| {
            app.show_drawing_toolbar(ui, app.theme.chart.bg)
        });
        assert!(!app.chart.chart_options().tooltip.enabled);
        let (state, _) = control(&ctx, "Show readout", |ui| app.show_readout_controls(ui));
        assert_eq!(state, Some(Toggled::False));

        // Reads the close switch, which must show `before`, then clicks it.
        let flip_closes = |app: &mut DemoApp, before: Toggled| {
            let (state, point) = cursor_switch(app, "Snap crosshair to closes");
            assert_eq!(state, Some(before));
            ui_click(&ctx, size, point, |ui| {
                app.show_drawing_toolbar(ui, app.theme.chart.bg)
            });
        };
        // Hovers the high of the bar in the middle of the chart whose close sits furthest below
        // it, and returns the heights of the crosshair price, that high and that close.
        let hover_high = |app: &mut DemoApp| {
            let frame = |app: &mut DemoApp, events| {
                ui_frame(&ctx, size, events, |ui| {
                    app.chart.show(ui);
                    app.drain_chart_events(ui.ctx());
                });
            };
            for _ in 0..3 {
                frame(app, vec![]);
            }
            let map = app.chart.get_chart_mapping();
            let middle = map.rect.shrink2(map.rect.size() * 0.25);
            let (x, high, close) = app
                .chart
                .state()
                .shown()
                .iter()
                .enumerate()
                .map(|(index, bar)| {
                    let x = map.idx_to_x(index);
                    (x, map.price_to_y(bar.high), map.price_to_y(bar.close))
                })
                .filter(|&(x, high, _)| middle.contains(egui::pos2(x, high)))
                .max_by(|a, b| (a.2 - a.1).total_cmp(&(b.2 - b.1)))
                .expect("a bar in the middle of the chart");
            assert!(close - high > 4.0, "{high} {close}");
            frame(app, vec![egui::Event::PointerMoved(egui::pos2(x, high))]);
            let price = app.crosshair.clone().expect("hovered").price;
            (map.price_to_y(price), high, close)
        };
        let mode = |app: &DemoApp| app.chart.chart_options().crosshair.mode;
        // The main close is the only close at a bar, so the switch reads the close, not the high.
        assert!(app.comparisons.is_empty());
        app.handle_drawing_toolbar(DrawingToolbarAction::SetMagnetType(MagnetType::Strong));
        assert_eq!(mode(&app), CrosshairMode::Magnet);
        let (price, high, _) = hover_high(&mut app);
        assert!((price - high).abs() < 0.1, "{price} {high}");
        flip_closes(&mut app, Toggled::False);
        assert_eq!(mode(&app), CrosshairMode::MagnetClose);
        let (price, _, close) = hover_high(&mut app);
        assert!((price - close).abs() < 0.1, "{price} {close}");
        // Switching the magnet off keeps closes.
        app.handle_drawing_toolbar(DrawingToolbarAction::ToggleMagnet);
        assert!(!app.chart.drawings().options.magnet_mode);
        assert_eq!(mode(&app), CrosshairMode::MagnetClose);
        let (price, _, close) = hover_high(&mut app);
        assert!((price - close).abs() < 0.1, "{price} {close}");
        // The layout saves it, and the switch shows it again once the layout is opened.
        settle(&mut app, &ctx);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Save);
        app.poll_storage(&ctx);
        app.chart.set_crosshair_mode(CrosshairMode::Normal);
        app.handle_layout_action(egui_charts::ui::top_toolbar::LayoutAction::Open(
            "Unnamed".into(),
        ));
        app.poll_storage(&ctx);
        assert_eq!(mode(&app), CrosshairMode::MagnetClose);
        // Switched off, the magnet decides again: no snapping while the magnet is off.
        flip_closes(&mut app, Toggled::True);
        assert!(!app.chart.drawings().options.magnet_mode);
        assert_eq!(mode(&app), CrosshairMode::Normal);
        settle(&mut app, &ctx);
        let (price, high, _) = hover_high(&mut app);
        assert!((price - high).abs() < 0.1, "{price} {high}");
        // A strength picked while the switch is on keeps closes; off again, the magnet snaps.
        flip_closes(&mut app, Toggled::False);
        app.handle_drawing_toolbar(DrawingToolbarAction::SetMagnetType(MagnetType::Weak));
        assert_eq!(mode(&app), CrosshairMode::MagnetClose);
        flip_closes(&mut app, Toggled::True);
        assert_eq!(mode(&app), CrosshairMode::Magnet);
    }

    /// Menu picks read settings, restored charts and history instead of host-side flags.
    #[test]
    fn heatmap_menu_follows_settings_slots_history_and_symbol_depth() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        let mut settings = ChartSettingsState::from_chart(&app.chart);
        settings.order_flow.show_heatmap = true;
        settings.order_flow.order_book.profile = BookProfile::LargestOrder;
        settings.order_flow.order_book.quantity_unit = Some("lots".into());
        settings.order_flow.show_volume_bubbles = true;
        settings.order_flow.volume_bubbles.max_diameter = 91.0;
        app.apply_chart_settings(settings);
        // Replay hides the live history; the toolbar must leave it for the replay's exit.
        let columns = |app: &DemoApp| {
            app.chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .config()
                .columns
        };
        let mut config = DepthHistoryConfig::default();
        config.columns = 7;
        app.chart
            .set_depth_history(Some(DepthHistory::new(config).unwrap()));
        app.chart
            .set_replay_time(Some(app.chart.bars(SeriesId::MAIN)[5].time));
        assert!(app.chart.depth_history(SeriesId::MAIN).is_none());
        ui_frame(&ctx, size, vec![], |ui| {
            app.show_top_toolbar(ui, app.theme.chart.bg)
        });
        app.chart.set_replay_time(None);
        assert_eq!(columns(&app), 7);
        app.switch_symbol(live::ORDER_LEVEL_SYMBOL);
        assert!(
            app.chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .config()
                .orders
                .is_some()
        );
        app.switch_symbol("STOCKA");
        assert!(
            app.chart
                .depth_history(SeriesId::MAIN)
                .unwrap()
                .config()
                .orders
                .is_none()
        );
        let choose = |app: &mut DemoApp, label: &str| {
            for _ in 0..3 {
                ui_frame(&ctx, size, vec![], |ui| {
                    app.show_top_toolbar(ui, app.theme.chart.bg)
                });
            }
            ui_click(&ctx, size, egui::pos2(20.0, 20.0), |ui| {
                app.show_top_toolbar(ui, app.theme.chart.bg)
            });
            let texts = ui_frame(&ctx, size, vec![], |ui| {
                app.show_top_toolbar(ui, app.theme.chart.bg)
            });
            let point = texts
                .iter()
                .find(|(text, _)| text == label)
                .expect("demo menu row")
                .1
                .center();
            ui_click(&ctx, size, point, |ui| {
                app.show_top_toolbar(ui, app.theme.chart.bg)
            });
            egui::Popup::close_all(&ctx);
        };
        choose(&mut app, "Largest order profile");
        assert_eq!(app.chart.order_book_view().profile, BookProfile::Total);
        assert_eq!(
            app.chart.order_book_view().quantity_unit.as_deref(),
            Some("lots")
        );
        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        assert_eq!(
            app.chart.order_book_view().profile,
            BookProfile::LargestOrder
        );
        choose(&mut app, "Liquidity heatmap");
        assert!(
            !ChartSettingsState::from_chart(&app.chart)
                .order_flow
                .show_heatmap
        );
        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        assert!(
            ChartSettingsState::from_chart(&app.chart)
                .order_flow
                .show_heatmap
        );
        // The menu brings the bubbles back as they were set, not with their defaults.
        choose(&mut app, "Liquidity heatmap");
        choose(&mut app, "Liquidity heatmap");
        assert_eq!(
            ChartSettingsState::from_chart(&app.chart)
                .order_flow
                .volume_bubbles
                .max_diameter,
            91.0
        );
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.activate_chart_slot(1);
        app.chart.set_liquidity_heatmap(false);
        choose(&mut app, "Liquidity heatmap");
        assert!(
            ChartSettingsState::from_chart(&app.chart)
                .order_flow
                .show_heatmap
        );
        app.activate_chart_slot(0);
        choose(&mut app, "Liquidity heatmap");
        assert!(
            !ChartSettingsState::from_chart(&app.chart)
                .order_flow
                .show_heatmap
        );
        app.chart.set_liquidity_heatmap(true);
        let mut view = app.chart.order_book_view();
        view.profile = BookProfile::LargestOrder;
        app.chart.set_order_book_view(view);
        settle(&mut app, &ctx);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Save);
        app.poll_storage(&ctx);
        app.chart.set_liquidity_heatmap(false);
        app.handle_layout_action(egui_charts::ui::top_toolbar::LayoutAction::Open(
            "Unnamed".into(),
        ));
        app.poll_storage(&ctx);
        choose(&mut app, "Largest order profile");
        assert_eq!(app.chart.order_book_view().profile, BookProfile::Total);
        choose(&mut app, "Liquidity heatmap");
        assert!(
            !ChartSettingsState::from_chart(&app.chart)
                .order_flow
                .show_heatmap
        );
    }

    pub(super) fn finish_feed(feed: &LiveFeed) {
        for _ in 0..10_000 {
            if !feed.loading() {
                return;
            }
            feed.advance(feed.now());
        }
        panic!("feed loading did not finish");
    }

    pub(super) fn finish_spreads(chart: &mut Chart, spreads: &mut [comparison::Spread]) {
        for _ in 0..10_000 {
            comparison::refresh_spreads(chart, spreads, true);
            if !spreads.iter().any(|spread| spread.loading(chart)) {
                return;
            }
        }
        panic!("spread loading did not finish");
    }

    pub(super) fn settle(app: &mut DemoApp, ctx: &egui::Context) {
        for _ in 0..4 {
            finish_feed(&app.live);
            finish_feed(&app.alert_live);
            finish_spreads(&mut app.chart, &mut app.spreads);
            for extra in &mut app.extra_charts {
                finish_feed(&extra.live);
                for (_, _, feed) in &extra.comparisons {
                    finish_feed(feed);
                }
                finish_spreads(&mut extra.chart, &mut extra.spreads);
            }
            for (_, _, feed) in &app.comparisons {
                finish_feed(feed);
            }
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            )
            .drop_without_applying_deltas();
        }
        assert!(!app.live.loading());
    }

    /// Cached bars paint immediately while bootstrap yields to input, then symbol details
    /// use the complete history.
    #[test]
    fn initial_history_is_visible_while_bootstrap_keeps_keyboard_input_responsive() {
        fn texts(
            app: &mut DemoApp,
            ctx: &egui::Context,
            events: Vec<egui::Event>,
        ) -> Vec<(String, f32)> {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            );
            let texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos.y)),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        }
        fn frame(app: &mut DemoApp, ctx: &egui::Context, events: Vec<egui::Event>) -> bool {
            let texts = texts(app, ctx, events);
            assert!(
                !app.chart.bars(SeriesId::MAIN).is_empty(),
                "history must be visible on every frame, including the first"
            );
            assert!(
                texts.iter().all(|(text, _)| !text.contains("Loading")),
                "bootstrap must not paint a loading indicator"
            );
            app.live.loading()
        }
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.open_panel = Some(SidePanel::Watchlist);
        let now = app.live.now();
        app.live.set_paused(true);
        assert!(
            frame(&mut app, &ctx, Vec::new()),
            "showing history must not finish bootstrap on the first frame"
        );
        assert!(frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("STOCKB".into())]
        ));
        assert!(
            app.symbol_search.is_open(),
            "loading must not swallow keyboard input"
        );
        app.symbol_search.close();
        for _ in 0..10_000 {
            if !frame(&mut app, &ctx, Vec::new()) {
                assert!(
                    app.chart
                        .bars(SeriesId::MAIN)
                        .last()
                        .is_some_and(|bar| bar.time >= now - chrono::TimeDelta::minutes(1)),
                    "the retained executions must reach the initial clock"
                );
                let texts = texts(&mut app, &ctx, Vec::new());
                let week = texts
                    .iter()
                    .find(|(text, _)| text == "1W")
                    .expect("the details panel shows performance")
                    .1;
                assert!(
                    texts
                        .iter()
                        .any(|(text, y)| (y - week).abs() < 2.0 && text.ends_with('%')),
                    "the weekly return must use the complete history"
                );
                return;
            }
        }
        panic!("paused history loading must finish");
    }

    /// Symbol navigation keeps earlier edits and synchronizes the host on undo and redo.
    #[test]
    fn symbol_navigation_undo_redo_keeps_the_demo_on_the_chart_symbol() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-10-27T19:29:50Z".parse().unwrap(),
            true,
        );
        app.switch_symbol("STOCKA");
        settle(&mut app, &ctx);
        let original_symbol = app.state.symbol.clone();
        let original_type = app.chart.chart_type();
        let original_bars = app.chart.bars(SeriesId::MAIN).to_vec();
        app.chart.apply_synced_time_scale(8.0, -30.0);
        let original_view = app.chart.get_time_scale_state();
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Line));
        app.switch_symbol("TOKENA");
        settle(&mut app, &ctx);
        let new_bars = app.chart.bars(SeriesId::MAIN).to_vec();
        assert_ne!(original_bars, new_bars);
        app.start_replay();
        assert!(app.replay.is_some());

        for (action, symbol, kind, session) in [
            (
                TopToolbarAction::Undo,
                original_symbol.as_str(),
                ChartType::Line,
                SessionType::RTH,
            ),
            (
                TopToolbarAction::Undo,
                original_symbol.as_str(),
                original_type,
                SessionType::RTH,
            ),
            (
                TopToolbarAction::Redo,
                original_symbol.as_str(),
                ChartType::Line,
                SessionType::RTH,
            ),
            (
                TopToolbarAction::Redo,
                "TOKENA",
                ChartType::Line,
                SessionType::Full24x7,
            ),
        ] {
            app.handle_top_toolbar(&ctx, action);
            assert_eq!(app.chart.symbol(), symbol);
            assert_eq!(app.state.symbol, symbol);
            assert_eq!(app.live.instrument().symbol, symbol);
            assert_eq!(app.state.chart_type, kind);
            assert_eq!(app.timeframe_toolbar.state.session_type, session);
            let bars = if symbol == "TOKENA" {
                &new_bars
            } else {
                &original_bars
            };
            assert_eq!(app.chart.bars(SeriesId::MAIN), bars);
            assert_eq!(&app.bars, bars);
            if symbol == original_symbol {
                assert_eq!(app.chart.get_time_scale_state(), original_view);
            }
            assert!(app.replay.is_none());
            assert!(!app.state.replay_active);
            assert!(app.chart.replay_time().is_none());
        }
        assert!(!app.chart.can_redo());
    }

    /// Drawing double clicks reach the properties dialog without taking over inline text editing.
    #[test]
    fn drawing_double_click_opens_geometry_settings() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType as Tool};
        for (tool, settings) in [
            (Tool::TrendLine, true),
            (Tool::Rect, true),
            (Tool::TextLabel, false),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                "2026-10-27T19:29:50Z".parse().unwrap(),
                true,
            );
            *app.chart = Chart::new(BarData::new());
            let mut drawing = Drawing::new(7, tool);
            drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 110.0)];
            drawing.text = Some("Annotation".into());
            drawing.completed = true;
            app.chart.drawings_mut().drawings.push(drawing);
            app.chart.set_data(
                SeriesId::MAIN,
                (0..40)
                    .map(|i| {
                        Bar::new(
                            chrono::DateTime::from_timestamp(i * 60, 0).unwrap(),
                            100.0,
                            120.0,
                            90.0,
                            105.0,
                            1.0,
                        )
                    })
                    .collect(),
            );
            let frame = |app: &mut DemoApp, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        time: Some(ctx.input(|i| i.time) + 0.1),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.chart.show(ui);
                        app.drain_chart_events(ui.ctx());
                    },
                )
                .drop_without_applying_deltas();
            };
            frame(&mut app, vec![]);
            let points = &app.chart.drawings().drawings[0].points;
            let pos = if settings {
                points[0].lerp(points[1], 0.5)
            } else {
                // On the text, which starts without a box around it.
                points[0] + egui::vec2(10.0, 5.0)
            };
            assert!(
                ctx.content_rect().contains(pos),
                "drawing must be on screen"
            );
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            for _ in 0..2 {
                frame(
                    &mut app,
                    [true, false]
                        .map(|pressed| egui::Event::PointerButton {
                            pos,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: egui::Modifiers::NONE,
                        })
                        .to_vec(),
                );
            }
            assert_eq!(app.drawing_props.is_open, settings, "{tool:?}");
            assert!(!app.settings_dialog.is_open);
        }
    }

    /// A card linking to a demo story shows that story, and its Open row opens only web links.
    #[test]
    fn post_cards_show_demo_stories_and_open_only_web_links() {
        use egui_charts::drawings::DrawingToolType as Tool;
        use egui_charts::model::PostLink;
        for (link, content, opens) in [
            (
                "https://example.com/news/STOCKA-0",
                Some((
                    "Sample stock A extends gains as buyers return",
                    "Demo Wire",
                    "STOCKA rose through the morning on steady volume, recovering the losses of the \
                     previous session. Buyers held the price above the opening range into the \
                     afternoon.",
                )),
                true,
            ),
            ("javascript://%0aalert(1)", None, false),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                "2026-10-27T19:29:50Z".parse().unwrap(),
                true,
            );
            settle(&mut app, &ctx);
            let frame = |app: &mut DemoApp, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        time: Some(ctx.input(|i| i.time) + 0.1),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.chart.show(ui);
                        app.drain_chart_events(ui.ctx());
                    },
                );
                let painted = std::mem::take(&mut output.shapes);
                let commands = std::mem::take(&mut output.platform_output.commands);
                output.drop_without_applying_deltas();
                (painted, commands)
            };
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            frame(&mut app, vec![]);
            app.chart
                .drawings_mut()
                .start_post_insertion(Tool::Tweet, PostLink::new(link));
            let rect = app.chart.get_rendered_price_rect();
            let pos = rect.min + rect.size() * egui::vec2(0.2, 0.3);
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            frame(&mut app, vec![button(pos, true)]);
            frame(&mut app, vec![button(pos, false)]);
            // The answer arrives at once and the next frame applies it; the clicks also expire.
            let mut painted = Vec::new();
            for _ in 0..4 {
                painted = frame(&mut app, vec![]).0;
            }
            let card = app
                .chart
                .drawings()
                .drawings
                .iter()
                .find(|drawing| drawing.tool_type == Tool::Tweet)
                .and_then(|drawing| drawing.post.as_ref())
                .expect("the placed card");
            let shown = card.content.as_ref();
            assert_eq!(
                shown.map(|c| (c.title.as_str(), c.author.as_str(), c.text.as_str())),
                content,
                "{link}"
            );
            assert!(shown.is_none_or(|c| c.picture.is_none()));
            // The card stands at the story's publication time; bring that time into view.
            if let Some(time) = shown.map(|content| content.time) {
                app.chart.scroll_to_time(time);
                for _ in 0..2 {
                    painted = frame(&mut app, vec![]).0;
                }
            }
            let open = painted
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == "Open" && rect.contains(text.pos) =>
                    {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
                    }
                    _ => None,
                })
                .expect("the card's Open row");
            let mut commands = frame(
                &mut app,
                vec![egui::Event::PointerMoved(open), button(open, true)],
            )
            .1;
            commands.extend(frame(&mut app, vec![button(open, false)]).1);
            assert_eq!(
                commands.iter().any(|command| matches!(
                    command,
                    egui::OutputCommand::OpenUrl(url) if url.url == link && url.new_tab
                )),
                opens,
                "{link}"
            );
        }
    }

    #[test]
    fn advancing_a_paused_sample_keeps_intermediate_bars() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let start = "2026-10-27T19:29:50Z".parse().unwrap();
        let mut app = DemoApp::new_at(&cc, start, true);
        settle(&mut app, &ctx);
        let end = start + chrono::Duration::minutes(3);
        app.advance_sample_to(end);
        assert!(app.live.paused());
        assert_eq!(app.live.now(), end);
        let bars = app.live.snapshot(
            Timeframe::Min1,
            egui_charts::model::BarClock::new(Timeframe::Min1),
            app.chart.extended_hours(),
        );
        let added: Vec<_> = bars.iter().filter(|bar| bar.time > start).collect();
        assert_eq!(added.len(), 3);
        assert!(added.iter().all(|bar| bar.volume > 0.0));
        assert!(
            added
                .windows(2)
                .all(|pair| pair[1].time - pair[0].time == chrono::Duration::minutes(1))
        );
    }

    #[test]
    fn replay_prices_fill_the_connected_paper_account() {
        use egui_charts::model::OrderSide;
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new(&cc);
        let symbol = app.chart.symbol().to_owned();
        let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars: Vec<_> = [100.0, 80.0, 110.0]
            .into_iter()
            .enumerate()
            .map(|(i, price)| {
                egui_charts::model::Bar::new(
                    start + chrono::Duration::minutes(i as i64),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect();
        app.chart.set_data(SeriesId::MAIN, bars);
        let paper = app.paper.clone();
        app.paper.set_price(&symbol, 999.0);
        app.trading.set_service(app.paper.clone());
        app.start_replay();
        app.trading.poll(&ctx, &mut app.toasts);
        app.trading.display.instant_orders = true;
        app.trading.trade(symbol.clone(), OrderSide::Buy);
        app.trading.poll(&ctx, &mut app.toasts);
        app.trading.trade(symbol.clone(), OrderSide::Buy);
        app.trading.poll(&ctx, &mut app.toasts);
        assert!(
            app.toasts
                .toasts()
                .iter()
                .any(|t| t.message == format!("Filled: Buy 1 {symbol} at 100.00"))
        );
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        app.trading.trade(symbol.clone(), OrderSide::Sell);
        app.trading.poll(&ctx, &mut app.toasts);
        assert!(
            app.toasts
                .toasts()
                .iter()
                .any(|t| t.message == format!("Filled: Sell 1 {symbol} at 110.00"))
        );
        assert!(Arc::ptr_eq(&paper, &app.paper));
        app.chart.fit_content();
        for toast in app.toasts.toasts_mut() {
            toast.duration = 0.0;
        }

        // A live quote must not price the ticket while the broker follows replay closes.
        for (side, width) in [(OrderSide::Buy, 1440.0), (OrderSide::Sell, 640.0)] {
            app.trading.set_quote(
                &symbol,
                egui_charts::model::Quote::new(999.0, 990.0, chrono::Utc::now())
                    .with_bid_ask(998.0, 1_000.0),
            );
            app.trading
                .open_ticket(egui_charts::model::OrderRequest::new(
                    &symbol,
                    side,
                    1.0,
                    egui_charts::model::OrderType::Market,
                ));
            let mut value = false;
            let mut marked = false;
            for pass in 0..3 {
                if pass == 1 {
                    app.chart.fit_content();
                }
                let mut ticket_left = 0.0;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        app.trading.show_ticket(ui, &app.chart);
                        ticket_left = ui.clip_rect().right();
                        app.show_chart(ui, app.theme.chart.bg);
                        app.show_notice(ui);
                    },
                );
                let has_text = |label| {
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Text(text)
                        if text.galley.text() == label)
                    })
                };
                value |= has_text("Trade value") && has_text("110.00");
                // The notices write the sides too, so look for the triangles rather than a label.
                let has_mark = |color| {
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Path(path)
                        if path.closed && path.points.len() == 3 && path.fill == color)
                    })
                };
                marked |= has_mark(app.theme.ui.accent) && has_mark(app.theme.chart.candle_down);
                if pass == 2 {
                    // The fills queue several notices, so the stack shows the newest and its controls.
                    let newest = app.toasts.toasts().last().expect("trade notice").id;
                    for id in [
                        egui::Id::unique("toast").with(newest),
                        egui::Id::unique("toast_stack").with(egui::Align2::LEFT_BOTTOM),
                    ] {
                        let rect = ctx.memory(|m| m.area_rect(id)).expect("trade notice");
                        assert!(
                            rect.right() <= ticket_left,
                            "notice covers ticket at {width}"
                        );
                    }
                }
                output.drop_without_applying_deltas();
            }
            assert!(value, "{side:?} ticket must use the replay close");
            assert!(
                marked,
                "replay fills must be marked on their historical bars"
            );
        }
    }

    /// A starred tool shows on the strip over the chart, and a click on it arms the tool.
    #[test]
    fn starred_tool_shows_on_the_favorites_strip_and_arms_on_click() {
        use eframe::App;
        use egui_charts::drawings::DrawingToolType;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.drawing_toolbar
            .toggle_favorite(DrawingToolType::TrendLine);
        let frame = |app: &mut DemoApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
            .drop_without_applying_deltas();
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        let strip = ctx
            .memory(|memory| memory.area_rect(egui::Id::NULL.with("floating_drawing_toolbar")))
            .expect("a starred tool shows the strip");
        // Past the grip, inside the first tool button.
        let pos = egui::pos2(strip.left() + 28.0, strip.center().y);
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(
            app.chart.drawings().active_tool(),
            Some(DrawingToolType::TrendLine)
        );
    }

    #[test]
    fn chart_click_selects_research_and_the_replay_start() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, "2026-11-03T02:00:00Z".parse().unwrap(), true);
        finish_feed(&app.live);
        app.apply_session();
        // Host signal buttons are separate click targets from the chart under test.
        app.chart.set_signal_source(None);
        let frame = |app: &mut DemoApp, events| {
            finish_feed(&app.live);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(ctx.input(|input| input.time) + 0.2),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if app.keyboard_dialog_open() {
                        app.chart.suppress_shortcuts(ui.ctx());
                    }
                    app.show_top_toolbar(ui, app.theme.ui.panel_bg);
                    app.show_chart(ui, app.theme.ui.panel_bg);
                    app.show_replay_start(ui.ctx());
                },
            );
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            shapes
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let map = app.chart.get_chart_mapping();
        let index = app.chart.state().shown().len() - 5;
        let time = app.chart.state().shown()[index].time;
        let pos = egui::pos2(map.idx_to_x(index), map.rect.center().y);
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        app.handle_top_toolbar(&ctx, TopToolbarAction::Host("research".into()));
        let message = &app.toasts.toasts().last().unwrap().message;
        assert!(
            message.contains(&time.to_rfc3339()),
            "{message}, expected {time}"
        );
        assert!(
            message.contains(&format!("price: {:.4}", map.y_to_price(pos.y))),
            "{message}"
        );
        // One click on Replay waits for a bar with no dialog in between, and Escape cancels.
        app.toggle_replay();
        assert!(app.replay_selecting && !app.replay_start.is_open);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!app.replay_selecting && !app.state.replay_active);
        assert!(!app.replay_control.is_visible());
        app.toggle_replay();
        let index = index - 3;
        let time = app.chart.state().shown()[index].time;
        let pos = egui::pos2(map.idx_to_x(index), map.rect.center().y);
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(!app.replay_selecting);
        let replay = app.replay.as_ref().unwrap();
        assert_eq!(replay.state.curr_bar, index);
        assert!(!replay.state.is_playing());
        assert_eq!(app.last_chart_click.as_ref().unwrap().time, Some(time));
        app.handle_top_toolbar(&ctx, TopToolbarAction::Host("research".into()));
        let message = &app.toasts.toasts().last().unwrap().message;
        assert!(message.contains(&time.to_rfc3339()), "{message}");
        assert!(
            message.contains(&format!("price: {:.4}", map.y_to_price(pos.y))),
            "{message}"
        );
        app.switch_symbol("STOCKA");
        frame(&mut app, vec![]);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Host("research".into()));
        assert_eq!(
            app.toasts.toasts().last().unwrap().message,
            "Click the chart to choose a research time and price"
        );
    }

    /// Wherever bar selection draws its cut line, a click starts replay at the bar it marks: in
    /// the volume band at the bottom of the price pane and after the last bar.
    #[test]
    fn bar_selection_click_starts_wherever_the_cut_line_is_drawn() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, "2026-11-03T02:00:00Z".parse().unwrap(), true);
        finish_feed(&app.live);
        app.apply_session();
        app.chart.set_signal_source(None);
        let frame = |app: &mut DemoApp, events| {
            finish_feed(&app.live);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(ctx.input(|input| input.time) + 0.2),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_top_toolbar(ui, app.theme.ui.panel_bg);
                    app.show_chart(ui, app.theme.ui.panel_bg);
                    app.show_replay_start(ui.ctx());
                },
            );
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            shapes
        };
        for volume_band in [true, false] {
            app.stop_replay();
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            let map = app.chart.get_chart_mapping();
            let last = app.chart.state().shown().len() - 1;
            let (marked, pos) = if volume_band {
                let y = app.chart.get_rendered_volume_rect().center().y;
                (last - 8, egui::pos2(map.idx_to_x(last - 8), y))
            } else {
                (
                    last,
                    egui::pos2(map.idx_to_x(last) + 20.0, map.rect.center().y),
                )
            };
            app.toggle_replay();
            assert!(app.replay_selecting);
            let shapes = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            let x = map.idx_to_x(marked);
            assert!(shapes.iter().any(|shape| matches!(
                &shape.shape,
                egui::Shape::LineSegment { points, .. }
                    if (points[0].x - x).abs() < 0.5
                        && (points[1].x - x).abs() < 0.5
                        && (points[1].y - points[0].y).abs() > map.rect.height() * 0.9
            )));
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            assert_eq!(
                app.replay.as_ref().map(|replay| replay.state.curr_bar),
                Some(marked),
                "volume band: {volume_band}"
            );
        }
    }

    #[test]
    fn application_storage_restores_drawings_after_restart() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new(&cc);
        app.live.set_paused(true);
        let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
        drawing.completed = true;
        drawing.chart_points = [-0.5, app.chart.bars(SeriesId::MAIN).len() as f32 + 1.5]
            .map(|index| ChartPoint::new(index, 100.0))
            .to_vec();
        let expected = drawing.chart_points.clone();
        let original_bars = app.chart.bars(SeriesId::MAIN).to_vec();
        app.chart.drawings_mut().drawings.push(drawing);
        let mut props = DrawingProps::from_drawing(&app.chart.drawings().drawings[0], &app.chart);
        props.line_width = 4.0;
        props.save_as_default = true;
        props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
        // The chart writes its edited drawings to the demo's storage as it shows them.
        let frame = |chart: &mut Chart| {
            ctx.run_ui(Default::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        frame(&mut app.chart);
        let mut storage = AppStorage::default();
        eframe::App::save(&mut *app, &mut storage);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new(&cc);
        frame(&mut restored.chart);
        assert_eq!(restored.chart.drawings().drawings.len(), 1);
        restored.chart.set_data(SeriesId::MAIN, original_bars);
        assert_eq!(restored.chart.drawings().drawings[0].chart_points, expected);
        let defaults = &restored.chart.drawings().tool_defaults[&DrawingToolType::TrendLine];
        assert_eq!(defaults.settings.stroke_width, 4.0);
        assert!(defaults.settings.chart_points.is_empty());
        restored.switch_symbol("STOCKA");
        assert_eq!(
            restored.chart.drawings().tool_defaults[&DrawingToolType::TrendLine]
                .settings
                .stroke_width,
            4.0
        );
    }

    /// The drawing bar's picker places a colour flag, which restarts with an emoji saved before the full set.
    #[test]
    fn emoji_picker_places_colour_emoji_that_survive_a_restart() {
        use egui::accesskit::Role;
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let at = "2026-11-10T19:29:50Z".parse().unwrap();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, at, true);
        app.live.set_paused(true);
        settle(&mut app, &ctx);
        let frame = |app: &mut DemoApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            )
        };
        let press = |app: &mut DemoApp, pos| {
            frame(app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
            for pressed in [true, false] {
                let button = egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(app, vec![button]).drop_without_applying_deltas();
            }
        };
        // The control `role` labelled or, for a text field, hinted `label`.
        let click = |app: &mut DemoApp, role, label: &str| {
            let output = frame(app, vec![]);
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == role
                        && (node.label() == Some(label) || node.placeholder() == Some(label))
                })
                .and_then(|(_, node)| node.bounds())
                .unwrap_or_else(|| panic!("missing control: {label}"));
            output.drop_without_applying_deltas();
            let center = egui::pos2(
                (bounds.x0 + bounds.x1) as f32 * 0.5,
                (bounds.y0 + bounds.y1) as f32 * 0.5,
            );
            press(app, center);
        };
        let emoji = |app: &DemoApp| -> Vec<Option<String>> {
            app.chart
                .drawings()
                .drawings
                .iter()
                .filter(|drawing| drawing.tool_type == DrawingToolType::FontIcon)
                .map(|drawing| drawing.text.clone())
                .collect()
        };
        let flag = "\u{1f1eb}\u{1f1f7}";
        let mut legacy = Drawing::new(900, DrawingToolType::FontIcon);
        legacy.text = Some("✈".into());
        legacy.completed = true;
        let bars = app.chart.bars(SeriesId::MAIN);
        let price = bars[bars.len() - 10].close;
        legacy.chart_points = vec![ChartPoint::new(bars.len() as f32 - 10.0, price)];
        app.chart.drawings_mut().drawings.push(legacy);

        click(&mut app, Role::Button, "Icons/Emojis");
        click(
            &mut app,
            Role::TextInput,
            "Search icons, emoji and stickers",
        );
        frame(&mut app, vec![egui::Event::Text("flag: france".into())])
            .drop_without_applying_deltas();
        click(&mut app, Role::Button, "Flag: France");
        // Past the double click interval.
        for _ in 0..30 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        let rect = app.chart.get_rendered_price_rect();
        press(
            &mut app,
            rect.left_center() + egui::vec2(rect.width() / 4.0, 0.0),
        );
        assert_eq!(emoji(&app), [Some("✈".into()), Some(flag.into())]);

        assert!(app.chart.flush_stored_drawings());
        let mut storage = AppStorage::default();
        eframe::App::save(&mut *app, &mut storage);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new_at(&cc, at, true);
        app.live.set_paused(true);
        app.poll_storage(&ctx);
        settle(&mut app, &ctx);
        assert_eq!(emoji(&app), [Some("✈".into()), Some(flag.into())]);
        frame(&mut app, vec![]).drop_without_applying_deltas();
        let output = frame(&mut app, vec![]);
        for text in ["✈", flag] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| matches!(&shape.shape, egui::Shape::Text(t)
                    if t.galley.job.text == text
                        && t.galley.rows.iter().any(|r| r.glyphs.iter().any(|g| g.is_color)))),
                "{text:?} is not drawn in colour after the restart"
            );
        }
        output.drop_without_applying_deltas();
    }

    #[test]
    fn replay_bar_sits_below_the_time_axis() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.start_replay();
        for pass in 0..3 {
            let mut chart = egui::Rect::NOTHING;
            let mut bottom_bar = 0.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.show_bottom_toolbar(ui, app.theme.ui.panel_bg);
                    bottom_bar = ui.available_rect_before_wrap().bottom();
                    app.reserve_replay_strip(ui, app.theme.ui.panel_bg);
                    chart = ui.available_rect_before_wrap();
                    app.show_chart(ui, app.theme.ui.panel_bg);
                    app.show_replay_control(ui.ctx());
                },
            );
            if pass == 2 {
                let exit = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Exit replay" => {
                            Some(text.pos.y)
                        }
                        _ => None,
                    })
                    .unwrap();
                // The chart's time axis ends at its bottom, above the strip and the bottom bar.
                let strip = chart.bottom()..bottom_bar;
                assert!(strip.contains(&exit), "{exit} outside {strip:?}");
            }
            output.textures_delta.clear();
            output.drop_without_applying_deltas();
        }
    }

    /// A choice of the bar's start control restarts a running replay over the whole history.
    #[test]
    fn replay_bar_starts_restart_from_the_whole_history() {
        use egui_charts::ui::replay::ReplayStartAction as Start;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.switch_timeframe(Timeframe::Hour1);
        let count = app.chart.bars(SeriesId::MAIN).len();
        let date = app.chart.bars(SeriesId::MAIN)[count / 2].time;
        for (start, expected) in [
            (Start::First, Some(0)),
            (Start::Date(date), Some(count / 2)),
            (Start::Random, None),
        ] {
            // Near the first bar, so most of the history is hidden from the chart.
            app.start_replay_at(1);
            app.apply_replay_start(start);
            let replay = app.replay.as_ref().unwrap();
            assert_eq!(replay.state.total_bars, count, "{start:?}");
            assert!(replay.state.curr_bar < count);
            if let Some(expected) = expected {
                assert_eq!(replay.state.curr_bar, expected, "{start:?}");
            }
            assert!(app.state.replay_active && app.replay_control.is_visible());
        }
        // A date past the history leaves the running replay alone.
        let late = app.bars.last().unwrap().time + chrono::TimeDelta::days(1);
        let curr = app.replay.as_ref().unwrap().state.curr_bar;
        app.apply_replay_start(Start::Date(late));
        assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, curr);
        // A restart keeps the update interval.
        app.set_replay_interval(Timeframe::Min1);
        app.apply_replay_start(Start::First);
        assert_eq!(app.replay_interval, Timeframe::Min1);
        app.apply_replay_start(Start::SelectDate);
        assert!(app.replay_start.is_open);
    }

    /// Select date reads the typed day on the chart's calendar: replay starts at the first bar
    /// of that day in the chart's zone. New York is on EST from 1 November 2026, Tokyo is UTC+9.
    #[test]
    fn select_date_starts_at_the_first_bar_of_the_local_day() {
        use egui::accesskit::{Action, ActionData, ActionRequest, Role, TreeId};
        use egui_charts::config::TimezoneMode;
        use egui_charts::ui::replay::ReplayStartAction;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-03T02:00:00Z".parse().unwrap(),
            true,
        );
        app.switch_symbol("TOKENA");
        app.switch_timeframe(Timeframe::Hour1);
        settle(&mut app, &ctx);
        let frame = |app: &mut DemoApp, events: Vec<egui::Event>| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.show_replay_start(ui.ctx()),
            )
        };
        for (zone, expected) in [
            (
                TimezoneMode::Timezone("America/New_York".parse().unwrap()),
                "2026-11-02T05:00:00Z",
            ),
            (
                TimezoneMode::Timezone("Asia/Tokyo".parse().unwrap()),
                "2026-11-01T15:00:00Z",
            ),
            (TimezoneMode::Utc, "2026-11-02T00:00:00Z"),
        ] {
            app.stop_replay();
            apply_timezone_mode(&mut app.chart, &mut app.timeframe_toolbar, zone.clone());
            let bars = app.chart.bars(SeriesId::MAIN).to_vec();
            app.apply_replay_start(ReplayStartAction::SelectDate);
            assert!(app.replay_start.is_open);
            // A new window lays itself out on its first frame and shows on the next.
            frame(&mut app, vec![]).drop_without_applying_deltas();
            let output = frame(&mut app, vec![]);
            let update = output.platform_output.accesskit_update.as_ref().unwrap();
            let target_node = (update.nodes.iter())
                .find(|(_, node)| node.role() == Role::TextInput)
                .map(|(id, _)| *id);
            output.drop_without_applying_deltas();
            let target_node = target_node.unwrap();
            frame(
                &mut app,
                vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::SetValue,
                    target_tree: TreeId::ROOT,
                    target_node,
                    data: Some(ActionData::Value("2026-11-02".into())),
                })],
            )
            .drop_without_applying_deltas();
            let output = frame(&mut app, vec![]);
            let point = (output.shapes.iter()).find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Start at date" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            let point = point.unwrap();
            let press = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                &mut app,
                vec![egui::Event::PointerMoved(point), press(true), press(false)],
            )
            .drop_without_applying_deltas();
            let expected: chrono::DateTime<chrono::Utc> = expected.parse().unwrap();
            let start = app.replay.as_ref().unwrap().state.curr_bar;
            assert_eq!(bars[start].time, expected, "{zone:?}");
            assert!(start > 0 && bars[start - 1].time < expected, "{zone:?}");
        }
    }

    #[test]
    fn replay_finer_source_and_saved_position_keep_the_shared_cutoff() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.switch_timeframe(Timeframe::Hour1);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.start_replay();
        assert_eq!(
            app.chart.bars(SeriesId::MAIN).len(),
            1,
            "first source bar only"
        );
        app.replay
            .as_mut()
            .unwrap()
            .handle_action(ReplayAction::JumpToBar(10));
        app.push_replay_window_to_chart();
        let cutoff = app.chart.replay_time().unwrap();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_replay_control(ui.ctx())
        })
        .drop_without_applying_deltas();
        let state = &app.replay.as_ref().unwrap().state;
        assert_eq!(
            app.replay_control.state.progress_pct,
            state.curr_bar as f32 / (state.total_bars - 1) as f32 * 100.0,
            "transport must display the cursor chosen after chart rendering"
        );
        assert_eq!(app.extra_charts[0].chart.replay_time(), Some(cutoff));
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::Three);
        app.advance_replay();
        assert_eq!(
            app.extra_charts[1].chart.replay_time(),
            Some(cutoff),
            "new slots join paused replay"
        );
        app.set_replay_interval(Timeframe::Min1);
        assert_eq!(
            app.chart.replay_time(),
            Some(cutoff),
            "finer source must retain the cutoff"
        );
        app.handle_replay_control(ReplayControlAction::NextBar);
        app.set_replay_interval(Timeframe::Hour1);
        let saved = app.replay.as_ref().unwrap().session().unwrap();
        let cutoff = app.chart.replay_time().unwrap();
        assert_eq!(app.extra_charts[0].chart.replay_time(), Some(cutoff));
        assert_eq!(app.state.timeframe, Timeframe::Hour1);
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        assert!(
            app.extra_charts[1].chart.bars(SeriesId::MAIN).len() > 11,
            "new slots retain history beyond their copied prefix"
        );
        assert!(app.replay.as_mut().unwrap().restore_session(&saved));
        app.push_replay_window_to_chart();
        app.stop_replay();
        assert!(app.extra_charts[0].chart.replay_time().is_none());
        settle(&mut app, &ctx);
        let mut storage = AppStorage::default();
        eframe::App::save(&mut *app, &mut storage);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new(&cc);
        // The session records the three charts, every one a copy of the source at one hour.
        let mut expected = saved.clone();
        expected.charts = (0..3)
            .map(|slot| {
                egui_charts::ui::replay::ReplayChart::new(
                    slot,
                    app.state.symbol.clone(),
                    Timeframe::Hour1,
                )
            })
            .collect();
        assert_eq!(restored.replay_session, Some((expected, Timeframe::Hour1)));
        restored.apply_replay_start(egui_charts::ui::replay::ReplayStartAction::Resume);
        assert_eq!(restored.chart.replay_time(), Some(cutoff));
        assert!(!restored.replay.as_ref().unwrap().state.is_playing());

        // Before scopes and charts were saved, the demo replayed every chart in the default layout.
        let session = storage.0.get_mut("replay_session").unwrap();
        let (from, to) = (
            session.find(",scope:AllCharts").unwrap(),
            session.rfind("),").unwrap(),
        );
        session.replace_range(from..to, "");
        assert!(!session.contains("charts"));
        let mut cc = eframe::CreationContext::_new_kittest(ctx);
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new(&cc);
        assert_eq!(restored.extra_charts.len(), 2);
        restored.apply_replay_start(egui_charts::ui::replay::ReplayStartAction::Resume);
        assert_eq!(restored.chart.replay_time(), Some(cutoff));
        assert!(
            restored
                .extra_charts
                .iter()
                .all(|extra| extra.chart.replay_time() == Some(cutoff))
        );
        assert_eq!(restored.replay_slots, [0, 1, 2]);
    }

    #[derive(Default)]
    pub(super) struct AppStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for AppStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    /// The selection toolbar opens the same drawing alert editor as the drawing menu.
    #[test]
    fn selection_toolbar_add_alert_opens_the_dialog_for_the_drawing() {
        use eframe::App as _;
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-04T17:30:00Z".parse().unwrap(),
            false,
        );
        app.live.set_paused(true);
        settle(&mut app, &ctx);
        ctx.enable_accesskit();
        let id = app
            .chart
            .drawings()
            .drawings
            .iter()
            .map(|d| d.id)
            .max()
            .unwrap_or(0)
            + 1;
        let n = app.bars.len();
        let price = app.bars.last().unwrap().close;
        let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
        drawing.completed = true;
        drawing.chart_points = vec![
            ChartPoint::new((n - 20) as f32, price),
            ChartPoint::new((n - 5) as f32, price),
        ];
        let source = drawing.alert_source(0);
        app.chart.drawings_mut().drawings.push(drawing);
        app.chart.drawings_mut().select(id);
        let draw = |app: &mut DemoApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
        };
        draw(&mut app, vec![]).drop_without_applying_deltas();
        let output = draw(&mut app, vec![]);
        let pos = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                if node.label() != Some("Add alert") || node.role() == egui::accesskit::Role::Label
                {
                    return None;
                }
                let rect = node.bounds()?;
                Some(egui::pos2(
                    (rect.x0 + rect.x1) as f32 / 2.0,
                    (rect.y0 + rect.y1) as f32 / 2.0,
                ))
            })
            .expect("missing control: Add alert");
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
            )
            .drop_without_applying_deltas();
        }
        assert!(app.alert_dialog.is_open);
        assert!(
            matches!(&app.alert_dialog.alert.condition, AlertCondition::Compare { other, .. } if *other == source)
        );
    }

    #[test]
    fn depth_menu_requires_a_declaration_and_restores_its_view() {
        use eframe::App;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let now = "2026-10-27T14:00:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        settle(&mut app, &ctx);
        let draw = |app: &mut DemoApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
        };
        let click = |app: &mut DemoApp, label: &str| {
            let output = draw(app, vec![]);
            let pos = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    if node.label() != Some(label) || node.role() == egui::accesskit::Role::Label {
                        return None;
                    }
                    let rect = node.bounds()?;
                    Some(egui::pos2(
                        (rect.x0 + rect.x1) as f32 / 2.0,
                        (rect.y0 + rect.y1) as f32 / 2.0,
                    ))
                })
                .unwrap_or_else(|| panic!("missing control: {label}"));
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                draw(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: Default::default(),
                        },
                    ],
                )
                .drop_without_applying_deltas();
            }
        };
        let has = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)
        })
        };
        click(&mut app, "Menu");
        let output = draw(&mut app, vec![]);
        assert!(has(&output, "Depth"));
        output.drop_without_applying_deltas();
        click(&mut app, "Depth");
        let output = draw(&mut app, vec![]);
        assert!(has(&output, "Bid size"));
        assert!(has(&output, "Center"));
        output.drop_without_applying_deltas();
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        let saved = eframe::Storage::get_string(&storage, "depth_panel").unwrap();
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new_at(&cc, now, true);
        settle(&mut restored, &ctx);
        let output = draw(&mut restored, vec![]);
        assert!(has(&output, "Bid size"));
        output.drop_without_applying_deltas();
        let mut resaved = AppStorage::default();
        restored.save(&mut resaved);
        assert_eq!(
            eframe::Storage::get_string(&resaved, "depth_panel"),
            Some(saved)
        );
        let mut instrument = app.chart.instrument().unwrap().clone();
        instrument.services = Some(vec![]);
        *app.chart = egui_charts::ChartBuilder::new()
            .with_symbol(&instrument.symbol)
            .build();
        app.chart.set_instrument(Some(instrument));
        let output = draw(&mut app, vec![]);
        assert!(!has(&output, "Bid size"));
        output.drop_without_applying_deltas();
        click(&mut app, "Menu");
        let output = draw(&mut app, vec![]);
        assert!(!has(&output, "Depth"));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn footprint_alerts_follow_the_active_chart() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let mut params = app.chart.chart_type_params().clone();
        params.footprint_imbalance_ratio = 4.0;
        params.footprint_diagonal = false;
        params.footprint_imbalance_min_volume = 5.0;
        params.footprint_stack = 2;
        params.footprint_row_ticks = 2;
        app.chart.set_chart_type(ChartType::VolumeFootprint);
        app.chart.set_chart_type_params(params);
        settle(&mut app, &ctx);
        assert_eq!(app.chart.footprint_row_ticks(), 2);
        let rule = app.live.detectors().settings().footprint.rule;
        assert_eq!(
            (
                rule.imbalance_ratio,
                rule.diagonal,
                rule.imbalance_min_volume,
                rule.stack,
                rule.row_ticks
            ),
            (4.0, false, 5.0, 2, 2)
        );
    }

    #[test]
    fn opening_state_is_quiet_and_saved_features_restore() {
        use eframe::App;
        use egui_charts::services::{DetectionCounts, DetectionKind};

        let now = "2026-10-27T14:00:00Z".parse().unwrap();
        for width in [393.0, 1440.0] {
            for theme in [egui::Theme::Light, egui::Theme::Dark] {
                let ctx = egui::Context::default();
                ctx.run_ui(
                    egui::RawInput {
                        system_theme: Some(theme),
                        ..Default::default()
                    },
                    |_| {},
                )
                .drop_without_applying_deltas();
                let mut app = DemoApp::new_at(
                    &eframe::CreationContext::_new_kittest(ctx.clone()),
                    now,
                    true,
                );
                assert!(app.open_panel.is_none());
                assert_eq!(app.chart.chart_type(), ChartType::Candles);
                assert!(app.chart.config().show_volume);
                assert!(!app.api_samples);
                let events = app.chart.event_visibility();
                assert!(
                    !events.show_dividends
                        && !events.show_splits
                        && !events.show_news
                        && !events.show_ideas
                );
                assert!(app.live_strategy.is_none());
                for second in 0..=60 {
                    app.advance_sample_to(now + chrono::TimeDelta::seconds(second));
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 900.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                    )
                    .drop_without_applying_deltas();
                    assert!(app.toasts.is_empty(), "{theme:?}, {width}, second {second}");
                    assert!(app.live.detectors().take_alerts().is_empty());
                }
                for kind in DetectionKind::ALL {
                    assert_eq!(
                        app.live.detectors().counts("SAMPLE", kind, app.live.now()),
                        DetectionCounts::default()
                    );
                }
                assert_eq!(app.notifications.unread(), 0);
                let mut settings = app.live.detectors().settings();
                settings.big_trades.enabled = true;
                settings.big_trades.alert = true;
                settings.big_trades.sound = true;
                settings.big_trades.rule.threshold = egui_charts::services::Threshold::Fixed(10.0);
                settings.stop_runs.enabled = true;
                settings.stop_run_count_alert = Some(egui_charts::services::CountAlert::default());
                app.live.detectors().set_settings(settings.clone());
                let mut events = app.chart.event_visibility().clone();
                events.show_news = true;
                app.chart.set_event_visibility(events.clone());
                app.execute_search_command(&ctx, "samples");
                // Matching labels alone must not discard annotations the visitor added.
                let mut markers = app.chart.markers(SeriesId::MAIN).to_vec();
                markers.push(egui_charts::model::Marker::new(now).with_text("Fill"));
                app.chart.set_markers(SeriesId::MAIN, markers);
                app.chart.add_price_line(
                    SeriesId::MAIN,
                    egui_charts::model::PriceLine::new(320.0, egui::Color32::BLUE)
                        .with_title("Demo level"),
                );
                for named in [false, true] {
                    app.open_panel = Some(if named {
                        SidePanel::TimeAndSales
                    } else {
                        SidePanel::OrderFlow
                    });
                    if named {
                        app.handle_layout_action(
                            egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction::Save,
                        );
                        app.poll_storage(&ctx);
                    }
                    for legacy in [false, true] {
                        let mut storage = AppStorage::default();
                        // Saving also removes the old key from existing storage.
                        eframe::set_value(&mut storage, "api_samples", &true);
                        app.save(&mut storage);
                        assert!(eframe::Storage::get_string(&storage, "api_samples").is_none());
                        if legacy {
                            eframe::set_value(&mut storage, "api_samples", &true);
                        }
                        let mut cc =
                            eframe::CreationContext::_new_kittest(egui::Context::default());
                        cc.storage = Some(&storage);
                        let mut restored = DemoApp::new_at(&cc, now, true);
                        restored.poll_storage(&cc.egui_ctx);
                        assert_eq!(restored.live.detectors().settings(), settings);
                        assert!(restored.open_panel == (!named).then_some(SidePanel::OrderFlow));
                        assert!(!restored.api_samples);
                        assert!(restored.widget_bar.widgets.is_empty());
                        let markers = restored.chart.markers(SeriesId::MAIN);
                        assert_eq!(markers.len(), 1, "named={named}, legacy={legacy}");
                        assert_eq!(markers[0].text.as_deref(), Some("Fill"));
                        assert_eq!(markers[0].color, egui::Color32::PLACEHOLDER);
                        let lines: Vec<_> = restored.chart.price_lines(SeriesId::MAIN).collect();
                        assert_eq!(lines.len(), 1, "named={named}, legacy={legacy}");
                        assert_eq!(lines[0].1.price, 320.0);
                        assert_eq!(lines[0].1.color, egui::Color32::BLUE);
                        assert_eq!(restored.chart.event_visibility(), &events);
                        restored.execute_search_command(&cc.egui_ctx, "signals");
                        assert!(restored.api_samples);
                        assert_eq!(restored.widget_bar.widgets.len(), 2);
                        assert_eq!(restored.chart.markers(SeriesId::MAIN).len(), 2);
                    }
                }
            }
        }
    }

    #[test]
    fn cursor_favourites_move_from_the_legacy_key_to_settings() {
        use eframe::App;
        use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
        let legacy = [CursorType::Dot, CursorType::Magic];
        for (document, expected, readable) in [
            (None, legacy.as_slice(), true),
            (Some(r#"{"version":1}"#), legacy.as_slice(), true),
            (
                Some(r#"{"version":1,"cursor_favorites":[]}"#),
                legacy.as_slice(),
                true,
            ),
            (
                Some(r#"{"version":1,"cursor_favorites":["Magic"]}"#),
                &legacy[1..],
                true,
            ),
            (Some(r#"{"version":2}"#), legacy.as_slice(), false),
        ] {
            let mut storage = AppStorage::default();
            eframe::set_value(&mut storage, "cursor_favorites", &legacy.to_vec());
            if let Some(document) = document {
                eframe::set_value(
                    &mut storage,
                    "templates",
                    &std::collections::HashMap::from([("Settings/settings", document)]),
                );
            }
            let ctx = egui::Context::default();
            let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
            cc.storage = Some(&storage);
            let mut app = DemoApp::new(&cc);
            assert_eq!(
                app.drawing_toolbar.state.cursor_favorites, expected,
                "{document:?}"
            );
            app.save(&mut storage);
            assert_eq!(
                eframe::Storage::get_string(&storage, "cursor_favorites").is_none(),
                readable
            );
            let documents: std::collections::HashMap<String, String> =
                eframe::get_value(&storage, "templates").unwrap();
            if !readable {
                assert_eq!(Some(documents["Settings/settings"].as_str()), document);
                continue;
            }
            let settings: serde_json::Value =
                serde_json::from_str(&documents["Settings/settings"]).unwrap();
            assert_eq!(
                settings["cursor_favorites"],
                serde_json::to_value(expected).unwrap()
            );
            let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
            cc.storage = Some(&storage);
            let mut restored = DemoApp::new(&cc);
            assert_eq!(restored.drawing_toolbar.state.cursor_favorites, expected);
            restored.drawing_toolbar.state.cursor_favorites.clear();
            restored.save(&mut storage);
            let mut cc = eframe::CreationContext::_new_kittest(ctx);
            cc.storage = Some(&storage);
            assert!(
                DemoApp::new(&cc)
                    .drawing_toolbar
                    .state
                    .cursor_favorites
                    .is_empty()
            );
        }
    }

    /// Switching the theme, from the toolbar or by a search command, is no chart's undo step:
    /// the Undo after it brings back the grid hidden and the axis text recoloured before it and
    /// leaves both charts on the new theme's colours.
    #[test]
    fn switching_the_theme_is_no_undo_step_of_a_chart() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        settle(&mut app, &ctx);
        for by_command in [false, true] {
            // The theme replaces an edited colour, which must not make the switch a step either.
            app.chart.user_action(|chart| {
                chart.config_mut().show_grid = false;
                chart.config_mut().text_color = egui::Color32::BLUE;
            });
            let dark = app.theme.dark;
            if by_command {
                let command = if dark {
                    "appearance:light"
                } else {
                    "appearance:dark"
                };
                app.execute_search_command(&ctx, command);
            } else {
                app.handle_top_toolbar(&ctx, TopToolbarAction::ThemeToggle);
            }
            assert_ne!(app.theme.dark, dark, "by command {by_command}");
            app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
            assert!(app.chart.config().show_grid, "by command {by_command}");
            for chart in [&app.chart, &app.extra_charts[0].chart] {
                assert!(!chart.can_undo(), "by command {by_command}");
                assert_eq!(chart.config().background_color, app.theme.chart.bg);
                assert_eq!(chart.config().text_color, app.theme.chart.axis_text);
            }
        }
    }

    /// The template menu offers the library's built-in sets apart from the saved ones, and
    /// choosing one replaces the chart's studies in a single undo step.
    #[test]
    fn built_in_study_sets_apply_from_the_template_menu_in_one_undo_step() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let builtin = StudyTemplateSet::builtin();
        assert_eq!(
            app.top_toolbar.builtin_indicator_templates,
            builtin
                .iter()
                .map(|(name, _)| ((*name).to_owned(), (*name).to_owned()))
                .collect::<Vec<_>>()
        );
        app.chart
            .add_indicator(Box::new(egui_charts::studies::EMA::new(5)));
        let names = |app: &DemoApp| {
            app.chart
                .indicators()
                .map(|(_, study)| study.name().to_owned())
                .collect::<Vec<_>>()
        };
        let before = names(&app);
        let (name, set) = &builtin[3];
        app.handle_top_toolbar(
            &ctx,
            TopToolbarAction::ApplyBuiltinIndicatorTemplate((*name).to_owned()),
        );
        assert_eq!(
            names(&app),
            set.studies
                .iter()
                .map(|study| study.name.clone())
                .collect::<Vec<_>>()
        );
        assert!(app.chart.undo());
        assert_eq!(names(&app), before);
    }

    #[test]
    fn legacy_themes_follow_system_and_new_choices_survive_restart() {
        // Repeated construction used to overflow this stack before checking the saved theme.
        std::thread::Builder::new()
            .name("theme-restarts".into())
            .stack_size(512 * 1024)
            .spawn(check_theme_restarts)
            .unwrap()
            .join()
            .unwrap();
    }

    fn check_theme_restarts() {
        use eframe::App;
        use egui_charts::drawings::DrawingToolType;
        let cases = [
            (None, Some(true), None, 1),
            (Some(egui::Theme::Light), Some(true), Some("dark"), 3),
            (Some(egui::Theme::Dark), Some(false), Some("light"), 1),
        ];
        for (initial, legacy_key, legacy_document, restarts) in cases {
            let ctx = egui::Context::default();
            ctx.run_ui(
                egui::RawInput {
                    system_theme: initial,
                    ..Default::default()
                },
                |_| {},
            )
            .drop_without_applying_deltas();
            let mut storage = AppStorage::default();
            if let Some(dark) = legacy_key {
                eframe::set_value(&mut storage, "dark_theme", &dark);
            }
            if let Some(theme) = legacy_document {
                // Earlier releases saved the active theme even without a user choice.
                let settings = format!(
                    r#"{{"version":1,"theme":"{theme}","symbol":"AAPL","interval":"Min5","drawing_favorites":["TrendLine"],"layout":null,"layout_autosave":true}}"#
                );
                eframe::set_value(
                    &mut storage,
                    "templates",
                    &std::collections::HashMap::from([
                        ("Settings/settings", settings.as_str()),
                        ("Layouts/Kept", "saved layout"),
                    ]),
                );
            }
            for explicit in [false, true, true].into_iter().take(restarts) {
                let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
                cc.storage = Some(&storage);
                let mut app = DemoApp::new(&cc);
                if legacy_document.is_some() {
                    assert_eq!(app.state.symbol, "STOCKA");
                    assert_eq!(app.state.timeframe, Timeframe::Min5);
                    assert_eq!(
                        app.drawing_toolbar.state.favorites,
                        [DrawingToolType::TrendLine]
                    );
                    assert!(app.top_toolbar.layout_menu.autosave);
                }
                if !explicit {
                    assert_eq!(app.theme.dark, initial == Some(egui::Theme::Dark));
                }
                if explicit {
                    app.handle_top_toolbar(&ctx, TopToolbarAction::ThemeToggle);
                }
                let choice = explicit.then_some(app.theme.dark);
                app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
                for system in [egui::Theme::Dark, egui::Theme::Light, egui::Theme::Dark] {
                    ctx.run_ui(
                        egui::RawInput {
                            system_theme: Some(system),
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(393.0, 852.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                    )
                    .drop_without_applying_deltas();
                    let expected = if choice.unwrap_or(system == egui::Theme::Dark) {
                        Theme::dark()
                    } else {
                        Theme::light()
                    };
                    assert_eq!(app.theme, expected);
                    assert_eq!(app.chart.config().background_color, expected.chart.bg);
                    assert_eq!(
                        app.extra_charts[0].chart.config().background_color,
                        expected.chart.bg
                    );
                    assert_eq!(egui_charts::theme::ThemeExt::active_theme(&ctx), expected);
                }
                app.save(&mut storage);
                if legacy_document.is_some() {
                    let documents: std::collections::HashMap<String, String> =
                        eframe::get_value(&storage, "templates").unwrap();
                    assert_eq!(documents["Layouts/Kept"], "saved layout");
                }
                drop(app);
                ctx.run_ui(
                    egui::RawInput {
                        system_theme: Some(egui::Theme::Light),
                        ..Default::default()
                    },
                    |_| {},
                )
                .drop_without_applying_deltas();
                let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
                cc.storage = Some(&storage);
                let restored = DemoApp::new(&cc);
                assert_eq!(restored.theme.dark, choice.unwrap_or(false));
                assert_eq!(
                    restored.user_settings().theme,
                    choice.map(|dark| if dark { "dark" } else { "light" }.into())
                );
                assert_eq!(
                    restored.chart.config().background_color,
                    restored.theme.chart.bg
                );
                assert_eq!(
                    restored.chart.config().bearish_color,
                    restored.theme.chart.candle_down
                );
            }
        }

        let ctx = egui::Context::default();
        let mut storage = AppStorage::default();
        for old in [false, true] {
            eframe::set_value(&mut storage, "theme_choice", &old);
            let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
            cc.storage = Some(&storage);
            let mut app = DemoApp::new(&cc);
            assert_eq!(
                app.theme.dark, old,
                "explicit boolean preferences still load"
            );
            app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
            for (command, expected) in [
                ("appearance:high-contrast", Theme::high_contrast()),
                ("appearance:light", Theme::light()),
                ("appearance:dark", Theme::dark()),
            ] {
                app.execute_search_command(&ctx, command);
                assert_eq!(app.theme, expected);
                assert_eq!(app.chart.config().background_color, expected.chart.bg);
                assert_eq!(
                    app.extra_charts[0].chart.config().background_color,
                    expected.chart.bg
                );
                app.save(&mut storage);
                let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
                cc.storage = Some(&storage);
                assert_eq!(DemoApp::new(&cc).theme, expected);
            }
            app.execute_search_command(&ctx, "appearance:system");
            app.save(&mut storage);
            for system in [egui::Theme::Dark, egui::Theme::Light] {
                ctx.run_ui(
                    egui::RawInput {
                        system_theme: Some(system),
                        ..Default::default()
                    },
                    |_| {},
                )
                .drop_without_applying_deltas();
                let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
                cc.storage = Some(&storage);
                let restored = DemoApp::new(&cc);
                assert_eq!(restored.theme.dark, system == egui::Theme::Dark);
                assert_eq!(restored.user_settings().theme, None);
            }
        }
    }

    #[test]
    fn named_templates_survive_demo_restart_with_inputs_and_drawing_identity_intact() {
        use eframe::App;
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.handle_drawing_toolbar(DrawingToolbarAction::SaveTemplate {
            name: "Missing".into(),
        });
        assert_eq!(
            app.toasts.toasts().last().unwrap().message,
            "Select a drawing first"
        );
        assert!(app.drawing_props.templates.entries().is_empty());
        let mean = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
        let rsi = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::RSI::new(2)));
        app.chart
            .set_indicator_source(
                mean,
                egui_charts::studies::StudySource::Study { id: rsi.0, plot: 0 },
            )
            .unwrap();
        let mut drawing = Drawing::new(17, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(3.0, 100.0), ChartPoint::new(5.0, 200.0)];
        drawing.completed = true;
        drawing.stroke_width = 4.0;
        app.chart.drawings_mut().drawings.push(drawing);
        app.chart.drawings_mut().select(17);
        app.settings_dialog.templates.poll(&ctx);
        app.drawing_props.templates.poll(&ctx);
        app.show_templates(&ctx);
        app.settings_dialog
            .templates
            .save("Canvas", ChartSettingsState::from_chart(&app.chart))
            .unwrap();
        app.study_templates
            .templates
            .save("Average", app.chart.study_template_set())
            .unwrap();
        app.handle_drawing_toolbar(DrawingToolbarAction::SaveTemplate {
            name: "Trend".into(),
        });
        app.settings_dialog.templates.poll(&ctx);
        app.drawing_props.templates.poll(&ctx);
        app.show_templates(&ctx);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        for _ in 0..2 {
            app.settings_dialog.templates.poll(&ctx);
            app.drawing_props.templates.poll(&ctx);
            app.show_templates(&ctx);
        }
        assert!(
            app.settings_dialog
                .templates
                .entries()
                .contains_key("Canvas")
        );
        let ema = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::EMA::new(5)));
        app.chart.set_indicator_user_edit_enabled(ema, false);
        let ids = |app: &DemoApp| app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>();
        let before = ids(&app);
        app.handle_top_toolbar(
            &ctx,
            TopToolbarAction::ApplyIndicatorTemplate("Average".into()),
        );
        // The study the user may not edit stays beside the template's.
        assert!(ids(&app).contains(&ema));
        let studies: Vec<_> = app
            .chart
            .indicators()
            .filter(|(id, _)| *id != ema)
            .collect();
        assert_eq!(
            studies[0].1.inputs()[0].value,
            egui_charts::studies::InputValue::Length(7)
        );
        assert_eq!(
            studies[0].1.source(),
            Some(egui_charts::studies::StudySource::Study {
                id: studies[1].0.0,
                plot: 0
            })
        );
        assert!(app.chart.undo());
        assert_eq!(ids(&app), before);
        assert!(app.chart.redo());
        assert!(ids(&app).contains(&ema));
        for id in [23, 24] {
            let mut target = Drawing::new(id, DrawingToolType::TrendLine);
            target.chart_points = vec![ChartPoint::new(8.0, 300.0), ChartPoint::new(10.0, 400.0)];
            target.completed = true;
            target.stroke_width = 1.0;
            app.chart.drawings_mut().drawings.push(target);
        }
        app.chart.drawings_mut().select_drawings([23, 24]);
        // The row the toolbar's template menu shows for the template.
        app.show_templates(&ctx);
        let template_id = app
            .drawing_toolbar
            .state
            .templates
            .iter()
            .find(|row| row.name == "Trend")
            .unwrap()
            .id
            .clone();
        app.handle_drawing_toolbar(DrawingToolbarAction::LoadTemplate { template_id });
        let widths = |app: &DemoApp| {
            [23, 24].map(|id| {
                let drawings = &app.chart.drawings().drawings;
                drawings.iter().find(|d| d.id == id).unwrap().stroke_width
            })
        };
        let target = app
            .chart
            .drawings()
            .drawings
            .iter()
            .find(|drawing| drawing.id == 23)
            .unwrap();
        assert_eq!(target.chart_points[0].bar_idx, 8.0);
        assert_eq!(widths(&app), [4.0, 4.0]);
        assert!(app.chart.undo());
        assert_eq!(widths(&app), [1.0, 1.0]);
    }

    /// A picture pasted over the price pane becomes one selected image drawing whose pixels
    /// load, and a picture wider than the texture limit is refused with its reason.
    #[test]
    fn pasted_picture_becomes_a_selected_image_drawing() {
        use eframe::App;
        use egui::load::ImagePoll;
        use egui_charts::drawings::DrawingToolType;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        settle(&mut app, &ctx);
        let pos = app.chart.get_rendered_price_rect().center();
        let limit = ctx.input(|i| i.max_texture_side);
        let color = egui::Color32::from_rgba_unmultiplied(255, 0, 0, 128);
        for (size, refusal) in [
            ([64, 32], None),
            (
                [limit + 1, 1],
                Some("Picture dimensions exceed the texture limits"),
            ),
            ([0, 1], Some("Picture dimensions exceed the texture limits")),
            ([64, 32], Some("Could not encode picture")),
        ] {
            let mut picture = egui::ColorImage::filled(size, color);
            if refusal == Some("Could not encode picture") {
                picture.pixels.clear();
            }
            let picture = std::sync::Arc::new(picture);
            app.toasts.clear();
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PasteImage(picture),
                    ],
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
            .drop_without_applying_deltas();
            // The refused picture leaves the placed one alone.
            let images: Vec<_> = (app.chart.drawings().drawings.iter())
                .filter(|drawing| drawing.tool_type == DrawingToolType::Image)
                .collect();
            assert_eq!(images.len(), 1, "{size:?}");
            let refusals: Vec<_> = (app.toasts.toasts().iter())
                .filter(|toast| toast.message.starts_with("Could not paste the picture"))
                .map(|toast| toast.message.as_str())
                .collect();
            if let Some(reason) = refusal {
                assert_eq!(refusals, [format!("Could not paste the picture: {reason}")]);
                continue;
            }
            assert!(refusals.is_empty(), "{refusals:?}");
            assert!(app.chart.drawings().is_selected(images[0].id));
            // 64 x 32 pixels fit the 240 x 180 limit unscaled.
            assert_eq!(images[0].image.and_then(|c| c.size), Some([64.0, 32.0]));
            let uri = images[0].text.clone().expect("picture address");
            // The loader may decode on another thread.
            let started = std::time::Instant::now();
            let loaded = loop {
                match ctx.try_load_image(&uri, egui::SizeHint::default()) {
                    Ok(ImagePoll::Pending { .. }) if started.elapsed().as_secs() < 10 => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    poll => break poll,
                }
            };
            let Ok(ImagePoll::Ready { image }) = loaded else {
                panic!("picture did not load: {:?}", loaded.err());
            };
            assert_eq!(image.size, [64, 32]);
            assert!(image.pixels.iter().all(|&p| p == color));
        }
    }

    /// Native clipboard events paste the selected drawings without a session cache,
    /// and the pasted group is one undoable addition.
    #[test]
    fn native_drawing_clipboard_pastes_once_and_undoes_the_copy() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 120.0)];
        drawing.completed = true;
        app.chart.drawings_mut().drawings.push(drawing.clone());
        drawing.id = 8;
        drawing
            .chart_points
            .iter_mut()
            .for_each(|p| p.price += 10.0);
        app.chart.drawings_mut().drawings.push(drawing);
        app.chart.drawings_mut().select_drawings([7, 8]);
        let output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ui| app.handle_keyboard(ui.ctx()),
        );
        let text = output.platform_output.commands.iter().find_map(|command| {
            if let egui::OutputCommand::CopyText(text) = command {
                Some(text.clone())
            } else {
                None
            }
        });
        output.drop_without_applying_deltas();
        let text = text.expect("copy must provide system clipboard text");
        // A native clipboard must remain usable after the session cache is gone.
        app.copied_drawing = None;
        for (event, count) in [
            (egui::Event::Paste("unrelated clipboard text".into()), 2),
            (egui::Event::Paste(text), 4),
            (
                egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                2,
            ),
        ] {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| app.handle_keyboard(ui.ctx()),
            )
            .drop_without_applying_deltas();
            assert_eq!(app.chart.drawings().drawings.len(), count);
        }
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[0],
            ChartPoint::new(10.0, 100.0)
        );
    }

    /// Lock and Hide from the drawing menu and toolbar set every selected drawing to the opposite
    /// of the menu drawing's state, mixed selections included, and one undo restores them all.
    #[test]
    fn drawing_menu_hides_and_locks_every_selected_drawing() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.drawings_mut().drawings.clear();
        // (id, locked, visible): 7 plain, 8 locked, 9 hidden.
        for (id, locked, visible) in [(7, false, true), (8, true, true), (9, false, false)] {
            let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
            let price = 100.0 + id as f64;
            drawing.chart_points = vec![ChartPoint::new(10.0, price), ChartPoint::new(20.0, price)];
            drawing.completed = true;
            drawing.locked = locked;
            drawing.visible = visible;
            app.chart.drawings_mut().drawings.push(drawing);
        }
        let state = |app: &DemoApp| {
            [7, 8, 9].map(|id| {
                let drawings = &app.chart.drawings().drawings;
                let drawing = drawings.iter().find(|d| d.id == id).unwrap();
                (drawing.locked, drawing.visible)
            })
        };
        let initial = [(false, true), (true, true), (false, false)];
        for (action, expected) in [
            (
                DrawingContextMenuAction::Lock,
                [(true, true), (true, true), (true, false)],
            ),
            (
                DrawingContextMenuAction::Hide,
                [(false, false), (true, false), (false, false)],
            ),
        ] {
            // The menu opens on 7, which a right-click within the selection makes primary.
            app.chart.drawings_mut().select_drawings([7, 8, 9]);
            assert_eq!(app.chart.drawings().sel_drawing(), Some(7));
            app.handle_drawing_context(&ctx, action.clone());
            assert_eq!(state(&app), expected, "{action:?}");
            assert!(app.chart.undo(), "{action:?}");
            assert_eq!(state(&app), initial, "{action:?}");
        }
    }

    #[test]
    fn study_menu_host_actions_keep_sources_and_clipboard_settings() {
        use egui::{Event, PointerButton, RawInput, Shape};
        use egui_charts::studies::{SMA, StudySource};
        for theme in [Theme::light(), Theme::dark()] {
            for width in [640.0, 1440.0] {
                let ctx = egui::Context::default();
                let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                app.live.set_paused(true);
                app.theme = theme.clone();
                egui_charts::theme::apply_to_egui(&ctx, &theme);
                let parent = app.chart.add_indicator(Box::new(SMA::new(7)));
                app.handle_study_context(
                    &ctx,
                    parent,
                    StudyContextMenuAction::AddToFavorites("SMA".into()),
                );
                assert!(app.indicator_dialog.favorites.contains("SMA"));
                app.handle_study_context(&ctx, parent, StudyContextMenuAction::ObjectTree);
                assert!(app.open_panel == Some(SidePanel::ObjectTree));
                assert!(
                    app.object_tree
                        .state()
                        .is_selected(indicator_tree_id(parent))
                );
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    app.handle_study_context(ui.ctx(), parent, StudyContextMenuAction::Copy);
                });
                let text = output
                    .platform_output
                    .commands
                    .iter()
                    .find_map(|command| match command {
                        egui::OutputCommand::CopyText(text) => Some(text.clone()),
                        _ => None,
                    })
                    .expect("Copy supplies clipboard text");
                output.drop_without_applying_deltas();
                let mut other = egui_charts::ChartBuilder::new().build();
                let pasted = other
                    .paste_indicator(&text, &egui_charts::studies::IndicatorFactory::new())
                    .unwrap();
                let pasted = other.indicators().find(|(id, _)| *id == pasted).unwrap().1;
                assert_eq!(
                    pasted
                        .inputs()
                        .iter()
                        .find(|input| input.id == "period")
                        .unwrap()
                        .value,
                    egui_charts::studies::InputValue::Length(7)
                );

                app.handle_study_context(&ctx, parent, StudyContextMenuAction::AddIndicator);
                let frame = |app: &mut DemoApp, events| {
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| app.show_dialogs(ui.ctx()),
                    )
                };
                let text_at = |output: &egui::FullOutput, label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Text(text) if text.galley.text() == label => {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing {label}"))
                };
                for _ in 0..2 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![Event::Text("Simple Moving".into())]);
                output.drop_without_applying_deltas();
                let output = frame(&mut app, vec![]);
                let row = text_at(&output, "Simple Moving Average");
                output.drop_without_applying_deltas();
                let click = |app: &mut DemoApp, pos| {
                    for pressed in [true, false] {
                        frame(
                            app,
                            vec![
                                Event::PointerMoved(pos),
                                Event::PointerButton {
                                    pos,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Default::default(),
                                },
                            ],
                        )
                        .drop_without_applying_deltas();
                    }
                };
                click(&mut app, row);
                let child = app
                    .chart
                    .indicators()
                    .find(|(id, study)| *id != parent && study.name() == "SMA")
                    .expect("the dialog adds the selected study")
                    .1;
                assert_eq!(
                    child.source(),
                    Some(StudySource::Study {
                        id: parent.0,
                        plot: 0
                    })
                );
            }
        }
    }

    /// Typed search owns subsequent editing keys, including in the opening frame.
    #[test]
    fn typed_symbol_search_keeps_queued_editing_keys() {
        use eframe::App;
        use egui_charts::drawings::{Drawing, DrawingToolType};
        for (typed, suffix, expected) in [
            ("SAMX", "PLE", "SAMPLE"),
            ("INDEXFZ202X", "6", "INDEXFZ2026"),
            ("INDEXFH202X", "7", "INDEXFH2027"),
            ("STOCKAX", "", "STOCKA"),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            let table_study = (expected == "STOCKA").then(|| {
                app.data_table.open();
                app.chart
                    .add_indicator(Box::new(egui_charts::studies::SMA::new(7)))
            });
            app.chart
                .drawings_mut()
                .drawings
                .push(Drawing::new(7, DrawingToolType::TrendLine));
            app.chart.drawings_mut().select(7);
            let key = |key| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            for events in [
                vec![
                    egui::Event::Text(typed.into()),
                    key(egui::Key::Backspace),
                    egui::Event::Text(suffix.into()),
                ],
                vec![key(egui::Key::Enter)],
            ] {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                )
                .drop_without_applying_deltas();
                if app.symbol_search.is_open() {
                    assert_eq!(
                        app.chart.drawings().drawings.len(),
                        1,
                        "search editing must not delete drawings"
                    );
                }
            }
            assert!(
                !app.symbol_search.is_open(),
                "Enter selects the edited {expected} query"
            );
            assert_eq!(app.chart.symbol(), expected);
            if let Some(study) = table_study {
                assert!(app.chart.indicators().any(|(id, _)| id == study));
            }
            assert_eq!(app.data_table.is_open, expected == "STOCKA");
            if expected.starts_with("INDEXF") {
                assert_eq!(
                    app.chart.instrument().unwrap().exchange.as_deref(),
                    Some("Futures")
                );
            }
            assert!(!app.chart.bars(SeriesId::MAIN).is_empty());
            if expected == "SAMPLE" {
                assert_eq!(
                    app.chart.drawings().drawings.len(),
                    1,
                    "search editing must not delete drawings"
                );
            }
        }
    }

    /// Keys arrive one a frame, as a keyboard sends them. A bare number enters minutes, a comma
    /// opens the field for an interval with a unit, and Enter applies either through the normal
    /// host path.
    #[test]
    fn typed_interval_entry_applies_minutes() {
        use eframe::App;
        let typed = |keys: &[&str]| {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            let enter = egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let keys = keys.iter().map(|key| egui::Event::Text((*key).into()));
            for event in keys.chain([enter]) {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events: vec![event],
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                )
                .drop_without_applying_deltas();
            }
            (ctx, app)
        };
        let (_, app) = typed(&[",", "2", "h"]);
        assert_eq!(app.state.timeframe.duration_ms(), 7_200_000);
        let (ctx, mut app) = typed(&["1", "5"]);
        assert_eq!(app.state.timeframe, Timeframe::Min15);
        assert_eq!(app.state.timeframe.duration_ms(), 900_000);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Min15));
        assert_eq!(app.top_toolbar.timeframe_selector.current, Timeframe::Min15);
        let original = app.chart.config().background_color;
        let mut settings = (*app.settings_state).clone();
        settings.chart_basic_styles.background_color = egui::Color32::RED;
        settings.scales_and_lines.invert_scale = true;
        settings.timezone = "America/New_York".into();
        app.apply_chart_settings(settings.clone());
        app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Hour1));
        settings.chart_basic_styles.background_color = egui::Color32::BLUE;
        app.apply_chart_settings(settings);
        for (action, timeframe, color, inverted, timezone) in [
            (
                TopToolbarAction::Undo,
                Timeframe::Hour1,
                egui::Color32::RED,
                true,
                "America/New_York",
            ),
            (
                TopToolbarAction::Undo,
                Timeframe::Min15,
                egui::Color32::RED,
                true,
                "America/New_York",
            ),
            (
                TopToolbarAction::Undo,
                Timeframe::Min15,
                original,
                false,
                "UTC",
            ),
            (
                TopToolbarAction::Undo,
                Timeframe::Min1,
                original,
                false,
                "UTC",
            ),
            (
                TopToolbarAction::Redo,
                Timeframe::Min15,
                original,
                false,
                "UTC",
            ),
            (
                TopToolbarAction::Redo,
                Timeframe::Min15,
                egui::Color32::RED,
                true,
                "America/New_York",
            ),
            (
                TopToolbarAction::Redo,
                Timeframe::Hour1,
                egui::Color32::RED,
                true,
                "America/New_York",
            ),
            (
                TopToolbarAction::Redo,
                Timeframe::Hour1,
                egui::Color32::BLUE,
                true,
                "America/New_York",
            ),
        ] {
            app.handle_top_toolbar(&ctx, action);
            assert_eq!(app.state.timeframe, timeframe);
            assert_eq!(app.chart.timeframe(), Some(timeframe));
            assert_eq!(app.timeframe_toolbar.state.timeframe, timeframe);
            assert_eq!(app.chart.config().background_color, color);
            assert_eq!(
                app.chart.state().price_scale().options().invert_scale,
                inverted
            );
            assert_eq!(app.timeframe_toolbar.state.timezone, timezone);
            let bars = app.chart.bars(SeriesId::MAIN);
            assert_eq!(
                (bars[bars.len() - 1].time - bars[bars.len() - 2].time).num_milliseconds(),
                timeframe.duration_ms()
            );
        }
    }

    /// A single pattern added from the browser tints only its formation, behind the candle.
    #[test]
    fn indicator_browser_adds_a_single_pattern_with_candle_tint() {
        use eframe::App;

        let ctx = egui::Context::default();
        let start = "2026-11-18T15:30:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            start,
            true,
        );
        app.live.set_paused(true);
        settle(&mut app, &ctx);
        *app.chart = Chart::new(BarData::new());
        let mut bars: Vec<_> = (0..14)
            .map(|i| {
                let close = 100.0 + f64::from(i % 2);
                let open = if i == 0 {
                    99.0
                } else {
                    101.0 - f64::from(i % 2)
                };
                Bar::new(
                    start + chrono::TimeDelta::minutes(i.into()),
                    open,
                    open.max(close) + 0.25,
                    open.min(close) - 0.25,
                    close,
                    1.0,
                )
            })
            .collect();
        bars.push(Bar::new(
            start + chrono::TimeDelta::minutes(14),
            100.0,
            102.0,
            98.0,
            100.0625,
            1.0,
        ));
        app.chart.set_data(SeriesId::MAIN, bars.clone());
        app.chart.fit_content();

        let size = egui::vec2(1440.0, 900.0);
        app.handle_top_toolbar(&ctx, TopToolbarAction::OpenIndicators);
        for _ in 0..3 {
            ui_frame(&ctx, size, vec![], |ui| {
                app.ui(ui, &mut eframe::Frame::_new_kittest())
            });
        }
        let texts = ui_frame(&ctx, size, vec![egui::Event::Text("Doji".into())], |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        let row = texts.iter().find(|(text, _)| text == "Doji").unwrap().1;
        ui_click(&ctx, size, row.center(), |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        app.indicator_dialog.close();
        let id = app
            .chart
            .indicators()
            .find(|(_, study)| study.name() == "Doji")
            .unwrap()
            .0;
        // Choose the study's visual order before checking that its tint is behind price.
        assert!(
            app.chart
                .reorder_indicator(id, egui_charts::widget::VisualOrder::SendToBack)
        );
        for _ in 0..18 {
            ui_frame(&ctx, size, vec![], |ui| {
                app.ui(ui, &mut eframe::Frame::_new_kittest())
            });
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
        );
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        assert_eq!(app.chart.bars(SeriesId::MAIN), bars);
        let (_, study) = app
            .chart
            .indicators()
            .find(|(_, study)| study.name() == "Doji")
            .unwrap();
        assert_eq!(study.line_names(), ["Neutral", "Background"]);
        let tinted: Vec<_> = (0..bars.len())
            .filter_map(|i| study.values().value(1, i).map(|value| (i, value)))
            .collect();
        assert_eq!(tinted, [(14, 100.0625)]);

        let map = app.chart.get_chart_mapping();
        let x = map.idx_to_x(14);
        let color = egui::Color32::from_rgba_unmultiplied(120, 123, 134, 26);
        let backgrounds: Vec<_> = shapes
            .iter()
            .enumerate()
            .filter_map(|(i, shape)| {
                if let egui::Shape::Rect(rect) = &shape.shape {
                    (rect.fill == color).then_some((i, rect.rect))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(backgrounds.len(), 1, "one tinted candle column");
        let (background_at, rect) = backgrounds[0];
        assert!((rect.center().x - x).abs() < 0.01);
        assert!((rect.width() - map.bar_spacing).abs() < 0.01);
        assert!((rect.top() - map.rect.top()).abs() < 0.01);
        assert!((rect.bottom() - map.rect.bottom()).abs() < 0.01);
        let candle = egui::pos2(x, map.price.price_to_y(100.03125));
        let candle_at = shapes.iter().position(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill != color && rect.rect.contains(candle)
                && rect.rect.width() < map.bar_spacing && rect.rect.height() < map.rect.height()
        )).expect("the Doji candle body");
        assert!(background_at < candle_at, "the tint is behind the candle");
    }

    /// Quick search executes catalogue entries as well as opening settings.
    #[test]
    fn quick_search_executes_studies_tools_and_settings() {
        use eframe::App;
        use egui::{Event, Key, Modifiers, RawInput};
        for (query, same_frame, toolbar) in [
            "RSI",
            "Trend Line",
            "Chart settings",
            "Sample backtest",
            "Export chart",
            "Volatility smile",
            "Yield curve",
            "Primitives and custom series",
            "Sample study editor",
            "Undo",
            "Take a snapshot",
            "Toggle percent scale",
        ]
        .into_iter()
        .flat_map(|query| {
            [
                (query, false, false),
                (query, true, false),
                (query, true, true),
            ]
        }) {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            settle(&mut app, &ctx);
            app.live.set_paused(true);
            if matches!(
                query,
                "RSI"
                    | "Volatility smile"
                    | "Yield curve"
                    | "Primitives and custom series"
                    | "Undo"
            ) {
                app.chart.user_action(|chart| {
                    let dm = chart.drawings_mut();
                    let tool = egui_charts::drawings::DrawingToolType::TrendLine;
                    dm.start_drawing_with_coords(tool, egui::pos2(10.0, 100.0), |x| x, f64::from);
                    dm.add_point_with_coords(egui::pos2(20.0, 120.0), |x| x, f64::from);
                });
            }
            if toolbar {
                app.handle_top_toolbar(&ctx, TopToolbarAction::OpenQuickSearch);
                assert!(!app.symbol_search.is_open());
            }
            let key = |key, modifiers| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            };
            for (modifiers, events) in [
                (
                    Modifiers::CTRL.plus(Modifiers::COMMAND),
                    if same_frame {
                        vec![
                            if toolbar {
                                Event::ModifiersChanged(Modifiers::NONE)
                            } else {
                                key(Key::K, Modifiers::CTRL.plus(Modifiers::COMMAND))
                            },
                            Event::Text(query.into()),
                        ]
                    } else {
                        vec![key(Key::K, Modifiers::CTRL.plus(Modifiers::COMMAND))]
                    },
                ),
                (
                    Modifiers::NONE,
                    if same_frame {
                        vec![]
                    } else {
                        vec![Event::Text(query.into())]
                    },
                ),
                (Modifiers::NONE, vec![key(Key::Enter, Modifiers::NONE)]),
            ] {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events: std::iter::once(Event::ModifiersChanged(modifiers))
                            .chain(events)
                            .collect(),
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                )
                .drop_without_applying_deltas();
            }
            match query {
                "RSI" => {
                    let id = app
                        .chart
                        .indicators()
                        .find(|(_, study)| study.name() == "RSI")
                        .expect("palette adds RSI")
                        .0;
                    let volume_id = app
                        .chart
                        .indicators()
                        .find(|(_, study)| study.is_volume())
                        .expect("the default chart shows Volume")
                        .0;
                    for (action, studies, drawings, can_undo, can_redo) in [
                        (TopToolbarAction::Undo, 0, 1, true, true),
                        (TopToolbarAction::Redo, 1, 1, true, false),
                        (TopToolbarAction::Undo, 0, 1, true, true),
                        (TopToolbarAction::Undo, 0, 0, false, true),
                        (TopToolbarAction::Redo, 0, 1, true, true),
                        (TopToolbarAction::Redo, 1, 1, true, false),
                    ] {
                        app.handle_top_toolbar(&ctx, action);
                        let expected = if studies == 1 {
                            vec![volume_id, id]
                        } else {
                            vec![volume_id]
                        };
                        assert_eq!(
                            app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                            expected
                        );
                        assert_eq!(app.chart.drawings().drawings.len(), drawings);
                        ctx.run_ui(RawInput::default(), |ui| {
                            app.show_top_toolbar(ui, egui::Color32::BLACK)
                        })
                        .drop_without_applying_deltas();
                        assert_eq!(app.top_toolbar.state.can_undo, can_undo);
                        assert_eq!(app.top_toolbar.state.can_redo, can_redo);
                    }
                }
                "Trend Line" => assert_eq!(
                    app.chart.drawings().active_tool(),
                    Some(egui_charts::drawings::DrawingToolType::TrendLine)
                ),
                "Undo" => assert!(app.chart.drawings().drawings.is_empty()),
                // The row shows the snapshot shortcut's keys and runs what they run.
                "Take a snapshot" => assert!(matches!(
                    app.screenshot_requested,
                    Some((ScreenshotTarget::Post { copy_link: true }, ..))
                )),
                "Toggle percent scale" => assert_eq!(
                    app.chart.state().price_scale().options().mode,
                    egui_charts::scales::PriceScaleMode::Percentage
                ),
                "Sample backtest" => {
                    let output = ctx.run_ui(RawInput::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest());
                    });
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == "Strategy report")));
                    output.drop_without_applying_deltas();
                }
                "Export chart" => {
                    assert!(app.export_dialog.is_open());
                    let output = ctx.run_ui(RawInput::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest());
                    });
                    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text().starts_with("Error:"))));
                    output.drop_without_applying_deltas();
                }
                "Volatility smile"
                | "Yield curve"
                | "Primitives and custom series"
                | "Sample study editor" => {
                    let label = match query {
                        "Volatility smile" => "30-day IV, %",
                        "Yield curve" => "Today",
                        "Primitives and custom series" => "Bar range, price units",
                        _ => "Sample study editor",
                    };
                    let bars = app.chart.bars(SeriesId::MAIN).to_vec();
                    let studies: Vec<_> = app.chart.indicators().map(|(id, _)| id).collect();
                    let range = app.chart.get_time_scale_state();
                    let can_undo = app.chart.can_undo();
                    let mut found = false;
                    for _ in 0..3 {
                        let output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(1440.0, 900.0),
                                )),
                                ..Default::default()
                            },
                            |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                        );
                        found |= output.shapes.iter().any(|shape| {
                            matches!(&shape.shape,
                            egui::Shape::Text(text) if text.galley.text().contains(label))
                        });
                        output.drop_without_applying_deltas();
                    }
                    assert!(found, "{query} must show {label}");
                    if query == "Primitives and custom series" {
                        let frame = |app: &mut DemoApp, events| {
                            ctx.run_ui(
                                RawInput {
                                    events,
                                    ..Default::default()
                                },
                                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                            )
                        };
                        let output = frame(&mut app, Vec::new());
                        let head = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Circle(circle)
                                    if circle.fill == egui::Color32::from_rgb(255, 152, 0)
                                        && shape.clip_rect.contains(circle.center) =>
                                {
                                    Some(circle.center)
                                }
                                _ => None,
                            })
                            .max_by(|left, right| left.y.total_cmp(&right.y))
                            .expect("custom range heads are visible");
                        output.drop_without_applying_deltas();
                        let output = frame(&mut app, vec![Event::PointerMoved(head)]);
                        let cursor = output.platform_output.cursor_icon;
                        output.drop_without_applying_deltas();
                        assert_eq!(cursor, egui::CursorIcon::PointingHand);
                        for enabled in [false, true] {
                            for control in ["Session background", "Custom range series"] {
                                let output = frame(&mut app, Vec::new());
                                let point = output
                                    .shapes
                                    .iter()
                                    .find_map(|shape| match &shape.shape {
                                        egui::Shape::Text(text)
                                            if text.galley.text() == control =>
                                        {
                                            Some(text.pos + text.galley.size() / 2.0)
                                        }
                                        _ => None,
                                    })
                                    .unwrap();
                                output.drop_without_applying_deltas();
                                frame(
                                    &mut app,
                                    vec![
                                        Event::PointerMoved(point),
                                        Event::PointerButton {
                                            pos: point,
                                            button: egui::PointerButton::Primary,
                                            pressed: true,
                                            modifiers: Modifiers::NONE,
                                        },
                                        Event::PointerButton {
                                            pos: point,
                                            button: egui::PointerButton::Primary,
                                            pressed: false,
                                            modifiers: Modifiers::NONE,
                                        },
                                    ],
                                )
                                .drop_without_applying_deltas();
                            }
                            let output = frame(&mut app, Vec::new());
                            for needle in ["Open ", "Bar range, price units"] {
                                let visible = output.shapes.iter().any(|shape| {
                                    matches!(&shape.shape,
                                    egui::Shape::Text(text) if text.galley.text().contains(needle))
                                });
                                assert_eq!(visible, enabled, "{needle}");
                            }
                            output.drop_without_applying_deltas();
                        }
                    }
                    if query != "Sample study editor" {
                        if query == "Volatility smile" && same_frame && toolbar {
                            app.keyboard_shortcuts.open();
                        }
                        for (events, modifiers) in [
                            (vec![key(Key::Home, Modifiers::NONE)], Modifiers::NONE),
                            (vec![key(Key::K, Modifiers::COMMAND)], Modifiers::COMMAND),
                            (
                                vec![Event::Text("Return to price chart".into())],
                                Modifiers::NONE,
                            ),
                            (vec![key(Key::Enter, Modifiers::NONE)], Modifiers::NONE),
                        ] {
                            ctx.run_ui(
                                RawInput {
                                    events: std::iter::once(Event::ModifiersChanged(modifiers))
                                        .chain(events)
                                        .collect(),
                                    ..Default::default()
                                },
                                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                            )
                            .drop_without_applying_deltas();
                            assert_eq!(
                                app.chart.get_time_scale_state(),
                                range,
                                "sample keys must not move the hidden workspace"
                            );
                        }
                        let output = ctx.run_ui(RawInput::default(), |ui| {
                            app.ui(ui, &mut eframe::Frame::_new_kittest())
                        });
                        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::Shape::Text(text) if text.galley.text() == "Return to price chart")));
                        output.drop_without_applying_deltas();
                        app.execute_search_command(&ctx, "price-workspace");
                        assert_eq!(app.chart.bars(SeriesId::MAIN), bars);
                        assert_eq!(
                            app.chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                            studies
                        );
                        assert_eq!(app.chart.get_time_scale_state(), range);
                        assert_eq!(app.chart.drawings().drawings.len(), 1);
                        assert_eq!(app.chart.can_undo(), can_undo);
                    }
                }
                _ => assert!(app.settings_dialog.is_open),
            }
        }
    }

    /// The real More menu exposes shared state and removes it once the demo host revokes it.
    #[test]
    fn watchlist_sharing_can_be_stopped_from_more() {
        use eframe::App;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.open_panel = Some(SidePanel::Watchlist);
        let size = egui::vec2(1440.0, 900.0);
        let click = |app: &mut DemoApp, label: &str| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
            let point = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    (node.label() == Some(label) && node.role() != egui::accesskit::Role::Label)
                        .then(|| node.bounds())
                        .flatten()
                })
                .min_by(|a, b| a.y0.total_cmp(&b.y0))
                .map(|rect| {
                    egui::pos2(
                        ((rect.x0 + rect.x1) / 2.0) as f32,
                        ((rect.y0 + rect.y1) / 2.0) as f32,
                    )
                })
                .unwrap_or_else(|| panic!("missing {label}"));
            output.drop_without_applying_deltas();
            ui_click(&ctx, size, point, |ui| {
                app.ui(ui, &mut eframe::Frame::_new_kittest())
            });
        };
        click(&mut app, "More");
        click(&mut app, "Copy link");
        let shown = ui_frame(&ctx, size, vec![], |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        assert!(shown.iter().any(|(text, _)| text == "Shared by link"));
        click(&mut app, "Stop sharing");
        let shown = ui_frame(&ctx, size, vec![], |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        assert!(shown.iter().any(|(text, _)| text == "Sharing stopped"));
        assert!(!shown.iter().any(|(text, _)| text == "Stop sharing"));
    }

    /// The focused chart adds to the shown list, or flags its symbol while a flag view is shown,
    /// reports duplicates and restores that list and the flag.
    #[test]
    fn the_shown_watchlist_takes_alt_w_and_survives_a_restart() {
        use eframe::App;
        use egui::{Event, Key, Modifiers};
        use egui_charts::model::{Watchlist, WatchlistFlag};
        let ctx = egui::Context::default();
        // The demo builds its bars from the clock; a fixed time gives every run the same chart.
        let now = "2026-11-22T02:05:02Z".parse().unwrap();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, now, true);
        app.watchlist = WatchlistPanel::new(vec![
            Watchlist::from_text("One", "STOCKA"),
            Watchlist::from_text("Two", "STOCKB"),
        ]);
        app.watchlist.set_active_list(1);
        app.open_catalog_symbol("STOCKC");
        settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        ui_frame(&ctx, size, vec![], |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        let chart = app.chart.get_chart_mapping().rect.center();
        ui_click(&ctx, size, chart, |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        let alt_w = |app: &mut DemoApp| {
            ui_frame(
                &ctx,
                size,
                vec![
                    Event::ModifiersChanged(Modifiers::ALT),
                    Event::Key {
                        key: Key::W,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::ALT,
                    },
                    Event::Key {
                        key: Key::W,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: Modifiers::ALT,
                    },
                ],
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
        };
        for duplicate in [false, true] {
            alt_w(&mut app);
            assert_eq!(app.watchlist.lists()[0].to_text(), "STOCKA");
            assert_eq!(app.watchlist.lists()[1].to_text(), "STOCKB,STOCKC");
            if duplicate {
                assert!(
                    app.toasts
                        .toasts()
                        .iter()
                        .any(|toast| toast.message == "STOCKC is already in that watchlist")
                );
            }
        }
        // In a flag view the key flags the chart symbol instead of adding it to the hidden list.
        // The chart keeps its focus: a second click this soon is a double click, which opens the
        // chart settings when the series lies under it.
        app.watchlist.set_flag_filter(Some(WatchlistFlag::Red));
        app.open_catalog_symbol("STOCKD");
        settle(&mut app, &ctx);
        alt_w(&mut app);
        assert_eq!(app.watchlist.symbols().collect::<Vec<_>>(), ["STOCKD"]);
        assert_eq!(app.watchlist.lists()[1].to_text(), "STOCKB,STOCKC");
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        let mut cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        cc.storage = Some(&storage);
        let restored = DemoApp::new_at(&cc, now, true);
        assert_eq!(restored.watchlist.active_list(), 1);
        assert_eq!(
            restored.watchlist.symbols().collect::<Vec<_>>(),
            ["STOCKB", "STOCKC"]
        );
        assert_eq!(
            restored.watchlist.unlisted_flags(),
            [("STOCKD".to_owned(), WatchlistFlag::Red)]
        );
    }

    /// A chosen watchlist file imports as a list named after the file without its extension, and
    /// a file that is not UTF-8 text adds no list and says so.
    #[test]
    fn a_chosen_watchlist_file_imports_under_its_name_and_text_only() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        for (file, bytes, imported) in [
            (
                "Tech.txt",
                b"NASDAQ:AAPL,###Chips,NASDAQ:NVDA".to_vec(),
                true,
            ),
            ("Bad.txt", vec![b'N', 0xff, b'A'], false),
        ] {
            let before = app.watchlist.lists().len();
            let (tx, rx) = std::sync::mpsc::channel();
            app.watchlist_import = Some(rx);
            tx.send(Ok(Some((file.to_owned(), bytes)))).unwrap();
            settle(&mut app, &ctx);
            let lists = app.watchlist.lists();
            if imported {
                assert_eq!(lists.len(), before + 1, "{file}");
                let list = lists.last().unwrap();
                assert_eq!(list.name, "Tech");
                assert_eq!(list.to_text(), "NASDAQ:AAPL,###Chips,NASDAQ:NVDA");
                assert_eq!(app.watchlist.active_list(), before);
            } else {
                assert_eq!(lists.len(), before, "{file}");
                assert!(
                    app.toasts
                        .toasts()
                        .iter()
                        .any(|toast| toast.message == "Bad.txt is not UTF-8 text")
                );
            }
        }
    }

    #[test]
    fn watchlist_shortcuts_follow_the_filtered_order_and_replay_keeps_its_keys() {
        use eframe::App;
        use egui::{Event, Key, Modifiers};
        use egui_charts::model::{Watchlist, WatchlistFlag, WatchlistItem};

        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        let mut list = Watchlist::from_text("List", "STOCKC,###Section,STOCKB,TOKENA,STOCKA");
        for item in &mut list.items {
            if let WatchlistItem::Symbol { symbol, flag } = item
                && symbol != "TOKENA"
            {
                *flag = Some(WatchlistFlag::Red);
            }
        }
        app.watchlist = WatchlistPanel::new(vec![list]);
        app.watchlist.set_flag_filter(Some(WatchlistFlag::Red));
        app.open_panel = Some(SidePanel::Watchlist);
        settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        let texts = ui_frame(&ctx, size, vec![], |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        let header = texts.iter().find(|(text, _)| text == "Symbol").unwrap().1;
        ui_click(&ctx, size, header.center(), |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        let chart = app.chart.get_chart_mapping().rect.center();
        ui_click(&ctx, size, chart, |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest())
        });
        settle(&mut app, &ctx);
        let chart_focus = ctx.memory(|memory| memory.focused()).unwrap();
        let press = |app: &mut DemoApp, key, modifiers| {
            ui_frame(
                &ctx,
                size,
                vec![
                    Event::ModifiersChanged(modifiers),
                    Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                    Event::Key {
                        key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    },
                ],
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
        };
        for (key, modifiers, symbol) in [
            (Key::ArrowDown, Modifiers::NONE, "STOCKA"),
            (Key::ArrowDown, Modifiers::NONE, "STOCKB"),
            (Key::ArrowUp, Modifiers::NONE, "STOCKA"),
            (Key::ArrowUp, Modifiers::NONE, "STOCKC"),
            (Key::ArrowDown, Modifiers::NONE, "STOCKA"),
            (Key::Space, Modifiers::NONE, "STOCKB"),
            (Key::Space, Modifiers::SHIFT, "STOCKA"),
        ] {
            press(&mut app, key, modifiers);
            assert_eq!(app.chart.symbol(), symbol, "{key:?} / {modifiers:?}");
            assert_eq!(app.state.symbol, symbol);
            assert_eq!(app.live.instrument().symbol, symbol);
            settle(&mut app, &ctx);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(chart_focus),
                "{key:?} changed keyboard focus"
            );
        }

        // With the replay bar shown its keys step and play, and the chart keeps its own: the jump
        // keys, which hold Alt beside the replay's Shift and arrow, and the list keys below.
        app.start_replay();
        app.handle_replay_control(ReplayControlAction::Pause);
        settle(&mut app, &ctx);
        let jump = Modifiers::ALT | Modifiers::SHIFT;
        // The cursor's move, unchecked while the replay plays, and whether it plays afterwards.
        for (key, modifiers, moved, playing) in [
            (Key::ArrowRight, Modifiers::NONE, Some(1), false),
            (Key::ArrowLeft, Modifiers::NONE, Some(-1), false),
            (Key::ArrowDown, Modifiers::SHIFT, None, true),
            (Key::ArrowDown, Modifiers::SHIFT, None, false),
            (Key::ArrowRight, jump, Some(0), false),
            (Key::ArrowLeft, jump, Some(0), false),
            (Key::ArrowRight, Modifiers::SHIFT, Some(1), false),
        ] {
            let before = app.replay.as_ref().unwrap().state.curr_bar;
            press(&mut app, key, modifiers);
            assert_eq!(app.chart.symbol(), "STOCKA");
            let replay = app.replay.as_ref().unwrap();
            if let Some(moved) = moved {
                assert_eq!(
                    replay.state.curr_bar as isize - before as isize,
                    moved,
                    "{key:?} / {modifiers:?}"
                );
            }
            assert_eq!(
                replay.state.is_playing(),
                playing,
                "{key:?} / {modifiers:?}"
            );
        }
        // The list keys flip the list beside the replay bar too. A symbol change exits replay as
        // a row click does.
        for (key, modifiers, symbol) in [
            (Key::Space, Modifiers::NONE, "STOCKB"),
            (Key::Space, Modifiers::SHIFT, "STOCKA"),
            (Key::ArrowDown, Modifiers::NONE, "STOCKB"),
        ] {
            settle(&mut app, &ctx);
            app.start_replay();
            settle(&mut app, &ctx);
            assert!(app.replay.is_some() && app.replay_control.is_visible());
            press(&mut app, key, modifiers);
            assert_eq!(app.chart.symbol(), symbol, "{key:?} / {modifiers:?}");
            assert!(app.replay.is_none(), "{key:?} / {modifiers:?}");
        }
    }

    #[test]
    fn session_shortcut_switches_symbols_with_hours_and_keeps_continuous_markets() {
        for (symbol, initial, expected) in [
            ("SAMPLE", SessionType::Full24x7, SessionType::RTH),
            ("SAMPLE", SessionType::RTH, SessionType::ETH),
            ("SAMPLE", SessionType::ETH, SessionType::RTH),
            ("STOCKA", SessionType::RTH, SessionType::ETH),
            ("TOKENA", SessionType::Full24x7, SessionType::Full24x7),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            if symbol != "SAMPLE" {
                app.switch_symbol(symbol);
            }
            app.timeframe_toolbar.set_session(initial);
            app.apply_session();
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::E,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::ALT.plus(egui::Modifiers::SHIFT),
                    }],
                    ..Default::default()
                },
                |ui| app.handle_keyboard(ui.ctx()),
            )
            .drop_without_applying_deltas();
            assert_eq!(
                app.timeframe_toolbar.state.session_type, expected,
                "{symbol} / {initial:?}"
            );
            assert_eq!(app.chart.extended_hours(), expected != SessionType::RTH);
            assert_eq!(app.live.view(app.chart.extended_hours()).0, expected);
        }
    }

    /// Settings and their undo keep the session in the chart, toolbar and demo feed together,
    /// including when a restored clock change or a dividend choice also reloads the bars.
    #[test]
    fn settings_session_follows_undo_with_a_clock_change() {
        for zone in [None, Some("Europe/Zurich")] {
            let ctx = egui::Context::default();
            let now = "2026-11-18T15:30:00Z".parse().unwrap();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                now,
                true,
            );
            settle(&mut app, &ctx);
            app.timeframe_toolbar.set_session(SessionType::RTH);
            app.apply_session();
            let mut settings = app.chart_settings_state();
            settings.extended_hours = Some(true);
            if let Some(zone) = zone {
                settings.timezone = zone.into();
            }
            app.apply_chart_settings(settings);
            let session = |app: &DemoApp| {
                (
                    app.chart.extended_hours(),
                    app.timeframe_toolbar.state.session_type,
                    app.live.view(app.chart.extended_hours()).0,
                )
            };
            assert_eq!(
                session(&app),
                (true, SessionType::ETH, SessionType::ETH),
                "{zone:?}: apply"
            );
            assert!(app.chart.undo());
            app.sync_chart_history();
            assert_eq!(
                session(&app),
                (false, SessionType::RTH, SessionType::RTH),
                "{zone:?}: undo"
            );
            assert!(app.chart.redo());
            app.sync_chart_history();
            assert_eq!(
                session(&app),
                (true, SessionType::ETH, SessionType::ETH),
                "{zone:?}: redo"
            );
        }

        // A dividend reload keeps a 24/7 view, which the chart holds as extended hours.
        let ctx = egui::Context::default();
        let now = "2026-11-18T15:30:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        settle(&mut app, &ctx);
        app.timeframe_toolbar.set_session(SessionType::Full24x7);
        app.apply_session();
        let mut settings = app.chart_settings_state();
        settings.adjust_for_dividends = !settings.adjust_for_dividends;
        app.apply_chart_settings(settings);
        assert_eq!(
            (
                app.timeframe_toolbar.state.session_type,
                app.live.view(app.chart.extended_hours()).0,
            ),
            (SessionType::Full24x7, SessionType::Full24x7)
        );
    }

    /// The demo's chart settings reach the custom-session list and apply it to its chart.
    #[test]
    fn session_profile_custom_sessions_are_reachable_from_demo_settings() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.chart.set_chart_type(ChartType::SessionVolumeProfile);
        app.open_chart_settings_dialog();
        let size = egui::vec2(1440.0, 900.0);
        let click = |app: &mut DemoApp, text: &str, scroll: bool| {
            let show = |app: &mut DemoApp, events| {
                ui_frame(&ctx, size, events, |ui| app.show_dialogs(ui.ctx()))
            };
            for _ in 0..20 {
                for _ in 0..5 {
                    show(app, vec![]);
                }
                if let Some((_, rect)) = show(app, vec![]).into_iter().find(|(label, rect)| {
                    label == text && (180.0..720.0).contains(&rect.center().y)
                }) {
                    ui_click(&ctx, size, rect.center(), |ui| app.show_dialogs(ui.ctx()));
                    return;
                }
                assert!(scroll, "missing {text}");
                show(
                    app,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(900.0, 450.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            source: egui::MouseWheelSource::Wheel,
                            phase: egui::TouchPhase::Move,
                            delta: egui::vec2(0.0, -100.0),
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            panic!("missing {text}");
        };
        click(&mut app, "Chart hours", true);
        click(&mut app, "Custom", false);
        click(&mut app, "Add session", true);
        // The footer is fixed and does not need scrolling.
        let texts = ui_frame(&ctx, size, vec![], |ui| app.show_dialogs(ui.ctx()));
        let point = texts
            .iter()
            .find(|(text, _)| text == "Ok")
            .unwrap()
            .1
            .center();
        ui_click(&ctx, size, point, |ui| app.show_dialogs(ui.ctx()));
        let egui_charts::model::ProfileSession::Custom(custom) =
            &app.chart.trade_profile_settings().session
        else {
            panic!("custom hours applied")
        };
        assert_eq!(custom.sessions.len(), 2);
    }

    /// The demo's chart settings offer the Overnight background switch, and Ok hides the
    /// overnight background of its chart while the post-market stays shaded.
    #[test]
    fn chart_settings_overnight_switch_reaches_the_demo_chart() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.open_chart_settings_dialog();
        let size = egui::vec2(1440.0, 900.0);
        let show = |app: &mut DemoApp, events| {
            ui_frame(&ctx, size, events, |ui| app.show_dialogs(ui.ctx()))
        };
        // The Session backgrounds sit below the fold of the Symbol tab; the footer is fixed.
        let switch = (0..20)
            .find_map(|_| {
                for _ in 0..5 {
                    show(&mut app, vec![]);
                }
                let found = show(&mut app, vec![]).into_iter().find(|(label, rect)| {
                    label == "Overnight" && (180.0..720.0).contains(&rect.center().y)
                });
                if found.is_none() {
                    show(
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(egui::pos2(900.0, 450.0)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                source: egui::MouseWheelSource::Wheel,
                                phase: egui::TouchPhase::Move,
                                delta: egui::vec2(0.0, -100.0),
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
                found
            })
            .expect("the Overnight switch is offered")
            .1
            .center();
        ui_click(&ctx, size, switch, |ui| app.show_dialogs(ui.ctx()));
        let ok = show(&mut app, vec![])
            .into_iter()
            .find(|(text, _)| text == "Ok")
            .unwrap()
            .1
            .center();
        ui_click(&ctx, size, ok, |ui| app.show_dialogs(ui.ctx()));
        let config = app.chart.config();
        assert!(!config.shade_overnight && config.shade_postmarket);
    }

    /// The demo exposes the Columns close switch and Range's independent colours and Thin bars.
    #[test]
    fn chart_settings_columns_and_range_rows_reach_the_demo_chart() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        let size = egui::vec2(1440.0, 900.0);
        for kind in [ChartType::Columns, ChartType::Range] {
            app.state.chart_type = kind;
            app.chart.set_chart_type(kind);
            app.open_chart_settings_dialog();
            let show = |app: &mut DemoApp, events| {
                ui_frame(&ctx, size, events, |ui| app.show_dialogs(ui.ctx()))
            };
            let mut labels = Vec::new();
            for _ in 0..5 {
                labels = show(&mut app, vec![]);
            }
            if kind == ChartType::Range {
                for label in ["Rising bar colour", "Falling bar colour", "Thin bars"] {
                    assert!(
                        labels.iter().any(|(text, _)| text == label),
                        "{label} is offered"
                    );
                }
            } else {
                let switch = labels
                    .iter()
                    .find(|(text, _)| text == "Color bars based on previous close")
                    .expect("the Columns close switch is offered")
                    .1
                    .center();
                ui_click(&ctx, size, switch, |ui| app.show_dialogs(ui.ctx()));
            }
            let ok = show(&mut app, vec![])
                .into_iter()
                .find(|(text, _)| text == "Ok")
                .unwrap()
                .1
                .center();
            ui_click(&ctx, size, ok, |ui| app.show_dialogs(ui.ctx()));
            if kind == ChartType::Columns {
                assert!(
                    !app.chart
                        .chart_type_params()
                        .styles
                        .columns
                        .color_by_previous_close
                );
            }
        }
    }

    /// The demo's chart settings offer the switch that keeps the left edge, and with it on an
    /// interval change keeps the bar at the left edge of the demo chart in place.
    #[test]
    fn chart_settings_keep_left_edge_switch_reaches_the_demo_chart() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        app.open_chart_settings_dialog();
        let size = egui::vec2(1440.0, 900.0);
        let show = |app: &mut DemoApp, events| {
            ui_frame(&ctx, size, events, |ui| app.show_dialogs(ui.ctx()))
        };
        // Above the fixed footer of the dialog, which the last row of the tab scrolls down to.
        let find = |app: &mut DemoApp, label: &str| {
            show(app, vec![])
                .into_iter()
                .find(|(text, rect)| text == label && (180.0..790.0).contains(&rect.center().y))
                .map(|(_, rect)| rect.center())
        };
        show(&mut app, vec![]);
        let tab = find(&mut app, "Scales and lines").expect("the Scales and lines tab");
        ui_click(&ctx, size, tab, |ui| app.show_dialogs(ui.ctx()));
        // The Time scale section ends the tab, below the fold.
        let switch = (0..20)
            .find_map(|_| {
                for _ in 0..5 {
                    show(&mut app, vec![]);
                }
                let found = find(&mut app, "Keep left edge on interval change");
                if found.is_none() {
                    show(
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(egui::pos2(900.0, 450.0)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                source: egui::MouseWheelSource::Wheel,
                                phase: egui::TouchPhase::Move,
                                delta: egui::vec2(0.0, -100.0),
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
                found
            })
            .expect("the switch is offered");
        ui_click(&ctx, size, switch, |ui| app.show_dialogs(ui.ctx()));
        let ok = show(&mut app, vec![])
            .into_iter()
            .find(|(text, _)| text == "Ok")
            .unwrap()
            .1
            .center();
        ui_click(&ctx, size, ok, |ui| app.show_dialogs(ui.ctx()));
        assert!(
            app.chart
                .chart_options()
                .time_scale
                .keep_left_edge_on_interval_change
        );
        // The open of the bar at the left edge and of the bar after it.
        let left = |app: &DemoApp| {
            let index = app.chart.visible_logical_range().unwrap().left.floor() as usize;
            (
                app.chart.time_at_index(index).unwrap(),
                app.chart.time_at_index(index + 1).unwrap(),
            )
        };
        let middle = (app.chart.bars(SeriesId::MAIN).len() / 2) as f32;
        app.chart
            .set_visible_logical_range(egui_charts::model::LogicalRange::new(
                middle + 0.25,
                middle + 99.25,
            ));
        settle(&mut app, &ctx);
        let (kept, _) = left(&app);
        app.switch_timeframe(Timeframe::Hour1);
        settle(&mut app, &ctx);
        let (open, next) = left(&app);
        assert!(open <= kept && kept < next, "{open} {kept} {next}");
    }

    /// The settings offer the futures data choices on the demo's futures contracts only;
    /// a changed choice reloads the bars, while Ok without a change keeps them.
    #[test]
    fn futures_data_choices_show_for_futures_and_reload_the_bars() {
        const ROWS: [&str; 2] = ["Back-adjust continuous futures", "Settlement as close"];
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        let size = egui::vec2(1440.0, 900.0);
        // Search from the top even when an earlier opening left the tab scrolled down.
        let scroll_to = |app: &mut DemoApp, label: &str| -> Option<egui::Pos2> {
            let wheel = |dy: f32| {
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    source: egui::MouseWheelSource::Wheel,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0.0, dy),
                    modifiers: Default::default(),
                }]
            };
            let show = |app: &mut DemoApp, events| {
                ui_frame(&ctx, size, events, |ui| app.show_dialogs(ui.ctx()))
            };
            show(
                app,
                vec![egui::Event::PointerMoved(egui::pos2(900.0, 450.0))],
            );
            show(app, wheel(5000.0));
            for _ in 0..20 {
                for _ in 0..10 {
                    show(app, vec![]);
                }
                if let Some((_, rect)) = show(app, vec![])
                    .into_iter()
                    .find(|(text, _)| text == label)
                {
                    return Some(rect.center());
                }
                show(app, wheel(-150.0));
            }
            None
        };
        for (symbol, offered) in [("STOCKA", false), (live::ORDER_LEVEL_SYMBOL, true)] {
            app.switch_symbol(symbol);
            app.open_chart_settings_dialog();
            assert!(
                scroll_to(&mut app, "Adjust data for dividends").is_some(),
                "{symbol}"
            );
            for row in ROWS {
                app.open_chart_settings_dialog();
                assert_eq!(
                    scroll_to(&mut app, row).is_some(),
                    offered,
                    "{symbol}: {row}"
                );
            }
            app.settings_dialog.close();
        }
        // One bar makes a reload observable without inspecting the feed's internals.
        for (row, reloaded) in [(None, false), (Some(ROWS[1]), true)] {
            let last = app.chart.bars(SeriesId::MAIN).last().unwrap().clone();
            app.chart.set_data(SeriesId::MAIN, vec![last]);
            app.open_chart_settings_dialog();
            if let Some(row) = row {
                let at = scroll_to(&mut app, row).expect(row);
                ui_click(&ctx, size, at, |ui| app.show_dialogs(ui.ctx()));
            }
            let ok = scroll_to(&mut app, "Ok").expect("Ok");
            ui_click(&ctx, size, ok, |ui| app.show_dialogs(ui.ctx()));
            assert!(!app.settings_dialog.is_open, "{row:?}");
            assert_eq!(
                app.chart.bars(SeriesId::MAIN).len() > 1,
                reloaded,
                "{row:?}"
            );
        }
        assert!(!ChartSettingsState::from_chart(&app.chart).settlement_as_close);
    }

    /// The demo opens the footprint colouring rows, hides side shade pickers when unused,
    /// and applies the accepted choice to its chart.
    #[test]
    fn footprint_colourings_reach_the_chart_through_demo_settings() {
        use egui_charts::model::FootprintColor;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        let size = egui::vec2(1440.0, 900.0);
        let scroll_to = |app: &mut DemoApp, label: &str| -> Option<egui::Pos2> {
            let wheel = |dy: f32| {
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    source: egui::MouseWheelSource::Wheel,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0.0, dy),
                    modifiers: Default::default(),
                }]
            };
            let show = |app: &mut DemoApp, events| {
                ui_frame(&ctx, size, events, |ui| app.show_dialogs(ui.ctx()))
            };
            show(
                app,
                vec![egui::Event::PointerMoved(egui::pos2(900.0, 450.0))],
            );
            show(app, wheel(5000.0));
            for _ in 0..20 {
                for _ in 0..10 {
                    show(app, vec![]);
                }
                if let Some((_, rect)) = show(app, vec![])
                    .into_iter()
                    .find(|(text, _)| text == label)
                {
                    return Some(rect.center());
                }
                show(app, wheel(-150.0));
            }
            None
        };
        app.chart.set_chart_type(ChartType::VolumeFootprint);
        app.open_chart_settings_dialog();
        assert!(scroll_to(&mut app, "Buy shade 1").is_some());
        let colouring = scroll_to(&mut app, "Volume steps").expect("Colouring combo");
        ui_click(&ctx, size, colouring, |ui| app.show_dialogs(ui.ctx()));
        let choices = ui_frame(&ctx, size, vec![], |ui| app.show_dialogs(ui.ctx()));
        assert!(choices.iter().any(|(text, _)| text == "Side colours"));
        let larger = choices
            .into_iter()
            .find(|(text, _)| text == "Larger side")
            .expect("Larger side choice")
            .1
            .center();
        ui_click(&ctx, size, larger, |ui| app.show_dialogs(ui.ctx()));
        assert!(scroll_to(&mut app, "Buy shade 1").is_none());
        assert_eq!(
            app.chart.chart_type_params().footprint_color_by,
            FootprintColor::VolumeSteps,
            "the choice stays staged until Ok"
        );
        let ok = scroll_to(&mut app, "Ok").expect("Ok");
        ui_click(&ctx, size, ok, |ui| app.show_dialogs(ui.ctx()));
        assert!(!app.settings_dialog.is_open);
        assert_eq!(
            app.chart.chart_type_params().footprint_color_by,
            FootprintColor::LargerSide
        );
    }

    /// A chart of the symbol whose feed prepares footprint rows first asks for the rows of the
    /// bars on view, not the whole initial page, and loads older rows as it scrolls back, also
    /// for minutes before the stream's first execution, without asking for the trade tape.
    /// Opening time and sales asks for it once.
    #[test]
    fn prepared_footprint_chart_pages_rows_by_view_and_loads_its_tape_once_time_and_sales_opens() {
        use egui_charts::data::{
            Datafeed, DepthPage, DepthRequest, FootprintPage, FootprintRequest, HistoryPage,
            HistoryRequest, Reply, Subscription, SubscriptionId, TradePage, TradeRequest,
        };
        use egui_charts::model::Instrument;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

        /// The demo feed, counting the trade pages and keeping the footprint requests asked of it.
        struct Counted(Arc<LiveFeed>, AtomicUsize, Mutex<Vec<FootprintRequest>>);

        impl Datafeed for Counted {
            fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
                self.0.resolve(symbol, reply);
            }
            fn resolve_converted(&self, symbol: &str, conversion: &str, reply: Reply<Instrument>) {
                self.0.resolve_converted(symbol, conversion, reply);
            }
            fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
                self.0.history(request, reply);
            }
            fn trades(&self, request: TradeRequest, reply: Reply<TradePage>) {
                self.1.fetch_add(1, Relaxed);
                self.0.trades(request, reply);
            }
            fn footprints(&self, request: FootprintRequest, reply: Reply<FootprintPage>) {
                self.2.lock().unwrap().push(request.clone());
                self.0.footprints(request, reply);
            }
            fn depth_history(&self, request: DepthRequest, reply: Reply<DepthPage>) {
                self.0.depth_history(request, reply);
            }
            fn subscribe(&self, subscription: Subscription) {
                self.0.subscribe(subscription);
            }
            fn unsubscribe(&self, id: SubscriptionId) {
                self.0.unsubscribe(id);
            }
            fn request_depth_snapshot(&self, id: SubscriptionId) {
                self.0.request_depth_snapshot(id);
            }
        }

        let ctx = egui::Context::default();
        let now = "2026-10-23T19:07:05Z".parse().unwrap();
        // Two hours back from the hour: the first execution the stream delivers.
        let streamed_from: chrono::DateTime<chrono::Utc> = "2026-10-23T17:00:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        let feed = Arc::new(Counted(
            app.live.clone(),
            AtomicUsize::new(0),
            Mutex::default(),
        ));
        app.chart.set_feed(feed.clone());
        app.open_catalog_symbol(live::PREPARED_FOOTPRINT_SYMBOL);
        settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        let frame = |app: &mut DemoApp| {
            ui_frame(&ctx, size, vec![], |ui| {
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest())
            })
        };
        let rows = |texts: &[(String, egui::Rect)]| {
            texts
                .iter()
                .any(|(text, rect)| text == "Total" && rect.max.x < 1080.0)
        };
        // Eight bars at the live edge, 19:00 to 19:07, when the footprint first asks for rows.
        app.chart.set_visible_bars(8);
        frame(&mut app);
        app.chart.scroll_to_real_time(false);
        frame(&mut app);
        let (first, end) = app.chart.state().visible_range();
        assert_eq!(end - first, 8);
        app.chart.set_chart_type(ChartType::VolumeFootprint);
        settle(&mut app, &ctx);
        settle(&mut app, &ctx);
        let texts = frame(&mut app);
        {
            let requests = feed.2.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].before, None);
            // The bars on view and one view of prefetch, not the initial page of 10,080.
            assert_eq!(requests[0].count, 16);
        }
        assert!(rows(&texts), "footprint rows on view");
        // The first page holds the newest 16 of the minutes from 17:00, so it starts at 18:51.
        let first_page: chrono::DateTime<chrono::Utc> = "2026-10-23T18:51:00Z".parse().unwrap();
        app.chart
            .scroll_to_time("2026-10-23T18:30:00Z".parse().unwrap());
        frame(&mut app);
        frame(&mut app);
        let texts = frame(&mut app);
        let first = app.chart.state().visible_range().0;
        assert!(app.chart.bars(SeriesId::MAIN)[first].time < first_page);
        assert!(rows(&texts), "footprint rows before the first page");
        {
            let requests = feed.2.lock().unwrap();
            assert_eq!(requests.get(1).map(|r| r.before), Some(Some(first_page)));
            assert!(requests[1..].iter().all(|r| r.before.is_some()));
        }
        // Rows of the minutes before the stream can only come from prepared pages.
        let shown = now - chrono::TimeDelta::minutes(150);
        app.chart.scroll_to_time(shown);
        frame(&mut app);
        frame(&mut app);
        let texts = frame(&mut app);
        let first = app.chart.state().visible_range().0;
        assert!(app.chart.bars(SeriesId::MAIN)[first].time < streamed_from);
        assert!(rows(&texts), "footprint rows before the stream");
        assert_eq!(feed.1.load(Relaxed), 0);

        app.execute_search_command(&ctx, "tape");
        settle(&mut app, &ctx);
        settle(&mut app, &ctx);
        let texts = frame(&mut app);
        assert_eq!(feed.1.load(Relaxed), 1);
        let times: Vec<_> = texts
            .iter()
            .filter(|(text, rect)| {
                rect.min.x > 1080.0 && text.len() == 8 && text.as_bytes()[2] == b':'
            })
            .map(|(text, _)| text.as_str())
            .collect();
        assert!(!times.is_empty(), "time and sales rows");
        assert!(times.iter().all(|time| *time < "19:07:05"), "{times:?}");
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(feed.1.load(Relaxed), 1);
    }

    /// Bars outside regular hours are shaded only in the extended session, and a saved layout
    /// returns in the session it was saved in. A layout saved before the session was recorded
    /// returns its extended hours choice as the 24/7 session.
    #[test]
    fn saved_layouts_keep_their_session_and_only_the_extended_one_shades() {
        use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
        use egui_charts::widget::ChartLayoutMode;
        let shading = egui::Color32::from_rgb(201, 37, 180);
        // 07:00 in New York on a Tuesday: outside regular hours.
        let now = "2026-11-03T12:00:00Z".parse().unwrap();
        for (saved, switched, legacy, opened) in [
            (
                SessionType::Full24x7,
                SessionType::RTH,
                false,
                SessionType::Full24x7,
            ),
            (
                SessionType::Full24x7,
                SessionType::ETH,
                false,
                SessionType::Full24x7,
            ),
            (SessionType::RTH, SessionType::ETH, false, SessionType::RTH),
            (
                SessionType::ETH,
                SessionType::Full24x7,
                false,
                SessionType::ETH,
            ),
            (
                SessionType::Full24x7,
                SessionType::RTH,
                true,
                SessionType::Full24x7,
            ),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                now,
                true,
            );
            let shaded = |app: &mut DemoApp| {
                settle(app, &ctx);
                // This fixture detects the shared fill, independent of fresh phase defaults.
                let config = app.chart.config_mut();
                config.extended_hours_color = shading;
                config.premarket_color = None;
                config.postmarket_color = None;
                for extra in &mut app.extra_charts {
                    let config = extra.chart.config_mut();
                    config.extended_hours_color = shading;
                    config.premarket_color = None;
                    config.postmarket_color = None;
                }
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| eframe::App::ui(&mut *app, ui, &mut eframe::Frame::_new_kittest()),
                );
                let rects = output
                    .shapes
                    .iter()
                    .filter(|shape| {
                        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == shading)
                    })
                    .count();
                output.drop_without_applying_deltas();
                rects > 0
            };
            app.timeframe_toolbar.set_session(saved);
            app.apply_session();
            app.ensure_chart_slots(ChartLayoutMode::TwoHorizontal);
            assert_eq!(
                shaded(&mut app),
                saved == SessionType::ETH,
                "saved {saved:?}, switched to {switched:?}"
            );

            app.handle_layout_action(LayoutAction::Save);
            app.poll_storage(&ctx);
            let name = app.layout_tabs[app.layout_tab].name.clone();
            if legacy {
                let groups = app.storage.layout_groups(&name).unwrap().unwrap();
                let mut groups = serde_json::to_value(&groups).unwrap();
                groups.as_object_mut().unwrap().remove("continuous");
                let groups = serde_json::from_value(groups).unwrap();
                app.storage.save_layout_groups(&name, &groups).unwrap();
            }
            app.timeframe_toolbar.set_session(switched);
            app.apply_session();
            app.handle_layout_action(LayoutAction::Open(name));
            app.poll_storage(&ctx);
            assert_eq!(
                shaded(&mut app),
                opened == SessionType::ETH,
                "saved {saved:?}, switched to {switched:?}, legacy {legacy}"
            );
            assert_eq!(
                app.timeframe_toolbar.state.session_type, opened,
                "saved {saved:?}, switched to {switched:?}, legacy {legacy}"
            );
            assert_eq!(
                app.live.view(app.chart.extended_hours()).0,
                opened,
                "saved {saved:?}, switched to {switched:?}, legacy {legacy}"
            );
            let extra = &app.extra_charts[0];
            assert_eq!(
                extra.live.view(extra.chart.extended_hours()).0,
                opened,
                "saved {saved:?}, switched to {switched:?}, legacy {legacy}"
            );
        }
    }

    /// The dialog gets Escape before the chart or replay transport.
    #[test]
    fn dialogs_keep_drawing_and_replay_keys_with_their_owner() {
        use eframe::App;
        use egui_charts::drawings::{Drawing, DrawingToolType};
        for replay in [false, true] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            if replay {
                app.start_replay();
                app.replay
                    .as_mut()
                    .unwrap()
                    .handle_action(ReplayAction::Pause);
            }
            app.chart
                .drawings_mut()
                .drawings
                .push(Drawing::new(7, DrawingToolType::TrendLine));
            app.chart.drawings_mut().select(7);
            app.chart
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::TrendLine));
            app.keyboard_shortcuts.open();
            let before = app.replay.as_ref().map(|replay| replay.state.curr_bar);
            for pass in 0..4 {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events: if pass >= 2 {
                            vec![egui::Event::Key {
                                key: if pass == 2 {
                                    egui::Key::Escape
                                } else {
                                    egui::Key::ArrowRight
                                },
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            }]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                )
                .drop_without_applying_deltas();
            }
            assert!(!app.keyboard_shortcuts.is_open);
            assert_eq!(app.chart.drawings().sel_drawing(), Some(7));
            assert_eq!(
                app.chart.drawings().active_tool(),
                Some(DrawingToolType::TrendLine)
            );
            assert_eq!(app.replay.is_some(), replay);
            assert_eq!(app.replay_control.is_visible(), replay);
            if let Some(before) = before {
                assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, before + 1);
            }
        }
    }

    /// The dialog's CSV choices reach the file the demo writes.
    #[test]
    fn csv_export_follows_the_dialog_data_options() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let path = std::env::temp_dir().join(format!(
            "egui-charts-export-test-{}.csv",
            std::process::id()
        ));
        let mut data = egui_charts::widget::DataExportOptions::default();
        data.include_main_series = false;
        app.run_export(
            &ctx,
            egui_charts::ui::export::ExportConfig {
                format: ExportFormat::Csv,
                data,
                ..Default::default()
            },
            Some(path.clone()),
        );
        let csv = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let header = csv.lines().next().unwrap();
        assert!(header.starts_with("time"), "{header}");
        assert!(!header.contains("open"), "{header}");
    }

    #[test]
    fn png_exports_include_the_crosshair_without_a_renderer() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let bars: Vec<_> = (0..100)
            .map(|index| {
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(index),
                    100.0,
                    110.0,
                    90.0,
                    105.0,
                    1000.0,
                )
            })
            .collect();
        let time = bars[50].time;
        *app.chart = Chart::new(BarData::from_bars(bars));
        app.chart.set_crosshair_position(Some((time, 103.0)));
        for _ in 0..2 {
            ctx.run_ui(Default::default(), |ui| {
                app.chart.show(ui);
            })
            .drop_without_applying_deltas();
        }
        let path = std::env::temp_dir().join(format!(
            "egui-charts-export-test-{}.png",
            std::process::id()
        ));
        let mut images = Vec::new();
        for include_crosshair in [false, true] {
            app.run_export(
                &ctx,
                egui_charts::ui::export::ExportConfig {
                    image: {
                        let mut options = egui_charts::widget::ImageExportOptions::default();
                        options.size = [200, 120];
                        options.include_timestamp = false;
                        options.include_crosshair = include_crosshair;
                        options
                    },
                    ..Default::default()
                },
                Some(path.clone()),
            );
            app.handle_screenshot(&ctx);
            let png = std::fs::read(&path).unwrap();
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            let image = image::load_from_memory(&png).unwrap();
            assert_eq!((image.width(), image.height()), (200, 120));
            images.push(png);
        }
        std::fs::remove_file(path).unwrap();
        assert_ne!(images[0], images[1]);
        for action in [TopToolbarAction::CopyImage, TopToolbarAction::OpenImage] {
            let copy = action == TopToolbarAction::CopyImage;
            app.handle_top_toolbar(&ctx, action);
            let (target, options, _) = app.screenshot_requested.as_mut().unwrap();
            assert_eq!(matches!(target, ScreenshotTarget::Clipboard), copy);
            options.size = [200, 120];
            let output = ctx.run_ui(Default::default(), |ui| app.handle_screenshot(ui.ctx()));
            assert!(
                output
                    .platform_output
                    .commands
                    .iter()
                    .any(|command| match command {
                        egui::OutputCommand::CopyImage(image) => copy && image.size == [200, 120],
                        egui::OutputCommand::OpenUrl(url) =>
                            !copy
                                && url.new_tab
                                && std::path::Path::new(&url.url).is_absolute()
                                && url.url.ends_with(".png"),
                        _ => false,
                    })
            );
            for command in &output.platform_output.commands {
                if let egui::OutputCommand::OpenUrl(url) = command {
                    std::fs::remove_file(&url.url).unwrap();
                }
            }
            output.drop_without_applying_deltas();
        }

        // Every download of a symbol is kept: the default name carries the capture time.
        use chrono::Timelike;
        fn paths_time(path: &std::path::Path) -> chrono::NaiveDateTime {
            let name = path.file_name().unwrap().to_str().unwrap();
            let stamp = name
                .strip_prefix("BTC_USD_")
                .and_then(|name| name.strip_suffix(".png"));
            chrono::NaiveDateTime::parse_from_str(stamp.unwrap(), "%Y-%m-%d_%H-%M-%S").unwrap()
        }
        let now = || chrono::Local::now().naive_local();
        app.state.symbol = "BTC/USD".into();
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        for _ in 0..2 {
            // The second download is a second after the first.
            while paths.len() == 1 && now().with_nanosecond(0).unwrap() <= paths_time(&paths[0]) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let before = now().with_nanosecond(0).unwrap();
            app.handle_top_toolbar(&ctx, TopToolbarAction::TakeScreenshot);
            app.screenshot_requested.as_mut().unwrap().1.size = [200, 120];
            app.handle_screenshot(&ctx);
            let after = now();
            let message = &app.toasts.toasts().last().unwrap().message;
            let path = std::path::PathBuf::from(
                message
                    .strip_prefix("Export successful: ")
                    .unwrap_or_else(|| panic!("{message}")),
            );
            assert_eq!(path.parent(), Some(std::env::temp_dir().as_path()));
            assert!(
                (before..=after).contains(&paths_time(&path)),
                "{}",
                path.display()
            );
            assert!(std::fs::read(&path).unwrap().starts_with(b"\x89PNG"));
            paths.push(path);
        }
        assert_ne!(paths[0], paths[1]);
        for path in paths {
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn saved_layouts_open_again_and_after_a_restart() {
        use eframe::App;
        use egui_charts::model::ChartType;
        use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        ctx.run_ui(Default::default(), |ui| {
            app.chart.show(ui);
        })
        .drop_without_applying_deltas();
        app.user_action(|app| {
            app.chart
                .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
        });
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Line));
        app.handle_comparison(CompareAction::AddSymbol(
            "STOCKA".into(),
            Default::default(),
        ));
        app.add_series_pane();
        let rsi = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::RSI::new(14)));
        let pane = app.chart.series_pane(rsi).unwrap();
        app.chart.move_pane(pane, 0);
        let id = app.comparisons[0].1;
        let mut options = app
            .chart
            .series()
            .find(|(key, _)| *key == id)
            .unwrap()
            .1
            .clone();
        options.width = 4.0;
        options.color = egui::Color32::YELLOW;
        options.scale = egui_charts::model::SeriesScale::Separate(7);
        app.chart.set_series_options(id, options);
        app.chart.set_series_visible(id, false);
        app.chart
            .series_price_scale_mut(id)
            .unwrap()
            .set_range(100.0, 200.0);
        app.add_spread("STOCKA-STOCKB");
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.layout.set_sync_group(0, Some("A".into()));
        app.layout.set_sync_group(1, Some("B".into()));
        app.layout.set_detached(1, true);
        app.layout.sync.sync_symbol = true;
        app.layout.sync.sync_timeframe = true;
        app.layout.sync.sync_time_axis = false;
        app.top_toolbar.chart_grid_selector.sync = app.layout.sync.clone();
        for _ in 0..3 {
            ctx.run_ui(Default::default(), |ui| {
                app.chart.show(ui);
                for extra in &mut app.extra_charts {
                    extra.chart.show(ui);
                }
            })
            .drop_without_applying_deltas();
        }
        app.chart
            .set_crosshair_style(egui_charts::config::CrosshairStyle::Dot);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Save);
        assert!(!app.export_dialog.is_open());
        app.poll_storage(&ctx);
        assert_eq!(app.top_toolbar.layout_menu.layouts, ["Unnamed"]);
        assert_eq!(app.layout_tabs.len(), 1);
        // Opening a layout keeps the host's alert preferences and applied toast lifetime.
        let mut list_options = app.alerts.options().clone();
        list_options.name = false;
        list_options.source_icon = false;
        list_options.descending = false;
        list_options.sort = egui_charts::ui::widget_bar::AlertSortKey::Fired;
        app.alerts.set_options(list_options.clone());
        let mut settings = (*app.settings_state).clone();
        settings.alerts.notification_seconds = 12;
        settings.alerts.close_toasts = false;
        settings.alerts.desktop_notifications = true;
        settings.alerts.show_active_only = false;
        settings.alerts.sound_volume = 0.35;
        app.apply_chart_settings(settings.clone());
        // Opening the current layout again returns to it as saved.
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Area));
        app.handle_layout_action(LayoutAction::Open("Unnamed".into()));
        app.poll_storage(&ctx);
        assert_eq!(app.settings_state.alerts, settings.alerts);
        let saved = |app: &DemoApp| {
            assert_eq!(app.alerts.options(), &list_options);
            assert_eq!(app.layout.sync_group(0), Some("A"));
            assert_eq!(app.layout.sync_group(1), Some("B"));
            assert!(app.layout.is_detached(1));
            assert!(app.layout.sync.sync_symbol);
            assert!(app.layout.sync.sync_timeframe);
            assert!(!app.layout.sync.sync_time_axis);
            assert_eq!(app.top_toolbar.chart_grid_selector.sync, app.layout.sync);
            assert_eq!(app.chart.chart_type(), ChartType::Line);
            assert_eq!(app.comparisons.len(), 2);
            assert_eq!(
                app.chart.chart_options().crosshair.style,
                egui_charts::config::CrosshairStyle::Dot
            );
            assert_eq!(app.extra_charts.len(), 1);
            assert_eq!(app.extra_charts[0].comparisons.len(), 2);
            assert_eq!(app.spreads.len(), 1);
            assert_eq!(app.extra_charts[0].spreads.len(), 1);
            for chart in [&app.chart, &app.extra_charts[0].chart] {
                let spreads: Vec<_> = chart
                    .series()
                    .filter(|(_, options)| options.name == "STOCKA-STOCKB")
                    .collect();
                assert_eq!(spreads.len(), 1, "restore each spread exactly once");
                let id = spreads[0].0;
                assert!(!chart.bars(id).is_empty());
                assert_ne!(
                    chart.series_pane(id),
                    Some(egui_charts::widget::PaneId::MAIN)
                );
            }
            for (_, id, _) in &app.extra_charts[0].comparisons {
                assert!(!app.extra_charts[0].chart.bars(*id).is_empty());
            }
            let id = app
                .comparisons
                .iter()
                .find(|(symbol, _, _)| symbol == "STOCKA")
                .unwrap()
                .1;
            let options = app.chart.series().find(|(key, _)| *key == id).unwrap().1;
            assert_eq!(options.width, 4.0);
            assert_eq!(options.color, egui::Color32::YELLOW);
            assert_eq!(options.scale, egui_charts::model::SeriesScale::Separate(7));
            assert!(!app.chart.is_series_visible(id));
            assert_eq!(app.chart.series_price_scale(id).unwrap().range().min, 100.0);
            assert_eq!(app.chart.series_price_scale(id).unwrap().range().max, 200.0);
            let pane = app.chart.series_pane(app.comparisons[1].1).unwrap();
            assert_ne!(pane, egui_charts::widget::PaneId::MAIN);
            assert_eq!(app.chart.panes()[1], pane);
            assert_eq!(app.chart.pane_stretch_factor(pane), Some(1.5));
            for (_, id, _) in &app.comparisons {
                assert!(!app.chart.bars(*id).is_empty());
            }
            assert!(app.chart.indicators().any(|(_, study)| {
                study.inputs().first().map(|input| &input.value)
                    == Some(&egui_charts::studies::InputValue::Length(7))
            }));
        };
        settle(&mut app, &ctx);
        saved(&app);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        app.poll_storage(&ctx);
        settle(&mut app, &ctx);
        saved(&app);
        app.handle_layout_action(LayoutAction::Rename("Unnamed".into(), "Renamed".into()));
        app.poll_storage(&ctx);
        settle(&mut app, &ctx);
        saved(&app);
        app.handle_layout_action(LayoutAction::SaveAs("Copy".into()));
        app.poll_storage(&ctx);
        settle(&mut app, &ctx);
        saved(&app);
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Area));
        assert!(app.chart.can_undo());
        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        assert_eq!(app.chart.chart_type(), ChartType::Line);

        // A long catalogue and long names must fit the shortcut's chooser on small screens.
        app.top_toolbar.layout_menu.layouts = (0..40)
            .map(|i| format!("Saved layout {i}: {}", "a long name ".repeat(10)))
            .collect();
        for theme in [Theme::light(), Theme::dark()] {
            egui_charts::theme::apply_to_egui(&ctx, &theme);
            for size in [egui::vec2(320.0, 360.0), egui::vec2(1440.0, 900.0)] {
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                app.open_layout = true;
                for _ in 0..3 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| app.show_open_layout(ui.ctx()),
                    );
                    output.drop_without_applying_deltas();
                }
                let bounds = ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique("open_layout")))
                    .unwrap();
                assert!(
                    screen.contains_rect(bounds),
                    "Open layout exceeds {size:?}: {bounds:?}"
                );
            }
        }
    }

    #[test]
    fn drawing_zoom_changes_the_view_without_editing_drawings() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        app.chart.begin_user_action();
        let dm = app.chart.drawings_mut();
        dm.start_drawing_with_coords(
            egui_charts::drawings::DrawingToolType::TrendLine,
            egui::pos2(1.0, 2.0),
            |x| x,
            f64::from,
        );
        dm.add_point_with_coords(egui::pos2(3.0, 4.0), |x| x, f64::from);
        let drawings: Vec<_> = dm
            .drawings
            .iter()
            .map(|drawing| (drawing.id, drawing.chart_points.clone()))
            .collect();
        app.chart.end_user_action();
        app.drawing_toolbar.state.zoom_mode_active = true;
        app.handle_drawing_toolbar(DrawingToolbarAction::ZoomIn);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.chart.show(ui);
        })
        .drop_without_applying_deltas();
        let before = app.chart.get_time_scale_state();
        let rect = app.chart.get_rendered_price_rect();
        assert!(app.chart.execute_box_zoom(
            rect.min + rect.size() * 0.2,
            rect.max - rect.size() * 0.2,
            rect,
            rect.width()
        ));
        for restore in [false, true] {
            if restore {
                app.handle_drawing_toolbar(DrawingToolbarAction::ZoomOut);
                assert_eq!(app.chart.get_time_scale_state(), before);
            } else {
                assert!(app.chart.get_time_scale_state().0 > before.0);
            }
            assert_eq!(
                app.chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|drawing| (drawing.id, drawing.chart_points.clone()))
                    .collect::<Vec<_>>(),
                drawings
            );
            assert!(app.chart.can_undo());
        }
    }

    #[test]
    fn comparison_dialog_adds_hides_and_removes_chart_series() {
        use egui_charts::model::SeriesScale;
        use egui_charts::ui::top_toolbar::ComparePlacement;
        for (placement, main_scale, add_label) in [
            (
                ComparePlacement::SamePercentScale,
                SeriesScale::Right,
                "STOCKA",
            ),
            (
                ComparePlacement::NewPriceScale,
                SeriesScale::Separate(1),
                "New price scale",
            ),
            (ComparePlacement::NewPane, SeriesScale::Right, "New pane"),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            finish_feed(&app.live);
            app.apply_session();
            app.live.set_paused(true);
            app.chart.set_feed_paused(true);
            app.chart.set_series_scale(SeriesId::MAIN, main_scale);
            let main_symbol = app.chart.symbol().to_owned();
            let main_bars = app.chart.bars(SeriesId::MAIN).to_vec();
            app.handle_top_toolbar(&ctx, TopToolbarAction::OpenCompare);
            for (label, count, visible) in [
                ("STOCKA", 1, true),
                ("Hide", 1, false),
                ("Show", 1, true),
                ("X", 0, false),
            ] {
                let mut frame = |events, target| {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            events,
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1440.0, 900.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.show_dialogs(ui.ctx()),
                    );
                    let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == target => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                    pos
                };
                frame(vec![], label);
                let mut pos = frame(vec![], label)
                    .unwrap_or_else(|| panic!("comparison action {label} is visible"));
                frame(vec![egui::Event::PointerMoved(pos)], label);
                if label == "STOCKA" {
                    pos = frame(vec![], add_label).expect("placement choice is visible");
                    frame(vec![egui::Event::PointerMoved(pos)], add_label);
                }
                for pressed in [true, false] {
                    frame(
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                        label,
                    );
                }
                for (_, id, feed) in &app.comparisons {
                    finish_feed(feed);
                    comparison::refresh(&mut app.chart, *id, feed);
                }
                let comparisons: Vec<_> = app
                    .chart
                    .series()
                    .filter(|(_, series)| series.name == "STOCKA")
                    .collect();
                assert_eq!(
                    comparisons.len(),
                    count,
                    "{label} must change actual chart membership"
                );
                if let Some((id, _)) = comparisons.first() {
                    assert_eq!(app.chart.is_series_visible(*id), visible);
                    assert!(!app.chart.bars(*id).is_empty());
                    match placement {
                        ComparePlacement::SamePercentScale => {
                            assert_eq!(app.chart.series_scale(*id), Some(main_scale));
                            assert_eq!(
                                app.chart
                                    .series_price_scale(SeriesId::MAIN)
                                    .unwrap()
                                    .options()
                                    .mode,
                                egui_charts::scales::PriceScaleMode::Percentage
                            );
                        }
                        ComparePlacement::NewPriceScale => {
                            assert_ne!(app.chart.series_scale(*id), Some(main_scale));
                            assert_eq!(
                                app.chart.series_pane(*id),
                                Some(egui_charts::widget::PaneId::MAIN)
                            );
                        }
                        ComparePlacement::NewPane => assert_ne!(
                            app.chart.series_pane(*id),
                            Some(egui_charts::widget::PaneId::MAIN)
                        ),
                        _ => unreachable!(),
                    }
                }
                if label == "STOCKA" {
                    let colour = comparisons[0].1.color;
                    let mut painted = false;
                    for _ in 0..3 {
                        let output = ctx.run_ui(Default::default(), |ui| {
                            app.show_chart(ui, app.theme.chart.bg)
                        });
                        painted |= output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(colour)));
                        output.drop_without_applying_deltas();
                    }
                    assert!(painted, "comparison prices must be painted");
                    let template = app.chart.study_template_set();
                    assert_eq!(
                        template
                            .studies
                            .iter()
                            .map(|study| study.name.as_str())
                            .collect::<Vec<_>>(),
                        ["Volume"]
                    );
                    app.load_study_template(&template);
                    assert!(
                        app.chart
                            .series()
                            .any(|(_, series)| series.name == "STOCKA")
                    );
                }
                assert_eq!(app.compare_dialog.compared_symbols.len(), count);
                assert_eq!(app.chart.symbol(), main_symbol);
                assert!(
                    app.chart.bars(SeriesId::MAIN) == main_bars,
                    "comparison actions must preserve the main data"
                );
            }
        }
    }

    #[test]
    fn display_settings_apply_styles_and_read_back() {
        use egui_charts::config::BackgroundStyle;
        use egui_charts::widget::settings::{BackgroundType, PrecisionMode, WatermarkMode};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        app.chart.set_size(Some(egui::vec2(800.0, 600.0)));
        let mut state = (*app.settings_state).clone();
        state.candle_colors = serde_json::from_str("{}").unwrap();
        state.chart_basic_styles.background_type = BackgroundType::VerticalGradient;
        state.chart_basic_styles.background_gradient_top = egui::Color32::RED;
        state.chart_basic_styles.background_gradient_bottom = egui::Color32::BLUE;
        state.crosshair.line_width = 3.0;
        state.crosshair.line_style = LineStyle::Solid;
        state.crosshair.color = egui::Color32::GREEN;
        state.precision = PrecisionMode::Decimals5;
        state.watermark.mode = WatermarkMode::Symbol;
        state.grid_lines.horizontal_color = egui::Color32::RED;
        state.grid_lines.vertical_color = egui::Color32::BLUE;
        state.margins.top_percent = 12.5;
        state.margins.bottom_percent = 7.5;
        state.margins.right_bars = 17.0;
        // The demo keeps its margin in pixels, which an edit in bars replaces.
        state.margins.right_pixels = None;
        state.trading.show_positions = false;
        state.trading.show_orders = false;
        state.trading.show_executions = false;
        state.trading.show_execution_labels = false;
        app.apply_chart_settings(state.clone());
        assert_eq!(app.trading.display, state.trading);
        assert_eq!(
            app.chart.config().candle_border_width,
            0.0,
            "older settings retain borderless candles"
        );
        assert_eq!(
            app.chart.config().background_style,
            BackgroundStyle::VerticalGradient {
                top_color: egui::Color32::RED,
                bottom_color: egui::Color32::BLUE,
            }
        );
        assert_eq!(
            app.chart.chart_options().crosshair.vert_line_style,
            LineStyle::Solid
        );
        assert_eq!(
            app.chart.chart_options().crosshair.horz_line_style,
            LineStyle::Solid
        );
        assert_eq!(app.chart.chart_options().crosshair.horz_line_width, 3.0);
        assert_eq!(
            app.chart.chart_options().crosshair.vert_line_color,
            egui::Color32::GREEN
        );
        assert!(app.chart.config().show_watermark);
        let scale = app.chart.state().price_scale();
        assert_eq!(
            scale.map(Default::default(), 0.0, 1.0).price_precision(),
            Some(5)
        );
        assert_eq!(scale.options().scale_margins.top, 0.125);
        assert_eq!(scale.options().scale_margins.bottom, 0.075);
        state.alerts.show_alerts = false;
        state.alerts.show_alert_labels = false;
        state.alerts.alert_color = egui::Color32::YELLOW;
        app.settings_state.alerts = state.alerts.clone();
        app.open_chart_settings_dialog();
        assert_eq!(app.settings_state.alerts, state.alerts);
        assert_eq!(app.settings_dialog.settings().alerts, state.alerts);
        assert_eq!(app.settings_state.trading, state.trading);
        assert_eq!(app.settings_dialog.settings().trading, state.trading);
        assert_eq!(app.settings_state.grid_lines, state.grid_lines);
        assert_eq!(app.settings_state.crosshair, state.crosshair);
        assert_eq!(app.settings_state.precision, state.precision);
        assert_eq!(app.settings_state.margins, state.margins);
        assert_eq!(app.settings_state.watermark, state.watermark);
        let mut next = app.chart.bars(SeriesId::MAIN).last().unwrap().clone();
        next.time += chrono::TimeDelta::minutes(1);
        app.chart.update(SeriesId::MAIN, next);
        assert_eq!(
            app.chart.get_time_scale_state().1,
            17.0,
            "live bars retain the chosen right margin"
        );
        let before = app.settings_state.clone();
        for (timezone, top, bottom, width) in [
            ("Unknown", 10.0, 10.0, 1.0),
            ("UTC", f32::NAN, 10.0, 1.0),
            ("UTC", 101.0, 0.0, 1.0),
            ("UTC", -1.0, 10.0, 1.0),
            ("UTC", 10.0, 10.0, 0.0),
        ] {
            let mut invalid = state.clone();
            invalid.timezone = timezone.into();
            invalid.margins.top_percent = top;
            invalid.margins.bottom_percent = bottom;
            invalid.crosshair.line_width = width;
            invalid.candle_colors.body_up = egui::Color32::YELLOW;
            apply_settings_state(&invalid, &mut app.chart, &mut app.timeframe_toolbar);
            app.open_chart_settings_dialog();
            assert_eq!(app.settings_state, before);
        }
        app.notifications.poll(&ctx);
        let initial = app.notifications.unread();
        let mut received = initial;
        for enabled in [false, true, true, false] {
            let mut state = (*app.settings_state).clone();
            state.events.show_news = false;
            state.events.show_news_notifications = enabled;
            app.apply_chart_settings(state);
            app.chart.reload_marks();
            for _ in 0..4 {
                app.chart.fit_content();
                ctx.run_ui(Default::default(), |ui| {
                    app.chart.show(ui);
                })
                .drop_without_applying_deltas();
                app.notifications.poll(&ctx);
            }
            let unread = app.notifications.unread();
            if enabled && received == initial {
                assert!(
                    unread > initial,
                    "hidden news markers still allow notifications"
                );
            } else {
                assert_eq!(unread, received, "quiet or repeated news must not notify");
            }
            received = unread;
        }
    }

    #[test]
    fn alt_a_creates_a_host_alert_at_the_hovered_price_and_dragging_submits_its_new_level() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        // A one-shot alert must stay active until the drag, regardless of the simulation clock.
        app.chart.set_feed_paused(true);
        let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        app.chart.set_data(
            SeriesId::MAIN,
            (0..2)
                .map(|i| {
                    Bar::new(
                        start + chrono::TimeDelta::minutes(i),
                        100.0,
                        110.0,
                        90.0,
                        105.0,
                        1000.0,
                    )
                })
                .collect(),
        );
        let frame = |app: &mut DemoApp, events: Vec<egui::Event>, covered: bool| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_chart(ui, egui::Color32::BLACK);
                    if covered {
                        egui::Window::new("Settings overlay")
                            .fixed_pos(
                                app.chart.get_chart_mapping().rect.center()
                                    - egui::vec2(50.0, 50.0),
                            )
                            .show(ui.ctx(), |ui| {
                                ui.label("Settings");
                            });
                    }
                },
            )
            .drop_without_applying_deltas();
            app.alerts.poll(&ctx, &mut app.toasts);
        };
        frame(&mut app, vec![], false);
        frame(&mut app, vec![], false);
        let map = app.chart.get_chart_mapping();
        let point = egui::pos2(map.rect.center().x, map.rect.center().y);
        let range = map.price.range();
        let expected = app.chart.alert_price_at(point).unwrap();
        let tick = app
            .chart
            .instrument()
            .and_then(|instrument| instrument.tick_size)
            .unwrap_or(0.01);
        assert!((expected - (range.min + range.max) / 2.0).abs() <= tick / 2.0 + 1e-9);
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                },
            ],
            false,
        );
        assert_eq!(app.alerts.alerts().len(), 1);
        assert_eq!(
            app.alerts.alerts()[0].condition,
            AlertCondition::Crossing(expected)
        );
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert_eq!(
            app.alerts.alerts()[0].status,
            egui_charts::model::AlertStatus::Active
        );
        let target = point + egui::vec2(0.0, 45.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let scale = app.chart.get_time_scale_state();
        frame(&mut app, vec![button(point, true)], false);
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(point + egui::vec2(0.0, 5.0))],
            false,
        );
        frame(&mut app, vec![egui::Event::PointerMoved(target)], false);
        frame(&mut app, vec![button(target, false)], false);
        let AlertCondition::Crossing(level) = app.alerts.alerts()[0].condition else {
            panic!("expected crossing");
        };
        let expected_drag =
            expected - 45.0 / f64::from(map.rect.height()) * (range.max - range.min);
        assert!(
            (level - expected_drag).abs() < 1e-6,
            "{level} expected {expected_drag}"
        );
        assert_eq!(
            app.chart.get_time_scale_state(),
            scale,
            "dragging an alert must not pan the chart"
        );
        frame(&mut app, vec![egui::Event::PointerMoved(point)], true);
        frame(&mut app, vec![], true);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            true,
        );
        assert_eq!(
            app.alerts.alerts().len(),
            1,
            "a covering window owns the hovered point"
        );
        assert!(!app.alert_dialog.is_open);
        use egui_charts::config::KeyboardAction;
        app.chart
            .shortcuts_mut()
            .bindings
            .retain(|(_, action)| *action != KeyboardAction::CreateAlert);
        app.chart.shortcuts_mut().bindings.insert(
            0,
            (
                egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::B),
                KeyboardAction::CreateAlert,
            ),
        );
        frame(&mut app, vec![], false);
        frame(&mut app, vec![], false);
        for (key, expected) in [(egui::Key::A, 1), (egui::Key::B, 2)] {
            frame(
                &mut app,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                }],
                false,
            );
            assert_eq!(
                app.alerts.alerts().len(),
                expected,
                "alert creation follows the chart map"
            );
        }
    }

    /// External chart modes and completed drawings are reflected before the toolbars are painted.
    #[test]
    fn toolbars_follow_chart_modes_and_completed_tools() {
        use egui_charts::config::CrosshairStyle;
        use egui_charts::drawings::DrawingToolType;
        use egui_charts::ui::drawing_toolbar::{MagnetType, categories::cursors::CursorType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let frame = |app: &mut DemoApp| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.show_top_toolbar(ui, egui::Color32::BLACK);
                app.show_drawing_toolbar(ui, egui::Color32::BLACK);
            })
            .drop_without_applying_deltas();
        };
        assert_eq!(
            app.chart.chart_options().crosshair.style,
            CrosshairStyle::Full
        );
        frame(&mut app);
        assert_eq!(
            app.drawing_toolbar.state.current_cursor_type,
            CursorType::Cross
        );
        for (style, cursor, magnet) in [
            (CrosshairStyle::Full, CursorType::Cross, MagnetType::Weak),
            (CrosshairStyle::Dot, CursorType::Dot, MagnetType::Strong),
            (CrosshairStyle::Arrow, CursorType::Arrow, MagnetType::Weak),
            (
                CrosshairStyle::Demonstration,
                CursorType::Demonstration,
                MagnetType::Strong,
            ),
            (CrosshairStyle::Magic, CursorType::Magic, MagnetType::Strong),
        ] {
            app.chart.set_crosshair_style(style);
            let dm = app.chart.drawings_mut();
            dm.options.magnet_mode = true;
            dm.options.strong_magnet = magnet != MagnetType::Weak;
            dm.set_active_tool(Some(DrawingToolType::TrendLine));
            frame(&mut app);
            assert_eq!(
                app.drawing_toolbar.state.sel_tool,
                Some(DrawingToolType::TrendLine)
            );
            assert_eq!(app.drawing_toolbar.state.current_cursor_type, cursor);
            assert!(app.drawing_toolbar.state.magnet_mode);
            assert_eq!(app.drawing_toolbar.state.magnet_type, magnet);
            let dm = app.chart.drawings_mut();
            dm.start_drawing_with_coords(
                DrawingToolType::TrendLine,
                egui::pos2(1.0, 2.0),
                |x| x,
                f64::from,
            );
            dm.add_point_with_coords(egui::pos2(3.0, 4.0), |x| x, f64::from);
            assert!(dm.active_tool().is_none());
            frame(&mut app);
            assert!(app.drawing_toolbar.state.sel_tool.is_none());
            app.chart.drawings_mut().set_eraser_mode(true);
            frame(&mut app);
            assert!(app.drawing_toolbar.state.eraser_mode_enabled);
            app.chart.drawings_mut().set_eraser_mode(false);
            app.chart.drawings_mut().options.magnet_mode = false;
            frame(&mut app);
            assert!(!app.drawing_toolbar.state.eraser_mode_enabled);
            assert!(!app.drawing_toolbar.state.magnet_mode);
        }
        app.start_replay();
        assert_eq!(
            app.replay.as_ref().unwrap().state.curr_bar,
            0,
            "replay starts at the first available bar"
        );
        assert!(
            !app.replay.as_ref().unwrap().state.is_playing(),
            "start selection begins paused"
        );
        frame(&mut app);
        assert!(app.top_toolbar.state.replay_active);
        app.stop_replay();
        frame(&mut app);
        assert!(!app.top_toolbar.state.replay_active);
    }

    /// A narrow first layout and later resizes retain the configured zoom and live edge.
    #[test]
    fn opening_view_keeps_default_spacing_across_resizes() {
        for theme in [Theme::light(), Theme::dark()] {
            for scale in [1.0, 2.0] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                app.live.set_paused(true);
                // Keep label widths independent of the simulation's wall-clock start.
                app.chart.set_feed_paused(true);
                let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
                app.chart.set_data(
                    SeriesId::MAIN,
                    (0..200)
                        .map(|i| {
                            Bar::new(
                                start + chrono::TimeDelta::minutes(i),
                                100.0,
                                110.0,
                                90.0,
                                105.0,
                                1000.0,
                            )
                        })
                        .collect(),
                );
                app.chart.set_theme(&ctx, &theme);
                app.chart.set_instrument(Some(
                    egui_charts::model::Instrument::new("SAMPLE").with_price_decimals(2),
                ));
                for width in [140.0, 0.0, 80.0, 320.0, 1440.0] {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            app.chart.show(ui);
                            if width == 140.0 && ui.ctx().current_pass_index() == 0 {
                                app.chart.set_instrument(Some(
                                    egui_charts::model::Instrument::new("SAMPLE")
                                        .with_price_decimals(6),
                                ));
                                ui.ctx().request_discard("initial axis precision changed");
                            }
                        },
                    )
                    .drop_without_applying_deltas();
                    let time_scale = app.chart.state().time_scale();
                    assert_eq!(
                        time_scale.bar_spacing(),
                        6.1,
                        "width {width}, scale {scale}"
                    );
                    assert!(
                        (time_scale.right_offset() - (65.0 - 6.1 * 0.5 - 1.0) / 6.1).abs() < 1e-5,
                        "width {width}, scale {scale}: offset {}",
                        time_scale.right_offset()
                    );
                }
            }
        }
    }

    /// Family pages fit both themes and narrow windows; coordinate drafts apply as one undoable edit.
    #[test]
    fn drawing_properties_family_inputs_and_coordinates_apply_through_dialog() {
        use chrono::{TimeZone, Utc};
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType as Tool};
        use egui_charts::ui::dialogs::PropertiesTab;

        fn frame(
            app: &mut DemoApp,
            ctx: &egui::Context,
            events: Vec<egui::Event>,
        ) -> Vec<(String, egui::Rect)> {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.show_dialogs(ui.ctx()),
            );
            let texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.job.text.clone(),
                        egui::Rect::from_min_size(text.pos, text.galley.size()),
                    )),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        }
        fn click(app: &mut DemoApp, ctx: &egui::Context, point: egui::Pos2) {
            frame(app, ctx, vec![egui::Event::PointerMoved(point)]);
            frame(
                app,
                ctx,
                [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .to_vec(),
            );
        }
        fn scroll(
            app: &mut DemoApp,
            ctx: &egui::Context,
            point: egui::Pos2,
            delta: f32,
        ) -> Vec<(String, egui::Rect)> {
            let mut texts = frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, delta),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            for _ in 0..8 {
                texts = frame(app, ctx, vec![]);
            }
            texts
        }
        fn enter(app: &mut DemoApp, ctx: &egui::Context) {
            frame(
                app,
                ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            frame(
                app,
                ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }

        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        // Times 0, 60, 180 seconds: 120 is halfway between bars 1 and 2, so index 1.5.
        app.chart.set_data(
            SeriesId::MAIN,
            [0, 60, 180]
                .map(|seconds| {
                    Bar::new(
                        Utc.timestamp_opt(seconds, 0).unwrap(),
                        100.0,
                        120.0,
                        90.0,
                        110.0,
                        1.0,
                    )
                })
                .to_vec(),
        );
        for (tool, heading) in [
            (Tool::FibonacciRetracement, "Fibonacci levels"),
            (Tool::FibonacciExtension, "Fibonacci levels"),
            (Tool::FibonacciChannel, "Fibonacci levels"),
            (Tool::FibonacciWedge, "Fibonacci levels"),
            (Tool::Pitchfork, "Pitchfork levels"),
            (Tool::SchiffPitchfork, "Pitchfork levels"),
            (Tool::ModifiedSchiffPitchfork, "Pitchfork levels"),
            (Tool::InsidePitchfork, "Pitchfork levels"),
            (Tool::GannBox, "Price levels"),
            (Tool::GannSquare, "Price levels"),
            (Tool::GannFixed, "Price levels"),
            (Tool::GannFan, "Fan lines"),
            (Tool::LongPos, "Entry price"),
            (Tool::ShortPos, "Entry price"),
            (Tool::FixedRangeVolumeProfile, "VA values colour"),
            (Tool::AnchoredVolumeProfile, "VA values colour"),
            (Tool::RegressionTrend, "Upper deviation"),
            (Tool::BarsPattern, "Mirrored"),
        ] {
            app.chart.drawings_mut().drawings.clear();
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(0.0, 100.0),
                ChartPoint::new(2.0, 120.0),
                ChartPoint::new(1.0, 95.0),
            ];
            let before = format!("{drawing:?}");
            app.chart.drawings_mut().drawings.push(drawing);
            app.open_drawing_props_for(7);
            assert_eq!(
                app.drawing_props.coordinates.points.len(),
                3,
                "{tool:?}: every anchor must be editable"
            );
            frame(&mut app, &ctx, vec![]);
            let texts = frame(&mut app, &ctx, vec![]);
            let inputs = texts
                .iter()
                .find(|(text, _)| text == "Inputs")
                .unwrap_or_else(|| panic!("{tool:?}: tool-specific Inputs tab: {texts:?}"))
                .1
                .center();
            click(&mut app, &ctx, inputs);
            let mut texts = frame(&mut app, &ctx, vec![]);
            // The profile's value controls start below the dialog's opening view.
            let scroll_at = if matches!(
                tool,
                Tool::FixedRangeVolumeProfile | Tool::AnchoredVolumeProfile
            ) {
                let point = texts
                    .iter()
                    .find(|(text, _)| text == "VAH")
                    .unwrap()
                    .1
                    .center();
                texts = scroll(&mut app, &ctx, point, -160.0);
                Some(point)
            } else {
                None
            };
            assert!(
                texts.iter().any(|(text, _)| text == heading),
                "{tool:?}: {texts:?}"
            );
            if let Some(point) = scroll_at {
                // Keep the next tool's opening view at the top too.
                scroll(&mut app, &ctx, point, 2000.0);
            }
            assert_eq!(
                format!("{:?}", app.chart.drawings().drawings[0]),
                before,
                "visiting a page must not change defaults"
            );
            for (theme, width) in [Theme::light(), Theme::dark()]
                .into_iter()
                .flat_map(|theme| [320.0, 360.0].map(|width| (theme.clone(), width)))
            {
                let narrow = egui::Context::default();
                egui_charts::theme::apply_to_egui(&narrow, &theme);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 640.0));
                let tabs = if tool == Tool::FibonacciRetracement {
                    PropertiesTab::all().to_vec()
                } else {
                    vec![PropertiesTab::Inputs]
                };
                for tab in tabs {
                    app.drawing_props.active_tab = tab;
                    for pass in 0..3 {
                        let mut output = narrow.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            |ui| app.show_dialogs(ui.ctx()),
                        );
                        output.textures_delta.clear();
                        if pass > 0 {
                            let area = narrow
                                .memory(|memory| {
                                    memory.area_rect(egui::Id::unique(Some("Drawing Properties")))
                                })
                                .unwrap();
                            assert!(
                                screen.contains_rect(area),
                                "{tool:?}, {tab:?}: dialog exceeds the window: {area:?}"
                            );
                        }
                        let mut footer = Vec::new();
                        for shape in &output.shapes {
                            if let egui::Shape::Text(text) = &shape.shape
                                && matches!(
                                    text.galley.job.text.as_str(),
                                    "Ok" | "Cancel" | "Template"
                                )
                            {
                                let bounds =
                                    egui::Rect::from_min_size(text.pos, text.galley.size());
                                if matches!(
                                    text.galley.job.text.as_str(),
                                    "Ok" | "Cancel" | "Template"
                                ) {
                                    footer.push(bounds);
                                }
                                assert!(
                                    screen.left() <= bounds.left()
                                        && screen.right() >= bounds.right(),
                                    "{tool:?}: control outside narrow window: {bounds:?}"
                                );
                                assert!(
                                    shape.clip_rect.left() <= bounds.left()
                                        && shape.clip_rect.right() >= bounds.right(),
                                    "{tool:?}: horizontally clipped control: {bounds:?}, clip: {:?}",
                                    shape.clip_rect
                                );
                            }
                        }
                        for (index, bounds) in footer.iter().enumerate() {
                            assert!(
                                footer[index + 1..]
                                    .iter()
                                    .all(|other| !bounds.intersects(*other)),
                                "{tool:?}: overlapping footer controls: {footer:?}"
                            );
                        }
                        output.drop_without_applying_deltas();
                    }
                }
            }
            app.drawing_props.close();
        }
        // The demo opens the library's fixed-square label controls and applies their preview.
        {
            app.chart.drawings_mut().drawings[0].tool_type = Tool::GannFixed;
            app.open_drawing_props_for(7);
            app.drawing_props.active_tab = PropertiesTab::Inputs;
            app.drawing_props.config.height = 2400.0;
            let label_ctx = egui::Context::default();
            let mut label_frame = |events| {
                let output = label_ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 2600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.show_dialogs(ui.ctx()),
                );
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Labels" => {
                            Some(text.visual_bounding_rect().center())
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                labels
            };
            for _ in 0..3 {
                label_frame(vec![]);
            }
            let point = label_frame(vec![])[0];
            label_frame(vec![egui::Event::PointerMoved(point)]);
            label_frame(
                [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .to_vec(),
            );
            assert!(
                app.chart.drawings().drawings[0]
                    .gann
                    .as_ref()
                    .unwrap()
                    .show_labels
            );
            app.drawing_props.close();
            frame(&mut app, &ctx, vec![]);
        }
        app.chart.drawings_mut().drawings[0].tool_type = Tool::TrendLine;
        app.open_drawing_props_for(7);
        app.drawing_props.active_tab = PropertiesTab::Coordinates;
        assert_eq!(app.drawing_props.coordinates.points[1].time, "00:03:00");
        app.drawing_props.coordinates.points[1].price = "123.5".into();
        app.drawing_props.coordinates.points[1].time = "00:02".into();
        app.drawing_props.coordinates.points[0].price = "NaN".into();
        frame(&mut app, &ctx, vec![]);
        enter(&mut app, &ctx);
        assert!(app.drawing_props.is_open, "invalid drafts remain editable");
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[1].price,
            120.0
        );
        app.drawing_props.coordinates.points[0].price = "100".into();
        enter(&mut app, &ctx);
        assert!(
            !app.drawing_props.is_open,
            "{:?}",
            frame(&mut app, &ctx, vec![])
        );
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[1],
            ChartPoint::new(1.5, 123.5)
        );
        assert!(app.chart.undo());
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[1],
            ChartPoint::new(2.0, 120.0)
        );
        assert!(app.chart.redo());
        app.open_drawing_props_for(7);
        app.drawing_props.coordinates.points[1].price = "124.5".into();
        // History can arrive while the draft is open. The timestamp stays at 120 seconds,
        // now halfway between indices 2 and 3; untouched anchors must move with history too.
        app.chart.set_data(
            SeriesId::MAIN,
            [-60, 0, 60, 180]
                .map(|seconds| {
                    Bar::new(
                        Utc.timestamp_opt(seconds, 0).unwrap(),
                        100.0,
                        120.0,
                        90.0,
                        110.0,
                        1.0,
                    )
                })
                .to_vec(),
        );
        frame(&mut app, &ctx, vec![]);
        enter(&mut app, &ctx);
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[1],
            ChartPoint::new(2.5, 124.5)
        );
        assert_eq!(
            app.chart.drawings().drawings[0].chart_points[0].bar_idx,
            1.0
        );
        app.chart.drawings_mut().drawings[0].visible = false;
        app.open_drawing_props_for(7);
        app.drawing_props.props.line_width = 4.0;
        let texts = frame(&mut app, &ctx, vec![]);
        let menu = texts
            .iter()
            .find(|(text, _)| text == "Template")
            .unwrap()
            .1
            .center();
        click(&mut app, &ctx, menu);
        let texts = frame(&mut app, &ctx, vec![]);
        let save = texts
            .iter()
            .find(|(text, _)| text == "Save as default for this tool")
            .unwrap()
            .1
            .center();
        click(&mut app, &ctx, save);
        enter(&mut app, &ctx);
        app.chart.drawings_mut().start_drawing_with_coords(
            Tool::TrendLine,
            egui::pos2(0.0, 100.0),
            |x| x,
            |y| y as f64,
        );
        assert_eq!(
            app.chart.drawings().curr_drawing().unwrap().stroke_width,
            4.0
        );
        assert_eq!(
            app.chart
                .drawings()
                .curr_drawing()
                .unwrap()
                .chart_points
                .len(),
            1,
            "defaults cannot copy anchors"
        );
        assert!(
            app.chart.drawings().curr_drawing().unwrap().visible,
            "a hidden source must not make new drawings invisible"
        );
        assert!(!app.chart.drawings().drawings[0].visible);
        app.chart.drawings_mut().cancel_curr_drawing();
        app.chart.drawings_mut().start_drawing_with_coords(
            Tool::HorizontalLine,
            egui::pos2(0.0, 100.0),
            |x| x,
            |y| y as f64,
        );
        assert_eq!(
            app.chart.drawings().drawings.last().unwrap().stroke_width,
            2.0,
            "defaults belong to one tool"
        );
    }

    /// The demo's drawing dialog offers line ends for curves, double curves and brushes, and
    /// smoothing for highlighters; what is picked there lands on the drawing.
    #[test]
    fn curve_line_ends_and_highlighter_smoothing_apply_through_the_demo_dialog() {
        use egui::accesskit::{Action, ActionData, ActionRequest, TreeId};
        use egui_charts::drawings::{ArrowStyle, ChartPoint, Drawing, DrawingToolType as Tool};

        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.set_feed_paused(true);
        let click = |app: &mut DemoApp, node: &egui::accesskit::Node| {
            let bounds = node.bounds().unwrap();
            let at = egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            );
            ui_click(&ctx, egui::vec2(1440.0, 900.0), at, |ui| {
                app.show_dialogs(ui.ctx());
            });
        };
        let open = |app: &mut DemoApp, tool: Tool| {
            app.chart.drawings_mut().drawings.clear();
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(0.0, 100.0),
                ChartPoint::new(2.0, 120.0),
                ChartPoint::new(1.0, 95.0),
            ];
            app.chart.drawings_mut().drawings.push(drawing);
            app.open_drawing_props_for(7);
            for _ in 0..3 {
                draw(&ctx, app, vec![]);
            }
            let nodes = draw(&ctx, app, vec![]);
            click(app, &control(&nodes, "Style").1);
        };
        for tool in [Tool::Curve, Tool::DoubleCurve, Tool::Brush] {
            open(&mut app, tool);
            let nodes = draw(&ctx, &mut app, vec![]);
            assert_eq!(
                control(&nodes, "Line start").1.value(),
                Some("Plain"),
                "{tool:?}"
            );
            click(&mut app, &control(&nodes, "Line end").1);
            for label in ["Arrow", "Ok"] {
                let nodes = draw(&ctx, &mut app, vec![]);
                click(&mut app, &control(&nodes, label).1);
            }
            let drawing = &app.chart.drawings().drawings[0];
            assert_eq!(drawing.arrow_end_style, ArrowStyle::Arrow, "{tool:?}");
            assert_eq!(drawing.arrow_start_style, ArrowStyle::None, "{tool:?}");
        }

        open(&mut app, Tool::Highlighter);
        let nodes = draw(&ctx, &mut app, vec![]);
        let (smoothing, node) = control(&nodes, "Smoothing");
        assert_eq!(node.numeric_value(), Some(5.0));
        let request = egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: *smoothing,
            data: Some(ActionData::NumericValue(9.0)),
        });
        draw(&ctx, &mut app, vec![request]);
        let nodes = draw(&ctx, &mut app, vec![]);
        click(&mut app, &control(&nodes, "Ok").1);
        assert_eq!(app.chart.drawings().drawings[0].smoothing, 9);
    }

    /// Turning Lock all off preserves each drawing's own lock, including through undo and redo.
    #[test]
    fn lock_all_drawings_returns_each_drawing_to_its_own_lock() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        for (id, bar, price) in [(7, 1.0, 105.0), (8, 3.0, 107.0)] {
            let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
            drawing.chart_points = vec![
                ChartPoint::new(bar, price),
                ChartPoint::new(bar + 1.0, price + 1.0),
            ];
            drawing.completed = true;
            app.chart.drawings_mut().drawings.push(drawing);
        }
        app.user_action(|app| {
            let dm = app.chart.drawings_mut();
            dm.select(7);
            dm.set_selected_locked(true);
        });
        for (command, locked) in [
            ("lock-drawings", true),
            ("undo", false),
            ("redo", true),
            ("lock-drawings", false),
        ] {
            app.execute_search_command(&ctx, command);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.show_drawing_toolbar(ui, egui::Color32::BLACK)
            })
            .drop_without_applying_deltas();
            assert_eq!(app.drawing_toolbar.state.lock_all, locked, "{command}");
        }
        let locks: Vec<_> = app
            .chart
            .drawings()
            .drawings
            .iter()
            .map(|d| (d.id, d.locked))
            .collect();
        assert_eq!(locks, [(7, true), (8, false)]);
    }

    /// A drawing placed while every drawing is locked cannot be dragged, and the button stays on.
    #[test]
    fn lock_all_drawings_holds_a_drawing_placed_while_it_is_on() {
        use egui_charts::drawings::DrawingToolType;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.execute_search_command(&ctx, "lock-drawings");
        let dm = app.chart.drawings_mut();
        let x_to_bar = |x: f32| x / 10.0;
        let y_to_price = |y: f32| 200.0 - f64::from(y);
        dm.start_drawing_with_coords(
            DrawingToolType::TrendLine,
            egui::pos2(10.0, 95.0),
            x_to_bar,
            y_to_price,
        );
        dm.add_point_with_coords(egui::pos2(20.0, 94.0), x_to_bar, y_to_price);
        dm.complete_curr_drawing();
        let placed = dm.drawings.last().unwrap().clone();
        assert!(!placed.locked);
        dm.start_drag_body(placed.id, egui::pos2(15.0, 94.5));
        dm.update_drag_handle(
            egui::pos2(55.0, 54.5),
            x_to_bar,
            y_to_price,
            |price, from, to| price - f64::from(to - from),
        );
        dm.end_drag_handle();
        assert_eq!(
            dm.drawings.last().unwrap().chart_points,
            placed.chart_points
        );
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_drawing_toolbar(ui, egui::Color32::BLACK)
        })
        .drop_without_applying_deltas();
        assert!(app.drawing_toolbar.state.lock_all);
    }

    /// Existing drawing settings survive opening and applying the dialog, and the object tree
    /// leaves the colour of a drawing the user cannot edit as it is.
    #[test]
    fn drawing_controls_round_trip_visibility_fill_fonts_and_locks() {
        use egui_charts::drawings::{Drawing, DrawingToolType, TimeframeVisibility};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let mut drawing = Drawing::new(7, DrawingToolType::TextLabel);
        drawing.line_style = LineStyle::Dotted;
        drawing.fill_color = Some([51, 102, 153, 85]);
        drawing.font_size = 24.0;
        drawing.extend_left = true;
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        drawing.text = Some("Annotation".into());
        drawing.locked = true;
        app.chart.drawings_mut().drawings.push(drawing);
        app.open_drawing_props_for(7);
        let props = app.drawing_props.props.clone();
        assert_eq!(
            props.timeframe_visibility,
            TimeframeVisibility::specific(&["1h"])
        );
        assert!(props.fill_enabled);
        assert_eq!(
            props.fill_color.unwrap().to_srgba_unmultiplied(),
            [51, 102, 153, 85]
        );
        assert_eq!(props.font_size, 24.0);
        assert!(matches!(props.line_style, LineStyle::Dotted));
        assert!(props.extend_left);
        assert!(
            DrawingProps {
                font_size: 40.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut app.chart)
            .is_err()
        );
        assert_eq!(app.chart.drawings().drawings[0].font_size, 24.0);
        app.chart
            .drawings_mut()
            .edit_drawing(7, |d| d.locked = false)
            .unwrap();
        props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
        let mut second = app.chart.drawings().drawings[0].clone();
        second.id = 8;
        app.chart.drawings_mut().drawings.push(second);
        app.chart.drawings_mut().select_drawings([7, 8]);
        let mut visibility = TimeframeVisibility::default();
        visibility.set_range(egui_charts::model::TimeframeUnit::Hour, Some(2..=4));
        let changed = DrawingProps {
            timeframe_visibility: visibility,
            font_size: 28.0,
            fill_enabled: false,
            ..app.drawing_props.props.clone()
        };
        changed
            .apply_to_chart(DrawingId(7), &mut app.chart)
            .unwrap();
        let saved = &app.chart.drawings().drawings[0];
        assert!(saved.timeframe_visibility.is_visible_on("4h"));
        assert!(!saved.timeframe_visibility.is_visible_on("1h"));
        assert_eq!(saved.fill_color, None);
        assert_eq!(saved.font_size, 28.0);
        assert_eq!(app.chart.drawings().drawings[1].font_size, 28.0);
        assert!(app.chart.undo());
        assert!(
            app.chart
                .drawings()
                .drawings
                .iter()
                .all(|d| d.font_size == 24.0)
        );
        assert!(app.chart.redo());
        app.chart.drawings_mut().drawings.retain(|d| d.id == 7);
        app.handle_object_tree(
            ObjectTreeAction::ChangeColor(
                7,
                egui::Color32::from_rgba_unmultiplied(51, 102, 153, 85),
            ),
            &[],
        );
        assert_eq!(app.chart.drawings().drawings[0].color, [51, 102, 153, 85]);
        app.chart.drawings_mut().end_drag_handle();
        app.chart.drawings_mut().drawings[0].user_edit_enabled = false;
        app.handle_object_tree(ObjectTreeAction::ChangeColor(7, egui::Color32::RED), &[]);
        assert_eq!(app.chart.drawings().drawings[0].color, [51, 102, 153, 85]);
        app.chart.drawings_mut().drawings[0].user_edit_enabled = true;
        app.chart.drawings_mut().set_locked(true);
        app.handle_object_tree(ObjectTreeAction::ChangeColor(7, egui::Color32::RED), &[]);
        assert_eq!(app.chart.drawings().drawings[0].color, [51, 102, 153, 85]);
        app.chart.drawings_mut().set_locked(false);
        use egui_charts::data::VolumeProfileConfig;
        use egui_charts::drawings::{
            FibonacciConfig, GannConfig, GannLevel, PitchforkConfig, PositionSize, PositionSizing,
            RegressionTrendConfig,
        };
        for tool in [
            DrawingToolType::FibonacciRetracement,
            DrawingToolType::Pitchfork,
            DrawingToolType::GannBox,
            DrawingToolType::LongPos,
            DrawingToolType::ShortPos,
            DrawingToolType::FixedRangeVolumeProfile,
            DrawingToolType::RegressionTrend,
            DrawingToolType::ParallelChannel,
        ] {
            app.chart.drawings_mut().drawings.clear();
            app.chart
                .drawings_mut()
                .drawings
                .push(Drawing::new(7, tool));
            let mut peer = Drawing::new(8, DrawingToolType::TrendLine);
            peer.chart_points
                .push(egui_charts::drawings::ChartPoint::new(5.0, 75.0));
            app.chart.drawings_mut().drawings.push(peer);
            app.chart.drawings_mut().select_drawings([7, 8]);
            let mut props =
                DrawingProps::from_drawing(&app.chart.drawings().drawings[0], &app.chart);
            let settings = &mut props.tool_settings.as_mut().unwrap().settings;
            settings.fib_config = Some(FibonacciConfig {
                reverse: true,
                ..Default::default()
            });
            settings.pitchfork = Some(PitchforkConfig {
                background_opacity: 75,
                ..Default::default()
            });
            settings.gann = Some(GannConfig {
                price_levels: [0.0, 0.5, 1.0]
                    .map(|v| GannLevel::new(v, [128, 128, 128, 255]))
                    .to_vec(),
                time_levels: [0.25, 0.75]
                    .map(|v| GannLevel::new(v, [128, 128, 128, 255]))
                    .to_vec(),
                ..Default::default()
            });
            settings.position_size = Some(PositionSize::RiskPercent {
                account_size: 10_000.0,
                percent: 1.0,
            });
            settings.position_sizing = PositionSizing::default();
            settings.position_sizing.point_value = 50.0;
            settings.position_sizing.lot_size = 10.0;
            settings.position_sizing.quantity_step = 0.01;
            settings.position_sizing.leverage = 5.0;
            let sizing = settings.position_sizing;
            settings.volume_profile = VolumeProfileConfig::new(48, 0.8);
            settings.regression_trend = RegressionTrendConfig::new(3.0);
            settings.channel_midline = true;
            let expected = format!("{:?}", settings.fib_config);
            props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
            let saved = &app.chart.drawings().drawings[0];
            assert_eq!(format!("{:?}", saved.fib_config), expected);
            assert_eq!(saved.pitchfork.as_ref().unwrap().background_opacity, 75);
            assert_eq!(
                saved
                    .gann
                    .as_ref()
                    .unwrap()
                    .time_levels
                    .iter()
                    .map(|l| l.value)
                    .collect::<Vec<_>>(),
                [0.25, 0.75]
            );
            assert_eq!(
                saved.position_size.unwrap().quantity(100.0, 98.0),
                Some(50.0)
            );
            assert_eq!(saved.volume_profile.unwrap().rows(), 48);
            assert_eq!(saved.position_sizing, sizing);
            assert_eq!(saved.regression_trend.unwrap().upper_deviation(), 3.0);
            assert!(saved.channel_midline);
            assert!(app.chart.drawings().drawings[1].fib_config.is_none());
            assert_eq!(app.chart.drawings().drawings[1].chart_points[0].price, 75.0);
            assert!(app.chart.undo());
            assert_eq!(
                format!("{:?}", app.chart.drawings().drawings[0].fib_config),
                format!("{:?}", Drawing::new(7, tool).fib_config)
            );
            assert!(app.chart.redo());
            app.chart
                .drawings_mut()
                .edit_drawing(7, |d| d.locked = true)
                .unwrap();
            props.font_size = 18.0;
            props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
            assert_eq!(app.chart.drawings().drawings[0].font_size, 14.0);
            assert_eq!(app.chart.drawings().drawings[1].font_size, 18.0);
            // A malformed ratio cannot partially change common style or another selected tool.
            props
                .tool_settings
                .as_mut()
                .unwrap()
                .settings
                .fib_config
                .as_mut()
                .unwrap()
                .levels[0]
                .value = f32::NAN;
            props.line_width = 9.0;
            assert!(props.apply_to_chart(DrawingId(7), &mut app.chart).is_err());
            assert_eq!(
                app.chart.drawings().drawings[0].stroke_width,
                Drawing::new(7, tool).stroke_width
            );
            assert_eq!(
                app.chart.drawings().drawings[1].stroke_width,
                Drawing::new(8, DrawingToolType::TrendLine).stroke_width
            );
        }
    }

    #[test]
    fn zoom_controls_leave_drawing_history_untouched() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.live.set_paused(true);
        app.chart.user_action(|chart| {
            use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
            let mut line = Drawing::new(0, DrawingToolType::TrendLine);
            line.chart_points = vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 101.0)];
            chart.drawings_mut().add_synced_drawing(line).unwrap();
        });
        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        app.drawing_toolbar.state.zoom_mode_active = true;
        app.handle_drawing_toolbar(DrawingToolbarAction::ZoomIn);
        assert!(app.chart.drawings().drawings.is_empty());
        assert!(app.chart.can_redo());
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.chart.show(ui);
        })
        .drop_without_applying_deltas();
        let before = app.chart.get_time_scale_state();
        let rect = app.chart.get_rendered_price_rect();
        assert!(app.chart.execute_box_zoom(
            rect.min + rect.size() * 0.2,
            rect.max - rect.size() * 0.2,
            rect,
            rect.width()
        ));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_drawing_toolbar(ui, egui::Color32::BLACK);
        })
        .drop_without_applying_deltas();
        assert!(app.drawing_toolbar.state.has_zoom_history);
        app.handle_drawing_toolbar(DrawingToolbarAction::ZoomOut);
        assert_eq!(app.chart.get_time_scale_state(), before);
        assert!(!app.chart.state().has_zoom_history());
        assert!(app.chart.drawings().drawings.is_empty());
        assert!(app.chart.can_redo());
    }

    /// A cursor pick ends Zoom in, in the chart as in the toolbar.
    #[test]
    fn cursor_pick_ends_box_zoom_mode() {
        use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        app.drawing_toolbar.state.zoom_mode_active = true;
        app.handle_drawing_toolbar(DrawingToolbarAction::ZoomIn);
        assert!(app.chart.is_zoom_mode_active());
        // What the toolbar does to its own state on a cursor pick.
        app.drawing_toolbar.state.clear_selection();
        app.handle_drawing_toolbar(DrawingToolbarAction::SetCursorType(CursorType::Dot));
        assert!(!app.chart.is_zoom_mode_active());
    }

    /// The Magic cursor picked in the drawing toolbar hides the pointer over the plot for its
    /// wand, and a click there throws a burst of emoji that is gone three seconds later.
    #[test]
    fn magic_cursor_throws_emoji_on_a_plot_click() {
        use eframe::App;
        use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.handle_drawing_toolbar(DrawingToolbarAction::SetCursorType(CursorType::Magic));
        settle(&mut app, &ctx);
        let at = app.chart.get_rendered_price_rect().center();
        assert!(
            app.chart.drawings().hit_test(at).is_none(),
            "a drawing at {at:?}"
        );
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let mut frame = |time: f64, events: Vec<egui::Event>| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
            // The Magic cursor's emoji all lie from U+1F300 on.
            let emoji = output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                        if text.galley.text().chars().next().is_some_and(|c| c >= '\u{1F300}'))
                })
                .count();
            let cursor = output.platform_output.cursor_icon;
            output.drop_without_applying_deltas();
            (emoji, cursor)
        };
        frame(10.0, vec![egui::Event::PointerMoved(at)]);
        assert_eq!(
            frame(10.1, vec![egui::Event::PointerMoved(at)]),
            (0, egui::CursorIcon::None)
        );
        frame(10.2, vec![button(true)]);
        assert_eq!(frame(10.25, vec![button(false)]).0, 40);
        assert_eq!(frame(13.3, vec![]).0, 0);
    }

    #[test]
    fn object_tree_applies_layer_bulk_and_focus_actions() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx));
        for id in 1..=3 {
            let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 200.0)];
            drawing.completed = true;
            app.chart
                .drawings_mut()
                .add_synced_drawing(drawing)
                .unwrap();
        }
        let ids: Vec<_> = app.chart.drawings().drawings.iter().map(|d| d.id).collect();
        app.chart
            .drawings_mut()
            .edit_drawing(ids[2], |drawing| drawing.listed = false)
            .unwrap();
        assert!(
            !app.build_source_items()
                .iter()
                .any(|source| source.id == ids[2])
        );
        app.chart
            .drawings_mut()
            .edit_drawing(ids[2], |drawing| drawing.listed = true)
            .unwrap();
        let sources = app.build_source_items();
        assert_eq!(
            sources
                .iter()
                .map(|s| s.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            sources.len()
        );
        app.handle_object_tree(ObjectTreeAction::ToggleVisibility(MAIN_SERIES_ID), &sources);
        assert!(!app.chart.is_main_series_visible());
        assert!(app.chart.undo());
        assert!(app.chart.is_main_series_visible());
        assert!(app.chart.redo());
        assert!(!app.chart.is_main_series_visible());
        assert!(!app.build_source_items()[0].visible);
        assert!(app.chart.drawings().drawings[0].visible);
        let sources = app.build_source_items();
        app.handle_object_tree(ObjectTreeAction::ToggleVisibility(MAIN_SERIES_ID), &sources);
        assert!(app.chart.is_main_series_visible());
        app.handle_object_tree(ObjectTreeAction::ToggleVisibility(ids[0]), &sources);
        assert!(!app.chart.drawings().drawings[0].visible);
        let sources = app.build_source_items();
        app.handle_object_tree(ObjectTreeAction::ToggleVisibility(ids[0]), &sources);
        assert!(app.chart.drawings().drawings[0].visible);
        // Tied layers still have a stable paint order. Move across exactly one neighbour.
        for drawing in &mut app.chart.drawings_mut().drawings {
            drawing.z_order = 0;
        }
        for (action, expected) in [
            (
                ObjectTreeAction::MoveUp(ids[0]),
                vec![ids[1], ids[0], ids[2]],
            ),
            (ObjectTreeAction::MoveDown(ids[0]), ids.clone()),
            (
                ObjectTreeAction::BringToFront(ids[0]),
                vec![ids[1], ids[2], ids[0]],
            ),
            (ObjectTreeAction::SendToBack(ids[0]), ids.clone()),
        ] {
            let sources = app.build_source_items();
            let before: Vec<_> = app
                .chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.z_order)
                .collect();
            app.handle_object_tree(action, &sources);
            let after: Vec<_> = app
                .chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.z_order)
                .collect();
            assert!(app.chart.undo());
            assert_eq!(
                app.chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| d.z_order)
                    .collect::<Vec<_>>(),
                before
            );
            assert!(app.chart.redo());
            assert_eq!(
                app.chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| d.z_order)
                    .collect::<Vec<_>>(),
                after
            );
            let mut actual: Vec<_> = app
                .chart
                .drawings()
                .drawings
                .iter()
                .map(|d| (d.z_order, d.id))
                .collect();
            actual.sort_by_key(|(z, _)| *z);
            assert_eq!(
                actual.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
                expected
            );
        }
        let sources = app.build_source_items();
        app.handle_object_tree(ObjectTreeAction::Rename(ids[0], "Support".into()), &sources);
        assert_eq!(
            app.chart.drawings().drawings[0].label.as_deref(),
            Some("Support")
        );
        assert!(app.chart.undo());
        assert_eq!(app.chart.drawings().drawings[0].label, None);
        assert!(app.chart.redo());
        app.handle_object_tree(ObjectTreeAction::Duplicate(ids[0]), &sources);
        assert_eq!(app.chart.drawings().drawings.len(), 4);
        assert!(app.chart.undo());
        assert_eq!(app.chart.drawings().drawings.len(), 3);
        assert!(app.chart.redo());
        assert_eq!(app.chart.drawings().drawings.len(), 4);
        assert!(app.chart.undo());
        let spacing = app.chart.get_time_scale_state().0;
        let manual = app.chart.state().price_scale().is_manual();
        app.handle_object_tree(ObjectTreeAction::ZoomTo(ids[0]), &sources);
        assert_eq!(app.chart.get_time_scale_state().0, spacing);
        assert_eq!(app.chart.state().price_scale().is_manual(), manual);
        let study = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(2)));
        app.chart.set_indicator_inputs(study, "2 close");
        let sources = app.build_source_items();
        app.handle_object_tree(
            ObjectTreeAction::Duplicate(indicator_tree_id(study)),
            &sources,
        );
        let copy = app
            .chart
            .indicators()
            .find(|(id, _)| *id != study)
            .unwrap()
            .0;
        assert_eq!(app.chart.indicator_inputs(copy), Some("2 close"));
        app.object_tree.state_mut().select(ids[0]);
        app.object_tree
            .state_mut()
            .toggle_selection(indicator_tree_id(study));
        let sources = app.build_source_items();
        app.handle_object_tree(ObjectTreeAction::DeleteSelected, &sources);
        assert_eq!(app.chart.drawings().drawings.len(), 2);
        assert!(!app.chart.indicators().any(|(id, _)| id == study));
    }

    #[test]
    fn opening_settings_keeps_the_charts_current_display() {
        for (_, theme) in egui_charts::theme::presets() {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            let mut config = app.chart.config().clone().with_theme(&theme);
            config.show_quote_boxes = false;
            config.show_left_axis = true;
            config.show_right_axis = false;
            config.show_countdown = false;
            app.chart.price_scale_mut().options_mut().mode =
                egui_charts::scales::PriceScaleMode::Percentage;
            app.chart.price_scale_mut().set_range(-5.0, 5.0);
            *app.chart.config_mut() = config.clone();
            // The ticket menu changes the live trading options before settings are reopened.
            app.trading.display.instant_orders = true;
            app.handle_top_toolbar(&ctx, TopToolbarAction::OpenSettings);
            assert!(app.settings_state.trading.instant_orders);
            assert!(app.settings_dialog.settings().trading.instant_orders);
            let state = app.settings_state.clone();
            apply_settings_state(&state, &mut app.chart, &mut app.timeframe_toolbar);
            let applied = app.chart.config();
            assert!(!applied.show_quote_boxes);
            assert!(applied.show_left_axis && !applied.show_right_axis);
            assert!(!applied.show_countdown);
            assert!(state.scales_and_lines.percentage_scale);
            assert!(!state.scales_and_lines.auto_scale);
            assert_eq!(applied.grid_color, config.grid_color);
            assert_eq!(applied.background_color, config.background_color);
            assert_eq!(applied.bullish_color, config.bullish_color);
            assert_eq!(applied.bearish_color, config.bearish_color);
            assert_eq!(
                applied
                    .bullish_border_color
                    .unwrap_or(applied.bullish_color),
                config.bullish_border_color.unwrap_or(config.bullish_color)
            );
            assert_eq!(
                applied
                    .bearish_border_color
                    .unwrap_or(applied.bearish_color),
                config.bearish_border_color.unwrap_or(config.bearish_color)
            );
            assert_eq!(
                applied.bullish_wick_color.unwrap_or(applied.bullish_color),
                config.bullish_wick_color.unwrap_or(config.bullish_color)
            );
            assert_eq!(
                applied.bearish_wick_color.unwrap_or(applied.bearish_color),
                config.bearish_wick_color.unwrap_or(config.bearish_color)
            );
        }
    }

    #[test]
    fn data_window_reads_the_hovered_rendered_element() {
        for kind in [ChartType::Candles, ChartType::Renko] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.chart.set_feed_paused(true);
            app.chart.set_chart_type(kind);
            use egui_charts::studies::{CustomIndicator, IndicatorValue, SMA};
            let average = app.chart.add_indicator(Box::new(SMA::new(2)));
            let study = app.chart.indicator_mut(average).unwrap();
            let mut settings = study.settings();
            settings.precision = Some(3);
            study.apply_settings(&[], settings).unwrap();
            let gap = app.chart.add_indicator(Box::new(
                CustomIndicator::new("Gap", |bars| {
                    vec![IndicatorValue::Multiple(vec![f64::NAN, 42.125]); bars.len()]
                })
                .with_overlay(false)
                .with_line_count(2)
                .with_line_names(vec!["Missing".into(), "Hidden plot".into()]),
            ));
            let study = app.chart.indicator_mut(gap).unwrap();
            let mut settings = study.settings();
            settings.plots[1].visible = false;
            study.apply_settings(&[], settings).unwrap();
            let hidden = app
                .chart
                .add_indicator(Box::new(CustomIndicator::new("Invisible", |bars| {
                    vec![IndicatorValue::Single(123.0); bars.len()]
                })));
            app.chart.set_indicator_visible(hidden, false);
            let frame = |app: &mut DemoApp, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.chart.show(ui);
                        app.drain_chart_events(ui.ctx());
                    },
                )
                .drop_without_applying_deltas();
            };
            for _ in 0..6 {
                frame(&mut app, vec![]);
            }
            // The volume histogram with its moving average shown.
            let volume = app
                .chart
                .indicators()
                .find(|(_, study)| study.is_volume())
                .unwrap()
                .0;
            app.chart
                .indicator_mut(volume)
                .unwrap()
                .set_input(
                    "show_average",
                    egui_charts::studies::InputValue::Toggle(true),
                )
                .unwrap();
            if kind == ChartType::Candles {
                // A comparison whose bar at each main bar's time closes at 500 plus its index.
                let comparison = app.chart.add_comparison("CMP", egui::Color32::GOLD);
                let bars = app
                    .chart
                    .bars(egui_charts::widget::SeriesId::MAIN)
                    .iter()
                    .enumerate()
                    .map(|(index, bar)| Bar::from_value(bar.time, 500.0 + index as f64))
                    .collect();
                app.chart.set_data(comparison, bars);
                frame(&mut app, vec![]);
            }
            let (start, end) = app.chart.state().visible_range();
            let index = (start + end) / 2;
            let map = app.chart.get_chart_mapping();
            frame(
                &mut app,
                vec![egui::Event::PointerMoved(egui::pos2(
                    map.idx_to_x(index),
                    map.rect.center().y,
                ))],
            );
            let info = app.build_data_window();
            let bars = app.chart.state().shown();
            assert_eq!(info.close, bars[index].close, "{kind:?}");
            assert_eq!(
                info.timestamp,
                bars[index].time.format("%a %d %b '%y %H:%M").to_string(),
                "{kind:?}"
            );
            assert_eq!(info.prev_close, bars[index - 1].close, "{kind:?}");
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui_charts::ui::widget_bar::show_data_window(ui, &info, true);
            });
            let text: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert!(text.iter().any(|text| text == "SMA: SMA(2)"), "{text:?}");
            // The two-element average is the sum of the displayed closes divided by two.
            assert!(
                text.contains(&format!(
                    "{:.3}",
                    (bars[index - 1].close + bars[index].close) / 2.0
                )),
                "{text:?}"
            );
            if kind == ChartType::Candles {
                // One volume row, then the average's row and no row for the volume columns.
                assert!(text.iter().any(|text| text == "Vol"), "{text:?}");
                let volume: Vec<_> = text
                    .iter()
                    .filter(|text| text.starts_with("Volume: "))
                    .collect();
                assert!(
                    volume.len() == 1 && !volume[0].contains(" volume"),
                    "{text:?}"
                );
                let row = text.iter().position(|text| text == "CMP");
                let value = row.and_then(|row| text.get(row + 1));
                assert_eq!(
                    value.and_then(|value| value.replace(',', "").parse::<f64>().ok()),
                    Some(500.0 + index as f64),
                    "{text:?}"
                );
            }
            assert!(text.iter().any(|text| text == "Gap: Missing"), "{text:?}");
            assert!(text.iter().any(|text| text == "∅"), "{text:?}");
            assert!(
                !text
                    .iter()
                    .any(|text| text == "Invisible" || text.contains("Hidden plot")),
                "{text:?}"
            );
            let pointer = app.crosshair.clone().unwrap();
            let latest = bars.last().unwrap().close;
            let last = bars.len() - 1;
            frame(&mut app, vec![egui::Event::PointerGone]);
            let off_bars = app.build_data_window();
            assert_eq!(off_bars.close, latest);
            if kind == ChartType::Candles {
                let value = off_bars.series[0].value.as_deref().unwrap();
                assert_eq!(
                    value.replace(',', "").parse::<f64>(),
                    Ok(500.0 + last as f64)
                );
            }
            assert_eq!(app.pointer_readout, Some(pointer.clone()));
            app.open_chart_settings_dialog();
            let mut settings = app.settings_state.clone();
            settings.timezone = "Europe/Paris".to_owned();
            let view = app.chart.get_time_scale_state();
            apply_settings_state(&settings, &mut app.chart, &mut app.timeframe_toolbar);
            assert_eq!(
                app.chart
                    .format_time("2024-10-27T01:00:00Z".parse().unwrap()),
                "Sun 27 Oct '24 02:00"
            );
            assert_eq!(app.timeframe_toolbar.state.timezone, "Europe/Paris");
            assert_eq!(app.chart.get_time_scale_state(), view);
            app.open_chart_settings_dialog();
            assert_eq!(app.settings_state.timezone, "Europe/Paris");

            // A saved event still renders its legend and tooltip after the chart is gone.
            *app.chart = Chart::new(BarData::new());
            app.api_samples = true;
            for theme in [Theme::light(), Theme::dark()] {
                egui_charts::theme::apply_to_egui(&ctx, &theme);
                for width in [320.0, 800.0] {
                    let mut labels = Vec::new();
                    let mut hover = egui::pos2(40.0, 15.0);
                    for step in 0..4 {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                time: Some(100.0 + ctx.cumulative_frame_nr() as f64),
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 600.0),
                                )),
                                events: (step < 2)
                                    .then_some(egui::Event::PointerMoved(hover))
                                    .into_iter()
                                    .collect(),
                                ..Default::default()
                            },
                            |ui| app.show_chart(ui, theme.chart.bg),
                        );
                        if let Some(text) = output.shapes.iter().find_map(|shape| {
                            if let egui::Shape::Text(text) = &shape.shape
                                && text.galley.text().starts_with("O ")
                            {
                                Some(text)
                            } else {
                                None
                            }
                        }) {
                            hover = text.pos + text.galley.size() / 2.0;
                        }
                        labels = output
                            .shapes
                            .iter()
                            .filter_map(|shape| {
                                if let egui::Shape::Text(text) = &shape.shape {
                                    Some(text.galley.text().to_owned())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        output.drop_without_applying_deltas();
                    }
                    assert!(
                        labels.contains(&format!(
                            "O {:.2} H {:.2} L {:.2} C {:.2} V {:.0}",
                            info.open, info.high, info.low, info.close, info.volume
                        )),
                        "{labels:?}"
                    );
                    assert!(
                        labels.contains(&pointer.time.unwrap().to_rfc3339()),
                        "{labels:?}"
                    );
                    assert!(
                        labels.iter().any(|label| label.starts_with("SMA: SMA(2) ")),
                        "{labels:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn ranges_fit_displayed_history_in_both_themes_and_window_sizes() {
        use chrono::{DateTime, Datelike, Duration, Months, TimeZone, Utc};
        type Preset = (
            DateRange,
            Timeframe,
            fn(DateTime<Utc>) -> Option<DateTime<Utc>>,
        );
        let presets: [Preset; 9] = [
            (DateRange::Day1, Timeframe::Min1, |last| {
                Some(last - Duration::days(1))
            }),
            (DateRange::Day5, Timeframe::Min5, |last| {
                Some(last - Duration::days(5))
            }),
            (DateRange::Month1, Timeframe::Min30, |last| {
                last.checked_sub_months(Months::new(1))
            }),
            (DateRange::Month3, Timeframe::Hour1, |last| {
                last.checked_sub_months(Months::new(3))
            }),
            (
                DateRange::Month6,
                Timeframe::Custom(std::num::NonZeroU64::new(7200).unwrap()),
                |last| last.checked_sub_months(Months::new(6)),
            ),
            (DateRange::YTD, Timeframe::Day1, |last| {
                Utc.with_ymd_and_hms(last.year(), 1, 1, 0, 0, 0).single()
            }),
            (DateRange::Year1, Timeframe::Day1, |last| {
                last.checked_sub_months(Months::new(12))
            }),
            (DateRange::Year5, Timeframe::Week1, |last| {
                last.checked_sub_months(Months::new(60))
            }),
            (DateRange::All, Timeframe::Month1, |_| None),
        ];
        let mut wrong_intervals = Vec::new();
        for theme in [Theme::light(), Theme::dark()] {
            for size in [egui::vec2(1316.0, 871.0), egui::vec2(640.0, 450.0)] {
                for (interval, replay) in [(Timeframe::Hour1, true), (Timeframe::Min1, false)] {
                    let ctx = egui::Context::default();
                    let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                    app.theme = theme.clone();
                    app.chart.set_theme(&ctx, &theme);
                    app.switch_timeframe(interval);
                    app.timeframe_toolbar.set_session(SessionType::ETH);
                    app.apply_session();
                    app.chart.set_feed_paused(true);
                    let frame = |app: &mut DemoApp| {
                        ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| app.show_chart(ui, theme.chart.bg),
                        )
                        .drop_without_applying_deltas();
                    };
                    for _ in 0..3 {
                        frame(&mut app);
                    }
                    if replay {
                        app.start_replay();
                        frame(&mut app);
                    }
                    for (range, interval, cutoff) in presets {
                        app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(
                            range,
                        ));
                        for _ in 0..3 {
                            frame(&mut app);
                        }
                        if app.chart.timeframe() != Some(interval)
                            || app.state.timeframe != interval
                        {
                            wrong_intervals.push((range, app.chart.timeframe(), interval));
                        }
                        assert_eq!(app.replay.is_some(), replay);
                        if replay {
                            continue;
                        }
                        let bars = app.chart.bars(SeriesId::MAIN);
                        assert!(!bars.is_empty(), "{range:?}");
                        let end = bars.len();
                        let start = cutoff(bars.last().unwrap().time)
                            .map_or(0, |cutoff| bars.partition_point(|bar| bar.time < cutoff));
                        let (shown_start, shown_end) = app.chart.state().visible_range();
                        assert_eq!(shown_end, end, "{range:?}");
                        assert!(shown_start.abs_diff(start) <= 1, "{range:?}");
                        let expected = (app.chart.get_rendered_price_rect().width() - 1.0)
                            / ((end - start) as f32 + 0.5);
                        assert!(
                            (app.chart.get_time_scale_state().0 - expected).abs() < 0.001,
                            "{range:?}"
                        );
                    }
                    if !replay {
                        app.switch_timeframe(Timeframe::Hour1);
                        for _ in 0..3 {
                            frame(&mut app);
                        }
                        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
                        app.extra_charts[0].chart.set_feed_paused(true);
                        app.layout.sync.sync_time_axis = false;
                        frame(&mut app);
                        let day = app.bars[app.bars.len() / 2].time.date_naive();
                        let assert_date_range = |chart: &Chart| {
                            let range = chart.visible_time_range().unwrap();
                            for (actual, expected) in [
                                (range.from, day.and_hms_opt(0, 0, 0).unwrap().and_utc()),
                                (range.to, day.and_hms_opt(23, 59, 59).unwrap().and_utc()),
                            ] {
                                // A fraction of a pixel can span seconds across a session gap.
                                let error = (chart.time_to_index(actual).unwrap()
                                    - chart.time_to_index(expected).unwrap())
                                .abs()
                                    * f64::from(chart.get_time_scale_state().0);
                                assert!(error < 0.001, "{day} {size:?}: {range:?}, {error}px");
                            }
                        };
                        for sync in [false, true] {
                            app.layout.sync.sync_date_range = sync;
                            let before = app.extra_charts[0].chart.get_time_scale_state();
                            app.set_date_range(day, None, day, None);
                            frame(&mut app);
                            assert_date_range(&app.chart);
                            if sync {
                                assert_date_range(&app.extra_charts[0].chart);
                            } else {
                                assert_eq!(
                                    app.extra_charts[0].chart.get_time_scale_state(),
                                    before
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(wrong_intervals.is_empty(), "{wrong_intervals:?}");
    }

    #[test]
    fn a_new_preset_replaces_an_undelivered_range_fit() {
        for prior in [DateRange::Month1, DateRange::Year5] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                "2026-11-12T15:00:00Z".parse().unwrap(),
                true,
            );
            settle(&mut app, &ctx);
            app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(prior));
            // Both choices can arrive before the chart's next frame drains either switch.
            app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(DateRange::All));
            settle(&mut app, &ctx);
            assert_eq!(app.chart.timeframe(), Some(Timeframe::Month1));
            let count = app.chart.bars(SeriesId::MAIN).len();
            assert!(count > 0);
            let expected =
                (app.chart.get_rendered_price_rect().width() - 1.0) / (count as f32 + 0.5);
            assert!(
                (app.chart.get_time_scale_state().0 - expected).abs() < 0.001,
                "{prior:?}"
            );
        }
    }

    #[test]
    fn a_preset_keeps_an_interval_the_data_source_does_not_offer() {
        use owndata::feed::tests::{Server, pages, profile, until};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let server = Server::start(pages(vec![
            r#"[{"t":1794268800,"o":10,"h":12,"l":9,"c":11,"v":100},{"t":1794355200,"o":11,"h":13,"l":10,"c":12,"v":100}]"#.into(),
            "[]".into(),
        ]));
        let mut own = profile(&server.origin);
        own.intervals
            .retain(|interval| interval.chart == Timeframe::Day1);
        app.connect_own_data(own, owndata::Secrets::new());
        until(&ctx, &mut app.chart, "daily history", |chart| {
            chart
                .feed_state(SeriesId::MAIN)
                .is_some_and(|feed| feed.history_complete)
        });
        settle(&mut app, &ctx);
        assert_eq!(app.chart.bars(SeriesId::MAIN).len(), 2);
        app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(DateRange::Year5));
        settle(&mut app, &ctx);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Day1));
        assert_eq!(app.state.timeframe, Timeframe::Day1);
        let (start, end) = app.chart.state().visible_range();
        assert!(start <= 1);
        assert_eq!(end, 2);
        let expected = (app.chart.get_rendered_price_rect().width() - 1.0) / 2.5;
        assert!((app.chart.get_time_scale_state().0 - expected).abs() < 0.001);
        assert!(
            !app.toasts
                .toasts()
                .iter()
                .any(|toast| toast.message.contains("has no interval"))
        );
    }

    #[test]
    fn undo_after_a_preset_twice_keeps_the_view_until_bars_arrive() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        app.chart.apply_synced_time_scale(7.0, -30.0);
        settle(&mut app, &ctx);
        app.chart
            .user_action(|chart| chart.set_chart_type(ChartType::Line));
        settle(&mut app, &ctx);
        assert!(app.chart.undo());
        settle(&mut app, &ctx);
        let before = app.chart.get_time_scale_state();
        assert!(app.chart.redo());
        settle(&mut app, &ctx);
        app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(DateRange::Year5));
        settle(&mut app, &ctx);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Week1));
        assert!(app.chart.undo());
        assert!(app.chart.undo());
        settle(&mut app, &ctx);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Min1));
        assert_eq!(app.chart.chart_type(), ChartType::Candles);
        assert_eq!(app.chart.get_time_scale_state(), before);
    }

    #[test]
    fn undo_after_a_preset_restores_interval_and_view() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        app.chart.apply_synced_time_scale(7.0, -30.0);
        settle(&mut app, &ctx);
        let before = app.chart.get_time_scale_state();
        app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(DateRange::Year5));
        settle(&mut app, &ctx);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Week1));
        assert_ne!(app.chart.get_time_scale_state().0, before.0);
        assert!(app.chart.undo());
        settle(&mut app, &ctx);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Min1));
        assert_eq!(app.chart.get_time_scale_state(), before);
        assert!(!app.chart.undo(), "a preset records exactly one step");
        for finish_fit in [true, false] {
            app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(DateRange::Day1));
            if finish_fit {
                settle(&mut app, &ctx);
            }
            assert!(app.chart.undo());
            settle(&mut app, &ctx);
            assert_eq!(app.chart.timeframe(), Some(Timeframe::Min1));
            assert_eq!(app.chart.get_time_scale_state(), before);
        }
    }

    /// A size entered in the chart settings is the user's: no chart type, interval or symbol
    /// choice replaces it. The types without an entered size keep a size taken from the bars on
    /// show.
    #[test]
    fn entered_price_based_sizes_survive_navigation() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Kagi));
        let mut settings = ChartSettingsState::from_chart(&app.chart);
        let params = settings.chart_type_params.as_mut().unwrap();
        params.kagi_size_method = BoxSizeMethod::Fixed;
        params.kagi_reversal = 12.5;
        app.apply_chart_settings(settings);
        assert_eq!(app.chart.chart_type_params().kagi_reversal, 12.5);
        let sample_brick = app.chart.chart_type_params().renko_brick_size;
        for change in ["Point and Figure", "Kagi", "one hour", "TOKENA"] {
            match change {
                "Point and Figure" => app.handle_top_toolbar(
                    &ctx,
                    TopToolbarAction::ChartStyleSelected(ChartType::PointAndFigure),
                ),
                "Kagi" => app.handle_top_toolbar(
                    &ctx,
                    TopToolbarAction::ChartStyleSelected(ChartType::Kagi),
                ),
                "one hour" => app
                    .handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Hour1)),
                symbol => app.switch_symbol(symbol),
            }
            settle(&mut app, &ctx);
            let params = app.chart.chart_type_params();
            assert_eq!(params.kagi_size_method, BoxSizeMethod::Fixed, "{change}");
            assert_eq!(params.kagi_reversal, 12.5, "{change}");
        }
        // TOKENA trades far above the sample symbol, and its bricks are sized to match.
        let close = app.chart.bars(SeriesId::MAIN).last().unwrap().close;
        let brick = app.chart.chart_type_params().renko_brick_size;
        assert!(
            brick > sample_brick && brick < close / 10.0,
            "{sample_brick} {brick} {close}"
        );
    }

    /// A chart that follows a linked symbol change loads the symbol in the session its market
    /// trades, as the active chart does, and both keep the same bars at a linked interval.
    #[test]
    fn linked_symbol_change_loads_the_new_symbols_session() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        settle(&mut app, &ctx);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.layout.sync.sync_symbol = true;
        app.layout.sync.sync_timeframe = true;
        app.top_toolbar.chart_grid_selector.sync = app.layout.sync.clone();
        settle(&mut app, &ctx);
        let span = |chart: &Chart| {
            let bars = chart.bars(SeriesId::MAIN);
            let times = bars.first().zip(bars.last()).map(|(a, b)| (a.time, b.time));
            (chart.symbol().to_owned(), bars.len(), times)
        };
        for step in ["STOCKB", "one hour"] {
            if step == "STOCKB" {
                app.switch_symbol("STOCKB");
            } else {
                app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Hour1));
            }
            settle(&mut app, &ctx);
            assert_eq!(span(&app.extra_charts[0].chart), span(&app.chart), "{step}");
        }
    }

    /// A new session shows the navigation row while the pointer is over the main pane only.
    #[test]
    fn new_session_shows_navigation_buttons_on_hover() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        settle(&mut app, &ctx);
        let pane = app.chart.get_rendered_price_rect().center();
        for (pointer, shown) in [(pane, true), (egui::pos2(700.0, 15.0), false)] {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events: vec![egui::Event::PointerMoved(pointer)],
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
            // The row's logarithmic toggle is the only "log" the demo paints.
            let painted = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "log")
            });
            output.drop_without_applying_deltas();
            assert_eq!(painted, shown, "{pointer:?}");
        }
    }

    /// A touch held on the plot shows its values after the hold delay, keeps them after the
    /// finger lifts and clears them at the next tap; with the long press order option and a
    /// broker, the hold opens the order ticket with a limit order and shows no values.
    #[test]
    fn touch_hold_values_stay_after_release_until_the_next_tap() {
        use eframe::App;
        use egui::TouchPhase::{End, Start};
        for order in [false, true] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            let size = egui::vec2(1440.0, 900.0);
            if order {
                app.connect_paper();
            }
            settle(&mut app, &ctx);
            if order {
                let texts = ui_frame(&ctx, size, vec![], |ui| app.show_readout_controls(ui));
                let (_, rect) = texts
                    .iter()
                    .find(|(text, _)| text == "Long press opens an order")
                    .expect("long press order switch");
                ui_click(&ctx, size, rect.center(), |ui| {
                    app.show_readout_controls(ui)
                });
                assert!(app.chart.chart_options().tooltip.long_press_order);
                settle(&mut app, &ctx);
            }
            let plot = app.chart.get_rendered_price_rect();
            let pos = plot.center();
            let symbol = app.chart.symbol().to_owned();
            let mut ticket = false;
            let touch = |phase| {
                let mut events = vec![
                    egui::Event::Touch {
                        device_id: egui::TouchDeviceId(1),
                        id: egui::TouchId(1),
                        phase,
                        pos,
                        force: None,
                    },
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: phase == Start,
                        modifiers: Default::default(),
                    },
                ];
                if phase == End {
                    events.push(egui::Event::PointerGone);
                }
                events
            };
            // The demo keeps the default 500 ms hold delay.
            for (time, events, shown) in [
                (10.1, touch(Start), false),
                (10.4, vec![], false),
                (10.7, vec![], true),
                (10.8, touch(End), true),
                (10.9, vec![], true),
                (11.4, touch(Start), true),
                (11.5, touch(End), false),
            ] {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                );
                // Only the readout card paints an "Open" row; the status line writes "O 1.23".
                let painted = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == "Open" && plot.contains(text.pos))
                });
                // The ticket's send button writes its order as "1 ES @ 4,000.25 LIMIT".
                let limit = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text().contains(&format!(" {symbol} @ "))
                        && text.galley.text().ends_with(" LIMIT"))
                });
                output.drop_without_applying_deltas();
                assert_eq!(painted, shown && !order, "order {order}, time {time}");
                if !order {
                    assert!(!limit, "time {time}");
                }
                // The ticket shows its order once the broker's rules arrive.
                ticket |= limit && (10.7..10.95).contains(&time);
            }
            assert_eq!(
                ticket, order,
                "the hold opens the ticket with a limit order"
            );
        }
    }

    /// Shift+B and Shift+S open the order ticket for the chart symbol at market, through the
    /// chart's Buy and Sell buttons, and type nothing.
    #[test]
    fn market_order_keys_open_the_ticket_for_the_chart_symbol() {
        use eframe::App;
        for (key, letter, side) in [(egui::Key::B, "B", "Buy"), (egui::Key::S, "S", "Sell")] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut app = DemoApp::new_at(&cc, "2026-11-12T15:00:00Z".parse().unwrap(), true);
            app.connect_paper();
            settle(&mut app, &ctx);
            let symbol = app.chart.symbol().to_owned();
            let size = egui::vec2(1440.0, 900.0);
            let press = vec![
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::SHIFT,
                },
                egui::Event::Text(letter.into()),
            ];
            let mut send = Vec::new();
            // A frame without the key, which shows no ticket, then the key and up to three more.
            for frame in 0..5 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events: if frame == 1 { press.clone() } else { vec![] },
                        ..Default::default()
                    },
                    |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
                );
                send = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| {
                        let label = node.label()?;
                        (node.role() == egui::accesskit::Role::Button
                            && label.ends_with(&format!(" {symbol} MKT")))
                        .then(|| label.to_owned())
                    })
                    .collect::<Vec<_>>();
                output.drop_without_applying_deltas();
                if frame == 0 {
                    assert!(send.is_empty(), "{letter}: no ticket before the key");
                } else if !send.is_empty() {
                    break;
                }
            }
            // The send button is labelled with its side and order, as in "Buy 1 ES MKT".
            assert!(
                send.iter()
                    .any(|label| label.starts_with(&format!("{side} "))),
                "{letter}: {send:?}"
            );
            assert!(!app.symbol_search.is_open(), "{letter} was not typed");
        }
    }

    /// The chart preset menu names and highlights its choice, and Shift+B uses its size and
    /// exits in instant placement mode.
    #[test]
    fn a_preset_chosen_beside_buy_and_sell_sizes_the_shift_b_order() {
        use egui::accesskit::Toggled;
        use egui_charts::ui::trading::{Bracket, BracketMode, OrderPreset};
        use egui_kittest::kittest::NodeT as _;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.connect_paper();
        settle(&mut app, &ctx);
        let symbol = app.chart.symbol().to_owned();
        let last = app
            .chart
            .bars(SeriesId::MAIN)
            .iter()
            .rfind(|bar| !bar.is_whitespace())
            .unwrap()
            .close;
        let tick = app
            .chart
            .instrument()
            .and_then(|instrument| instrument.tick_size)
            .unwrap_or(0.01);
        app.paper.set_price(&symbol, last);
        app.trading.display.instant_orders = true;
        let presets = [("Scalp", 2.0, 10.0, 5.0), ("Swing", 3.0, 20.0, 10.0)].map(
            |(name, quantity, tp, sl)| {
                let mut preset = OrderPreset::new(name, quantity);
                preset.take_profit = Some(Bracket::new(BracketMode::Ticks, tp));
                preset.stop_loss = Some(Bracket::new(BracketMode::Ticks, sl));
                preset
            },
        );
        app.trading.presets.insert(String::new(), presets.into());
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                ctx.all_styles_mut(|style| {
                    style.interaction.tooltip_delay = 0.0;
                    style.interaction.show_tooltips_only_when_still = false;
                });
            })
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                app,
            );
        h.run_steps(3);
        h.get_by_label("Order presets").click();
        h.run_steps(2);
        for (name, selected) in [("No preset", true), ("Scalp", false), ("Swing", false)] {
            assert_eq!(
                h.get_by_label(name).accesskit_node().toggled(),
                Some(if selected {
                    Toggled::True
                } else {
                    Toggled::False
                })
            );
        }
        h.get_by_label("Swing").click();
        h.run_steps(2);
        assert_eq!(
            h.state().trading.presets[""]
                .iter()
                .map(|preset| preset.applied)
                .collect::<Vec<_>>(),
            [false, true]
        );
        for label in ["BUY · Swing", "SELL · Swing"] {
            assert!(h.output().shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)));
        }
        h.get_by_label("Order presets").click();
        h.run_steps(2);
        assert_eq!(
            h.get_by_label("Swing").accesskit_node().toggled(),
            Some(Toggled::True)
        );
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        let buy = h
            .output()
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "BUY · Swing" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap();
        h.hover_at(buy);
        h.run_steps(2);
        assert!(
            h.query_all_by_role(egui::Role::Label)
                .any(|node| node.value().is_some_and(|text| text.contains(
                    "Preset Swing\nQuantity 3\nTake profit 20 ticks\nStop loss 10 ticks"
                )))
        );
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::B);
        h.run_steps(3);
        let saved = serde_json::to_value(h.state().paper.save()).unwrap();
        let working = saved["working"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|order| order["request"]["symbol"] == symbol)
            .collect::<Vec<_>>();
        assert_eq!(working.len(), 2, "{working:?}");
        for order in &working {
            assert_eq!(order["request"]["quantity"], 3.0);
        }
        let tp = working
            .iter()
            .find_map(|order| order["request"]["order_type"]["Limit"].as_f64())
            .unwrap();
        let sl = working
            .iter()
            .find_map(|order| order["request"]["order_type"]["Stop"].as_f64())
            .unwrap();
        assert_eq!(((tp - sl) / tick).round(), 30.0);
    }

    /// Undocking restores the chart's width and keeps the choice in the settings the host saves.
    #[test]
    fn the_order_ticket_undocks_into_a_window_and_docks_again() {
        use egui_charts::model::{OrderRequest, OrderSide, OrderType};
        use egui_kittest::kittest::NodeT as _;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        app.connect_paper();
        settle(&mut app, &ctx);
        let symbol = app.chart.symbol().to_owned();
        app.trading.open_ticket(OrderRequest::new(
            &symbol,
            OrderSide::Buy,
            1.0,
            OrderType::Market,
        ));
        let mut h = Harness::builder()
            .with_accessibility_check(false)
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                app,
            );
        h.run_steps(3);
        let docked = h.state().chart.get_chart_mapping().rect.right();
        h.get_by_label("Undock ticket").click();
        h.run_steps(3);
        let display = serde_json::to_value(h.state().chart_settings_state().trading).unwrap();
        assert_eq!(display["undocked_ticket"], true);
        assert!(h.state().chart.get_chart_mapping().rect.right() >= docked + 300.0);
        assert!(
            h.query_all_by_role(egui::accesskit::Role::Button)
                .any(|node| {
                    node.accesskit_node()
                        .label()
                        .is_some_and(|label| label.ends_with(&format!(" {symbol} MKT")))
                })
        );
        h.get_by_label("Dock ticket").click();
        h.run_steps(3);
        let display = serde_json::to_value(h.state().trading.display.clone()).unwrap();
        assert_eq!(display["undocked_ticket"], false);
        assert!((h.state().chart.get_chart_mapping().rect.right() - docked).abs() < 1.0);
    }

    /// The paper account attaches exits to open positions, so a position line shows its TP and
    /// SL buttons and its label menu offers Protect position.
    #[test]
    fn paper_positions_offer_their_exit_buttons_and_protect_position() {
        use egui::accesskit::Role;
        use egui_charts::model::{OrderId, OrderRequest, OrderSide, OrderType, TimeInForce};
        use egui_kittest::kittest::NodeT as _;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.connect_paper();
        settle(&mut app, &ctx);
        let symbol = app.chart.symbol().to_owned();
        let bars = app.chart.bars(SeriesId::MAIN);
        let last = bars.iter().rfind(|bar| !bar.is_whitespace()).unwrap().close;
        app.paper.set_price(&symbol, last);
        let mut request = OrderRequest::new(&symbol, OrderSide::Buy, 2.0, OrderType::Market);
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let (answer, reply) = egui_charts::data::ReplyReader::<OrderId>::new();
        egui_charts::services::Broker::place(&*app.paper, request, reply);
        answer.take().expect("market reply").expect("filled");
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                app,
            );
        h.run_steps(3);
        // A text on the row of the position line, which lies at the entry price.
        let on_line = |h: &Harness<'_, Box<DemoApp>>, name: &str| {
            let mapping = h.state().chart.get_chart_mapping();
            let y = mapping.price_to_y(last);
            h.output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == name => {
                        let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                        (mapping.rect.contains(rect.center()) && (rect.center().y - y).abs() < 2.0)
                            .then(|| rect.center())
                    }
                    _ => None,
                })
        };
        let label = on_line(&h, "2").expect("the position's quantity on its label");
        h.hover_at(label);
        h.run_steps(2);
        for button in ["TP", "SL"] {
            assert!(on_line(&h, button).is_some(), "{button} button");
        }
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: label,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(3);
        let protect = h.get_by_role_and_label(Role::Button, "Protect position");
        assert!(!protect.accesskit_node().is_disabled());
    }

    /// The depth ladder opens a working order for modifying, and a saved preset of another order
    /// type applied to it is named above the form as a type the order cannot change to, while the
    /// order keeps its own type.
    #[test]
    fn a_preset_the_working_order_cannot_take_is_named_in_the_ticket() {
        use egui::accesskit::Role;
        use egui_charts::model::{OrderId, OrderRequest, OrderSide, OrderType, TimeInForce};
        use egui_charts::ui::trading::{OrderKind, OrderPreset};
        use egui_kittest::kittest::NodeT as _;

        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T15:30:00Z".parse().unwrap(),
            true,
        );
        app.connect_paper();
        settle(&mut app, &ctx);
        let symbol = app.chart.symbol().to_owned();
        let bars = app.chart.bars(SeriesId::MAIN);
        let last = bars.iter().rfind(|bar| !bar.is_whitespace()).unwrap().close;
        let tick = app
            .chart
            .instrument()
            .and_then(|instrument| instrument.tick_size)
            .unwrap_or(0.01);
        app.paper.set_price(&symbol, last);
        // A few ticks under the price, so the buy limit keeps working in a row of the ladder.
        let mut request = OrderRequest::new(
            &symbol,
            OrderSide::Buy,
            1.0,
            OrderType::Limit(last - 5.0 * tick),
        );
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let (answer, reply) = egui_charts::data::ReplyReader::<OrderId>::new();
        egui_charts::services::Broker::place(&*app.paper, request, reply);
        answer.take().expect("limit reply").expect("working limit");
        let mut stop = OrderPreset::new("Stop entry", 1.0);
        stop.order_type = Some(OrderKind::Stop);
        app.trading.presets.insert(String::new(), vec![stop]);
        app.trading.toggle_ticket(symbol);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    eframe::App::ui(&mut **app, ui, &mut eframe::Frame::_new_kittest());
                },
                app,
            );
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "DOM")
            .click_accesskit();
        h.run_steps(2);
        let tag = h
            .output()
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "LIMIT" => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
                }
                _ => None,
            })
            .expect("the order's tag on the ladder");
        // A click on the tag opens its menu.
        h.hover_at(tag);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: tag,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Edit order")
            .click_accesskit();
        h.run_steps(2);
        h.get_by_label("More").click_accesskit();
        h.run_steps(2);
        h.get_by_label("Stop entry").click_accesskit();
        h.run_steps(2);
        let notice = h
            .get_by_label("Order type Stop cannot be applied to a working order")
            .rect();
        // Modifying offers only the order's own type, near the top of the form.
        let kind = h.get_by_role_and_label(Role::Button, "Limit");
        assert_eq!(
            kind.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        assert!(notice.bottom() <= kind.rect().top(), "{notice:?}");
    }

    /// A day picked in the dialog is a day on the chart's clock. New York is five hours behind
    /// UTC in November, so its 10 November starts at 05:00 UTC, and the days 9 and 10 November
    /// run from 05:00 UTC on the 9th to 05:00 UTC on the 11th.
    #[test]
    fn date_picker_days_follow_the_chart_time_zone() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, "2026-11-12T15:00:00Z".parse().unwrap(), true);
        settle(&mut app, &ctx);
        apply_timezone(
            &mut app.chart,
            &mut app.timeframe_toolbar,
            "America/New_York",
        );
        let day = |day| chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        let near = |time: chrono::DateTime<chrono::Utc>, expected: &str| {
            let expected: chrono::DateTime<chrono::Utc> = expected.parse().unwrap();
            (time - expected).abs() <= chrono::TimeDelta::minutes(5)
        };
        app.go_to_date(day(10), None);
        settle(&mut app, &ctx);
        let span = app.chart.visible_time_range().unwrap();
        let centre = span.from + (span.to - span.from) / 2;
        assert!(near(centre, "2026-11-10T05:00:00Z"), "{span:?}");
        app.set_date_range(day(9), None, day(10), None);
        settle(&mut app, &ctx);
        let span = app.chart.visible_time_range().unwrap();
        assert!(near(span.from, "2026-11-09T05:00:00Z"), "{span:?}");
        assert!(near(span.to, "2026-11-11T05:00:00Z"), "{span:?}");
    }

    /// The demo's interval menu lists every range preset and applies the chosen size in ticks.
    #[test]
    fn interval_menu_selects_a_range_preset() {
        use eframe::App;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
        );
        let trigger = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                // Without saved favorites the menu trigger is named for the current interval.
                if node.role() == egui::accesskit::Role::Button && node.label() == Some("1 minute")
                {
                    node.bounds()
                } else {
                    None
                }
            })
            .expect("Chart interval trigger");
        let point = egui::pos2(
            ((trigger.x0 + trigger.x1) / 2.0) as f32,
            ((trigger.y0 + trigger.y1) / 2.0) as f32,
        );
        output.drop_without_applying_deltas();
        ui_click(&ctx, size, point, |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest());
        });
        let rows = ui_frame(&ctx, size, Vec::new(), |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest());
        });
        for label in [
            "1 range tick",
            "10 range ticks",
            "100 range ticks",
            "1000 range ticks",
        ] {
            assert_eq!(
                rows.iter().filter(|(text, _)| text == label).count(),
                1,
                "{label} must be listed in the demo interval menu"
            );
        }
        let point = rows
            .iter()
            .find(|(text, _)| text == "100 range ticks")
            .unwrap()
            .1
            .center();
        ui_click(&ctx, size, point, |ui| {
            app.ui(ui, &mut eframe::Frame::_new_kittest());
        });
        assert_eq!(app.chart.timeframe(), Some("100r".parse().unwrap()));
        assert_eq!(app.chart.chart_type(), ChartType::Range);
        let tick_size = app.chart.instrument().unwrap().tick_size.unwrap();
        assert!((app.chart.chart_type_params().range_size - 100.0 * tick_size).abs() < 1e-10);
    }

    /// Selecting seconds must rebuild the same executions, including the current partial minute.
    #[test]
    fn interval_switch_rebuilds_the_same_executions() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        let frame = |app: &mut DemoApp| {
            finish_feed(&app.live);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        for _ in 0..3 {
            frame(&mut app);
        }
        let tick_size = app.chart.instrument().unwrap().tick_size.unwrap();
        for bar in app.chart.bars(SeriesId::MAIN) {
            for price in [bar.open, bar.high, bar.low, bar.close] {
                assert!(
                    (price / tick_size - (price / tick_size).round()).abs() < 1e-7,
                    "backfill and executions must share the instrument's tick size"
                );
            }
        }
        for interval in ["10r", "25r"] {
            let timeframe = interval.parse().unwrap();
            app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(timeframe));
            assert_eq!(app.chart.chart_type(), ChartType::Range);
            assert_eq!(app.state.chart_type, ChartType::Range);
            assert_eq!(app.chart.timeframe(), Some(timeframe));
            let ticks = if interval == "10r" { 10.0 } else { 25.0 };
            assert!((app.chart.chart_type_params().range_size - ticks * tick_size).abs() < 1e-10);
            app.chart.set_data(SeriesId::MAIN, Vec::new());
            let mut params = app.chart.chart_type_params().clone();
            params.range_size = 1.0;
            app.chart.set_chart_type_params(params);
            app.size_range_interval();
            assert!((app.chart.chart_type_params().range_size - ticks * tick_size).abs() < 1e-10);
        }
        app.switch_timeframe(Timeframe::Min1);
        let minute = app.chart.bars(SeriesId::MAIN).last().unwrap().clone();
        app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Sec1));
        for _ in 0..3 {
            frame(&mut app);
        }
        let clock = egui_charts::model::BarClock::new(Timeframe::Min1);
        let seconds: Vec<_> = app
            .chart
            .bars(SeriesId::MAIN)
            .iter()
            .filter(|bar| clock.bar_open(bar.time) == minute.time)
            .collect();
        assert!(
            !seconds.is_empty(),
            "seconds must cover the same partial minute"
        );
        assert_eq!(seconds[0].open, minute.open);
        assert_eq!(seconds.last().unwrap().close, minute.close);
        assert_eq!(
            seconds
                .iter()
                .map(|bar| bar.high)
                .fold(f64::NEG_INFINITY, f64::max),
            minute.high
        );
        assert_eq!(
            seconds
                .iter()
                .map(|bar| bar.low)
                .fold(f64::INFINITY, f64::min),
            minute.low
        );
        assert_eq!(
            seconds.iter().map(|bar| bar.volume).sum::<f64>(),
            minute.volume
        );
        app.handle_top_toolbar(&ctx, TopToolbarAction::IntervalSelected(Timeframe::Min1));
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(app.chart.bars(SeriesId::MAIN).last(), Some(&minute));
        app.start_replay();
        app.switch_timeframe(Timeframe::Sec1);
        let expected = app.chart.bars(SeriesId::MAIN).to_vec();
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(
            app.chart.bars(SeriesId::MAIN),
            expected,
            "feed must not overwrite replay"
        );
        app.stop_replay();
        let mut options = app.chart.chart_options().clone();
        options.time_scale.timezone = egui_charts::config::TimezoneMode::nyse();
        let chart = std::mem::replace(&mut *app.chart, Chart::new(BarData::new()));
        *app.chart = chart.with_chart_options(options);
        let timeframe = "45min".parse().unwrap();
        app.switch_timeframe(timeframe);
        for _ in 0..3 {
            frame(&mut app);
        }
        let clock = egui_charts::model::BarClock::new(timeframe)
            .with_timezone(egui_charts::config::TimezoneMode::nyse().to_tz().unwrap());
        assert!(
            app.chart.bars(SeriesId::MAIN).len() >= 210,
            "coarse intervals need older bar history beyond the trade buffer"
        );
        let last_trade = app.chart.trades(SeriesId::MAIN).last().unwrap().ts;
        assert_eq!(
            app.chart.bars(SeriesId::MAIN).last().unwrap().time,
            clock.bar_open(last_trade),
            "feed must use the selected local calendar"
        );
    }

    /// After a choice in the time axis menu, the clock shows the time and the zone of the time
    /// axis: #2886.
    #[test]
    fn clock_follows_the_time_axis_menu() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-10T19:29:50Z".parse().unwrap(),
            true,
        );
        app.switch_symbol("STOCKA");
        let frame = |app: &mut DemoApp, events| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            );
            let texts: Vec<(String, egui::Pos2)> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        };
        let click = |app: &mut DemoApp, button, pos| {
            frame(app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
        };
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let plot = app.chart.get_rendered_price_rect();
        let bottom = (app.chart.get_rendered_indicator_panes().last())
            .map_or(plot.bottom(), |pane| pane.panel_rect.bottom());
        let axis = egui::pos2(plot.center().x, bottom + 14.0);
        click(&mut app, egui::PointerButton::Secondary, axis);
        let texts = frame(&mut app, vec![]);
        let exchange = texts
            .iter()
            .find(|(text, _)| text == "Exchange time")
            .unwrap_or_else(|| panic!("the time axis menu offers exchange time: {texts:?}"))
            .1;
        click(&mut app, egui::PointerButton::Primary, exchange);
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        assert!(matches!(
            app.chart.chart_options().time_scale.timezone,
            egui_charts::config::TimezoneMode::Exchange(_)
        ));
        let now = app
            .timeframe_toolbar
            .state
            .curr_time
            .expect("the clock runs");
        let clock = app.timeframe_toolbar.state.formatted_time();
        let time = app.chart.format_time(now);
        assert!(time.ends_with(&clock[..5]), "{clock} against {time}");
        assert!(clock.ends_with(" America/New_York"), "{clock}");
    }

    /// A second chart's menu opens a data table fixed to that chart beside the regular table of
    /// the active chart: each copies its own chart's rows, closing the fixed one leaves the
    /// regular one open, and typing over the fixed one starts no symbol search: #4162.
    #[test]
    fn fixed_data_table_keeps_its_chart_beside_the_regular_table() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-10T19:29:50Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        // Two charts of SAMPLE: one minute bars in the first slot, hourly bars in the second.
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.activate_chart_slot(1);
        app.switch_timeframe(Timeframe::Hour1);
        settle(&mut app, &ctx);
        let frame = |app: &mut DemoApp, events| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            );
            let texts: Vec<(String, egui::Pos2)> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect();
            let commands = output.platform_output.commands.clone();
            output.drop_without_applying_deltas();
            (texts, commands)
        };
        let click = |app: &mut DemoApp, button, pos| {
            let mut commands = frame(app, vec![egui::Event::PointerMoved(pos)]).1;
            for pressed in [true, false] {
                commands.extend(
                    frame(
                        app,
                        vec![egui::Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    )
                    .1,
                );
            }
            commands
        };
        let typed = |app: &mut DemoApp| {
            frame(app, vec![egui::Event::Text("Q".into())]);
            frame(app, vec![]);
            app.symbol_search.is_open()
        };
        let copies = |texts: &[(String, egui::Pos2)]| -> Vec<egui::Pos2> {
            texts
                .iter()
                .filter(|(text, _)| text == "Copy")
                .map(|(_, pos)| *pos)
                .collect()
        };
        let copied = |commands: Vec<egui::OutputCommand>| {
            commands
                .into_iter()
                .find_map(|command| match command {
                    egui::OutputCommand::CopyText(text) => Some(text),
                    _ => None,
                })
                .expect("Copy puts the table on the clipboard")
        };

        // The chart menu of the second chart, away from its candles, opens the fixed table.
        let pos = app.chart.get_rendered_price_rect().left_top() + egui::vec2(30.0, 30.0);
        click(&mut app, egui::PointerButton::Secondary, pos);
        let texts = frame(&mut app, vec![]).0;
        let row = texts
            .iter()
            .find(|(text, _)| text == "Open fixed data table")
            .expect("the chart menu offers a fixed data table")
            .1;
        click(&mut app, egui::PointerButton::Primary, row);
        let fixed = ctx
            .memory(|m| m.area_rect(egui::Id::unique("fixed_data_table")))
            .expect("the row opens the fixed table");
        assert_eq!(copies(&frame(&mut app, vec![]).0).len(), 1);
        assert!(!typed(&mut app), "typing over the fixed table alone");

        // The regular table follows the first chart, the active one now, and opens on top.
        app.activate_chart_slot(0);
        app.data_table.open();
        frame(&mut app, vec![]);
        let texts = frame(&mut app, vec![]).0;
        let top = ctx.top_layer_id().expect("a window is on top");
        let regular = ctx
            .memory(|m| m.area_rect(top.id))
            .expect("the regular table is the top window");
        let buttons = copies(&texts);
        assert_eq!(buttons.len(), 2, "both tables are open");
        let regular_copy = *buttons.iter().find(|pos| regular.contains(**pos)).unwrap();
        let fixed_copy = *buttons
            .iter()
            .find(|pos| fixed.contains(**pos) && !regular.contains(**pos))
            .unwrap();
        let regular_rows = copied(click(&mut app, egui::PointerButton::Primary, regular_copy));
        let fixed_rows = copied(click(&mut app, egui::PointerButton::Primary, fixed_copy));
        // Newest first, at the 19:29:50 clock: the live minute and the live hour lead.
        let times = |rows: &str| -> Vec<String> {
            rows.lines()
                .skip(1)
                .map(|row| row.split('\t').next().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            times(&regular_rows)[..2],
            ["2026-11-10T19:29:00+00:00", "2026-11-10T19:28:00+00:00"]
        );
        let hours = times(&fixed_rows);
        assert_eq!(
            hours[..2],
            ["2026-11-10T19:00:00+00:00", "2026-11-10T18:00:00+00:00"]
        );
        assert!(
            hours.iter().all(|time| time.ends_with(":00:00+00:00")),
            "the fixed table holds only the hourly chart's rows"
        );
        assert_eq!(
            hours.len(),
            app.extra_charts[0].chart.bars(SeriesId::MAIN).len()
        );

        // Its Copy click raised the fixed table above the regular one.
        assert!(
            !typed(&mut app),
            "typing over the fixed table beside the other"
        );
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, vec![escape]);
        let buttons = copies(&frame(&mut app, vec![]).0);
        assert_eq!(
            buttons,
            [regular_copy],
            "Escape closes the fixed table only"
        );
        assert!(app.data_table.is_open);
        assert!(
            typed(&mut app),
            "the regular table takes typed search again"
        );
    }
    /// The series menu opens an alert and copies a price at the values their rows name.
    #[test]
    fn series_menu_alert_starts_at_the_clicked_price() {
        for row_prefix in ["Add alert on STOCKA at ", "Copy price "] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                "2026-11-10T19:29:50Z".parse().unwrap(),
                true,
            );
            app.switch_symbol("STOCKA");
            settle(&mut app, &ctx);
            let frame = |app: &mut DemoApp, events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
                );
                let texts: Vec<(String, egui::Pos2)> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some((
                            text.galley.text().to_owned(),
                            text.pos + text.galley.size() / 2.0,
                        )),
                        _ => None,
                    })
                    .collect();
                let commands = output.platform_output.commands.clone();
                output.drop_without_applying_deltas();
                (texts, commands)
            };
            let click = |app: &mut DemoApp, button, pos| {
                let mut commands = frame(app, vec![egui::Event::PointerMoved(pos)]).1;
                for pressed in [true, false] {
                    commands.extend(
                        frame(
                            app,
                            vec![egui::Event::PointerButton {
                                pos,
                                button,
                                pressed,
                                modifiers: Default::default(),
                            }],
                        )
                        .1,
                    );
                }
                commands
            };
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            // Click off the tick grid inside a candle whose body lies clear of the last close.
            let mapping = app.chart.get_chart_mapping();
            let plot = app.chart.get_rendered_price_rect();
            let bars = app.chart.bars(SeriesId::MAIN);
            let last = bars.last().expect("the demo has bars").close;
            let (pos, low, high) = bars
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, bar)| {
                    let (low, high) = (bar.open.min(bar.close), bar.open.max(bar.close));
                    let pos = egui::pos2(
                        mapping.idx_to_x(index),
                        mapping.price_to_y((low + high) / 2.0) + 3.0,
                    );
                    ((last < low || last > high)
                        && mapping.price_to_y(low) - mapping.price_to_y(high) >= 8.0
                        && plot.shrink(40.0).contains(pos))
                    .then_some((pos, low, high))
                })
                .expect("a visible candle away from the last close");
            click(&mut app, egui::PointerButton::Secondary, pos);
            let texts = frame(&mut app, vec![]).0;
            let row = texts
                .iter()
                .find(|(text, _)| text.starts_with(row_prefix))
                .unwrap_or_else(|| panic!("the series menu offers {row_prefix}: {texts:?}"));
            let mut commands = click(&mut app, egui::PointerButton::Primary, row.1);
            commands.extend(frame(&mut app, vec![]).1);
            if row_prefix == "Copy price " {
                let expected = row.0.strip_prefix(row_prefix).unwrap();
                assert!(
                    commands.iter().any(|command| matches!(command,
                    egui::OutputCommand::CopyText(text) if text == expected)),
                    "the series row copies {expected}: {commands:?}"
                );
                continue;
            }
            assert!(
                app.alert_dialog.is_open,
                "the series row opens the alert dialog"
            );
            let AlertCondition::Crossing(price) = app.alert_dialog.alert.condition else {
                panic!("{:?}", app.alert_dialog.alert.condition);
            };
            let expected: f64 = row
                .0
                .strip_prefix(row_prefix)
                .unwrap()
                .strip_suffix('…')
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(price, expected);
            assert!((low..=high).contains(&price), "{price} in {low}..{high}");
        }
    }

    /// The demo published every subscribed quote once per frame, and the first push after a drain
    /// wakes the UI, so the demo asked for a repaint on every frame forever: #852. A settled demo
    /// with nothing on screen changing must ask for at most the once-a-second repaint of the bar
    /// countdown and the toolbar clock, never an immediate one. The first second of frames is
    /// left out: the scroll bars fade in and the settings service debounces its first write.
    #[test]
    fn a_settled_demo_does_not_repaint_every_frame() {
        use eframe::App;
        use std::time::Duration;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.live.set_paused(true);
        app.handle_comparison(CompareAction::AddSymbol(
            "FOREXA".into(),
            Default::default(),
        ));
        settle(&mut app, &ctx);
        app.ensure_chart_slots(ChartLayoutMode::Eight);
        for extra in &app.extra_charts {
            extra.live.set_paused(false);
        }
        app.layout.set_detached(1, true);
        app.ensure_chart_slots(ChartLayoutMode::Single);
        let hidden: Vec<_> = app
            .extra_charts
            .iter()
            .map(|extra| extra.live.now())
            .collect();
        // The startup toast lives on the wall clock and paints while it is up; it is not what
        // this test measures.
        app.toasts.toasts_mut().clear();
        let mut frame = eframe::Frame::_new_kittest();
        let mut time = 0.0f64;
        let mut delays = Vec::new();
        for _ in 0..60 {
            time += 0.05;
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut frame),
            );
            assert_eq!(
                app.extra_charts
                    .iter()
                    .map(|extra| extra.live.now())
                    .collect::<Vec<_>>(),
                hidden,
                "hidden feeds must not advance while the visible chart is paused"
            );
            delays.push(out.viewport_output[&egui::ViewportId::ROOT].repaint_delay);
            out.drop_without_applying_deltas();
        }
        for (index, delay) in delays.iter().enumerate().skip(24) {
            assert_ne!(
                *delay,
                Duration::ZERO,
                "frame {index} asked for an immediate repaint: {delays:?}"
            );
            assert!(
                *delay <= Duration::from_secs(1),
                "frame {index} asked for {delay:?}, more than the countdown's second"
            );
        }
        app.ensure_chart_slots(ChartLayoutMode::Eight);
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui, &mut frame))
                .drop_without_applying_deltas();
        }
        assert!(
            app.extra_charts
                .iter()
                .zip(&hidden)
                .all(|(extra, previous)| extra.live.now() > *previous),
            "restored slots must resume their feeds"
        );
        assert!(
            app.extra_charts
                .iter()
                .all(|extra| extra.comparisons.len() == 1),
            "restoring slots keeps their comparison bindings"
        );
    }

    #[test]
    fn copy_price_uses_the_selected_instruments_precision() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        for (symbol, price, expected) in [
            ("STOCKA", 123.45678, "123.46"),
            ("STOCKA", -0.001, "0.00"),
            ("FOREXA", 1.12345678, "1.1235"),
        ] {
            app.switch_symbol(symbol);
            let output = ctx.run_ui(Default::default(), |ui| {
                app.handle_context_action(
                    ui.ctx(),
                    ChartContextAction::Chart(ContextMenuAction::CopyPrice(price)),
                );
            });
            let commands = output.platform_output.commands.clone();
            output.drop_without_applying_deltas();
            assert!(
                commands.iter().any(|command| matches!(command,
                egui::OutputCommand::CopyText(text) if text == expected)),
                "{symbol}: {commands:?}"
            );
        }
    }

    #[test]
    fn settings_entry_points_open_the_same_chart_dialog() {
        for entry in ["top", "axis", "double-click", "series"] {
            let ctx = egui::Context::default();
            // The sample is generated from the clock; at a fixed time no study line crosses the
            // close that the double click aims at, which would take the click from the series.
            let start = "2026-10-28T16:14:00Z".parse().unwrap();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                start,
                true,
            );
            app.live.set_paused(true);
            app.chart.set_feed_paused(true);
            let frame = |app: &mut DemoApp, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.show_chart(ui, app.theme.chart.bg),
                )
                .drop_without_applying_deltas();
            };
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            match entry {
                "top" => app.handle_top_toolbar(&ctx, TopToolbarAction::OpenSettings),
                "series" => app.handle_series_context(SeriesContextMenuAction::OpenSettings),
                _ => {
                    let map = app.chart.get_chart_mapping();
                    let index = app.chart.state().shown().len() - 5;
                    let pos = if entry == "axis" {
                        egui::pos2(
                            map.rect.right() + app.chart.config().right_axis_width / 2.0,
                            map.rect.bottom() + 14.0,
                        )
                    } else {
                        egui::pos2(
                            map.idx_to_x(index),
                            map.price_to_y(app.chart.state().shown()[index].close),
                        )
                    };
                    frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
                    for _ in 0..if entry == "axis" { 1 } else { 2 } {
                        for pressed in [true, false] {
                            frame(
                                &mut app,
                                vec![egui::Event::PointerButton {
                                    pos,
                                    pressed,
                                    button: egui::PointerButton::Primary,
                                    modifiers: Default::default(),
                                }],
                            );
                        }
                    }
                }
            }
            assert!(
                app.settings_dialog.is_open,
                "{entry} must open chart settings"
            );
            assert_eq!(
                app.settings_dialog.settings(),
                &ChartSettingsState::from_chart(&app.chart)
            );
        }
    }

    #[test]
    fn series_settings_show_and_colour_the_alert_levels() {
        use egui_charts::ui::dialogs::SeriesSettingsTab;
        const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);
        fn painted(app: &mut DemoApp, ctx: &egui::Context, color: egui::Color32) -> bool {
            let mut painted = false;
            for _ in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                        ..Default::default()
                    },
                    |ui| app.show_chart(ui, app.theme.chart.bg),
                );
                painted = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                    egui::Shape::LineSegment { stroke, .. } if stroke.color == color)
                });
                output.drop_without_applying_deltas();
            }
            painted
        }
        fn apply(app: &mut DemoApp, ctx: &egui::Context) {
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ui_frame(ctx, SIZE, vec![], |ui| app.show_dialogs(ui.ctx()));
            }
            let at = texts
                .iter()
                .find(|(text, _)| text == "Ok")
                .unwrap()
                .1
                .center();
            ui_click(ctx, SIZE, at, |ui| app.show_dialogs(ui.ctx()));
        }
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T17:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        let price = app.chart.bars(SeriesId::MAIN).last().unwrap().close;
        let mut alert = Alert::new(app.chart.symbol(), AlertCondition::Above(price));
        alert.id = Some(egui_charts::model::AlertId("level".into()));
        app.alert_host = Arc::new(crate::panel_alerts::DemoAlerts::new(
            vec![alert],
            Vec::new(),
            |_| {},
        ));
        app.alerts.set_service(app.alert_host.clone());
        app.alerts.poll(&ctx, &mut app.toasts);
        let orange = egui::Color32::from_rgb(255, 152, 0);
        let blue = egui::Color32::from_rgb(0, 0, 255);
        let purple = egui::Color32::from_rgb(156, 39, 176);
        assert!(painted(&mut app, &ctx, orange));
        assert!(!painted(&mut app, &ctx, blue));
        app.handle_chart_context(ContextMenuAction::OpenSeriesSettings);
        assert!(app.series_settings.is_open);
        app.series_settings.active_tab = SeriesSettingsTab::ScalesAndLines;
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = ui_frame(&ctx, SIZE, vec![], |ui| app.show_dialogs(ui.ctx()));
        }
        let at = texts
            .iter()
            .find(|(text, _)| text == "Show alerts")
            .unwrap()
            .1
            .right_center()
            + egui::vec2(24.0, 0.0);
        // The label and its checkbox share the grid row; use the checkbox's column.
        let checkbox_x = texts
            .iter()
            .find(|(text, _)| text == "Show alert labels")
            .unwrap()
            .1
            .right()
            + 24.0;
        ui_click(&ctx, SIZE, egui::pos2(checkbox_x, at.y), |ui| {
            app.show_dialogs(ui.ctx())
        });
        assert!(!app.series_settings.settings.alerts.visible);
        apply(&mut app, &ctx);
        assert!(!painted(&mut app, &ctx, orange));
        for (chart_wide, own, drawn, not_drawn) in [
            (orange, Some(purple), purple, orange),
            // A colour picked equal to the default is the series' own.
            (blue, Some(orange), orange, blue),
            // A series without its own colour follows the chart settings.
            (blue, None, blue, orange),
        ] {
            app.settings_state.alerts.alert_color = chart_wide;
            app.handle_chart_context(ContextMenuAction::OpenSeriesSettings);
            assert_eq!(app.series_settings.settings.chart_alert_color, chart_wide);
            app.series_settings.settings.alerts.visible = true;
            app.series_settings.settings.alerts.color = own;
            apply(&mut app, &ctx);
            assert!(painted(&mut app, &ctx, drawn), "{own:?} on {chart_wide:?}");
            assert!(
                !painted(&mut app, &ctx, not_drawn),
                "{own:?} on {chart_wide:?}"
            );
        }
    }

    #[test]
    fn live_alerts_survive_symbol_changes_replay_and_watchlist_membership() {
        use eframe::App;
        for watchlist in [false, true] {
            for replay in [false, true] {
                let ctx = egui::Context::default();
                let mut app = DemoApp::new_at(
                    &eframe::CreationContext::_new_kittest(ctx.clone()),
                    chrono::Utc::now(),
                    true,
                );
                settle(&mut app, &ctx);
                app.switch_symbol("STOCKA");
                app.switch_timeframe(Timeframe::Hour1);
                let mut list = Watchlist::new("Test list");
                list.add_symbol("STOCKA");
                list.add_symbol("TOKENA");
                app.watchlist = WatchlistPanel::new(vec![list]);
                let mut alert = Alert::new("STOCKA", AlertCondition::Above(0.0));
                if watchlist {
                    alert.watchlist = Some("Test list".into());
                    if !replay {
                        alert.frequency = egui_charts::model::AlertFrequency::OncePerBarClose;
                    }
                }
                app.chart.prepare_alert(&mut alert);
                app.alerts.save(alert);
                app.alerts.poll(&ctx, &mut app.toasts);
                app.switch_symbol("FOREXA");
                if replay {
                    app.start_replay();
                }
                for _ in 0..4 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                settle(&mut app, &ctx);
                let mut symbols: Vec<_> = app
                    .alerts
                    .log()
                    .iter()
                    .map(|trigger| trigger.symbol.as_str())
                    .collect();
                symbols.sort_unstable();
                assert_eq!(
                    symbols,
                    if watchlist {
                        vec!["STOCKA", "TOKENA"]
                    } else {
                        vec!["STOCKA"]
                    },
                    "watchlist={watchlist}, replay={replay}"
                );
                app.switch_timeframe(Timeframe::Min1);
                app.watchlist.add_symbol("STOCKB");
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                settle(&mut app, &ctx);
                // Adding a member keeps a completed once alert stopped.
                let expected = match (watchlist, replay) {
                    (true, false) => 3,
                    (true, true) => 2,
                    _ => 1,
                };
                assert_eq!(app.alerts.log().len(), expected);
                if watchlist && !replay {
                    let feed = LiveFeed::new("STOCKB", app.live.now());
                    finish_feed(&feed);
                    feed.set_view(SessionType::RTH, false);
                    let bars = feed.snapshot(
                        Timeframe::Hour1,
                        egui_charts::model::BarClock::new(Timeframe::Hour1).with_session(
                            crate::sampledata::us_equity_zone(),
                            crate::sampledata::us_equity_session(),
                            false,
                        ),
                        false,
                    );
                    let trigger = app
                        .alerts
                        .log()
                        .iter()
                        .find(|trigger| trigger.symbol == "STOCKB")
                        .unwrap();
                    assert_eq!(
                        trigger.price,
                        Some(bars[bars.len() - 2].close),
                        "new watchlist members keep the alert's original interval and session"
                    );
                }

                app.switch_timeframe(Timeframe::Hour1);
                // A removed parent must not resolve to a copied study with the same numeric id.
                let parent = app
                    .chart
                    .add_indicator(Box::new(egui_charts::studies::SMA::new(2)));
                let child = app
                    .chart
                    .add_indicator(Box::new(egui_charts::studies::SMA::new(2)));
                app.chart.remove_series(parent);
                // Removing a source removes its readers, so name the removed parent afterwards,
                // as a layout saved with a missing source does.
                app.chart
                    .indicator_mut(child)
                    .unwrap()
                    .set_input(
                        "source",
                        egui_charts::studies::InputValue::Source(
                            egui_charts::studies::StudySource::Study {
                                id: parent.0,
                                plot: 0,
                            },
                        ),
                    )
                    .unwrap();
                let mut missing = Alert::new("FOREXA", AlertCondition::Above(0.0));
                missing.frequency = egui_charts::model::AlertFrequency::OncePerBarClose;
                missing.source = egui_charts::model::AlertSource::Study {
                    id: child.0.to_string(),
                    output: 0,
                };
                app.chart.prepare_alert(&mut missing);
                app.alerts.save(missing);
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                settle(&mut app, &ctx);
                assert_eq!(app.alerts.log().len(), expected);
                app.switch_timeframe(Timeframe::Min1);
                let replacement = app
                    .chart
                    .add_indicator(Box::new(egui_charts::studies::SMA::new(2)));
                let mut edited = app.alerts.alerts().last().unwrap().clone();
                edited.source = egui_charts::model::AlertSource::Study {
                    id: replacement.0.to_string(),
                    output: 0,
                };
                app.chart.prepare_alert(&mut edited);
                app.alerts.save(edited);
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                settle(&mut app, &ctx);
                assert_eq!(
                    app.alerts.log().len(),
                    expected + 1,
                    "an edited alert must use its newly selected study"
                );
                let reference = LiveFeed::new("FOREXA", app.live.now());
                finish_feed(&reference);
                let bars = reference.snapshot(
                    Timeframe::Hour1,
                    egui_charts::model::BarClock::new(Timeframe::Hour1),
                    false,
                );
                assert_eq!(
                    app.alerts.log().back().unwrap().price,
                    Some(bars[bars.len() - 2].close),
                    "editing a source must retain the alert's original interval"
                );
                app.watchlist = WatchlistPanel::new(vec![Watchlist::new("Empty")]);
                let mut expired = Alert::new("FOREXA", AlertCondition::Above(0.0));
                expired.watchlist = Some("Empty".into());
                expired.expires = Some(chrono::Utc::now() - chrono::TimeDelta::seconds(1));
                app.alerts.save(expired);
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                assert_eq!(
                    app.alerts.alerts().last().unwrap().status,
                    egui_charts::model::AlertStatus::Expired,
                    "expiry must not depend on a subscribed watchlist member"
                );
            }
        }
    }

    #[test]
    fn replay_preserves_alert_pause_state() {
        for paused in [false, true] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(paused);
            let mut alert = Alert::new(app.chart.symbol(), AlertCondition::Above(0.0));
            app.chart.prepare_alert(&mut alert);
            app.alerts.save(alert);
            app.alerts.poll(&ctx, &mut app.toasts);
            app.start_replay();
            assert!(app.replay.is_some());
            app.monitor_alerts(&ctx);
            assert_eq!(app.alert_live.paused(), paused);
        }
    }

    /// Alerts sharing a study id retain different parameters and do not repeat unchanged bars.
    #[test]
    fn alerts_on_one_symbol_see_one_series() {
        let ctx = egui::Context::default();
        // The feed is generated from the clock. At some times the last three closes make the two
        // averages equal or too close for the alert level to separate them, so the time is fixed.
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-04T17:30:00Z".parse().unwrap(),
            false,
        );
        app.live.set_paused(true);
        finish_feed(&app.live);
        app.apply_session();
        app.open_alert(None);
        let mut price = app.alert_dialog.alert.clone();
        price.condition = AlertCondition::Above(0.0);
        price.frequency = egui_charts::model::AlertFrequency::EveryTime;
        app.alerts.save(price);
        for _ in 0..4 {
            app.alerts.poll(&ctx, &mut app.toasts);
            finish_feed(&app.alert_live);
            app.monitor_alerts(&ctx);
        }
        let study = app
            .chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(2)));
        let bars = app.chart.bars(SeriesId::MAIN);
        let closes: Vec<_> = bars.iter().rev().take(3).map(|bar| bar.close).collect();
        let short = (closes[0] + closes[1]) / 2.0;
        let long = closes.iter().sum::<f64>() / 3.0;
        assert_ne!(short, long);
        let level = (short + long) / 2.0;
        for period in [2, 3] {
            app.chart
                .indicator_mut(study)
                .unwrap()
                .set_input("period", egui_charts::studies::InputValue::Length(period))
                .unwrap();
            let mut average = Alert::new(
                app.chart.symbol(),
                if short > long {
                    AlertCondition::Above(level)
                } else {
                    AlertCondition::Below(level)
                },
            );
            average.timeframe = Some(app.state.timeframe);
            average.frequency = egui_charts::model::AlertFrequency::EveryTime;
            average.source = AlertSource::Study {
                id: study.0.to_string(),
                output: 0,
            };
            app.handle_study_context(
                &ctx,
                study,
                egui_charts::ui::dialogs::StudyContextMenuAction::AddAlert(Box::new(average)),
            );
            for _ in 0..6 {
                app.alerts.poll(&ctx, &mut app.toasts);
                finish_feed(&app.alert_live);
                app.monitor_alerts(&ctx);
            }
            app.alerts.poll(&ctx, &mut app.toasts);
            assert_eq!(app.alerts.log().len(), 2, "period {period}");
        }
    }

    /// Buy and Sell Volume on the demo chart offers its buy volume, sell volume and delta as
    /// alert sources, read from the demo's sided executions (#4262): on a bar with executions,
    /// buy minus sell volume is the delta.
    #[test]
    fn buy_and_sell_volume_outputs_are_demo_alert_sources() {
        use egui_charts::studies::IndicatorFactory;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-18T17:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        let study = app.chart.add_indicator(
            IndicatorFactory::new()
                .create("Buy and Sell Volume")
                .unwrap(),
        );
        settle(&mut app, &ctx);
        let (sources, names): (Vec<_>, Vec<_>) = (app.chart.alert_sources().into_iter())
            .filter(|(source, _)| {
                matches!(source, AlertSource::Study { id, .. } if *id == study.0.to_string())
            })
            .map(|(source, name)| (source, name.to_string()))
            .unzip();
        assert_eq!(
            names,
            [
                "Buy and Sell Volume: Buy volume (lots)",
                "Buy and Sell Volume: Sell volume (lots)",
                "Buy and Sell Volume: Delta (lots)",
            ]
        );
        let traded = (0..app.chart.bars(SeriesId::MAIN).len()).find_map(|bar| {
            let [buy, sell, delta] = [0, 1, 2].map(|i| app.chart.alert_value(&sources[i], bar));
            Some((buy?, sell?, delta?)).filter(|(buy, sell, _)| buy + sell > 0.0)
        });
        let (buy, sell, delta) = traded.expect("a bar with executions");
        assert!(
            (buy - sell - delta).abs() <= 1e-9 * (buy + sell),
            "{buy} - {sell} = {delta}"
        );
    }

    /// An alert created with the dialog's defaults requests the alert sound chosen in the chart
    /// settings when it fires, with no other switch; one with Sound unticked requests none.
    #[test]
    fn alerts_from_the_dialog_request_the_alert_sound_only_with_sound_ticked() {
        let ctx = egui::Context::default();
        // A fixed time keeps the generated feed the same on every run.
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-04T17:30:00Z".parse().unwrap(),
            false,
        );
        app.live.set_paused(true);
        finish_feed(&app.live);
        app.apply_session();
        app.settings_state.alerts.sound = "Bell".into();
        for sound in [true, false] {
            app.open_alert(None);
            let mut alert = app.alert_dialog.alert.clone();
            // The dialog ticks Sound for a new alert.
            assert!(alert.delivery.sound);
            alert.delivery.sound = sound;
            // Every close is above zero, so the alert fires once.
            alert.condition = AlertCondition::Above(0.0);
            let fired = app.alerts.log().len();
            app.alerts.save(alert);
            for _ in 0..4 {
                finish_feed(&app.alert_live);
                app.poll_alerts(&ctx);
            }
            assert_eq!(app.alerts.log().len(), fired + 1, "sound={sound}");
            assert_eq!(app.alerts.take_sound(), sound.then(|| "Bell".to_owned()));
        }
    }

    /// Selecting the shown symbol changes nothing; selecting another still loads it.
    #[test]
    fn reselecting_the_active_symbol_keeps_its_bars() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        app.live.set_paused(true);
        let before = app.chart.bars(SeriesId::MAIN).to_vec();
        assert!(!before.is_empty());
        let symbol = app.chart.symbol().to_owned();
        app.switch_symbol(&symbol);
        for frame in 0..3 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| eframe::App::ui(&mut *app, ui, &mut eframe::Frame::_new_kittest()),
            )
            .drop_without_applying_deltas();
            let now = app.chart.bars(SeriesId::MAIN);
            assert_eq!(
                (now.len(), now.last().map(|bar| bar.time)),
                (before.len(), before.last().map(|bar| bar.time)),
                "frame {frame}"
            );
        }
        assert!(!app.live.loading(), "the feed restarted its bootstrap");
        app.start_replay_at(before.len() / 2);
        app.switch_symbol(&symbol);
        assert!(app.replay.is_some(), "the replay of the shown symbol stops");
        app.switch_symbol("TOKENA");
        assert!(app.replay.is_none(), "another symbol stops the replay");
        settle(&mut app, &ctx);
        assert_eq!(app.chart.symbol(), "TOKENA");
        assert!(!app.chart.bars(SeriesId::MAIN).is_empty());
    }
}
