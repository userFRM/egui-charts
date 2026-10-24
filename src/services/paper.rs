//! A simulated account behind the [`Broker`] service.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;

use super::{AccountEvent, Broker, Events};
use crate::data::{FeedError, Reply};
use crate::model::{
    Account, BalanceChange, Capabilities, Execution, JournalEntry, Order, OrderId, OrderRequest,
    OrderSide, OrderStatus, OrderType, Position, TimeInForce, TradingRules,
};

/// A paper account: a [`Broker`] that fills orders against the prices the host feeds it, so the
/// order ticket, the chart's order and position lines and the account panel work as they do with
/// a real account.
///
/// Feed it the last price of every symbol it trades with [`set_price`](Self::set_price), live or
/// replayed. Market orders fill at the last price; limit and stop orders fill at the first price
/// that reaches them, a limit at that price or better; a triggered stop-limit works on as a
/// limit order. An order's take profit and stop loss become exit orders once it fills, and when
/// one exit fills the other is cancelled. Each symbol has one net position with its average
/// price: a fill against it books the realised profit or loss of the closed quantity, and a fill
/// beyond it opens the other side at the fill price. A position holds its market value divided by
/// the leverage as margin, and a working order the same for its quantity at its price. Each fill
/// pays the commission rate times its value. Every change is pushed as an [`AccountEvent`], with
/// the finished orders, balance changes and a journal of every action as history.
///
/// ```
/// use std::sync::Arc;
/// use egui_charts::services::PaperAccount;
///
/// let paper = Arc::new(PaperAccount::new("USD", 100_000.0).leverage(10.0).commission(0.001));
/// paper.set_price("AAPL", 187.5);
/// // trading.set_service(paper.clone());
/// ```
pub struct PaperAccount {
    currency: String,
    initial: f64,
    leverage: f64,
    commission: f64,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    events: Option<Events<AccountEvent>>,
    next_id: u64,
    balance: f64,
    working: Vec<Order>,
    finished: Vec<Order>,
    /// Every bracket exit and its paired exit, if any.
    exits: HashMap<OrderId, Option<OrderId>>,
    positions: HashMap<String, Position>,
    last: HashMap<String, f64>,
    balance_history: Vec<BalanceChange>,
    journal: Vec<JournalEntry>,
}

/// The values a [`PaperAccount`] was created with, which [`State`] needs to value the account.
struct Terms<'a> {
    currency: &'a str,
    initial: f64,
    leverage: f64,
    commission: f64,
}

fn capabilities() -> Capabilities {
    Capabilities {
        market: true,
        limit: true,
        stop: true,
        stop_limit: true,
        brackets: true,
        modify: true,
    }
}

/// Describes an order in the journal, such as "buy 2 AAPL limit 185".
fn describe(request: &OrderRequest) -> String {
    let side = match request.side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let kind = match request.order_type {
        OrderType::Market => "at market".to_owned(),
        OrderType::Limit(limit) => format!("limit {limit}"),
        OrderType::Stop(stop) => format!("stop {stop}"),
        OrderType::StopLimit { stop, limit } => format!("stop {stop} limit {limit}"),
    };
    format!("{side} {} {} {kind}", request.quantity, request.symbol)
}

impl PaperAccount {
    /// An empty account holding `balance` in `currency`, without leverage or commission.
    pub fn new(currency: impl Into<String>, balance: f64) -> Self {
        let state = State {
            balance,
            ..State::default()
        };
        Self {
            currency: currency.into(),
            initial: balance,
            leverage: 1.0,
            commission: 0.0,
            state: Mutex::new(state),
        }
    }

    /// Sets the leverage: margin is the market value divided by it. Values below 1 count as 1.
    pub fn leverage(mut self, leverage: f64) -> Self {
        self.leverage = if leverage.is_finite() {
            leverage.max(1.0)
        } else {
            1.0
        };
        self
    }

