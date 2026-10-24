//! Trading from the chart through the host's [`Broker`]: the order ticket and the Buy and Sell
//! buttons. The broker holds the account; this module shows what it pushes and sends the orders
//! the user confirms.

mod ticket;

pub use ticket::{Bracket, BracketMode, OrderPreset};

use std::sync::Arc;

use crate::chart::selection::SeriesId;
use crate::chart::{TradingLineAction, TradingLineOptions, render_trading_lines};
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{
    Account, Capabilities, Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType,
    Position, TradingRules,
};
use crate::services::{AccountEvent, Broker, Listener};
use crate::ui_kit::modal::dialogs::ConfirmDialog;
use crate::ui_kit::notifications::Toasts;
use crate::widget::Chart;
pub(crate) use ticket::to_step;
use ticket::{Market, OrderTicket, order_quantity};

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
    rules_key: u64,
    rules_ready: bool,
    ticket: OrderTicket,
    /// A change the user asked for on a line, waiting for confirmation.
    pending: Option<TradingLineAction>,
    /// Decimals of the chart's prices, for notifications.
    decimals: usize,
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
            rules_key: 1,
            rules_ready: false,
            ticket: OrderTicket::default(),
            pending: None,
            decimals: 2,
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
        self.ticket.open = false;
        self.one_click = false;
        self.connect();
    }

    fn connect(&mut self) {
        self.pending = None;
        self.rules = TradingRules::new("");
        self.rules_ready = false;
        self.account = None;
        self.orders.clear();
        self.positions.clear();
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

    /// Handles the Buy or Sell button: a market order of [`quantity`](Self::quantity) at once
    /// with [`one_click`](Self::one_click), otherwise the ticket for one.
    pub fn trade(&mut self, symbol: String, side: OrderSide) {
        self.load_rules(&symbol);
        let mut request = OrderRequest::new(symbol, side, self.quantity, OrderType::Market);
        if self.one_click
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
            self.rules = TradingRules::new(symbol);
            self.rules_ready = false;
            self.rules_key += 2;
            if let Some(service) = &self.service {
                service.rules(symbol, self.inbox.reply(self.rules_key));
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
            }
            if key == self.rules_key {
                if let Ok(answer) = answer.downcast::<Result<TradingRules, FeedError>>() {
                    match *answer {
                        Ok(rules) if rules.symbol == self.rules.symbol && valid(&rules) => {
                            self.rules = rules;
                            self.rules_ready = true;
                        }
                        Err(FeedError::NotSupported) => self.rules_ready = true,
                        Err(error) => toasts.error(format!("Trading rules unavailable: {error}")),
                        Ok(_) => toasts.error("The broker returned invalid trading rules"),
                    }
                }
                continue;
            }
            if key != 0 {
                continue;
            }
            let error = match answer.downcast::<Result<OrderId, FeedError>>() {
                Ok(placed) => placed.err(),
                Err(answer) => answer
                    .downcast::<Result<(), FeedError>>()
                    .ok()
                    .and_then(|done| done.err()),
            };
            if let Some(error) = error {
                toasts.error(format!("Order refused: {error}"));
            }
        }
    }

    fn apply(&mut self, event: AccountEvent, toasts: &mut Toasts) {
        let finite = |value: Option<f64>| value.is_none_or(f64::is_finite);
        let positive = |value: f64| value.is_finite() && value > 0.0;
        let valid = match &event {
            AccountEvent::Account(account) => account.balance.is_finite() && finite(account.equity),
            AccountEvent::Order(order) => {
                let request = &order.request;
                positive(request.quantity)
                    && order.filled.is_finite()
                    && (0.0..=request.quantity).contains(&order.filled)
                    && finite(request.take_profit)
                    && finite(request.stop_loss)
                    && match request.order_type {
                        OrderType::Market => true,
                        OrderType::Limit(price) | OrderType::Stop(price) => price.is_finite(),
                        OrderType::StopLimit { stop, limit } => {
                            stop.is_finite() && limit.is_finite()
                        }
                    }
            }
            AccountEvent::Position(position) => {
                position.quantity.is_finite()
                    && position.average_price.is_finite()
                    && finite(position.unrealized_pnl)
            }
            AccountEvent::Execution(fill) => positive(fill.quantity) && fill.price.is_finite(),
            _ => true,
        };
        if !valid {
            return;
        }
        match event {
            AccountEvent::Account(account) => {
                if matches!(self.pending, Some(TradingLineAction::Modify(_, _)))
                    && !account.capabilities.modify
                    || matches!(
                        self.pending,
                        Some(TradingLineAction::Close(_) | TradingLineAction::Reverse(_))
                    ) && !account.capabilities.market
                {
                    self.pending = None;
                }
                self.account = Some(account);
            }
            AccountEvent::Order(order) => {
                if matches!(&self.pending,
                    Some(TradingLineAction::Cancel(old) | TradingLineAction::Modify(old, _))
                    if old.id == order.id && old != &order)
                {
                    self.pending = None;
                }
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
                if matches!(&self.pending,
                    Some(TradingLineAction::Close(old) | TradingLineAction::Reverse(old))
                    if old.symbol == position.symbol
                        && (old.quantity != position.quantity || old.average_price != position.average_price))
                {
                    self.pending = None;
                }
                self.positions.retain(|open| open.symbol != position.symbol);
                if position.quantity != 0.0 {
                    self.positions.push(position);
                }
            }
            AccountEvent::Execution(fill) => toasts.info(format!(
                "Filled: {} {} at {:.*}",
                fill.side.label(),
                fill.quantity,
                self.decimals,
                fill.price
            )),
            _ => {}
        }
    }

    /// Draws the chart symbol's working orders, brackets and positions as lines on `chart`, and
    /// the order ticket. Sends the order the user submits, and asks the user to confirm a change
    /// made on a line before sending it.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        let symbol = chart.symbol().to_owned();
        let symbol = symbol.as_str();
        let instrument = chart
            .instrument()
            .filter(|instrument| instrument.symbol == symbol);
        let decimals = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map_or(2, usize::from);
        self.decimals = decimals;
        let tick = instrument
            .and_then(|instrument| instrument.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0)
            .unwrap_or(10f64.powi(-(decimals as i32)));
        let last = chart.bars(SeriesId::MAIN).last().map(|bar| bar.close);
        // Prices and ticks belong to the chart's current symbol.
        if self.ticket.symbol != symbol {
            self.ticket.open = false;
        }
        self.load_rules(symbol);
        let currency = self
            .account
            .as_ref()
            .map_or("", |account| account.currency.as_str());
        let capabilities = self.capabilities();

        if self.service.is_some() {
            let mapping = chart.get_chart_mapping();
            let options = TradingLineOptions {
                modify: capabilities.modify,
                close: capabilities.market,
                last_price: last,
                point_value: if self.rules.symbol == symbol {
                    self.rules.point_value
                } else {
                    1.0
                },
                currency,
                tick_size: tick,
            };
            let lines = {
                let _theme = chart
                    .theme_override
                    .as_ref()
                    .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
                render_trading_lines(
                    ui,
                    &mapping,
                    self.orders.iter().filter(|o| o.request.symbol == symbol),
                    self.positions.iter().filter(|p| p.symbol == symbol),
                    &options,
                )
            };
            chart.trading_tags = lines.tags;
            self.pending = self.pending.take().or(lines.action);
        }
        if let Some(action) = &self.pending {
            let money = |value: f64| format!("{value:.decimals$}");
            let (title, message, confirm) = describe(action, &money);
            let mut open = true;
            let result = ConfirmDialog::new("trading_confirm", title, message)
                .confirm_text(confirm)
                .cancel_text("Back")
                .show(ui.ctx(), &mut open);
            if result.is_confirmed() {
                let action = self.pending.take();
                self.execute(action);
            } else if result.is_cancelled() || !open {
                self.pending = None;
            }
        }

        if !self.ticket.open {
            return;
        }
        let market = Market {
            last: (self.ticket.symbol == symbol).then_some(last).flatten(),
            tick,
            decimals,
            currency,
            capabilities,
            rules: &self.rules,
            rules_ready: self.rules_ready,
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

    /// Sends a confirmed change to the broker. Closing and reversing a position are market
    /// orders on the other side, for the position's size and for twice it.
    fn execute(&self, action: Option<TradingLineAction>) {
        let Some(service) = &self.service else {
            return;
        };
        let reverse = matches!(action, Some(TradingLineAction::Reverse(_)));
        match action {
            Some(TradingLineAction::Cancel(order)) => service.cancel(order.id, self.inbox.reply(0)),
            Some(TradingLineAction::Modify(order, request)) => {
                service.modify(order.id, request, self.inbox.reply(0));
            }
            Some(TradingLineAction::Close(position) | TradingLineAction::Reverse(position))
                if position.quantity != 0.0 =>
            {
                let side = if position.quantity > 0.0 {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                let quantity = position.quantity.abs() * if reverse { 2.0 } else { 1.0 };
                let mut order =
                    OrderRequest::new(position.symbol, side, quantity, OrderType::Market);
                if self.rules_ready && self.rules.symbol == order.symbol {
                    order.time_in_force = self.rules.time_in_force[0];
                }
                service.place(order, self.inbox.reply(0));
            }
            _ => {}
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
        && !rules.time_in_force.is_empty()
}

/// The title, question and confirm button text of the confirmation for `action`.
fn describe(
    action: &TradingLineAction,
    price: &dyn Fn(f64) -> String,
) -> (&'static str, String, &'static str) {
    let order = |request: &OrderRequest| {
        let mut text = format!(
            "{} {} {}",
            request.side.label(),
            request.quantity,
            request.symbol
        );
        match request.order_type {
            OrderType::Limit(limit) => text += &format!(" limit {}", price(limit)),
            OrderType::Stop(stop) => text += &format!(" stop {}", price(stop)),
            OrderType::StopLimit { stop, limit } => {
                text += &format!(" stop {} limit {}", price(stop), price(limit));
            }
            _ => text += " at market",
        }
        if let Some(take_profit) = request.take_profit {
            text += &format!(", take profit {}", price(take_profit));
        }
        if let Some(stop_loss) = request.stop_loss {
            text += &format!(", stop loss {}", price(stop_loss));
        }
        text
    };
    let position = |position: &Position| {
        format!(
            "the {} position of {} {}",
            if position.quantity > 0.0 {
                "long"
            } else {
                "short"
            },
            position.quantity.abs(),
            position.symbol
        )
    };
    match action {
        TradingLineAction::Cancel(working) => (
            "Cancel order",
            format!("Cancel the order to {}?", order(&working.request)),
            "Cancel order",
        ),
        TradingLineAction::Modify(_, changed) => (
            "Modify order",
            format!("Change the order to {}?", order(changed)),
            "Modify order",
        ),
        TradingLineAction::Close(open) => (
            "Close position",
            format!("Close {} at market?", position(open)),
            "Close position",
        ),
        TradingLineAction::Reverse(open) => (
            "Reverse position",
            format!("Reverse {} at market?", position(open)),
            "Reverse position",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::services::Events;
    use std::sync::Mutex;

    /// A broker that pushes `state` on connecting and records the calls it gets.
    #[derive(Default)]
    struct Recorder {
        state: Vec<AccountEvent>,
        placed: Mutex<Vec<OrderRequest>>,
        cancelled: Mutex<Vec<OrderId>>,
        modified: Mutex<Vec<(OrderId, OrderRequest)>>,
        rules: Mutex<Vec<(String, Reply<TradingRules>)>>,
    }

    impl Broker for Recorder {
        fn connect(&self, events: Events<AccountEvent>) {
            let capabilities = Capabilities {
                market: true,
                limit: true,
                modify: true,
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

    /// Line gestures wait for the actual confirmation button and send one broker call.
    #[test]
    fn line_changes_wait_for_confirmation_and_send_one_call() {
        use crate::theme::{Theme, apply_to_egui};
        for theme in [Theme::light(), Theme::dark()] {
            for width in [800.0, 320.0] {
                for operation in [
                    "Cancel order",
                    "Modify order",
                    "Close position",
                    "Reverse position",
                ] {
                    let mut request =
                        OrderRequest::new("SYM", OrderSide::Sell, 1.0, OrderType::Limit(105.0));
                    request.take_profit = Some(102.0);
                    request.stop_loss = Some(108.0);
                    let order = Order::new(OrderId::new("7"), request, OrderStatus::Working);
                    let position = Position::new("SYM", -2.0, 100.0);
                    let broker = Arc::new(Recorder {
                        state: vec![
                            AccountEvent::Order(order.clone()),
                            AccountEvent::Position(position.clone()),
                        ],
                        ..Recorder::default()
                    });
                    let mut trading = Trading::default();
                    trading.set_service(broker.clone());
                    let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                    chart.symbol = "SYM".into();
                    let ctx = egui::Context::default();
                    apply_to_egui(
                        &ctx,
                        &if theme == Theme::light() {
                            Theme::dark()
                        } else {
                            Theme::light()
                        },
                    );
                    chart.set_theme_override(Some(theme.clone()));
                    let frame = |trading: &mut Trading, chart: &mut Chart, events| {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 600.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                trading.poll(ui.ctx(), &mut Toasts::new());
                                chart.show(ui);
                                trading.show(ui, chart);
                            },
                        );
                        let confirm = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) if text.galley.text() == operation => {
                                    Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                                }
                                _ => None,
                            })
                            .next_back();
                        output.drop_without_applying_deltas();
                        confirm
                    };
                    let click = |pos, pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    frame(&mut trading, &mut chart, vec![]);
                    frame(&mut trading, &mut chart, vec![]);
                    assert!(
                        chart
                            .trading_tags
                            .iter()
                            .any(|(_, color)| *color == theme.ui.error)
                    );
                    let mapping = chart.get_chart_mapping();
                    let y = mapping.price_to_y(if operation.ends_with("order") {
                        105.0
                    } else {
                        100.0
                    });
                    let x = match operation {
                        "Modify order" => mapping.rect.center().x,
                        "Reverse position" => mapping.rect.right() - 35.0,
                        _ => mapping.rect.right() - 17.0,
                    };
                    let start = egui::pos2(x, y);
                    frame(
                        &mut trading,
                        &mut chart,
                        vec![egui::Event::PointerMoved(start)],
                    );
                    frame(&mut trading, &mut chart, vec![click(start, true)]);
                    let end = if operation == "Modify order" {
                        egui::pos2(x, mapping.price_to_y(103.0))
                    } else {
                        start
                    };
                    frame(
                        &mut trading,
                        &mut chart,
                        vec![egui::Event::PointerMoved(end)],
                    );
                    frame(&mut trading, &mut chart, vec![click(end, false)]);
                    assert!(trading.pending.is_some(), "{operation} at {width}");
                    assert!(broker.placed.lock().unwrap().is_empty());
                    assert!(broker.cancelled.lock().unwrap().is_empty());
                    assert!(broker.modified.lock().unwrap().is_empty());
                    frame(&mut trading, &mut chart, vec![]);
                    let confirm = frame(&mut trading, &mut chart, vec![]).expect(operation);
                    assert!(
                        confirm.x >= 0.0 && confirm.x <= width,
                        "confirmation stays reachable"
                    );
                    frame(
                        &mut trading,
                        &mut chart,
                        vec![egui::Event::PointerMoved(confirm)],
                    );
                    frame(&mut trading, &mut chart, vec![click(confirm, true)]);
                    frame(&mut trading, &mut chart, vec![click(confirm, false)]);
                    frame(&mut trading, &mut chart, vec![]);
                    assert!(trading.pending.is_none());
                    match operation {
                        "Cancel order" => assert_eq!(*broker.cancelled.lock().unwrap(), [order.id]),
                        "Modify order" => {
                            let changed = broker.modified.lock().unwrap();
                            assert_eq!(changed.len(), 1);
                            assert_eq!(changed[0].0, order.id);
                            assert_eq!(changed[0].1.order_type, OrderType::Limit(103.0));
                            assert_eq!(changed[0].1.take_profit, Some(100.0));
                            assert_eq!(changed[0].1.stop_loss, Some(106.0));
                        }
                        _ => {
                            let quantity = if operation == "Close position" {
                                2.0
                            } else {
                                4.0
                            };
                            assert_eq!(
                                *broker.placed.lock().unwrap(),
                                [OrderRequest::new(
                                    "SYM",
                                    OrderSide::Buy,
                                    quantity,
                                    OrderType::Market
                                )]
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_broker_numbers_do_not_replace_account_state() {
        let mut trading = Trading::default();
        let mut toasts = Toasts::new();
        let account = Account::new("USD", 1000.0, Capabilities::default());
        let order = Order::new(
            OrderId::new("7"),
            OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(100.0)),
            OrderStatus::Working,
        );
        let position = Position::new("SYM", 2.0, 100.0);
        for event in [
            AccountEvent::Account(account.clone()),
            AccountEvent::Order(order.clone()),
            AccountEvent::Position(position.clone()),
        ] {
            trading.apply(event, &mut toasts);
        }
        for bad in [f64::NAN, f64::INFINITY] {
            let mut invalid_order = order.clone();
            invalid_order.request.stop_loss = Some(bad);
            for event in [
                AccountEvent::Account(Account::new("USD", bad, Capabilities::default())),
                AccountEvent::Order(invalid_order),
                AccountEvent::Position(Position::new("SYM", bad, 100.0)),
            ] {
                trading.apply(event, &mut toasts);
                assert_eq!(trading.account, Some(account.clone()));
                assert_eq!(trading.orders.as_slice(), std::slice::from_ref(&order));
                assert_eq!(
                    trading.positions.as_slice(),
                    std::slice::from_ref(&position)
                );
            }
        }
    }

    #[test]
    fn account_changes_discard_stale_confirmations() {
        let mut trading = Trading::default();
        let position = Position::new("SYM", 2.0, 100.0);
        let order = Order::new(
            OrderId::new("7"),
            OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(100.0)),
            OrderStatus::Working,
        );
        for action in [
            TradingLineAction::Close(position.clone()),
            TradingLineAction::Reverse(position.clone()),
            TradingLineAction::Cancel(order.clone()),
            TradingLineAction::Modify(order.clone(), order.request.clone()),
        ] {
            trading.pending = Some(action);
            let mut marked = position.clone();
            marked.unrealized_pnl = Some(5.0);
            trading.apply(AccountEvent::Position(marked), &mut Toasts::new());
            assert!(
                trading.pending.is_some(),
                "a price tick preserves confirmation"
            );
            trading.apply(
                AccountEvent::Position(Position::new("SYM", 0.0, 100.0)),
                &mut Toasts::new(),
            );
            let mut filled = order.clone();
            filled.filled = 1.0;
            trading.apply(AccountEvent::Order(filled), &mut Toasts::new());
            assert!(
                trading.pending.is_none(),
                "changed exposure invalidates confirmation"
            );
        }
    }
}
