//! Panels: the widget bar, the side panels, the market panels and the notification log.

use super::core::{Direction, SidePanel};
use super::layout::{Region, panel_frame};
use super::{Action, ExportContents, Terminal};
use chrono::{DateTime, Datelike, TimeDelta, TimeZone, Utc};
use egui_charts::config::KeyboardAction;
use egui_charts::icons::{Icon, icons};
use egui_charts::localization as l10n;
use egui_charts::model::Tick;
use egui_charts::model::{Bar, Watchlist};
use egui_charts::services::{ProfileDirectory, ProfileId, Profiles, UserSettings};
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::financials::FinancialsPanel;
use egui_charts::ui::profile::ProfileCard;
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_charts::ui::widget_bar::{
    CalendarAction, CalendarPanel, ChatsPanel, HotlistsAction, HotlistsPanel, MarketDetail,
    MarketHeatmapPanel, NewsAction, NewsPanel, NotificationsAction, NotificationsPanel,
    PeriodReturn, ProductsAction, ProductsPanel, RightBarIcon, ScreenerAction, ScreenerPanel,
    Seasonality, SeasonalityMonth, SymbolDetailsPanel, TimeAndSalesPanel, WatchlistAction,
    WatchlistDetails, WatchlistPanel, WidgetBar, WidgetBarAction, WidgetBarSection,
    WidgetBarWidget,
};
use egui_charts::widget::{SeriesId, ShortcutRequest};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The window width below which the rail and the side panels are left out, giving the chart the
/// whole width.
const RAIL_MIN_WIDTH: f32 = 600.0;

/// The rail entries of the panels that have no built-in icon: their panel, widget id, icon and
/// tooltip. Host widgets use other ids.
const WIDGETS: [(SidePanel, u64, &Icon, &str); 2] = [
    (
        SidePanel::TimeAndSales,
        u64::MAX,
        &icons::SETTINGS_EVENTS,
        "Time and sales",
    ),
    (
        SidePanel::DataWindow,
        u64::MAX - 1,
        &icons::INFO,
        "Data window",
    ),
];

/// The market panels of the side panel area. The terminal feeds them from the host's datafeed and
/// services; the host sets their content, such as watchlists, screener sets, hotlist names,
/// heatmap entries, calendar countries and products, through
/// [`Terminal::panels_mut`](super::Terminal::panels_mut). Left as they are, the panels have
/// the defaults of their components.
pub struct MarketPanels {
    /// The watchlists.
    pub watchlist: WatchlistPanel,
    /// The details of the chart's symbol, under the watchlist.
    pub details: SymbolDetailsPanel,
    /// Earnings, dividends and headlines supplied by the host for the wide watchlist.
    pub watchlist_details: WatchlistDetails,
    /// The trades of the chart's symbol.
    pub time_and_sales: TimeAndSalesPanel,
    /// Market highlights.
    pub highlights: NewsPanel,
    /// News of the chart's symbol and of the market.
    pub news: NewsPanel,
    /// The economic and corporate calendars.
    pub calendar: CalendarPanel,
    /// The screener.
    pub screener: ScreenerPanel,
    /// The hotlists.
    pub hotlists: HotlistsPanel,
    /// The market heatmap. The rail shows its entry while there are entries.
    pub heatmap: MarketHeatmapPanel,
    /// The log of alert triggers and other events.
    pub notifications: NotificationsPanel,
    /// The chat rooms. The rail shows their entry once the host supplied a chats service.
    pub chats: ChatsPanel,
    /// The products launcher. The rail shows its entry while there are products.
    pub products: ProductsPanel,
}

impl Default for MarketPanels {
    fn default() -> Self {
        Self {
            watchlist: WatchlistPanel::default(),
            details: SymbolDetailsPanel::new(),
            watchlist_details: WatchlistDetails::default(),
            time_and_sales: TimeAndSalesPanel::default(),
            highlights: NewsPanel::highlights(),
            news: NewsPanel::new(),
            calendar: CalendarPanel::new(),
            screener: ScreenerPanel::new(Vec::new()),
            hotlists: HotlistsPanel::new(Vec::new()),
            heatmap: MarketHeatmapPanel::default(),
            notifications: NotificationsPanel::new(),
            chats: ChatsPanel::new(),
            products: ProductsPanel::new(Vec::new()),
        }
    }
}

/// The widget bar, the panels and what they need between frames.
pub(super) struct State {
    /// Follows the terminal's control visibility, which the core applies to it.
    pub(super) widget_bar: WidgetBar,
    pub(super) market: MarketPanels,
    profiles: Option<ProfileDirectory>,
    profile: Option<ProfileId>,
    wide_watchlist: bool,
    /// Reuses the watchlist quote subscription and validation for the heatmap universes.
    heatmap_quotes: WatchlistPanel,
    /// Revision and last bar, since live changes to that bar do not change the revision.
    returns_input: Option<(u64, Option<Bar>)>,
    /// The text of the open import window.
    import: Option<String>,
    /// Whether the next symbol picked goes to the watchlist, not the chart.
    adding_symbol: bool,
    /// The symbol whose trades the time and sales panel lists.
    tape_symbol: String,
    /// The newest trade listed: its time and how many trades share that time.
    tape_cursor: Option<(DateTime<Utc>, usize)>,
    /// Open financials in place of the charts: the symbol and whether its figures had arrived
    /// when the panel was made.
    financials: Option<((String, bool), FinancialsPanel)>,
}

impl Default for State {
    fn default() -> Self {
        let mut widget_bar = WidgetBar::default();
        widget_bar.config.width = 45.0;
        widget_bar.icons = SidePanel::RAIL
            .iter()
            .filter_map(|panel| panel.icon())
            .chain([RightBarIcon::Help])
            .collect();
        widget_bar
            .widgets
            .extend(WIDGETS.map(|(_, id, icon, tooltip)| WidgetBarWidget {
                id,
                icon: *icon,
                tooltip: tooltip.into(),
                section: WidgetBarSection::Bottom,
            }));
        Self {
            widget_bar,
            market: MarketPanels::default(),
            profiles: None,
            profile: None,
            wide_watchlist: false,
            heatmap_quotes: WatchlistPanel::new(vec![Watchlist::new("Heatmap")]),
            returns_input: None,
            import: None,
            adding_symbol: false,
            tape_symbol: String::new(),
            tape_cursor: None,
            financials: None,
        }
    }
}

