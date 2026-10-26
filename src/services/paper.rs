//! A simulated account behind the [`Broker`] service.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AccountEvent, Broker, Events};
use crate::data::{FeedError, Reply};
use crate::model::{
    Account, BalanceChange, Capabilities, Execution, JournalEntry, Order, OrderId, OrderRequest,
    OrderSide, OrderStatus, OrderType, Position, PositionBracket, TimeInForce, TradingRules,
};

/// A paper account: a [`Broker`] that fills orders against the prices the host feeds it, so the
/// order ticket, the chart's order and position lines and the account panel work as they do with
/// a real account.
///
/// Feed it the last price of every symbol it trades with [`set_price`](Self::set_price), live or
/// replayed. During replay, call [`set_replay_time`](Self::set_replay_time) before each price so
/// fills and history use the source bar's time. Only good-till-cancelled orders are supported;
/// the account has no trading-session calendar. Market orders fill at the last price; limit and
/// stop orders fill at the first price that reaches them, a limit at that price or better;
/// a triggered stop-limit works on as a limit order. An order's take profit and stop loss become
/// exit orders once it fills, and when one exit fills the other is cancelled. Each symbol has
/// one net position with its average
/// price: a fill against it books the realised profit or loss of the closed quantity, and a fill
/// beyond it opens the other side at the fill price. A position holds its market value divided by
/// the leverage as margin, and a working order the same for its quantity at its price. Each fill
/// pays the commission rate times its value. Placement and modification reserve buying power
/// across all symbols, including working orders and their commissions. Market orders that
/// reduce exposure remain possible when funds are insufficient. Position exits only reduce the
/// remaining position.
/// Every change is pushed as an [`AccountEvent`], with
/// the most recent 1,000 finished orders, executions, balance changes and journal entries.
/// The history bound keeps a reconnect snapshot below the event stream's burst limit for
/// ordinary interactive accounts.
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
    replay_time: Option<DateTime<Utc>>,
    next_id: u64,
    balance: f64,
    working: Vec<Order>,
    finished: Vec<Order>,
    executions: Vec<Execution>,
    /// Every bracket exit and its paired exit, if any.
    exits: HashMap<OrderId, Option<OrderId>>,
    positions: HashMap<String, Position>,
    last: HashMap<String, f64>,
    balance_history: Vec<BalanceChange>,
    journal: Vec<JournalEntry>,
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
    /// Panics if the starting balance is not finite.
    pub fn new(currency: impl Into<String>, balance: f64) -> Self {
        assert!(balance.is_finite(), "starting balance must be finite");
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

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes `price` as the last price of `symbol`: fills the working orders it reaches and
    /// revalues the position. Non-finite prices are ignored.
    pub fn set_price(&self, symbol: &str, price: f64) {
        let mut state = self.lock();
        if price.is_finite() && state.last.insert(symbol.to_owned(), price) != Some(price) {
            state.reach(self, symbol, price);
            if let Some(mut position) = state.positions.remove(symbol) {
                state.value(self, &mut position);
                state.positions.insert(symbol.to_owned(), position.clone());
                state.push(AccountEvent::Position(position));
                state.push(state.account(self));
            }
        }
    }

    /// Uses `time` for orders, executions, balance changes and journal entries during replay.
    /// Set it to the revealed source bar's time before feeding its price. `None` restores the
    /// live UTC clock.
    pub fn set_replay_time(&self, time: Option<DateTime<Utc>>) {
        self.lock().replay_time = time;
    }

    /// Returns the account to its starting balance without orders, positions or history,
    /// retaining the host's prices and replay clock. The terminal shows the fresh account once
    /// it connects again, such as through
    /// [`Trading::set_service`](crate::ui::trading::Trading::set_service).
    pub fn reset(&self) {
        let mut state = self.lock();
        let last = std::mem::take(&mut state.last);
        *state = State {
            balance: self.initial,
            replay_time: state.replay_time,
            last,
            ..State::default()
        };
    }
}

impl State {
    fn now(&self) -> DateTime<Utc> {
        self.replay_time.unwrap_or_else(Utc::now)
    }

    fn push(&self, event: AccountEvent) {
        if let Some(events) = &self.events {
            events.push(event);
        }
    }

    fn log(&mut self, text: String) {
        let entry = JournalEntry::new(self.now(), text);
        self.push(AccountEvent::Journal(entry.clone()));
        if self.journal.len() == 1_000 {
            self.journal.remove(0);
        }
        self.journal.push(entry);
    }