    /// Sets the commission each fill pays, as a fraction of its value, such as 0.001 for 0.1%.
    pub fn commission(mut self, rate: f64) -> Self {
        self.commission = if rate.is_finite() { rate.max(0.0) } else { 0.0 };
        self
    }

    fn terms(&self) -> Terms<'_> {
        Terms {
            currency: &self.currency,
            initial: self.initial,
            leverage: self.leverage,
            commission: self.commission,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes `price` as the last price of `symbol`: fills the working orders it reaches and
    /// revalues the position. Non-finite prices are ignored.
    pub fn set_price(&self, symbol: &str, price: f64) {
        let terms = self.terms();
        let mut state = self.lock();
        if price.is_finite() && state.last.insert(symbol.to_owned(), price) != Some(price) {
            state.reach(&terms, symbol, price);
            if let Some(mut position) = state.positions.remove(symbol) {
                state.value(&terms, &mut position);
                state.positions.insert(symbol.to_owned(), position.clone());
                state.push(AccountEvent::Position(position));
                state.push(state.account(&terms));
            }
        }
    }

    /// Returns the account to its starting balance without orders, positions or history. The
    /// terminal shows the fresh account once it connects again, such as through
    /// [`Trading::set_service`](crate::ui::trading::Trading::set_service).
    pub fn reset(&self) {
        let mut state = self.lock();
        let last = std::mem::take(&mut state.last);
        *state = State {
            balance: self.initial,
            last,
            ..State::default()
        };
    }
}

impl State {
    fn push(&self, event: AccountEvent) {
        if let Some(events) = &self.events {
            events.push(event);
        }
    }

    fn log(&mut self, text: String) {
        let entry = JournalEntry::new(Utc::now(), text);
        self.push(AccountEvent::Journal(entry.clone()));
        self.journal.push(entry);
    }

    fn book(&mut self, amount: f64, action: String) {
        if amount == 0.0 {
            return;
        }
        self.balance += amount;
        let change = BalanceChange::new(Utc::now(), self.balance, amount, action);
        self.push(AccountEvent::Balance(change.clone()));
        self.balance_history.push(change);
    }

    /// Values `position` at the symbol's last price.
    fn value(&self, terms: &Terms, position: &mut Position) {
        let last = self
            .last
            .get(&position.symbol)
            .copied()
            .unwrap_or(position.average_price);
        let size = position.quantity.abs();
        position.last_price = Some(last);
        position.unrealized_pnl = Some((last - position.average_price) * position.quantity);
        position.trade_value = Some(size * position.average_price);
        position.market_value = Some(size * last);
        position.leverage = Some(terms.leverage);
        position.margin = Some(size * last / terms.leverage);
    }

    fn account(&self, terms: &Terms) -> AccountEvent {
        let mut account = Account::new(terms.currency, self.balance, capabilities());
        let open: f64 = self
            .positions
            .values()
            .filter_map(|p| p.unrealized_pnl)
            .sum();
        let margin: f64 = self.positions.values().filter_map(|p| p.margin).sum();
        let orders_margin: f64 = self
            .working
            .iter()
            .filter(|order| !self.exits.contains_key(&order.id))
            .map(|order| {
                let price = match order.request.order_type {
                    OrderType::Limit(price) | OrderType::Stop(price) => price,
                    OrderType::StopLimit { limit, .. } => limit,
                    _ => self.last.get(&order.request.symbol).copied().unwrap_or(0.0),
                };
                order.request.quantity * price / terms.leverage
            })
            .sum();
        let equity = self.balance + open;
        account.equity = Some(equity);
        account.realized_pnl = Some(self.balance - terms.initial);
        account.margin = Some(margin);
        account.orders_margin = Some(orders_margin);
        account.available_funds = Some(equity - margin - orders_margin);
        AccountEvent::Account(account)
    }

    fn id(&mut self) -> OrderId {
        self.next_id += 1;
        OrderId(self.next_id.to_string())
    }

    fn finish(&mut self, mut order: Order, status: OrderStatus) {
        order.status = status;
        order.closed = Some(Utc::now());
        self.push(AccountEvent::Order(order.clone()));
        self.finished.push(order);
    }

