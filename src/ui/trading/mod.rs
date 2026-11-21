//! Trading from the chart through the host's [`Broker`]: the broker picker, the order ticket,
//! the Buy and Sell buttons, depth ladder and the account panel. The broker holds the account;
//! this module shows what it pushes and sends the orders the user confirms.

mod account_panel;
mod broker_picker;
mod dom;
mod events;
mod executions;
mod format;
mod ticket;
mod view;
mod widgets;

pub use account_panel::{AccountPanel, AccountPanelAction};
pub use broker_picker::{BrokerFact, BrokerPicker, BrokerPickerAction, BrokerTile};
pub use dom::DepthPanel;
pub use executions::{ExecutionMark, ExecutionMarkStyle, render_executions};
pub use format::TradingFormatter;
pub use ticket::{Bracket, BracketMode, Kind as OrderKind, OrderPreset, PresetLevel, PresetSizing};
pub(crate) use widgets::grouped;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::chart::TradingLineAction;
use crate::data::inbox::Inbox;
use crate::data::{FeedError, FeedStatus};
use crate::model::{
    Account, Capabilities, Instrument, Order, OrderId, OrderPreview, OrderRequest, OrderSide,
    OrderType, Position, Quote, TradingRules,
};
use crate::services::{Broker, HistoryKind, Listener};
use crate::ui::drawing_toolbar::{DrawingToolbarAction, ToolbarState};
use crate::ui::model::ConnectionState;
use crate::widget::settings::TradingSettings;
use format::Numbers;
use ticket::{OrderTicket, order_quantity};

