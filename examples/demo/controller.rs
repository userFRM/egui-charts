//! Application controller: owns every dialog, panel, and the replay engine, and
//! routes the typed action each library widget emits into a real chart mutation.
//!
//! The chart engine is pure: widgets return `Action` enums and never touch the
//! chart themselves. This controller is the host half that consumes those
//! actions. Layouts live in host memory; templates use application storage. Controls that need
//! a trading account are hidden.

#[path = "alert_monitor.rs"]
mod alert_monitor;
#[path = "audio.rs"]
mod audio;
#[path = "commands.rs"]
mod commands;
#[path = "interval.rs"]
mod interval;
#[path = "layouts.rs"]
mod layouts;
use egui_charts::widget::ChartLayout;
use layouts::{DemoChart, LayoutTab};

#[path = "comparison.rs"]
mod comparison;
#[path = "marks.rs"]
mod marks;
#[path = "replay.rs"]
mod replay;
#[path = "storage.rs"]
mod storage;
#[path = "strategy.rs"]
mod strategy;
#[path = "templates.rs"]
mod templates;
use egui_charts::services::StoreKind;
use egui_charts::studies::StudyTemplate;
use egui_charts::ui::dialogs::templates::{DrawingTemplate, TemplateDialog};
#[path = "feeds.rs"]
mod feeds;
#[path = "live.rs"]
mod live;
#[path = "market.rs"]
mod market;

use eframe::egui;
use egui_charts::Chart;
use egui_charts::model::{Alert, AlertCondition};
use egui_charts::model::{Bar, BarData, ChartType, RenkoConfig, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::ui::app_state::ChartAppState;
use egui_charts::ui::context_menu::{
    ChartContextAction, ChartMenus, ContextMenuAction, HostMenuEntry,
};
use egui_charts::ui::dialogs::DrawingContextMenuAction;
use egui_charts::ui::dialogs::{
    AlertDialog, AlertDialogAction, DrawingId, DrawingPropertiesAction, DrawingPropertiesDialog,
    KeyboardShortcutsDialog, SeriesContextMenuAction, StudyContextMenuAction,
};
use egui_charts::ui::drawing_toolbar::{DrawingToolbar, DrawingToolbarAction};
use egui_charts::ui::export::{ExportDialog, ExportFormat, ExportResponse};
use egui_charts::ui::floating_replay_control::{FloatingReplayControl, ReplayControlAction};
use egui_charts::ui::replay::{ReplayAction, ReplayController, ReplaySession, ReplayStartPicker};
use egui_charts::ui::timeframe_toolbar::{
    DatePicker, DatePickerAction, SessionType, TimeframeToolbar, TimeframeToolbarAction,
};
#[cfg(test)]
use egui_charts::ui::top_toolbar::CompareAction;
use egui_charts::ui::top_toolbar::components::SymbolSearchDialog;
use egui_charts::ui::top_toolbar::components::indicator_dialog::{
    IndicatorDialog, IndicatorDialogAction, IndicatorSettingsDialog,
};
use egui_charts::ui::top_toolbar::components::settings_menu::{SettingsAction, SettingsDialog};
use egui_charts::ui::top_toolbar::{TopToolbar, TopToolbarAction};
use egui_charts::ui::widget_bar::panels::object_tree::{
    DataWindowInfo, ObjectTreeAction, ObjectTreePanel, SourceItem,
};
use egui_charts::ui::widget_bar::{AlertPanelAction, AlertsPanel};
use egui_charts::ui::widget_bar::{
    CalendarAction, CalendarPanel, ChatsPanel, HotlistsAction, HotlistsPanel, NewsAction,
    NewsPanel, NotificationsAction, NotificationsPanel, Product, ProductsAction, ProductsPanel,
    ScreenerAction, ScreenerPanel, SymbolDetailsPanel, TimeAndSalesPanel, Watchlist,
    WatchlistAction, WatchlistPanel, WidgetBarSection, WidgetBarWidget,
};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBar, WidgetBarAction};
use egui_charts::ui_kit::notifications::{NotificationPanel, Toasts};
use egui_charts::widget::SeriesId;
use egui_charts::widget::settings::ChartSettingsState;
use live::LiveFeed;
use std::sync::Arc;

use crate::sampledata::{DemoSymbol, visible_bars_for_range};