    /// Fills `order` in full at `price`: updates the net position, books the realised profit
    /// and the commission, and turns brackets into exit orders.
    fn fill(&mut self, terms: &Terms, mut order: Order, price: f64) {
        let request = order.request.clone();
        let signed = match request.side {
            OrderSide::Buy => request.quantity,
            OrderSide::Sell => -request.quantity,
        };
        let mut position = self
            .positions
            .remove(&request.symbol)
            .unwrap_or_else(|| Position::new(&request.symbol, 0.0, price));
        let held = position.quantity;
        let mut realized = 0.0;
        if held * signed >= 0.0 {
            position.average_price =
                (held * position.average_price + signed * price) / (held + signed);
        } else {
            let closed = signed.abs().min(held.abs());
            realized = closed * (price - position.average_price) * held.signum();
            if signed.abs() > held.abs() {
                position.average_price = price;
            }
        }
        position.quantity = held + signed;
        self.value(terms, &mut position);
        if position.quantity != 0.0 {
            self.positions
                .insert(request.symbol.clone(), position.clone());
        }

        order.filled = request.quantity;
        order.fill_price = Some(price);
        let execution = Execution::new(
            order.id.clone(),
            Utc::now(),
            request.side,
            price,
            request.quantity,
        );
        let id = order.id.clone();
        self.finish(order, OrderStatus::Filled);
        self.push(AccountEvent::Execution(execution));
        self.push(AccountEvent::Position(position.clone()));
        self.log(format!(
            "Order {id} filled: {} at {price}",
            describe(&request)
        ));
        self.book(
            realized,
            format!(
                "Profit or loss of {} {} closed by order {id}",
                (held.abs()).min(request.quantity),
                request.symbol
            ),
        );
        self.book(
            -request.quantity * price * terms.commission,
            format!("Commission for order {id}"),
        );
        if held != 0.0 && position.quantity == 0.0 {
            self.log(format!("Position {} closed", request.symbol));
        }
        if let Some(Some(other)) = self.exits.remove(&id) {
            self.cancel_working(&other);
        }
        // Exits may only reduce the remaining position, including after a manual close or reverse.
        for mut exit in self.working.clone() {
            if exit.request.symbol != request.symbol || !self.exits.contains_key(&exit.id) {
                continue;
            }
            let buy = exit.request.side == OrderSide::Buy;
            if position.quantity == 0.0 || buy == (position.quantity > 0.0) {
                self.cancel_working(&exit.id);
            } else if exit.request.quantity > position.quantity.abs() {
                exit.request.quantity = position.quantity.abs();
                if let Some(working) = self
                    .working
                    .iter_mut()
                    .find(|working| working.id == exit.id)
                {
                    *working = exit.clone();
                }
                self.push(AccountEvent::Order(exit));
            }
        }

        let exit = |price, order_type: fn(f64) -> OrderType| {
            let side = match request.side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            };
            let mut exit =
                OrderRequest::new(&request.symbol, side, request.quantity, order_type(price));
            exit.time_in_force = TimeInForce::GoodTillCancelled;
            exit
        };
        let exits: Vec<_> = [
            request
                .take_profit
                .map(|price| exit(price, OrderType::Limit)),
            request.stop_loss.map(|price| exit(price, OrderType::Stop)),
        ]
        .into_iter()
        .flatten()
        .map(|exit| {
            let id = self.id();
            let mut order = Order::new(id, exit, OrderStatus::Working);
            order.placed = Some(Utc::now());
            order
        })
        .collect();
        if let [take_profit, stop_loss] = exits.as_slice() {
            self.exits
                .insert(take_profit.id.clone(), Some(stop_loss.id.clone()));
            self.exits
                .insert(stop_loss.id.clone(), Some(take_profit.id.clone()));
        }
        for exit in exits {
            self.exits.entry(exit.id.clone()).or_insert(None);
            self.push(AccountEvent::Order(exit.clone()));
            self.log(format!(
                "Exit order {} placed: {}",
                exit.id,
                describe(&exit.request)
            ));
            self.working.push(exit);
        }
        self.push(self.account(terms));
    }

