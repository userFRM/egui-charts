//! A simulated account behind the [`Broker`] service.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AccountEvent, Broker, Events};
use crate::data::{FeedError, Reply};
use crate::model::rounding_tolerance;
use crate::model::{
    Account, BalanceChange, Capabilities, Execution, JournalEntry, Order, OrderId, OrderRequest,
    OrderSide, OrderStatus, OrderType, Position, PositionBracket, TimeInForce, TradingRules,
};

/// The times in force the account supports, the default first.
const TIMES_IN_FORCE: [TimeInForce; 1] = [TimeInForce::GoodTillCancelled];

/// A paper account: a [`Broker`] that fills orders against the prices the host feeds it, so the
/// order ticket, the chart's order and position lines and the account panel work as they do with
/// a real account.
///
/// Feed it the last price of every symbol it trades with [`set_price`](Self::set_price), live or
/// replayed. During replay, call [`set_replay_time`](Self::set_replay_time) before each price so
/// fills and history use the source bar's time. Only good-till-cancelled orders are supported;
/// the account has no trading-session calendar. Market orders fill at the bid or ask of a usable
/// [quote](Self::set_bid_ask), otherwise at the last price; limit and
/// stop orders fill at the first price that reaches them, a limit at that price or better;
/// a triggered stop-limit works on as a limit order. An order's take profits and stop loss become
/// exit orders once it fills, for the quantity the fill opened: take-profit levels each close their
/// own quantity, the first ones first when the fill opened less than the order's quantity, as after
/// closing an opposite position. When a take profit fills, the stop loss shrinks by its quantity
/// and is cancelled once nothing is left; when the stop loss fills, the take profits are cancelled.
/// Each symbol has one net position with its average
/// price: a fill against it books the realised profit or loss of the closed quantity, and a fill
/// beyond it opens the other side at the fill price. Values use the point value and leverage of
/// the symbol's [rules](Self::set_rules); a symbol without rules has a point value of 1 and the
/// account's leverage. A position holds its market value divided by the leverage as margin, and
/// a working order the same for its quantity at its price. Each fill
/// pays its commission: the rate times its value, plus the amount per order and the amount per
/// unit of its quantity, booked as one balance change. Placement and modification reserve buying power
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
    commission_per_order: f64,
    commission_per_unit: f64,
    state: Mutex<State>,
}

impl std::fmt::Debug for PaperAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaperAccount")
            .field("currency", &self.currency)
            .field("initial", &self.initial)
            .field("leverage", &self.leverage)
            .field("commission", &self.commission)
            .field("commission_per_order", &self.commission_per_order)
            .field("commission_per_unit", &self.commission_per_unit)
            .finish_non_exhaustive()
    }
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
    /// Every bracket exit, and the exits of the same order it acts on when it fills: a take
    /// profit's stop loss, which it shrinks, and a stop loss's take profits, which it cancels.
    exits: HashMap<OrderId, Vec<OrderId>>,
    positions: HashMap<String, Position>,
    last: HashMap<String, f64>,
    rules: HashMap<String, TradingRules>,
    quotes: HashMap<String, (f64, f64)>,
    balance_history: Vec<BalanceChange>,
    journal: Vec<JournalEntry>,
}