/// The trading controls of one chart, connected to the host's [`Broker`].
///
/// Call [`poll`](Self::poll) every frame, [`show_ticket`](Self::show_ticket) before laying out
/// the chart, so the order ticket takes its panel beside it, and [`show`](Self::show) after the
/// chart. Open the order ticket with [`open_ticket`](Self::open_ticket), for example from the
/// chart menu's [`Order`](crate::ui::context_menu::ContextMenuAction::Order) entries, and route
/// the chart's Buy and Sell buttons ([`Chart::take_quote_action`](crate::widget::Chart::take_quote_action)) to [`trade`](Self::trade).
/// Without a two-sided quote the buttons show the last price while a broker is set.
/// Orders reach the broker only from the ticket's submit button, or once the user confirms them
/// in the depth ladder, or at once from Buy and Sell and the ladder when the user turned on
/// [`TradingSettings::instant_orders`].
///
/// ```no_run
/// # fn frame(ui: &mut egui::Ui, chart: &mut egui_charts::Chart,
/// #     trading: &mut egui_charts::ui::trading::Trading,
/// #     toasts: &mut egui_charts::ui_kit::notifications::Toasts) {
/// use egui_charts::model::OrderSide;
/// use egui_charts::widget::QuoteSide;
///
/// trading.poll(ui.ctx(), toasts);
/// trading.show_ticket(ui, chart);
/// chart.show(ui);
/// if let Some(side) = chart.take_quote_action() {
///     if side == QuoteSide::Ticket {
///         trading.open_ticket(egui_charts::model::OrderRequest::new(chart.symbol(),
///             OrderSide::Buy, trading.quantity, egui_charts::model::OrderType::Market));
///     } else {
///         let side = if side == QuoteSide::Buy { OrderSide::Buy } else { OrderSide::Sell };
///         trading.trade(chart.symbol().to_owned(), side);
///     }
/// }
/// trading.show(ui, chart);
/// # }
/// ```
pub struct Trading {
    service: Option<Arc<dyn Broker>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    /// The connection status the broker last pushed on the current stream; `None`, which counts
    /// as connected, until it pushes one.
    connection: Option<FeedStatus>,
    /// Set while the pushes the broker made during [`Broker::connect`] wait in the inbox: they
    /// restate the account and raise no notification.
    restating: bool,
    account: Option<Account>,
    account_id: String,
    orders: Vec<Order>,
    positions: Vec<Position>,
    rules: TradingRules,
    rules_key: u64,
    request_key: u64,
    rules_ready: bool,
    /// Whether the rules of `rules.symbol` failed for another reason than the connection, or
    /// came back invalid: they are not asked for again when the broker is live again.
    rules_failed: bool,
    /// Confirmed position orders waiting for their symbol's trading rules.
    placing: Vec<(u64, Position, OrderRequest)>,
    ticket: OrderTicket,
    /// A change the user asked for on a line, waiting for confirmation.
    pending: Option<TradingLineAction>,
    /// The position to protect, with the take profit and stop loss entered so far.
    protecting: Option<(Position, Option<f64>, Option<f64>)>,
    /// The quantity in a pending Close dialog; each new confirmation resets it.
    close_quantity: Option<f64>,
    /// An order the user asked the broker to preview, until it is placed or discarded.
    preview: Option<Preview>,
    /// Decimals of the chart's prices at the last [`show`](Self::show), in which
    /// [`numbers`](Self::numbers) writes the prices of the chart symbol.
    decimals: usize,
    /// The metadata of the symbols the host described with
    /// [`set_instrument`](Self::set_instrument), by symbol.
    instruments: HashMap<String, Instrument>,
    /// The chart symbol and its logo at the last [`show`](Self::show), for rows and notices.
    chart_logo: Option<(String, Option<String>)>,
    formatter: Option<Arc<dyn TradingFormatter>>,
    /// The latest quote of a symbol, from [`set_quote`](Self::set_quote).
    quote: Option<(String, Quote)>,
    /// The quantity the ticket opens with, and that instant orders and the chart menu's orders
    /// use. Positive, in the symbol's trading units. The chart's legend shows it under the quote
    /// buttons' spread, where a click opens a calculator that changes it.
    pub quantity: f64,
    /// The ticket presets by the broker's [`account_id`](Broker::account_id). Hosts persist
    /// them by this stable id; brokers that keep the default share the empty key.
    pub presets: BTreeMap<String, Vec<OrderPreset>>,
    /// The account panel's names and the history it lists.
    pub account_panel: AccountPanel,
    /// The chart settings' trading options: what [`show`](Self::show) draws, whether Buy and
    /// Sell trade at once, which events raise notifications and whether fills ask for a sound.
    /// Copy the chart settings' [`trading`](crate::widget::settings::ChartSettingsState::trading)
    /// options here when the user applies them, and read them back when opening the dialog or
    /// saving: [`apply_hide_action`](Self::apply_hide_action) changes them from the Hide menu.
    pub display: TradingSettings,
    /// The sound asked for by fills since the host last took it.
    sound: Option<String>,
    /// The open quantity calculator: where it sits and the text of its field.
    calculator: Option<(egui::Pos2, String)>,
    /// The action behind each order answer, kept until the broker answers so a refusal names it.
    sent: Vec<(u64, Sent)>,
    /// User cancellations stay silent until the order ends or the cancellation is refused.
    cancelling: HashSet<OrderId>,
    /// A loss was announced, but no later live update has announced the return yet.
    connection_lost: bool,
}

impl std::fmt::Debug for Trading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trading")
            .field("generation", &self.generation)
            .field("connection", &self.connection)
            .field("restating", &self.restating)
            .field("account", &self.account)
            .field("account_id", &self.account_id)
            .field("orders", &self.orders)
            .field("positions", &self.positions)
            .field("rules", &self.rules)
            .field("rules_key", &self.rules_key)
            .field("request_key", &self.request_key)
            .field("rules_ready", &self.rules_ready)
            .field("rules_failed", &self.rules_failed)
            .field("placing", &self.placing)
            .field("pending", &self.pending)
            .field("close_quantity", &self.close_quantity)
            .field("decimals", &self.decimals)
            .field("instruments", &self.instruments)
            .field("chart_logo", &self.chart_logo)
            .field("quote", &self.quote)
            .field("quantity", &self.quantity)
            .field("presets", &self.presets)
            .field("account_panel", &self.account_panel)
            .field("display", &self.display)
            .field("sound", &self.sound)
            .field("calculator", &self.calculator)
            .field("sent", &self.sent)
            .field("cancelling", &self.cancelling)
            .field("connection_lost", &self.connection_lost)
            .finish_non_exhaustive()
    }
}