    fn book(&mut self, amount: f64, action: String) {
        if amount == 0.0 {
            return;
        }
        self.balance += amount;
        let change = BalanceChange::new(self.now(), self.balance, amount, action);
        self.push(AccountEvent::Balance(change.clone()));
        if self.balance_history.len() == 1_000 {
            self.balance_history.remove(0);
        }
        self.balance_history.push(change);
    }

    /// Values `position` at the symbol's last price.
    fn value(&self, terms: &PaperAccount, position: &mut Position) {
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
        position.margin = Some(size * last.abs() / terms.leverage);
    }

    fn account(&self, terms: &PaperAccount) -> AccountEvent {
        let mut account = Account::new(
            &terms.currency,
            self.balance,
            Capabilities {
                market: true,
                limit: true,
                stop: true,
                stop_limit: true,
                brackets: true,
                modify: true,
            },
        );
        let (equity, margin) = self.equity_and_margin(terms);
        let (orders_margin, commissions) = self.reservations(terms, None);
        account.equity = Some(equity);
        account.realized_pnl = Some(self.balance - terms.initial);
        account.margin = Some(margin);
        account.orders_margin = Some(orders_margin);
        account.available_funds = Some(equity - margin - orders_margin - commissions);
        AccountEvent::Account(account)
    }

    fn equity_and_margin(&self, terms: &PaperAccount) -> (f64, f64) {
        let mut equity = self.balance;
        let mut margin = 0.0;
        for position in self.positions.values() {
            let price = self
                .last
                .get(&position.symbol)
                .copied()
                .unwrap_or(position.average_price);
            equity += (price - position.average_price) * position.quantity;
            margin += position.quantity.abs() * price.abs() / terms.leverage;
        }
        (equity, margin)
    }

    fn reservations(&self, terms: &PaperAccount, replacing: Option<&OrderId>) -> (f64, f64) {
        let value: f64 = self
            .working
            .iter()
            .filter(|order| {
                Some(&order.id) != replacing
                    && !self.exits.contains_key(&order.id)
                    && order.request.position_bracket.is_none()
            })
            .map(|order| {
                let price = match order.request.order_type {
                    OrderType::Limit(price) | OrderType::Stop(price) => price,
                    OrderType::StopLimit { limit, .. } => limit,
                    _ => self.last.get(&order.request.symbol).copied().unwrap_or(0.0),
                };
                order.request.quantity * price.abs()
            })
            .sum();
        (value / terms.leverage, value * terms.commission)
    }

    /// Projects this request against the whole account, excluding the order it replaces.
    fn buying_power(
        &self,
        terms: &PaperAccount,
        request: &OrderRequest,
        replacing: Option<&OrderId>,
    ) -> Result<(), FeedError> {
        let held = self
            .positions
            .get(&request.symbol)
            .map_or(0.0, |p| p.quantity);
        let signed = if request.side == OrderSide::Buy {
            request.quantity
        } else {
            -request.quantity
        };
        let after = held + signed;
        if request.position_bracket.is_some()
            || replacing.is_some_and(|id| self.exits.contains_key(id))
        {
            return Ok(());
        }
        let last = self.last[&request.symbol];
        let buy = request.side == OrderSide::Buy;
        let limit_reached = |limit| if buy { last <= limit } else { last >= limit };
        let stop_reached = |stop| if buy { last >= stop } else { last <= stop };
        let immediate = match request.order_type {
            OrderType::Market => true,
            OrderType::Limit(limit) => limit_reached(limit),
            OrderType::Stop(stop) => stop_reached(stop),
            OrderType::StopLimit { stop, limit } => stop_reached(stop) && limit_reached(limit),
        };
        if immediate && after.abs() < held.abs() {
            return Ok(());
        }
        let (equity, mut margin) = self.equity_and_margin(terms);
        let price = match request.order_type {
            _ if immediate => last,
            OrderType::Market => last,
            OrderType::Limit(price) | OrderType::Stop(price) => price,
            OrderType::StopLimit { limit, .. } => limit,
        };
        let (reserved, commissions) = self.reservations(terms, replacing);
        let value = request.quantity * price.abs();
        if immediate {
            margin += (after.abs() - held.abs()) * last.abs() / terms.leverage;
        } else {
            margin += value / terms.leverage;
        }
        let needed = margin + reserved + commissions + value * terms.commission;
        if !needed.is_finite() || !equity.is_finite() || needed > equity {
            return Err(FeedError::Other(format!(
                "Insufficient buying power: {needed:.2} {} needed, {equity:.2} available",
                terms.currency
            )));
        }
        Ok(())
    }