/// Negative and non-finite costs count as zero.
fn cost(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
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
    ///
    /// # Panics
    ///
    /// Panics if `balance` is NaN or infinite.
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
            commission_per_order: 0.0,
            commission_per_unit: 0.0,
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
        self.commission = cost(rate);
        self
    }

    /// Sets a fixed commission each fill pays per order, in the account's currency, beside the
    /// rate of [`commission`](Self::commission) and the amount per unit. Negative and non-finite
    /// amounts count as 0.
    pub fn commission_per_order(mut self, amount: f64) -> Self {
        self.commission_per_order = cost(amount);
        self
    }

    /// Sets a commission each fill pays per unit of its quantity, such as per share or per
    /// contract, in the account's currency, beside the rate and the amount per order. Negative
    /// and non-finite amounts count as 0.
    pub fn commission_per_unit(mut self, amount: f64) -> Self {
        self.commission_per_unit = cost(amount);
        self
    }

    fn commission_of(&self, quantity: f64, value: f64) -> f64 {
        value * self.commission + self.commission_per_order + quantity * self.commission_per_unit
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

    /// Takes `bid` and `ask` as the quote of `symbol`: while both are known and the bid is not
    /// above the ask, market orders fill a buy at the ask and a sell at the bid, including the
    /// orders that close or reverse a position. Otherwise, and once either is `None` or not
    /// finite, they fill at the last price. Limit and stop orders trigger and fill on the last
    /// price as before. The account still needs the last price of every symbol it trades.
    pub fn set_bid_ask(&self, symbol: &str, bid: Option<f64>, ask: Option<f64>) {
        let mut state = self.lock();
        match (bid, ask) {
            (Some(bid), Some(ask)) if bid.is_finite() && ask.is_finite() && bid <= ask => {
                state.quotes.insert(symbol.to_owned(), (bid, ask));
            }
            _ => {
                state.quotes.remove(symbol);
            }
        }
    }

    /// Gives `rules.symbol` its trading rules: the quantity step, minimum and maximum quantity,
    /// point value, leverage and times in force that [`rules`](Broker::rules) answers, so the
    /// order ticket sizes its orders by them. Fills, profit and loss, trade and market values,
    /// margin, commissions and buying power use the symbol's point value and leverage. Rules
    /// without a leverage take the account's, and a leverage below 1 counts as 1. Times in force
    /// the account does not support are left out, and good till cancelled is offered when none is
    /// left. Replaces earlier rules of the symbol, revalues its position and pushes the account.
    /// The account checks no quantity against the rules. Returns `false`, and keeps the earlier
    /// rules, when `rules` are not valid: a step, minimum or point value that is not positive and
    /// finite, a maximum below the minimum, a leverage or pip value that is not positive and
    /// finite, or no time in force.
    pub fn set_rules(&self, mut rules: TradingRules) -> bool {
        if !rules.is_valid() {
            return false;
        }
        rules.time_in_force.retain(|t| TIMES_IN_FORCE.contains(t));
        if rules.time_in_force.is_empty() {
            rules.time_in_force.push(TimeInForce::GoodTillCancelled);
        }
        rules.leverage = rules.leverage.map(|leverage| leverage.max(1.0));
        let mut state = self.lock();
        let symbol = rules.symbol.clone();
        state.rules.insert(symbol.clone(), rules);
        if let Some(mut position) = state.positions.remove(&symbol) {
            state.value(self, &mut position);
            state.positions.insert(symbol, position.clone());
            state.push(AccountEvent::Position(position));
        }
        state.push(state.account(self));
        true
    }

    /// Uses `time` for orders, executions, balance changes and journal entries during replay.
    /// Set it to the revealed source bar's time before feeding its price. `None` restores the
    /// live UTC clock.
    pub fn set_replay_time(&self, time: Option<DateTime<Utc>>) {
        self.lock().replay_time = time;
    }

    /// Returns the account to its starting balance without orders, positions or history. It keeps
    /// its settings and symbol rules, the host's prices, quotes and replay clock, and the count its order
    /// ids continue from. It ends the event stream of the last [`connect`](Broker::connect), so
    /// the terminal connects
    /// again at its next poll and shows the fresh account; a host that reads the stream itself
    /// calls `connect` again once [`Events::is_active`] is false.
    pub fn reset(&self) {
        let mut state = self.lock();
        if let Some(events) = state.events.take() {
            events.end();
        }
        let State {
            replay_time,
            next_id,
            last,
            rules,
            quotes,
            ..
        } = std::mem::take(&mut *state);
        *state = State {
            balance: self.initial,
            replay_time,
            next_id,
            last,
            rules,
            quotes,
            ..State::default()
        };
    }
}

impl State {
    /// The point value and leverage of `symbol`: its rules', else 1 and the account's leverage.
    fn terms_of(&self, terms: &PaperAccount, symbol: &str) -> (f64, f64) {
        let rules = self.rules.get(symbol);
        (
            rules.map_or(1.0, |rules| rules.point_value),
            rules
                .and_then(|rules| rules.leverage)
                .unwrap_or(terms.leverage),
        )
    }

