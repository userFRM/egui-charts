//! The account panel: the connected account's summary, positions, working orders, order and
//! balance history and trading journal, shown at the bottom of the terminal.

use crate::localization as l10n;
use std::collections::HashSet;

use chrono::{DateTime, Utc};
use egui::{Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, vec2};

use super::Trading;
use crate::chart::TradingLineAction;
use crate::icons::icons;
use crate::model::{
    BalanceChange, JournalEntry, Order, OrderRequest, OrderSide, OrderStatus, OrderType, Position,
};
use crate::scales::{CurrencyFormatter, PriceFormatter};
use crate::services::AccountEvent;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::ButtonSize;
use crate::ui_kit::{EmptyState, IconButton};

const TAB_HEIGHT: f32 = 34.0;
const HEADER_HEIGHT: f32 = 52.0;
const ROW_HEIGHT: f32 = 36.0;
/// Room at the end of each row for its edit and close buttons and the column chooser.
const ACTIONS_WIDTH: f32 = 64.0;
/// Maximum entries retained in each completed account history.
pub(super) const HISTORY_LIMIT: usize = 1_000;

/// What the user asked for in the [`AccountPanel`] that the host carries out. Cancelling or
/// changing an order and closing a position go to the broker from the panel itself.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AccountPanelAction {
    /// Show this symbol on the chart.
    SelectSymbol(String),
    /// Connect to the account at this index of [`AccountPanel::accounts`].
    SelectAccount(usize),
    /// Disconnect from the broker.
    Disconnect,
    /// Open the account's settings, such as resetting a [paper
    /// account](crate::services::PaperAccount).
    Settings,
    /// Collapse the panel to its tab bar, or restore it.
    Minimize,
    /// Give the panel the full height, or restore it.
    Maximize,
    /// The table in view as comma-separated values, for the host to save as `name`.csv.
    Export {
        /// The table's name, such as "Positions".
        name: String,
        /// A header line, then one line per row.
        csv: String,
    },
}

/// The account panel of [`Trading`], shown with [`Trading::show_account_panel`].
///
/// A tab named after the broker sits above the account selectors and the account's summary:
/// balance, equity, realised and unrealised profit and loss, margin, available funds and the
/// margin of working orders, each shown when the broker reports it. Below are the positions,
/// working orders, finished orders, balance changes and trading journal the broker pushes, each
/// with a column chooser and an export. Order History also lists individual executions. A
/// position offers close, and an order offers edit and cancel on hover; each waits for the
/// user's confirmation like the chart's lines. Clicking
/// a symbol asks the host to show it. Each history keeps the most recent 1,000 entries in
/// arrival order; active orders are retained independently. Older history remains host-owned.
#[derive(Default)]
pub struct AccountPanel {
    /// The broker's name, shown on the tab and in the account selector.
    pub broker: String,
    /// The accounts the host offers at the broker, such as "Paper 1". The selector shows the
    /// [`active`](Self::active) one with the account's currency and switches between them.
    pub accounts: Vec<String>,
    /// The index in [`accounts`](Self::accounts) of the connected account.
    pub active: usize,
    tab: Tab,
    hidden: HashSet<(Tab, &'static str)>,
    history: Vec<Order>,
    pub(super) fills: super::dom::Fills,
    show_executions: bool,
    balance: Vec<BalanceChange>,
    journal: Vec<JournalEntry>,
    /// The order whose edit dialog is open, and its changed request.
    pub(super) editing: Option<(Order, OrderRequest)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Tab {
    #[default]
    Positions,
    Orders,
    History,
    Balance,
    Journal,
    Executions,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Positions,
        Tab::Orders,
        Tab::History,
        Tab::Balance,
        Tab::Journal,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Positions => "Positions",
            Tab::Orders => "Orders",
            Tab::History => "Order History",
            Tab::Balance => "Balance History",
            Tab::Journal => "Trading Journal",
            Tab::Executions => "Executions",
        }
    }

    fn columns(self) -> &'static [Column] {
        const fn left(title: &'static str, width: f32) -> Column {
            Column {
                title,
                right: false,
                width,
            }
        }
        const fn right(title: &'static str, width: f32) -> Column {
            Column {
                title,
                right: true,
                width,
            }
        }
        match self {
            Tab::Positions => {
                const COLUMNS: &[Column] = &[
                    left("Symbol", 110.0),
                    left("Side", 44.0),
                    right("Qty", 44.0),
                    right("Avg Fill Price", 84.0),
                    right("Last Price", 76.0),
                    right("Unrealized P&L", 96.0),
                    right("Unrealized P&L %", 96.0),
                    right("Trade Value", 96.0),
                    right("Market Value", 96.0),
                    right("Leverage", 56.0),
                    right("Margin", 80.0),
                    left("Expiration Date", 96.0),
                ];
                COLUMNS
            }
            Tab::Orders => {
                const COLUMNS: &[Column] = &[
                    left("Symbol", 110.0),
                    left("Side", 44.0),
                    left("Type", 60.0),
                    right("Qty", 44.0),
                    right("Limit Price", 80.0),
                    right("Stop Price", 80.0),
                    right("Take Profit", 80.0),
                    right("Stop Loss", 80.0),
                    left("Status", 76.0),
                    left("Placing Time", 130.0),
                    left("Order ID", 60.0),
                ];
                COLUMNS
            }
            Tab::History => {
                const COLUMNS: &[Column] = &[
                    left("Symbol", 110.0),
                    left("Side", 44.0),
                    left("Type", 60.0),
                    right("Qty", 44.0),
                    right("Limit Price", 80.0),
                    right("Stop Price", 80.0),
                    right("Fill Price", 80.0),
                    left("Status", 76.0),
                    left("Placing Time", 130.0),
                    left("Closing Time", 130.0),
                    left("Order ID", 60.0),
                ];
                COLUMNS
            }
            Tab::Balance => {
                const COLUMNS: &[Column] = &[
                    left("Time", 130.0),
                    right("Balance Before", 110.0),
                    right("Balance After", 110.0),
                    right("Realized P&L", 110.0),
                    left("Action", 300.0),
                ];
                COLUMNS
            }
            Tab::Executions => {
                const COLUMNS: &[Column] = &[
                    left("Symbol", 110.0),
                    left("Side", 44.0),
                    right("Qty", 44.0),
                    right("Fill Price", 80.0),
                    left("Time", 130.0),
                    left("Order ID", 60.0),
                ];
                COLUMNS
            }
            Tab::Journal => {
                const COLUMNS: &[Column] = &[left("Time", 130.0), left("Text", 500.0)];
                COLUMNS
            }
        }
    }
}

