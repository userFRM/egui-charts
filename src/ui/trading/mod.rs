//! Trading from the chart through the host's [`Broker`]: the order ticket and the Buy and Sell
//! buttons. The broker holds the account; this module shows what it pushes and sends the orders
//! the user confirms.

mod ticket;

pub use ticket::{Bracket, BracketMode, OrderPreset};

use std::sync::Arc;

use crate::chart::selection::SeriesId;
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{
    Account, Capabilities, Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType,
    Position, TradingRules,
};
use crate::services::{AccountEvent, Broker, Listener};
use crate::ui_kit::notifications::Toasts;
use crate::widget::Chart;
pub(crate) use ticket::to_step;
use ticket::{Market, OrderTicket, format_quantity};

/// The trading controls of one chart, connected to the host's [`Broker`].
///
/// Call [`poll`](Self::poll) every frame, then [`show`](Self::show) after the chart. Open the
/// order ticket with [`open_ticket`](Self::open_ticket), for example from the chart menu's
/// [`Order`](crate::ui::context_menu::ContextMenuAction::Order) entries, and route the chart's
/// Buy and Sell buttons ([`Chart::take_quote_action`]) to [`trade`](Self::trade). Orders reach
/// the broker only from the ticket's submit button, or at once from Buy and Sell when the user
/// turned on [`one_click`](Self::one_click).
///
/// ```no_run
/// # fn frame(ui: &mut egui::Ui, chart: &mut egui_charts::Chart,
/// #     trading: &mut egui_charts::ui::trading::Trading,
/// #     toasts: &mut egui_charts::ui_kit::notifications::Toasts) {
/// use egui_charts::model::OrderSide;
/// use egui_charts::widget::QuoteSide;
///
/// trading.poll(ui.ctx(), toasts);
/// chart.show(ui);
/// if let Some(side) = chart.take_quote_action() {
///     let side = if side == QuoteSide::Buy { OrderSide::Buy } else { OrderSide::Sell };
///     trading.trade(chart.symbol().to_owned(), side);
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
    ticket: OrderTicket,
    /// The quantity the ticket opens with, and that one-click orders and the chart menu's
    /// orders use. Positive, in the symbol's trading units.
    pub quantity: f64,
    /// Buy and Sell place a market order of [`quantity`](Self::quantity) at once instead of
    /// opening the ticket. Off by default; the user turns it on in the ticket.
    pub one_click: bool,
    /// The quantity and brackets the user saved in the ticket. Persist them in the host.
    pub presets: Vec<OrderPreset>,
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
            ticket: OrderTicket::default(),
            quantity: 1.0,
            one_click: false,
            presets: Vec::new(),
        }
    }
}

impl Trading {
    /// Connects to `service`, dropping the previous account's state.
    pub fn set_service(&mut self, service: Arc<dyn Broker>) {
        self.listener = None;
        self.service = Some(service);
        self.inbox = Arc::default();
        self.rules = TradingRules::new("");
        self.connect();
    }

