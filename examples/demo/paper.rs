//! Session-only paper account: market, limit, stop and stop-limit orders filled at the chart's
//! last price, one net position per symbol, take-profit and stop-loss exits that cancel each
//! other, and working orders whose prices can be changed.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;
use egui_charts::data::{FeedError, Reply};
use egui_charts::model::{
    Account, Capabilities, Execution, Order, OrderId, OrderRequest, OrderSide, OrderStatus,
    OrderType, Position, TimeInForce, TradingRules,
};
use egui_charts::services::{AccountEvent, Broker, Events};

const CURRENCY: &str = "USD";
const BALANCE: f64 = 100_000.0;

#[derive(Default)]
pub struct PaperAccount(Mutex<State>);

#[derive(Default)]
struct State {
    events: Option<Events<AccountEvent>>,
    next_id: u64,
    balance: f64,
    working: Vec<Order>,
    /// Every bracket exit and its paired exit, if any.
    exits: HashMap<OrderId, Option<OrderId>>,
    positions: HashMap<String, Position>,
    last: HashMap<String, f64>,
}

fn capabilities() -> Capabilities {
    let mut capabilities = Capabilities::default();
    capabilities.market = true;
    capabilities.limit = true;
    capabilities.stop = true;
    capabilities.stop_limit = true;
    capabilities.brackets = true;
    capabilities.modify = true;
    capabilities
}

impl State {
    fn push(&self, event: AccountEvent) {
        if let Some(events) = &self.events {
            events.push(event);
        }
    }

    fn account(&self) -> AccountEvent {
        let mut account = Account::new(CURRENCY, BALANCE + self.balance, capabilities());
        let open: f64 = self
            .positions
            .values()
            .filter_map(|p| p.unrealized_pnl)
            .sum();
        account.equity = Some(account.balance + open);
        AccountEvent::Account(account)
    }

    fn id(&mut self) -> OrderId {
        self.next_id += 1;
        OrderId(format!("paper-{}", self.next_id))
    }

    /// Fills `order` in full at `price`: updates the net position, books the realised profit
    /// and turns brackets into exit orders.
    fn fill(&mut self, mut order: Order, price: f64) {
        let request = order.request.clone();
        let signed = match request.side {
            OrderSide::Buy => request.quantity,
            OrderSide::Sell => -request.quantity,
        };
        let last = self.last.get(&request.symbol).copied().unwrap_or(price);
        let mut position = self
            .positions
            .remove(&request.symbol)
            .unwrap_or_else(|| Position::new(&request.symbol, 0.0, price));
        let held = position.quantity;
        if held * signed >= 0.0 {
            position.average_price =
                (held * position.average_price + signed * price) / (held + signed);
        } else {
            let closed = signed.abs().min(held.abs());
            self.balance += closed * (price - position.average_price) * held.signum();
            if signed.abs() > held.abs() {
                position.average_price = price;
            }
        }
        position.quantity = held + signed;
        position.unrealized_pnl = Some((last - position.average_price) * position.quantity);
        if position.quantity != 0.0 {
            self.positions
                .insert(request.symbol.clone(), position.clone());
        }

        order.status = OrderStatus::Filled;
        order.filled = request.quantity;
        let execution = Execution::new(
            order.id.clone(),
            Utc::now(),
            request.side,
            price,
            request.quantity,
        );
        self.push(AccountEvent::Order(order.clone()));
        self.push(AccountEvent::Execution(execution));
        self.push(AccountEvent::Position(position.clone()));
        if let Some(Some(other)) = self.exits.remove(&order.id) {
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
            Order::new(id, exit, OrderStatus::Working)
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
            self.working.push(exit);
        }
        self.push(self.account());
    }

    fn cancel_working(&mut self, id: &OrderId) -> bool {
        let Some(index) = self.working.iter().position(|order| &order.id == id) else {
            return false;
        };
        let mut order = self.working.remove(index);
        if let Some(Some(other)) = self.exits.remove(id)
            && let Some(pair) = self.exits.get_mut(&other)
        {
            *pair = None;
        }
        order.status = OrderStatus::Cancelled;
        self.push(AccountEvent::Order(order));
        true
    }
}