/// A table column: its title, whether its cells are right-aligned, and its smallest width.
struct Column {
    title: &'static str,
    right: bool,
    width: f32,
}

/// One table cell: its text and colour, a unit in small muted text after it, and whether it is a
/// symbol chip.
struct Cell {
    text: String,
    color: Color32,
    unit: String,
    symbol: bool,
}

/// A table row: its cells, the symbol a click on its chip shows, and what its edit and close
/// buttons do.
struct Row {
    cells: Vec<Cell>,
    symbol: Option<String>,
    edit: Option<RowAction>,
    close: Option<RowAction>,
}

enum RowAction {
    Position(Position),
    Order(Order),
}

/// `value` with thousands separators and `decimals` decimals, signed with `sign`.
pub(super) fn grouped(value: f64, decimals: usize, sign: bool) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    let magnitude = CurrencyFormatter::new("", decimals).format(value.abs());
    let shown = magnitude.bytes().any(|b| matches!(b, b'1'..=b'9'));
    match (shown, value < 0.0, sign) {
        (true, true, _) => format!("-{magnitude}"),
        (true, false, true) => format!("+{magnitude}"),
        _ => magnitude,
    }
}

/// `text` at `size` in the bundled face's semibold weight.
pub(super) fn semibold(text: &str, size: f32, color: Color32) -> egui::text::LayoutJob {
    let mut format = egui::TextFormat::simple(FontId::proportional(size), color);
    format.coords.push("wght", 600.0);
    egui::text::LayoutJob::single_section(text.to_owned(), format)
}

/// The rising colour for a gain, the falling colour for a loss, and the text colour when `text`
/// shows zero.
fn pnl_color(theme: &crate::theme::Theme, value: f64, text: &str) -> Color32 {
    if !text.bytes().any(|b| matches!(b, b'1'..=b'9')) {
        theme.ui.text
    } else if value > 0.0 {
        theme.chart.candle_up
    } else {
        theme.chart.candle_down
    }
}

fn time(time: Option<DateTime<Utc>>) -> String {
    time.map_or_else(String::new, |time| {
        time.format("%Y-%m-%d %H:%M:%S").to_string()
    })
}

fn kind(order_type: &OrderType) -> &'static str {
    match order_type {
        OrderType::Market => "Market",
        OrderType::Limit(_) => "Limit",
        OrderType::Stop(_) => "Stop",
        OrderType::StopLimit { .. } => "Stop Limit",
    }
}

fn status(status: &OrderStatus) -> String {
    match status {
        OrderStatus::Working => "Working".to_owned(),
        OrderStatus::Filled => "Filled".to_owned(),
        OrderStatus::Cancelled => "Cancelled".to_owned(),
        OrderStatus::Rejected(reason) => format!("Rejected: {reason}"),
    }
}

impl AccountPanel {
    /// Keeps the history the broker pushes: finished orders, balance changes and the journal.
    pub(super) fn record(&mut self, event: &AccountEvent) {
        if let AccountEvent::Order(order) = event
            && self
                .editing
                .as_ref()
                .is_some_and(|(old, _)| old.id == order.id && old != order)
        {
            self.editing = None;
        }
        if let AccountEvent::Account(account) = event
            && !account.capabilities.modify
        {
            self.editing = None;
        }
        if let AccountEvent::Order(order) = event {
            self.fills.order(order);
        }
        match event {
            AccountEvent::Order(order) if order.status != OrderStatus::Working => {
                self.history.retain(|finished| finished.id != order.id);
                self.history.push(order.clone());
                if self.history.len() > HISTORY_LIMIT {
                    let old = self.history.remove(0);
                    self.fills.forget(&old.id);
                }
            }
            AccountEvent::Execution(fill) => self.fills.execution(fill),
            AccountEvent::Balance(change) => self.balance.push(change.clone()),
            AccountEvent::Journal(entry) => self.journal.push(entry.clone()),
            _ => {}
        }
        if self.balance.len() > HISTORY_LIMIT {
            self.balance.remove(0);
        }
        if self.journal.len() > HISTORY_LIMIT {
            self.journal.remove(0);
        }
    }

    /// Forgets the history, as a new stream from the broker starts from nothing.
    pub(super) fn clear(&mut self) {
        self.history.clear();
        self.fills = super::dom::Fills::default();
        self.balance.clear();
        self.journal.clear();
        self.editing = None;
    }
}