/// An order sent to [`Broker::preview`] and the estimate it was answered with. Confirming the
/// preview places `request` itself, whatever the ticket shows by then.
#[derive(Debug)]
struct Preview {
    key: u64,
    request: OrderRequest,
    estimate: Option<OrderPreview>,
}

/// An action sent to the broker, or refused locally, awaiting its answer.
#[derive(Debug)]
enum Sent {
    Place(OrderRequest),
    /// The last listed request may be absent if the order ended before confirmation.
    Cancel(OrderId, Option<OrderRequest>),
    /// Describe the order before the change, which a refusal leaves in place.
    Modify(OrderRequest),
}

impl Default for Trading {
    fn default() -> Self {
        Self {
            service: None,
            inbox: Arc::default(),
            listener: None,
            generation: 0,
            connection: None,
            restating: false,
            account: None,
            account_id: String::new(),
            orders: Vec::new(),
            positions: Vec::new(),
            rules: TradingRules::new(""),
            rules_key: 1,
            request_key: 1,
            rules_ready: false,
            rules_failed: false,
            placing: Vec::new(),
            ticket: OrderTicket::default(),
            pending: None,
            protecting: None,
            close_quantity: None,
            preview: None,
            decimals: 2,
            instruments: HashMap::new(),
            chart_logo: None,
            formatter: None,
            quote: None,
            quantity: 1.0,
            presets: BTreeMap::new(),
            account_panel: AccountPanel::default(),
            display: TradingSettings::default(),
            sound: None,
            calculator: None,
            sent: Vec::new(),
            cancelling: HashSet::new(),
            connection_lost: false,
        }
    }
}

pub(crate) use crate::chart::quantity_text;

impl Trading {
    /// Sets the latest quote of `symbol`, such as the one `WatchlistPanel::quote` holds for the
    /// chart's symbol. While the chart shows `symbol`, the ticket enters a market buy at the
    /// quote's ask and a market sell at its bid; without them, at the last price.
    pub fn set_quote(&mut self, symbol: &str, quote: Quote) {
        self.quote = Some((symbol.to_owned(), quote));
    }

    /// Describes `instrument` to the trading panels: its price decimals write the prices of its
    /// symbol in the account tables, and its pip size gives the ticket a pip value when the
    /// broker's [`TradingRules::pip_value`] is absent. The chart's own instrument is used for the
    /// symbol it shows, so call this for the other symbols of the account, such as those of
    /// positions and working orders. Its logo marks the symbol in the account tables and notices.
    /// A symbol without an instrument shows two decimals.
    pub fn set_instrument(&mut self, instrument: Instrument) {
        self.instruments
            .insert(instrument.symbol.clone(), instrument);
    }

    /// Writes the prices, quantities, spreads and pip values of the ticket, the depth ladder and
    /// the account tables and fill marks, and the prices and quantities of order and fill
    /// notifications and the quantities of order, bracket and position lines, with `formatter`,
    /// where it returns text. `None` restores the built-in text.
    pub fn set_formatter(&mut self, formatter: Option<Arc<dyn TradingFormatter>>) {
        self.formatter = formatter;
    }

    /// The logo of `symbol`: the chart instrument's for the chart symbol, else that of the
    /// instrument the host described.
    fn logo(&self, symbol: &str) -> Option<&str> {
        match &self.chart_logo {
            Some((chart, logo)) if chart == symbol => logo.as_deref(),
            _ => self.instruments.get(symbol)?.logo.as_deref(),
        }
    }