fn is_panel_widget(id: u64) -> bool {
    WIDGETS.iter().any(|(_, widget, ..)| *widget == id)
}

impl SidePanel {
    /// The panels with a built-in rail entry, in the order of the rail.
    const RAIL: [SidePanel; 12] = [
        SidePanel::Watchlist,
        SidePanel::Alerts,
        SidePanel::ObjectTree,
        SidePanel::Highlights,
        SidePanel::Screener,
        SidePanel::Hotlists,
        SidePanel::MarketHeatmap,
        SidePanel::Calendar,
        SidePanel::News,
        SidePanel::Notifications,
        SidePanel::Chats,
        SidePanel::Products,
    ];

    fn icon(self) -> Option<RightBarIcon> {
        Some(match self {
            SidePanel::Watchlist => RightBarIcon::Watchlist,
            SidePanel::Alerts => RightBarIcon::Alerts,
            SidePanel::ObjectTree => RightBarIcon::ObjectTree,
            SidePanel::Highlights => RightBarIcon::Highlights,
            SidePanel::Screener => RightBarIcon::Screener,
            SidePanel::Hotlists => RightBarIcon::Hotlists,
            SidePanel::MarketHeatmap => RightBarIcon::MarketHeatmap,
            SidePanel::Calendar => RightBarIcon::Calendar,
            SidePanel::News => RightBarIcon::News,
            SidePanel::Notifications => RightBarIcon::Notifications,
            SidePanel::Chats => RightBarIcon::Chats,
            SidePanel::Products => RightBarIcon::Products,
            SidePanel::DataWindow | SidePanel::TimeAndSales => return None,
        })
    }

    fn from_icon(icon: RightBarIcon) -> Option<Self> {
        Self::RAIL
            .into_iter()
            .find(|panel| panel.icon() == Some(icon))
    }
}

impl Terminal {
    /// The market panels, to set the content the host supplies.
    pub fn panels_mut(&mut self) -> &mut MarketPanels {
        &mut self.panels.market
    }

    /// The right-hand widget bar, to register host panels and entries.
    pub fn widget_bar_mut(&mut self) -> &mut WidgetBar {
        &mut self.panels.widget_bar
    }

    pub(super) fn panels_init(&mut self) {
        let market = &mut self.panels.market;
        if let Some(quotes) = &self.services.quotes {
            market.watchlist.set_service(quotes.clone());
        }
        if let Some(notifications) = &self.services.notifications {
            market.notifications.set_service(notifications.clone());
        }
        if let Some(chats) = &self.services.chats {
            market.chats.set_service(chats.clone());
        }
        if let Some(storage) = &self.services.storage {
            market.details.set_storage(storage.clone());
        }
        market.details.set_feed(self.feed.clone());
        market.highlights.set_feed(self.feed.clone());
        market.news.set_feed(self.feed.clone());
        market.calendar.set_feed(self.feed.clone());
        market.screener.set_feed(self.feed.clone());
        market.hotlists.set_feed(self.feed.clone());
        self.panel = Some(SidePanel::Watchlist);
    }

    pub(super) fn panels_poll(&mut self, ctx: &egui::Context) {
        let market = &mut self.panels.market;
        for entry in std::mem::take(&mut self.notices) {
            market.notifications.push(entry);
        }
        market.details.poll(ctx);
        market.highlights.poll(ctx);
        market.news.poll(ctx);
        market.calendar.poll(ctx);
        market.screener.poll(ctx);
        market.hotlists.poll(ctx);
        for entry in market.notifications.poll(ctx) {
            if self.charts[self.active]
                .event_visibility()
                .show_news_notifications
            {
                self.toasts.info(entry.title);
            }
        }
        if let Some(profiles) = &mut self.panels.profiles {
            profiles.poll(ctx);
        }
        self.menus.set_watchlists(
            market
                .watchlist
                .lists()
                .iter()
                .map(|list| list.name.clone()),
        );
        market.chats.poll(ctx);
        let unread = market.notifications.unread() as u32;
        self.panels
            .widget_bar
            .set_notification_cnt(RightBarIcon::Notifications, unread);
        self.feed_heatmap(ctx);
        self.feed_market_panels(ctx);
        self.feed_financials();
    }

    /// Offers Financials once the feed has answered with figures for the details' symbol, and
    /// keeps open financials on that symbol, showing its figures once they arrive.
    fn feed_financials(&mut self) {
        let details = &mut self.panels.market.details;
        let available = details.financials().is_some();
        details.set_financials_visible(available);
        let key = (details.symbol().unwrap_or_default().to_owned(), available);
        if details.take_financials_request() {
            self.panel = None;
        } else if self
            .panels
            .financials
            .as_ref()
            .is_none_or(|(shown, _)| *shown == key)
        {
            return;
        }
        let title = self.charts[self.active]
            .instrument()
            .filter(|instrument| instrument.symbol == key.0)
            .and_then(|instrument| instrument.description.clone())
            .unwrap_or_else(|| key.0.clone());
        let panel = FinancialsPanel::new(title, details.financials().cloned());
        self.panels.financials = Some((key, panel));
    }

    /// Replaces the chart workspace until the user returns to it.
    pub(super) fn show_financials(&mut self, ui: &mut egui::Ui) -> bool {
        let Some((_, panel)) = &mut self.panels.financials else {
            return false;
        };
        let back = egui::Frame::NONE
            .inner_margin(12.0)
            .show(ui, |ui| panel.show(ui));
        if back.inner {
            self.panels.financials = None;
        }
        true
    }