    fn id(&mut self) -> OrderId {
        self.next_id += 1;
        OrderId(self.next_id.to_string())
    }

    fn finish(&mut self, mut order: Order, status: OrderStatus) {
        order.status = status;
        order.closed = Some(self.now());
        self.push(AccountEvent::Order(order.clone()));
        if self.finished.len() == 1_000 {
            self.finished.remove(0);
        }
        self.finished.push(order);
    }

    /// Fills `order` in full at `price`: updates the net position, books the realised profit
    /// and the commission, and turns brackets into exit orders.
    fn fill(&mut self, terms: &PaperAccount, mut order: Order, price: f64) {
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
            self.now(),
            request.side,
            price,
            request.quantity,
        );
        let id = order.id.clone();
        self.finish(order, OrderStatus::Filled);
        self.push(AccountEvent::Execution(execution.clone()));
        if self.executions.len() == 1_000 {
            self.executions.remove(0);
        }
        self.executions.push(execution);
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
            -request.quantity * price.abs() * terms.commission,
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
            if exit.request.symbol != request.symbol
                || (!self.exits.contains_key(&exit.id) && exit.request.position_bracket.is_none())
            {
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

        let protected = if position.quantity.signum() == signed.signum() {
            request.quantity.min(position.quantity.abs())
        } else {
            0.0
        };
        let exit = |price, order_type: fn(f64) -> OrderType| {
            let side = match request.side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            };
            let mut exit = OrderRequest::new(&request.symbol, side, protected, order_type(price));
            exit.time_in_force = TimeInForce::GoodTillCancelled;
            exit.position_bracket = Some(match exit.order_type {
                OrderType::Limit(_) => PositionBracket::TakeProfit,
                _ => PositionBracket::StopLoss,
            });
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
        .filter(|_| protected > 0.0)
        .map(|exit| {
            let id = self.id();
            let mut order = Order::new(id, exit, OrderStatus::Working);
            order.placed = Some(self.now());
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
    fn reach(&mut self, terms: &PaperAccount, symbol: &str, price: f64) {
        let mut index = 0;
        while index < self.working.len() {
            let order = &mut self.working[index];
            let buy = order.request.side == OrderSide::Buy;
            let fill = match order.request.order_type {
                OrderType::Market if order.request.symbol == symbol => Some(price),
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
                let mut request = order.request.clone();
                request.order_type = OrderType::Market;
                match self.buying_power(terms, &request, Some(&order.id)) {
                    Ok(()) => self.fill(terms, order, fill),
                    Err(error) => {
                        self.log(format!("Order {} rejected: {error}", order.id));
                        self.finish(order, OrderStatus::Rejected(error.to_string()));
                        self.push(self.account(terms));
                    }
                }
                index = 0;
            } else {
                index += 1;
            }
        }
    }
}

/// Reject malformed requests before they can change the account.
fn validate(order: &OrderRequest, last: f64) -> Result<(), FeedError> {
    let price = match order.order_type {
        OrderType::Market => last,
        OrderType::Limit(price) | OrderType::Stop(price) => price,
        OrderType::StopLimit { stop, limit } if stop.is_finite() => limit,
        OrderType::StopLimit { .. } => f64::NAN,
    };
    let buy = order.side == OrderSide::Buy;
    if !order.quantity.is_finite()
        || order.quantity <= 0.0
        || !price.is_finite()
        || !(order.quantity * price).is_finite()
        || order
            .take_profit
            .is_some_and(|p| !p.is_finite() || if buy { p <= price } else { p >= price })
        || order
            .stop_loss
            .is_some_and(|p| !p.is_finite() || if buy { p >= price } else { p <= price })
    {
        return Err(FeedError::Other(
            "invalid quantity, price or brackets".into(),
        ));
    }
    if order.time_in_force != TimeInForce::GoodTillCancelled {
        return Err(FeedError::NotSupported);
    }
    Ok(())
}

impl Broker for PaperAccount {
    fn connect(&self, events: Events<AccountEvent>) {
        let mut state = self.lock();
        state.events = Some(events);
        state.push(state.account(self));
        for order in state.working.iter().chain(&state.finished) {
            state.push(AccountEvent::Order(order.clone()));
        }
        for position in state.positions.values() {
            state.push(AccountEvent::Position(position.clone()));
        }
        for execution in &state.executions {
            state.push(AccountEvent::Execution(execution.clone()));
        }
        for change in &state.balance_history {
            state.push(AccountEvent::Balance(change.clone()));
        }
        for entry in &state.journal {
            state.push(AccountEvent::Journal(entry.clone()));
        }
    }

    fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
        let mut state = self.lock();
        let Some(&last) = state.last.get(&order.symbol) else {
            return reply.send(Err(FeedError::SymbolNotFound(order.symbol)));
        };
        if let Err(error) = validate(&order, last) {
            return reply.send(Err(error));
        }
        let id = state.id();
        reply.send(Ok(id.clone()));
        state.log(format!("Order {id} placed: {}", describe(&order)));
        let mut order = Order::new(id, order, OrderStatus::Working);
        order.placed = Some(state.now());
        let held = state
            .positions
            .get(&order.request.symbol)
            .map_or(0.0, |p| p.quantity);
        let request = &order.request;
        if request.position_bracket.is_some()
            && (held == 0.0
                || (request.side == OrderSide::Buy) == (held > 0.0)
                || request.quantity > held.abs())
        {
            state.finish(
                order,
                OrderStatus::Rejected("A position exit must reduce the open position".into()),
            );
            return;
        }
        if let Err(error) = state.buying_power(self, request, None) {
            state.finish(order, OrderStatus::Rejected(error.to_string()));
            return;
        }
        if order.request.order_type == OrderType::Market {
            state.fill(self, order, last);
        } else {
            let symbol = order.request.symbol.clone();
            state.push(AccountEvent::Order(order.clone()));
            state.working.push(order);
            // A marketable limit or a reached stop fills at once.
            state.reach(self, &symbol, last);
            state.push(state.account(self));
        }
    }

    fn cancel(&self, id: OrderId, reply: Reply<()>) {
        let mut state = self.lock();
        if state.cancel_working(&id) {
            reply.send(Ok(()));
            state.push(state.account(self));
        } else {
            reply.send(Err(FeedError::Other(format!(
                "order {id} is no longer working"
            ))));
        }
    }

    fn modify(&self, id: OrderId, order: OrderRequest, reply: Reply<()>) {
        let mut state = self.lock();
        let Some(working) = state.working.iter().find(|working| working.id == id) else {
            return reply.send(Err(FeedError::Other(format!(
                "order {id} is no longer working"
            ))));
        };
        if order.symbol != working.request.symbol
            || order.side != working.request.side
            || order.position_bracket != working.request.position_bracket
        {
            return reply.send(Err(FeedError::Other(
                "an order's symbol, side and position bracket cannot change".into(),
            )));
        }
        let last = state.last[&order.symbol];
        if let Err(error) = validate(&order, last) {
            return reply.send(Err(error));
        }
        if (state.exits.contains_key(&id) || order.position_bracket.is_some())
            && (state.positions.get(&order.symbol).is_none_or(|position| {
                order.quantity > position.quantity.abs()
                    || (order.side == OrderSide::Buy) == (position.quantity > 0.0)
            }) || order.take_profit.is_some()
                || order.stop_loss.is_some())
        {
            return reply.send(Err(FeedError::Other(
                "an exit may only reduce its position".into(),
            )));
        }
        if let Err(error) = state.buying_power(self, &order, Some(&id)) {
            return reply.send(Err(error));
        }
        let working = state
            .working
            .iter_mut()
            .find(|working| working.id == id)
            .unwrap();
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
            state.reach(self, &symbol, last);
        }
        state.push(state.account(self));
    }

    fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
        let mut rules = TradingRules::new(symbol);
        rules.time_in_force = vec![TimeInForce::GoodTillCancelled];
        rules.leverage = Some(self.leverage);
        reply.send(Ok(rules));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::inbox::Inbox;
    use crate::services::events::Listener;
    use std::sync::Arc;

    fn snapshot(paper: &PaperAccount) -> Vec<AccountEvent> {
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new(&inbox, 2);
        paper.connect(events);
        inbox
            .drain()
            .answers
            .into_iter()
            .map(|(_, event)| *event.downcast::<AccountEvent>().unwrap())
            .collect()
    }

    fn place(paper: &PaperAccount, mut request: OrderRequest) -> Result<OrderId, FeedError> {
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let inbox = Arc::new(Inbox::default());
        paper.place(request, inbox.reply(0));
        *inbox
            .drain()
            .answers
            .pop()
            .unwrap()
            .1
            .downcast::<Result<OrderId, FeedError>>()
            .unwrap()
    }

    #[test]
    fn account_figures_follow_adds_partial_closes_and_reversals() {
        for direction in [1.0, -1.0] {
            let paper = PaperAccount::new("USD", 10_000.0)
                .leverage(10.0)
                .commission(0.001);
            let time = DateTime::from_timestamp(1_704_067_200, 0).unwrap();
            paper.set_replay_time(Some(time));
            let mut balance = 10_000.0;
            // Price, signed fill, remaining quantity, average, gross realised profit.
            for (price, fill, quantity, average, realized) in [
                (100.0, 2.0_f64, 2.0, 100.0, 0.0),
                (130.0, 1.0, 3.0, 110.0, 0.0),
                (140.0, -1.0, 2.0, 110.0, 30.0),
                (120.0, -4.0, -2.0, 120.0, 20.0),
                (-10.0, 1.0, -1.0, 120.0, 130.0),
            ] {
                paper.set_price("SYM", price);
                let side = if fill * direction > 0.0 {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                place(
                    &paper,
                    OrderRequest::new("SYM", side, fill.abs(), OrderType::Market),
                )
                .unwrap();
                balance += realized * direction - fill.abs() * price.abs() * 0.001;
                let events = snapshot(&paper);
                assert!(events.iter().all(|event| match event {
                    AccountEvent::Order(order) => {
                        order.placed == Some(time) && order.closed == Some(time)
                    }
                    AccountEvent::Execution(fill) => fill.time == time,
                    AccountEvent::Balance(change) => change.time == time,
                    AccountEvent::Journal(entry) => entry.time == time,
                    _ => true,
                }));
                let account = events
                    .iter()
                    .find_map(|e| {
                        if let AccountEvent::Account(a) = e {
                            Some(a)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let position = events
                    .iter()
                    .find_map(|e| {
                        if let AccountEvent::Position(p) = e {
                            Some(p)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let near = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} != {b}");
                near(position.quantity, quantity * direction);
                near(position.average_price, average);
                near(account.balance, balance);
                near(account.realized_pnl.unwrap(), balance - 10_000.0);
                let open = (price - average) * quantity * direction;
                let margin = (quantity * price).abs() / 10.0;
                near(position.unrealized_pnl.unwrap(), open);
                near(position.margin.unwrap(), margin);
                near(account.margin.unwrap(), margin);
                near(account.equity.unwrap(), balance + open);
                near(account.available_funds.unwrap(), balance + open - margin);
            }
            paper.set_price("SYM", 100.0);
            place(
                &paper,
                OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(90.0)),
            )
            .unwrap();
            let events = snapshot(&paper);
            let account = events
                .iter()
                .find_map(|e| {
                    if let AccountEvent::Account(a) = e {
                        Some(a)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(account.orders_margin, Some(9.0));
            assert_eq!(
                account.available_funds,
                Some(account.equity.unwrap() - account.margin.unwrap() - 9.0 - 0.09)
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, AccountEvent::Execution(_)))
                    .count(),
                5
            );
            paper.reset();
            let events = snapshot(&paper);
            assert_eq!(events.len(), 1);
            assert!(matches!(&events[0], AccountEvent::Account(a) if a.balance == 10_000.0));
            for replay in [true, false] {
                if !replay {
                    paper.set_replay_time(None);
                }
                let before = Utc::now();
                place(
                    &paper,
                    OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market),
                )
                .unwrap();
                let events = snapshot(&paper);
                let fill = events
                    .iter()
                    .rev()
                    .find_map(|event| match event {
                        AccountEvent::Execution(fill) => Some(fill),
                        _ => None,
                    })
                    .unwrap();
                if replay {
                    assert_eq!(fill.time, time, "reset must keep the host's replay clock");
                } else {
                    assert!(
                        fill.time >= before,
                        "leaving replay restores the live clock"
                    );
                }
            }
        }
    }

    #[test]
    fn bracket_exits_only_reduce_the_remaining_position_and_cancel_their_pair() {
        for closed in [0.0, 1.0, 2.0, 4.0] {
            for close_brackets in [false, true] {
                let paper = PaperAccount::new("USD", 10_000.0);
                paper.set_price("SYM", 100.0);
                let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market);
                entry.take_profit = Some(105.0);
                entry.stop_loss = Some(95.0);
                place(&paper, entry).unwrap();
                if closed > 0.0 {
                    let mut close =
                        OrderRequest::new("SYM", OrderSide::Sell, closed, OrderType::Market);
                    if close_brackets {
                        close.take_profit = Some(90.0);
                        close.stop_loss = Some(110.0);
                    }
                    place(&paper, close).unwrap();
                }
                let events = snapshot(&paper);
                let exits: Vec<_> = events
                    .iter()
                    .filter_map(|e| match e {
                        AccountEvent::Order(o) if o.status == OrderStatus::Working => Some(o),
                        _ => None,
                    })
                    .collect();
                let expected = if closed < 2.0 {
                    2.0 - closed
                } else if close_brackets {
                    closed - 2.0
                } else {
                    0.0
                };
                assert_eq!(exits.len(), if expected > 0.0 { 2 } else { 0 });
                assert!(exits.iter().all(|o| o.request.quantity == expected));
                paper.set_price("SYM", 106.0);
                paper.set_price("SYM", 94.0);
                paper.set_price("SYM", 111.0);
                let events = snapshot(&paper);
                let position = events.iter().find_map(|e| {
                    if let AccountEvent::Position(p) = e {
                        Some(p.quantity)
                    } else {
                        None
                    }
                });
                assert_eq!(
                    position.unwrap_or(0.0),
                    if closed == 4.0 && !close_brackets {
                        -2.0
                    } else {
                        0.0
                    }
                );
                assert!(!events.iter().any(
                    |e| matches!(e, AccountEvent::Order(o) if o.status == OrderStatus::Working)
                ));
                assert!(events.iter().any(
                    |e| matches!(e, AccountEvent::Order(o) if o.status == OrderStatus::Cancelled)
                ));
            }
        }
    }

    #[test]
    fn reconnect_keeps_recent_history_within_the_stream_limit() {
        let paper = PaperAccount::new("USD", 1_000_000.0).commission(0.001);
        paper.set_price("SYM", 100.0);
        for _ in 0..1_001 {
            place(
                &paper,
                OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market),
            )
            .unwrap();
        }
        let inbox = Arc::new(Inbox::default());
        let (listener, events) = Listener::new(&inbox, 2);
        paper.connect(events);
        assert!(listener.is_active());
        let events: Vec<_> = inbox
            .drain()
            .answers
            .into_iter()
            .map(|(_, e)| *e.downcast::<AccountEvent>().unwrap())
            .collect();
        for count in [
            events
                .iter()
                .filter(|e| matches!(e, AccountEvent::Order(_)))
                .count(),
            events
                .iter()
                .filter(|e| matches!(e, AccountEvent::Execution(_)))
                .count(),
            events
                .iter()
                .filter(|e| matches!(e, AccountEvent::Balance(_)))
                .count(),
            events
                .iter()
                .filter(|e| matches!(e, AccountEvent::Journal(_)))
                .count(),
        ] {
            assert_eq!(count, 1_000);
        }
    }

    #[test]
    fn rejected_exposure_and_position_exits_do_not_fill() {
        let paper = PaperAccount::new("USD", 100.0).leverage(2.0);
        paper.set_price("SYM", 100.0);
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market),
        )
        .unwrap();
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market),
        )
        .unwrap();
        for (side, quantity) in [(OrderSide::Buy, 1.0), (OrderSide::Sell, 3.0)] {
            let mut exit = OrderRequest::new("SYM", side, quantity, OrderType::Limit(105.0));
            exit.position_bracket = Some(PositionBracket::TakeProfit);
            place(&paper, exit).unwrap();
        }
        let events = snapshot(&paper);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, AccountEvent::Execution(_)))
                .count(),
            1
        );
        assert_eq!(events.iter().filter(|e| matches!(e, AccountEvent::Order(o) if matches!(o.status, OrderStatus::Rejected(_)))).count(), 3);
        // Reducing exposure remains possible even when the position exceeds buying power.
        paper.set_price("SYM", 200.0);
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Sell, 1.0, OrderType::Market),
        )
        .unwrap();
        assert_eq!(paper.lock().positions["SYM"].quantity, 1.0);
    }

    #[test]
    fn buying_power_counts_other_symbols_reservations_costs_and_modifications() {
        for (first, commission, quantity) in [
            (OrderType::Market, 0.0, 10.0),
            (OrderType::Limit(90.0), 0.0, 6.0),
            (OrderType::Market, 0.01, 5.0),
        ] {
            let paper = PaperAccount::new("USD", 1_000.0).commission(commission);
            paper.set_price("A", 100.0);
            paper.set_price("B", 100.0);
            // A holds 1,000 or 500 margin, or reserves 450. The new B order
            // needs 1,000, 600 or 500 plus 5 commission, respectively.
            place(
                &paper,
                OrderRequest::new(
                    "A",
                    OrderSide::Buy,
                    if quantity == 10.0 { 10.0 } else { 5.0 },
                    first,
                ),
            )
            .unwrap();
            let id = place(
                &paper,
                OrderRequest::new("B", OrderSide::Buy, quantity, OrderType::Market),
            )
            .unwrap();
            let state = paper.lock();
            assert!(!state.positions.contains_key("B"));
            assert!(state.finished.iter().any(|order| order.id == id && matches!(order.status, OrderStatus::Rejected(_))));
        }
        let paper = PaperAccount::new("USD", 1_000.0).commission(0.001);
        paper.set_price("A", 100.0);
        let mut request = OrderRequest::new("A", OrderSide::Buy, 1.0, OrderType::Limit(90.0));
        let id = place(&paper, request.clone()).unwrap();
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let inbox = Arc::new(Inbox::default());
        for (quantity, price, accepted) in [
            (20.0, 90.0, false),
            (10.0, 90.0, true),
            (10.0, 91.0, true),
            (10.0, 100.0, false),
        ] {
            let before = snapshot(&paper);
            request.quantity = quantity;
            request.order_type = OrderType::Limit(price);
            paper.modify(id.clone(), request.clone(), inbox.reply(0));
            let result = *inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<(), FeedError>>()
                .unwrap();
            assert_eq!(
                result.is_ok(),
                accepted,
                "{quantity} at {price}: {result:?}"
            );
            if !accepted {
                assert_eq!(snapshot(&paper), before);
            }
        }
        paper.set_price("A", 90.0);
        let state = paper.lock();
        assert_eq!(state.positions["A"].quantity, 10.0);
        // 10 * 90 margin and 900 * 0.001 commission leave 99.10 available.
        assert_eq!(state.balance, 999.1);
        assert_eq!(state.positions["A"].margin, Some(900.0));
        drop(state);

        // A gap through a stop cannot spend funds already held in another symbol.
        let paper = PaperAccount::new("USD", 1_000.0);
        paper.set_price("A", 100.0);
        paper.set_price("B", 100.0);
        place(
            &paper,
            OrderRequest::new("A", OrderSide::Buy, 5.0, OrderType::Market),
        )
        .unwrap();
        let id = place(
            &paper,
            OrderRequest::new("B", OrderSide::Buy, 4.0, OrderType::Stop(110.0)),
        )
        .unwrap();
        paper.set_price("B", 200.0);
        assert!(!paper.lock().positions.contains_key("B"));
        assert!(
            paper
                .lock()
                .finished
                .iter()
                .any(|order| order.id == id && matches!(order.status, OrderStatus::Rejected(_)))
        );
        // Unrealised losses reduce equity even though the cash balance has not changed.
        paper.set_price("A", 50.0);
        paper.set_price("B", 100.0);
        place(
            &paper,
            OrderRequest::new("B", OrderSide::Buy, 6.0, OrderType::Market),
        )
        .unwrap();
        assert!(!paper.lock().positions.contains_key("B"));
        // A marketable limit uses its actual, better fill price: 11 * 90 = 990.
        let paper = PaperAccount::new("USD", 1_000.0).commission(0.001);
        paper.set_price("A", 90.0);
        place(
            &paper,
            OrderRequest::new("A", OrderSide::Buy, 11.0, OrderType::Limit(100.0)),
        )
        .unwrap();
        assert_eq!(paper.lock().positions["A"].quantity, 11.0);
        assert_eq!(paper.lock().positions["A"].average_price, 90.0);

        // Rejections during matching must report equity and margin at the new mark too.
        let paper = PaperAccount::new("USD", 1_000.0);
        paper.set_price("A", 100.0);
        place(
            &paper,
            OrderRequest::new("A", OrderSide::Buy, 5.0, OrderType::Market),
        )
        .unwrap();
        place(
            &paper,
            OrderRequest::new("A", OrderSide::Buy, 4.0, OrderType::Stop(110.0)),
        )
        .unwrap();
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new(&inbox, 2);
        paper.connect(events);
        inbox.drain();
        paper.set_price("A", 200.0);
        let mut accounts = 0;
        for (_, event) in inbox.drain().answers {
            if let AccountEvent::Account(account) = *event.downcast::<AccountEvent>().unwrap() {
                accounts += 1;
                assert_eq!(account.equity, Some(1_500.0));
                assert_eq!(account.margin, Some(1_000.0));
            }
        }
        assert!(accounts > 0, "matching must publish the account");
    }

    #[test]
    fn malformed_requests_and_exit_increases_leave_the_account_unchanged() {
        let paper = PaperAccount::new("USD", 10_000.0);
        paper.set_price("SYM", 100.0);
        for quantity in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                place(
                    &paper,
                    OrderRequest::new("SYM", OrderSide::Buy, quantity, OrderType::Market)
                )
                .is_err()
            );
            assert_eq!(snapshot(&paper).len(), 1);
        }
        for kind in [
            OrderType::Limit(f64::NAN),
            OrderType::Stop(f64::INFINITY),
            OrderType::StopLimit {
                stop: f64::NAN,
                limit: 90.0,
            },
        ] {
            assert!(place(&paper, OrderRequest::new("SYM", OrderSide::Buy, 1.0, kind)).is_err());
        }
        let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market);
        entry.take_profit = Some(110.0);
        place(&paper, entry).unwrap();
        let before = snapshot(&paper);
        let exit = before
            .iter()
            .find_map(|e| match e {
                AccountEvent::Order(o) if o.status == OrderStatus::Working => Some(o),
                _ => None,
            })
            .unwrap();
        let mut changed = exit.request.clone();
        changed.quantity = 2.0;
        let inbox = Arc::new(Inbox::default());
        paper.modify(exit.id.clone(), changed, inbox.reply(0));
        assert!(
            inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<(), FeedError>>()
                .unwrap()
                .is_err()
        );
        assert_eq!(snapshot(&paper), before);
    }

    #[test]
    fn only_gtc_orders_are_offered_and_accepted_without_session_boundaries() {
        let paper = PaperAccount::new("USD", 1_000.0);
        paper.set_price("SYM", 100.0);
        let inbox = Arc::new(Inbox::default());
        paper.rules("SYM", inbox.reply(0));
        let rules = *inbox
            .drain()
            .answers
            .pop()
            .unwrap()
            .1
            .downcast::<Result<TradingRules, FeedError>>()
            .unwrap();
        assert_eq!(
            rules.unwrap().time_in_force,
            vec![TimeInForce::GoodTillCancelled]
        );
        let mut request = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(90.0));
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let id = place(&paper, request.clone()).unwrap();
        let before = snapshot(&paper);
        for time_in_force in [
            TimeInForce::Day,
            TimeInForce::Week,
            TimeInForce::Month,
            TimeInForce::ImmediateOrCancel,
            TimeInForce::FillOrKill,
        ] {
            request.time_in_force = time_in_force;
            paper.place(request.clone(), inbox.reply(0));
            let result = *inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<OrderId, FeedError>>()
                .unwrap();
            assert_eq!(result, Err(FeedError::NotSupported));
            paper.modify(id.clone(), request.clone(), inbox.reply(0));
            let result = *inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<(), FeedError>>()
                .unwrap();
            assert_eq!(result, Err(FeedError::NotSupported));
            assert_eq!(snapshot(&paper), before);
        }
    }

    #[test]
    fn bracket_exits_only_reduce_the_remaining_position() {
        let terms = PaperAccount::new("USD", 0.0);
        for attached in [false, true] {
            for stop_loss in [None, Some(95.0)] {
                for closed in [1.0, 2.0, 4.0] {
                    let mut state = State::default();
                    let mut entry =
                        OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market);
                    if attached {
                        entry.take_profit = Some(105.0);
                        entry.stop_loss = stop_loss;
                    }
                    state.fill(
                        &terms,
                        Order::new(OrderId::new("entry"), entry, OrderStatus::Working),
                        100.0,
                    );
                    if !attached {
                        for (price, role) in [
                            (Some(105.0), PositionBracket::TakeProfit),
                            (stop_loss, PositionBracket::StopLoss),
                        ] {
                            if let Some(price) = price {
                                let kind = match role {
                                    PositionBracket::TakeProfit => OrderType::Limit(price),
                                    PositionBracket::StopLoss => OrderType::Stop(price),
                                };
                                let mut exit = OrderRequest::new("SYM", OrderSide::Sell, 2.0, kind);
                                exit.position_bracket = Some(role);
                                let id = state.id();
                                state
                                    .working
                                    .push(Order::new(id, exit, OrderStatus::Working));
                            }
                        }
                    }
                    state.fill(
                        &terms,
                        Order::new(
                            OrderId::new("close"),
                            OrderRequest::new("SYM", OrderSide::Sell, closed, OrderType::Market),
                            OrderStatus::Working,
                        ),
                        100.0,
                    );
                    if closed == 1.0 {
                        assert!(!state.working.is_empty());
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
}
