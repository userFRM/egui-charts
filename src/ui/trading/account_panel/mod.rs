//! The account panel: the connected account's summary, positions, working orders, order history,
//! closed positions, balance history and trading journal, shown at the bottom of the terminal.

mod history;
mod view;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use egui::Color32;

use history::Paging;

pub(super) use crate::chart::quantity_decimals;

use super::{Trading, changed};
use crate::model::{
    BalanceChange, ClosedPosition, Execution, JournalEntry, Order, OrderId, OrderRequest,
    OrderStatus, OrderType, Position,
};
use crate::services::{AccountEvent, HistoryKind};

const TAB_HEIGHT: f32 = 38.0;
const HEADER_HEIGHT: f32 = 52.0;
const ROW_HEIGHT: f32 = 48.0;
/// Room at the end of each row for its edit and close buttons and the column chooser.
const ACTIONS_WIDTH: f32 = 64.0;
/// Maximum entries retained in each completed account history.
pub(super) const HISTORY_LIMIT: usize = 1_000;

/// The account's orders and executions, shared by the ladder, chart and account history.
#[derive(Debug, Default)]
pub(super) struct Fills {
    pub(super) orders: HashMap<OrderId, OrderRequest>,
    pub(super) executions: Vec<Execution>,
    /// How many executions at the front came from pages that the stream has not repeated.
    paged: usize,
    working: HashSet<OrderId>,
    filled: HashMap<OrderId, f64>,
    /// Latest fill for each symbol, independent of execution-history eviction.
    latest: HashMap<String, Execution>,
}

impl Fills {
    pub(super) fn order(&mut self, order: &Order) {
        self.orders.insert(order.id.clone(), order.request.clone());
        if order.status == OrderStatus::Working {
            self.working.insert(order.id.clone());
        } else {
            self.working.remove(&order.id);
        }
        if let Some(fill) = self
            .executions
            .iter()
            .filter(|fill| fill.order == order.id)
            .max_by_key(|fill| fill.time)
            .cloned()
        {
            self.remember(&order.request.symbol, &fill);
        }
    }

    pub(super) fn execution(&mut self, fill: &Execution) {
        let (repeated, dropped) = push_live(&mut self.executions, &mut self.paged, fill);
        // A fill that a page brought first was counted then.
        if !repeated {
            *self.filled.entry(fill.order.clone()).or_default() += fill.quantity;
        }
        if let Some(symbol) = self.symbol(fill).map(str::to_owned) {
            self.remember(&symbol, fill);
        }
        if let Some(old) = dropped {
            self.forget(&old.order);
        }
    }

    /// The order's symbol when known, otherwise the fill's own.
    pub(super) fn symbol<'a>(&'a self, fill: &'a Execution) -> Option<&'a str> {
        self.orders
            .get(&fill.order)
            .map(|order| order.symbol.as_str())
            .or(fill.symbol.as_deref())
    }

    pub(super) fn forget(&mut self, id: &OrderId) {
        if !self.working.contains(id) && !self.executions.iter().any(|fill| &fill.order == id) {
            self.orders.remove(id);
            self.filled.remove(id);
        }
    }

    fn remember(&mut self, symbol: &str, fill: &Execution) {
        if self
            .latest
            .get(symbol)
            .is_none_or(|old| old.time <= fill.time)
        {
            self.latest.insert(symbol.to_owned(), fill.clone());
        }
    }

    pub(super) fn price(&self, symbol: &str) -> Option<f64> {
        self.latest.get(symbol).map(|fill| fill.price)
    }

    pub(super) fn filled_quantity(&self, id: &OrderId) -> f64 {
        self.filled.get(id).copied().unwrap_or_default()
    }
}

