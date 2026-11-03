//! Trading: the broker picker, the order ticket, the order and position lines, and the bottom dock
//! that holds the account manager and the strategy report.

use super::core::Direction;
use super::layout::{Region, panel_frame};
use super::{Action, ExportContents, Terminal};
use egui::{Role, Sense, WidgetInfo, pos2, vec2};
use egui_charts::localization as l10n;
use egui_charts::model::{OrderRequest, OrderSide, OrderType, Quote, StrategyReport, Timeframe};
use egui_charts::services::{Events, SignalEvent, Signals, UserSettings};
use egui_charts::ui::Control;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
use egui_charts::ui::strategy_report::{StrategyReportAction, StrategyReportPanel};
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_charts::ui::trading::{AccountPanelAction, BrokerPicker, BrokerPickerAction, Trading};
use egui_charts::widget::{QuoteSide, SeriesId};
use std::sync::Arc;

/// Height of the dock's strip of tabs.
const STRIP: f32 = 36.0;
/// Height of the expanded dock body.
const BODY: f32 = 320.0;
/// A signal stream takes 10,000 pushes between frames; retain the last report fills.
const MARKED_SIGNALS: usize = 10_000;

/// How much of the window the dock takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Dock {
    #[default]
    Closed,
    Normal,
    /// The strip of tabs alone.
    Minimised,
    Maximised,
}

/// The panels the dock offers, in tab order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Tab {
    #[default]
    Account,
    Report,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Account => "Account Manager",
            Self::Report => "Strategy Report",
        }
    }
}

/// What the user did on the dock's strip.
#[derive(Default)]
struct StripClick {
    tab: Option<Tab>,
    minimise: bool,
    maximise: bool,
}

#[derive(Default)]
pub(super) struct State {
    account: Trading,
    /// Report updates are taken once, including a request to clear.
    strategy_report: Option<Option<StrategyReport>>,
    picker: BrokerPicker,
    dock: Dock,
    tab: Tab,
    report: StrategyReportPanel,
    /// The trade of the report selected in its list, by index into the report's trades.
    trade: Option<usize>,
    /// The entries and exits of the mounted report, drawn as signals.
    markers: Option<Arc<ReportMarkers>>,
    /// The chart slots whose signal sources already include the report's.
    marked_slots: usize,
}