    fn cancel_working(&mut self, id: &OrderId) -> bool {
        let Some(index) = self.working.iter().position(|order| &order.id == id) else {
            return false;
        };
        let order = self.working.remove(index);
        if let Some(Some(other)) = self.exits.remove(id)
            && let Some(pair) = self.exits.get_mut(&other)
        {
            *pair = None;
        }
        self.log(format!(
            "Order {id} cancelled: {}",
            describe(&order.request)
        ));
        self.finish(order, OrderStatus::Cancelled);
        true
    }

    /// Fills the working orders of `symbol` that `price` reached.
    fn reach(&mut self, terms: &Terms, symbol: &str, price: f64) {
        let mut index = 0;
        while index < self.working.len() {
            let order = &mut self.working[index];
            let buy = order.request.side == OrderSide::Buy;
            let fill = match order.request.order_type {
                _ if order.request.symbol != symbol => None,
                OrderType::Limit(limit) if (buy && price <= limit) || (!buy && price >= limit) => {
                    Some(price)
                }
                OrderType::Stop(stop) if (buy && price >= stop) || (!buy && price <= stop) => {
                    Some(price)
                }
                OrderType::StopLimit { stop, limit }
                    if (buy && price >= stop) || (!buy && price <= stop) =>
                {
                    // Triggered: it works on as a limit order, which may fill at once.
                    order.request.order_type = OrderType::Limit(limit);
                    let order = order.clone();
                    self.push(AccountEvent::Order(order));
                    continue;
                }
                _ => None,
            };
            if let Some(fill) = fill {
                let order = self.working.remove(index);
                self.fill(terms, order, fill);
                index = 0;
            } else {
                index += 1;
            }
        }
    }
}

impl Broker for PaperAccount {
    fn connect(&self, events: Events<AccountEvent>) {
        let terms = self.terms();
        let mut state = self.lock();
        state.events = Some(events);
        state.push(state.account(&terms));
        for order in state.working.iter().chain(&state.finished) {
            state.push(AccountEvent::Order(order.clone()));
        }
        for position in state.positions.values() {
            state.push(AccountEvent::Position(position.clone()));
        }
        for change in &state.balance_history {
            state.push(AccountEvent::Balance(change.clone()));
        }
        for entry in &state.journal {
            state.push(AccountEvent::Journal(entry.clone()));
        }
    }

    fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
        let terms = self.terms();
        let mut state = self.lock();
        let Some(&last) = state.last.get(&order.symbol) else {
            return reply.send(Err(FeedError::SymbolNotFound(order.symbol)));
        };
        let id = state.id();
        reply.send(Ok(id.clone()));
        state.log(format!("Order {id} placed: {}", describe(&order)));
        let mut order = Order::new(id, order, OrderStatus::Working);
        order.placed = Some(Utc::now());
        if order.request.order_type == OrderType::Market {
            state.fill(&terms, order, last);
        } else {
            let symbol = order.request.symbol.clone();
            state.push(AccountEvent::Order(order.clone()));
            state.working.push(order);
            // A marketable limit or a reached stop fills at once.
            state.reach(&terms, &symbol, last);
            state.push(state.account(&terms));
        }
    }

    fn cancel(&self, id: OrderId, reply: Reply<()>) {
        let terms = self.terms();
        let mut state = self.lock();
        if state.cancel_working(&id) {
            reply.send(Ok(()));
            state.push(state.account(&terms));
        } else {
            reply.send(Err(FeedError::Other(format!(
                "order {id} is no longer working"
            ))));
        }
    }

    fn modify(&self, id: OrderId, order: OrderRequest, reply: Reply<()>) {
        let terms = self.terms();
        let mut state = self.lock();
        let Some(working) = state.working.iter_mut().find(|working| working.id == id) else {
            return reply.send(Err(FeedError::Other(format!(
                "order {id} is no longer working"
            ))));
        };
        working.request = order;
        let (working, symbol) = (working.clone(), working.request.symbol.clone());
        reply.send(Ok(()));
        state.push(AccountEvent::Order(working.clone()));
        state.log(format!(
            "Order {id} changed: {}",
            describe(&working.request)
        ));
        // A limit moved through the market or a stop moved past it fills at once.
        if let Some(&last) = state.last.get(&symbol) {
            state.reach(&terms, &symbol, last);
        }
        state.push(state.account(&terms));
    }

    fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
        let mut rules = TradingRules::new(symbol);
        rules.time_in_force = vec![TimeInForce::Day, TimeInForce::GoodTillCancelled];
        reply.send(Ok(rules));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_figures_follow_fills_commission_and_leverage() {
        let paper = PaperAccount::new("USD", 10_000.0)
            .leverage(10.0)
            .commission(0.001);
        let inbox = std::sync::Arc::new(crate::data::inbox::Inbox::default());
        let place = |side, quantity, order_type| {
            let order = OrderRequest::new("SYM", side, quantity, order_type);
            paper.place(order, inbox.reply(0));
        };
        paper.set_price("SYM", 100.0);
        place(OrderSide::Buy, 2.0, OrderType::Market);
        paper.set_price("SYM", 110.0);
        place(OrderSide::Sell, 1.0, OrderType::Market);
        place(OrderSide::Buy, 1.0, OrderType::Limit(90.0));

        let state = paper.lock();
        let AccountEvent::Account(account) = state.account(&paper.terms()) else {
            unreachable!()
        };
        let position = &state.positions["SYM"];
        // Commissions of 0.20 and 0.11, and 10 earned on the unit sold 10 above its price.
        let balance = 10_000.0 - 0.2 + 10.0 - 0.11;
        let near = |a: Option<f64>, b: f64| assert!((a.unwrap() - b).abs() < 1e-9, "{a:?} != {b}");
        near(Some(account.balance), balance);
        near(account.realized_pnl, balance - 10_000.0);
        near(position.unrealized_pnl, 10.0);
        near(position.trade_value, 100.0);
        near(position.market_value, 110.0);
        near(position.margin, 11.0);
        near(account.equity, balance + 10.0);
        near(account.orders_margin, 9.0);
        near(account.available_funds, balance + 10.0 - 11.0 - 9.0);
        assert_eq!(state.finished.len(), 2);
        assert_eq!(state.balance_history.len(), 3);
    }

    #[test]
    fn bracket_exits_only_reduce_the_remaining_position() {
        let paper = PaperAccount::new("USD", 0.0);
        let terms = paper.terms();
        for stop_loss in [None, Some(95.0)] {
            for closed in [1.0, 2.0, 4.0] {
                let mut state = State::default();
                let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market);
                entry.take_profit = Some(105.0);
                entry.stop_loss = stop_loss;
                state.fill(
                    &terms,
                    Order::new(OrderId::new("entry"), entry, OrderStatus::Working),
                    100.0,
                );
                let close = OrderRequest::new("SYM", OrderSide::Sell, closed, OrderType::Market);
                state.fill(
                    &terms,
                    Order::new(OrderId::new("close"), close, OrderStatus::Working),
                    100.0,
                );
                if closed == 1.0 {
                    assert!(
                        state
                            .working
                            .iter()
                            .all(|order| order.request.quantity == 1.0)
                    );
                } else {
                    assert!(state.working.is_empty());
                }
                state.reach(&terms, "SYM", 106.0);
                state.reach(&terms, "SYM", 94.0);
                assert_eq!(
                    state.positions.get("SYM").map_or(0.0, |p| p.quantity),
                    if closed == 4.0 { -2.0 } else { 0.0 }
                );
                if closed == 1.0 {
                    assert_eq!(
                        state.balance, 6.0,
                        "a marketable limit fills at the better price"
                    );
                }
            }
        }
    }
}