impl PaperAccount {
    /// Fills the working orders of `symbol` that `price` reached, and revalues its position.
    pub fn set_price(&self, symbol: &str, price: f64) {
        let mut state = self.0.lock().unwrap();
        if price.is_finite() && state.last.insert(symbol.to_owned(), price) != Some(price) {
            state.reach(symbol, price);
            if let Some(position) = state.positions.get_mut(symbol) {
                position.unrealized_pnl =
                    Some((price - position.average_price) * position.quantity);
                let position = position.clone();
                state.push(AccountEvent::Position(position));
                state.push(state.account());
            }
        }
    }
}

impl State {
    /// Fills the working orders of `symbol` that `price` reached.
    fn reach(&mut self, symbol: &str, price: f64) {
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
                self.fill(order, fill);
                index = 0;
            } else {
                index += 1;
            }
        }
    }
}

impl Broker for PaperAccount {
    fn connect(&self, events: Events<AccountEvent>) {
        let mut state = self.0.lock().unwrap();
        state.events = Some(events);
        state.push(state.account());
        for order in &state.working {
            state.push(AccountEvent::Order(order.clone()));
        }
        for position in state.positions.values() {
            state.push(AccountEvent::Position(position.clone()));
        }
    }

    fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
        let mut state = self.0.lock().unwrap();
        let Some(&last) = state.last.get(&order.symbol) else {
            return reply.send(Err(FeedError::SymbolNotFound(order.symbol)));
        };
        let id = state.id();
        reply.send(Ok(id.clone()));
        let order = Order::new(id, order, OrderStatus::Working);
        if order.request.order_type == OrderType::Market {
            state.fill(order, last);
        } else {
            let symbol = order.request.symbol.clone();
            state.push(AccountEvent::Order(order.clone()));
            state.working.push(order);
            // A marketable limit or a reached stop fills at once.
            state.reach(&symbol, last);
        }
    }

    fn cancel(&self, id: OrderId, reply: Reply<()>) {
        let mut state = self.0.lock().unwrap();
        if state.cancel_working(&id) {
            reply.send(Ok(()));
        } else {
            reply.send(Err(FeedError::Other(format!(
                "order {id} is no longer working"
            ))));
        }
    }

    fn modify(&self, id: OrderId, order: OrderRequest, reply: Reply<()>) {
        let mut state = self.0.lock().unwrap();
        let Some(working) = state.working.iter_mut().find(|working| working.id == id) else {
            return reply.send(Err(FeedError::Other(format!(
                "order {id} is no longer working"
            ))));
        };
        working.request = order;
        let (working, symbol) = (working.clone(), working.request.symbol.clone());
        reply.send(Ok(()));
        state.push(AccountEvent::Order(working));
        // A limit moved through the market or a stop moved past it fills at once.
        if let Some(&last) = state.last.get(&symbol) {
            state.reach(&symbol, last);
        }
    }

    fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
        let mut rules = TradingRules::new(symbol);
        rules.time_in_force = vec![TimeInForce::Day, TimeInForce::GoodTillCancelled];
        rules.leverage = Some(20.0);
        reply.send(Ok(rules));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_exits_only_reduce_the_remaining_position() {
        for stop_loss in [None, Some(95.0)] {
            for closed in [1.0, 2.0, 4.0] {
                let mut state = State::default();
                let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market);
                entry.take_profit = Some(105.0);
                entry.stop_loss = stop_loss;
                state.fill(
                    Order::new(OrderId::new("entry"), entry, OrderStatus::Working),
                    100.0,
                );
                let close = OrderRequest::new("SYM", OrderSide::Sell, closed, OrderType::Market);
                state.fill(
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
                state.reach("SYM", 106.0);
                state.reach("SYM", 94.0);
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