impl Terminal {
    /// Shows `report` in the strategy report panel, or closes it with `None`.
    /// Charts retain its last 10,000 fill markers in the report's original trade order.
    pub fn set_strategy_report(&mut self, report: Option<StrategyReport>) {
        self.trading.strategy_report = Some(report);
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    pub(super) fn trading_init(&mut self) {
        if self.services.brokers.is_empty() {
            return;
        }
        self.trading.picker.tiles = self
            .services
            .brokers
            .iter()
            .map(|(tile, _)| tile.clone())
            .collect();
        self.set_control_visible(Control::Trading, true);
        self.top_toolbar.state.can_trade = true;
    }

    pub(super) fn trading_poll(&mut self, ctx: &egui::Context) {
        self.mount_report();
        if !self.trading_enabled() || !self.shown(Control::Trading) {
            for chart in &mut self.charts {
                chart.config_mut().show_quote_boxes = false;
            }
        }
        self.trading.account.display = self.studies.settings.trading.clone();
        if self.services.brokers.is_empty() {
            return;
        }
        self.trading.account.poll(ctx, &mut self.toasts);
        if let Some(sound) = self.trading.account.take_execution_sound() {
            // An empty name asks for the host's first sound.
            let name = if sound.is_empty() {
                self.services.sounds.first().cloned().unwrap_or_default()
            } else {
                sound
            };
            let volume = self.trading.account.display.execution_sound_volume;
            self.play_sound(&name, volume);
        }
    }

    pub(super) fn trading_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Right if self.trading_enabled() => {
                self.trading
                    .account
                    .show_ticket(ui, &self.charts[self.active]);
            }
            // After the drawing toolbar and before the side panels and the bottom bar, so that the
            // dock runs under the bottom bar to the window's right edge.
            Region::Left => self.show_dock(ui),
            Region::Chart => {
                self.highlight_trade(ui);
                if self.trading_enabled() {
                    self.show_trading_lines(ui);
                }
            }
            Region::Windows if self.trading_enabled() => {
                if let Some(action) = self.trading.picker.show(&self.ctx()) {
                    self.emit(Action::BrokerPicker(action));
                }
            }
            _ => {}
        }
    }

    pub(super) fn trading_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            // Apply the drawing toolbar's visibility actions to the broker layers; the next poll
            // hands the settings to the account.
            Action::DrawingToolbar(DrawingToolbarAction::HideAllPoss) => {
                let show = &mut self.studies.settings.trading.show_positions;
                *show = !*show;
            }
            Action::DrawingToolbar(DrawingToolbarAction::HideAllOrders) => {
                let show = &mut self.studies.settings.trading.show_orders;
                *show = !*show;
            }
            Action::StrategyReport(action) => return self.perform_report(action),
            action if !self.trading_enabled() => return Some(action),
            Action::TopToolbar(TopToolbarAction::ToggleTrading) => {
                let trading = &mut self.trading;
                if !trading.account.has_broker() {
                    trading.picker.open();
                } else if trading.dock != Dock::Closed && trading.tab == Tab::Account {
                    trading.dock = Dock::Closed;
                } else {
                    (trading.dock, trading.tab) = (Dock::Normal, Tab::Account);
                }
            }
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::Order(request))) => {
                self.open_ticket(request);
            }
            Action::BrokerPicker(action) => return self.perform_picker(action),
            Action::Account(action) => return self.perform_account(action),
            action => return Some(action),
        }
        None
    }

    pub(super) fn trading_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {
    }

    pub(super) fn trading_modal(&self) -> bool {
        self.trading.picker.is_open()
    }

    /// Whether the host supplied a broker, which is what shows the trading controls.
    fn trading_enabled(&self) -> bool {
        !self.services.brokers.is_empty()
    }

    fn perform_picker(&mut self, action: BrokerPickerAction) -> Option<Action> {
        let BrokerPickerAction::Connect(id) = &action else {
            // Opening a broker account is the host's to offer.
            return Some(Action::BrokerPicker(action));
        };
        let Some((tile, broker)) = self
            .services
            .brokers
            .iter()
            .find(|(tile, _)| tile.id == *id)
        else {
            return Some(Action::BrokerPicker(action));
        };
        let (name, broker) = (tile.name.clone(), broker.clone());
        let trading = &mut self.trading;
        trading.account.set_service(broker);
        trading.account.account_panel.broker = name.clone();
        trading.account.account_panel.accounts = vec![name];
        (trading.dock, trading.tab) = (Dock::Normal, Tab::Account);
        None
    }

    fn perform_account(&mut self, action: AccountPanelAction) -> Option<Action> {
        match action {
            AccountPanelAction::SelectSymbol(symbol) => self.switch_symbol(&symbol),
            AccountPanelAction::Disconnect => {
                self.trading.account.disconnect();
                self.studies.settings.trading.instant_orders = false;
            }
            AccountPanelAction::Minimize => self.toggle_dock(Dock::Minimised),
            AccountPanelAction::Maximize => self.toggle_dock(Dock::Maximised),
            AccountPanelAction::Export { name, csv } => {
                let file = format!("{}.csv", name.to_lowercase().replace(' ', "_"));
                if !self.save_file(file, ExportContents::Text(csv.clone())) {
                    return Some(Action::Account(AccountPanelAction::Export { name, csv }));
                }
            }
            // Account settings and other accounts are the broker's: the host performs them.
            action => return Some(Action::Account(action)),
        }
        None
    }

    fn perform_report(&mut self, action: StrategyReportAction) -> Option<Action> {
        match action {
            StrategyReportAction::SelectTrade(trade) => {
                if self.replay_active {
                    self.perform(Action::TopToolbar(TopToolbarAction::ToggleReplay));
                }
                if self.chart().symbol() != trade.symbol {
                    self.switch_symbol(&trade.symbol);
                }
                if self.chart().timeframe() != Some(trade.timeframe) {
                    self.chart_mut().set_timeframe(trade.timeframe);
                }
                self.chart_mut().scroll_to_time(trade.entry_time);
                self.trading.trade = Some(trade.trade_index);
            }
            StrategyReportAction::ExportSummaryCsv { filename, content } => {
                if !self.save_file(filename.clone(), ExportContents::Text(content.clone())) {
                    return Some(Action::StrategyReport(
                        StrategyReportAction::ExportSummaryCsv { filename, content },
                    ));
                }
            }
            StrategyReportAction::ExportCsv(csv) => {
                let name = "strategy-trades.csv".to_owned();
                if !self.save_file(name, ExportContents::Text(csv.clone())) {
                    return Some(Action::StrategyReport(StrategyReportAction::ExportCsv(csv)));
                }
            }
            action => return Some(Action::StrategyReport(action)),
        }
        None
    }

    fn open_ticket(&mut self, request: OrderRequest) {
        if self.trading.account.has_broker() {
            self.trading.account.open_ticket(request);
        } else {
            self.trading.picker.open();
        }
    }

    /// Minimises or maximises the dock, or restores it when it already is.
    fn toggle_dock(&mut self, dock: Dock) {
        let current = &mut self.trading.dock;
        *current = if *current == dock { Dock::Normal } else { dock };
    }

    /// Shows the report the host set, or removes it, and keeps its entries and exits on the
    /// signals of every chart slot, new ones included.
    fn mount_report(&mut self) {
        let trading = &mut self.trading;
        // Taken once: a report holding NaN never equals itself.
        let changed = trading.strategy_report.is_some();
        if let Some(report) = trading.strategy_report.take() {
            let chart = &self.charts[self.active];
            let bars = match &report {
                Some(report)
                    if report.symbol == chart.symbol()
                        && Some(report.timeframe) == chart.timeframe() =>
                {
                    chart.bars(SeriesId::MAIN)
                }
                _ => &[],
            };
            trading.markers = report
                .clone()
                .map(|report| Arc::new(ReportMarkers { report }));
            if report.is_some() {
                (trading.dock, trading.tab) = (Dock::Normal, Tab::Report);
            }
            trading.report.set_report(report, bars);
            trading.trade = None;
            trading.marked_slots = 0;
        }
        if changed || trading.markers.is_some() && trading.marked_slots < self.charts.len() {
            let first = trading.marked_slots.min(self.charts.len());
            let mut sources = self.services.signals.clone();
            sources.extend(
                trading
                    .markers
                    .iter()
                    .map(|markers| markers.clone() as Arc<dyn Signals>),
            );
            for chart in &mut self.charts[first..] {
                chart.set_signal_sources(sources.clone());
            }
        }
        trading.marked_slots = self.charts.len();
    }

    /// The panels the dock has something to show for.
    fn dock_tabs(&self) -> Vec<Tab> {
        let mut tabs = Vec::new();
        if self.trading_enabled() && self.trading.account.has_broker() {
            tabs.push(Tab::Account);
        }
        if self.trading.report.report().is_some() {
            tabs.push(Tab::Report);
        }
        tabs
    }

    /// The strip of tabs below the bottom bar, with the panel of the selected tab over it.
    fn show_dock(&mut self, ui: &mut egui::Ui) {
        let tabs = self.dock_tabs();
        if tabs.is_empty() || self.trading.dock == Dock::Closed {
            return;
        }
        if !tabs.contains(&self.trading.tab) {
            self.trading.tab = tabs[0];
        }
        let available = ui.available_height();
        let body = match self.trading.dock {
            Dock::Minimised => 0.0,
            Dock::Maximised => (available - 160.0 - STRIP - 5.0).max(BODY),
            _ => BODY,
        };
        // The chart keeps 96 points over the 40 point bottom bar.
        let height = (STRIP + body + 5.0).min((available - 136.0).max(STRIP + 5.0));
        let theme = self.theme.clone();
        let symbol = self.chart().symbol().to_owned();
        let (dock, tab) = (self.trading.dock, self.trading.tab);
        let trading = &mut self.trading;
        let mut click = StripClick::default();
        let mut account = None;
        let mut report = StrategyReportAction::None;
        let mut clear = false;
        egui::Panel::bottom("egui_charts_terminal_dock")
            .exact_size(height)
            .show_separator_line(false)
            .frame(panel_frame(theme.ui.panel_bg).outer_margin(egui::Margin {
                top: 5,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                click = egui::Panel::bottom("egui_charts_terminal_dock_tabs")
                    .exact_size(STRIP)
                    .show_separator_line(false)
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        show_strip(ui, &theme, &tabs, tab, dock != Dock::Minimised)
                    })
                    .inner;
                ui.take_available_space();
                if dock == Dock::Minimised {
                    return;
                }
                match tab {
                    Tab::Account => account = trading.account.show_account_panel(ui, &symbol),
                    Tab::Report => {
                        clear = ui.button(l10n::text(ui.ctx(), "Clear report")).clicked();
                        report = trading.report.show(ui);
                    }
                }
            });
        if let Some(clicked) = click.tab {
            let same = clicked == tab && dock != Dock::Minimised;
            self.trading.tab = clicked;
            self.trading.dock = if same { Dock::Minimised } else { Dock::Normal };
        }
        if click.minimise {
            self.toggle_dock(Dock::Minimised);
        }
        if click.maximise {
            self.toggle_dock(Dock::Maximised);
        }
        if clear {
            self.trading.strategy_report = Some(None);
        }
        if let Some(action) = account {
            self.emit(Action::Account(action));
        }
        if report != StrategyReportAction::None {
            self.emit(Action::StrategyReport(report));
        }
    }

    /// The quote buttons, the order and position lines and their confirmation, over the chart.
    fn show_trading_lines(&mut self, ui: &mut egui::Ui) {
        let trading = &mut self.trading;
        let chart = &mut self.charts[self.active];
        if self.replay_active
            && let Some(bar) = chart
                .bars(SeriesId::MAIN)
                .iter()
                .rfind(|bar| !bar.is_whitespace())
        {
            // Replay has no historical spread: its close prices the ticket.
            let quote = Quote::new(bar.close, bar.close, bar.time);
            trading.account.set_quote(chart.symbol(), quote);
        }
        if let Some(side) = chart.take_quote_action() {
            if !trading.account.has_broker() {
                trading.picker.open();
            } else if side == QuoteSide::Ticket {
                let quantity = trading.account.quantity;
                let request =
                    OrderRequest::new(chart.symbol(), OrderSide::Buy, quantity, OrderType::Market);
                trading.account.open_ticket(request);
            } else {
                let side = if side == QuoteSide::Buy {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                trading.account.trade(chart.symbol().to_owned(), side);
            }
        }
        trading.account.show(ui, chart);
        self.studies.settings.trading = trading.account.display.clone();
        self.menus
            .set_order_entries(trading.account.quantity, trading.account.capabilities());
    }

    /// Shades the span of the selected trade of the report on the chart it was run on.
    fn highlight_trade(&self, ui: &egui::Ui) {
        let chart = self.chart();
        let Some(report) = self.trading.report.report().filter(|report| {
            report.symbol == chart.symbol() && Some(report.timeframe) == chart.timeframe()
        }) else {
            return;
        };
        let Some(trade) = self
            .trading
            .trade
            .and_then(|index| report.trades.get(index))
        else {
            return;
        };
        let Some(left) = chart
            .time_to_index(trade.entry_time)
            .and_then(|index| chart.index_to_x(index))
        else {
            return;
        };
        let mapping = chart.get_chart_mapping();
        let right = trade
            .exit
            .as_ref()
            .and_then(|exit| chart.time_to_index(exit.time))
            .and_then(|index| chart.index_to_x(index))
            .unwrap_or(mapping.rect.right());
        let span = egui::Rect::from_min_max(
            pos2(left, mapping.rect.top()),
            pos2(right, mapping.rect.bottom()),
        )
        .intersect(mapping.rect);
        if span.is_positive() {
            ui.painter()
                .rect_filled(span, 0.0, self.theme.ui.accent.linear_multiply(0.12));
        }
    }
}