impl Trading {
    /// Shows the account panel in `ui`, such as a bottom panel of the terminal, for a chart
    /// showing `chart_symbol`. Returns what the user asked of the host; cancelling, changing and
    /// closing go to the broker after the user confirms them in [`show`](Self::show).
    pub fn show_account_panel(
        &mut self,
        ui: &mut Ui,
        chart_symbol: &str,
    ) -> Option<AccountPanelAction> {
        let mut action = None;
        let theme = ui.active_theme();
        let currency = self
            .account
            .as_ref()
            .map_or(String::new(), |account| account.currency.clone());

        ui.painter().rect_filled(
            Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), TAB_HEIGHT)),
            0.0,
            theme.ui.btn_bg_hover,
        );
        // Tab strip: the broker's tab with a Trade button, then minimize and maximize.
        ui.horizontal(|ui| {
            ui.set_height(TAB_HEIGHT);
            let tab = egui::Frame::new()
                .fill(theme.ui.panel_bg)
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(10, 4));
            tab.show(ui, |ui| {
                ui.add(icons::CANDLESTICK.as_image_tinted(vec2(20.0, 20.0), theme.ui.text));
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), &self.account_panel.broker))
                        .color(theme.ui.text),
                );
                if ui
                    .add(egui::Button::new(l10n::text(ui.ctx(), "Trade")).corner_radius(4))
                    .on_hover_text(l10n::text(ui.ctx(), "Open the order ticket"))
                    .clicked()
                {
                    self.toggle_ticket(chart_symbol.to_owned());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if IconButton::new(&icons::MAXIMIZE)
                    .tooltip("Maximize panel")
                    .show(ui)
                    .clicked()
                {
                    action = Some(AccountPanelAction::Maximize);
                }
                if IconButton::new(&icons::MINUS)
                    .tooltip("Minimize panel")
                    .show(ui)
                    .clicked()
                {
                    action = Some(AccountPanelAction::Minimize);
                }
            });
        });

        if ui.available_height() < HEADER_HEIGHT {
            return action;
        }
        // Keep selectors and every summary figure reachable in narrow panels.
        egui::ScrollArea::horizontal()
            .id_salt("account_summary")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Account selectors, then the summary from the right edge as far as there is room.
                let (mut row, _) = ui
                    .allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());
                let mut selectors = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.with_min_x(row.left() + 8.0))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let panel = &mut self.account_panel;
                let chevron = || icons::CHEVRON_DOWN.as_image_square(16.0);
                let broker = selectors.add(
                    egui::Button::new((
                        semibold(&panel.broker, theme.text.body, theme.ui.text),
                        chevron(),
                    ))
                    .frame_when_inactive(false)
                    .image_tint_follows_text_color(true),
                );
                egui::Popup::menu(&broker).show(|ui| {
                    if ui.button(l10n::text(ui.ctx(), "Disconnect")).clicked() {
                        action = Some(AccountPanelAction::Disconnect);
                    }
                });
                let name = panel.accounts.get(panel.active).map_or("", String::as_str);
                let account = selectors.add(
                    egui::Button::new((format!("{name} {currency}").trim().to_owned(), chevron()))
                        .frame_when_inactive(false)
                        .image_tint_follows_text_color(true),
                );
                egui::Popup::menu(&account).show(|ui| {
                    for (index, name) in panel.accounts.iter().enumerate() {
                        if ui
                            .selectable_label(index == panel.active, l10n::text(ui.ctx(), name))
                            .clicked()
                        {
                            action = Some(AccountPanelAction::SelectAccount(index));
                        }
                    }
                });
                if IconButton::new(&icons::SETTINGS_NUT)
                    .tooltip("Account settings")
                    .show(&mut selectors)
                    .clicked()
                {
                    action = Some(AccountPanelAction::Settings);
                }
                if let Some(account) = &self.account {
                    let unrealized = account.equity.map(|equity| equity - account.balance);
                    let figures = [
                        ("Account balance", Some(account.balance), false, None),
                        ("Equity", account.equity, false, None),
                        ("Realized P&L", account.realized_pnl, true, None),
                        ("Unrealized P&L", unrealized, true, None),
                        (
                            "Account margin",
                            account.margin,
                            false,
                            Some("The margin the open positions hold"),
                        ),
                        (
                            "Available funds",
                            account.available_funds,
                            false,
                            Some("Equity less the margin of the positions and the working orders"),
                        ),
                        (
                            "Orders margin",
                            account.orders_margin,
                            false,
                            Some("The margin the working orders hold once filled"),
                        ),
                    ];
                    let painter = ui.painter();
                    let value_font = FontId::proportional(theme.text.body);
                    let figures: Vec<_> = figures
                        .into_iter()
                        .filter_map(|(label, value, signed, info)| {
                            let value = value?;
                            let text = grouped(value, 2, signed);
                            let color = if signed {
                                pnl_color(&theme, value, &text)
                            } else {
                                theme.ui.text
                            };
                            let label = painter.layout_job(semibold(
                                &l10n::text(ui.ctx(), label),
                                theme.text.small,
                                theme.ui.text,
                            ));
                            let value = painter.layout_no_wrap(
                                l10n::text(ui.ctx(), text),
                                value_font.clone(),
                                color,
                            );
                            let info_width = if info.is_some() { 18.0 } else { 0.0 };
                            let width = (label.size().x + info_width).max(value.size().x);
                            Some((label, value, color, info, width))
                        })
                        .collect();
                    let summary_width: f32 = figures.iter().map(|figure| figure.4 + 28.0).sum();
                    row.max.x = row
                        .right()
                        .max(selectors.min_rect().right() + 24.0 + summary_width);
                    ui.expand_to_include_rect(row);
                    let mut right = row.right() - 16.0;
                    for (label, value, color, info, width) in figures.into_iter().rev() {
                        let left = right - width;
                        let top = row.center().y - (label.size().y + value.size().y + 2.0) / 2.0;
                        if let Some(info) = info {
                            let icon = Rect::from_min_size(
                                egui::pos2(
                                    left + label.size().x + 4.0,
                                    top + label.size().y / 2.0 - 7.0,
                                ),
                                vec2(14.0, 14.0),
                            );
                            icons::INFO
                                .as_image_tinted(icon.size(), theme.ui.text_muted)
                                .paint_at(ui, icon);
                            ui.interact(icon, ui.scope_id().with(info), Sense::hover())
                                .on_hover_text(l10n::text(ui.ctx(), info));
                        }
                        let painter = ui.painter();
                        painter.galley(egui::pos2(left, top + label.size().y + 2.0), value, color);
                        painter.galley(egui::pos2(left, top), label, theme.ui.text);
                        right = left - 28.0;
                    }
                }
            });

        // Tabs, with the number of positions and working orders.
        egui::ScrollArea::horizontal()
            .id_salt("account_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.x = 20.0;
                    for tab in Tab::ALL {
                        let count = match tab {
                            Tab::Positions => self.positions.len(),
                            Tab::Orders => self.orders.len(),
                            _ => 0,
                        };
                        let mut title = RichText::new(l10n::text(ui.ctx(), tab.title()));
                        let active = self.account_panel.tab == tab;
                        title = title.color(if active {
                            theme.ui.text
                        } else {
                            theme.ui.text_secondary
                        });
                        let label = if count > 0 {
                            egui::Atoms::new((
                                title,
                                RichText::new(l10n::text(ui.ctx(), count.to_string()))
                                    .color(theme.ui.text_secondary),
                            ))
                        } else {
                            egui::Atoms::new(title)
                        };
                        let response = ui.add(egui::Button::new(label).frame(false));
                        if active {
                            let underline = Rect::from_min_max(
                                egui::pos2(response.rect.left(), response.rect.bottom() + 2.0),
                                egui::pos2(response.rect.right(), response.rect.bottom() + 5.0),
                            );
                            ui.painter().rect_filled(underline, 1.0, theme.ui.text);
                        }
                        if response.clicked() {
                            self.account_panel.tab = tab;
                        }
                    }
                });
            });
        if self.account_panel.tab == Tab::History {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.account_panel.show_executions,
                    false,
                    l10n::text(ui.ctx(), "Orders"),
                );
                ui.selectable_value(
                    &mut self.account_panel.show_executions,
                    true,
                    l10n::text(ui.ctx(), "Executions"),
                );
            });
        }
        let rule = ui.available_rect_before_wrap();
        ui.add_space(6.0);
        ui.painter().hline(
            rule.x_range().shrink(8.0),
            rule.top() + 4.0,
            Stroke::new(1.0, theme.ui.border),
        );

        let tab = if self.account_panel.tab == Tab::History && self.account_panel.show_executions {
            Tab::Executions
        } else {
            self.account_panel.tab
        };
        let capabilities = self.capabilities();
        let price = |symbol: &str, value: f64| {
            let decimals = if symbol == chart_symbol {
                self.decimals
            } else {
                2
            };
            grouped(value, decimals, false)
        };
        let money = |value: Option<f64>, signed: bool| {
            let Some(value) = value else {
                return (String::new(), theme.ui.text, String::new());
            };
            let text = grouped(value, 2, signed);
            let color = if signed {
                pnl_color(&theme, value, &text)
            } else {
                theme.ui.text
            };
            (text, color, currency.clone())
        };
        let plain = |text: String| Cell {
            text,
            color: theme.ui.text,
            unit: String::new(),
            symbol: false,
        };
        let symbol = |symbol: &str| Cell {
            text: symbol.to_owned(),
            color: theme.ui.text,
            unit: String::new(),
            symbol: true,
        };
        let side = |side: OrderSide, text: &str| Cell {
            text: text.to_owned(),
            color: super::ticket::side_color(&theme, side),
            unit: String::new(),
            symbol: false,
        };
        let colored = |(text, color, unit): (String, Color32, String)| Cell {
            text,
            color,
            unit,
            symbol: false,
        };
        let optional =
            |symbol: &str, value: Option<f64>| value.map_or(String::new(), |v| price(symbol, v));
        let order_cells = |order: &Order, finished: bool| {
            let request = &order.request;
            let (limit, stop) = match request.order_type {
                OrderType::Limit(limit) => (Some(limit), None),
                OrderType::Stop(stop) => (None, Some(stop)),
                OrderType::StopLimit { stop, limit } => (Some(limit), Some(stop)),
                _ => (None, None),
            };
            let mut cells = vec![
                symbol(&request.symbol),
                side(
                    request.side,
                    if request.side == OrderSide::Buy {
                        "Buy"
                    } else {
                        "Sell"
                    },
                ),
                plain(kind(&request.order_type).to_owned()),
                plain(grouped(
                    request.quantity,
                    quantity_decimals(request.quantity),
                    false,
                )),
                plain(optional(&request.symbol, limit)),
                plain(optional(&request.symbol, stop)),
            ];
            if finished {
                cells.push(plain(optional(&request.symbol, order.fill_price)));
            } else {
                cells.push(plain(optional(&request.symbol, request.take_profit)));
                cells.push(plain(optional(&request.symbol, request.stop_loss)));
            }
            cells.push(plain(status(&order.status)));
            cells.push(plain(time(order.placed)));
            if finished {
                cells.push(plain(time(order.closed)));
            }
            cells.push(plain(order.id.to_string()));
            cells
        };
        let rows: Vec<Row> = match tab {
            Tab::Positions => {
                self.positions
                    .iter()
                    .map(|position| {
                        let s = position.symbol.as_str();
                        let percent = position
                            .unrealized_pnl
                            .zip(position.trade_value)
                            .filter(|(_, value)| *value != 0.0)
                            .map(|(pnl, value)| pnl / value * 100.0);
                        let (long, label) = if position.quantity > 0.0 {
                            (OrderSide::Buy, "Long")
                        } else {
                            (OrderSide::Sell, "Short")
                        };
                        let size = position.quantity.abs();
                        Row {
                            cells: vec![
                                symbol(s),
                                side(long, label),
                                plain(grouped(size, quantity_decimals(size), false)),
                                plain(price(s, position.average_price)),
                                plain(optional(s, position.last_price)),
                                colored(money(position.unrealized_pnl, true)),
                                colored({
                                    let (text, color, _) = money(percent, true);
                                    let text = if text.is_empty() { text } else { text + "%" };
                                    (text, color, String::new())
                                }),
                                colored(money(position.trade_value, false)),
                                colored(money(position.market_value, false)),
                                plain(
                                    position
                                        .leverage
                                        .map_or(String::new(), |leverage| format!("{leverage}:1")),
                                ),
                                colored(money(position.margin, false)),
                                plain(position.expiration.map_or(String::new(), |date| {
                                    date.format("%Y-%m-%d").to_string()
                                })),
                            ],
                            symbol: Some(position.symbol.clone()),
                            edit: None,
                            close: capabilities
                                .market
                                .then(|| RowAction::Position(position.clone())),
                        }
                    })
                    .collect()
            }
            Tab::Orders => self
                .orders
                .iter()
                .map(|order| Row {
                    cells: order_cells(order, false),
                    symbol: Some(order.request.symbol.clone()),
                    edit: capabilities.modify.then(|| RowAction::Order(order.clone())),
                    close: Some(RowAction::Order(order.clone())),
                })
                .collect(),
            Tab::History => self
                .account_panel
                .history
                .iter()
                .rev()
                .map(|order| Row {
                    cells: order_cells(order, true),
                    symbol: Some(order.request.symbol.clone()),
                    edit: None,
                    close: None,
                })
                .collect(),
            Tab::Executions => self
                .account_panel
                .fills
                .executions
                .iter()
                .rev()
                .map(|fill| {
                    let order = self.account_panel.fills.orders.get(&fill.order);
                    let name = order.map(|order| order.symbol.as_str()).unwrap_or("");
                    Row {
                        cells: vec![
                            symbol(name),
                            side(fill.side, fill.side.label()),
                            plain(grouped(
                                fill.quantity,
                                quantity_decimals(fill.quantity),
                                false,
                            )),
                            plain(price(name, fill.price)),
                            plain(time(Some(fill.time))),
                            plain(fill.order.to_string()),
                        ],
                        symbol: order.map(|order| order.symbol.clone()),
                        edit: None,
                        close: None,
                    }
                })
                .collect(),
            Tab::Balance => self
                .account_panel
                .balance
                .iter()
                .rev()
                .map(|change| Row {
                    cells: vec![
                        plain(time(Some(change.time))),
                        plain(grouped(change.balance - change.amount, 2, false)),
                        plain(grouped(change.balance, 2, false)),
                        colored(money(Some(change.amount), true)),
                        plain(change.action.clone()),
                    ],
                    symbol: None,
                    edit: None,
                    close: None,
                })
                .collect(),
            Tab::Journal => self
                .account_panel
                .journal
                .iter()
                .rev()
                .map(|entry| Row {
                    cells: vec![plain(time(Some(entry.time))), plain(entry.text.clone())],
                    symbol: None,
                    edit: None,
                    close: None,
                })
                .collect(),
        };

        let columns: Vec<(usize, &Column)> = tab
            .columns()
            .iter()
            .enumerate()
            .filter(|(_, column)| !self.account_panel.hidden.contains(&(tab, column.title)))
            .collect();
        let font = FontId::proportional(theme.text.body);
        let heading = FontId::proportional(theme.text.small);
        let unit_font = FontId::proportional(theme.text.tiny);
        // Columns take their smallest width, at least their title's, and share what is left; a
        // narrow panel scrolls.
        let widths: Vec<f32> = columns
            .iter()
            .map(|(_, column)| {
                let title = ui.painter().layout_no_wrap(
                    l10n::text(ui.ctx(), column.title),
                    heading.clone(),
                    theme.ui.text,
                );
                column.width.max(title.size().x + 16.0)
            })
            .collect();
        let least: f32 = widths.iter().sum();
        let width = ui.available_width().max(least + ACTIONS_WIDTH + 16.0);
        let spare = (width - least - ACTIONS_WIDTH - 16.0) / columns.len().max(1) as f32;
        let edges = |rect: Rect| {
            let mut left = rect.left() + 8.0;
            widths
                .iter()
                .map(|width| {
                    let cell = Rect::from_min_max(
                        egui::pos2(left, rect.top()),
                        egui::pos2(left + width + spare - 12.0, rect.bottom()),
                    );
                    left += width + spare;
                    cell
                })
                .collect::<Vec<_>>()
        };

        let mut row_action = None;
        egui::ScrollArea::horizontal()
            .id_salt("account_panel_columns")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (header, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
                let chooser = Rect::from_min_size(
                    egui::pos2(
                        header.right().min(ui.clip_rect().right()) - 36.0,
                        header.top() + 4.0,
                    ),
                    vec2(24.0, 24.0),
                );
                let mut header_clip = ui.clip_rect();
                header_clip.max.x = chooser.left() - 4.0;
                for ((_, column), cell) in columns.iter().zip(edges(header)) {
                    let (x, align) = if column.right {
                        (cell.right(), Align2::RIGHT_CENTER)
                    } else {
                        (cell.left(), Align2::LEFT_CENTER)
                    };
                    ui.painter()
                        .with_clip_rect(cell.intersect(header_clip))
                        .text(
                            egui::pos2(x, cell.center().y),
                            align,
                            l10n::text(ui.ctx(), column.title),
                            heading.clone(),
                            theme.ui.text_secondary,
                        );
                }
                let chooser = ui.put(chooser, |ui: &mut Ui| {
                    IconButton::new(&icons::COLUMNS)
                        .size(ButtonSize::SM)
                        .tooltip("Columns and export")
                        .show(ui)
                });
                egui::Popup::menu(&chooser)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        for column in tab.columns() {
                            let key = (tab, column.title);
                            let mut shown = !self.account_panel.hidden.contains(&key);
                            if ui
                                .checkbox(&mut shown, l10n::text(ui.ctx(), column.title))
                                .changed()
                            {
                                if shown {
                                    self.account_panel.hidden.remove(&key);
                                } else {
                                    self.account_panel.hidden.insert(key);
                                }
                            }
                        }
                        ui.separator();
                        if ui.button(l10n::text(ui.ctx(), "Export as CSV")).clicked() {
                            action = Some(AccountPanelAction::Export {
                                name: tab.title().to_owned(),
                                csv: csv(&columns, &rows),
                            });
                            ui.close();
                        }
                    });

                if rows.is_empty() {
                    let mut empty =
                        ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            egui::pos2(ui.clip_rect().left(), ui.cursor().top()),
                            vec2(ui.clip_rect().width(), ui.available_height()),
                        )));
                    EmptyState::new(match tab {
                        Tab::Positions => "No open positions",
                        Tab::Orders => "No working orders",
                        Tab::History => "No finished orders",
                        Tab::Balance => "No balance changes",
                        Tab::Journal => "No journal entries",
                        Tab::Executions => "No executions",
                    })
                    .fill_height()
                    .show(&mut empty);
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("account_panel_rows", tab))
                    .auto_shrink([false, false])
                    .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
                        for row in &rows[range] {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                            let hovered = ui.rect_contains_pointer(rect);
                            if hovered {
                                ui.painter().rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
                            }
                            ui.painter().hline(
                                rect.x_range(),
                                rect.bottom() - 0.5,
                                Stroke::new(1.0, theme.ui.border),
                            );
                            for (&(index, column), cell_rect) in columns.iter().zip(edges(rect)) {
                                let cell = &row.cells[index];
                                if cell.symbol && !cell.text.is_empty() {
                                    let galley = ui.painter().layout_job(semibold(
                                        &cell.text,
                                        theme.text.small,
                                        theme.ui.text,
                                    ));
                                    let chip = Rect::from_min_size(
                                        egui::pos2(cell_rect.left(), rect.center().y - 12.0),
                                        vec2((galley.size().x + 16.0).min(cell_rect.width()), 24.0),
                                    );
                                    let current = cell.text == chart_symbol;
                                    let (fill, text) = if current {
                                        (theme.ui.accent, theme.ui.accent_text)
                                    } else {
                                        (theme.ui.panel_bg_secondary, theme.ui.text)
                                    };
                                    let response = ui
                                        .interact(
                                            chip,
                                            ui.scope_id().with((
                                                "chip",
                                                &cell.text,
                                                rect.top() as i32,
                                            )),
                                            Sense::click(),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    ui.painter().rect_filled(chip, 4.0, fill);
                                    let galley = ui.painter().layout_job(semibold(
                                        &cell.text,
                                        theme.text.small,
                                        text,
                                    ));
                                    ui.painter().with_clip_rect(chip.shrink(4.0)).galley(
                                        chip.left_center() + vec2(8.0, -galley.size().y / 2.0),
                                        galley,
                                        text,
                                    );
                                    if response.clicked() {
                                        action = row
                                            .symbol
                                            .clone()
                                            .map(AccountPanelAction::SelectSymbol);
                                    }
                                    continue;
                                }
                                let painter = ui.painter().with_clip_rect(cell_rect);
                                let y = rect.center().y;
                                let mut right = cell_rect.right();
                                if !cell.unit.is_empty() && !cell.text.is_empty() {
                                    let unit = painter.text(
                                        egui::pos2(right, y + 1.0),
                                        Align2::RIGHT_CENTER,
                                        l10n::text(ui.ctx(), &cell.unit),
                                        unit_font.clone(),
                                        theme.ui.text_muted,
                                    );
                                    right = unit.left() - 3.0;
                                }
                                let (x, align) = if column.right {
                                    (right, Align2::RIGHT_CENTER)
                                } else {
                                    (cell_rect.left(), Align2::LEFT_CENTER)
                                };
                                painter.text(
                                    egui::pos2(x, y),
                                    align,
                                    l10n::text(ui.ctx(), &cell.text),
                                    font.clone(),
                                    cell.color,
                                );
                            }
                            if hovered && (row.edit.is_some() || row.close.is_some()) {
                                let visible = rect.right().min(ui.clip_rect().right());
                                let buttons = Rect::from_min_max(
                                    egui::pos2(visible - ACTIONS_WIDTH, rect.top()),
                                    egui::pos2(visible - 12.0, rect.bottom()),
                                );
                                let mut child = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(buttons)
                                        .layout(Layout::right_to_left(Align::Center)),
                                );
                                let (close, edit) = match row.close.as_ref().or(row.edit.as_ref()) {
                                    Some(RowAction::Position(_)) => ("Close position", ""),
                                    _ => ("Cancel order", "Modify order"),
                                };
                                if row.close.is_some()
                                    && IconButton::new(&icons::CLOSE)
                                        .size(ButtonSize::SM)
                                        .tooltip(close)
                                        .show(&mut child)
                                        .clicked()
                                {
                                    row_action = row.close.as_ref().map(|a| (a, false));
                                }
                                if row.edit.is_some()
                                    && IconButton::new(&icons::PENCIL)
                                        .size(ButtonSize::SM)
                                        .tooltip(edit)
                                        .show(&mut child)
                                        .clicked()
                                {
                                    row_action = row.edit.as_ref().map(|a| (a, true));
                                }
                            }
                        }
                    });
            });

        match row_action {
            Some((RowAction::Position(position), _)) => {
                self.pending = Some(TradingLineAction::Close(position.clone()));
            }
            Some((RowAction::Order(order), false)) => {
                self.pending = Some(TradingLineAction::Cancel(order.clone()));
            }
            Some((RowAction::Order(order), true)) => {
                self.ticket.open = false;
                self.load_rules(&order.request.symbol);
                self.account_panel.editing = Some((order.clone(), order.request.clone()));
            }
            None => {}
        }
        self.show_order_editor(ui.ctx());
        action
    }

    /// The edit dialog of a working order: its quantity, prices and brackets. Saving sends the
    /// change to the broker; the dialog is the user's confirmation.
    fn show_order_editor(&mut self, ctx: &egui::Context) {
        let decimals = self.decimals;
        let brackets = self.capabilities().brackets;
        let Some((order, request)) = &mut self.account_panel.editing else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new(l10n::format_text(
            ctx,
            "Modify order {0}",
            &[("0", order.id.to_string())],
        ))
        .id(egui::Id::unique("account_panel_modify"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            egui::Grid::new("account_panel_modify_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    let price = |ui: &mut Ui, label: &str, value: &mut f64| {
                        ui.label(l10n::text(ui.ctx(), label));
                        ui.add(
                            egui::DragValue::new(value)
                                .range(0.0..=f64::MAX)
                                .max_decimals(decimals)
                                .speed(10f64.powi(-(decimals as i32))),
                        );
                        ui.end_row();
                    };
                    ui.label(l10n::text(ui.ctx(), "Quantity"));
                    ui.add(egui::DragValue::new(&mut request.quantity).range(0.0..=f64::MAX));
                    ui.end_row();
                    match &mut request.order_type {
                        OrderType::Limit(limit) => price(ui, "Limit price", limit),
                        OrderType::Stop(stop) => price(ui, "Stop price", stop),
                        OrderType::StopLimit { stop, limit } => {
                            price(ui, "Stop price", stop);
                            price(ui, "Limit price", limit);
                        }
                        _ => {}
                    }
                    if brackets {
                        let reference = match request.order_type {
                            OrderType::Limit(p) | OrderType::Stop(p) => p,
                            OrderType::StopLimit { limit, .. } => limit,
                            _ => 0.0,
                        };
                        for (label, bracket) in [
                            ("Take profit", &mut request.take_profit),
                            ("Stop loss", &mut request.stop_loss),
                        ] {
                            let mut on = bracket.is_some();
                            ui.checkbox(&mut on, label);
                            if on {
                                let value = bracket.get_or_insert(reference);
                                ui.add(
                                    egui::DragValue::new(value)
                                        .range(0.0..=f64::MAX)
                                        .max_decimals(decimals)
                                        .speed(10f64.powi(-(decimals as i32))),
                                );
                            } else {
                                *bracket = None;
                            }
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.0);
            let valid = self.rules_ready
                && self.rules.symbol == request.symbol
                && super::ticket::order_quantity(request.quantity, &self.rules)
                    .is_ok_and(|quantity| quantity == request.quantity)
                && request != &order.request;
            save = ui
                .add_enabled(
                    valid,
                    egui::Button::new(l10n::text(ui.ctx(), "Modify order")),
                )
                .clicked();
        });
        if save {
            let (order, request) = self.account_panel.editing.take().expect("editing");
            self.execute(Some(TradingLineAction::Modify(order, request)));
        } else if !open {
            self.account_panel.editing = None;
        }
    }
}