    /// How the numbers of `symbol` are written, with the decimals of the instrument the host
    /// described, or of the chart's prices for `chart_symbol`.
    fn numbers<'a>(&'a self, symbol: &'a str, chart_symbol: &str) -> Numbers<'a> {
        let decimals = if symbol == chart_symbol {
            self.decimals
        } else {
            (self.instruments.get(symbol))
                .and_then(crate::scales::instrument_price_precision)
                .map_or(2, usize::from)
        };
        Numbers {
            symbol,
            decimals,
            formatter: self.formatter.as_deref(),
        }
    }

    /// Connects to `service`, dropping the previous account's state. A position order that still
    /// waits for its trading rules is reported as refused at the next [`poll`](Self::poll).
    pub fn set_service(&mut self, service: Arc<dyn Broker>) {
        self.listener = None;
        self.account_id = service.account_id().to_owned();
        self.service = Some(service);
        self.inbox = Arc::default();
        self.ticket.open = false;
        self.connect();
    }

    /// Disconnects from the broker, dropping the account's state. A position order that still
    /// waits for its trading rules is reported as refused at the next [`poll`](Self::poll).
    pub fn disconnect(&mut self) {
        self.listener = None;
        self.service = None;
        self.account_id.clear();
        self.inbox = Arc::default();
        self.display.instant_orders = false;
        self.ticket.open = false;
        self.connect();
    }

    /// Whether a broker is set and connected: the last status it pushed on the current stream is
    /// [`FeedStatus::Live`], or it pushed none. False while it reports that it is
    /// [`Connecting`](FeedStatus::Connecting) or [`Disconnected`](FeedStatus::Disconnected), and
    /// without a broker; [`has_broker`](Self::has_broker) tells whether one is set.
    pub fn is_connected(&self) -> bool {
        self.service.is_some()
            && self
                .connection
                .is_none_or(|status| status == FeedStatus::Live)
    }

    /// Whether a broker is set, from [`set_service`](Self::set_service) until
    /// [`disconnect`](Self::disconnect), connected or not.
    pub fn has_broker(&self) -> bool {
        self.service.is_some()
    }

    /// The connection status the account panel and the order ticket show while the broker
    /// reports that it is not live; `None` while it is live, before it reports a status and
    /// without a broker.
    fn offline(&self) -> Option<ConnectionState> {
        match self.connection {
            Some(FeedStatus::Connecting) => Some(ConnectionState::Connecting),
            Some(FeedStatus::Disconnected) => Some(ConnectionState::Disconnected),
            Some(FeedStatus::Live) | None => None,
        }
    }

    /// Starts over with the broker in `service`, or without one: the order ticket, a previewed
    /// order, the execution sound, the quantity calculator and the trading rules of the previous
    /// account are dropped, and a new stream starts.
    fn connect(&mut self) {
        self.sent.clear();
        self.cancelling.clear();
        self.connection_lost = false;
        self.preview = None;
        self.sound = None;
        self.calculator = None;
        self.ticket = OrderTicket::default();
        self.rules = TradingRules::new("");
        self.rules_ready = false;
        self.rules_failed = false;
        self.listen();
    }

    /// Starts a new stream, on which the broker restates the account. What the old stream built
    /// is dropped, and so are the changes to its orders and positions that wait for
    /// confirmation, since those orders and positions may have changed unseen. Confirmed
    /// position orders that wait for trading rules lose their answer with the old stream and are
    /// reported as refused. The order ticket, a previewed order, the quantity calculator and the
    /// trading rules stay.
    fn listen(&mut self) {
        // Their rules answers belong to the old stream, whose positions may have changed unseen.
        for (_, _, order) in std::mem::take(&mut self.placing) {
            self.refuse(Sent::Place(order), NOT_LIVE);
        }
        self.pending = None;
        self.protecting = None;
        self.account = None;
        self.connection = None;
        self.orders.clear();
        self.positions.clear();
        self.account_panel.clear();
        // Stream keys are even; rules, previews, history pages and order answers use odd keys.
        self.generation += 2;
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        self.restating = self.service.is_some();
        if let Some(service) = &self.service {
            service.connect(events);
            // Queued after what the broker pushed while connecting, this ends the restated state.
            self.inbox.reply(self.generation).send(Ok(()));
        }
    }