/// Draws the tabs at the left of the strip and the minimise and maximise buttons at its right,
/// unless the open account panel shows its own, and returns what was clicked. The selected tab is
/// underlined while the dock is open.
fn show_strip(
    ui: &mut egui::Ui,
    theme: &egui_charts::theme::Theme,
    tabs: &[Tab],
    selected: Tab,
    expanded: bool,
) -> StripClick {
    let mut click = StripClick::default();
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP), Sense::hover());
    let painter = ui.painter().clone();
    let mut x = strip.left() + 18.0;
    for &tab in tabs {
        let label = l10n::text(ui.ctx(), tab.label());
        let galley = painter.layout_no_wrap(
            label.clone(),
            egui::FontId::proportional(14.0),
            theme.ui.text,
        );
        let rect = egui::Rect::from_min_size(pos2(x, strip.top()), vec2(galley.size().x, STRIP));
        let response = ui.interact(rect, ui.scope_id().with(("dock_tab", tab)), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(Role::Tab, true, &label));
        let active = expanded && tab == selected;
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_selected(active));
        painter.galley(
            pos2(rect.left(), strip.center().y - galley.size().y / 2.0),
            galley,
            theme.ui.text,
        );
        if active {
            painter.hline(
                rect.x_range(),
                strip.bottom() - 1.0,
                egui::Stroke::new(2.0, theme.ui.accent),
            );
        }
        click.tab = click.tab.or(response.clicked().then_some(tab));
        x = rect.right() + 34.0;
    }
    // Buttons 23 points apart, the last ending 15 points from the edge.
    let maximise = pos2(strip.right() - 15.0 - 8.0, strip.center().y);
    let minimise = maximise - vec2(23.0, 0.0);
    let buttons = if expanded && selected == Tab::Account {
        &[][..]
    } else {
        &[(minimise, "Minimize panel"), (maximise, "Maximize panel")][..]
    };
    for &(centre, name) in buttons {
        let rect = egui::Rect::from_center_size(centre, vec2(22.0, 22.0));
        let label = l10n::text(ui.ctx(), name);
        let response = ui
            .interact(rect, ui.scope_id().with(name), Sense::click())
            .on_hover_text(&label);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
        let color = if response.hovered() {
            theme.ui.text
        } else {
            theme.ui.text_muted
        };
        if centre == minimise {
            let bar = [centre - vec2(7.0, 0.0), centre + vec2(7.0, 0.0)];
            painter.line_segment(bar, egui::Stroke::new(1.5, color));
            click.minimise = response.clicked();
        } else {
            painter.rect_stroke(
                egui::Rect::from_center_size(centre, vec2(16.0, 16.0)),
                3.0,
                egui::Stroke::new(1.5, color),
                egui::StrokeKind::Inside,
            );
            click.maximise = response.clicked();
        }
    }
    if expanded {
        painter.hline(
            strip.x_range(),
            strip.top(),
            egui::Stroke::new(1.0, theme.ui.border),
        );
    }
    click
}