    /// Subscribes to every heatmap symbol, including symbols outside the active watchlist.
    fn feed_heatmap(&mut self, ctx: &egui::Context) {
        let mut symbols: Vec<_> = self
            .panels
            .market
            .heatmap
            .universes
            .iter()
            .flat_map(|universe| &universe.entries)
            .map(|entry| entry.symbol.clone())
            .collect();
        symbols.sort();
        symbols.dedup();
        // Compared as the list keeps them, so a symbol it trims or drops does not resubscribe.
        let list = Watchlist::from_text("Heatmap", &symbols.join(","));
        let quotes = &mut self.panels.heatmap_quotes;
        if !quotes.symbols().eq(list.symbols()) {
            *quotes = WatchlistPanel::new(vec![list]);
            if let Some(service) = &self.services.quotes {
                quotes.set_service(service.clone());
            }
        }
        quotes.poll(ctx);
        for entry in self
            .panels
            .market
            .heatmap
            .universes
            .iter_mut()
            .flat_map(|universe| &mut universe.entries)
        {
            if let Some(quote) = quotes.quote(&entry.symbol) {
                entry.values.insert("price".into(), quote.last);
                let change = quote.change_percent();
                if change.is_finite() {
                    entry.values.insert("change".into(), change);
                } else {
                    entry.values.remove("change");
                }
            }
        }
    }

    /// Hands the chart's symbol, quote, time zone and trades to the panels that follow them.
    fn feed_market_panels(&mut self, ctx: &egui::Context) {
        let chart = &self.charts[self.active];
        let State {
            market,
            tape_symbol,
            tape_cursor,
            returns_input,
            ..
        } = &mut self.panels;
        let chart_symbol = chart.symbol().to_owned();
        let instrument = chart.instrument();
        let mut selected = chart_symbol.clone();
        if let Some(symbol) = market
            .watchlist
            .symbols()
            .find(|symbol| symbol.eq_ignore_ascii_case(&chart_symbol))
        {
            selected = symbol.to_owned();
        }
        if let Some(decimals) = instrument.and_then(|instrument| instrument.price_decimals) {
            market.watchlist.set_price_decimals(&selected, decimals);
        }
        market.watchlist.set_selected(&selected);
        market.watchlist.poll(ctx);
        market.details.set_instrument(
            &instrument
                .cloned()
                .unwrap_or_else(|| egui_charts::model::Instrument::new(&chart_symbol)),
        );
        if let Some(quote) = market.watchlist.quote(&selected).cloned() {
            market.details.set_quote(quote);
        }
        let timezone = chart
            .chart_options()
            .time_scale
            .timezone
            .to_tz()
            .unwrap_or_default();
        market.details.set_timezone(timezone.clone());
        market.highlights.set_symbol(&chart_symbol);
        market.news.set_symbol(&chart_symbol);
        let bars = chart.bars(SeriesId::MAIN);
        let input = (chart.state().revision(), bars.last().cloned());
        if returns_input.as_ref() != Some(&input) {
            market
                .details
                .set_period_returns(&chart_symbol, MarketDetail::Ready(period_returns(bars)));
            market
                .details
                .set_seasonality(&chart_symbol, MarketDetail::Ready(seasonality(bars)));
            *returns_input = Some(input);
        }
        let config = &mut market.time_and_sales.config;
        config.price_decimals = instrument.and_then(|instrument| instrument.price_decimals);
        config.size_decimals = instrument.and_then(|instrument| instrument.volume_decimals);
        config.timezone = timezone;

        // The chart keeps the trades the feed sent, oldest first; the panel lists each once.
        if *tape_symbol != chart_symbol {
            market.time_and_sales.clear();
            tape_symbol.clone_from(&chart_symbol);
            *tape_cursor = None;
        }
        let (since, seen) = tape_cursor.unwrap_or((DateTime::<Utc>::MIN_UTC, 0));
        let mut at_since = 0;
        let mut large: Option<Tick> = None;
        for trade in chart.trades(SeriesId::MAIN) {
            if trade.ts < since {
                continue;
            }
            if trade.ts == since {
                at_since += 1;
                if at_since <= seen {
                    continue;
                }
            }
            *tape_cursor = match *tape_cursor {
                Some((time, count)) if time == trade.ts => Some((time, count + 1)),
                _ => Some((trade.ts, 1)),
            };
            if market.time_and_sales.push(trade.clone()).is_some() {
                large = Some(trade.clone());
            }
        }
        if let Some(trade) = large {
            let decimals = instrument.and_then(|i| i.price_decimals).unwrap_or(2);
            self.toasts.info(format!(
                "Large trade: {} at {:.*}",
                trade.size,
                usize::from(decimals),
                trade.price
            ));
        }
    }