    /// The order types and features the account accepts; none before the broker reports them.
    pub fn capabilities(&self) -> Capabilities {
        self.account
            .as_ref()
            .map(|account| account.capabilities)
            .unwrap_or_default()
    }

    /// Refresh the Hide menu's position, order and execution checks from [`display`](Self::display).
    /// Call this before each
    /// [`DrawingToolbar::show_with_action`](crate::ui::drawing_toolbar::DrawingToolbar::show_with_action),
    /// then pass its returned action to [`apply_hide_action`](Self::apply_hide_action).
    pub fn sync_hide_menu(&self, state: &mut ToolbarState) {
        state.hide_positions = !self.display.show_positions;
        state.hide_orders = !self.display.show_orders;
        state.hide_executions = !self.display.show_executions;
    }

    /// Apply the visibility requested by the toolbar after a Hide menu click to
    /// [`display`](Self::display): [`HideAllPoss`](DrawingToolbarAction::HideAllPoss) changes
    /// positions, [`HideAllOrders`](DrawingToolbarAction::HideAllOrders) orders,
    /// [`HideAllExecutions`](DrawingToolbarAction::HideAllExecutions) execution marks, and
    /// [`HideAll`](DrawingToolbarAction::HideAll) all three. Other actions leave it unchanged.
    /// Refresh `state` with [`sync_hide_menu`](Self::sync_hide_menu) before showing the toolbar
    /// so its flags hold the requested visibility after the click.
    pub fn apply_hide_action(&mut self, action: &DrawingToolbarAction, state: &ToolbarState) {
        use DrawingToolbarAction as A;
        let all = matches!(action, A::HideAll);
        if all || matches!(action, A::HideAllPoss) {
            self.display.show_positions = !state.hide_positions;
        }
        if all || matches!(action, A::HideAllOrders) {
            self.display.show_orders = !state.hide_orders;
        }
        if all || matches!(action, A::HideAllExecutions) {
            self.display.show_executions = !state.hide_executions;
        }
    }

    /// Opens the order ticket filled from `request`, for the user to check and submit.
    pub fn open_ticket(&mut self, request: OrderRequest) {
        self.load_rules(&request.symbol);
        self.ticket.open(&request);
    }

    /// Opens the order ticket for `symbol`, or closes it when it is open, as the toolbar's Trade
    /// button does.
    pub fn toggle_ticket(&mut self, symbol: String) {
        if self.ticket.open {
            self.ticket.open = false;
        } else {
            self.open_ticket(OrderRequest::new(
                symbol,
                OrderSide::Buy,
                self.quantity,
                OrderType::Market,
            ));
        }
    }

    /// Handles the Buy or Sell button: a market order of [`quantity`](Self::quantity) at once
    /// with [`TradingSettings::instant_orders`], otherwise the ticket for one.
    pub fn trade(&mut self, symbol: String, side: OrderSide) {
        self.load_rules(&symbol);
        let mut request = OrderRequest::new(symbol, side, self.quantity, OrderType::Market);
        if self.display.instant_orders
            && self.capabilities().market
            && self.rules_ready
            && let Ok(quantity) = order_quantity(self.quantity, &self.rules)
        {
            self.ticket.open = false;
            request.quantity = quantity;
            request.time_in_force = self.rules.time_in_force[0];
            self.place(request);
        } else {
            self.open_ticket(request);
        }
    }

    fn place(&mut self, request: OrderRequest) {
        if let Some((service, key)) = self.live_service(Sent::Place(request.clone())) {
            service.place(request, self.inbox.reply(key));
        }
    }

    /// Cancels `id`, describing it with its last listed request when it is still available.
    fn cancel_order(&mut self, id: OrderId, request: Option<OrderRequest>) {
        if let Some((service, key)) = self.live_service(Sent::Cancel(id.clone(), request)) {
            self.cancelling.insert(id.clone());
            service.cancel(id, self.inbox.reply(key));
        }
    }