    fn connect(&mut self) {
        self.account = None;
        self.orders.clear();
        self.positions.clear();
        self.generation += 1;
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

    /// Handles the Buy or Sell button: a market order of [`quantity`](Self::quantity) at once
    /// with [`one_click`](Self::one_click), otherwise the ticket for one.
    pub fn trade(&mut self, symbol: String, side: OrderSide) {
        let request = OrderRequest::new(symbol, side, self.quantity, OrderType::Market);
        if self.one_click && self.capabilities().market {
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
            self.rules = TradingRules::new(symbol);
            if let Some(service) = &self.service {
                service.rules(symbol, self.inbox.reply(0));
            }
        }
    }

    /// Applies the broker's pushes and answers, reconnects a lagged stream, and reports refused
    /// orders and fills in `toasts`. Call it every frame, even while the chart is hidden.
    pub fn poll(&mut self, ctx: &egui::Context, toasts: &mut Toasts) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.connect();
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                if let Ok(event) = answer.downcast::<AccountEvent>() {
                    self.apply(*event, toasts);
                }
                continue;
            } else if key != 0 {
                continue;
            }
            let error = match answer.downcast::<Result<TradingRules, FeedError>>() {
                Ok(rules) => {
                    if let Ok(rules) = *rules
                        && rules.symbol == self.rules.symbol
                        && rules.is_valid()
                    {
                        self.rules = rules;
                    }
                    continue;
                }
                Err(answer) => match answer.downcast::<Result<OrderId, FeedError>>() {
                    Ok(placed) => placed.err(),
                    Err(answer) => answer
                        .downcast::<Result<(), FeedError>>()
                        .ok()
                        .and_then(|done| done.err()),
                },
            };
            if let Some(error) = error {
                toasts.error(format!("Order refused: {error}"));
            }
        }
    }

    fn apply(&mut self, event: AccountEvent, toasts: &mut Toasts) {
        match event {
            AccountEvent::Account(account) => self.account = Some(account),
            AccountEvent::Order(order) => {
                self.orders.retain(|working| working.id != order.id);
                match &order.status {
                    OrderStatus::Working => self.orders.push(order),
                    OrderStatus::Rejected(reason) => {
                        toasts.error(format!("Order rejected: {reason}"));
                    }
                    _ => {}
                }
            }
            AccountEvent::Position(position) => {
                self.positions.retain(|open| open.symbol != position.symbol);
                if position.quantity != 0.0 {
                    self.positions.push(position);
                }
            }
            AccountEvent::Execution(fill) => toasts.info(format!(
                "Filled: {} {} at {}",
                fill.side.label(),
                format_quantity(fill.quantity),
                fill.price
            )),
            _ => {}
        }
    }

    /// Draws the order ticket for `chart` and sends the order the user submits.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &Chart) {
        if !self.ticket.open {
            return;
        }
        let instrument = chart
            .instrument()
            .filter(|instrument| instrument.symbol == self.ticket.symbol);
        let decimals = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map_or(2, usize::from);
        let tick = instrument
            .and_then(|instrument| instrument.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0)
            .unwrap_or(10f64.powi(-(decimals as i32)));
        let last = (chart.symbol() == self.ticket.symbol)
            .then(|| chart.bars(SeriesId::MAIN).last().map(|bar| bar.close))
            .flatten();
        let market = Market {
            last,
            tick,
            decimals,
            currency: self
                .account
                .as_ref()
                .map_or("", |account| account.currency.as_str()),
            capabilities: self.capabilities(),
            rules: &self.rules,
        };
        let anchor = chart.get_rendered_price_rect().right_top() + egui::vec2(-8.0, 8.0);
        if let Some(request) = self.ticket.show(
            ui.ctx(),
            anchor,
            &market,
            &mut self.presets,
            &mut self.one_click,
        ) {
            self.place(request);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::services::Events;
    use std::sync::Mutex;

    /// A broker that records the orders it is sent.
    #[derive(Default)]
    struct Recorder {
        placed: Mutex<Vec<OrderRequest>>,
    }

    impl Broker for Recorder {
        fn connect(&self, events: Events<AccountEvent>) {
            let capabilities = Capabilities {
                market: true,
                ..Capabilities::default()
            };
            events.push(AccountEvent::Account(Account::new(
                "USD",
                0.0,
                capabilities,
            )));
        }
        fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
            self.placed.lock().unwrap().push(order);
            reply.send(Ok(OrderId::new("1")));
        }
        fn cancel(&self, _: OrderId, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    /// Guards the money path of the Buy and Sell buttons: without one-click trading they only
    /// open the ticket and send nothing; with it they send exactly one market order of the
    /// default quantity. A regression either trades without the user's confirmation or loses
    /// the one-click order.
    #[test]
    fn buy_and_sell_trade_at_once_only_with_one_click() {
        let broker = Arc::new(Recorder::default());
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        trading.quantity = 3.0;

        trading.trade("AAPL".into(), OrderSide::Sell);
        assert!(broker.placed.lock().unwrap().is_empty());
        assert!(trading.ticket.open);

        trading.one_click = true;
        trading.trade("AAPL".into(), OrderSide::Buy);
        let expected = OrderRequest::new("AAPL", OrderSide::Buy, 3.0, OrderType::Market);
        assert_eq!(*broker.placed.lock().unwrap(), [expected]);
    }
}
