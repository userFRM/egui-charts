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

pub use account_panel::{AccountPanel, AccountPanelAction};
pub use broker_picker::{BrokerFact, BrokerPicker, BrokerPickerAction, BrokerTile};
pub use format::TradingFormatter;
pub use ticket::{Bracket, BracketMode, OrderPreset};

use std::collections::HashMap;
use std::sync::Arc;

use crate::chart::TradingLineAction;
use crate::data::inbox::Inbox;
use crate::model::{
    Account, Capabilities, Instrument, Order, OrderPreview, OrderRequest, OrderSide, OrderType,
    Position, Quote, TradingRules,
};
use crate::services::{Broker, Listener};
use crate::widget::settings::TradingSettings;
use format::Numbers;
use ticket::{OrderTicket, order_quantity};

/// The trading controls of one chart, connected to the host's [`Broker`].
///
/// Call [`poll`](Self::poll) every frame, [`show_ticket`](Self::show_ticket) before laying out
/// the chart, so the order ticket takes its panel beside it, and [`show`](Self::show) after the
/// chart. Open the order ticket with [`open_ticket`](Self::open_ticket), for example from the
/// chart menu's [`Order`](crate::ui::context_menu::ContextMenuAction::Order) entries, and route
/// the chart's Buy and Sell buttons ([`Chart::take_quote_action`]) to [`trade`](Self::trade).
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
    account: Option<Account>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    rules: TradingRules,
    rules_key: u64,
    request_key: u64,
    rules_ready: bool,
    /// Confirmed position orders waiting for their symbol's trading rules.
    placing: Vec<(u64, Position, OrderRequest)>,
    ticket: OrderTicket,
    /// A change the user asked for on a line, waiting for confirmation.
    pending: Option<TradingLineAction>,
    /// An order the user asked the broker to preview, until it is placed or discarded.
    preview: Option<Preview>,
    /// Decimals of the chart's prices, for notifications.
    decimals: usize,
    /// The metadata of the symbols the host described with
    /// [`set_instrument`](Self::set_instrument), by symbol.
    instruments: HashMap<String, Instrument>,
    formatter: Option<Arc<dyn TradingFormatter>>,
    /// The latest quote of a symbol, from [`set_quote`](Self::set_quote).
    quote: Option<(String, Quote)>,
    /// The quantity the ticket opens with, and that instant orders and the chart menu's orders
    /// use. Positive, in the symbol's trading units. The chart's legend shows it under the quote
    /// buttons' spread, where a click opens a calculator that changes it.
    pub quantity: f64,
    /// The quantity and brackets the user saved in the ticket. Persist them in the host.
    pub presets: Vec<OrderPreset>,
    /// The account panel's names and the history it lists.
    pub account_panel: AccountPanel,
    /// The chart settings' trading options: what [`show`](Self::show) draws, whether Buy and
    /// Sell trade at once, which events raise notifications and whether fills ask for a sound.
    /// Copy the chart settings' [`trading`](crate::widget::settings::ChartSettingsState::trading)
    /// options here when the user applies them.
    pub display: TradingSettings,
    /// The sound asked for by fills since the host last took it.
    sound: Option<String>,
    /// The open quantity calculator: where it sits and the text of its field.
    calculator: Option<(egui::Pos2, String)>,
}

/// An order sent to [`Broker::preview`] and the estimate it was answered with. Confirming the
/// preview places `request` itself, whatever the ticket shows by then.
struct Preview {
    key: u64,
    request: OrderRequest,
    estimate: Option<OrderPreview>,
}