    /// Tracks `sent` and returns the live broker with its answer key. While the broker is not
    /// live, nothing is sent and the next [`poll`](Self::poll) reports that action as refused.
    fn live_service(&mut self, sent: Sent) -> Option<(Arc<dyn Broker>, u64)> {
        let service = self.service.clone()?;
        if !self.is_connected() {
            self.refuse(sent, NOT_LIVE);
            return None;
        }
        Some((service, self.track(sent)))
    }

    /// Allocates an answer key and keeps the action until its answer is read.
    fn track(&mut self, sent: Sent) -> u64 {
        self.request_key += 2;
        self.sent.push((self.request_key, sent));
        self.request_key
    }

    /// Queues a local refusal through the same answer path as the broker's refusals.
    fn refuse(&mut self, sent: Sent, reason: &str) {
        let key = self.track(sent);
        self.inbox
            .reply::<()>(key)
            .send(Err(FeedError::Other(reason.into())));
    }

    fn load_rules(&mut self, symbol: &str) {
        if self.rules.symbol != symbol {
            self.pending = None;
            self.rules = TradingRules::new(symbol);
            self.rules_ready = false;
            self.rules_failed = false;
            self.ask_rules();
        }
    }

    /// Asks the broker for the rules of `rules.symbol` under a new key, so that the answer to an
    /// earlier request is dropped; while the broker is not live, the request waits until it is.
    fn ask_rules(&mut self) {
        self.request_key += 2;
        self.rules_key = self.request_key;
        if self.is_connected()
            && let Some(service) = &self.service
        {
            service.rules(&self.rules.symbol, self.inbox.reply(self.rules_key));
        }
    }

    /// Asks once more for the rules of `rules.symbol` when they have neither arrived nor been
    /// refused.
    fn ask_rules_again(&mut self) {
        if !self.rules_ready && !self.rules_failed && !self.rules.symbol.is_empty() {
            self.ask_rules();
        }
    }

    /// Asks the broker for the page of `kind` before the oldest entry the account panel lists,
    /// once the account arrived and while the broker is live; see [`AccountPanel`].
    fn load_older(&mut self, kind: HistoryKind) {
        let Some(service) = &self.service else {
            return;
        };
        if self.account.is_none() || !self.is_connected() {
            return;
        }
        let key = self.request_key + 2;
        if let Some(request) = self.account_panel.ask(kind, key) {
            self.request_key = key;
            service.history(request, self.inbox.reply(key));
        }
    }
}