fn study_input_summary(study: &dyn egui_charts::studies::Indicator) -> String {
    study
        .inputs()
        .iter()
        .map(|input| match (&input.kind, &input.value) {
            (
                egui_charts::studies::InputKind::Choice(choices),
                egui_charts::studies::InputValue::Choice(index),
            ) => choices.get(*index).cloned().unwrap_or_default(),
            (_, egui_charts::studies::InputValue::Length(value)) => value.to_string(),
            (_, egui_charts::studies::InputValue::Integer(value)) => value.to_string(),
            (_, egui_charts::studies::InputValue::Number(value)) => value.to_string(),
            (_, egui_charts::studies::InputValue::Toggle(value)) => value.to_string(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Object-tree ids for indicators are offset into a high range so a single
/// `usize` id space cleanly distinguishes indicators from drawings (which use
/// small monotonic ids from the drawing manager). The main series reserves the last id.
const MAIN_SERIES_ID: usize = usize::MAX;
/// Local tones offered by the settings dialog.
const DEMO_SOUNDS: [&str; 3] = ["Chime", "Bell", "Beep"];
const INDICATOR_ID_BASE: usize = usize::MAX / 2;

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
#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePanel {
    ObjectTree,
    Alerts,
    DataWindow,
    Watchlist,
    TimeAndSales,
    Highlights,
    Screener,
    Hotlists,
    MarketHeatmap,
    Calendar,
    News,
    Notifications,
    Chats,
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
    Product {
        id: id.into(),
        icon,
        title: title.into(),
        description: description.into(),
    }
}

/// Export formats the demo can perform, with browser downloads on the web.
const EXPORT_FORMATS: &[ExportFormat] = &[
    ExportFormat::Png,
    ExportFormat::Csv,
    ExportFormat::Clipboard,
];

/// Where a rendered chart image is delivered.
enum ScreenshotTarget {
    File(Option<std::path::PathBuf>),
    Clipboard,
    NewTab,
}

/// App state surfaced to the toolbars via `ChartAppState`.
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

/// The full demo application.
pub struct DemoApp {
    pub chart: Chart,
    layout: ChartLayout,
    layout_slot: usize,
    extra_charts: Vec<DemoChart>,
    layout_tabs: Vec<LayoutTab>,
    layout_tab: usize,
    live: Arc<LiveFeed>,
    /// Whether an object tree colour picker is editing a drawing, as one undo step.
    colour_drag: bool,
    liquidity_heatmap: bool,
    menus: ChartMenus,
    last_chart_click: Option<egui_charts::widget::ChartPointer>,
    /// The latest crosshair payload of the active chart, from which the data window reads.
    crosshair: Option<egui_charts::widget::ChartPointer>,
    /// The last pointer event stays readable while the pointer is over the host legend.
    pointer_readout: Option<egui_charts::widget::ChartPointer>,
    pub state: DemoAppState,
    pub bars: Vec<Bar>,
    pub theme: Theme,
    /// No explicit choice follows the system appearance without saving its current value.
    theme_choice: Option<bool>,

    // Persistent toolbars.
    top_toolbar: TopToolbar,
    drawing_toolbar: DrawingToolbar,
    timeframe_toolbar: TimeframeToolbar,
    widget_bar: WidgetBar,

    // Dialogs (each owns its open/closed state).
    indicator_dialog: IndicatorDialog,
    indicator_settings: IndicatorSettingsDialog,
    settings_dialog: SettingsDialog,
    settings_state: ChartSettingsState,
    symbol_search: SymbolSearchDialog,
    compare_dialog: egui_charts::ui::top_toolbar::CompareSymbolsDialog,
    comparisons: Vec<(String, SeriesId, Arc<LiveFeed>)>,
    /// Spreads picked in symbol search, with feeds of their symbols.
    spreads: Vec<comparison::Spread>,
    storage: Arc<storage::DemoStorage>,
    settings: egui_charts::services::SettingsStore,
    /// Changes of the chart arrangement, which autosave counts with the charts' edits.
    grid_edits: u64,
    study_templates: TemplateDialog<Vec<StudyTemplate>>,
    drawing_templates: TemplateDialog<DrawingTemplate>,
    quick_search: egui_charts::ui_kit::command_palette::CommandPaletteHandler,
    alert_dialog: AlertDialog,
    drawing_props: DrawingPropertiesDialog,
    keyboard_shortcuts: KeyboardShortcutsDialog,
    interval_entry: Option<String>,
    interval_focus: bool,
    /// The saved-layout chooser of the Open layout shortcut is open.
    open_layout: bool,
    copied_drawing: Option<String>,
    // A study copied in this session, kept across symbols and intervals.
    copied_study: Option<String>,
    export_dialog: ExportDialog,
    date_picker: DatePicker,

    // A finished host-run sample, independent of subsequent chart navigation.
    strategy_report: egui_charts::ui::strategy_report::StrategyReportPanel,
    strategy_trade: Option<usize>,
    strategy_session: SessionType,

    // Right-hand side panel.
    object_tree: ObjectTreePanel,
    open_panel: Option<SidePanel>,
    data_window_expanded: bool,

    // Host-owned session-only alerts, shown through the library service connection.
    alerts: AlertsPanel,
    alert_host: Arc<crate::panel_alerts::DemoAlerts>,
    audio: audio::AudioPlayer,
    alert_monitors: std::collections::HashMap<
        (egui_charts::model::AlertId, String),
        alert_monitor::AlertMonitor,
    >,

    // Session-only paper account, connected from the broker picker and traded through the
    // library's order ticket and account panel.
    trading: egui_charts::ui::trading::Trading,
    paper: Arc<egui_charts::services::PaperAccount>,
    broker_picker: egui_charts::ui::trading::BrokerPicker,
    account_panel: AccountPanelSize,
    // The account settings ask whether to reset the paper account.
    paper_reset: bool,
    // Watchlists and time and sales, fed by synthetic quotes and the live feed's executions.
    watchlist: WatchlistPanel,
    quotes: Arc<live::DemoQuotes>,
    // Text pasted for a watchlist import, while its window is open.
    watchlist_import: Option<String>,
    // Symbol search is open for the watchlist's add button: the pick joins the list.
    watchlist_search: bool,
    time_and_sales: TimeAndSalesPanel,
    // Below the watchlist: the chart symbol's synthetic details and notes kept in app storage.
    symbol_details: SymbolDetailsPanel,
    /// The chart symbol's financials, shown in place of the chart while they are open.
    financials: Option<(String, egui_charts::ui::financials::FinancialsPanel)>,
    // Synthetic highlights and headlines from the live feed.
    highlights: NewsPanel,
    news: NewsPanel,
    calendar: CalendarPanel,
    screener: ScreenerPanel,
    hotlists: HotlistsPanel,
    market_heatmap: egui_charts::ui::widget_bar::MarketHeatmapPanel,
    // The demo's event log: its messages and alert triggers.
    notifications: NotificationsPanel,
    notification_host: Arc<feeds::DemoNotifications>,
    chats: ChatsPanel,
    products: ProductsPanel,

    // Replay engine and its floating transport. `None` until first toggled.
    replay: Option<ReplayController>,
    replay_start: ReplayStartPicker,
    replay_selecting: bool,
    replay_interval: Timeframe,
    replay_session: Option<(ReplaySession, Timeframe)>,
    replay_slots: Vec<usize>,
    live_was_paused: bool,
    replay_control: FloatingReplayControl,

    // Pending offscreen image options and delivery destination.
    screenshot_requested: Option<(ScreenshotTarget, egui_charts::widget::ImageExportOptions)>,

    toasts: Toasts,
    /// Handle of the demo's price line, moved and removed from the chart menu.
    demo_level: Option<egui_charts::widget::PriceLineId>,
    /// Whether the sample panels, markers, change marks, bar marks and demo level are on, from the
    /// quick search.
    api_samples: bool,
}

fn sample_widgets() -> [WidgetBarWidget; 2] {
    [
        WidgetBarWidget {
            id: 4,
            icon: egui_charts::icons::icons::SETTINGS_EVENTS,
            tooltip: "Time and sales".into(),
            section: WidgetBarSection::Bottom,
        },
        WidgetBarWidget {
            id: 2,
            icon: egui_charts::icons::icons::TEXT,
            tooltip: "Session notes".into(),
            section: WidgetBarSection::Bottom,
        },
    ]
}

impl DemoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register egui's image loaders so the embedded SVG toolbar icons decode.
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let theme_choice = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "dark_theme"));
        let dark = theme_choice
            .or_else(|| {
                cc.egui_ctx
                    .system_theme()
                    .map(|theme| theme == egui::Theme::Dark)
            })
            .unwrap_or(false);
        let theme = if dark { Theme::dark() } else { Theme::light() };
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
        let live = Arc::new(LiveFeed::new(symbol, chrono::Utc::now()));
        let quotes = Arc::new(live::DemoQuotes::new(live.clone()));
        let mut chart = egui_charts::ChartBuilder::new()
            .with_feed(live.clone())
            .with_symbol(symbol)
            .with_timeframe(timeframe)
            .with_theme(theme.clone())
            .with_visible_candles(0)
            .with_chart_options(options)
            .build();
        chart.config_mut().volume_height_fraction = 0.25;
        chart.config_mut().volume_baseline_inset = 0.0;
        chart.config_mut().candle_width = 0.75;
        chart.config_mut().right_axis_width = 59.0;
        chart.config_mut().skip_background = true;
        chart.set_symbol(symbol);
        chart.set_timeframe(timeframe);
        let storage = Arc::new(storage::DemoStorage::restore(cc.storage));
        chart.set_storage(storage.clone());

        let mut top_toolbar = TopToolbar::new().with_config(
            egui_charts::ui::top_toolbar::TopToolbarConfig::default().with_height(38.0),
        );
        top_toolbar.timeframe_selector.current = Timeframe::Min1;
        // A host's own call to action sits at the right end; the demo only reports it.
        top_toolbar.call_to_action = Some(("upgrade".into(), "Upgrade".into()));
        let mut menus = ChartMenus::default();
        menus.host_entries.extend([
            HostMenuEntry::new("research", "Open in research")
                .with_icon(&egui_charts::icons::icons::QUICK_SEARCH),
            HostMenuEntry::new("demo-level", "Move demo level here"),
            HostMenuEntry::new("demo-level-remove", "Remove demo level"),
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
        let mut timeframe_toolbar = TimeframeToolbar::default();
        timeframe_toolbar.config.active_bg = egui::Color32::TRANSPARENT;
        timeframe_toolbar.state.timeframe = timeframe;
        timeframe_toolbar.set_session(SessionType::Full24x7);

        let mut app = Self {
            chart,
            live,
            layout: ChartLayout::default(),
            layout_slot: 0,
            extra_charts: Vec::new(),
            layout_tabs: vec![LayoutTab::unnamed()],
            layout_tab: 0,
            colour_drag: false,
            liquidity_heatmap: false,
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
            theme,
            theme_choice,
            top_toolbar,
            drawing_toolbar: DrawingToolbar::default(),
            timeframe_toolbar,
            widget_bar,
            indicator_dialog: IndicatorDialog::new(),
            indicator_settings: IndicatorSettingsDialog::default(),
            settings_dialog: {
                let mut dialog = SettingsDialog::new();
                dialog.execution_sounds = DEMO_SOUNDS.map(str::to_owned).to_vec();
                dialog
            },
            settings_state: ChartSettingsState::default(),
            symbol_search: SymbolSearchDialog::new(),
            compare_dialog: egui_charts::ui::top_toolbar::CompareSymbolsDialog::new(),
            comparisons: Vec::new(),
            spreads: Vec::new(),
            settings: egui_charts::services::SettingsStore::new(storage.clone()),
            storage,
            grid_edits: 0,
            study_templates: TemplateDialog::new(StoreKind::StudyTemplates),
            drawing_templates: TemplateDialog::new(StoreKind::DrawingTemplates),
            quick_search: egui_charts::ui_kit::command_palette::CommandPaletteHandler::new(
                commands::quick_search_commands(),
            ),
            alert_dialog: AlertDialog::new(),
            drawing_props: DrawingPropertiesDialog::new(),
            keyboard_shortcuts: KeyboardShortcutsDialog::new(),
            interval_entry: None,
            interval_focus: false,
            open_layout: false,
            copied_drawing: None,
            copied_study: None,
            export_dialog: ExportDialog::new().with_formats(EXPORT_FORMATS),
            date_picker: DatePicker::new(),
            strategy_report: Default::default(),
            strategy_trade: None,
            strategy_session: SessionType::Full24x7,
            object_tree: ObjectTreePanel::new(),
            open_panel: Some(SidePanel::Watchlist),
            data_window_expanded: true,
            alerts: AlertsPanel::default(),
            alert_host: Arc::default(),
            audio: Default::default(),
            alert_monitors: Default::default(),
            trading: Default::default(),
            paper: Arc::new(
                egui_charts::services::PaperAccount::new("USD", 100_000.0).leverage(10.0),
            ),
            broker_picker: {
                let mut picker = egui_charts::ui::trading::BrokerPicker::new(example_brokers());
                picker.tiles[0].facts = broker_facts(egui_charts::ui::trading::BrokerTile::PAPER);
                picker
            },
            account_panel: AccountPanelSize::Normal,
            paper_reset: false,
            watchlist: WatchlistPanel::new(
                cc.storage
                    .and_then(|storage| eframe::get_value(storage, "watchlists"))
                    .unwrap_or_else(|| {
                        vec![Watchlist::from_text(
                            "Watchlist",
                            "###Crypto,BTCUSD,ETHUSD,###Stocks,AAPL,MSFT,NVDA,TSLA,\
                             ###Indices and forex,SPX,EURUSD,SAMPLE",
                        )]
                    }),
            ),
            quotes,
            watchlist_import: None,
            watchlist_search: false,
            time_and_sales: TimeAndSalesPanel::default(),
            symbol_details: SymbolDetailsPanel::new(),
            financials: None,
            highlights: NewsPanel::highlights(),
            news: NewsPanel::new(),
            calendar: CalendarPanel::new(),
            screener: ScreenerPanel::new(feeds::screener_sets()),
            hotlists: HotlistsPanel::new(feeds::HOTLISTS.map(String::from).to_vec()),
            market_heatmap: egui_charts::ui::widget_bar::MarketHeatmapPanel {
                entries: feeds::heatmap(),
            },
            notifications: NotificationsPanel::new(),
            notification_host: Arc::default(),
            chats: ChatsPanel::new(),
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
            replay: None,
            replay_start: ReplayStartPicker::default(),
            replay_selecting: false,
            replay_interval: Timeframe::Hour1,
            replay_session: cc
                .storage
                .and_then(|storage| eframe::get_value(storage, "replay_session"))
                .flatten(),
            replay_slots: Vec::new(),
            live_was_paused: false,
            replay_control: FloatingReplayControl::new(),
            screenshot_requested: None,
            toasts: Toasts::new(),
            demo_level: None,
            api_samples: false,
        };
        app.drawing_toolbar
            .set_control_visible(egui_charts::ui::Control::Trading, false);
        app.settings_dialog
            .templates
            .set_storage(app.storage.clone());
        app.study_templates
            .templates
            .set_storage(app.storage.clone());
        app.drawing_templates
            .templates
            .set_storage(app.storage.clone());
        let search: Arc<dyn egui_charts::services::SymbolSearch> = Arc::new(DemoSymbol::symbols());
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
        app.top_toolbar.state.can_trade = true;
        app.watchlist.set_selected(symbol);
        app.watchlist.set_service(app.quotes.clone());
        app.alert_dialog.set_storage(app.alert_host.clone());
        app.symbol_details.set_feed(app.live.clone());
        app.symbol_details.set_storage(app.storage.clone());
        app.top_toolbar.layout_menu.set_storage(app.storage.clone());
        app.highlights.set_feed(app.live.clone());
        app.news.set_feed(app.live.clone());
        app.calendar.set_feed(app.live.clone());
        app.screener.set_feed(app.live.clone());
        app.hotlists.set_feed(app.live.clone());
        app.notification_host
            .notify("Connected to the synthetic feed", None);
        app.notifications.set_service(app.notification_host.clone());
        app.chats
            .set_service(Arc::new(feeds::DemoChats::new(chrono::Utc::now())));
        app.calendar
            .set_countries(feeds::COUNTRIES.iter().map(|code| code.to_string()));
        app.apply_session();
        app.chart.drawings_mut().tool_defaults = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "drawing_defaults"))
            .unwrap_or_default();
        app.settings_state = ChartSettingsState::from_chart(&app.chart);
        // The demo's storage answers at once, so the settings apply before the first frame.
        let current = app.user_settings();
        if let Some(settings) = app.settings.poll(&cc.egui_ctx, &current) {
            app.apply_user_settings(&cc.egui_ctx, settings);
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
        self.state.chart_type = self.chart.chart_type();
        let timezone = &self.chart.chart_options().time_scale.timezone;
        let name = timezone
            .to_tz()
            .map_or_else(|| "UTC".to_owned(), |zone| zone.name().to_owned());
        let clock_changed = self.timeframe_toolbar.state.timezone != name;
        self.timeframe_toolbar.set_timezone(name);
        if let Some(tf) = self.chart.timeframe()
            && (tf != self.state.timeframe || clock_changed)
        {
            self.switch_timeframe(tf);
        }
    }

    // --- data helpers ------------------------------------------------------

    fn switch_timeframe(&mut self, tf: Timeframe) {
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
        // Source playback stays on its own interval when the display interval changes.
        self.configure_replay_intervals();
        self.push_replay_window_to_chart();
    }

    pub(crate) fn switch_symbol(&mut self, symbol: &str) {
        if let Some(row) = DemoSymbol::symbols()
            .into_iter()
            .find(|row| row.symbol == symbol)
        {
            self.symbol_search.record_symbol(&row);
        }
        self.copied_drawing = None;
        let spec = DemoSymbol::find(symbol);
        self.live.set_symbol(spec.symbol);
        self.time_and_sales.clear();
        self.watchlist.set_selected(spec.symbol);
        self.timeframe_toolbar.state.session_menu_open = false;
        self.state.symbol = spec.symbol.to_string();
        self.chart.set_symbol(spec.symbol);
        for control in [
            egui_charts::ui::Control::Session,
            egui_charts::ui::Control::DividendAdjustment,
        ] {
            self.timeframe_toolbar
                .set_control_visible(control, spec.symbol == "SAMPLE");
        }
        self.timeframe_toolbar
            .set_session(if matches!(spec.exchange, "Crypto" | "Forex") {
                SessionType::Full24x7
            } else {
                SessionType::RTH
            });
        self.regenerate_bars();
        // A symbol change invalidates an in-flight replay.
        self.stop_replay();
    }

    fn regenerate_bars(&mut self) {
        self.apply_session();
        self.size_price_based_charts();
    }

    /// The sizes of the price-based charts are in price units: take them from the bars on show.
    fn size_price_based_charts(&mut self) {
        let chart = &mut self.chart;
        let mut params = chart.chart_type_params().clone();
        let size = RenkoConfig::from_atr(chart.bars(SeriesId::MAIN), 14, 0.5).brick_size;
        if size > 0.0 && size.is_finite() {
            params.renko_brick_size = size;
            params.kagi_reversal = size;
            params.range_size = size;
            params.pnf_box_size = size;
        }
        if let Timeframe::Range(ticks) = self.state.timeframe {
            let tick_size = self.live.instrument().tick_size.unwrap_or(0.01);
            params.range_size = ticks.get() as f64 * tick_size;
            params.range_size_method = egui_charts::model::BoxSizeMethod::Fixed;
        }
        chart.set_chart_type_params(params);
    }

    fn apply_session(&mut self) {
        self.live.set_view(
            self.timeframe_toolbar.state.session_type,
            self.timeframe_toolbar.state.adjust_for_dividends,
        );
        self.chart.reload_feed();
        self.chart
            .set_extended_hours(self.timeframe_toolbar.state.session_type != SessionType::RTH);
        let tf = self.state.timeframe;
        let clock = self.chart.bar_clock();
        self.bars = self.live.snapshot(tf, clock, self.chart.extended_hours());
        self.chart.set_data(SeriesId::MAIN, self.bars.clone());
        self.chart.set_instrument(Some(self.live.instrument()));
        self.chart
            .set_legend_statuses(&[egui_charts::widget::LegendStatus {
                text: "SIM",
                color: egui::Color32::from_rgb(38, 166, 154),
                background: egui::Color32::TRANSPARENT,
            }]);
        self.chart.set_quote(None);
        self.chart.set_events(Vec::new());
        self.annotate();
    }

    // --- alerts ------------------------------------------------------------

    fn open_alert(&mut self, price: Option<f64>) {
        self.list_alert_sources();
        let price = price
            .or_else(|| self.chart.bars(SeriesId::MAIN).last().map(|b| b.close))
            .unwrap_or(0.0);
        self.alert_dialog.open(self.state.symbol.clone(), price);
    }

    /// Offers the studies' outputs and every level of the drawings as alert sources.
    fn list_alert_sources(&mut self) {
        use egui_charts::model::AlertSource;
        let bar = self.chart.bars(SeriesId::MAIN).len().saturating_sub(1) as f64;
        self.alert_dialog.sources = self
            .chart
            .indicators()
            .flat_map(|(id, study)| {
                study
                    .line_names()
                    .into_iter()
                    .enumerate()
                    .map(move |(output, name)| {
                        (
                            AlertSource::Study {
                                id: id.0.to_string(),
                                output,
                            },
                            format!("{}: {name}", study.name()),
                        )
                    })
            })
            .chain(
                self.chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| d.completed)
                    .flat_map(|d| {
                        let levels = d.alert_levels(bar);
                        let single = levels.len() == 1;
                        levels.into_iter().map(move |(level, name, _)| {
                            let drawing = format!("{} {}", d.tool_type.as_str(), d.id);
                            (
                                d.alert_source(level),
                                if single {
                                    drawing
                                } else {
                                    format!("{drawing}: {name}")
                                },
                            )
                        })
                    }),
            )
            .collect();
        self.alert_dialog.watchlists = self
            .watchlist
            .lists()
            .iter()
            .map(|list| (list.name.clone(), list.name.clone()))
            .collect();
    }

    fn quick_price_alert(&mut self, symbol: String, price: f64) {
        self.alerts
            .save(Alert::new(symbol, AlertCondition::Crossing(price)));
    }

    // --- selection / context helpers --------------------------------------

    fn open_chart_settings_dialog(&mut self) {
        let (alerts, trading) = (
            self.settings_state.alerts.clone(),
            self.trading.display.clone(),
        );
        self.settings_state = ChartSettingsState::from_chart(&self.chart);
        self.settings_state.alerts = alerts;
        self.settings_state.trading = trading;
        self.settings_state.adjust_for_dividends =
            self.timeframe_toolbar.state.adjust_for_dividends;
        self.settings_dialog.open(self.settings_state.clone());
    }

    fn open_drawing_props_for(&mut self, drawing_id: usize) {
        self.drawing_props
            .open_for_chart(&self.chart, DrawingId(drawing_id));
    }

    fn open_indicator_settings(&mut self, id: SeriesId) {
        if !self.indicator_settings.open_for_chart(&self.chart, id)
            && self.chart.series().any(|(series, _)| series == id)
        {
            self.compare_dialog.open();
        }
    }

    // --- dialog wiring -----------------------------------------------------

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.quick_search.show(ctx) {
            self.execute_search_command(ctx, &id);
        }
        self.indicator_dialog
            .set_metric_source(Some(self.live.clone()), self.chart.symbol());
        // Indicators: add the study the dialog created.
        if let IndicatorDialogAction::AddIndicator(study) =
            self.indicator_dialog.show(ctx, &self.theme)
        {
            self.chart.begin_user_action();
            let inputs = study_input_summary(study.as_ref());
            let id = self.chart.add_indicator(study);
            self.chart.set_indicator_inputs(id, inputs);
            self.chart.end_user_action();
        }

        if let Some(edit) = self.indicator_settings.show(ctx, &self.theme) {
            self.chart.begin_user_action();
            let result = self
                .chart
                .indicator_mut(edit.id)
                .ok_or("Study was removed")
                .and_then(|study| {
                    study.apply_settings(&edit.inputs, edit.settings)?;
                    Ok(study_input_summary(study))
                });
            match result {
                Ok(inputs) => {
                    self.chart.set_indicator_inputs(edit.id, inputs);
                }
                Err(error) => self.notify(error),
            }
            self.chart.end_user_action();
        }

        self.show_templates(ctx);

        // Chart settings.
        match self.settings_dialog.show(ctx, &self.theme) {
            SettingsAction::Apply(state) => self.apply_chart_settings(*state),
            SettingsAction::PreviewSound { name, volume } => {
                self.play_sound(ctx, &name, volume, true)
            }
            SettingsAction::Cancel => {}
            _ => {}
        }

        let symbols = DemoSymbol::symbols();
        self.sync_comparisons();
        let action = self.compare_dialog.show(ctx, &symbols);
        self.handle_comparison(action);
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
            AlertDialogAction::Create(alert) => self.alerts.save(*alert),
            AlertDialogAction::Cancel | AlertDialogAction::None => {}
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

        // Keyboard shortcuts help.
        let _ = self
            .keyboard_shortcuts
            .show_with_shortcuts(ctx, self.chart.shortcuts());
        self.show_interval_entry(ctx);
        self.show_open_layout(ctx);

        // Export dialog (screenshot / CSV).
        match self.export_dialog.show(ctx) {
            ExportResponse::Export { config, save_path } => self.run_export(ctx, config, save_path),
            ExportResponse::Closed | ExportResponse::None => {}
        }

        // Date picker (opened from the bottom toolbar): jump the visible window.
        match self.date_picker.show(ctx) {
            DatePickerAction::GoToDate(date) => self.go_to_date(date),
            DatePickerAction::SetRange(start, end) => self.set_date_range(start, end),
            DatePickerAction::Cancel | DatePickerAction::None => {}
            _ => {}
        }
    }

    fn apply_chart_settings(&mut self, state: ChartSettingsState) {
        let news_changed = state.events.show_news_notifications
            != self.chart.event_visibility().show_news_notifications;
        self.chart.begin_user_action();
        self.settings_state = state.clone();
        self.trading.display = state.trading.clone();
        self.alerts
            .set_notification_seconds(state.alerts.notification_seconds as f32);
        apply_settings_state(&state, &mut self.chart, &mut self.timeframe_toolbar);
        self.chart.end_user_action();
        if news_changed {
            self.annotate();
        }
        if state.adjust_for_dividends != self.timeframe_toolbar.state.adjust_for_dividends {
            self.timeframe_toolbar.state.adjust_for_dividends = state.adjust_for_dividends;
            self.stop_replay();
            self.apply_session();
        }
    }

    fn remove_all_indicators(&mut self) {
        let chart = &mut self.chart;
        let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
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

    fn set_date_range(&mut self, start: chrono::NaiveDate, end: chrono::NaiveDate) {
        let range = egui_charts::widget::VisibleTimeRange {
            from: start.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            to: end.and_hms_opt(23, 59, 59).unwrap().and_utc(),
        };
        let mut charts: Vec<_> = self.extra_charts.iter_mut().map(|c| &mut c.chart).collect();
        charts.insert(self.layout_slot, &mut self.chart);
        self.layout.apply_date_range(&mut charts, range);
    }

    fn go_to_date(&mut self, date: chrono::NaiveDate) {
        let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
        let target = self
            .bars
            .iter()
            .find(|bar| bar.time >= start)
            .or_else(|| self.bars.last())
            .map_or(start, |bar| bar.time);
        self.chart.scroll_to_time(target);
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
            ExportFormat::Csv => return self.export_csv(save_path),
        };
        self.screenshot_requested = Some((target, config.image));
        ctx.request_repaint();
    }

    fn export_csv(&mut self, save_path: Option<std::path::PathBuf>) {
        let from_dialog = save_path.is_some();
        let out = match self.chart.export_csv() {
            Ok(csv) => csv,
            Err(error) => {
                if from_dialog {
                    self.export_dialog.mark_error(error.to_string());
                }
                self.notify(format!("Export failed: {error}"));
                return;
            }
        };
        let path = save_path.unwrap_or_else(|| {
            let filename = format!("{}_export.csv", self.state.symbol);
            #[cfg(not(target_arch = "wasm32"))]
            return std::env::temp_dir().join(filename);
            #[cfg(target_arch = "wasm32")]
            std::path::PathBuf::from(filename)
        });
        #[cfg(not(target_arch = "wasm32"))]
        let result = std::fs::write(&path, out).map_err(|e| e.to_string());
        #[cfg(target_arch = "wasm32")]
        let result = browser_download(out.as_bytes(), &path);
        match result {
            Ok(()) => {
                if from_dialog {
                    self.export_dialog.mark_success();
                }
                self.notify(format!("Export successful: {}", path.display()));
            }
            Err(error) => {
                if from_dialog {
                    self.export_dialog.mark_error(error.clone());
                }
                self.notify(format!("Export failed: {error}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App: per-frame update that drives the panels, toolbars and dialogs.
// ---------------------------------------------------------------------------

impl eframe::App for DemoApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let current = self.user_settings();
        self.settings.flush(&current);
        eframe::set_value(
            storage,
            "drawing_defaults",
            &self.chart.drawings().tool_defaults,
        );
        eframe::set_value(storage, "watchlists", &self.watchlist.lists());
        self.checkpoint_replay();
        eframe::set_value(storage, "replay_session", &self.replay_session);
        self.storage.persist(storage);
        eframe::set_value(
            storage,
            "interval_preferences",
            &self.top_toolbar.timeframe_selector.preferences(),
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
        self.alerts.poll(&ctx, &mut self.toasts);
        self.monitor_alerts(&ctx);
        self.alerts.poll(&ctx, &mut self.toasts);
        let triggers = self.alert_host.take_triggers();
        for symbol in &triggers {
            self.notification_host
                .notify(format!("Alert triggered: {symbol}"), Some(symbol));
        }
        self.symbol_details.poll(&ctx);
        self.highlights.poll(&ctx);
        self.news.poll(&ctx);
        self.calendar.poll(&ctx);
        self.screener.poll(&ctx);
        self.hotlists.poll(&ctx);
        self.notifications.poll(&ctx);
        self.chats.poll(&ctx);
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
        if !triggers.is_empty() && self.settings_state.alerts.play_sound {
            let name = self.settings_state.alerts.sound.clone();
            self.play_sound(&ctx, &name, self.settings_state.alerts.sound_volume, false);
        }
        if let Some(error) = self.audio.take_error() {
            self.toasts.error(error);
        }
        self.poll_storage(&ctx);
        // The watchlist opens beside the chart by default, but a compact window starts with the
        // chart alone.
        if ctx.cumulative_frame_nr() == 0 && ctx.content_rect().width() < 600.0 {
            self.open_panel = None;
        }

        // Drive replay before laying out the chart so its window is current.
        self.advance_replay();
        if self.replay.is_none() {
            let delay = self.live.advance(chrono::Utc::now());
            if !self.live.paused() {
                ctx.request_repaint_after(delay);
            }
        }
        self.feed_market_panels(&ctx);

        for (_, _, feed) in &self.comparisons {
            feed.set_paused(self.live.paused() || self.replay.is_some());
            feed.advance(chrono::Utc::now());
        }

        for extra in &mut self.extra_charts {
            extra.live.set_symbol(extra.chart.symbol());
            let delay = extra.live.advance(chrono::Utc::now());
            for (_, id, feed) in &extra.comparisons {
                feed.set_paused(extra.live.paused());
                feed.advance(chrono::Utc::now());
                comparison::refresh(&mut extra.chart, *id, feed);
            }
            comparison::refresh_spreads(&mut extra.chart, &mut extra.spreads, extra.live.paused());
            if !extra.live.paused() {
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
        if !self.keyboard_dialog_open() {
            let action = self.replay_control.shortcut(&ctx);
            self.handle_replay_control(action);
            self.handle_keyboard(&ctx);
        }
        if self.keyboard_dialog_open() {
            self.chart.suppress_shortcuts(&ctx);
        }
        self.show_layout_tabs(ui);
        self.refresh_layout_menu();
        self.show_top_toolbar(ui, panel_bg);
        self.layout.sync = self.top_toolbar.chart_grid_selector.sync.clone();
        self.show_drawing_toolbar(ui, panel_bg);
        self.show_widget_bar(ui, panel_bg);
        self.trading.show_ticket(ui, &self.chart);
        let narrow_account = ui.available_width() < 700.0 && self.trading.is_connected();
        if narrow_account {
            self.show_account_panel(ui, panel_bg);
        }
        self.show_side_panel(ui);
        if !narrow_account {
            self.show_account_panel(ui, panel_bg);
        }
        self.show_strategy_report(ui, panel_bg);
        self.show_bottom_toolbar(ui, panel_bg);
        self.show_chart(ui, panel_bg);
        self.show_replay_control(&ctx);
        self.show_replay_start(&ctx);
        self.show_dialogs(&ctx);
        self.show_watchlist_import(&ctx);

        // Transient status notice.
        self.show_notice(ui);
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
    fn show_top_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        self.top_toolbar.sync_from_app_state(&self.state);
        self.top_toolbar.state.can_undo = self.chart.can_undo();
        self.top_toolbar.state.can_redo = self.chart.can_redo();
        let bg = if self.theme.dark {
            bg.lerp_to_gamma(self.theme.ui.text, 0.07)
        } else {
            bg
        };
        egui::Panel::top("toolbar")
            .exact_size(self.top_toolbar.config.height + 4.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                bottom: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                use egui_charts::ui_kit::buttons::IconButton;
                let mut menu_action = TopToolbarAction::None;
                let mut panel_action = None;
                let mut widget_action = None;
                let mut show_help = false;
                let mut heatmap_enabled = self.liquidity_heatmap;
                let mut paused = self.live.paused();
                let action = self.top_toolbar.show_with_slots(
                    ui,
                    &self.state,
                    |ui| {
                        let response =
                            IconButton::new(&egui_charts::ui::top_toolbar::buttons::MENU_ICON)
                                .icon_scale(24.0 / 28.0)
                                .tooltip("Menu")
                                .show(ui);
                        egui::Popup::menu(&response).show(|ui| {
                            ui.checkbox(&mut heatmap_enabled, "Liquidity heatmap");
                            ui.add_enabled(self.replay.is_none(), |ui: &mut egui::Ui| {
                                ui.checkbox(&mut paused, "Pause synthetic feed")
                            });
                            if ui.ctx().content_rect().width() < 600.0 {
                                ui.separator();
                                for (label, panel) in [
                                    ("Watchlist", SidePanel::Watchlist),
                                    ("Alerts", SidePanel::Alerts),
                                    ("Object Tree", SidePanel::ObjectTree),
                                    ("Data Window", SidePanel::DataWindow),
                                    ("Time and sales", SidePanel::TimeAndSales),
                                ]
                                .into_iter()
                                .filter(|(_, panel)| {
                                    self.api_samples || *panel != SidePanel::TimeAndSales
                                }) {
                                    if ui.button(label).clicked() {
                                        panel_action = Some(panel);
                                        ui.close();
                                    }
                                }
                                for widget in &self.widget_bar.widgets {
                                    if SidePanel::from_widget(widget.id).is_none()
                                        && ui.button(&widget.tooltip).clicked()
                                    {
                                        widget_action = Some(widget.id);
                                        ui.close();
                                    }
                                }
                                if ui.button("Keyboard shortcuts").clicked() {
                                    show_help = true;
                                    ui.close();
                                }
                                ui.separator();
                            }
                            // Host actions live in the menu so the default strip keeps the
                            // built-in controls only.
                            for (label, action) in [
                                ("Quick search", TopToolbarAction::OpenQuickSearch),
                                ("Switch theme", TopToolbarAction::ThemeToggle),
                                ("Research", TopToolbarAction::Host("research".into())),
                                ("Copy symbol", TopToolbarAction::Host("copy-symbol".into())),
                            ] {
                                if ui.button(label).clicked() {
                                    menu_action = action;
                                    ui.close();
                                }
                            }
                        });
                    },
                    |_| {},
                );
                if let Some(panel) = panel_action {
                    self.open_panel = (self.open_panel != Some(panel)).then_some(panel);
                    self.widget_bar.active_widget = None;
                }
                if let Some(id) = widget_action {
                    self.open_panel = None;
                    self.widget_bar.active_widget =
                        (self.widget_bar.active_widget != Some(id)).then_some(id);
                }
                if show_help {
                    self.keyboard_shortcuts.toggle();
                }
                self.live.set_paused(paused);
                if heatmap_enabled != self.liquidity_heatmap {
                    self.liquidity_heatmap = heatmap_enabled;
                    self.chart.set_liquidity_heatmap(heatmap_enabled);
                    self.chart
                        .set_volume_bubbles(heatmap_enabled.then(Default::default));
                }
                self.handle_top_toolbar(ui.ctx(), action);
                self.handle_top_toolbar(ui.ctx(), menu_action);
            });
    }

    pub(crate) fn handle_top_toolbar(&mut self, ctx: &egui::Context, action: TopToolbarAction) {
        match action {
            TopToolbarAction::Host(id) => {
                if id == "copy-symbol" {
                    ctx.copy_text(self.chart.symbol().to_string());
                }
                if id == "research" {
                    if let Some(click) = &self.last_chart_click {
                        self.toasts.info(format!(
                            "Host action: {id}, last chart click: {}, price: {:.4}",
                            click.time.to_rfc3339(),
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
                if !self.trading.is_connected() {
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
            _ => {}
        }

        self.user_action(|app| app.handle_top_toolbar_edit(ctx, action));
    }

    fn handle_top_toolbar_edit(&mut self, ctx: &egui::Context, action: TopToolbarAction) {
        match action {
            TopToolbarAction::ChartStyleSelected(ct) => {
                self.state.chart_type = ct;
                self.chart.set_chart_type(ct);
                self.size_price_based_charts();
            }
            TopToolbarAction::IntervalSelected(tf) => self.switch_timeframe(tf),
            TopToolbarAction::OpenSymbolSearch => self.symbol_search.open(),
            TopToolbarAction::OpenCompare => self.compare_dialog.open(),
            TopToolbarAction::OpenQuickSearch => self.quick_search.open(),
            TopToolbarAction::OpenIndicators => {
                self.indicator_dialog.open_for_chart(&self.chart, None);
            }
            TopToolbarAction::CreateAlert => self.open_alert(None),
            TopToolbarAction::ToggleReplay => self.toggle_replay(),
            TopToolbarAction::OpenSettings => {
                self.open_chart_settings_dialog();
            }
            TopToolbarAction::TakeScreenshot
            | TopToolbarAction::CopyImage
            | TopToolbarAction::OpenImage => {
                self.export_dialog.close();
                self.toasts.clear();
                let target = match action {
                    TopToolbarAction::CopyImage => ScreenshotTarget::Clipboard,
                    TopToolbarAction::OpenImage => ScreenshotTarget::NewTab,
                    _ => ScreenshotTarget::File(None),
                };
                self.screenshot_requested = Some((target, {
                    let mut options = egui_charts::widget::ImageExportOptions::default();
                    options.include_crosshair = true;
                    options
                }));
                ctx.request_repaint();
            }
            TopToolbarAction::ToggleFullscreen => {
                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
            }
            TopToolbarAction::Undo | TopToolbarAction::Redo => {}
            TopToolbarAction::AddIndicator(name) => {
                if let Some(study) = self.indicator_dialog.factory().create(&name) {
                    self.chart.begin_user_action();
                    let inputs = study_input_summary(study.as_ref());
                    let id = self.chart.add_indicator(study);
                    self.chart.set_indicator_inputs(id, inputs);
                    self.chart.end_user_action();
                }
            }
            TopToolbarAction::SaveIndicatorTemplate => {
                self.study_templates.open(self.current_study_template());
            }
            TopToolbarAction::ApplyIndicatorTemplate(id) => {
                if let Some(template) = self.study_templates.templates.entries().get(&id).cloned() {
                    self.load_study_template(&template);
                }
            }
            // Light/dark toggle is a real, cheap visual change.
            TopToolbarAction::ThemeToggle => self.toggle_theme(ctx),
            TopToolbarAction::Layout(_)
            | TopToolbarAction::Save
            | TopToolbarAction::ToggleTrading
            | TopToolbarAction::Host(_)
            | TopToolbarAction::None => {}
            _ => {}
        }
    }

    fn show_drawing_toolbar(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        if self.chart.zoom_was_applied() {
            self.drawing_toolbar.state.zoom_mode_active = false;
        }
        self.drawing_toolbar.state.has_zoom_history = self.chart.state().has_zoom_history();
        use egui_charts::config::CrosshairStyle;
        use egui_charts::ui::drawing_toolbar::{
            DrawingToolbarDomainState, MagnetType, categories::cursors::CursorType,
        };
        let dm = self.chart.drawings();
        let magnet_type = if !dm.options.strong_magnet {
            MagnetType::Weak
        } else if self.drawing_toolbar.state.magnet_type == MagnetType::OHLC {
            // OHLC is an alias of strong snapping; retain the user's menu choice.
            MagnetType::OHLC
        } else {
            MagnetType::Strong
        };
        self.drawing_toolbar
            .sync_from_domain_state(&DrawingToolbarDomainState {
                sel_tool: dm.active_tool,
                magnet_mode: dm.options.magnet_mode,
                snap_to_indicators: dm.options.snap_to_indicators,
                magnet_type,
                current_cursor_type: match self.chart.chart_options().crosshair.style {
                    CrosshairStyle::Full => CursorType::Cross,
                    CrosshairStyle::Dot => CursorType::Dot,
                    CrosshairStyle::Arrow | CrosshairStyle::Hidden => CursorType::Arrow,
                    CrosshairStyle::Demonstration => CursorType::Demonstration,
                },
                eraser_mode_enabled: dm.eraser_mode,
                stay_in_drawing_mode: dm.stay_in_drawing_mode,
                drawing_color: dm.options.default_color,
            });
        let drawings = &self.chart.drawings().drawings;
        self.drawing_toolbar.state.lock_all =
            !drawings.is_empty() && drawings.iter().all(|d| d.locked);
        egui::Panel::left("drawing_toolbar")
            .resizable(false)
            .exact_size(54.0)
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
        match action {
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

    fn handle_drawing_toolbar_edit(&mut self, action: DrawingToolbarAction) {
        // Variants that touch the indicator registry (not the drawing manager)
        // or surface a notice are handled first, before the `dm` borrow.
        match action {
            DrawingToolbarAction::ManageTemplates => {
                if let Some(template) = self.current_drawing_template() {
                    self.drawing_templates.open(template);
                } else {
                    self.drawing_templates.open_catalog();
                }
                return;
            }
            DrawingToolbarAction::SaveTemplate { name } => {
                let Some(template) = self.current_drawing_template() else {
                    self.notify("Select a drawing first");
                    return;
                };
                if let Err(error) = self.drawing_templates.templates.save(&name, template) {
                    self.notify(error);
                }
                return;
            }
            DrawingToolbarAction::LoadTemplate { template_id } => {
                if let Some(template) = self
                    .drawing_templates
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
                let chart = &mut self.chart;
                let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
                for id in ids {
                    chart.set_indicator_visible(id, false);
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
                });
                self.chart.drawings_mut().set_active_tool(None);
                self.chart
                    .drawings_mut()
                    .set_eraser_mode(cursor == CursorType::Eraser);
                return;
            }
            DrawingToolbarAction::ToggleValuesTooltip => {
                self.chart
                    .set_tooltip_enabled(self.drawing_toolbar.state.values_tooltip_on_long_press);
                return;
            }
            _ => {}
        }

        let dm = self.chart.drawings_mut();
        match action {
            DrawingToolbarAction::SelectTool(tool) => dm.set_active_tool(Some(tool)),
            DrawingToolbarAction::ToggleMagnet => {
                dm.options.magnet_mode = !dm.options.magnet_mode;
                dm.options.strong_magnet = self.drawing_toolbar.state.magnet_type
                    != egui_charts::ui::drawing_toolbar::MagnetType::Weak;
                dm.update_snap_options();
                let mode = if dm.options.magnet_mode {
                    egui_charts::config::CrosshairMode::Magnet
                } else {
                    egui_charts::config::CrosshairMode::Normal
                };
                self.chart.set_crosshair_mode(mode);
            }
            DrawingToolbarAction::SetSnapToIndicators(enabled) => {
                dm.options.snap_to_indicators = enabled;
            }
            DrawingToolbarAction::SetMagnetType(kind) => {
                dm.options.magnet_mode = true;
                dm.options.strong_magnet =
                    kind != egui_charts::ui::drawing_toolbar::MagnetType::Weak;
                dm.update_snap_options();
                self.chart
                    .set_crosshair_mode(egui_charts::config::CrosshairMode::Magnet);
            }
            DrawingToolbarAction::ToggleEraserMode => {
                let on = !dm.eraser_mode;
                dm.set_eraser_mode(on);
            }
            DrawingToolbarAction::ToggleStayInDrawingMode => {
                dm.stay_in_drawing_mode = !dm.stay_in_drawing_mode;
            }
            DrawingToolbarAction::ColorChanged(rgba) => dm.options.default_color = rgba,
            DrawingToolbarAction::ClearAllDrawings | DrawingToolbarAction::RemoveAllDrawings => {
                dm.clear_all()
            }
            DrawingToolbarAction::HideAllDrawings => dm.toggle_visibility_all(),
            DrawingToolbarAction::LockAllDrawings => dm.toggle_lock_all(),
            DrawingToolbarAction::InsertGlyph { icon_name, size } => {
                dm.start_icon_insertion(icon_name.to_string());
                if let Some(drawing) = &mut dm.curr_drawing {
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
            | DrawingToolbarAction::HideAllPoss
            | DrawingToolbarAction::HideAllOrders
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
            .exact_size(49.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                left: 4,
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

    fn toggle_panel(&mut self, icon: RightBarIcon) {
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
        self.widget_bar.show_panel(ui);
        let Some(which) = self.open_panel else {
            return;
        };
        egui::Panel::right("detail_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(220.0)
            .show(ui, |ui| match which {
                SidePanel::ObjectTree => self.show_object_tree(ui),
                SidePanel::Alerts => self.show_alerts_panel(ui),
                SidePanel::Watchlist => {
                    if ui.available_height() >= 400.0 {
                        egui::Panel::bottom("symbol_details")
                            .resizable(true)
                            .default_size(ui.available_height() / 2.0)
                            .min_size(120.0)
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| self.symbol_details.show(ui));
                    }
                    self.show_watchlist(ui);
                }
                SidePanel::TimeAndSales => self.time_and_sales.show(ui),
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
                SidePanel::Hotlists => {
                    if let HotlistsAction::SelectSymbol(symbol) = self.hotlists.show(ui) {
                        self.open_catalog_symbol(&symbol);
                    }
                }
                SidePanel::Notifications => {
                    if let NotificationsAction::SelectSymbol(symbol) = self.notifications.show(ui) {
                        self.open_catalog_symbol(&symbol);
                    }
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
                    self.data_window_expanded =
                        egui_charts::ui::widget_bar::panels::object_tree::show_data_window(
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
        let action = self.object_tree.show(ui, Some(&data_window), &mut sources);
        self.handle_object_tree(action, &sources);
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

        // Live indicators.
        for (id, ind) in self.chart.indicators() {
            let color = ind.colors().first().copied().unwrap_or(egui::Color32::GRAY);
            let mut item =
                SourceItem::indicator(indicator_tree_id(id), ind.name(), color).with_series_id(id);
            item.visible = ind.is_visible();
            items.push(item);
        }

        // Live drawings.
        for d in self.chart.drawings().listed_drawings() {
            let mut item =
                SourceItem::drawing(d.id, d.tool_type, d.color32()).with_panes(&self.chart);
            item.visible = d.visible;
            item.locked = d.locked;
            item.z_index = d.z_order;
            item.label = d.label.clone();
            item.folder = d.folder.clone();
            items.push(item);
        }
        items
    }

    /// Reads the hovered values from the crosshair payload alone; off the bars, the chart's
    /// own readout shows the latest bar.
    fn build_data_window(&self) -> DataWindowInfo {
        let Some((pointer, bar)) = self
            .crosshair
            .as_ref()
            .and_then(|pointer| Some((pointer, pointer.bar.as_ref()?)))
        else {
            return DataWindowInfo::from_chart(&self.chart);
        };
        let mut info = DataWindowInfo::new(self.chart.symbol()).with_timeframe(
            self.chart
                .timeframe()
                .map(|tf| tf.as_str())
                .unwrap_or_default(),
        );
        if let Some(instrument) = self.chart.instrument() {
            info = info.with_instrument(instrument);
        }
        info = info
            .with_ohlcv(bar.open, bar.high, bar.low, bar.close, bar.volume)
            .with_timestamp(self.chart.format_time(bar.time))
            .with_prev_close(pointer.previous_close.unwrap_or(bar.open));
        info.studies = pointer.studies.iter().map(|(_, row)| row.clone()).collect();
        info
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
                    if let Some(ind) = chart.indicator_mut(study) {
                        ind.set_color(0, color);
                    }
                } else {
                    chart
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
    /// and sales the executions of the live feed and reports matching filters as notifications.
    fn feed_market_panels(&mut self, ctx: &egui::Context) {
        // The simulation clock rather than the wall clock, which the pause freezes: a paused
        // demo must be able to go idle instead of repainting for a market that stands still.
        let now = self.live.now();
        for entry in &mut self.market_heatmap.entries {
            if let Some(quote) = live::watchlist_quote(&entry.symbol, now) {
                entry.change_percent = (quote.last - quote.prev_close) / quote.prev_close * 100.0;
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
        }
        self.watchlist.set_selected(&selected);
        self.quotes.publish(now);
        self.watchlist.poll(ctx);
        self.symbol_details.set_instrument(
            &self
                .chart
                .instrument()
                .cloned()
                .unwrap_or_else(|| egui_charts::model::Instrument::new(&chart_symbol)),
        );
        let quoted = (self.watchlist.symbols())
            .find(|symbol| symbol.eq_ignore_ascii_case(&chart_symbol))
            .unwrap_or(&chart_symbol);
        if let Some(quote) = self.watchlist.quote(quoted).cloned() {
            self.symbol_details.set_quote(quote.clone());
            self.trading.set_quote(&chart_symbol, quote);
        }
        self.symbol_details
            .set_average_volume(self.live.daily_volume().1);
        // Open financials follow the chart's symbol.
        if self.symbol_details.take_financials_request()
            || (self.financials.as_ref()).is_some_and(|(symbol, _)| *symbol != chart_symbol)
        {
            let title = (self.chart.instrument())
                .and_then(|instrument| instrument.description.clone())
                .unwrap_or_else(|| chart_symbol.clone());
            let panel = egui_charts::ui::financials::FinancialsPanel::new(
                title,
                feeds::financials(&chart_symbol),
            );
            self.open_panel = None;
            self.financials = Some((chart_symbol.clone(), panel));
        }
        let timezone = self.timeframe_toolbar.state.timezone.parse();
        self.symbol_details
            .set_timezone(timezone.unwrap_or_default());
        self.highlights.set_symbol(&chart_symbol);
        self.news.set_symbol(&chart_symbol);
        let instrument = self.chart.instrument();
        let mut alert = None;
        for trade in self.live.drain_tape() {
            if self.time_and_sales.push(trade.clone()).is_some() {
                alert = Some(trade);
            }
        }
        if let Some(trade) = alert {
            let decimals = instrument.and_then(|i| i.price_decimals).unwrap_or(2);
            self.toasts.info(format!(
                "Large trade: {} at {:.*}",
                trade.size,
                usize::from(decimals),
                trade.price
            ));
        }
        let config = &mut self.time_and_sales.config;
        config.price_decimals = instrument.and_then(|instrument| instrument.price_decimals);
        config.size_decimals = instrument.and_then(|instrument| instrument.volume_decimals);
        config.timezone = self
            .timeframe_toolbar
            .state
            .timezone
            .parse()
            .unwrap_or_default();
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
        if DemoSymbol::catalog()
            .iter()
            .any(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
        {
            self.switch_symbol(symbol);
        } else {
            self.notify(format!("{symbol} has no synthetic data in this demo"));
        }
    }

    fn show_watchlist(&mut self, ui: &mut egui::Ui) {
        match self.watchlist.show(ui) {
            WatchlistAction::SelectSymbol(symbol) => self.open_catalog_symbol(&symbol),
            WatchlistAction::AddSymbol => {
                self.watchlist_search = true;
                self.symbol_search.open();
            }
            WatchlistAction::AdvancedView => {
                self.notify("The advanced watchlist view is not part of this demo")
            }
            WatchlistAction::Import => self.watchlist_import = Some(String::new()),
            WatchlistAction::Export { text, .. } => self.save_text("watchlist.txt", &text),
            WatchlistAction::None => {}
            _ => {}
        }
    }

    /// The demo has no file picker for imports: the user pastes the exported text instead.
    fn show_watchlist_import(&mut self, ctx: &egui::Context) {
        let Some(text) = &mut self.watchlist_import else {
            return;
        };
        let mut open = true;
        let mut import = false;
        egui::Window::new("Import watchlist")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Paste symbols separated by commas; ###Title starts a section.");
                ui.add(egui::TextEdit::multiline(text).desired_rows(6));
                import = ui.button("Import").clicked();
            });
        if import {
            self.watchlist.import("Imported", text);
        }
        if import || !open {
            self.watchlist_import = None;
        }
    }

    fn show_alerts_panel(&mut self, ui: &mut egui::Ui) {
        match self.alerts.show(ui) {
            AlertPanelAction::CreateAlert => self.open_alert(None),
            AlertPanelAction::EditAlert(alert) => {
                self.open_alert(None);
                self.alert_dialog.edit(*alert);
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

    /// The account panel of the connected broker, under the chart and its interval bar.
    fn show_account_panel(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        use egui_charts::ui::trading::{AccountPanelAction, BrokerPickerAction, BrokerTile};

        match self.broker_picker.show(ui.ctx()) {
            Some(BrokerPickerAction::Connect(id)) if id == BrokerTile::PAPER => {
                self.trading.set_service(self.paper.clone());
                self.trading.account_panel.broker = "Paper Trading".to_owned();
                self.trading.account_panel.accounts = vec!["Paper 1".to_owned()];
                self.account_panel = AccountPanelSize::Normal;
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
                "Close every position, cancel every order and return the balance to 100,000 USD?",
            )
            .confirm_text("Reset")
            .show(ui.ctx(), &mut self.paper_reset);
            if result.is_confirmed() {
                self.paper.reset();
                self.trading.set_service(self.paper.clone());
                self.paper_reset = false;
            }
        }
        if !self.trading.is_connected() || self.account_panel == AccountPanelSize::Closed {
            return;
        }
        let height = match self.account_panel {
            AccountPanelSize::Minimized => 42.0,
            AccountPanelSize::Maximized => (ui.available_height() - 160.0).max(320.0),
            _ => 320.0,
        };
        let height = height.min((ui.available_height() - 96.0).max(42.0));
        let symbol = self.chart.symbol().to_owned();
        let trading = &mut self.trading;
        let action = egui::Panel::bottom("account_panel")
            .exact_size(height)
            .show_separator_line(false)
            .frame(panel_frame(bg).corner_radius(4).outer_margin(egui::Margin {
                top: 1,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| trading.show_account_panel(ui, &symbol))
            .inner;
        match action {
            Some(AccountPanelAction::SelectSymbol(symbol)) => self.open_catalog_symbol(&symbol),
            Some(AccountPanelAction::Disconnect) => self.trading.disconnect(),
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
                let action = self.timeframe_toolbar.show(ui);
                self.handle_timeframe_toolbar(action);
                self.show_timeframe_menus(ui.ctx());
            });
    }

    fn handle_timeframe_toolbar(&mut self, action: TimeframeToolbarAction) {
        match action {
            TimeframeToolbarAction::DateRangeChanged(range) => {
                let visible = visible_bars_for_range(range, self.chart.bars(SeriesId::MAIN));
                let width = self.chart.get_rendered_price_rect().width();
                // Include the latest slot in the scale's exclusive data range.
                let spacing = width.max(1.0) / (visible.max(1) as f32 + 0.5);
                self.chart.disable_tracking_mode();
                self.chart.apply_synced_time_scale(spacing, 0.5);
                if self.layout.sync.sync_date_range
                    && let Some(range) = self.chart.visible_time_range()
                {
                    let mut charts: Vec<_> =
                        self.extra_charts.iter_mut().map(|c| &mut c.chart).collect();
                    charts.insert(self.layout_slot, &mut self.chart);
                    self.layout.apply_date_range(&mut charts, range);
                }
            }
            TimeframeToolbarAction::OpenDatePicker => self.date_picker.open(),
            TimeframeToolbarAction::OpenTimezoneMenu => {
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
                    ui.label(
                        "SAMPLE: weekdays, regular 14:30–21:00 UTC; extended 08:30–21:00 UTC.",
                    );
                });
            self.timeframe_toolbar.state.session_menu_open &= open;
        }
        if self.timeframe_toolbar.state.timezone_menu_open {
            let mut open = true;
            egui::Window::new("Chart timezone")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    for label in ["UTC", "America/New_York"] {
                        if ui
                            .selectable_label(self.timeframe_toolbar.state.timezone == label, label)
                            .clicked()
                        {
                            self.user_action(|app| {
                                apply_timezone(&mut app.chart, &mut app.timeframe_toolbar, label)
                            });
                            self.timeframe_toolbar.state.timezone_menu_open = false;
                        }
                    }
                });
            self.timeframe_toolbar.state.timezone_menu_open &= open;
        }
    }

    fn show_chart(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        if let Some((_, financials)) = &mut self.financials {
            let frame = panel_frame(bg).corner_radius(4).inner_margin(16);
            let back = egui::CentralPanel::default()
                .frame(frame)
                .show(ui, |ui| financials.show(ui));
            if back.inner {
                self.financials = None;
            }
            return;
        }
        let chart = egui::CentralPanel::default()
            .frame(panel_frame(bg).corner_radius(4))
            .show(ui, |ui| {
                if self.api_samples
                    && let Some(pointer) = &self.pointer_readout
                {
                    Self::show_pointer_readout(ui, pointer);
                }
                let mut charts: Vec<_> =
                    self.extra_charts.iter_mut().map(|c| &mut c.chart).collect();
                charts.insert(self.layout_slot, &mut self.chart);
                self.layout.show(ui, &mut charts);
                self.highlight_strategy_trade(ui);
                let mapping = self.chart.get_chart_mapping();
                if self.settings_state.alerts.show_alerts {
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
                        self.alerts
                            .alerts()
                            .iter()
                            .filter(|a| a.symbol == self.chart.symbol() && a.watchlist.is_none()),
                        self.settings_state.alerts.alert_color,
                        self.settings_state.alerts.show_alert_labels,
                        self.settings_state.alerts.show_active_only,
                        |price| format!("{price:.decimals$} {currency}"),
                    ) {
                        self.alerts.save(alert);
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
                if self.chart.take_event_click().is_some() {
                    self.notify("Synthetic session: generated example data");
                }
                if let Some(bar) = self
                    .chart
                    .bars(SeriesId::MAIN)
                    .iter()
                    .rfind(|bar| !bar.is_whitespace())
                {
                    self.paper.set_price(self.chart.symbol(), bar.close);
                    if self.replay.is_some() {
                        // Replay has no historical spread; its close prices both broker and ticket.
                        self.trading.set_quote(
                            self.chart.symbol(),
                            egui_charts::model::Quote::new(bar.close, bar.close, bar.time),
                        );
                    }
                }
                if let Some(side) = self.chart.take_quote_action() {
                    if !self.trading.is_connected() {
                        self.broker_picker.open();
                    } else if side == egui_charts::widget::QuoteSide::Ticket {
                        self.trading
                            .open_ticket(egui_charts::model::OrderRequest::new(
                                self.chart.symbol(),
                                egui_charts::model::OrderSide::Buy,
                                self.trading.quantity,
                                egui_charts::model::OrderType::Market,
                            ));
                    } else {
                        let side = match side {
                            egui_charts::widget::QuoteSide::Buy => {
                                egui_charts::model::OrderSide::Buy
                            }
                            egui_charts::widget::QuoteSide::Sell => {
                                egui_charts::model::OrderSide::Sell
                            }
                            egui_charts::widget::QuoteSide::Ticket => unreachable!(),
                        };
                        self.trading.trade(self.chart.symbol().to_owned(), side);
                    }
                }
                self.trading.show(ui, &mut self.chart);
            });
        self.replay_control.set_dock_rect(chart.response.rect);
        if !self.replay_selecting && self.replay.is_none() {
            self.activate_chart_slot(self.layout.active());
        }
        self.drain_chart_events(ui.ctx());
        self.menus
            .set_order_entries(self.trading.quantity, self.trading.capabilities());
        if let Some(action) = self.menus.show(ui.ctx(), &mut self.chart) {
            self.handle_context_action(ui.ctx(), action);
        }
    }

    fn drain_chart_events(&mut self, ctx: &egui::Context) {
        let mut replay_click = None;
        let mut open_settings = false;
        for event in self.chart.drain_events() {
            match event {
                egui_charts::widget::ChartEvent::CrosshairMoved(pointer) => {
                    if pointer.is_some() {
                        self.pointer_readout = pointer.clone();
                    }
                    self.crosshair = pointer;
                    // The data window was drawn before the chart; show the new values now.
                    ctx.request_repaint();
                }
                egui_charts::widget::ChartEvent::Click { position, button } => {
                    if button == egui::PointerButton::Primary && position.series == SeriesId::MAIN {
                        replay_click.get_or_insert(position.time);
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
                egui_charts::widget::ChartEvent::SymbolChanged(_)
                | egui_charts::widget::ChartEvent::TimeframeChanged(_) => {
                    self.last_chart_click = None;
                    self.pointer_readout = None;
                }
                _ => {}
            }
        }
        if let Some(time) = replay_click {
            self.handle_replay_bar_click(ctx, time);
        }
        if open_settings && !self.replay_selecting {
            self.open_chart_settings_dialog();
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
                ui.label(pointer.time.to_rfc3339());
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
            ChartContextAction::Chart(a) => self.handle_chart_context(a),
            ChartContextAction::Series(a) => self.handle_series_context(a),
            ChartContextAction::Study { id, action } => self.handle_study_context(ctx, id, action),
            ChartContextAction::Drawing(a) => self.handle_drawing_context(a),
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
            StudyContextMenuAction::AddAlert(alert) => self.alerts.save(*alert),
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
            ContextMenuAction::OpenSeriesSettings => self.open_chart_settings_dialog(),
            ContextMenuAction::ObjectTree => self.open_panel = Some(SidePanel::ObjectTree),
            ContextMenuAction::RemoveAllIndicators => self.remove_all_indicators(),
            ContextMenuAction::AddAlert { symbol, price } => self.quick_price_alert(symbol, price),
            ContextMenuAction::ResetChart => self.chart.reset_chart_view(),
            ContextMenuAction::ExportCsv => self.export_csv(None),
            ContextMenuAction::Order(request) => self.trading.open_ticket(request),
            _ => {}
        }
    }

    fn handle_series_context(&mut self, action: SeriesContextMenuAction) {
        match action {
            SeriesContextMenuAction::OpenSettings => self.open_chart_settings_dialog(),
            SeriesContextMenuAction::AddIndicator => {
                self.indicator_dialog.open_for_chart(&self.chart, None);
            }
            SeriesContextMenuAction::AddAlert => self.open_alert(None),
            _ => {}
        }
    }

    fn handle_drawing_context(&mut self, action: DrawingContextMenuAction) {
        self.user_action(|app| app.handle_drawing_context_edit(action));
    }

    fn handle_drawing_context_edit(&mut self, action: DrawingContextMenuAction) {
        // The library selects the right-clicked drawing, so the selected id is
        // the menu's target.
        let Some(id) = self.chart.drawings().sel_drawing() else {
            return;
        };
        let dm = self.chart.drawings_mut();
        match action {
            DrawingContextMenuAction::Delete => dm.delete_selected(),
            DrawingContextMenuAction::Edit => self.open_drawing_props_for(id),
            DrawingContextMenuAction::Clone => self.clone_drawing(id),
            DrawingContextMenuAction::Lock => dm.edit_drawing(id, |d| d.locked = !d.locked),
            DrawingContextMenuAction::Hide => dm.edit_drawing(id, |d| d.visible = !d.visible),
            DrawingContextMenuAction::BringToFront => dm.bring_to_front(id),
            DrawingContextMenuAction::SendToBack => dm.send_to_back(id),
            DrawingContextMenuAction::MoveTo(pane) => {
                self.chart.move_drawing_to_pane(id, pane);
            }
            DrawingContextMenuAction::AddAlert(alert) => {
                self.list_alert_sources();
                self.alert_dialog.edit(*alert);
            }
            DrawingContextMenuAction::None => {}
            _ => {}
        }
    }

    fn show_replay_control(&mut self, ctx: &egui::Context) {
        let Some(replay) = self.replay.as_ref() else {
            return;
        };
        self.replay_control.sync_from(
            replay.state.is_active(),
            replay.state.is_playing(),
            replay.state.speed.multiplier(),
            replay.state.curr_bar,
            replay.state.total_bars,
        );
        let action = self.replay_control.show(ctx);
        if action != ReplayControlAction::None {
            ctx.request_repaint();
        }
        self.handle_replay_control(action);
    }

    fn handle_replay_control(&mut self, action: ReplayControlAction) {
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
            ReplayControlAction::Stop => self.stop_replay(),
            ReplayControlAction::Close => {
                self.replay_control.dismiss();
            }
            ReplayControlAction::None => return,
            _ => {}
        }
        self.push_replay_window_to_chart();
    }

    fn advance_replay(&mut self) {
        let Some(replay) = self.replay.as_mut() else {
            return;
        };
        replay.update();
        self.push_replay_window_to_chart();
    }

    fn keyboard_dialog_open(&self) -> bool {
        self.keyboard_shortcuts.is_open
            || self.quick_search.is_open()
            || self.symbol_search.is_open()
            || self.compare_dialog.is_open
            || self.indicator_dialog.is_open()
            || self.settings_dialog.is_open
            || self.drawing_props.is_open
            || self.export_dialog.is_open()
            || self.alert_dialog.is_open
            || self.study_templates.is_open
            || self.drawing_templates.is_open
            || self.indicator_settings.is_open()
            || self.interval_entry.is_some()
            || self.open_layout
            || self.date_picker.is_open
            || self.replay_start.is_open
            || self.replay_selecting
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        use egui_charts::config::KeyboardAction as A;
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
                ShortcutRequest::Action(action) => match action {
                    A::OpenQuickSearch => self.quick_search.open(),
                    A::ShowKeyboardHelp => self.keyboard_shortcuts.toggle(),
                    A::OpenIndicatorSearch => {
                        self.indicator_dialog.open_for_chart(&self.chart, None);
                    }
                    A::OpenSymbolSearch => self.symbol_search.open(),
                    A::OpenIntervalSearch => {
                        self.interval_entry = Some(String::new());
                        self.interval_focus = true;
                    }
                    A::GoToDate => self.date_picker.open(),
                    A::Snapshot => self.handle_top_toolbar(ctx, TopToolbarAction::TakeScreenshot),
                    A::SaveLayout => self.handle_top_toolbar(ctx, TopToolbarAction::Save),
                    A::OpenLayout => self.open_layout = true,
                    A::OpenDataWindow => self.open_panel = Some(SidePanel::DataWindow),
                    A::ToggleSession if self.state.symbol == "SAMPLE" => {
                        let session =
                            if self.timeframe_toolbar.state.session_type == SessionType::RTH {
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
                            {
                                let price = mapping.y_to_price(pos.y);
                                if price.is_finite() {
                                    self.quick_price_alert(self.chart.symbol().to_owned(), price);
                                }
                            }
                        } else {
                            self.open_alert(None);
                        }
                    }
                    A::MaximizeChart => {
                        self.handle_top_toolbar(ctx, TopToolbarAction::ToggleFullscreen)
                    }
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
                    A::CycleChartFocus | A::CycleChartFocusBack => {
                        let count = self
                            .layout
                            .mode
                            .chart_count()
                            .min(self.extra_charts.len() + 1);
                        let delta = if action == A::CycleChartFocus {
                            1
                        } else {
                            count - 1
                        };
                        self.activate_chart_slot((self.layout_slot + delta) % count);
                        self.layout.set_active(self.layout_slot);
                    }
                    _ => {}
                },
                _ => {}
            }
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
                .paste_indicator(text, &egui_charts::studies::IndicatorFactory::new())
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
        let Some((target, options)) = self.screenshot_requested.take() else {
            return;
        };
        let result = self
            .trading
            .export_png(&self.chart, ctx, &options)
            .and_then(|png| match target {
                ScreenshotTarget::File(path) => self.save_screenshot(&png, path),
                ScreenshotTarget::NewTab => open_image(ctx, &png),
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
        let path = path.unwrap_or_else(|| {
            let filename = format!("{}_chart.png", self.state.symbol);
            #[cfg(not(target_arch = "wasm32"))]
            return std::env::temp_dir().join(filename);
            #[cfg(target_arch = "wasm32")]
            std::path::PathBuf::from(filename)
        });
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

fn panel_frame(bg: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .corner_radius(4)
        .fill(bg)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::ZERO)
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
    if chart.chart_options().time_scale.timezone != timezone {
        let mut options = chart.chart_options().clone();
        options.time_scale.timezone = timezone;
        // Preserve the current pan and zoom when applying display options.
        let (spacing, offset) = chart.get_time_scale_state();
        let previous = std::mem::replace(chart, Chart::new(BarData::new()));
        *chart = previous.with_chart_options(options);
        chart.apply_synced_time_scale(spacing, offset);
    }
    toolbar.set_timezone(name.to_owned());
}

/// Apply display settings to the live chart and its clock.
fn apply_settings_state(
    state: &ChartSettingsState,
    chart: &mut Chart,
    toolbar: &mut TimeframeToolbar,
) {
    if state.apply_to_chart(chart).is_ok() {
        toolbar.set_timezone(state.timezone.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::model::LineStyle;
    use egui_charts::ui::dialogs::DrawingProps;
    use egui_charts::ui::timeframe_toolbar::DateRange;

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
        app.trading.display.show_execution_labels = true;
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
                marked |= has_text("1 @ 100.00") && has_text("1 @ 110.00");
                if pass == 2 {
                    for toast in app.toasts.toasts().iter().rev().take(5) {
                        let rect = ctx
                            .memory(|m| m.area_rect(egui::Id::unique("toast").with(toast.id)))
                            .expect("trade notice");
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

    #[test]
    fn chart_click_selects_research_and_the_replay_start() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new(&cc);
        let frame = |app: &mut DemoApp, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
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
        app.replay_selecting = true;
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
        assert_eq!(app.last_chart_click.as_ref().unwrap().time, time);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Host("research".into()));
        let message = &app.toasts.toasts().last().unwrap().message;
        assert!(message.contains(&time.to_rfc3339()), "{message}");
        assert!(
            message.contains(&format!("price: {:.4}", map.y_to_price(pos.y))),
            "{message}"
        );
        app.switch_symbol("AAPL");
        frame(&mut app, vec![]);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Host("research".into()));
        assert_eq!(
            app.toasts.toasts().last().unwrap().message,
            "Click the chart to choose a research time and price"
        );
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
        eframe::App::save(&mut app, &mut storage);
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
        restored.switch_symbol("AAPL");
        assert_eq!(
            restored.chart.drawings().tool_defaults[&DrawingToolType::TrendLine]
                .settings
                .stroke_width,
            4.0
        );
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
        let mut storage = AppStorage::default();
        eframe::App::save(&mut app, &mut storage);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut restored = DemoApp::new(&cc);
        assert_eq!(
            restored.replay_session,
            Some((saved.clone(), Timeframe::Hour1))
        );
        restored.replay_start.open(None);
        let mut resume = egui::Pos2::ZERO;
        for pass in 0..5 {
            let events = if pass >= 3 {
                vec![
                    egui::Event::PointerMoved(resume),
                    egui::Event::PointerButton {
                        pos: resume,
                        button: egui::PointerButton::Primary,
                        pressed: pass == 3,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                vec![]
            };
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| restored.show_replay_start(ui.ctx()),
            );
            if pass == 2 {
                resume = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Resume saved session" => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(ctx.content_rect().contains(resume));
            }
            output.drop_without_applying_deltas();
        }
        assert_eq!(restored.chart.replay_time(), Some(cutoff));
        assert!(!restored.replay.as_ref().unwrap().state.is_playing());
    }

    #[derive(Default)]
    struct AppStorage(std::collections::HashMap<String, String>);

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

    #[test]
    fn theme_selection_survives_restart_and_initializes_the_chart() {
        use eframe::App;
        for initial in [None, Some(egui::Theme::Light), Some(egui::Theme::Dark)] {
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
            for explicit in [false, true, true] {
                let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
                cc.storage = Some(&storage);
                let mut app = DemoApp::new(&cc);
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
        assert!(app.drawing_templates.templates.entries().is_empty());
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
        app.show_templates(&ctx);
        app.settings_dialog
            .templates
            .save("Canvas", ChartSettingsState::from_chart(&app.chart))
            .unwrap();
        app.study_templates
            .templates
            .save("Average", app.current_study_template())
            .unwrap();
        app.drawing_templates
            .templates
            .save("Trend", app.current_drawing_template().unwrap())
            .unwrap();
        app.settings_dialog.templates.poll(&ctx);
        app.show_templates(&ctx);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        for _ in 0..2 {
            app.settings_dialog.templates.poll(&ctx);
            app.show_templates(&ctx);
        }
        assert!(
            app.settings_dialog
                .templates
                .entries()
                .contains_key("Canvas")
        );
        app.chart
            .add_indicator(Box::new(egui_charts::studies::EMA::new(5)));
        app.handle_top_toolbar(
            &ctx,
            TopToolbarAction::ApplyIndicatorTemplate("Average".into()),
        );
        assert_eq!(
            app.chart.indicators().next().unwrap().1.inputs()[0].value,
            egui_charts::studies::InputValue::Length(7)
        );
        let studies: Vec<_> = app.chart.indicators().collect();
        assert_eq!(
            studies[0].1.source(),
            Some(egui_charts::studies::StudySource::Study {
                id: studies[1].0.0,
                plot: 0
            })
        );
        let mut target = Drawing::new(23, DrawingToolType::TrendLine);
        target.chart_points = vec![ChartPoint::new(8.0, 300.0), ChartPoint::new(10.0, 400.0)];
        target.completed = true;
        app.chart.drawings_mut().drawings.push(target);
        app.chart.drawings_mut().select(23);
        app.handle_drawing_toolbar(DrawingToolbarAction::LoadTemplate {
            template_id: "Trend".into(),
        });
        let target = app
            .chart
            .drawings()
            .drawings
            .iter()
            .find(|drawing| drawing.id == 23)
            .unwrap();
        assert_eq!(target.id, 23);
        assert_eq!(target.chart_points[0].bar_idx, 8.0);
        assert_eq!(target.stroke_width, 4.0);
        assert!(app.chart.undo());
        assert_ne!(
            app.chart
                .drawings()
                .drawings
                .iter()
                .find(|drawing| drawing.id == 23)
                .unwrap()
                .stroke_width,
            4.0
        );
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
                let row = text_at(&output, "Simple Moving Avg");
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
            ("ESZ202X", "6", "ESZ2026"),
            ("ESH202X", "7", "ESH2027"),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
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
            if expected.starts_with("ES") {
                assert_eq!(
                    app.chart.instrument().unwrap().exchange.as_deref(),
                    Some("CME")
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

    /// A bare number enters minutes and Enter applies it through the normal host path.
    #[test]
    fn typed_interval_entry_applies_minutes() {
        use eframe::App;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        for events in [
            vec![egui::Event::Text("15".into())],
            vec![],
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
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
        }
        assert_eq!(app.state.timeframe, Timeframe::Min15);
        assert_eq!(app.state.timeframe.duration_ms(), 900_000);
        assert_eq!(app.chart.timeframe(), Some(Timeframe::Min15));
        assert_eq!(app.top_toolbar.timeframe_selector.current, Timeframe::Min15);
        let original = app.chart.config().background_color;
        let mut settings = app.settings_state.clone();
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
            app.live.set_paused(true);
            if query == "RSI" {
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
                    app.chart.drawings().active_tool,
                    Some(egui_charts::drawings::DrawingToolType::TrendLine)
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
                _ => assert!(app.settings_dialog.is_open),
            }
        }
    }

    #[test]
    fn session_shortcut_switches_the_default_sample_and_keeps_continuous_markets() {
        for (symbol, initial, expected) in [
            ("SAMPLE", SessionType::Full24x7, SessionType::RTH),
            ("SAMPLE", SessionType::RTH, SessionType::ETH),
            ("SAMPLE", SessionType::ETH, SessionType::RTH),
            ("BTCUSD", SessionType::Full24x7, SessionType::Full24x7),
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
                app.chart.drawings().active_tool,
                Some(DrawingToolType::TrendLine)
            );
            assert_eq!(app.replay.is_some(), replay);
            assert_eq!(app.replay_control.is_visible(), replay);
            if let Some(before) = before {
                assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, before + 1);
            }
        }
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
        app.chart = Chart::new(BarData::from_bars(bars));
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
            let (target, options) = app.screenshot_requested.as_mut().unwrap();
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
    }

    #[test]
    fn saved_layouts_open_again_and_after_a_restart() {
        use eframe::App;
        use egui_charts::model::ChartType;
        use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        ctx.run_ui(Default::default(), |ui| {
            app.chart.show(ui);
        })
        .drop_without_applying_deltas();
        app.user_action(|app| {
            app.chart
                .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
        });
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Line));
        app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
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
        app.add_spread("AAPL-MSFT");
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
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
        let mut settings = app.settings_state.clone();
        settings.alerts.notification_seconds = 12;
        settings.alerts.show_active_only = false;
        settings.alerts.sound_volume = 0.35;
        app.apply_chart_settings(settings.clone());
        // Opening the current layout again returns to it as saved.
        app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Area));
        app.handle_layout_action(LayoutAction::Open("Unnamed".into()));
        app.poll_storage(&ctx);
        assert_eq!(app.settings_state.alerts, settings.alerts);
        let saved = |app: &DemoApp| {
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
                    .filter(|(_, options)| options.name == "AAPL-MSFT")
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
                .find(|(symbol, _, _)| symbol == "AAPL")
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
        saved(&app);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        app.poll_storage(&ctx);
        saved(&app);
        app.handle_layout_action(LayoutAction::Rename("Unnamed".into(), "Renamed".into()));
        app.poll_storage(&ctx);
        saved(&app);
        app.handle_layout_action(LayoutAction::SaveAs("Copy".into()));
        app.poll_storage(&ctx);
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
                "AAPL",
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
            app.live.set_paused(true);
            app.chart.set_feed_paused(true);
            app.chart.set_series_scale(SeriesId::MAIN, main_scale);
            let main_symbol = app.chart.symbol().to_owned();
            let main_bars = app.chart.bars(SeriesId::MAIN).to_vec();
            app.handle_top_toolbar(&ctx, TopToolbarAction::OpenCompare);
            for (label, count, visible) in [
                ("AAPL", 1, true),
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
                let mut pos = frame(vec![], label).expect("comparison action is visible");
                frame(vec![egui::Event::PointerMoved(pos)], label);
                if label == "AAPL" {
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
                let comparisons: Vec<_> = app
                    .chart
                    .series()
                    .filter(|(_, series)| series.name == "AAPL")
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
                if label == "AAPL" {
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
                    let template = app.current_study_template();
                    assert_eq!(
                        template
                            .iter()
                            .map(|study| study.name.as_str())
                            .collect::<Vec<_>>(),
                        ["Volume"]
                    );
                    app.load_study_template(&template);
                    assert!(app.chart.series().any(|(_, series)| series.name == "AAPL"));
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
        let mut state = app.settings_state.clone();
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
        state.margins.right_bars = 17;
        state.trading.show_positions = false;
        state.trading.show_orders = false;
        state.trading.show_executions = false;
        state.trading.show_execution_labels = true;
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
            ("UTC", 60.0, 40.0, 1.0),
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
            let mut state = app.settings_state.clone();
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
        let expected = (range.min + range.max) / 2.0;
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
            (CrosshairStyle::Dot, CursorType::Dot, MagnetType::OHLC),
        ] {
            if magnet == MagnetType::OHLC {
                app.drawing_toolbar.state.magnet_type = magnet;
            }
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
            assert!(dm.active_tool.is_none());
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
            (Tool::GannBox, "Price divisions"),
            (Tool::GannSquare, "Price divisions"),
            (Tool::GannFixed, "Price divisions"),
            (Tool::LongPos, "Size by"),
            (Tool::ShortPos, "Size by"),
            (Tool::FixedRangeVolumeProfile, "Rows"),
            (Tool::AnchoredVolumeProfile, "Rows"),
            (Tool::RegressionTrend, "Deviations"),
            (Tool::ParallelChannel, "Show middle line"),
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
            let texts = frame(&mut app, &ctx, vec![]);
            assert!(
                texts.iter().any(|(text, _)| text == heading),
                "{tool:?}: {texts:?}"
            );
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
                                    "Apply" | "Cancel" | "Remove" | "Clone" | "Delete"
                                )
                            {
                                let bounds =
                                    egui::Rect::from_min_size(text.pos, text.galley.size());
                                if matches!(
                                    text.galley.job.text.as_str(),
                                    "Apply" | "Cancel" | "Clone" | "Delete"
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
            app.chart
                .drawings()
                .curr_drawing
                .as_ref()
                .unwrap()
                .stroke_width,
            4.0
        );
        assert_eq!(
            app.chart
                .drawings()
                .curr_drawing
                .as_ref()
                .unwrap()
                .chart_points
                .len(),
            1,
            "defaults cannot copy anchors"
        );
        assert!(
            app.chart.drawings().curr_drawing.as_ref().unwrap().visible,
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

    /// Existing drawing settings survive opening and applying the dialog, and the toolbar
    /// reflects locks changed through other drawing controls rather than an optimistic toggle.
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
        assert!(!props.visible_all_timeframes);
        assert_eq!(props.visible_timeframes, ["1h"]);
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
            .edit_drawing(7, |d| d.locked = false);
        props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
        let mut second = app.chart.drawings().drawings[0].clone();
        second.id = 8;
        app.chart.drawings_mut().drawings.push(second);
        app.chart.drawings_mut().select_drawings([7, 8]);
        let changed = DrawingProps {
            visible_all_timeframes: false,
            visible_timeframes: vec!["4h".into()],
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
        app.chart
            .drawings_mut()
            .edit_drawing(7, |d| d.locked = true);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_drawing_toolbar(ui, egui::Color32::BLACK)
        })
        .drop_without_applying_deltas();
        assert!(app.drawing_toolbar.state.lock_all);
        app.chart.drawings_mut().drawings[0].locked = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_drawing_toolbar(ui, egui::Color32::BLACK)
        })
        .drop_without_applying_deltas();
        assert!(!app.drawing_toolbar.state.lock_all);
        use egui_charts::data::VolumeProfileConfig;
        use egui_charts::drawings::{
            FibonacciConfig, GannConfig, PitchforkConfig, PositionSize, PositionSizing,
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
                price_levels: vec![0.0, 0.5, 1.0],
                time_levels: vec![0.25, 0.75],
                ..Default::default()
            });
            settings.position_size = Some(PositionSize::RiskPercent {
                account_size: 10_000.0,
                percent: 1.0,
            });
            settings.position_sizing = PositionSizing {
                point_value: 50.0,
                lot_size: 10.0,
                quantity_step: 0.01,
                leverage: 5.0,
            };
            let sizing = settings.position_sizing;
            settings.volume_profile = VolumeProfileConfig::new(48, 0.8);
            settings.regression_trend = RegressionTrendConfig::new(3.0);
            settings.channel_midline = true;
            let expected = format!("{:?}", settings.fib_config);
            props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
            let saved = &app.chart.drawings().drawings[0];
            assert_eq!(format!("{:?}", saved.fib_config), expected);
            assert_eq!(saved.pitchfork.as_ref().unwrap().background_opacity, 75);
            assert_eq!(saved.gann.as_ref().unwrap().time_levels, [0.25, 0.75]);
            assert_eq!(
                saved.position_size.unwrap().quantity(100.0, 98.0),
                Some(50.0)
            );
            assert_eq!(saved.volume_profile.unwrap().rows(), 48);
            assert_eq!(saved.position_sizing, sizing);
            assert_eq!(saved.regression_trend.unwrap().deviations(), 3.0);
            assert!(saved.channel_midline);
            assert!(app.chart.drawings().drawings[1].fib_config.is_none());
            assert_eq!(app.chart.drawings().drawings[1].chart_points[0].price, 75.0);
            assert!(app.chart.undo());
            assert!(app.chart.drawings().drawings[0].fib_config.is_none());
            assert!(app.chart.redo());
            app.chart
                .drawings_mut()
                .edit_drawing(7, |d| d.locked = true);
            props.font_size = 18.0;
            props.apply_to_chart(DrawingId(7), &mut app.chart).unwrap();
            assert_eq!(app.chart.drawings().drawings[0].font_size, 12.0);
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
            assert_eq!(app.chart.drawings().drawings[0].stroke_width, 2.0);
            assert_eq!(app.chart.drawings().drawings[1].stroke_width, 2.0);
        }
    }

    #[test]
    fn zoom_controls_leave_drawing_history_untouched() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.chart.user_action(|chart| {
            chart
                .drawings_mut()
                .add_synced_drawing(egui_charts::drawings::Drawing::new(
                    0,
                    egui_charts::drawings::DrawingToolType::TrendLine,
                ));
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

    #[test]
    fn object_tree_applies_layer_bulk_and_focus_actions() {
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx));
        for id in 1..=3 {
            let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 200.0)];
            drawing.completed = true;
            app.chart.drawings_mut().add_synced_drawing(drawing);
        }
        let ids: Vec<_> = app.chart.drawings().drawings.iter().map(|d| d.id).collect();
        app.chart
            .drawings_mut()
            .edit_drawing(ids[2], |drawing| drawing.listed = false);
        assert!(
            !app.build_source_items()
                .iter()
                .any(|source| source.id == ids[2])
        );
        app.chart
            .drawings_mut()
            .edit_drawing(ids[2], |drawing| drawing.listed = true);
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
                egui_charts::ui::widget_bar::panels::object_tree::show_data_window(ui, &info, true);
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
            frame(&mut app, vec![egui::Event::PointerGone]);
            assert_eq!(app.build_data_window().close, latest);
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
            app.chart = Chart::new(BarData::new());
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
                    assert!(labels.contains(&pointer.time.to_rfc3339()), "{labels:?}");
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
                    for range in [DateRange::Day1, DateRange::Month1, DateRange::All] {
                        let bars = app.chart.bars(SeriesId::MAIN);
                        let end = bars.len();
                        let start = if range == DateRange::Day1 {
                            let cutoff = bars.last().unwrap().time - chrono::Duration::days(1);
                            bars.iter().position(|b| b.time >= cutoff).unwrap()
                        } else {
                            0
                        };
                        app.handle_timeframe_toolbar(TimeframeToolbarAction::DateRangeChanged(
                            range,
                        ));
                        frame(&mut app);
                        let (shown_start, shown_end) = app.chart.state().visible_range();
                        assert_eq!(shown_end, end, "{range:?}");
                        assert!(shown_start.abs_diff(start) <= 1, "{range:?}");
                        let expected = app.chart.get_rendered_price_rect().width()
                            / ((end - start) as f32 + 0.5);
                        assert!((app.chart.get_time_scale_state().0 - expected).abs() < 0.001);
                        assert_eq!(app.replay.is_some(), replay);
                    }
                    if !replay {
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
                            app.set_date_range(day, day);
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
    }
    /// Selecting seconds must rebuild the same executions, including the current partial minute.
    #[test]
    fn interval_switch_rebuilds_the_same_executions() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let frame = |app: &mut DemoApp| {
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
            app.size_price_based_charts();
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
        let chart = std::mem::replace(&mut app.chart, Chart::new(BarData::new()));
        app.chart = chart.with_chart_options(options);
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
        app.live.set_paused(true);
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
    }

    #[test]
    fn copy_price_uses_the_selected_instruments_precision() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        for (symbol, price, expected) in [
            ("AAPL", 123.45678, "123.46"),
            ("AAPL", -0.001, "0.00"),
            ("EURUSD", 1.12345678, "1.1235"),
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
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
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
    fn live_alerts_survive_symbol_changes_replay_and_watchlist_membership() {
        use eframe::App;
        for watchlist in [false, true] {
            for replay in [false, true] {
                let ctx = egui::Context::default();
                let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                app.live.set_paused(true);
                app.switch_symbol("AAPL");
                app.switch_timeframe(Timeframe::Hour1);
                let mut list = Watchlist::new("Test list");
                list.add_symbol("AAPL");
                list.add_symbol("BTCUSD");
                app.watchlist = WatchlistPanel::new(vec![list]);
                let mut alert = Alert::new("AAPL", AlertCondition::Above(0.0));
                if watchlist {
                    alert.watchlist = Some("Test list".into());
                    if !replay {
                        alert.frequency = egui_charts::model::AlertFrequency::OncePerBarClose;
                    }
                }
                app.alerts.save(alert);
                app.alerts.poll(&ctx, &mut app.toasts);
                app.switch_symbol("EURUSD");
                if replay {
                    app.start_replay();
                }
                for _ in 0..4 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
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
                        vec!["AAPL", "BTCUSD"]
                    } else {
                        vec!["AAPL"]
                    },
                    "watchlist={watchlist}, replay={replay}"
                );
                app.switch_timeframe(Timeframe::Min1);
                app.watchlist.add_symbol("MSFT");
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                assert_eq!(app.alerts.log().len(), if watchlist { 3 } else { 1 });
                if watchlist && !replay {
                    let feed = LiveFeed::new("MSFT", app.live.now());
                    let bars = feed.snapshot(
                        Timeframe::Hour1,
                        egui_charts::model::BarClock::new(Timeframe::Hour1),
                        false,
                    );
                    let trigger = app
                        .alerts
                        .log()
                        .iter()
                        .find(|trigger| trigger.symbol == "MSFT")
                        .unwrap();
                    assert_eq!(
                        trigger.price,
                        Some(bars[bars.len() - 2].close),
                        "new watchlist members keep the alert's original interval"
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
                app.chart
                    .set_indicator_source(
                        child,
                        egui_charts::studies::StudySource::Study {
                            id: parent.0,
                            plot: 0,
                        },
                    )
                    .unwrap();
                app.chart.remove_series(parent);
                let mut missing = Alert::new("EURUSD", AlertCondition::Above(0.0));
                missing.frequency = egui_charts::model::AlertFrequency::OncePerBarClose;
                missing.source = egui_charts::model::AlertSource::Study {
                    id: child.0.to_string(),
                    output: 0,
                };
                app.alerts.save(missing);
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                assert_eq!(app.alerts.log().len(), if watchlist { 3 } else { 1 });
                app.switch_timeframe(Timeframe::Min1);
                let replacement = app
                    .chart
                    .add_indicator(Box::new(egui_charts::studies::SMA::new(2)));
                let mut edited = app.alerts.alerts().last().unwrap().clone();
                edited.source = egui_charts::model::AlertSource::Study {
                    id: replacement.0.to_string(),
                    output: 0,
                };
                app.alerts.save(edited);
                for _ in 0..2 {
                    ctx.run_ui(Default::default(), |ui| {
                        app.ui(ui, &mut eframe::Frame::_new_kittest())
                    })
                    .drop_without_applying_deltas();
                }
                assert_eq!(
                    app.alerts.log().len(),
                    if watchlist { 4 } else { 2 },
                    "an edited alert must use its newly selected study"
                );
                let bars = LiveFeed::new("EURUSD", app.live.now()).snapshot(
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
                let mut expired = Alert::new("EURUSD", AlertCondition::Above(0.0));
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
}