impl Default for Trading {
    fn default() -> Self {
        Self {
            service: None,
            inbox: Arc::default(),
            listener: None,
            generation: 0,
            account: None,
            orders: Vec::new(),
            positions: Vec::new(),
            rules: TradingRules::new(""),
            rules_key: 1,
            request_key: 1,
            rules_ready: false,
            placing: Vec::new(),
            ticket: OrderTicket::default(),
            pending: None,
            preview: None,
            decimals: 2,
            instruments: HashMap::new(),
            formatter: None,
            quote: None,
            quantity: 1.0,
            presets: Vec::new(),
            account_panel: AccountPanel::default(),
            display: TradingSettings::default(),
            sound: None,
            calculator: None,
        }
    }
}

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
    /// positions and working orders. A symbol without an instrument shows two decimals.
    pub fn set_instrument(&mut self, instrument: Instrument) {
        self.instruments
            .insert(instrument.symbol.clone(), instrument);
    }

    /// Writes the prices, quantities, spreads and pip values of the ticket, the depth ladder and
    /// the account tables with `formatter`, where it returns text. `None` restores the built-in
    /// text.
    pub fn set_formatter(&mut self, formatter: Option<Arc<dyn TradingFormatter>>) {
        self.formatter = formatter;
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

    /// Connects to `service`, dropping the previous account's state.
    pub fn set_service(&mut self, service: Arc<dyn Broker>) {
        self.listener = None;
        self.service = Some(service);
        self.inbox = Arc::default();
        self.ticket.open = false;
        self.connect();
    }

    /// Disconnects from the broker, dropping the account's state.
    pub fn disconnect(&mut self) {
        self.listener = None;
        self.service = None;
        self.inbox = Arc::default();
        self.display.instant_orders = false;
        self.ticket.open = false;
        self.connect();
    }

    /// Whether a broker is connected.
    pub fn is_connected(&self) -> bool {
        self.service.is_some()
    }

    fn connect(&mut self) {
        self.pending = None;
        self.preview = None;
        self.sound = None;
        self.calculator = None;
        self.ticket = OrderTicket::default();
        self.rules = TradingRules::new("");
        self.rules_ready = false;
        self.account = None;
        self.orders.clear();
        self.positions.clear();
        self.placing.clear();
        self.account_panel.clear();
        // Stream keys are even; symbol rules use odd keys, and order replies use zero.
        self.generation += 2;
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        if let Some(service) = &self.service {
            service.connect(events);
        }
    }

    /// The order types and features the account accepts; none before the broker reports them.
    pub fn capabilities(&self) -> Capabilities {
        self.account
            .as_ref()
            .map(|account| account.capabilities)
            .unwrap_or_default()
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

    fn place(&self, request: OrderRequest) {
        if let Some(service) = &self.service {
            service.place(request, self.inbox.reply(0));
        }
    }

    fn load_rules(&mut self, symbol: &str) {
        if self.rules.symbol != symbol {
            self.pending = None;
            self.rules = TradingRules::new(symbol);
            self.rules_ready = false;
            self.request_key += 2;
            self.rules_key = self.request_key;
            if let Some(service) = &self.service {
                service.rules(symbol, self.inbox.reply(self.rules_key));
            }
        }
    }
}

/// Whether every number of `rules` is positive and finite and a time in force is offered.
fn valid(rules: &TradingRules) -> bool {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    positive(rules.quantity_step)
        && positive(rules.min_quantity)
        && positive(rules.point_value)
        && rules
            .max_quantity
            .is_none_or(|max| positive(max) && max >= rules.min_quantity)
        && rules.leverage.is_none_or(positive)
        && rules.pip_value.is_none_or(positive)
        && !rules.time_in_force.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{FeedError, Reply};
    use crate::model::OrderId;
    use crate::services::{AccountEvent, Events};
    use crate::ui_kit::notifications::Toasts;
    use std::sync::Mutex;

    /// A broker that pushes `state` on connecting and records the calls it gets.
    #[derive(Default)]
    pub(super) struct Recorder {
        pub(super) state: Vec<AccountEvent>,
        pub(super) placed: Mutex<Vec<OrderRequest>>,
        pub(super) cancelled: Mutex<Vec<OrderId>>,
        pub(super) modified: Mutex<Vec<(OrderId, OrderRequest)>>,
        pub(super) rules: Mutex<Vec<(String, Reply<TradingRules>)>>,
        pub(super) previews: Mutex<Vec<(OrderRequest, Reply<OrderPreview>)>>,
    }

    impl Broker for Recorder {
        fn connect(&self, events: Events<AccountEvent>) {
            let capabilities = Capabilities {
                market: true,
                limit: true,
                stop: true,
                modify: true,
                close: true,
                reverse: true,
                preview: true,
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

    #[test]
    fn position_actions_use_their_symbol_rules_before_placing() {
        use crate::model::{PositionBracket, TimeInForce};
        for operation in ["close", "reverse", "take profit", "stop loss"] {
            for response in ["supported", "fallback", "failure", "invalid", "changed"] {
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
                        AccountEvent::Position(Position::new("OTHER", 1.0, 100.0)),
                        &mut Toasts::new(),
                    );
                }
                if response == "invalid" {
                    rules.time_in_force.clear();
                }
                reply.send(match response {
                    "fallback" => Err(FeedError::NotSupported),
                    "failure" => Err(FeedError::Network("offline".into())),
                    _ => Ok(rules),
                });
                trading.poll(&ctx, &mut Toasts::new());
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