/// The entries and exits of a strategy report as markers: entries of longs and exits of shorts
/// point up, the others down. Open trades have no exit.
struct ReportMarkers {
    report: StrategyReport,
}

impl Signals for ReportMarkers {
    fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
        if self.report.symbol != symbol || self.report.timeframe != timeframe {
            return;
        }
        for signal in self
            .report
            .signals()
            .into_iter()
            .rev()
            .take(MARKED_SIGNALS)
            .rev()
        {
            events.push(SignalEvent::Upsert(signal));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ExportFile;
    use super::super::test_support::{SIZE, TestFeed, click, frame};
    use super::*;
    use chrono::{DateTime, TimeDelta, TimeZone, Utc};
    use egui::{Color32, Pos2, RawInput, Rect, Shape, Vec2};
    use egui_charts::model::{StrategyExit, StrategyTrade};
    use egui_charts::services::PaperAccount;
    use egui_charts::ui::trading::BrokerTile;
    use egui_charts::widget::{ChartBuilder, ChartLayoutMode};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct HostSignals;

    impl Signals for HostSignals {
        fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
            use egui_charts::model::{Signal, SignalId, SignalShape};
            events.push(SignalEvent::Upsert(Signal {
                id: SignalId("host".into()),
                shape: SignalShape::Marker {
                    time: feed_end() - TimeDelta::milliseconds(timeframe.duration_ms()) * 250,
                    price: Some(if symbol == "BTCUSD" { 61_000.0 } else { 180.0 }),
                    direction: egui_charts::model::Direction::Neutral,
                },
                category: "Host".into(),
                label: Some("Host analytics".into()),
                tooltip: None,
            }));
        }
    }

