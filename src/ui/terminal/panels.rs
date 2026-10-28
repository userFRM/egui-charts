//! Panels: the widget bar, the side panels, the market panels and the notification log.

use super::core::{Direction, SidePanel};
use super::layout::{Region, panel_frame};
use super::{Action, ExportContents, Terminal};
use crate::icons::{Icon, icons};
use crate::model::Tick;
use crate::services::UserSettings;
use crate::ui::financials::FinancialsPanel;
use crate::ui::top_toolbar::TopToolbarAction;
use crate::ui::widget_bar::panels::object_tree::{DataWindowInfo, show_data_window};
use crate::ui::widget_bar::{
    CalendarAction, CalendarPanel, ChatsPanel, HotlistsAction, HotlistsPanel, MarketHeatmapPanel,
    NewsAction, NewsPanel, NotificationsAction, NotificationsPanel, ProductsAction, ProductsPanel,
    RightBarIcon, ScreenerAction, ScreenerPanel, SymbolDetailsPanel, TimeAndSalesPanel,
    WatchlistAction, WatchlistPanel, WidgetBar, WidgetBarAction, WidgetBarSection, WidgetBarWidget,
};
use crate::widget::{SeriesId, ShortcutRequest};
use chrono::{DateTime, Utc};

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
    market: MarketPanels,
    /// The chart symbol's financials, shown in place of the chart while they are open.
    financials: Option<(String, FinancialsPanel)>,
    /// Where the chart is, as the last bottom pass left it.
    chart_area: egui::Rect,
    data_window_expanded: bool,
    /// The text of the open import window.
    import: Option<String>,
    /// Whether the next symbol picked goes to the watchlist, not the chart.
    adding_symbol: bool,
    /// The symbol whose trades the time and sales panel lists.
    tape_symbol: String,
    /// The newest trade listed: its time and how many trades share that time.
    tape_cursor: Option<(DateTime<Utc>, usize)>,
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
            financials: None,
            chart_area: egui::Rect::NOTHING,
            data_window_expanded: true,
            import: None,
            adding_symbol: false,
            tape_symbol: String::new(),
            tape_cursor: None,
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
        market.notifications.poll(ctx);
        market.chats.poll(ctx);
        let unread = market.notifications.unread() as u32;
        self.panels
            .widget_bar
            .set_notification_cnt(RightBarIcon::Notifications, unread);
        self.feed_market_panels(ctx);
    }

    /// Hands the chart's symbol, quote, time zone and trades to the panels that follow them.
    fn feed_market_panels(&mut self, ctx: &egui::Context) {
        let chart = &self.charts[self.active];
        let State {
            market,
            financials,
            tape_symbol,
            tape_cursor,
            ..
        } = &mut self.panels;
        let chart_symbol = chart.symbol().to_owned();
        let instrument = chart.instrument();
        let mut selected = chart_symbol.clone();
        for symbol in market
            .watchlist
            .symbols()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        {
            if symbol.eq_ignore_ascii_case(&chart_symbol) {
                selected = symbol;
            }
        }
        if let Some(decimals) = instrument.and_then(|instrument| instrument.price_decimals) {
            market.watchlist.set_price_decimals(&selected, decimals);
        }
        market.watchlist.set_selected(&selected);
        market.watchlist.poll(ctx);
        market.details.set_instrument(
            &instrument
                .cloned()
                .unwrap_or_else(|| crate::model::Instrument::new(&chart_symbol)),
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
        // Open financials follow the chart's symbol.
        if market.details.take_financials_request()
            || financials
                .as_ref()
                .is_some_and(|(symbol, _)| *symbol != chart_symbol)
        {
            let title = instrument
                .and_then(|instrument| instrument.description.clone())
                .unwrap_or_else(|| chart_symbol.clone());
            // The datafeed carries no financials, so the panel says that none are available.
            *financials = Some((chart_symbol.clone(), FinancialsPanel::new(title, None)));
            self.panel = None;
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
            Region::Bottom => self.panels.chart_area = ui.available_rect_before_wrap(),
            Region::Chart => self.show_financials(ui),
            Region::Windows => self.show_import(ui.ctx()),
            Region::Top | Region::Left => {}
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
                !market.heatmap.entries.is_empty(),
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
        // The bodies of the object tree and the alerts panel belong to their groups.
        if matches!(which, SidePanel::ObjectTree | SidePanel::Alerts) {
            return None;
        }
        let chart = &self.charts[self.active];
        let State {
            market,
            data_window_expanded,
            ..
        } = &mut self.panels;
        let mut picked = None;
        egui::Panel::right("egui_charts_terminal_detail_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(220.0)
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
                SidePanel::DataWindow => {
                    *data_window_expanded = show_data_window(
                        ui,
                        &DataWindowInfo::from_chart(chart),
                        *data_window_expanded,
                    );
                }
                SidePanel::ObjectTree | SidePanel::Alerts => {}
            });
        picked
    }

    /// The financials of the chart's symbol, in place of the chart.
    fn show_financials(&mut self, ui: &mut egui::Ui) {
        let area = self.panels.chart_area;
        let Some((_, financials)) = &mut self.panels.financials else {
            return;
        };
        let mut back = false;
        egui::Area::new(ui.scope_id().with("egui_charts_terminal_financials"))
            .order(egui::Order::Middle)
            .fixed_pos(area.min)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(area);
                panel_frame(self.theme.ui.panel_bg)
                    .inner_margin(16)
                    .show(ui, |ui| {
                        ui.set_min_size(area.size() - egui::Vec2::splat(32.0));
                        ui.set_max_size(area.size() - egui::Vec2::splat(32.0));
                        back = financials.show(ui);
                    });
            });
        if back {
            self.panels.financials = None;
        }
    }

    /// There is no file picker for imports: the user pastes the exported text instead.
    fn show_import(&mut self, ctx: &egui::Context) {
        let Some(text) = &mut self.panels.import else {
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
            self.panels.market.watchlist.import("Imported", text);
        }
        if import || !open {
            self.panels.import = None;
        }
    }

    /// Hands exported `text` to the host's file export, or to the clipboard without one.
    fn export_text(&mut self, name: &str, text: String) {
        if self.save_file(name.to_owned(), ExportContents::Text(text.clone())) {
            self.toast(format!("Exported {name}"));
        } else {
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
        let watchlist = &mut self.panels.market.watchlist;
        match direction {
            Direction::Save => settings.watchlists = watchlist.lists().to_vec(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{MemoryStorage, SIZE, TestFeed, frame};
    use super::*;
    use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest};
    use crate::model::{Instrument, Watchlist};
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
            let page = TradePage {
                trades: vec![Tick::new(
                    "2026-01-01T12:34:56Z".parse().unwrap(),
                    181.25,
                    3.0,
                    None,
                )],
                ..TradePage::default()
            };
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
            (WidgetBarAction::Clicked(RightBarIcon::Help), None, true),
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
        assert!(matches!(
            advanced,
            Some(Action::Watchlist(WatchlistAction::AdvancedView))
        ));
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
}