/// Decimals that show `quantity` without rounding, up to 8.
fn quantity_decimals(quantity: f64) -> usize {
    (0..8)
        .find(|&decimals| {
            let scaled = quantity * 10f64.powi(decimals as i32);
            (scaled - scaled.round()).abs() < 1e-6
        })
        .unwrap_or(8)
}

/// The rows in view as comma-separated values, with the shown columns' titles as the header.
/// Cells holding a comma or a quote are quoted.
fn csv(columns: &[(usize, &Column)], rows: &[Row]) -> String {
    let field = |text: &str| {
        if text.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text.to_owned()
        }
    };
    let mut out = columns
        .iter()
        .map(|(_, column)| field(column.title))
        .collect::<Vec<_>>()
        .join(",");
    for row in rows {
        out.push('\n');
        out += &columns
            .iter()
            .map(|&(index, _)| {
                let cell = &row.cells[index];
                let text = if cell.unit.is_empty() {
                    cell.text.clone()
                } else {
                    format!("{} {}", cell.text, cell.unit)
                };
                field(&text)
            })
            .collect::<Vec<_>>()
            .join(",");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Execution;

    #[test]
    fn history_keeps_each_partial_fill_and_discards_stale_edits() {
        let mut panel = AccountPanel::default();
        let order = Order::new(
            crate::model::OrderId::new("7"),
            OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(100.0)),
            OrderStatus::Working,
        );
        let fill = Execution::new(order.id.clone(), Utc::now(), OrderSide::Buy, 99.0, 1.0);
        panel.record(&AccountEvent::Execution(fill.clone()));
        panel.record(&AccountEvent::Execution(fill.clone()));
        assert_eq!(panel.fills.executions, vec![fill.clone(), fill]);
        for event in [
            AccountEvent::Order({
                let mut partial = order.clone();
                partial.filled = 1.0;
                partial
            }),
            AccountEvent::Account(crate::model::Account::new(
                "USD",
                1000.0,
                Default::default(),
            )),
        ] {
            panel.editing = Some((order.clone(), order.request.clone()));
            panel.record(&event);
            assert!(panel.editing.is_none());
        }
        panel.clear();
        assert!(panel.fills.executions.is_empty());
    }

    #[test]
    fn history_retention_preserves_working_orders_and_latest_fills() {
        let mut panel = AccountPanel::default();
        let mut order = Order::new(
            crate::model::OrderId::new("active"),
            OrderRequest::new("OLD", OrderSide::Buy, 5_000.0, OrderType::Market),
            OrderStatus::Working,
        );
        let time = DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        panel.record(&AccountEvent::Order(order.clone()));
        panel.record(&AccountEvent::Execution(Execution::new(
            order.id.clone(),
            time,
            OrderSide::Buy,
            99.0,
            1.0,
        )));
        order.status = OrderStatus::Filled;
        for i in 0..2_005 {
            order.id = crate::model::OrderId::new(i.to_string());
            order.request.symbol = format!("NEW-{i}");
            let time = time + chrono::Duration::seconds(i);
            panel.record(&AccountEvent::Order(order.clone()));
            panel.record(&AccountEvent::Execution(Execution::new(
                order.id.clone(),
                time,
                OrderSide::Buy,
                100.0,
                1.0,
            )));
            panel.record(&AccountEvent::Balance(BalanceChange::new(
                time,
                1000.0,
                1.0,
                i.to_string(),
            )));
            panel.record(&AccountEvent::Journal(JournalEntry::new(
                time,
                i.to_string(),
            )));
        }
        assert_eq!(panel.history.len(), 1_000);
        assert_eq!(panel.balance.len(), 1_000);
        assert_eq!(panel.journal.len(), 1_000);
        assert_eq!(panel.fills.executions.len(), 1_000);
        assert_eq!(panel.history[0].id.0, "1005");
        assert_eq!(panel.fills.executions[0].order.0, "1005");
        assert_eq!(panel.fills.orders.len(), 1_001);
        assert!(
            panel
                .fills
                .orders
                .contains_key(&crate::model::OrderId::new("active"))
        );
        assert_eq!(panel.fills.price("OLD"), Some(99.0));
        assert_eq!(panel.fills.price("NEW-0"), Some(100.0));
        assert_eq!(panel.fills.price("NEW-2004"), Some(100.0));
        // Orders without fills must not accumulate metadata either.
        order.status = OrderStatus::Cancelled;
        for i in 2_005..4_010 {
            order.id = crate::model::OrderId::new(i.to_string());
            panel.record(&AccountEvent::Order(order.clone()));
        }
        assert!(panel.fills.orders.len() <= 2_001);
        let active = crate::model::OrderId::new("active");
        for _ in 0..1_005 {
            panel.record(&AccountEvent::Execution(Execution::new(
                active.clone(),
                time,
                OrderSide::Buy,
                99.0,
                1.0,
            )));
        }
        assert_eq!(panel.fills.executions.len(), 1_000);
        assert_eq!(panel.fills.filled_quantity(&active), 1_006.0);
        panel.clear();
        assert!(panel.fills.orders.is_empty());
        assert_eq!(panel.fills.price("OLD"), None);
    }

    #[test]
    fn csv_round_trips_quoted_journal_text_and_visible_columns() {
        let columns: Vec<_> = Tab::Journal.columns().iter().enumerate().collect();
        let rows = vec![Row {
            cells: ["time", "line\rbreak"]
                .into_iter()
                .map(|text| Cell {
                    text: text.into(),
                    color: Color32::WHITE,
                    unit: String::new(),
                    symbol: false,
                })
                .collect(),
            symbol: None,
            edit: None,
            close: None,
        }];
        for shown in [&columns[..], &columns[1..]] {
            let exported = csv(shown, &rows);
            let mut reader = ::csv::Reader::from_reader(exported.as_bytes());
            assert_eq!(reader.headers().unwrap().len(), shown.len());
            assert_eq!(
                reader.records().next().unwrap().unwrap().iter().next_back(),
                Some("line\rbreak")
            );
        }
    }

    #[test]
    fn figures_group_thousands_and_sign_only_what_they_show() {
        for (value, sign, text) in [
            (-123_456.0, false, "-123,456.00"),
            (-24_932.084, true, "-24,932.08"),
            (100_050.68, true, "+100,050.68"),
            (1_234_567.891, false, "1,234,567.89"),
            (-0.004, true, "0.00"),
            (0.0, true, "0.00"),
            (f64::NAN, false, "-"),
        ] {
            assert_eq!(grouped(value, 2, sign), text, "{value}");
        }
    }
}