    pub(super) fn panels_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Right => self.show_right(ui),
            Region::Windows => {
                self.show_import(ui.ctx());
                self.show_wide_watchlist(ui.ctx());
                self.show_profile(ui.ctx());
            }
            Region::Top | Region::Left | Region::Bottom | Region::Chart => {}
        }
    }

    /// The rail and the side panel beside it.
    fn show_right(&mut self, ui: &mut egui::Ui) {
        if ui.ctx().content_rect().width() < RAIL_MIN_WIDTH {
            return;
        }
        let bg = self.theme.ui.panel_bg;
        let panel = self.panel;
        let market = &self.panels.market;
        let bar = &mut self.panels.widget_bar;
        for (icon, shown) in [
            (
                RightBarIcon::MarketHeatmap,
                !market.heatmap.universes.is_empty(),
            ),
            (RightBarIcon::Chats, self.services.chats.is_some()),
            (RightBarIcon::Products, !market.products.products.is_empty()),
        ] {
            bar.icons.retain(|entry| *entry != icon);
            if shown {
                bar.icons.push(icon);
            }
        }
        bar.active_icon = panel.and_then(SidePanel::icon);
        let entry = WIDGETS.iter().find(|(entry, ..)| Some(*entry) == panel);
        if let Some((_, id, ..)) = entry {
            bar.active_widget = Some(*id);
        } else if panel.is_some() || bar.active_widget.is_some_and(is_panel_widget) {
            bar.active_widget = None;
        }
        let mut emitted = Vec::new();
        let mut picked = None;
        egui::Panel::right("egui_charts_terminal_widget_bar")
            .resizable(false)
            .exact_size(49.0)
            .show_separator_line(false)
            .frame(panel_frame(bg).outer_margin(egui::Margin {
                left: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| match bar.show(ui) {
                WidgetBarAction::None => {}
                action => emitted.push(Action::WidgetBar(action)),
            });
        bar.show_panel(ui);
        if let Some(which) = panel {
            picked = self.show_side_panel(ui, which, &mut emitted);
        }
        self.emitted.extend(emitted);
        // The heatmap has no action of its own.
        if let Some(symbol) = picked {
            self.switch_symbol(&symbol);
        }
    }

    /// Shows the body of `which`, adding what it emitted to `emitted`, and returns the symbol the
    /// user picked in the heatmap.
    fn show_side_panel(
        &mut self,
        ui: &mut egui::Ui,
        which: SidePanel,
        emitted: &mut Vec<Action>,
    ) -> Option<String> {
        // The drawings and alerts groups own these panel bodies.
        if matches!(
            which,
            SidePanel::ObjectTree | SidePanel::Alerts | SidePanel::DataWindow
        ) {
            return None;
        }
        let market = &mut self.panels.market;
        let mut picked = None;
        egui::Panel::right("egui_charts_terminal_detail_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(220.0)
            .max_size((ui.available_width() * 0.5).max(220.0))
            .show_separator_line(false)
            .frame(
                panel_frame(self.theme.ui.panel_bg).outer_margin(egui::Margin {
                    left: 4,
                    ..egui::Margin::ZERO
                }),
            )
            .show(ui, |ui| match which {
                SidePanel::Watchlist => {
                    if ui.available_height() >= 400.0 {
                        egui::Panel::bottom("egui_charts_terminal_symbol_details")
                            .resizable(true)
                            .default_size(ui.available_height() / 2.0)
                            .min_size(120.0)
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| market.details.show(ui));
                    }
                    match market.watchlist.show(ui) {
                        WatchlistAction::None => {}
                        action => emitted.push(Action::Watchlist(action)),
                    }
                }
                SidePanel::TimeAndSales => market.time_and_sales.show(ui),
                SidePanel::Highlights => match market.highlights.show(ui) {
                    NewsAction::None => {}
                    action => emitted.push(Action::News(action)),
                },
                SidePanel::News => match market.news.show(ui) {
                    NewsAction::None => {}
                    action => emitted.push(Action::News(action)),
                },
                SidePanel::Screener => match market.screener.show(ui) {
                    ScreenerAction::None => {}
                    action => emitted.push(Action::Screener(action)),
                },
                SidePanel::MarketHeatmap => {
                    picked = market.heatmap.show(ui);
                }
                SidePanel::Hotlists => match market.hotlists.show(ui) {
                    HotlistsAction::None => {}
                    action => emitted.push(Action::Hotlists(action)),
                },
                SidePanel::Notifications => match market.notifications.show(ui) {
                    NotificationsAction::None => {}
                    action => emitted.push(Action::Notifications(action)),
                },
                SidePanel::Chats => market.chats.show(ui),
                SidePanel::Products => match market.products.show(ui) {
                    ProductsAction::None => {}
                    action => emitted.push(Action::Products(action)),
                },
                SidePanel::Calendar => match market.calendar.show(ui) {
                    CalendarAction::None => {}
                    action => emitted.push(Action::Calendar(action)),
                },
                SidePanel::ObjectTree | SidePanel::Alerts | SidePanel::DataWindow => {}
            });
        picked
    }

    /// Connects profile cards and follow requests to the host's profiles service.
    pub fn with_profiles(mut self, service: Arc<dyn Profiles>) -> Self {
        let mut profiles = ProfileDirectory::new();
        profiles.set_service(service);
        self.panels.profiles = Some(profiles);
        self
    }

    fn show_profile(&mut self, ctx: &egui::Context) {
        let (Some(id), Some(profiles)) = (&self.panels.profile, &mut self.panels.profiles) else {
            return;
        };
        let mut open = true;
        egui::Window::new(l10n::text(ctx, "Profile"))
            .id(egui::Id::unique("egui_charts_terminal_profile"))
            .open(&mut open)
            .show(ctx, |ui| ProfileCard::new(id).show(ui, profiles));
        if !open {
            self.panels.profile = None;
        }
    }

    fn show_wide_watchlist(&mut self, ctx: &egui::Context) {
        if !self.panels.wide_watchlist {
            return;
        }
        let mut open = true;
        let mut action = WatchlistAction::None;
        egui::Window::new(l10n::text(ctx, "Watchlist"))
            .id(egui::Id::unique("egui_charts_terminal_watchlist"))
            .open(&mut open)
            .default_size([900.0, 520.0])
            .show(ctx, |ui| {
                action = self
                    .panels
                    .market
                    .watchlist
                    .show_wide(ui, &self.panels.market.watchlist_details);
            });
        self.panels.wide_watchlist = open;
        if action != WatchlistAction::None {
            self.emit(Action::Watchlist(action));
        }
    }

    /// There is no file picker for imports: the user pastes the exported text instead.
    fn show_import(&mut self, ctx: &egui::Context) {
        let Some(text) = &mut self.panels.import else {
            return;
        };
        let mut open = true;
        let mut import = false;
        egui::Window::new(l10n::text(ctx, "Import watchlist"))
            .id(egui::Id::unique("egui_charts_terminal_watchlist_import"))
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(l10n::text(
                    ctx,
                    "Paste symbols separated by commas; ###Title starts a section.",
                ));
                ui.add(egui::TextEdit::multiline(text).desired_rows(6));
                import = ui.button(l10n::text(ctx, "Import")).clicked();
            });
        if import {
            self.panels.market.watchlist.import("Imported", text);
        }
        if import || !open {
            self.panels.import = None;
        }
    }

    /// Hands exported `text` to the host's file export, or to the clipboard without one.
    fn export_text(&mut self, name: &str, text: String) {
        if !self.save_file(name.to_owned(), ExportContents::Text(text.clone())) {
            self.ctx().copy_text(text);
            self.toast(format!("Copied {name} to the clipboard"));
        }
    }

    /// Toggles `panel`, refreshing the rankings that go stale while closed.
    fn toggle_panel(&mut self, panel: SidePanel) {
        if self.panel == Some(panel) {
            self.panel = None;
            return;
        }
        // Quotes move: list current figures each time a ranking opens.
        match panel {
            SidePanel::Hotlists => self.panels.market.hotlists.refresh(),
            SidePanel::Screener => self.panels.market.screener.refresh(),
            _ => {}
        }
        self.open_panel(panel);
    }

    pub(super) fn panels_perform(&mut self, action: Action) -> Option<Action> {
        match &action {
            Action::WidgetBar(WidgetBarAction::Clicked(icon)) => {
                // Help belongs to the group that shows the shortcuts.
                let Some(panel) = SidePanel::from_icon(*icon) else {
                    return Some(action);
                };
                self.toggle_panel(panel);
            }
            Action::WidgetBar(WidgetBarAction::WidgetClicked(id)) => {
                if let Some((panel, ..)) = WIDGETS.iter().find(|(_, widget, ..)| widget == id) {
                    self.toggle_panel(*panel);
                } else {
                    // A host panel replaces the side panel; the host sees its entry click.
                    self.panel = None;
                    return Some(action);
                }
            }
            Action::Shortcut(ShortcutRequest::Action(KeyboardAction::OpenDataWindow)) => {
                self.open_panel(SidePanel::DataWindow)
            }
            Action::Watchlist(WatchlistAction::SelectSymbol(symbol))
            | Action::Screener(ScreenerAction::SelectSymbol(symbol))
            | Action::Hotlists(HotlistsAction::SelectSymbol(symbol))
            | Action::Notifications(NotificationsAction::SelectSymbol(symbol))
            | Action::Calendar(CalendarAction::SelectSymbol(symbol)) => self.switch_symbol(symbol),
            Action::Watchlist(WatchlistAction::AddSymbol) => {
                let leftover = self.perform(Action::TopToolbar(TopToolbarAction::OpenSymbolSearch));
                self.panels.adding_symbol = leftover.is_none();
                return leftover;
            }
            Action::Watchlist(WatchlistAction::AdvancedView) => self.panels.wide_watchlist = true,
            Action::Watchlist(WatchlistAction::CompactView) => self.panels.wide_watchlist = false,
            Action::Notifications(NotificationsAction::OpenProfile(id))
                if self.panels.profiles.is_some() =>
            {
                self.panels.profile = Some(id.clone());
            }
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::AddToWatchlist {
                list,
                symbol,
            })) => {
                if !self.panels.market.watchlist.add_symbol_to(*list, symbol) {
                    self.toast(format!("{symbol} is already in that watchlist"));
                }
            }
            Action::Watchlist(WatchlistAction::Import) => {
                self.panels.import = Some(String::new());
            }
            Action::Watchlist(WatchlistAction::Export { text, .. }) => {
                self.export_text("watchlist.txt", text.clone());
            }
            Action::Screener(ScreenerAction::ExportCsv(text)) => {
                self.export_text("screener.csv", text.clone());
            }
            Action::Screener(ScreenerAction::SendToWatchlist(symbols)) => {
                self.panels
                    .market
                    .watchlist
                    .import("Screener", &symbols.join(","));
                self.open_panel(SidePanel::Watchlist);
            }
            Action::News(NewsAction::ShowOnChart(time)) => self.chart_mut().scroll_to_time(*time),
            Action::News(NewsAction::OpenUrl(url)) => {
                self.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            Action::Products(ProductsAction::Open(id)) => {
                let panel = match id.as_str() {
                    "screener" => SidePanel::Screener,
                    "calendar" => SidePanel::Calendar,
                    "news" => SidePanel::News,
                    _ => return Some(action),
                };
                self.open_panel(panel);
            }
            Action::SymbolPicked(instrument) if self.panels.adding_symbol => {
                self.panels.adding_symbol = false;
                self.panels.market.watchlist.add_symbol(&instrument.symbol);
            }
            // A search the user opens another way is not for the watchlist.
            Action::TopToolbar(TopToolbarAction::OpenSymbolSearch)
            | Action::Shortcut(ShortcutRequest::SymbolSearch(_)) => {
                self.panels.adding_symbol = false;
                return Some(action);
            }
            _ => return Some(action),
        }
        None
    }

    pub(super) fn panels_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        if direction == Direction::Load {
            self.panels.market.heatmap.selection = settings.market_heatmap.clone();
        }
        let watchlist = &mut self.panels.market.watchlist;
        match direction {
            Direction::Save => {
                settings.watchlists = watchlist.lists().to_vec();
                settings.market_heatmap = self.panels.market.heatmap.selection.clone();
            }
            Direction::Load if !settings.watchlists.is_empty() => {
                let selected = self.charts[self.active].symbol().to_owned();
                *watchlist = WatchlistPanel::new(std::mem::take(&mut settings.watchlists));
                if let Some(quotes) = &self.services.quotes {
                    watchlist.set_service(quotes.clone());
                }
                watchlist.set_selected(&selected);
            }
            Direction::Load => {}
        }
    }

    pub(super) fn panels_modal(&self) -> bool {
        self.panels.import.is_some()
            || self.panels.wide_watchlist
            || self.panels.profile.is_some()
            || self.panels.financials.is_some()
    }
}