    fn paper_terminal() -> Terminal {
        let paper = Arc::new(PaperAccount::new("USD", 100_000.0));
        paper.set_price("AAPL", 110.0);
        Terminal::new(Arc::new(TestFeed))
            .with_symbol("AAPL")
            .with_broker(BrokerTile::paper(), paper)
    }

    fn connect(terminal: &mut Terminal) {
        let connect = BrokerPickerAction::Connect(BrokerTile::PAPER.into());
        assert!(terminal.run(Action::BrokerPicker(connect)).is_none());
    }

    /// The last bar of the test feed is the hour before this time.
    fn feed_end() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap()
    }

    /// Two closed round trips on BTCUSD hourly bars, a long 300 hours back and a short 20 back.
    fn two_trade_report() -> StrategyReport {
        let hours = |ago| feed_end() - TimeDelta::hours(ago);
        let trade = |side, entry, exit, entry_price, exit_price, profit| {
            let mut trade = StrategyTrade::new(side, 1.0, hours(entry), entry_price);
            trade.exit = Some(StrategyExit::new(hours(exit), exit_price, profit));
            trade
        };
        let mut report = StrategyReport::new("Test run", "BTCUSD", Timeframe::Hour1, 10_000.0);
        report.currency = Some("USD".into());
        report.trades = vec![
            trade(OrderSide::Buy, 300, 290, 60_000.0, 60_500.0, 500.0),
            trade(OrderSide::Sell, 20, 10, 61_000.0, 60_800.0, 200.0),
        ];
        // A point the tester could not value reads as NaN.
        report.equity = vec![
            (hours(300), 10_000.0),
            (hours(150), f64::NAN),
            (hours(10), 10_700.0),
        ];
        report
    }

    /// The text painted with its position, and the rectangles filled.
    type Painted = (Vec<(String, Pos2)>, Vec<(Color32, Rect)>, Vec<Shape>);

    /// Runs one frame like `frame` and returns the text painted with its position and the
    /// rectangles filled.
    fn painted(terminal: &mut Terminal, size: Vec2) -> Painted {
        let ctx = terminal.ctx.clone().unwrap();
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(ctx.input(|i| i.time) + 0.1),
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            terminal.show(ui);
        });
        let (mut texts, mut rects, mut markers) = (Vec::new(), Vec::new(), Vec::new());
        for clipped in &output.shapes {
            match &clipped.shape {
                Shape::Text(text) => texts.push((text.galley.text().to_owned(), text.pos)),
                Shape::Rect(rect) => rects.push((rect.fill, rect.rect)),
                Shape::Path(path)
                    if path.closed && path.points.len() == 3 && path.stroke.width == 1.0 =>
                {
                    markers.push(clipped.shape.clone());
                }
                Shape::Circle(circle)
                    if circle.radius == 4.0 && circle.fill == terminal.theme.ui.accent =>
                {
                    markers.push(clipped.shape.clone())
                }
                _ => {}
            }
        }
        output.drop_without_applying_deltas();
        (texts, rects, markers)
    }

    fn settle(terminal: &mut Terminal) {
        for _ in 0..3 {
            frame(terminal, SIZE, vec![]);
        }
    }

    #[test]
    fn trading_controls_follow_the_broker_and_the_dock_the_open_panels() {
        let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
        terminal
            .chart_mut()
            .set_quote(Some(egui_charts::model::Level1Quote {
                symbol: "AAPL".into(),
                ts: feed_end(),
                bid: 179.0,
                ask: 180.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
        assert!(
            frame(&mut terminal, SIZE, vec![])
                .text_at("Trade")
                .is_none()
        );

        assert!(
            frame(&mut terminal, SIZE, vec![])
                .text_at("Buy at ask")
                .is_none()
        );
        let mut hidden = paper_terminal().hide(Control::Trading);
        assert!(frame(&mut hidden, SIZE, vec![]).text_at("Trade").is_none());
        assert!(!hidden.chart().config().show_quote_boxes);

        let mut terminal = paper_terminal();
        let trade = frame(&mut terminal, SIZE, vec![]).text_at("Trade");
        click(&mut terminal, SIZE, trade.expect("Trade button"));
        assert!(terminal.trading_modal(), "Trade opens the broker picker");
        settle(&mut terminal);
        let picker = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            picker.text_at("Paper Trading").expect("paper broker"),
        );
        let card = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            card.text_at("Connect").expect("connect card"),
        );
        assert!(!terminal.trading_modal());
        settle(&mut terminal);
        let tabs = frame(&mut terminal, SIZE, vec![]);
        assert!(tabs.text_at("Account Manager").is_some());
        assert!(tabs.text_at("Strategy Report").is_none());

        terminal.set_strategy_report(Some(two_trade_report()));
        settle(&mut terminal);
        let tabs = frame(&mut terminal, SIZE, vec![]);
        assert!(tabs.text_at("Account Manager").is_some());
        let report_tab = tabs.text_at("Strategy Report").expect("report tab");
        let clear = tabs.text_at("Clear report").expect("report panel");
        assert!(report_tab.y > clear.y, "the panel opens above its tabs");

        click(&mut terminal, SIZE, report_tab);
        assert_eq!(terminal.trading.dock, Dock::Minimised);
        let (texts, rects, _) = painted(&mut terminal, SIZE);
        assert!(texts.iter().all(|(text, _)| text != "Clear report"));
        let strip = rects
            .iter()
            .find(|(fill, rect)| {
                *fill == terminal.theme.ui.panel_bg
                    && (rect.height() - STRIP).abs() < 0.1
                    && rect.width() > 1000.0
            })
            .expect("36 point dock strip")
            .1;
        assert_eq!(strip.left(), 54.0);
        assert_eq!(strip.right(), SIZE.x);
        assert_eq!(strip.bottom(), SIZE.y);
        let tabs = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            tabs.text_at("Account Manager").unwrap(),
        );
        assert_eq!(
            (terminal.trading.dock, terminal.trading.tab),
            (Dock::Normal, Tab::Account)
        );
        for expected in [Dock::Maximised, Dock::Normal] {
            terminal.run(Action::Account(AccountPanelAction::Maximize));
            assert_eq!(terminal.trading.dock, expected);
        }
        terminal.set_strategy_report(None);
        settle(&mut terminal);
        let tabs = frame(&mut terminal, SIZE, vec![]);
        assert!(tabs.text_at("Account Manager").is_some());
        assert!(tabs.text_at("Strategy Report").is_none());
        for dock in [Dock::Normal, Dock::Maximised] {
            terminal.trading.dock = dock;
            let ctx = terminal.ctx();
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640.0, 420.0))),
                    ..Default::default()
                },
                |ui| {
                    terminal.show_dock(ui);
                    assert!(
                        ui.available_height() >= 136.0,
                        "dock leaves space for the chart and bottom bar"
                    );
                },
            )
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn hiding_positions_and_orders_toggles_their_lines() {
        let mut terminal = paper_terminal();
        frame(&mut terminal, SIZE, vec![]);
        let display = |terminal: &Terminal| {
            let display = &terminal.trading.account.display;
            (display.show_positions, display.show_orders)
        };
        assert_eq!(display(&terminal), (true, true));
        for (action, expected) in [
            (DrawingToolbarAction::HideAllPoss, (false, true)),
            (DrawingToolbarAction::HideAllOrders, (false, false)),
            (DrawingToolbarAction::HideAllPoss, (true, false)),
        ] {
            assert!(terminal.run(Action::DrawingToolbar(action)).is_none());
            frame(&mut terminal, SIZE, vec![]);
            assert_eq!(display(&terminal), expected);
        }
        use egui_charts::ui::top_toolbar::components::settings_menu::SettingsAction;
        let mut settings = terminal.studies.settings.clone();
        settings.trading.show_orders = false;
        settings.trading.show_executions = false;
        terminal.run(Action::Settings(SettingsAction::Apply(Box::new(settings))));
        frame(&mut terminal, SIZE, vec![]);
        assert_eq!(display(&terminal), (true, false));
        assert!(!terminal.trading.account.display.show_executions);
        let settings = terminal.studies.settings.clone();
        terminal.run(Action::DrawingToolbar(DrawingToolbarAction::HideAllOrders));
        terminal.run(Action::Settings(SettingsAction::Apply(Box::new(settings))));
        frame(&mut terminal, SIZE, vec![]);
        assert_eq!(display(&terminal), (true, false));
    }

    #[test]
    fn replay_closes_price_the_ticket_below_the_notices() {
        let mut terminal = paper_terminal();
        settle(&mut terminal);
        // The watchlist opens beside the chart; closed, a 640 point window keeps room for the
        // quote buttons beside the ticket.
        terminal.panel = None;
        let start = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let bars = [100.0, 80.0, 110.0]
            .into_iter()
            .enumerate()
            .map(|(i, price)| {
                let time = start + TimeDelta::minutes(i as i64);
                egui_charts::model::Bar::new(time, price, price, price, price, 1.0)
            });
        terminal
            .chart_mut()
            .set_data(SeriesId::MAIN, bars.collect());
        terminal.replay_active = true;
        connect(&mut terminal);
        terminal.studies.settings.trading.instant_orders = true;
        terminal.trading.account.display.instant_orders = true;
        for _ in 0..2 {
            terminal
                .trading
                .account
                .trade("AAPL".into(), OrderSide::Buy);
            frame(&mut terminal, SIZE, vec![]);
        }
        assert!(!terminal.toasts.toasts().is_empty(), "fills raise notices");
        for toast in terminal.toasts.toasts_mut() {
            toast.duration = 0.0;
        }
        terminal.studies.settings.trading.instant_orders = false;
        // A live quote must not price the ticket while the replay close does.
        for (side, width) in [(OrderSide::Buy, 1440.0), (OrderSide::Sell, 640.0)] {
            let live = Quote::new(999.0, 990.0, Utc::now()).with_bid_ask(998.0, 1_000.0);
            terminal.trading.account.set_quote("AAPL", live);
            terminal
                .chart_mut()
                .set_quote(Some(egui_charts::model::Level1Quote {
                    symbol: "AAPL".into(),
                    ts: start,
                    bid: 110.0,
                    ask: 110.0,
                    bid_size: 1.0,
                    ask_size: 1.0,
                }));
            let size = vec2(width, 900.0);
            let label = if side == OrderSide::Buy {
                "Buy at ask"
            } else {
                "Sell at bid"
            };
            let quotes = frame(&mut terminal, size, vec![]);
            click(
                &mut terminal,
                size,
                quotes.text_at(label).expect("chart quote button"),
            );
            let ticket_open = (0..3).any(|_| {
                painted(&mut terminal, size)
                    .0
                    .iter()
                    .any(|(text, _)| text == "Trade value")
            });
            assert!(ticket_open, "{side:?} quote opens the ticket at {width}");
            let order = OrderRequest::new("AAPL", side, 1.0, OrderType::Market);
            let menu = ChartContextAction::Chart(ContextMenuAction::Order(order));
            assert!(terminal.run(Action::ChartMenu(menu)).is_none());
            let mut ticket_left = None;
            let mut priced = false;
            for _ in 0..3 {
                let (texts, _, _) = painted(&mut terminal, size);
                let find = |label| texts.iter().find(|(text, _)| text == label);
                if let Some((_, value)) = find("Trade value") {
                    ticket_left = Some(value.x);
                    // The chart's price axis also reads 110.00: the ticket's own row counts.
                    priced |= texts
                        .iter()
                        .any(|(text, pos)| text == "110.00" && (pos.y - value.y).abs() < 2.0);
                }
            }
            assert!(priced, "{side:?} ticket must use the replay close");
            let ticket_left = ticket_left.expect("order ticket");
            let ctx = terminal.ctx();
            for toast in terminal.toasts.toasts().iter().rev().take(5) {
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::unique("toast").with(toast.id)))
                    .expect("trade notice");
                assert!(
                    rect.right() <= ticket_left,
                    "notice covers ticket at {width}"
                );
            }
        }
    }

    #[test]
    fn report_selects_navigates_exports_replaces_and_clears() {
        for dark in [false, true] {
            for size in [vec2(640.0, 900.0), SIZE] {
                let saved = Rc::new(RefCell::new(Vec::<ExportFile>::new()));
                let sink = saved.clone();
                let mut terminal = Terminal::new(Arc::new(TestFeed))
                    .with_symbol("AAPL")
                    .with_signals(Arc::new(HostSignals))
                    .with_file_export(&[], move |file| {
                        sink.borrow_mut().push(file);
                        Ok(())
                    });
                frame(&mut terminal, size, vec![]);
                // Closed, the watchlist leaves both chart slots room for the fills and markers.
                terminal.panel = None;
                if dark {
                    terminal.run(Action::TopToolbar(TopToolbarAction::ThemeToggle));
                }
                let empty = frame(&mut terminal, size, vec![]);
                assert!(empty.text_at("Clear report").is_none());

                let report = two_trade_report();
                terminal.set_strategy_report(Some(report.clone()));
                for _ in 0..3 {
                    frame(&mut terminal, size, vec![]);
                }
                let trades = frame(&mut terminal, size, vec![]).text_at("List of trades");
                click(&mut terminal, size, trades.expect("trades tab"));

                // Selecting a trade restores the report's symbol and interval and scrolls to it.
                let first = frame(&mut terminal, size, vec![]).text_at("1");
                click(&mut terminal, size, first.expect("first trade"));
                assert_eq!(terminal.chart().symbol(), "BTCUSD");
                assert_eq!(terminal.chart().timeframe(), Some(Timeframe::Hour1));
                assert_eq!(terminal.trading.trade, Some(0));
                for _ in 0..3 {
                    frame(&mut terminal, size, vec![]);
                }
                let range = terminal.chart().visible_time_range().unwrap();
                assert!(
                    range.from <= report.trades[0].entry_time
                        && range.to >= report.trades[0].entry_time
                );
                let (_, rects, _) = painted(&mut terminal, size);
                let shade = terminal.theme.ui.accent.linear_multiply(0.12);
                let height = terminal.chart().get_chart_mapping().rect.height();
                assert!(
                    rects
                        .iter()
                        .any(|(fill, rect)| *fill == shade && rect.height() == height)
                );

                let export = frame(&mut terminal, size, vec![]).text_at("Export trades CSV");
                click(&mut terminal, size, export.expect("export button"));
                let files = saved.borrow();
                let [file] = files.as_slice() else {
                    panic!("one export, got {}", files.len());
                };
                assert_eq!(file.name, "strategy-trades.csv");
                assert!(
                    matches!(&file.contents, ExportContents::Text(csv) if *csv == report.trades_csv())
                );
                drop(files);

                let export = frame(&mut terminal, size, vec![]).text_at("Export summary CSV");
                click(&mut terminal, size, export.expect("summary export"));
                assert_eq!(saved.borrow().len(), 2);
                assert!(
                    matches!(&saved.borrow()[1].contents, ExportContents::Text(csv) if csv.contains("Net profit"))
                );

                // A replaced report starts without a selection, and every slot marks its fills.
                let mut second = report.clone();
                second.name = "Second run".into();
                terminal.set_strategy_report(Some(second));
                for _ in 0..3 {
                    frame(&mut terminal, size, vec![]);
                }
                assert_eq!(terminal.trading.trade, None);
                let slot = ChartBuilder::new()
                    .with_feed(Arc::new(TestFeed))
                    .with_timeframe(Timeframe::Hour1)
                    .build();
                terminal.charts.push(slot);
                terminal.wire_slot(1);
                terminal.charts[1].set_symbol("BTCUSD");
                terminal.layout.mode = ChartLayoutMode::TwoHorizontal;
                for _ in 0..4 {
                    terminal
                        .charts
                        .iter_mut()
                        .for_each(|chart| chart.fit_content());
                    frame(&mut terminal, size, vec![]);
                }
                let output = painted(&mut terminal, size);
                let last = report.trades.last().unwrap();
                for chart in &terminal.charts {
                    for (time, price) in [
                        (last.entry_time, last.entry_price),
                        (
                            last.exit.as_ref().unwrap().time,
                            last.exit.as_ref().unwrap().price,
                        ),
                    ] {
                        let x = chart
                            .time_to_index(time)
                            .and_then(|index| chart.index_to_x(index.floor()))
                            .unwrap();
                        let point = pos2(x, chart.get_chart_mapping().price_to_y(price));
                        assert!(
                            output.2.iter().any(|shape| {
                                let bounds = shape.visual_bounding_rect();
                                matches!(shape, Shape::Path(_))
                                    && bounds.width() <= 16.0
                                    && bounds.height() <= 16.0
                                    && bounds.expand(0.1).contains(point)
                            }),
                            "report fill on every slot"
                        );
                    }
                }
                let accent = terminal.theme.ui.accent;
                let host_marker = |shapes: &[Shape]| {
                    shapes.iter().any(|shape| matches!(shape, Shape::Circle(circle) if circle.radius == 4.0 && circle.fill == accent))
                };
                assert!(host_marker(&output.2));

                let clear = frame(&mut terminal, size, vec![]).text_at("Clear report");
                click(&mut terminal, size, clear.expect("clear button"));
                for _ in 0..3 {
                    frame(&mut terminal, size, vec![]);
                }
                assert!(terminal.trading.report.report().is_none());
                let (texts, _, markers) = painted(&mut terminal, size);
                assert!(markers.iter().all(|shape| !matches!(shape, Shape::Path(_))));
                assert!(texts.iter().all(|(text, _)| text != "Clear report"));
                assert!(host_marker(&markers));
            }
        }
    }
}