/// Appends `entry` from the stream to `list`, whose first `paged` entries came from pages, in
/// place of the equal page entry when a page brought it first, then drops the oldest entry
/// beyond [`HISTORY_LIMIT`]. Returns whether `entry` repeated a page entry, and the entry
/// dropped.
fn push_live<T: PartialEq + Clone>(
    list: &mut Vec<T>,
    paged: &mut usize,
    entry: &T,
) -> (bool, Option<T>) {
    let repeated = list[..*paged].iter().position(|listed| listed == entry);
    if let Some(at) = repeated {
        list.remove(at);
        *paged -= 1;
    }
    list.push(entry.clone());
    let dropped = if list.len() > HISTORY_LIMIT {
        *paged = paged.saturating_sub(1);
        Some(list.remove(0))
    } else {
        None
    };
    (repeated.is_some(), dropped)
}

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
    /// Open the host's broker picker, such as a
    /// [`BrokerPicker`](crate::ui::trading::BrokerPicker), to choose another broker. The current
    /// broker stays connected until the user picks another.
    ChangeBroker,
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
/// balance, equity, realised and unrealised profit and loss, margin, margin buffer, available
/// funds and the margin of working orders, each shown when the broker reports it. The tab marks
/// a broker that reports that it is connecting or disconnected, until it reports that it is live.
/// Below are the positions, working orders, finished orders, closed positions, balance changes
/// and trading journal the broker pushes, each with a column chooser and an export. Order History
/// also lists individual executions. A position offers close, and an order offers edit and cancel
/// on hover; each waits for the user's confirmation like the chart's lines. The broker menu asks
/// the host to open its broker picker or disconnect. Clicking a symbol asks the host to show it.
/// Each history keeps the most recent 1,000 entries; active orders are retained independently.
/// Scrolling to the end of a history asks
/// [`Broker::history`](crate::services::Broker::history) for the entries before its oldest
/// one, a page at a time, until the broker has nothing older or the history lists 1,000
/// entries. Each entry is listed once, whether a page or the broker's stream brings it first,
/// and pages change no working order or position.
#[derive(Debug, Default)]
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
    pub(super) fills: Fills,
    show_executions: bool,
    balance: Vec<BalanceChange>,
    /// How many balance changes at the front came from pages that the stream has not repeated.
    balance_paged: usize,
    journal: Vec<JournalEntry>,
    /// The same for `journal`.
    journal_paged: usize,
    /// Closed positions, oldest first.
    closed: Vec<ClosedPosition>,
    /// How many closed positions at the front came from pages that the stream has not repeated.
    closed_paged: usize,
    /// Paging through the broker's older history, by kind, since the stream started.
    paging: HashMap<HistoryKind, Paging>,
    /// The order whose edit dialog is open, and its changed request.
    pub(super) editing: Option<(Order, OrderRequest)>,
    /// The pass that last drew the edit dialog, which the chart and the account panel share.
    pub(super) editor_pass: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Tab {
    #[default]
    Positions,
    Orders,
    History,
    Closed,
    Balance,
    Journal,
    Executions,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Positions,
        Tab::Orders,
        Tab::History,
        Tab::Closed,
        Tab::Balance,
        Tab::Journal,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Positions => "Positions",
            Tab::Orders => "Orders",
            Tab::History => "Order History",
            Tab::Closed => "Closed Positions",
            Tab::Balance => "Balance History",
            Tab::Journal => "Trading Journal",
            Tab::Executions => "Executions",
        }
    }

    /// The broker history this tab lists and pages through.
    fn history(self) -> Option<HistoryKind> {
        match self {
            Tab::History => Some(HistoryKind::Orders),
            Tab::Executions => Some(HistoryKind::Executions),
            Tab::Closed => Some(HistoryKind::ClosedPositions),
            Tab::Balance => Some(HistoryKind::Balance),
            Tab::Journal => Some(HistoryKind::Journal),
            Tab::Positions | Tab::Orders => None,
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
                    left("Symbol", 134.0),
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
                    left("Symbol", 134.0),
                    left("Side", 44.0),
                    left("Type", 60.0),
                    right("Qty", 44.0),
                    right("Limit Price", 80.0),
                    right("Stop Price", 80.0),
                    right("Take Profit", 80.0),
                    right("Stop Loss", 80.0),
                    left("Status", 76.0),
                    left("Placing Time", 130.0),
                    left("Expiry", 130.0),
                    left("Order ID", 60.0),
                ];
                COLUMNS
            }
            Tab::History => {
                const COLUMNS: &[Column] = &[
                    left("Symbol", 134.0),
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
            Tab::Closed => {
                const COLUMNS: &[Column] = &[
                    left("Symbol", 134.0),
                    left("Side", 44.0),
                    right("Qty", 44.0),
                    right("Avg Entry Price", 96.0),
                    right("Avg Exit Price", 96.0),
                    right("Realized P&L", 110.0),
                    left("Opening Time", 130.0),
                    left("Closing Time", 130.0),
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
                    left("Symbol", 134.0),
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
#[derive(Debug)]
struct Cell {
    text: String,
    color: Color32,
    unit: String,
    symbol: bool,
    /// The logo of a symbol chip's instrument, when the host supplied one.
    logo: Option<String>,
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
        OrderType::TrailingStop { .. } => "Trailing Stop",
    }
}

fn status(ctx: &egui::Context, status: &OrderStatus) -> String {
    match status {
        OrderStatus::Working => "Working".to_owned(),
        OrderStatus::Filled => "Filled".to_owned(),
        OrderStatus::Cancelled => "Cancelled".to_owned(),
        OrderStatus::Rejected(reason) => crate::localization::format_text(
            ctx,
            "Rejected: {reason}",
            &[("reason", reason.as_str())],
        ),
    }
}

impl AccountPanel {
    /// Keeps the broker's finished orders, closed positions, balance changes and journal.
    pub(super) fn record(&mut self, event: &AccountEvent) {
        if let AccountEvent::Order(order) = event
            && self
                .editing
                .as_ref()
                .is_some_and(|(old, _)| old.id == order.id && changed(old, order))
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
            AccountEvent::ClosedPosition(closed) => {
                push_live(&mut self.closed, &mut self.closed_paged, closed);
            }
            AccountEvent::Balance(change) => {
                push_live(&mut self.balance, &mut self.balance_paged, change);
            }
            AccountEvent::Journal(entry) => {
                push_live(&mut self.journal, &mut self.journal_paged, entry);
            }
            _ => {}
        }
    }

    /// Forgets the history, as a new stream from the broker starts from nothing.
    pub(super) fn clear(&mut self) {
        self.history.clear();
        self.fills = Fills::default();
        self.balance.clear();
        self.balance_paged = 0;
        self.journal.clear();
        self.journal_paged = 0;
        self.closed.clear();
        self.closed_paged = 0;
        self.paging.clear();
        self.editing = None;
    }
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
    use std::sync::Arc;

    use egui_kittest::{Harness, kittest::Queryable};

    use super::*;
    use crate::model::OrderSide;
    use crate::ui::trading::tests::Recorder;
    use crate::ui_kit::notifications::Toasts;

    /// A reported buffer, including a deficit, is shown beside the account margin; an absent
    /// buffer has neither a label nor a value. The summary paints text rather than labels.
    #[test]
    fn the_summary_shows_the_margin_buffer_only_when_reported() {
        use crate::model::{Account, Capabilities};
        use std::sync::Mutex;

        for (buffer, shown) in [(None, false), (Some(-250.5), true)] {
            let mut trading = Trading::default();
            let mut account = Account::new("USD", 1_000.0, Capabilities::default());
            account.margin = Some(200.0);
            account.margin_buffer = buffer;
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Account(account),
                &mut Toasts::new(),
            );
            let keys = Arc::new(Mutex::new(Vec::new()));
            let translated = keys.clone();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1440.0, 400.0))
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::localization::set_translator(ctx, move |english| {
                        translated.lock().unwrap().push(english.to_owned());
                        None
                    });
                })
                .build_ui_state(
                    |ui, trading: &mut Trading| {
                        trading.show_account_panel(ui, "SYM");
                    },
                    trading,
                );
            harness.run();
            let keys = keys.lock().unwrap();
            for key in ["Account margin", "200.00"] {
                assert!(keys.iter().any(|text| text == key));
            }
            for key in ["Margin buffer", "-250.50"] {
                assert_eq!(
                    keys.iter().any(|text| text == key),
                    shown,
                    "{buffer:?}: {key}"
                );
            }
        }
    }

    /// Menu entries ask the host for the chosen action while the current service stays live.
    #[test]
    fn the_broker_menu_asks_the_host_to_change_broker_or_disconnect() {
        for (item, expected) in [
            ("Change broker", AccountPanelAction::ChangeBroker),
            ("Disconnect", AccountPanelAction::Disconnect),
        ] {
            let mut trading = Trading::default();
            trading.set_service(Arc::new(Recorder::default()));
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1000.0, 400.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, state: &mut (Trading, Option<AccountPanelAction>)| {
                        if let Some(action) = state.0.show_account_panel(ui, "SYM") {
                            state.1 = Some(action);
                        }
                    },
                    (trading, None),
                );
            harness.run();
            harness
                .get_by_role_and_label(egui::Role::Button, "Broker")
                .click();
            harness.run();
            harness.get_by_label(item).click();
            harness.run();
            assert_eq!(harness.state().1, Some(expected));
            assert!(harness.state().0.has_broker());
            assert!(harness.state().0.is_connected());
        }
    }

    /// Completed positions keep their column order, numeric units and missing opening time in
    /// the export, with the newest position first.
    #[test]
    fn closed_positions_are_listed_newest_first_and_exported_by_column() {
        let ctx = egui::Context::default();
        let at = |seconds: i64| DateTime::from_timestamp(1_704_067_200 + seconds, 0).unwrap();
        let mut trading = Trading::default();
        trading.set_service(Arc::new(Recorder::default()));
        trading.poll(&ctx, &mut Toasts::new());
        let mut long = ClosedPosition::new("SYM", OrderSide::Buy, 3.0, 110.0, 125.0, 45.0, at(20));
        long.opened = Some(at(0));
        let short =
            ClosedPosition::new("SYM", OrderSide::Sell, 0.5, 1_250.0, 1_311.0, -30.5, at(40));
        for closed in [long, short] {
            trading.apply(
                &ctx,
                AccountEvent::ClosedPosition(closed),
                &mut Toasts::new(),
            );
        }
        trading.account_panel.tab = Tab::Closed;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1440.0, 400.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, state: &mut (Trading, Option<AccountPanelAction>)| {
                    if let Some(action) = state.0.show_account_panel(ui, "SYM") {
                        state.1 = Some(action);
                    }
                },
                (trading, None),
            );
        harness.run();
        harness.get_by_label("Columns and export").click();
        harness.run();
        harness.get_by_label("Export as CSV").click();
        harness.run();
        assert_eq!(
            harness.state().1,
            Some(AccountPanelAction::Export {
                name: "Closed Positions".into(),
                csv: "Symbol,Side,Qty,Avg Entry Price,Avg Exit Price,Realized P&L,Opening Time,Closing Time\n\
SYM,Short,0.5,\"1,250.00\",\"1,311.00\",-30.50 USD,,2024-01-01 00:00:40\n\
SYM,Long,3,110.00,125.00,+45.00 USD,2024-01-01 00:00:00,2024-01-01 00:00:20".into(),
            })
        );
    }

    /// A position's full quantity and close button stay visible without hovering.
    #[test]
    fn position_quantities_and_row_actions_show_at_rest() {
        for (quantity, expected) in [
            (1.0, "1"),
            (1e-8, "0.00000001"),
            (1.0000001, "1.0000001"),
            (1_000_000.0, "1,000,000"),
        ] {
            let broker = Arc::new(Recorder {
                state: vec![AccountEvent::Position(Position::new(
                    "SYM", quantity, 100.0,
                ))],
                ..Default::default()
            });
            let mut trading = Trading::default();
            trading.set_service(broker);
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            trading.account_panel.broker = "Broker".into();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1000.0, 400.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, trading: &mut Trading| {
                        trading.show_account_panel(ui, "SYM");
                    },
                    trading,
                );
            harness.run();
            harness.get_by_label("Close position");
            let (clip, bounds) = harness
                .output()
                .shapes
                .iter()
                .rev()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == expected => {
                        Some((shape.clip_rect, text.visual_bounding_rect()))
                    }
                    _ => None,
                })
                .expect("the position quantity");
            // Text and clip edges round independently to physical pixels.
            assert!(
                clip.expand(0.5).contains_rect(bounds),
                "{expected}: {bounds:?} outside {clip:?}"
            );
        }
    }

    /// Each number field of the Modify order dialog is named after its label or check box, with
    /// the accessibility check on, for an order with one take profit and for one with
    /// take-profit levels, every label translated once. A regression leaves a field unnamed,
    /// names it after another label, or skips or repeats a translation.
    #[test]
    fn the_modify_order_dialog_names_its_fields() {
        use crate::model::TakeProfitLevel;
        type Row = (
            OrderType,
            Vec<TakeProfitLevel>,
            Option<f64>,
            &'static [&'static str],
        );
        let mut stopped = TakeProfitLevel::new(110.0, 1.0);
        stopped.stop_loss = Some(85.0);
        let rows: [Row; 2] = [
            (
                OrderType::Limit(100.0),
                vec![],
                Some(110.0),
                &[
                    "[Quantity]",
                    "[Limit price]",
                    "[Take profit]",
                    "[Stop loss]",
                ],
            ),
            (
                OrderType::StopLimit {
                    stop: 101.0,
                    limit: 100.0,
                },
                vec![stopped, TakeProfitLevel::new(120.0, 1.0)],
                None,
                &[
                    "[Quantity]",
                    "[Stop price]",
                    "[Limit price]",
                    "[Take profit 1]",
                    "[Take profit 1 quantity]",
                    "[Stop loss 1]",
                    "[Take profit 2]",
                    "[Take profit 2 quantity]",
                    "[Stop loss]",
                ],
            ),
        ];
        for (order_type, levels, take_profit, names) in rows {
            let mut request = OrderRequest::new("SYM", OrderSide::Buy, 2.0, order_type);
            request.take_profit = take_profit;
            request.take_profit_levels = levels;
            request.stop_loss = Some(90.0);
            let order = Order::new(OrderId::new("9"), request, OrderStatus::Working);
            let mut trading = Trading::default();
            trading.set_service(Arc::new(Recorder {
                brackets: true,
                ..Recorder::default()
            }));
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            trading.account_panel.editing = Some((order.clone(), order.request));
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1000.0, 600.0))
                .with_context_setup(|ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::localization::set_translator(ctx, |english| {
                        Some(format!("[{english}]"))
                    });
                })
                .build_ui_state(
                    |ui, trading: &mut Trading| {
                        trading.show_account_panel(ui, "SYM");
                    },
                    trading,
                );
            harness.run();
            for name in names {
                harness.get_by_role_and_label(egui::Role::SpinButton, name);
            }
        }
    }

    /// Editing a level rounds to the quantity step, updates the total and reaches the broker.
    /// Below-minimum levels and above-maximum totals explain why saving is disabled.
    #[test]
    fn the_modify_order_dialog_edits_level_quantities_and_says_why_it_refuses() {
        use crate::model::{TakeProfitLevel, TradingRules};
        let mut request = OrderRequest::new(
            "SYM",
            OrderSide::Buy,
            4000.0,
            OrderType::StopLimit {
                stop: 101.0,
                limit: 100.0,
            },
        );
        request.take_profit_levels = vec![
            TakeProfitLevel::new(110.0, 2000.0),
            TakeProfitLevel::new(120.0, 2000.0),
        ];
        request.stop_loss = Some(90.0);
        let order = Order::new(OrderId::new("9"), request, OrderStatus::Working);
        let broker = Arc::new(Recorder {
            brackets: true,
            ..Recorder::default()
        });
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        let ctx = egui::Context::default();
        trading.poll(&ctx, &mut Toasts::new());
        trading.load_rules("SYM");
        // Steps and limits whose grouped text differs from `f64::to_string`.
        let mut rules = TradingRules::new("SYM");
        rules.quantity_step = 1000.0;
        rules.min_quantity = 1000.0;
        rules.max_quantity = Some(20000.0);
        for (_, reply) in std::mem::take(&mut *broker.rules.lock().unwrap()) {
            reply.send(Ok(rules.clone()));
        }
        trading.poll(&ctx, &mut Toasts::new());
        trading.account_panel.editing = Some((order.clone(), order.request.clone()));
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 600.0))
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::localization::set_translator(ctx, |english| Some(format!("[{english}]")));
            })
            .build_ui_state(
                |ui, trading: &mut Trading| {
                    trading.show_account_panel(ui, "SYM");
                },
                trading,
            );
        harness.run();
        let level = "[Take profit 2 quantity]";
        // Filled, whether the order has levels, the field, its value, the total and the reason.
        for (filled, levels, field, value, total, reason) in [
            (0.0, true, level, 2800.0, 5000.0, None),
            (
                0.0,
                true,
                level,
                400.0,
                2000.0,
                Some("[Each take-profit level needs at least the minimum quantity of 1,000]"),
            ),
            (
                0.0,
                true,
                level,
                60000.0,
                62000.0,
                Some("[The quantity is above the maximum of 20,000]"),
            ),
            (
                3000.0,
                true,
                level,
                400.0,
                2000.0,
                Some("[The quantity is below the 3,000 already filled]"),
            ),
            (
                0.0,
                false,
                "[Quantity]",
                1500.0,
                1500.0,
                Some("[Enter quantities in steps of 1,000]"),
            ),
        ] {
            let mut edited = order.clone();
            edited.filled = filled;
            if !levels {
                edited.request.take_profit_levels.clear();
            }
            harness.state_mut().account_panel.editing = Some((edited.clone(), edited.request));
            harness.run();
            harness
                .get_by_role_and_label(egui::Role::SpinButton, field)
                .set_value_accesskit(value);
            harness.run();
            let edited = &harness.state().account_panel.editing.as_ref().unwrap().1;
            assert_eq!(edited.quantity, total);
            if levels {
                assert_eq!(edited.take_profit_levels[1].quantity, total - 2000.0);
            }
            if let Some(reason) = reason {
                harness.get_by_label(reason);
            }
            harness
                .get_by_role_and_label(egui::Role::Button, "[Modify order]")
                .click();
            harness.run();
            let modified = broker.modified.lock().unwrap();
            assert_eq!(modified.len(), 1);
            let mut expected = order.request.clone();
            expected.quantity = 5000.0;
            expected.take_profit_levels[1].quantity = 3000.0;
            assert_eq!(modified[0], (order.id.clone(), expected));
        }
    }

    /// The Modify order dialog shows a position exit's quantity without letting it change, says
    /// why, checks no quantity rule against it and offers no exits of its own, so moving its price
    /// still saves. A regression lets the user resize an exit the broker refuses to resize, leaves
    /// the greyed-out quantity unexplained, or blocks the price change with a quantity problem the
    /// user cannot fix.
    #[test]
    fn the_modify_order_dialog_keeps_a_position_exits_quantity() {
        use crate::model::{PositionBracket, TradingRules};
        use egui_kittest::kittest::NodeT;
        let mut request = OrderRequest::new("SYM", OrderSide::Sell, 500.0, OrderType::Stop(95.0));
        request.position_bracket = Some(PositionBracket::StopLoss);
        let order = Order::new(OrderId::new("9"), request, OrderStatus::Working);
        let broker = Arc::new(Recorder {
            brackets: true,
            ..Recorder::default()
        });
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        let ctx = egui::Context::default();
        trading.poll(&ctx, &mut Toasts::new());
        trading.load_rules("SYM");
        // 500 is below the minimum and off the step.
        let mut rules = TradingRules::new("SYM");
        rules.quantity_step = 1000.0;
        rules.min_quantity = 1000.0;
        for (_, reply) in std::mem::take(&mut *broker.rules.lock().unwrap()) {
            reply.send(Ok(rules.clone()));
        }
        trading.poll(&ctx, &mut Toasts::new());
        trading.account_panel.editing = Some((order.clone(), order.request.clone()));
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 600.0))
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::localization::set_translator(ctx, |english| Some(format!("[{english}]")));
            })
            .build_ui_state(
                |ui, trading: &mut Trading| {
                    trading.show_account_panel(ui, "SYM");
                },
                trading,
            );
        harness.run();
        let quantity = harness.get_by_role_and_label(egui::Role::SpinButton, "[Quantity]");
        assert!(quantity.accesskit_node().is_disabled());
        harness.get_by_label(
            "[A position exit keeps its quantity. Cancel it and place a new exit to change it.]",
        );
        for absent in [
            "[Take profit]",
            "[Stop loss]",
            "[Enter quantities in steps of 1,000]",
            "[The quantity is below the minimum of 1,000]",
        ] {
            assert!(harness.query_by_label(absent).is_none(), "{absent}");
        }
        harness
            .get_by_role_and_label(egui::Role::SpinButton, "[Stop price]")
            .set_value_accesskit(96.0);
        harness.run();
        harness
            .get_by_role_and_label(egui::Role::Button, "[Modify order]")
            .click();
        harness.run();
        let mut expected = order.request.clone();
        expected.order_type = OrderType::Stop(96.0);
        assert_eq!(
            *broker.modified.lock().unwrap(),
            [(order.id.clone(), expected)]
        );
    }

    /// While the host names no broker and before the broker reports the account, the broker and
    /// account menus show only their chevrons; the accessibility check still finds each named
    /// after what it chooses. A regression leaves them unnamed.
    #[test]
    fn menus_without_text_are_named_after_what_they_choose() {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 400.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, trading: &mut Trading| {
                    trading.show_account_panel(ui, "SYM");
                },
                Trading::default(),
            );
        harness.run();
        for name in ["Broker", "Account"] {
            harness.get_by_role_and_label(egui::Role::Button, name);
        }
    }

    /// In panels as narrow as the demo gives on 320 and 393 point screens, the Trade, minimize
    /// and maximize buttons stay apart and inside the panel, with or without the broker's
    /// connection mark.
    #[test]
    fn tab_strip_buttons_stay_apart_in_narrow_panels() {
        use crate::data::FeedStatus;
        for width in [264.0, 337.0] {
            for status in [
                None,
                Some(FeedStatus::Connecting),
                Some(FeedStatus::Disconnected),
            ] {
                let mut trading = Trading::default();
                trading.set_service(Arc::new(Recorder::default()));
                trading.poll(&egui::Context::default(), &mut Toasts::new());
                if let Some(status) = status {
                    trading.apply(
                        &egui::Context::default(),
                        AccountEvent::Status(status),
                        &mut Toasts::new(),
                    );
                }
                trading.account_panel.broker = "Paper Trading".into();
                // The harness keeps an 8 point margin around the panel.
                let mut harness = Harness::builder()
                    .with_size(egui::vec2(width + 16.0, 320.0))
                    .with_context_setup(egui_extras::install_image_loaders)
                    .build_ui_state(
                        |ui, trading: &mut Trading| {
                            trading.show_account_panel(ui, "SYM");
                        },
                        trading,
                    );
                harness.run();
                let rects = ["Trade", "Minimize panel", "Maximize panel"]
                    .map(|label| harness.get_by_label(label).rect());
                for (index, rect) in rects.iter().enumerate() {
                    let case = format!("{status:?} at {width}: {index}");
                    assert!(rect.min.x >= 8.0 && rect.max.x <= 8.0 + width, "{case}");
                    for other in &rects[index + 1..] {
                        assert!(!rect.intersect(*other).is_positive(), "{case}");
                    }
                }
            }
        }
    }

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
        let mut trailing = order.clone();
        trailing.request.order_type = OrderType::TrailingStop {
            distance: crate::model::TrailDistance::Price(2.0),
            activation: None,
        };
        trailing.trailing_stop = Some(98.0);
        panel.editing = Some((trailing.clone(), trailing.request.clone()));
        trailing.trailing_stop = Some(99.0);
        panel.record(&AccountEvent::Order(trailing));
        assert!(
            panel.editing.is_some(),
            "a moving trailing stop keeps the dialog"
        );
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
            panel.record(&AccountEvent::ClosedPosition(ClosedPosition::new(
                format!("NEW-{i}"),
                OrderSide::Buy,
                1.0,
                100.0,
                101.0,
                1.0,
                time,
            )));
        }
        assert_eq!(panel.history.len(), 1_000);
        assert_eq!(panel.balance.len(), 1_000);
        assert_eq!(panel.journal.len(), 1_000);
        assert_eq!(panel.closed.len(), 1_000);
        assert_eq!(panel.closed[0].symbol, "NEW-1005");
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
        assert!(panel.closed.is_empty());
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
                    logo: None,
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
    fn last_fill_belongs_to_its_order_symbol_and_newest_execution() {
        let mut fills = Fills::default();
        let order = Order::new(
            OrderId::new("a"),
            OrderRequest::new("ES", OrderSide::Buy, 2.0, OrderType::Market),
            crate::model::OrderStatus::Working,
        );
        fills.order(&order);
        let now = chrono::Utc::now();
        let mut fill = Execution::new(order.id.clone(), now, OrderSide::Buy, 100.0, 1.0);
        fills.execution(&fill);
        fill.symbol = Some("CL".into());
        assert_eq!(fills.symbol(&fill), Some("ES"));
        fill.symbol = None;
        fill.time -= chrono::Duration::seconds(1);
        fill.price = 99.0;
        fills.execution(&fill);
        fill.order = OrderId::new("unknown");
        fills.execution(&fill);
        assert_eq!(fills.price("ES"), Some(100.0));
        assert_eq!(fills.price("NQ"), None);
        let mut cl = fill.clone();
        cl.order = OrderId::new("unknown-cl");
        cl.symbol = Some("CL".into());
        fills.execution(&cl);
        assert_eq!(fills.price("CL"), Some(99.0));
        let mut delayed = order.clone();
        delayed.id = fill.order.clone();
        delayed.request.symbol = "NQ".into();
        fills.order(&delayed);
        assert_eq!(fills.price("NQ"), Some(99.0));
        assert_eq!(fills.price("ES"), Some(100.0));
    }
}