/// The return in percent from the close at or before each look-back date to the last close.
/// A period the history does not reach back to has no return.
fn period_returns(bars: &[Bar]) -> Vec<PeriodReturn> {
    let Some(last) = bars.iter().rev().find(|bar| !bar.is_whitespace()) else {
        return Vec::new();
    };
    let year_start = Utc
        .with_ymd_and_hms(last.time.year(), 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(last.time);
    [
        ("1W", last.time.checked_sub_signed(TimeDelta::days(7))),
        ("1M", last.time.checked_sub_signed(TimeDelta::days(30))),
        ("3M", last.time.checked_sub_signed(TimeDelta::days(91))),
        ("6M", last.time.checked_sub_signed(TimeDelta::days(182))),
        // The last close of the previous year: a bar stamped at the year's start opens it.
        (
            "YTD",
            year_start.checked_sub_signed(TimeDelta::nanoseconds(1)),
        ),
        ("1Y", last.time.checked_sub_signed(TimeDelta::days(365))),
    ]
    .into_iter()
    .map(|(period, since)| {
        let base = since.and_then(|since| {
            bars.iter()
                .rev()
                .find(|bar| bar.time <= since && !bar.is_whitespace())
        });
        let percent = base
            .filter(|base| base.close > 0.0)
            .map(|base| (last.close / base.close - 1.0) * 100.0);
        PeriodReturn::new(period, percent)
    })
    .collect()
}

/// The average return of each calendar month, from its first open to its last close, over the
/// complete months of `bars`; the first and last months may be partial and are left out.
fn seasonality(bars: &[Bar]) -> Seasonality {
    let mut months: BTreeMap<(i32, u32), (f64, f64)> = BTreeMap::new();
    for bar in bars.iter().filter(|bar| !bar.is_whitespace()) {
        months
            .entry((bar.time.year(), bar.time.month()))
            .and_modify(|month| month.1 = bar.close)
            .or_insert((bar.open, bar.close));
    }
    let months: Vec<_> = months.into_iter().collect();
    let complete = months
        .get(1..months.len().saturating_sub(1))
        .unwrap_or_default();
    let years = complete
        .iter()
        .map(|((year, _), _)| *year)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let summaries = (1..=12u8)
        .filter_map(|month| {
            let returns: Vec<f64> = complete
                .iter()
                .filter(|((_, m), (open, _))| *m == u32::from(month) && *open > 0.0)
                .map(|(_, (open, close))| (close / open - 1.0) * 100.0)
                .collect();
            let count = returns.len() as f64;
            (count > 0.0).then(|| {
                SeasonalityMonth::new(
                    month,
                    Some(returns.iter().sum::<f64>() / count),
                    Some(returns.iter().filter(|r| **r > 0.0).count() as f64 / count),
                )
            })
        })
        .collect();
    Seasonality::new(years as u32, summaries)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{MemoryStorage, SIZE, TestFeed, frame};
    use super::*;
    use egui_charts::data::{
        Datafeed, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest,
    };
    use egui_charts::model::{Instrument, Watchlist};
    use std::sync::Arc;

    /// The test feed, with one recent trade at 181.25.
    struct TapeFeed;

    impl Datafeed for TapeFeed {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            TestFeed.resolve(symbol, reply);
        }

        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            TestFeed.history(request, reply);
        }

        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            let mut page = TradePage::default();
            page.trades = vec![Tick::new(
                "2026-01-01T12:34:56Z".parse().unwrap(),
                181.25,
                3.0,
                None,
            )];
            reply.send(Ok(page));
        }
    }

    fn terminal() -> Terminal {
        Terminal::new(Arc::new(TapeFeed)).with_symbol("AAPL")
    }

    /// The texts of one frame.
    fn texts(terminal: &mut Terminal) -> Vec<String> {
        let ctx = terminal.ctx.clone().unwrap();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            time: Some(ctx.input(|i| i.time) + 0.1),
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| drop(terminal.show(ui)));
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    #[test]
    fn rail_clicks_open_built_in_panels_and_return_host_widgets() {
        let mut terminal = terminal();
        frame(&mut terminal, SIZE, vec![]);
        // The icon clicked, the panel it opens, and whether the click is returned to the host.
        for (action, panel, returned) in [
            (
                WidgetBarAction::Clicked(RightBarIcon::Highlights),
                Some(SidePanel::Highlights),
                false,
            ),
            (
                WidgetBarAction::Clicked(RightBarIcon::Notifications),
                Some(SidePanel::Notifications),
                false,
            ),
            (
                WidgetBarAction::Clicked(RightBarIcon::Notifications),
                None,
                false,
            ),
            (WidgetBarAction::Clicked(RightBarIcon::Help), None, false),
            (
                WidgetBarAction::WidgetClicked(u64::MAX),
                Some(SidePanel::TimeAndSales),
                false,
            ),
            (WidgetBarAction::WidgetClicked(7), None, true),
        ] {
            let left = terminal.run(Action::WidgetBar(action));
            assert_eq!((terminal.panel, left.is_some()), (panel, returned));
        }
    }

    #[test]
    fn a_narrow_window_leaves_the_side_panel_out() {
        let mut terminal = terminal();
        // The watchlist is open from the start; its add button is drawn wherever it is.
        for (width, shown) in [(SIZE.x, true), (393.0, false)] {
            let output = frame(&mut terminal, egui::vec2(width, SIZE.y), vec![]);
            assert_eq!(output.text_at("Add symbol").is_some(), shown, "{width}");
        }
    }

    #[test]
    fn products_open_panels_and_return_unknown_ids() {
        let mut terminal = terminal();
        frame(&mut terminal, SIZE, vec![]);
        let open = |terminal: &mut Terminal, id: &str| {
            terminal.run(Action::Products(ProductsAction::Open(id.into())))
        };
        assert!(open(&mut terminal, "calendar").is_none());
        assert_eq!(terminal.panel, Some(SidePanel::Calendar));
        assert!(matches!(
            open(&mut terminal, "shortcuts"),
            Some(Action::Products(ProductsAction::Open(id))) if id == "shortcuts"
        ));
        assert_eq!(terminal.panel, Some(SidePanel::Calendar));
        let advanced = terminal.run(Action::Watchlist(WatchlistAction::AdvancedView));
        assert!(advanced.is_none());
        assert!(terminal.panels.wide_watchlist);
    }

    #[test]
    fn watchlists_survive_a_restart() {
        let storage = Arc::new(MemoryStorage::default());
        let start = |storage: &Arc<MemoryStorage>| {
            let mut terminal = terminal().with_storage(storage.clone());
            frame(&mut terminal, SIZE, vec![]);
            terminal
        };
        let mut first = start(&storage);
        first.panels_mut().watchlist.import("Mine", "MSFT,NVDA");
        frame(&mut first, SIZE, vec![]);
        drop(first);
        let mut second = start(&storage);
        frame(&mut second, SIZE, vec![]);
        let lists = second.panels_mut().watchlist.lists();
        assert!(
            lists.iter().any(|list: &Watchlist| list.name == "Mine"),
            "{} lists",
            lists.len()
        );
    }

    #[test]
    fn a_trade_from_the_feed_is_listed_once() {
        let mut terminal = terminal();
        frame(&mut terminal, SIZE, vec![]);
        terminal.run(Action::WidgetBar(WidgetBarAction::WidgetClicked(u64::MAX)));
        let mut listed = 0;
        for _ in 0..6 {
            listed = texts(&mut terminal)
                .iter()
                .filter(|text| *text == "181.25")
                .count();
        }
        assert_eq!(listed, 1);
    }

    /// Unsupported feeds leave no entry; host figures open in place of the chart, and a late
    /// answer for another symbol cannot replace the active company's figures.
    #[test]
    fn financials_only_offer_and_show_the_active_symbols_host_figures() {
        use super::super::test_support::click;
        use egui_charts::data::{CompanyFinancials, FinancialPage};
        use std::sync::Mutex;

        #[derive(Default)]
        struct Host(Mutex<Vec<(String, Reply<CompanyFinancials>)>>);
        impl Datafeed for Host {
            fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
                reply.send(Ok(Instrument::new(symbol).with_description("Host company")));
            }

            fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
                TestFeed.history(request, reply);
            }

            fn financials(&self, symbol: &str, reply: Reply<CompanyFinancials>) {
                self.0.lock().unwrap().push((symbol.into(), reply));
            }
        }
        let menu = |terminal: &mut Terminal| {
            frame(terminal, SIZE, vec![]);
            let ctx = terminal.ctx.clone().unwrap();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                    ..Default::default()
                },
                |ui| drop(terminal.show(ui)),
            );
            // The symbol details' More button is below the watchlist's More button.
            let more = output
                .platform_output
                .accesskit_update
                .iter()
                .flat_map(|update| &update.nodes)
                .filter(|(_, node)| node.label() == Some("More"))
                .filter_map(|(_, node)| node.bounds())
                .max_by(|a, b| a.y0.total_cmp(&b.y0))
                .expect("symbol menu");
            let pos = egui::pos2(
                ((more.x0 + more.x1) / 2.0) as f32,
                ((more.y0 + more.y1) / 2.0) as f32,
            );
            output.drop_without_applying_deltas();
            click(terminal, SIZE, pos);
            frame(terminal, SIZE, vec![])
        };
        let mut unsupported = terminal();
        frame(&mut unsupported, SIZE, vec![]);
        let shown = menu(&mut unsupported);
        assert!(shown.text_at("Copy symbol").is_some());
        assert!(shown.text_at("Financials").is_none());

        let host = Arc::new(Host::default());
        let mut terminal = Terminal::new(host.clone()).with_symbol("AAPL");
        frame(&mut terminal, SIZE, vec![]);
        assert!(menu(&mut terminal).text_at("Financials").is_none());
        egui::Popup::close_all(&terminal.ctx.clone().unwrap());
        let (symbol, reply) = host.0.lock().unwrap().pop().unwrap();
        assert_eq!(symbol, "AAPL");
        let figures = |name: &str, value| {
            CompanyFinancials::new(
                Some("USD".into()),
                vec![
                    FinancialPage::new("Income statement", vec!["FY 2025".into()])
                        .with_row(name, vec![Some(value)]),
                ],
            )
        };
        reply.send(Ok(figures("Host revenue", 42.0)));
        let shown = menu(&mut terminal);
        click(
            &mut terminal,
            SIZE,
            shown.text_at("Financials").expect("host figures available"),
        );
        let shown = frame(&mut terminal, SIZE, vec![]);
        assert!(shown.text_at("Back to chart").is_some());
        let shown = texts(&mut terminal);
        assert!(shown.iter().any(|text| text == "Host company"));
        assert!(shown.iter().any(|text| text == "Host revenue"));
        assert!(shown.iter().any(|text| text == "42.00"));
        assert_eq!(terminal.panel, None, "analysis gets the whole workspace");
        assert!(
            host.0.lock().unwrap().is_empty(),
            "do not request per frame"
        );

        terminal.switch_symbol("MSFT");
        frame(&mut terminal, SIZE, vec![]);
        let (_, late) = host.0.lock().unwrap().pop().unwrap();
        assert!(
            !texts(&mut terminal)
                .iter()
                .any(|text| text == "Host revenue")
        );
        terminal.switch_symbol("NVDA");
        frame(&mut terminal, SIZE, vec![]);
        let (symbol, current) = host.0.lock().unwrap().pop().unwrap();
        assert_eq!(symbol, "NVDA");
        current.send(Ok(figures("Current revenue", 84.0)));
        late.send(Ok(figures("Stale revenue", 21.0)));
        let shown = texts(&mut terminal);
        assert!(shown.iter().any(|text| text == "Current revenue"));
        assert!(!shown.iter().any(|text| text == "Stale revenue"));
        let shown = frame(&mut terminal, SIZE, vec![]);
        click(&mut terminal, SIZE, shown.text_at("Back to chart").unwrap());
        assert!(
            frame(&mut terminal, SIZE, vec![])
                .text_at("Back to chart")
                .is_none()
        );
    }

    /// Host pushes reach hidden panels once, quotes cover heatmap symbols outside the list,
    /// and bar-derived figures use returns from a known base rather than price differences.
    #[test]
    fn market_panels_follow_host_quotes_notifications_and_bar_returns() {
        use egui_charts::model::Quote;
        use egui_charts::services::{
            Events, Notification, Notifications, Profile, ProfileEvent, Quotes,
        };
        use egui_charts::ui::widget_bar::{MarketHeatmapEntry, MarketHeatmapUniverse};
        use std::sync::Mutex;

        #[derive(Default)]
        struct Host {
            quotes: Mutex<Vec<Events<(String, Quote)>>>,
            notices: Mutex<Option<Events<Notification>>>,
        }
        impl Quotes for Host {
            fn subscribe(&self, symbols: &[String], events: Events<(String, Quote)>) {
                for symbol in symbols {
                    events.push((
                        symbol.clone(),
                        Quote::new(110.0, 100.0, DateTime::UNIX_EPOCH),
                    ));
                }
                self.quotes.lock().unwrap().push(events);
            }
        }
        impl Notifications for Host {
            fn watch(&self, events: Events<Notification>) {
                *self.notices.lock().unwrap() = Some(events);
            }
        }
        impl Profiles for Host {
            fn watch(&self, events: Events<ProfileEvent>) {
                events.push(ProfileEvent::Upsert(Profile::new(
                    ProfileId("author".into()),
                    "Host author",
                )));
            }
            fn follow(&self, _: ProfileId, _: bool, reply: Reply<()>) {
                reply.send(Ok(()));
            }
        }
        let host = Arc::new(Host::default());
        let mut terminal = terminal()
            .with_quotes(host.clone())
            .with_notifications(host.clone())
            .with_profiles(host.clone());
        terminal
            .panels_mut()
            .heatmap
            .universes
            .push(MarketHeatmapUniverse::new(
                "test",
                "Test",
                vec![MarketHeatmapEntry {
                    symbol: "OUTSIDE".into(),
                    group: "Test".into(),
                    values: BTreeMap::new(),
                }],
            ));
        frame(&mut terminal, SIZE, vec![]);
        frame(&mut terminal, SIZE, vec![]);
        let values = &terminal.panels.market.heatmap.universes[0].entries[0].values;
        assert_eq!(values["price"], 110.0);
        // (110 - 100) / 100 * 100 = 10 percent, independent of tile size.
        assert!((values["change"] - 10.0).abs() < 1e-12);
        let mut visibility = terminal.chart().event_visibility().clone();
        visibility.show_news_notifications = true;
        terminal
            .chart_mut()
            .set_event_visibility(visibility.clone());
        let events = host.notices.lock().unwrap().clone().unwrap();
        events.push(Notification::new(DateTime::UNIX_EPOCH, "Host headline"));
        frame(&mut terminal, SIZE, vec![]);
        assert_eq!(terminal.panels.market.notifications.unread(), 1);
        assert_eq!(terminal.toasts.len(), 1);
        frame(&mut terminal, SIZE, vec![]);
        assert_eq!(
            terminal.toasts.len(),
            1,
            "do not repeat a push on another frame"
        );
        visibility.show_news_notifications = false;
        terminal.chart_mut().set_event_visibility(visibility);
        events.push(Notification::new(DateTime::UNIX_EPOCH, "Quiet headline"));
        frame(&mut terminal, SIZE, vec![]);
        assert_eq!(terminal.toasts.len(), 1);
        assert_eq!(
            terminal.panels.market.notifications.unread(),
            2,
            "the setting leaves the log intact"
        );
        terminal.run(Action::WidgetBar(WidgetBarAction::Clicked(
            RightBarIcon::Notifications,
        )));
        assert!(
            texts(&mut terminal)
                .iter()
                .any(|text| text == "Host headline")
        );
        assert_eq!(terminal.panels.market.notifications.unread(), 0);
        assert!(
            terminal
                .run(Action::Notifications(NotificationsAction::OpenProfile(
                    ProfileId("author".into())
                )))
                .is_none()
        );
        let _ = texts(&mut terminal);
        assert!(
            texts(&mut terminal)
                .iter()
                .any(|text| text == "Host author")
        );
        let bar = |time: &str, open: f64, close: f64| {
            Bar::new(
                time.parse().unwrap(),
                open,
                open.max(close),
                open.min(close),
                close,
                1.0,
            )
        };
        let bars = [
            bar("2024-12-31T00:00:00Z", 100.0, 100.0),
            bar("2025-01-01T00:00:00Z", 100.0, 110.0),
            bar("2025-01-31T00:00:00Z", 110.0, 120.0),
            bar("2025-02-01T00:00:00Z", 120.0, 110.0),
            bar("2025-02-28T00:00:00Z", 110.0, 90.0),
            bar("2025-12-25T00:00:00Z", 90.0, 100.0),
            bar("2026-01-01T00:00:00Z", 100.0, 110.0),
        ];
        let returns = period_returns(&bars);
        // YTD runs from the last close of 2025, not from the 2026 bar itself.
        for (period, expected) in [("1W", 10.0), ("YTD", 10.0), ("1Y", 0.0)] {
            let found = returns
                .iter()
                .find(|row| row.period == period)
                .unwrap()
                .percent
                .unwrap();
            assert!((found - expected).abs() < 1e-12, "{period}: {found}");
        }
        let seasonal = seasonality(&bars);
        assert_eq!(seasonal.years, 1);
        // January: (120 / 100 - 1) * 100 = 20. February: (90 / 120 - 1) * 100 = -25.
        for (month, expected, share) in [(1, 20.0, 1.0), (2, -25.0, 0.0)] {
            let row = seasonal
                .months
                .iter()
                .find(|row| row.month == month)
                .unwrap();
            assert!((row.average_percent.unwrap() - expected).abs() < 1e-12);
            assert_eq!(row.positive_share, Some(share));
        }
        assert!(period_returns(&[]).is_empty());
        let edge = [Bar::new(
            DateTime::<Utc>::MIN_UTC,
            100.0,
            100.0,
            100.0,
            100.0,
            1.0,
        )];
        assert!(
            period_returns(&edge)
                .iter()
                .filter(|row| row.period != "YTD")
                .all(|row| row.percent.is_none())
        );
        assert!(seasonality(&bars[..1]).months.is_empty());
    }
}