/// Why an order is refused while the broker is not live.
const NOT_LIVE: &str = "the broker is not connected";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{FeedError, Reply};
    use crate::model::OrderId;
    use crate::services::{AccountEvent, Events, HistoryPage, HistoryRequest};
    use crate::ui_kit::notifications::Toasts;
    use std::sync::Mutex;

    /// A broker that pushes `state` on connecting and records the calls it gets.
    #[derive(Default)]
    pub(super) struct Recorder {
        pub(super) account: &'static str,
        pub(super) state: Vec<AccountEvent>,
        pub(super) brackets: bool,
        pub(super) partial_close: bool,
        pub(super) placed: Mutex<Vec<OrderRequest>>,
        pub(super) cancelled: Mutex<Vec<OrderId>>,
        pub(super) modified: Mutex<Vec<(OrderId, OrderRequest)>>,
        pub(super) rules: Mutex<Vec<(String, Reply<TradingRules>)>>,
        pub(super) previews: Mutex<Vec<(OrderRequest, Reply<OrderPreview>)>>,
        pub(super) histories: Mutex<Vec<(HistoryRequest, Reply<HistoryPage>)>>,
    }

    impl Broker for Recorder {
        fn account_id(&self) -> &str {
            self.account
        }
        fn connect(&self, events: Events<AccountEvent>) {
            let capabilities = Capabilities {
                market: true,
                limit: true,
                stop: true,
                modify: true,
                close: true,
                reverse: true,
                preview: true,
                brackets: self.brackets,
                partial_close: self.partial_close,
                take_profit_level_stops: self.brackets,
                ..Capabilities::default()
            };
            events.push(AccountEvent::Account(Account::new(
                "USD",
                0.0,
                capabilities,
            )));
            self.state
                .iter()
                .for_each(|event| events.push(event.clone()));
        }
        fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
            self.rules.lock().unwrap().push((symbol.to_owned(), reply));
        }
        fn preview(&self, order: OrderRequest, reply: Reply<OrderPreview>) {
            self.previews.lock().unwrap().push((order, reply));
        }
        fn modify(&self, id: OrderId, order: OrderRequest, reply: Reply<()>) {
            self.modified.lock().unwrap().push((id, order));
            reply.send(Ok(()));
        }
        fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
            self.placed.lock().unwrap().push(order);
            reply.send(Ok(OrderId::new("1")));
        }
        fn cancel(&self, id: OrderId, reply: Reply<()>) {
            self.cancelled.lock().unwrap().push(id);
            reply.send(Ok(()));
        }
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            self.histories.lock().unwrap().push((request, reply));
        }
    }

    /// Guards the money path of the Buy and Sell buttons: without instant orders they only
    /// open the ticket and send nothing; with it they send exactly one market order of the
    /// default quantity. A regression either trades without the user's confirmation or loses
    /// the instant order.
    #[test]
    fn buy_and_sell_trade_at_once_only_with_instant_orders() {
        let broker = Arc::new(Recorder::default());
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        trading.quantity = 3.0;

        trading.trade("AAPL".into(), OrderSide::Sell);
        assert!(broker.placed.lock().unwrap().is_empty());
        assert!(trading.ticket.open);

        trading.display.instant_orders = true;
        trading.trade("AAPL".into(), OrderSide::Buy);
        assert!(broker.placed.lock().unwrap().is_empty(), "wait for rules");
        let mut rules = TradingRules::new("AAPL");
        rules.quantity_step = 2.0;
        rules.min_quantity = 2.0;
        rules.max_quantity = Some(4.0);
        rules.time_in_force = vec![crate::model::TimeInForce::GoodTillCancelled];
        broker
            .rules
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Ok(rules));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        for (quantity, accepted) in [(3.0, true), (1.0, false), (6.0, false), (f64::NAN, false)] {
            trading.quantity = quantity;
            trading.trade("AAPL".into(), OrderSide::Buy);
            let placed = std::mem::take(&mut *broker.placed.lock().unwrap());
            if accepted {
                let mut expected =
                    OrderRequest::new("AAPL", OrderSide::Buy, 2.0, OrderType::Market);
                expected.time_in_force = crate::model::TimeInForce::GoodTillCancelled;
                assert_eq!(placed, [expected]);
                assert!(!trading.ticket.open);
            } else {
                assert!(placed.is_empty());
            }
        }
        trading.load_rules("ES");
        trading.load_rules("NQ");
        let mut replies = broker.rules.lock().unwrap();
        let current = replies.pop().unwrap().1;
        let stale = replies.pop().unwrap().1;
        stale.send(Err(FeedError::NotSupported));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        assert!(
            !trading.rules_ready,
            "an old answer cannot enable the new symbol"
        );
        current.send(Err(FeedError::NotSupported));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        assert!(trading.rules_ready);
    }

    /// Position orders wait for their symbol's rules and are sent once they arrive; a failure,
    /// invalid rules, a changed position or an order type the account stops accepting sends
    /// nothing and says why. A regression sends a stale order or drops it without a word.
    #[test]
    fn position_actions_use_their_symbol_rules_before_placing() {
        use crate::model::{PositionBracket, TimeInForce};
        for operation in ["close", "reverse", "take profit", "stop loss"] {
            for response in [
                "supported",
                "fallback",
                "failure",
                "invalid",
                "expiry first",
                "changed",
                "withdrawn",
            ] {
                let broker = Arc::new(Recorder::default());
                let mut trading = Trading::default();
                trading.set_service(broker.clone());
                let ctx = egui::Context::default();
                trading.load_rules("CHART");
                let (_, reply) = broker.rules.lock().unwrap().pop().unwrap();
                reply.send(Ok(TradingRules::new("CHART")));
                trading.poll(&ctx, &mut Toasts::new());
                let position = Position::new("OTHER", 2.0, 100.0);
                let mut expected = OrderRequest::new(
                    "OTHER",
                    OrderSide::Sell,
                    if operation == "reverse" { 4.0 } else { 2.0 },
                    match operation {
                        "take profit" => OrderType::Limit(110.0),
                        "stop loss" => OrderType::Stop(90.0),
                        _ => OrderType::Market,
                    },
                );
                expected.position_bracket = match operation {
                    "take profit" => Some(PositionBracket::TakeProfit),
                    "stop loss" => Some(PositionBracket::StopLoss),
                    _ => None,
                };
                let action = match operation {
                    "close" => TradingLineAction::Close(position),
                    "reverse" => TradingLineAction::Reverse(position),
                    _ => TradingLineAction::Exit(position, expected.clone()),
                };
                trading.execute(Some(action));
                assert!(
                    broker.placed.lock().unwrap().is_empty(),
                    "wait for {operation} rules"
                );
                let (symbol, reply) = broker.rules.lock().unwrap().pop().unwrap();
                assert_eq!(symbol, "OTHER");
                let mut rules = TradingRules::new(symbol);
                rules.time_in_force = vec![TimeInForce::GoodTillCancelled];
                if response == "changed" {
                    trading.apply(
                        &egui::Context::default(),
                        AccountEvent::Position(Position::new("OTHER", 1.0, 100.0)),
                        &mut Toasts::new(),
                    );
                }
                if response == "withdrawn" {
                    let account = Account::new("USD", 0.0, Capabilities::default());
                    let event = AccountEvent::Account(account);
                    trading.apply(&egui::Context::default(), event, &mut Toasts::new());
                }
                if response == "invalid" {
                    rules.time_in_force.clear();
                }
                if response == "expiry first" {
                    rules.time_in_force = vec![TimeInForce::GoodTillDate, TimeInForce::Day];
                }
                reply.send(match response {
                    "fallback" => Err(FeedError::NotSupported),
                    "failure" => Err(FeedError::Network("offline".into())),
                    _ => Ok(rules),
                });
                let mut toasts = Toasts::new();
                trading.poll(&ctx, &mut toasts);
                let messages: Vec<_> = (toasts.toasts().iter())
                    .map(|toast| toast.message.as_str())
                    .collect();
                let order = match operation {
                    "close" => "Sell 2 OTHER at market",
                    "reverse" => "Sell 4 OTHER at market",
                    "take profit" => "Sell 2 OTHER take profit 110.00",
                    _ => "Sell 2 OTHER stop loss 90.00",
                };
                let reason = match response {
                    "failure" => Some("trading rules unavailable: network error: offline"),
                    "invalid" | "expiry first" => Some("the broker returned invalid trading rules"),
                    "changed" => Some("the position changed before the order was sent"),
                    "withdrawn" => Some("the account no longer accepts this order type"),
                    _ => None,
                };
                let expected_messages: Vec<_> = reason
                    .into_iter()
                    .map(|reason| format!("Order refused: {order}: {reason}"))
                    .collect();
                assert_eq!(
                    messages, expected_messages,
                    "{operation} with {response} rules"
                );
                let placed = std::mem::take(&mut *broker.placed.lock().unwrap());
                if matches!(response, "supported" | "fallback") {
                    if response == "supported" {
                        expected.time_in_force = TimeInForce::GoodTillCancelled;
                    }
                    assert_eq!(placed, [expected]);
                } else {
                    assert!(placed.is_empty(), "{operation} with {response} rules");
                }
                assert_eq!(trading.rules.symbol, "CHART");
                assert!(trading.rules_ready);
            }
        }
    }
}
