//! The account panel: the connected account's summary, positions, working orders, order and
//! balance history and trading journal, shown at the bottom of the terminal.

mod history;
mod view;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use egui::Color32;

use history::Paging;

use super::Trading;
use crate::model::{
    BalanceChange, Execution, JournalEntry, Order, OrderId, OrderRequest, OrderStatus, OrderType,
    Position,
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
#[derive(Default)]
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
        if let Some(order) = self.orders.get(&fill.order) {
            let symbol = order.symbol.clone();
            self.remember(&symbol, fill);
        }
        if let Some(old) = dropped {
            self.forget(&old.order);
        }
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
/// margin of working orders, each shown when the broker reports it. The tab marks a broker that
/// reports that it is connecting or disconnected, until it reports that it is live. Below are the
/// positions, working orders, finished orders, balance changes and trading journal the broker
/// pushes, each with a column chooser and an export. Order History also lists individual
/// executions. A position offers close, and an order offers edit and cancel on hover; each waits
/// for the user's confirmation like the chart's lines. Clicking
/// a symbol asks the host to show it. Each history keeps the most recent 1,000 entries; active
/// orders are retained independently. Scrolling to the end of a history asks
/// [`Broker::history`](crate::services::Broker::history) for the entries before its oldest
/// one, a page at a time, until the broker has nothing older or the history lists 1,000
/// entries. Each entry is listed once, whether a page or the broker's stream brings it first,
/// and pages change no working order or position.
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
    pub(super) fills: Fills,
    show_executions: bool,
    balance: Vec<BalanceChange>,
    /// How many balance changes at the front came from pages that the stream has not repeated.
    balance_paged: usize,
    journal: Vec<JournalEntry>,
    /// The same for `journal`.
    journal_paged: usize,
    /// Paging through the broker's older history, by kind, since the stream started.
    paging: HashMap<HistoryKind, Paging>,
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

    /// The broker history this tab lists and pages through.
    fn history(self) -> Option<HistoryKind> {
        match self {
            Tab::History => Some(HistoryKind::Orders),
            Tab::Executions => Some(HistoryKind::Executions),
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
        self.paging.clear();
        self.editing = None;
    }
}

/// Decimals that show `quantity` without rounding, up to 8.
pub(super) fn quantity_decimals(quantity: f64) -> usize {
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
    use std::sync::Arc;

    use egui_kittest::{Harness, kittest::Queryable};

    use super::*;
    use crate::model::OrderSide;
    use crate::ui::trading::tests::Recorder;
    use crate::ui_kit::notifications::Toasts;

    /// A position's close button can be found by scanning the rows, without the pointer on one.
    #[test]
    fn row_actions_show_at_rest() {
        let broker = Arc::new(Recorder {
            state: vec![AccountEvent::Position(Position::new("SYM", 1.0, 100.0))],
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
                    trading.apply(AccountEvent::Status(status), &mut Toasts::new());
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
        fill.time -= chrono::Duration::seconds(1);
        fill.price = 99.0;
        fills.execution(&fill);
        fill.order = OrderId::new("unknown");
        fills.execution(&fill);
        assert_eq!(fills.price("ES"), Some(100.0));
        assert_eq!(fills.price("NQ"), None);
        let mut delayed = order.clone();
        delayed.id = fill.order.clone();
        delayed.request.symbol = "NQ".into();
        fills.order(&delayed);
        assert_eq!(fills.price("NQ"), Some(99.0));
        assert_eq!(fills.price("ES"), Some(100.0));
    }
}