    /// The times in force `symbol` accepts.
    fn time_in_force(&self, symbol: &str) -> &[TimeInForce] {
        self.rules
            .get(symbol)
            .map_or(&TIMES_IN_FORCE, |rules| &rules.time_in_force)
    }

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
        let (pv, lev) = self.terms_of(terms, &position.symbol);
        let size = position.quantity.abs();
        position.last_price = Some(last);
        position.unrealized_pnl = Some((last - position.average_price) * position.quantity * pv);
        position.trade_value = Some(size * position.average_price * pv);
        position.market_value = Some(size * last * pv);
        position.leverage = Some(lev);
        position.margin = Some(size * last.abs() * pv / lev);
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
                take_profit_levels: true,
                close: true,
                reverse: true,
                preview: false,
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
            let (pv, lev) = self.terms_of(terms, &position.symbol);
            equity += (price - position.average_price) * position.quantity * pv;
            margin += position.quantity.abs() * price.abs() * pv / lev;
        }
        (equity, margin)
    }

    fn reservations(&self, terms: &PaperAccount, replacing: Option<&OrderId>) -> (f64, f64) {
        self.working
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
                let (pv, lev) = self.terms_of(terms, &order.request.symbol);
                let value = order.request.quantity * price.abs() * pv;
                (
                    value / lev,
                    terms.commission_of(order.request.quantity, value),
                )
            })
            .fold((0.0, 0.0), |(margin, commission), (m, c)| {
                (margin + m, commission + c)
            })
    }

    /// Where `request` fills now at the last price `last`: a market order at the quote's ask for
    /// a buy and bid for a sell, else at `last`; a limit or stop order at `last` once `last` reaches
    /// it, a stop-limit order once `last` reaches both its stop and its limit. `None` if it does
    /// not fill now.
    fn fills_at(&self, request: &OrderRequest, last: f64) -> Option<f64> {
        let at_last = |order_type: OrderType| order_type.fill_price(request.side, last, last);
        match request.order_type {
            OrderType::Market => Some(match self.quotes.get(&request.symbol) {
                Some(&(_, ask)) if request.side == OrderSide::Buy => ask,
                Some(&(bid, _)) => bid,
                None => last,
            }),
            OrderType::StopLimit { limit, .. } => {
                at_last(request.order_type).and(at_last(OrderType::Limit(limit)))
            }
            order_type => at_last(order_type),
        }
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
        let fill = self.fills_at(request, last);
        let immediate = fill.is_some();
        if immediate && after.abs() < held.abs() {
            return Ok(());
        }
        let (equity, mut margin) = self.equity_and_margin(terms);
        let price = fill.unwrap_or(match request.order_type {
            OrderType::Limit(price) | OrderType::Stop(price) => price,
            OrderType::StopLimit { limit, .. } => limit,
            _ => last,
        });
        let (reserved, commissions) = self.reservations(terms, replacing);
        let (pv, lev) = self.terms_of(terms, &request.symbol);
        let value = request.quantity * price.abs() * pv;
        if immediate {
            margin += (after.abs() - held.abs()) * last.abs() * pv / lev;
        } else {
            margin += value / lev;
        }
        let needed = margin + reserved + commissions + terms.commission_of(request.quantity, value);
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
        let (pv, _) = self.terms_of(terms, &request.symbol);
        let mut realized = 0.0;
        if held * signed >= 0.0 {
            position.average_price =
                (held * position.average_price + signed * price) / (held + signed);
        } else {
            let closed = signed.abs().min(held.abs());
            realized = closed * (price - position.average_price) * held.signum() * pv;
            if signed.abs() > held.abs() {
                position.average_price = price;
            }
        }
        let remaining = held + signed;
        position.quantity =
            if remaining.abs() <= rounding_tolerance(held.abs().max(signed.abs()), 1e-9) {
                0.0
            } else {
                remaining
            };
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
            -terms.commission_of(request.quantity, request.quantity * price.abs() * pv),
            format!("Commission for order {id}"),
        );
        if held != 0.0 && position.quantity == 0.0 {
            self.log(format!("Position {} closed", request.symbol));
        }
        if let Some(partners) = self.exits.remove(&id) {
            for other in partners {
                if let Some(list) = self.exits.get_mut(&other) {
                    list.retain(|exit| *exit != id);
                }
                let working = self.working.iter_mut().find(|order| order.id == other);
                match working {
                    Some(stop_loss)
                        if request.position_bracket == Some(PositionBracket::TakeProfit)
                            && stop_loss.request.quantity - request.quantity
                                > rounding_tolerance(stop_loss.request.quantity, 1e-9) =>
                    {
                        stop_loss.request.quantity -= request.quantity;
                        let stop_loss = stop_loss.clone();
                        self.push(AccountEvent::Order(stop_loss));
                    }
                    _ => {
                        self.cancel_working(&other);
                    }
                }
            }
        }
        // Exits may only reduce the remaining position, including after a manual close or reverse.
        for mut exit in self.working.clone() {
            if exit.request.symbol != request.symbol
                || (!self.exits.contains_key(&exit.id) && exit.request.position_bracket.is_none())
            {
                continue;
            }
            if !exit.request.side.reduces(position.quantity) {
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
        let exit = |price, quantity, order_type: fn(f64) -> OrderType| {
            let side = match request.side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            };
            let mut exit = OrderRequest::new(&request.symbol, side, quantity, order_type(price));
            exit.time_in_force = TimeInForce::GoodTillCancelled;
            exit.position_bracket = Some(match exit.order_type {
                OrderType::Limit(_) => PositionBracket::TakeProfit,
                _ => PositionBracket::StopLoss,
            });
            exit
        };
        // Levels close their own quantity, the first ones first when less than the order opened.
        let mut left = protected;
        let take_profits = request
            .take_profit_levels
            .iter()
            .map(|level| {
                let share = level.quantity.min(left);
                left -= share;
                (level.price, share)
            })
            .chain(request.take_profit.map(|price| (price, protected)))
            .filter(|(_, quantity)| *quantity > rounding_tolerance(protected, 1e-9))
            .map(|(price, quantity)| exit(price, quantity, OrderType::Limit));
        let stop_loss = request
            .stop_loss
            .filter(|_| protected > 0.0)
            .map(|price| exit(price, protected, OrderType::Stop));
        let exits: Vec<_> = take_profits
            .chain(stop_loss)
            .map(|exit| {
                let id = self.id();
                let mut order = Order::new(id, exit, OrderStatus::Working);
                order.placed = Some(self.now());
                order
            })
            .collect();
        let ids = |bracket| {
            exits
                .iter()
                .filter(|exit| exit.request.position_bracket == Some(bracket))
                .map(|exit| exit.id.clone())
                .collect::<Vec<_>>()
        };
        let (take_profits, stop_losses) = (
            ids(PositionBracket::TakeProfit),
            ids(PositionBracket::StopLoss),
        );
        for exit in exits {
            let partners = match exit.request.position_bracket {
                Some(PositionBracket::StopLoss) => take_profits.clone(),
                _ => stop_losses.clone(),
            };
            self.exits.insert(exit.id.clone(), partners);
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
        for other in self.exits.remove(id).into_iter().flatten() {
            if let Some(list) = self.exits.get_mut(&other) {
                list.retain(|exit| exit != id);
            }
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
            let fill = if order.request.symbol == symbol {
                order
                    .request
                    .order_type
                    .fill_price(order.request.side, price, price)
            } else {
                None
            };
            if let (Some(_), OrderType::StopLimit { limit, .. }) = (fill, order.request.order_type)
            {
                // Triggered: it works on as a limit order, which may fill at once.
                order.request.order_type = OrderType::Limit(limit);
                let order = order.clone();
                self.push(AccountEvent::Order(order));
                continue;
            }
            if let Some(fill) = fill {
                let order = self.working.remove(index);
                match self.buying_power(terms, &order.request, Some(&order.id)) {
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
fn validate(order: &OrderRequest, last: f64, point_value: f64) -> Result<(), FeedError> {
    let price = match order.order_type {
        OrderType::Market => last,
        OrderType::Limit(price) | OrderType::Stop(price) => price,
        OrderType::StopLimit { stop, limit } if stop.is_finite() => limit,
        OrderType::StopLimit { .. } => f64::NAN,
    };
    let buy = order.side == OrderSide::Buy;
    let no_profit = |p: f64| !p.is_finite() || if buy { p <= price } else { p >= price };
    let levels = &order.take_profit_levels;
    let bad_levels = !levels.is_empty()
        && (order.take_profit.is_some()
            || levels.iter().any(|level| {
                no_profit(level.price) || !level.quantity.is_finite() || level.quantity <= 0.0
            })
            || (levels.iter().map(|level| level.quantity).sum::<f64>() - order.quantity).abs()
                > rounding_tolerance(order.quantity, 1e-9));
    if !order.quantity.is_finite()
        || order.quantity <= 0.0
        || !price.is_finite()
        || !(order.quantity * price * point_value).is_finite()
        || order.take_profit.is_some_and(no_profit)
        || bad_levels
        || order
            .stop_loss
            .is_some_and(|p| !p.is_finite() || if buy { p >= price } else { p <= price })
    {
        return Err(FeedError::Other(
            "invalid quantity, price or brackets".into(),
        ));
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
        if let Err(error) = validate(&order, last, state.terms_of(self, &order.symbol).0) {
            return reply.send(Err(error));
        }
        if !state
            .time_in_force(&order.symbol)
            .contains(&order.time_in_force)
        {
            return reply.send(Err(FeedError::NotSupported));
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
            let price = state.fills_at(&order.request, last).unwrap_or(last);
            state.fill(self, order, price);
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
        if let Err(error) = validate(&order, last, state.terms_of(self, &order.symbol).0) {
            return reply.send(Err(error));
        }
        if order.time_in_force != TimeInForce::GoodTillCancelled {
            return reply.send(Err(FeedError::NotSupported));
        }
        if (state.exits.contains_key(&id) || order.position_bracket.is_some())
            && (state.positions.get(&order.symbol).is_none_or(|position| {
                order.quantity > position.quantity.abs()
                    || (order.side == OrderSide::Buy) == (position.quantity > 0.0)
            }) || order.take_profit.is_some()
                || !order.take_profit_levels.is_empty()
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
        let state = self.lock();
        let mut rules = state.rules.get(symbol).cloned().unwrap_or_else(|| {
            let mut rules = TradingRules::new(symbol);
            rules.time_in_force = TIMES_IN_FORCE.to_vec();
            rules
        });
        rules.leverage = Some(state.terms_of(self, symbol).1);
        drop(state);
        reply.send(Ok(rules));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::inbox::Inbox;
    use crate::model::TakeProfitLevel;
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

    fn rules_of(paper: &PaperAccount, symbol: &str) -> TradingRules {
        let inbox = Arc::new(Inbox::default());
        paper.rules(symbol, inbox.reply(0));
        (*inbox
            .drain()
            .answers
            .pop()
            .unwrap()
            .1
            .downcast::<Result<TradingRules, FeedError>>()
            .unwrap())
        .unwrap()
    }

    /// Symbol terms affect answers, fractional fills, valuation, reservations and realised money;
    /// unusable rules leave the earlier terms intact, and other symbols keep the account defaults.
    #[test]
    fn symbol_rules_set_the_answer_point_value_and_leverage() {
        let paper = PaperAccount::new("USD", 100_000.0).leverage(10.0);
        let mut expected = TradingRules::new("SYM");
        expected.time_in_force = vec![TimeInForce::GoodTillCancelled];
        expected.leverage = Some(10.0);
        let mut invalid = expected.clone();
        invalid.quantity_step = 0.0;
        assert!(!paper.set_rules(invalid));
        assert_eq!(rules_of(&paper, "SYM"), expected);

        let mut rules = TradingRules::new("SYM");
        (rules.quantity_step, rules.min_quantity) = (0.01, 0.01);
        rules.max_quantity = Some(100.0);
        rules.point_value = 50.0;
        rules.leverage = Some(20.0);
        rules.time_in_force = vec![TimeInForce::Day, TimeInForce::GoodTillCancelled];
        assert!(paper.set_rules(rules.clone()));
        rules.time_in_force = vec![TimeInForce::GoodTillCancelled];
        assert_eq!(rules_of(&paper, "SYM"), rules);
        for field in [
            "step",
            "minimum",
            "maximum",
            "point value",
            "leverage",
            "pip",
            "duration",
        ] {
            let mut invalid = rules.clone();
            match field {
                "step" => invalid.quantity_step = 0.0,
                "minimum" => invalid.min_quantity = -1.0,
                "maximum" => invalid.max_quantity = Some(0.001),
                "point value" => invalid.point_value = f64::NAN,
                "leverage" => invalid.leverage = Some(0.0),
                "pip" => invalid.pip_value = Some(f64::INFINITY),
                _ => invalid.time_in_force.clear(),
            }
            assert!(!paper.set_rules(invalid), "{field}");
            assert_eq!(rules_of(&paper, "SYM"), rules, "{field}");
        }
        paper.set_price("SYM", 100.0);
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Buy, 0.5, OrderType::Market),
        )
        .unwrap();
        paper.set_price("SYM", 101.0);
        paper.set_price("OTHER", 100.0);
        place(
            &paper,
            OrderRequest::new("OTHER", OrderSide::Buy, 1.0, OrderType::Market),
        )
        .unwrap();
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(90.0)),
        )
        .unwrap();
        let state = paper.lock();
        let position = &state.positions["SYM"];
        assert_eq!(position.quantity, 0.5);
        assert_eq!(position.unrealized_pnl, Some(25.0));
        assert_eq!(position.trade_value, Some(2_500.0));
        assert_eq!(position.market_value, Some(2_525.0));
        assert_eq!(position.leverage, Some(20.0));
        assert_eq!(position.margin, Some(126.25));
        assert_eq!(state.positions["OTHER"].margin, Some(10.0));
        let AccountEvent::Account(account) = state.account(&paper) else {
            unreachable!()
        };
        assert_eq!(account.margin, Some(136.25));
        assert_eq!(account.equity, Some(100_025.0));
        assert_eq!(account.orders_margin, Some(225.0));
        drop(state);
        let inbox = Arc::new(Inbox::default());
        paper.place(
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market),
            inbox.reply(0),
        );
        assert_eq!(
            *inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<OrderId, FeedError>>()
                .unwrap(),
            Err(FeedError::NotSupported)
        );
        rules.point_value = 100.0;
        assert!(paper.set_rules(rules.clone()));
        assert_eq!(paper.lock().positions["SYM"].unrealized_pnl, Some(50.0));
        assert_eq!(paper.lock().positions["SYM"].margin, Some(252.5));
        rules.point_value = 50.0;
        assert!(paper.set_rules(rules.clone()));
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Sell, 0.5, OrderType::Market),
        )
        .unwrap();
        assert_eq!(paper.lock().balance, 100_025.0);

        rules.leverage = Some(0.5);
        assert!(paper.set_rules(rules.clone()));
        assert_eq!(rules_of(&paper, "SYM").leverage, Some(1.0));
        rules.leverage = None;
        assert!(paper.set_rules(rules.clone()));
        assert_eq!(rules_of(&paper, "SYM").leverage, Some(10.0));
        paper.reset();
        rules.leverage = Some(10.0);
        assert_eq!(rules_of(&paper, "SYM"), rules);
    }

    /// Markets use a usable two-sided quote; missing, invalid and crossed quotes fall back to
    /// last, which still triggers and prices pending orders even when the ask exceeds the limit.
    #[test]
    fn market_orders_fill_at_the_quote_and_others_at_the_last_price() {
        use OrderSide::{Buy, Sell};
        for (bid, ask, kind, side, price) in [
            (Some(99.9), Some(100.1), OrderType::Market, Buy, 100.1),
            (Some(99.9), Some(100.1), OrderType::Market, Sell, 99.9),
            (None, None, OrderType::Market, Buy, 100.0),
            (None, None, OrderType::Market, Sell, 100.0),
            (Some(99.9), None, OrderType::Market, Buy, 100.0),
            (None, Some(100.1), OrderType::Market, Sell, 100.0),
            (Some(f64::NAN), Some(100.1), OrderType::Market, Buy, 100.0),
            (
                Some(99.9),
                Some(f64::INFINITY),
                OrderType::Market,
                Sell,
                100.0,
            ),
            (Some(100.1), Some(99.9), OrderType::Market, Buy, 100.0),
            (Some(99.0), Some(99.0), OrderType::Market, Buy, 99.0),
            (
                Some(99.9),
                Some(100.1),
                OrderType::Limit(100.05),
                Buy,
                100.0,
            ),
            (Some(99.9), Some(100.1), OrderType::Stop(99.95), Buy, 100.0),
        ] {
            let paper = PaperAccount::new("USD", 100_000.0);
            paper.set_price("SYM", 100.0);
            paper.set_bid_ask("SYM", Some(98.0), Some(102.0));
            paper.set_bid_ask("SYM", bid, ask);
            for _ in 0..2 {
                place(&paper, OrderRequest::new("SYM", side, 1.0, kind)).unwrap();
                let state = paper.lock();
                assert_eq!(
                    state.executions[0].price, price,
                    "{bid:?}/{ask:?} {kind:?} {side:?}"
                );
                assert_eq!(state.positions["SYM"].average_price, price);
                assert_eq!(state.positions["SYM"].last_price, Some(100.0));
                drop(state);
                paper.reset();
            }
        }
    }

    /// Every cost is booked once per fill and reserved per working order; the rate includes
    /// point value, while the per-unit amount uses quantity alone. Entry checks include each fee.
    #[test]
    fn commissions_book_the_rate_per_order_and_per_unit() {
        for (rate, per_order, per_unit, point_value, fill, working) in [
            (0.001, 0.0, 0.0, 1.0, 0.2, 0.09),
            (0.0, 5.0, 0.0, 1.0, 5.0, 5.0),
            (0.0, 0.0, 0.5, 1.0, 1.0, 0.5),
            (0.001, 5.0, 0.5, 50.0, 16.0, 10.0),
        ] {
            let account = |balance| {
                let paper = PaperAccount::new("USD", balance)
                    .commission(rate)
                    .commission_per_order(per_order)
                    .commission_per_unit(per_unit);
                let mut rules = TradingRules::new("SYM");
                rules.point_value = point_value;
                assert!(paper.set_rules(rules));
                paper.set_price("SYM", 100.0);
                paper
            };
            let paper = account(100_000.0);
            place(
                &paper,
                OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market),
            )
            .unwrap();
            place(
                &paper,
                OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(90.0)),
            )
            .unwrap();
            let near = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} != {b}");
            let state = paper.lock();
            assert_eq!(state.balance_history.len(), 1);
            assert!(
                state.balance_history[0]
                    .action
                    .starts_with("Commission for order")
            );
            near(state.balance_history[0].amount, -fill);
            near(state.balance, 100_000.0 - fill);
            let AccountEvent::Account(a) = state.account(&paper) else {
                unreachable!()
            };
            near(
                a.available_funds.unwrap(),
                a.equity.unwrap() - a.margin.unwrap() - a.orders_margin.unwrap() - working,
            );
            drop(state);
            let short = account(200.0 * point_value + fill - 0.01);
            place(
                &short,
                OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market),
            )
            .unwrap();
            assert!(matches!(
                short.lock().finished[0].status,
                OrderStatus::Rejected(_)
            ));
        }
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
            // Close the remaining unit, then 0.1 + 0.2 with 0.3 on either side.
            // One more unit scaled out in ten tenths is also flat: 1 - 10 * 0.1 = 0.
            for fill in [
                direction,
                0.1 * direction,
                0.2 * direction,
                -0.3 * direction,
                direction,
            ]
            .into_iter()
            .chain([-0.1 * direction; 10])
            {
                place(
                    &paper,
                    OrderRequest::new(
                        "SYM",
                        if fill > 0.0 {
                            OrderSide::Buy
                        } else {
                            OrderSide::Sell
                        },
                        fill.abs(),
                        OrderType::Market,
                    ),
                )
                .unwrap();
            }
            assert!(!paper.lock().positions.contains_key("SYM"));
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
                20
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
            for (attached, close_brackets) in [(true, false), (true, true), (false, false)] {
                let paper = PaperAccount::new("USD", 10_000.0);
                paper.set_price("SYM", 100.0);
                let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market);
                if attached {
                    entry.take_profit = Some(105.0);
                    entry.stop_loss = Some(95.0);
                }
                place(&paper, entry).unwrap();
                if !attached {
                    for (kind, role) in [
                        (OrderType::Limit(105.0), PositionBracket::TakeProfit),
                        (OrderType::Stop(95.0), PositionBracket::StopLoss),
                    ] {
                        let mut exit = OrderRequest::new("SYM", OrderSide::Sell, 2.0, kind);
                        exit.position_bracket = Some(role);
                        place(&paper, exit).unwrap();
                    }
                }
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

    /// Levels close their own quantity, and the first ones first when the fill opened less. A
    /// take profit shrinks the stop loss so that it protects what is left of the order and no
    /// more, and cancels it with the last level; the stop loss cancels the levels. A regression
    /// leaves the rest unprotected, closes units that were never part of the order, or keeps
    /// levels working on a position they no longer belong to.
    #[test]
    fn take_profit_levels_close_their_quantity_and_shrink_the_stop_loss() {
        let limit = |price, quantity| (OrderType::Limit(price), quantity);
        let stop = |price, quantity| (OrderType::Stop(price), quantity);
        // (held before, exits, price, exits, price, exits, position, balance)
        for (held, entered, first, partly, second, last, position, balance) in [
            // 1 at 106 earns 6 and 2 at 94 lose 12.
            (
                0.0_f64,
                vec![limit(105.0, 1.0), limit(110.0, 2.0), stop(95.0, 3.0)],
                106.0,
                vec![limit(110.0, 2.0), stop(95.0, 2.0)],
                94.0,
                vec![],
                0.0,
                9_994.0,
            ),
            // The same, on top of 5 units the order's exits must leave alone.
            (
                5.0,
                vec![limit(105.0, 1.0), limit(110.0, 2.0), stop(95.0, 3.0)],
                106.0,
                vec![limit(110.0, 2.0), stop(95.0, 2.0)],
                94.0,
                vec![],
                5.0,
                9_994.0,
            ),
            // Closing a short of 1 opens 2 of the 3, which both levels close at 111 for 11 each.
            (
                -1.0,
                vec![limit(105.0, 1.0), limit(110.0, 1.0), stop(95.0, 2.0)],
                111.0,
                vec![],
                111.0,
                vec![],
                0.0,
                10_022.0,
            ),
        ] {
            let paper = PaperAccount::new("USD", 10_000.0);
            paper.set_price("SYM", 100.0);
            if held != 0.0 {
                let side = if held > 0.0 {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                let start = OrderRequest::new("SYM", side, held.abs(), OrderType::Market);
                place(&paper, start).unwrap();
            }
            let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 3.0, OrderType::Market);
            entry.take_profit_levels = vec![
                TakeProfitLevel::new(105.0, 1.0),
                TakeProfitLevel::new(110.0, 2.0),
            ];
            entry.stop_loss = Some(95.0);
            place(&paper, entry).unwrap();
            let exits = || {
                let state = paper.lock();
                let exits = state.working.iter();
                exits
                    .map(|order| (order.request.order_type, order.request.quantity))
                    .collect::<Vec<_>>()
            };
            assert_eq!(exits(), entered, "held {held}");
            paper.set_price("SYM", first);
            assert_eq!(exits(), partly, "held {held}");
            paper.set_price("SYM", second);
            assert_eq!(exits(), last, "held {held}");
            let state = paper.lock();
            let quantity = state.positions.get("SYM").map_or(0.0, |p| p.quantity);
            assert_eq!(quantity, position, "held {held}");
            assert_eq!(state.balance, balance, "held {held}");
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
        let level = TakeProfitLevel::new;
        for (levels, take_profit) in [
            // They add up to 2 of 3.
            (vec![level(105.0, 1.0), level(110.0, 1.0)], None),
            (vec![level(95.0, 1.0), level(110.0, 2.0)], None),
            (vec![level(f64::NAN, 1.0), level(110.0, 2.0)], None),
            (vec![level(105.0, 0.0), level(110.0, 3.0)], None),
            (vec![level(105.0, 1.0), level(110.0, 2.0)], Some(110.0)),
        ] {
            let mut request = OrderRequest::new("SYM", OrderSide::Buy, 3.0, OrderType::Market);
            request.take_profit_levels = levels;
            request.take_profit = take_profit;
            assert!(place(&paper, request).is_err());
            assert_eq!(snapshot(&paper).len(), 1);
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
        let mut larger = exit.request.clone();
        larger.quantity = 2.0;
        // Valid for the exit's side and quantity: only the rule for exits refuses it.
        let mut with_level = exit.request.clone();
        with_level.take_profit_levels = vec![level(105.0, 1.0)];
        for changed in [larger, with_level] {
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
    fn take_profit_only_exits_reduce_the_position_and_fill_at_the_better_price() {
        for attached in [false, true] {
            for closed in [1.0, 2.0, 4.0] {
                let paper = PaperAccount::new("USD", 10_000.0);
                paper.set_price("SYM", 100.0);
                let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Market);
                if attached {
                    entry.take_profit = Some(105.0);
                }
                place(&paper, entry).unwrap();
                if !attached {
                    let mut exit =
                        OrderRequest::new("SYM", OrderSide::Sell, 2.0, OrderType::Limit(105.0));
                    exit.position_bracket = Some(PositionBracket::TakeProfit);
                    place(&paper, exit).unwrap();
                }
                place(
                    &paper,
                    OrderRequest::new("SYM", OrderSide::Sell, closed, OrderType::Market),
                )
                .unwrap();
                let events = snapshot(&paper);
                let quantities: Vec<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        AccountEvent::Order(order) if order.status == OrderStatus::Working => {
                            Some(order.request.quantity)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    quantities,
                    if closed == 1.0 { vec![1.0] } else { vec![] },
                    "attached {attached}, closed {closed}"
                );
                paper.set_price("SYM", 106.0);
                paper.set_price("SYM", 94.0);
                let events = snapshot(&paper);
                let position = events
                    .iter()
                    .find_map(|event| match event {
                        AccountEvent::Position(position) => Some(position.quantity),
                        _ => None,
                    })
                    .unwrap_or(0.0);
                assert_eq!(position, if closed == 4.0 { -2.0 } else { 0.0 });
                assert!(!events.iter().any(
                    |e| matches!(e, AccountEvent::Order(o) if o.status == OrderStatus::Working)
                ));
                if closed == 1.0 {
                    // One unit bought at 100 and sold at the reaching price 106 earns 6, not 5.
                    let account = events
                        .iter()
                        .find_map(|event| match event {
                            AccountEvent::Account(account) => Some(account),
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(account.balance, 10_006.0);
                    let fill = events
                        .iter()
                        .find_map(|event| match event {
                            AccountEvent::Order(order)
                                if order.request.order_type == OrderType::Limit(105.0) =>
                            {
                                Some(order)
                            }
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(fill.fill_price, Some(106.0));
                    assert_eq!(fill.filled, 1.0);
                }
            }
        }
    }
}
