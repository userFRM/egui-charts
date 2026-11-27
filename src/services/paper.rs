//! A simulated account behind the [`Broker`] service.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AccountEvent, Broker, Events};
use crate::data::{FeedError, Reply};
use crate::model::rounding_tolerance;
use crate::model::{
    Account, BalanceChange, Capabilities, ClosedPosition, Execution, JournalEntry, Order, OrderId,
    OrderRequest, OrderSide, OrderStatus, OrderType, Position, PositionBracket, StrategyExit,
    StrategyReport, StrategyTrade, TimeInForce, Timeframe, TradingRules, TrailDistance,
};

/// The times in force the account supports, the default first.
const TIMES_IN_FORCE: [TimeInForce; 3] = [
    TimeInForce::GoodTillCancelled,
    TimeInForce::ImmediateOrCancel,
    TimeInForce::FillOrKill,
];

/// Finished orders, executions, closed positions, balance changes and journal entries kept, which
/// keeps a reconnect snapshot below the event stream's burst limit.
const HISTORY: usize = 1_000;

/// The most take-profit levels an order may carry.
const MAX_TAKE_PROFIT_LEVELS: usize = 4;

/// A paper account: a [`Broker`] that fills orders against the prices the host feeds it, so the
/// order ticket, the chart's order and position lines and the account panel work as they do with
/// a real account.
///
/// Feed it the last price of every symbol it trades with [`set_price`](Self::set_price), live or
/// replayed. During replay, call [`set_replay_time`](Self::set_replay_time) before each price so
/// fills and history use the source bar's time; it refuses times before the account's last change.
/// Good-till-cancelled orders work until they fill or
/// are cancelled. Immediate-or-cancel and fill-or-kill orders fill at placement where a marketable
/// order would, and are cancelled at once otherwise; since every fill is complete, the two behave
/// alike. Day, week and month orders are refused, since the account has no trading-session
/// calendar, and a working order cannot change its time in force. Market orders fill at the bid or
/// ask of a usable [quote](Self::set_bid_ask), otherwise at the last price; limit and stop orders
/// fill at the first price that reaches them, a limit at that price or better; a triggered
/// stop-limit works on as a limit order. A trailing stop follows the best last price since its
/// activation price, or since it was placed without one, at its distance and never back, and
/// fills as a stop at the first price that reaches it; each move of its stop is pushed with the
/// order. It takes no take profit or stop loss. An order's take profits and stop loss become exit orders
/// once it fills, for the quantity the fill opened: take-profit levels each close their own
/// quantity, the first ones first when the fill opened less than the order's quantity, as after
/// closing an opposite position. A level's own stop loss closes the same quantity as its take
/// profit, and filling or cancelling either cancels the other, so a level stays whole or goes. The
/// order's stop loss closes the quantity of the levels without one: when one of their take profits
/// fills, it shrinks by that quantity and is cancelled once nothing is left; when it fills, those
/// take profits are cancelled. Cancelling it or one of those take profits leaves the rest working.
/// Cancelling an order the account has already cancelled, such as a level's other exit, succeeds.
/// An order carries at most 4 levels. Each symbol has one net position with its average price: a fill
/// against it books the realised profit or loss of the closed quantity, and a fill beyond it opens
/// the other side at the fill price. A fill that flattens or reverses a symbol records its completed
/// round trip, including partial closes, with profit or loss before commissions. Values use the
/// point value and leverage of the symbol's [rules](Self::set_rules); a symbol without rules has a
/// point value of 1 and the account's leverage. A position holds its market value divided by the
/// leverage as margin, and a working order the same for its quantity at its price. Each fill pays
/// its commission: the rate times its value, plus the amount per order and the amount per unit of
/// its quantity, booked as one balance change. Placement and modification reserve buying power
/// across all symbols, including working orders and their commissions. Market orders that reduce
/// exposure remain possible when funds are insufficient. Position exits only reduce the remaining
/// position, and their quantity cannot be changed: the account sizes them when they are placed and
/// shrinks them as the position shrinks. Every change is pushed as an [`AccountEvent`], with the
/// most recent 1,000 finished orders, executions, closed positions, balance changes and journal
/// entries. The history bound keeps a reconnect snapshot below the event stream's burst limit for
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

/// What a [`PaperAccount`] keeps between sessions, from [`PaperAccount::save`]: its settings,
/// symbol rules, balance, working orders with their exits, positions and history, without the
/// prices, quotes and replay clock, which the host feeds again, or the equity marked during
/// replay. Write it in any serde format and read it back for [`PaperAccount::restore`].
///
/// Fields added later to this type and the trading types it holds will carry `#[serde(default)]`,
/// so accounts saved by an earlier version keep loading.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedPaperAccount {
    currency: String,
    initial: f64,
    leverage: f64,
    commission: f64,
    commission_per_order: f64,
    commission_per_unit: f64,
    rules: Vec<TradingRules>,
    balance: f64,
    next_id: u64,
    working: Vec<Order>,
    exits: Vec<(OrderId, Vec<OrderId>)>,
    positions: Vec<Position>,
    finished: Vec<Order>,
    executions: Vec<Execution>,
    balance_history: Vec<BalanceChange>,
    journal: Vec<JournalEntry>,
    #[serde(default)]
    trips: Vec<(String, Trip)>,
    #[serde(default)]
    closed_positions: Vec<ClosedPosition>,
    #[serde(default)]
    trails: Vec<(OrderId, f64)>,
    #[serde(default)]
    paired: Vec<OrderId>,
}

/// The account a replay trading session starts with, as the replay start menu offers it: its
/// currency, initial capital and commission rate. [`account`](Self::account) builds a
/// [`PaperAccount`] for it, separate from any live account.
///
/// ```
/// use egui_charts::services::ReplayAccountSettings;
///
/// let settings = ReplayAccountSettings::new("EUR", 10_000.0, 0.001);
/// assert!(settings.is_valid());
/// let account = settings.account().expect("valid settings");
/// account.set_price("AAPL", 187.5);
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ReplayAccountSettings {
    /// The account's currency, such as "USD".
    pub currency: String,
    /// The starting balance in `currency`: positive and finite.
    pub initial_capital: f64,
    /// The commission each fill pays, as a fraction of its value: 0.001 for 0.1%.
    pub commission: f64,
}

impl Default for ReplayAccountSettings {
    /// 100,000 USD without commission.
    fn default() -> Self {
        Self::new("USD", 100_000.0, 0.0)
    }
}

impl ReplayAccountSettings {
    /// Settings with `currency`, `initial_capital` and the commission rate `commission`.
    pub fn new(currency: impl Into<String>, initial_capital: f64, commission: f64) -> Self {
        Self {
            currency: currency.into(),
            initial_capital,
            commission,
        }
    }

    /// Whether the currency is not blank, the initial capital is positive and finite and the
    /// commission is finite and not negative.
    pub fn is_valid(&self) -> bool {
        !self.currency.trim().is_empty()
            && self.initial_capital.is_finite()
            && self.initial_capital > 0.0
            && self.commission.is_finite()
            && self.commission >= 0.0
    }

    /// A new account with these settings, its currency trimmed, without leverage or other
    /// commissions, or `None` when the settings are not [valid](Self::is_valid).
    pub fn account(&self) -> Option<PaperAccount> {
        self.is_valid().then(|| {
            PaperAccount::new(self.currency.trim(), self.initial_capital)
                .commission(self.commission)
        })
    }
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
    /// Every bracket exit, and the exits of the same group it acts on when it fills: a take
    /// profit's stop loss, which it shrinks or cancels, and a stop loss's take profits, which it
    /// cancels. A level with its own stop loss is a group of two, whose exits are also cancelled
    /// together (`paired`); the order's stop loss and the take profits without their own form the
    /// other.
    exits: HashMap<OrderId, Vec<OrderId>>,
    /// The exits of take-profit levels with their own stop loss: cancelling one also cancels the
    /// exit it lists in `exits`, so a level stays whole or goes. The shared group's exits are
    /// cancelled alone.
    paired: HashSet<OrderId>,
    /// The best price of each working trailing stop since it started trailing.
    trails: HashMap<OrderId, f64>,
    positions: HashMap<String, Position>,
    trips: HashMap<String, Trip>,
    closed_positions: Vec<ClosedPosition>,
    last: HashMap<String, f64>,
    rules: HashMap<String, TradingRules>,
    quotes: HashMap<String, (f64, f64)>,
    balance_history: Vec<BalanceChange>,
    journal: Vec<JournalEntry>,
    /// Equity marked at each replay time the account was priced or filled at, oldest first.
    equity: Vec<(DateTime<Utc>, f64)>,
}

/// Negative and non-finite costs count as zero.
fn cost(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Accumulates a symbol's entries and partial closes until it becomes flat or reverses.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Trip {
    side: OrderSide,
    opened: Option<DateTime<Utc>>,
    /// Total entry quantity and the sum of its fill values.
    entered: f64,
    entry_value: f64,
    /// Total closed quantity and the sum of its fill values.
    exited: f64,
    exit_value: f64,
    realized: f64,
    /// Commissions of the trip's fills so far.
    #[serde(default)]
    fees: f64,
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
        OrderType::TrailingStop {
            distance,
            activation,
        } => {
            let distance = match distance {
                TrailDistance::Price(price) => format!("{price}"),
                TrailDistance::Ticks { ticks, .. } => format!("{ticks} ticks"),
                TrailDistance::Percent(percent) => format!("{percent}%"),
            };
            match activation {
                Some(activation) => format!("trailing stop {distance} from {activation}"),
                None => format!("trailing stop {distance}"),
            }
        }
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
    /// revalues the position. During replay, it marks the account's equity at the replay time
    /// for [`report`](Self::report). Non-finite prices are ignored.
    pub fn set_price(&self, symbol: &str, price: f64) {
        if !price.is_finite() {
            return;
        }
        let mut state = self.lock();
        if state.last.insert(symbol.to_owned(), price) != Some(price) {
            state.reach(self, symbol, price);
            if let Some(mut position) = state.positions.remove(symbol) {
                state.value(self, &mut position);
                state.positions.insert(symbol.to_owned(), position.clone());
                state.push(AccountEvent::Position(position));
                state.push(state.account(self));
            }
        }
        state.mark(self);
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
    ///
    /// ```
    /// use egui_charts::model::TradingRules;
    /// use egui_charts::services::PaperAccount;
    ///
    /// let paper = PaperAccount::new("USD", 100_000.0)
    ///     .commission_per_order(1.0)
    ///     .commission_per_unit(0.25);
    /// let mut rules = TradingRules::new("BTCUSD");
    /// rules.quantity_step = 0.001;
    /// rules.min_quantity = 0.001;
    /// rules.leverage = Some(2.0);
    /// assert!(paper.set_rules(rules));
    /// paper.set_price("BTCUSD", 50_000.0);
    /// paper.set_bid_ask("BTCUSD", Some(49_999.0), Some(50_001.0));
    /// ```
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
    /// live UTC clock. Returns `false`, and keeps the clock, when `time` is before the account's
    /// [`last_change`](Self::last_change), so its history never runs backwards: before moving
    /// the replay cursor back past the account's orders or fills, or starting again from an
    /// earlier bar, [`reset`](Self::reset) the account or build a new one, or ask the user, as
    /// the replay trading prompt of `ui::replay` does. Feed no prices of earlier bars to an
    /// account that refused their time.
    pub fn set_replay_time(&self, time: Option<DateTime<Utc>>) -> bool {
        let mut state = self.lock();
        if time.is_some_and(|time| state.last_change().is_some_and(|last| time < last)) {
            return false;
        }
        state.replay_time = time;
        true
    }

    /// The time of the account's newest change: the last order placed, changed, filled,
    /// cancelled or rejected. `None` for an account without history, as after
    /// [`reset`](Self::reset). During replay, [`set_replay_time`](Self::set_replay_time) refuses
    /// earlier times.
    pub fn last_change(&self) -> Option<DateTime<Utc>> {
        self.lock().last_change()
    }

    /// The account's trades as a strategy report named `name` for `symbol` and `timeframe`, for
    /// the results of a replay trading session: its starting balance and currency, every closed
    /// position as a closed trade and every open position as an open trade, each labelled with
    /// its symbol, with its commissions as fees and its profit net of them, and the equity
    /// marked at each replay time the account was priced or filled at.
    /// [`StrategyReport::performance`] then gives the net profit, win rate, profit factor and
    /// largest drawdown, and `StrategyReport::trades_csv` the list of trades. The newest 1,000
    /// closed positions are kept. A position closed in parts counts as one trade once it is
    /// flat or reversed; until then its realised part is only in the equity. Live prices mark
    /// no equity, and a saved account keeps no marks. A position without a reported opening
    /// time, from a save older than round-trip tracking, enters at its closing time; such an
    /// open position is left out.
    pub fn report(
        &self,
        name: impl Into<String>,
        symbol: impl Into<String>,
        timeframe: Timeframe,
    ) -> StrategyReport {
        let state = self.lock();
        let mut report = StrategyReport::new(name, symbol, timeframe, self.initial);
        report.currency = Some(self.currency.clone());
        for closed in &state.closed_positions {
            let fees = closed.commission.unwrap_or(0.0);
            let mut trade = StrategyTrade::new(
                closed.side,
                closed.quantity,
                closed.opened.unwrap_or(closed.closed),
                closed.average_price,
            );
            trade.fees = fees;
            trade.label = Some(closed.symbol.clone());
            trade.exit = Some(StrategyExit::new(
                closed.closed,
                closed.exit_price,
                closed.realized_pnl - fees,
            ));
            report.trades.push(trade);
        }
        let mut positions: Vec<_> = state.positions.values().collect();
        positions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        for position in positions {
            let Some(Trip {
                side,
                opened: Some(opened),
                fees,
                ..
            }) = state.trips.get(&position.symbol)
            else {
                continue;
            };
            let mut trade = StrategyTrade::new(
                *side,
                position.quantity.abs(),
                *opened,
                position.average_price,
            );
            trade.fees = *fees;
            trade.label = Some(position.symbol.clone());
            report.trades.push(trade);
        }
        report.equity = state.equity.clone();
        report
    }

    /// The account as [`restore`](Self::restore) reads it back.
    ///
    /// ```
    /// use egui_charts::services::{PaperAccount, SavedPaperAccount};
    ///
    /// let paper = PaperAccount::new("USD", 100_000.0).commission_per_order(1.0);
    /// let json = serde_json::to_string(&paper.save()).unwrap();
    /// // At the next launch:
    /// let saved: SavedPaperAccount = serde_json::from_str(&json).unwrap();
    /// let paper = PaperAccount::restore(saved).expect("a finite balance");
    /// paper.set_price("AAPL", 187.5);
    /// ```
    pub fn save(&self) -> SavedPaperAccount {
        let state = self.lock();
        let mut rules: Vec<_> = state.rules.values().cloned().collect();
        rules.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        let mut positions: Vec<_> = state.positions.values().cloned().collect();
        positions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        let mut trips: Vec<_> = state
            .trips
            .iter()
            .map(|(symbol, trip)| (symbol.clone(), trip.clone()))
            .collect();
        trips.sort_by(|a, b| a.0.cmp(&b.0));
        let exits = state
            .working
            .iter()
            .filter_map(|order| Some((order.id.clone(), state.exits.get(&order.id)?.clone())))
            .collect();
        let mut trails: Vec<_> = state
            .working
            .iter()
            .filter_map(|order| Some((order.id.clone(), *state.trails.get(&order.id)?)))
            .collect();
        trails.sort_by(|a, b| a.0.0.cmp(&b.0.0));
        let paired = state
            .working
            .iter()
            .filter(|order| state.paired.contains(&order.id))
            .map(|order| order.id.clone())
            .collect();
        SavedPaperAccount {
            currency: self.currency.clone(),
            initial: self.initial,
            leverage: self.leverage,
            commission: self.commission,
            commission_per_order: self.commission_per_order,
            commission_per_unit: self.commission_per_unit,
            rules,
            balance: state.balance,
            next_id: state.next_id,
            working: state.working.clone(),
            exits,
            positions,
            finished: state.finished.clone(),
            executions: state.executions.clone(),
            balance_history: state.balance_history.clone(),
            journal: state.journal.clone(),
            trips,
            closed_positions: state.closed_positions.clone(),
            trails,
            paired,
        }
    }

    /// The account `saved` describes, with its settings, symbol rules, balance, working orders and
    /// their exits, positions and history, and order ids that continue from it. Feed the prices of
    /// its symbols with [`set_price`](Self::set_price): until then its orders cannot be placed or
    /// changed, and the first price fills the working orders it reaches. Settings out of range
    /// count as their builders say, rules that are not valid are left out, and only the newest
    /// 1,000 entries of each history are kept. Positions and account figures retain their saved
    /// marks until a host price arrives. Returns `None` when the saved balance or starting balance
    /// is not finite. Older saves without round-trip tracking start it from the saved position's
    /// quantity and average price, with no reported opening time, earlier partial closes or
    /// commissions, and saves without its commissions count those of later fills only. In older
    /// saves, the exits of a take-profit level with its own stop loss are not paired, so each is
    /// cancelled on its own.
    pub fn restore(mut saved: SavedPaperAccount) -> Option<Self> {
        if !saved.initial.is_finite() || !saved.balance.is_finite() {
            return None;
        }
        let paper = Self::new(saved.currency, saved.initial)
            .leverage(saved.leverage)
            .commission(saved.commission)
            .commission_per_order(saved.commission_per_order)
            .commission_per_unit(saved.commission_per_unit);
        for rules in saved.rules {
            paper.set_rules(rules);
        }
        saved
            .finished
            .drain(..saved.finished.len().saturating_sub(HISTORY));
        saved
            .executions
            .drain(..saved.executions.len().saturating_sub(HISTORY));
        saved
            .closed_positions
            .drain(..saved.closed_positions.len().saturating_sub(HISTORY));
        saved
            .balance_history
            .drain(..saved.balance_history.len().saturating_sub(HISTORY));
        saved
            .journal
            .drain(..saved.journal.len().saturating_sub(HISTORY));
        {
            let mut state = paper.lock();
            state.balance = saved.balance;
            state.next_id = saved.next_id;
            state.working = saved.working;
            state.exits = saved.exits.into_iter().collect();
            state.trails = saved
                .trails
                .into_iter()
                .filter(|(id, _)| state.working.iter().any(|order| order.id == *id))
                .collect();
            state.paired = saved
                .paired
                .into_iter()
                .filter(|id| state.working.iter().any(|order| order.id == *id))
                .collect();
            state.trips = saved.trips.into_iter().collect();
            for position in &saved.positions {
                // Earlier saves have no trip totals or opening time; retain their known cost basis.
                state
                    .trips
                    .entry(position.symbol.clone())
                    .or_insert_with(|| Trip {
                        side: if position.quantity > 0.0 {
                            OrderSide::Buy
                        } else {
                            OrderSide::Sell
                        },
                        opened: None,
                        entered: position.quantity.abs(),
                        entry_value: position.quantity.abs() * position.average_price,
                        exited: 0.0,
                        exit_value: 0.0,
                        realized: 0.0,
                        fees: 0.0,
                    });
            }
            state.positions = saved
                .positions
                .into_iter()
                .map(|p| (p.symbol.clone(), p))
                .collect();
            state.finished = saved.finished;
            state.executions = saved.executions;
            state.closed_positions = saved.closed_positions;
            state.balance_history = saved.balance_history;
            state.journal = saved.journal;
        }
        Some(paper)
    }

    /// Returns the account to its starting balance without orders, positions or history. It keeps
    /// its settings and symbol rules, the host's prices, quotes and replay clock, and the count its
    /// order ids continue from. It ends the event stream of the last [`connect`](Broker::connect),
    /// so the terminal connects again at its next poll and shows the fresh account; a host that
    /// reads the stream itself calls `connect` again once [`Events::is_active`] is false.
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

    fn last_change(&self) -> Option<DateTime<Utc>> {
        // Every stamped change writes a journal entry at the same time.
        self.journal.last().map(|entry| entry.time)
    }

    /// Marks the equity at the replay time, replacing marks at or after it, which a move back
    /// over bars without changes leaves behind. Live time marks nothing.
    fn mark(&mut self, terms: &PaperAccount) {
        let Some(time) = self.replay_time else {
            return;
        };
        let (equity, _) = self.equity_and_margin(terms);
        while self.equity.last().is_some_and(|&(at, _)| at >= time) {
            self.equity.pop();
        }
        self.equity.push((time, equity));
    }

    fn push(&self, event: AccountEvent) {
        if let Some(events) = &self.events {
            events.push(event);
        }
    }

    fn log(&mut self, text: String) {
        let entry = JournalEntry::new(self.now(), text);
        self.push(AccountEvent::Journal(entry.clone()));
        if self.journal.len() == HISTORY {
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
        if self.balance_history.len() == HISTORY {
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
            .unwrap_or(position.last_price.unwrap_or(position.average_price));
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
                trailing_stop: true,
                brackets: true,
                take_profit_levels: true,
                take_profit_level_stops: true,
                max_take_profit_levels: Some(MAX_TAKE_PROFIT_LEVELS),
                close: true,
                partial_close: true,
                reverse: true,
                position_exits: true,
                preview: false,
                modify: true,
            },
        );
        let (equity, margin) = self.equity_and_margin(terms);
        let (orders_margin, commissions) = self.reservations(terms, None);
        account.equity = Some(equity);
        account.realized_pnl = Some(self.balance - terms.initial);
        account.margin = Some(margin);
        account.margin_buffer = Some(equity - margin);
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
                .unwrap_or(position.last_price.unwrap_or(position.average_price));
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

    /// Where `request` fills now at the last price `last`: a market order at the quote's ask for a
    /// buy and bid for a sell, else at `last`; a limit or stop order at `last` once `last` reaches
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
        // A fill away from last changes equity as well as the margin it holds.
        let spread = if immediate {
            (price - last) * signed * pv
        } else {
            0.0
        };
        let needed =
            margin + reserved + commissions + terms.commission_of(request.quantity, value) + spread;
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
        self.trails.remove(&order.id);
        self.paired.remove(&order.id);
        order.status = status;
        order.closed = Some(self.now());
        self.push(AccountEvent::Order(order.clone()));
        if self.finished.len() == HISTORY {
            self.finished.remove(0);
        }
        self.finished.push(order);
    }

    /// Follows an opening, adding or reducing fill and its commission through its round trip. The
    /// rounded remaining position decides when to finish the trip and whether the fill also opens
    /// the next one, which then takes the share of the commission for the quantity it opens.
    fn track(
        &mut self,
        request: &OrderRequest,
        held: f64,
        price: f64,
        realized: f64,
        remaining: f64,
        commission: f64,
    ) -> Option<ClosedPosition> {
        let now = self.now();
        let open = |quantity: f64, fees: f64| Trip {
            side: request.side,
            opened: Some(now),
            entered: quantity,
            entry_value: quantity * price,
            exited: 0.0,
            exit_value: 0.0,
            realized: 0.0,
            fees,
        };
        if held == 0.0 {
            self.trips
                .insert(request.symbol.clone(), open(request.quantity, commission));
            return None;
        }
        let symbol = request.symbol.as_str();
        if request.side.reduces(held) {
            let mut trip = self.trips.remove(symbol)?;
            let closed = request.quantity.min(held.abs());
            let reversed = remaining != 0.0 && (remaining > 0.0) != (held > 0.0);
            let closing = if reversed {
                commission * closed / request.quantity
            } else {
                commission
            };
            trip.exited += closed;
            trip.exit_value += closed * price;
            trip.realized += realized;
            trip.fees += closing;
            if remaining != 0.0 && !reversed {
                self.trips.insert(symbol.to_owned(), trip);
                return None;
            }
            if reversed {
                self.trips.insert(
                    symbol.to_owned(),
                    open(remaining.abs(), commission - closing),
                );
            }
            let mut closed = ClosedPosition::new(
                symbol,
                trip.side,
                trip.exited,
                trip.entry_value / trip.entered,
                trip.exit_value / trip.exited,
                trip.realized,
                now,
            );
            closed.opened = trip.opened;
            closed.commission = Some(trip.fees);
            Some(closed)
        } else {
            if let Some(trip) = self.trips.get_mut(symbol) {
                trip.entered += request.quantity;
                trip.entry_value += request.quantity * price;
                trip.fees += commission;
            }
            None
        }
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
        let commission = terms.commission_of(request.quantity, request.quantity * price.abs() * pv);
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
        let closed = self.track(
            &request,
            held,
            price,
            realized,
            position.quantity,
            commission,
        );
        self.value(terms, &mut position);
        if position.quantity != 0.0 {
            self.positions
                .insert(request.symbol.clone(), position.clone());
        }

        order.filled = request.quantity;
        order.fill_price = Some(price);
        let mut execution = Execution::new(
            order.id.clone(),
            self.now(),
            request.side,
            price,
            request.quantity,
        );
        execution.symbol = Some(request.symbol.clone());
        let id = order.id.clone();
        self.finish(order, OrderStatus::Filled);
        self.push(AccountEvent::Execution(execution.clone()));
        if self.executions.len() == HISTORY {
            self.executions.remove(0);
        }
        self.executions.push(execution);
        self.push(AccountEvent::Position(position.clone()));
        if let Some(closed) = closed {
            self.push(AccountEvent::ClosedPosition(closed.clone()));
            if self.closed_positions.len() == HISTORY {
                self.closed_positions.remove(0);
            }
            self.closed_positions.push(closed);
        }
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
        self.book(-commission, format!("Commission for order {id}"));
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
        // Levels close their own quantity, the first ones first when less than the order
        // opened. Own stops close the same share; the order's stop closes the other shares.
        let mut left = protected;
        let mut shares: Vec<_> = request
            .take_profit_levels
            .iter()
            .map(|level| {
                let share = level.quantity.min(left);
                left -= share;
                (level.price, share, level.stop_loss)
            })
            .collect();
        if let Some(price) = request.take_profit {
            shares.push((price, protected, None));
        }
        let shared_stop = if request.take_profit_levels.is_empty() {
            protected
        } else {
            shares
                .iter()
                .filter(|(.., stop)| stop.is_none())
                .map(|(_, share, _)| share)
                .sum()
        };
        let tolerance = rounding_tolerance(protected, 1e-9);
        // Only exits in the same group act on each other. None is the order's shared stop.
        let mut planned = Vec::new();
        for (index, &(price, share, stop)) in shares.iter().enumerate() {
            if share > tolerance {
                planned.push((exit(price, share, OrderType::Limit), stop.map(|_| index)));
            }
        }
        for (index, &(_, share, stop)) in shares.iter().enumerate() {
            if let Some(stop) = stop
                && share > tolerance
            {
                planned.push((exit(stop, share, OrderType::Stop), Some(index)));
            }
        }
        if let Some(price) = request.stop_loss
            && shared_stop > tolerance
        {
            planned.push((exit(price, shared_stop, OrderType::Stop), None));
        }
        let exits: Vec<_> = planned
            .into_iter()
            .map(|(exit, group)| {
                let mut order = Order::new(self.id(), exit, OrderStatus::Working);
                order.placed = Some(self.now());
                (order, group)
            })
            .collect();
        let partners: Vec<Vec<_>> = exits
            .iter()
            .map(|(exit, group)| {
                exits
                    .iter()
                    .filter(|(other, other_group)| {
                        group == other_group
                            && exit.request.position_bracket != other.request.position_bracket
                    })
                    .map(|(other, _)| other.id.clone())
                    .collect()
            })
            .collect();
        for ((exit, group), partners) in exits.into_iter().zip(partners) {
            if group.is_some() {
                self.paired.insert(exit.id.clone());
            }
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
        self.mark(terms);
    }

    /// Cancels the working order `id`; for an exit of a take-profit level with its own stop
    /// loss, the level's other exit too. Returns whether `id` was working.
    fn cancel_working(&mut self, id: &OrderId) -> bool {
        let Some(index) = self.working.iter().position(|order| &order.id == id) else {
            return false;
        };
        let order = self.working.remove(index);
        let partners = self.exits.remove(id).unwrap_or_default();
        for other in &partners {
            if let Some(list) = self.exits.get_mut(other) {
                list.retain(|exit| exit != id);
            }
        }
        let paired = self.paired.contains(id);
        self.log(format!(
            "Order {id} cancelled: {}",
            describe(&order.request)
        ));
        self.finish(order, OrderStatus::Cancelled);
        if paired {
            for other in &partners {
                self.cancel_working(other);
            }
        }
        true
    }

    /// Moves the trailing stop of `self.working[index]` with `price`: records its best price
    /// and sets [`Order::trailing_stop`]. Returns whether the stop changed. Other orders are
    /// left as they are.
    fn trail(&mut self, index: usize, price: f64) -> bool {
        let order = &mut self.working[index];
        let best = self.trails.get(&order.id).copied();
        let Some((best, stop)) = order
            .request
            .order_type
            .trail(order.request.side, best, price)
        else {
            return false;
        };
        self.trails.insert(order.id.clone(), best);
        let stop = stop.is_finite().then_some(stop);
        let changed = order.trailing_stop != stop;
        order.trailing_stop = stop;
        changed
    }

    /// Fills the working orders of `symbol` that `price` reached.
    fn reach(&mut self, terms: &PaperAccount, symbol: &str, price: f64) {
        let mut index = 0;
        while index < self.working.len() {
            let order = &self.working[index];
            let fill = if order.request.symbol != symbol {
                None
            } else if let OrderType::TrailingStop { .. } = order.request.order_type {
                if self.trail(index, price) {
                    self.push(AccountEvent::Order(self.working[index].clone()));
                }
                let order = &self.working[index];
                order.trailing_stop.and_then(|stop| {
                    OrderType::Stop(stop).fill_price(order.request.side, price, price)
                })
            } else {
                order
                    .request
                    .order_type
                    .fill_price(order.request.side, price, price)
            };
            let order = &mut self.working[index];
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
        OrderType::TrailingStop {
            distance,
            activation,
        } if distance.is_valid() && activation.is_none_or(f64::is_finite) => {
            activation.unwrap_or(last)
        }
        OrderType::TrailingStop { .. } => f64::NAN,
    };
    if matches!(order.order_type, OrderType::TrailingStop { .. })
        && (order.take_profit.is_some()
            || !order.take_profit_levels.is_empty()
            || order.stop_loss.is_some())
    {
        return Err(FeedError::Other(
            "a trailing stop takes no take profit or stop loss".into(),
        ));
    }
    let buy = order.side == OrderSide::Buy;
    let no_profit = |p: f64| !p.is_finite() || if buy { p <= price } else { p >= price };
    let no_loss = |p: f64| !p.is_finite() || if buy { p >= price } else { p <= price };
    let levels = &order.take_profit_levels;
    let bad_levels = !levels.is_empty()
        && (order.take_profit.is_some()
            || levels.iter().any(|level| {
                no_profit(level.price)
                    || !level.quantity.is_finite()
                    || level.quantity <= 0.0
                    || level.stop_loss.is_some_and(no_loss)
            })
            || (levels.iter().map(|level| level.quantity).sum::<f64>() - order.quantity).abs()
                > rounding_tolerance(order.quantity, 1e-9))
        || levels.len() > MAX_TAKE_PROFIT_LEVELS;
    if !order.quantity.is_finite()
        || order.quantity <= 0.0
        || !price.is_finite()
        || !(order.quantity * price * point_value).is_finite()
        || order.take_profit.is_some_and(no_profit)
        || bad_levels
        || order.stop_loss.is_some_and(no_loss)
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
        for closed in &state.closed_positions {
            state.push(AccountEvent::ClosedPosition(closed.clone()));
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
        let immediate = matches!(
            order.request.time_in_force,
            TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill
        );
        if order.request.order_type == OrderType::Market || immediate {
            match state.fills_at(&order.request, last) {
                Some(price) => state.fill(self, order, price),
                None => {
                    // Immediate orders that cannot fill now are cancelled, never left working.
                    state.log(format!(
                        "Order {} cancelled: {}",
                        order.id,
                        describe(&order.request)
                    ));
                    state.finish(order, OrderStatus::Cancelled);
                    state.push(state.account(self));
                }
            }
        } else {
            let symbol = order.request.symbol.clone();
            state.working.push(order);
            // The first event already carries the stop of a trailing stop that trails at once.
            let index = state.working.len() - 1;
            state.trail(index, last);
            state.push(AccountEvent::Order(state.working[index].clone()));
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
        } else if (state.finished.iter().rev())
            .any(|order| order.id == id && order.status == OrderStatus::Cancelled)
        {
            // Already cancelled, as a level's other exit is with the first: nothing changes.
            reply.send(Ok(()));
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
        let Some(&last) = state.last.get(&order.symbol) else {
            return reply.send(Err(FeedError::SymbolNotFound(order.symbol)));
        };
        if let Err(error) = validate(&order, last, state.terms_of(self, &order.symbol).0) {
            return reply.send(Err(error));
        }
        if order.time_in_force != TimeInForce::GoodTillCancelled {
            return reply.send(Err(FeedError::NotSupported));
        }
        // A stop loss that closed less than its take profits would cancel them in full on filling.
        if order.position_bracket.is_some() && order.quantity != working.request.quantity {
            return reply.send(Err(FeedError::Other(
                "a position exit's quantity cannot change".into(),
            )));
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
        let index = state
            .working
            .iter()
            .position(|working| working.id == id)
            .unwrap();
        let trailing = matches!(order.order_type, OrderType::TrailingStop { .. });
        state.working[index].request = order;
        if trailing {
            // The best price so far is kept, so a new distance moves the stop from it at once.
            state.trail(index, last);
        } else {
            state.trails.remove(&id);
            state.working[index].trailing_stop = None;
        }
        let working = state.working[index].clone();
        let symbol = working.request.symbol.clone();
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
        expected.time_in_force = TIMES_IN_FORCE.to_vec();
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
        rules.time_in_force = vec![
            TimeInForce::GoodTillDate,
            TimeInForce::ImmediateOrCancel,
            TimeInForce::GoodTillCancelled,
        ];
        assert!(paper.set_rules(rules.clone()));
        rules.time_in_force = vec![
            TimeInForce::ImmediateOrCancel,
            TimeInForce::GoodTillCancelled,
        ];
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
        // At either side of the spread, 100 margin plus 0.10 unrealised loss needs 100.10.
        for side in [Buy, Sell] {
            let paper = PaperAccount::new("USD", 100.0);
            paper.set_price("SYM", 100.0);
            paper.set_bid_ask("SYM", Some(99.9), Some(100.1));
            place(
                &paper,
                OrderRequest::new("SYM", side, 1.0, OrderType::Market),
            )
            .unwrap();
            let state = paper.lock();
            assert!(state.executions.is_empty());
            assert!(matches!(state.finished[0].status, OrderStatus::Rejected(_)));
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

    /// A fixed saved record catches required fields that would prevent earlier accounts loading.
    #[test]
    fn a_saved_account_keeps_its_stored_format() {
        // Written once by save; keep this record unchanged when adding fields.
        let json = r#"{
          "currency": "USD",
          "initial": 10000.0,
          "leverage": 5.0,
          "commission": 0.001,
          "commission_per_order": 1.0,
          "commission_per_unit": 0.25,
          "rules": [
            {
              "symbol": "ES",
              "quantity_step": 0.25,
              "min_quantity": 0.25,
              "max_quantity": null,
              "point_value": 50.0,
              "pip_value": null,
              "time_in_force": [
                "GoodTillCancelled"
              ],
              "leverage": 20.0
            }
          ],
          "balance": 9988.5,
          "next_id": 4,
          "working": [
            {
              "id": "2",
              "request": {
                "symbol": "ES",
                "side": "Sell",
                "quantity": 2.0,
                "order_type": {
                  "Limit": 105.0
                },
                "take_profit": null,
                "take_profit_levels": [],
                "stop_loss": null,
                "time_in_force": "GoodTillCancelled",
                "position_bracket": "TakeProfit"
              },
              "status": "Working",
              "filled": 0.0,
              "fill_price": null,
              "placed": "2024-01-01T00:00:00Z",
              "closed": null
            },
            {
              "id": "3",
              "request": {
                "symbol": "ES",
                "side": "Sell",
                "quantity": 2.0,
                "order_type": {
                  "Stop": 95.0
                },
                "take_profit": null,
                "take_profit_levels": [],
                "stop_loss": null,
                "time_in_force": "GoodTillCancelled",
                "position_bracket": "StopLoss"
              },
              "status": "Working",
              "filled": 0.0,
              "fill_price": null,
              "placed": "2024-01-01T00:00:00Z",
              "closed": null
            },
            {
              "id": "4",
              "request": {
                "symbol": "ES",
                "side": "Buy",
                "quantity": 1.0,
                "order_type": {
                  "Limit": 90.0
                },
                "take_profit": 100.0,
                "take_profit_levels": [],
                "stop_loss": 80.0,
                "time_in_force": "GoodTillCancelled",
                "position_bracket": null
              },
              "status": "Working",
              "filled": 0.0,
              "fill_price": null,
              "placed": "2024-01-01T00:00:00Z",
              "closed": null
            }
          ],
          "exits": [
            [
              "2",
              [
                "3"
              ]
            ],
            [
              "3",
              [
                "2"
              ]
            ]
          ],
          "positions": [
            {
              "symbol": "ES",
              "quantity": 2.0,
              "average_price": 100.0,
              "unrealized_pnl": 100.0,
              "last_price": 101.0,
              "trade_value": 10000.0,
              "market_value": 10100.0,
              "leverage": 20.0,
              "margin": 505.0,
              "expiration": null
            }
          ],
          "finished": [
            {
              "id": "1",
              "request": {
                "symbol": "ES",
                "side": "Buy",
                "quantity": 2.0,
                "order_type": "Market",
                "take_profit": null,
                "take_profit_levels": [
                  {
                    "price": 105.0,
                    "quantity": 2.0,
                    "stop_loss": null
                  }
                ],
                "stop_loss": 95.0,
                "time_in_force": "GoodTillCancelled",
                "position_bracket": null
              },
              "status": "Filled",
              "filled": 2.0,
              "fill_price": 100.0,
              "placed": "2024-01-01T00:00:00Z",
              "closed": "2024-01-01T00:00:00Z"
            }
          ],
          "executions": [
            {
              "order": "1",
              "time": "2024-01-01T00:00:00Z",
              "side": "Buy",
              "price": 100.0,
              "quantity": 2.0
            }
          ],
          "balance_history": [
            {
              "time": "2024-01-01T00:00:00Z",
              "balance": 9988.5,
              "amount": -11.5,
              "action": "Commission for order 1"
            }
          ],
          "journal": [
            {
              "time": "2024-01-01T00:00:00Z",
              "text": "Order 1 placed: buy 2 ES at market"
            }
          ]
        }"#;
        let saved: SavedPaperAccount = serde_json::from_str(json).unwrap();
        let paper = PaperAccount::restore(saved).unwrap();
        let saved = paper.save();
        assert_eq!(saved.balance, 9_988.5);
        assert_eq!(saved.rules[0].point_value, 50.0);
        assert_eq!(
            saved
                .working
                .iter()
                .map(|order| (
                    order.id.clone(),
                    order.request.order_type,
                    order.request.quantity,
                    order.request.position_bracket,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    OrderId::new("2"),
                    OrderType::Limit(105.0),
                    2.0,
                    Some(PositionBracket::TakeProfit)
                ),
                (
                    OrderId::new("3"),
                    OrderType::Stop(95.0),
                    2.0,
                    Some(PositionBracket::StopLoss)
                ),
                (OrderId::new("4"), OrderType::Limit(90.0), 1.0, None),
            ]
        );
        assert_eq!(saved.working[2].request.take_profit, Some(100.0));
        assert_eq!(saved.working[2].request.stop_loss, Some(80.0));
        assert_eq!(
            saved.exits,
            vec![
                (OrderId::new("2"), vec![OrderId::new("3")]),
                (OrderId::new("3"), vec![OrderId::new("2")]),
            ]
        );
        let mut position = Position::new("ES", 2.0, 100.0);
        position.unrealized_pnl = Some(100.0);
        position.last_price = Some(101.0);
        position.trade_value = Some(10_000.0);
        position.market_value = Some(10_100.0);
        position.leverage = Some(20.0);
        position.margin = Some(505.0);
        assert_eq!(saved.positions, vec![position]);
        assert_eq!(
            (
                saved.finished.len(),
                saved.executions.len(),
                saved.balance_history.len(),
                saved.journal.len()
            ),
            (1, 1, 1, 1)
        );
        assert_eq!(
            saved.finished[0].request.take_profit_levels,
            vec![TakeProfitLevel::new(105.0, 2.0)]
        );
        paper.set_replay_time(DateTime::from_timestamp(1_704_067_200, 0));
        paper.set_price("ES", 101.0);
        assert_eq!(
            place(
                &paper,
                OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Market)
            ),
            Ok(OrderId::new("5"))
        );
        paper.set_price("ES", 104.0);
        place(
            &paper,
            OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Market),
        )
        .unwrap();
        let saved = paper.save();
        assert_eq!(saved.closed_positions.len(), 1);
        let closed = &saved.closed_positions[0];
        assert_eq!(closed.opened, None);
        assert_eq!(closed.side, OrderSide::Buy);
        assert_eq!(closed.quantity, 3.0);
        assert!((closed.average_price - 301.0 / 3.0).abs() < 1e-9);
        assert_eq!(closed.exit_price, 104.0);
        assert!((closed.realized_pnl - 550.0).abs() < 1e-9);

        // Written by a save with round trips but without their commissions, which restores the
        // open trip with none: closing it counts the commission paid since.
        let json = r#"{
          "currency": "USD",
          "initial": 10000.0,
          "leverage": 1.0,
          "commission": 0.001,
          "commission_per_order": 0.0,
          "commission_per_unit": 0.0,
          "rules": [],
          "balance": 10019.47,
          "next_id": 2,
          "working": [],
          "exits": [],
          "positions": [
            {
              "symbol": "SYM",
              "quantity": -1.0,
              "average_price": 110.0,
              "unrealized_pnl": -0.0,
              "last_price": 110.0,
              "trade_value": 110.0,
              "market_value": 110.0,
              "leverage": 1.0,
              "margin": 110.0,
              "expiration": null
            }
          ],
          "finished": [
            {
              "id": "1",
              "request": {
                "symbol": "SYM",
                "side": "Buy",
                "quantity": 2.0,
                "order_type": "Market",
                "take_profit": null,
                "take_profit_levels": [],
                "stop_loss": null,
                "time_in_force": "GoodTillCancelled",
                "position_bracket": null
              },
              "status": "Filled",
              "filled": 2.0,
              "fill_price": 100.0,
              "placed": "2024-01-01T00:01:00Z",
              "closed": "2024-01-01T00:01:00Z"
            },
            {
              "id": "2",
              "request": {
                "symbol": "SYM",
                "side": "Sell",
                "quantity": 3.0,
                "order_type": "Market",
                "take_profit": null,
                "take_profit_levels": [],
                "stop_loss": null,
                "time_in_force": "GoodTillCancelled",
                "position_bracket": null
              },
              "status": "Filled",
              "filled": 3.0,
              "fill_price": 110.0,
              "placed": "2024-01-01T00:02:00Z",
              "closed": "2024-01-01T00:02:00Z"
            }
          ],
          "executions": [
            {
              "order": "1",
              "time": "2024-01-01T00:01:00Z",
              "side": "Buy",
              "price": 100.0,
              "quantity": 2.0
            },
            {
              "order": "2",
              "time": "2024-01-01T00:02:00Z",
              "side": "Sell",
              "price": 110.0,
              "quantity": 3.0
            }
          ],
          "balance_history": [
            {
              "time": "2024-01-01T00:01:00Z",
              "balance": 9999.8,
              "amount": -0.2,
              "action": "Commission for order 1"
            },
            {
              "time": "2024-01-01T00:02:00Z",
              "balance": 10019.8,
              "amount": 20.0,
              "action": "Profit or loss of 2 SYM closed by order 2"
            },
            {
              "time": "2024-01-01T00:02:00Z",
              "balance": 10019.47,
              "amount": -0.33,
              "action": "Commission for order 2"
            }
          ],
          "journal": [
            {
              "time": "2024-01-01T00:01:00Z",
              "text": "Order 1 placed: buy 2 SYM at market"
            },
            {
              "time": "2024-01-01T00:01:00Z",
              "text": "Order 1 filled: buy 2 SYM at market at 100"
            },
            {
              "time": "2024-01-01T00:02:00Z",
              "text": "Order 2 placed: sell 3 SYM at market"
            },
            {
              "time": "2024-01-01T00:02:00Z",
              "text": "Order 2 filled: sell 3 SYM at market at 110"
            }
          ],
          "trips": [
            [
              "SYM",
              {
                "side": "Sell",
                "opened": "2024-01-01T00:02:00Z",
                "entered": 1.0,
                "entry_value": 110.0,
                "exited": 0.0,
                "exit_value": 0.0,
                "realized": 0.0
              }
            ]
          ],
          "closed_positions": [
            {
              "symbol": "SYM",
              "side": "Buy",
              "quantity": 2.0,
              "average_price": 100.0,
              "exit_price": 110.0,
              "realized_pnl": 20.0,
              "opened": "2024-01-01T00:01:00Z",
              "closed": "2024-01-01T00:02:00Z"
            }
          ]
        }"#;
        let paper = PaperAccount::restore(serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(paper.save().closed_positions[0].commission, None);
        paper.set_price("SYM", 100.0);
        place(
            &paper,
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market),
        )
        .unwrap();
        let saved = paper.save();
        let closed = &saved.closed_positions[1];
        assert_eq!(closed.opened, "2024-01-01T00:02:00Z".parse().ok());
        assert_eq!((closed.side, closed.realized_pnl), (OrderSide::Sell, 10.0));
        assert_eq!(closed.commission, Some(0.1));
    }

    /// A JSON round trip preserves terms, history, working exits and the next id. Restored orders
    /// refuse changes before host prices arrive; a filled take profit still cancels its stop loss.
    #[test]
    fn a_saved_account_restores_its_state_and_settings() {
        let time = DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let paper = PaperAccount::new("USD", 10_000.0)
            .leverage(5.0)
            .commission(0.001)
            .commission_per_order(1.0)
            .commission_per_unit(0.25);
        paper.set_replay_time(Some(time));
        let mut rules = TradingRules::new("ES");
        (
            rules.quantity_step,
            rules.min_quantity,
            rules.point_value,
            rules.leverage,
        ) = (0.25, 0.25, 50.0, Some(20.0));
        rules.time_in_force = vec![TimeInForce::GoodTillCancelled];
        assert!(paper.set_rules(rules));
        paper.set_price("ES", 100.0);
        // Keep one unit outside the bracket: its stop must cancel when the protected two close.
        place(
            &paper,
            OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Market),
        )
        .unwrap();
        let mut entry = OrderRequest::new("ES", OrderSide::Buy, 2.0, OrderType::Market);
        entry.take_profit = Some(105.0);
        entry.stop_loss = Some(95.0);
        place(&paper, entry).unwrap();
        let mut limit = OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(90.0));
        limit.time_in_force = TimeInForce::GoodTillCancelled;
        let id = place(&paper, limit.clone()).unwrap();
        paper.set_price("ES", 101.0);
        let json = serde_json::to_string(&paper.save()).unwrap();
        let saved: SavedPaperAccount = serde_json::from_str(&json).unwrap();
        let restored = PaperAccount::restore(saved).unwrap();
        assert_eq!(serde_json::to_string(&restored.save()).unwrap(), json);
        assert_eq!(snapshot(&restored), snapshot(&paper));
        let inbox = Arc::new(Inbox::default());
        limit.order_type = OrderType::Limit(91.0);
        restored.modify(id, limit, inbox.reply(0));
        assert_eq!(
            *inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<(), FeedError>>()
                .unwrap(),
            Err(FeedError::SymbolNotFound("ES".into()))
        );
        restored.set_replay_time(Some(time));
        restored.set_price("ES", 101.0);
        assert_eq!(snapshot(&restored), snapshot(&paper));
        assert_eq!(rules_of(&restored, "ES"), rules_of(&paper, "ES"));
        for account in [&paper, &restored] {
            account.set_price("ES", 106.0);
        }
        assert_eq!(snapshot(&restored), snapshot(&paper));
        let market = OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Market);
        assert_eq!(place(&restored, market.clone()), place(&paper, market));
        assert_eq!(snapshot(&restored), snapshot(&paper));

        // Edited settings follow the builders, invalid rules are ignored and histories remain
        // bounded.
        let mut saved = paper.save();
        saved.leverage = 0.0;
        saved.commission = -1.0;
        saved.commission_per_order = f64::INFINITY;
        saved.commission_per_unit = f64::NAN;
        saved.rules[0].point_value = 0.0;
        saved.finished = vec![saved.finished[0].clone(); HISTORY + 3];
        saved.executions = vec![saved.executions[0].clone(); HISTORY + 3];
        saved.balance_history = vec![saved.balance_history[0].clone(); HISTORY + 3];
        saved.journal = vec![saved.journal[0].clone(); HISTORY + 3];
        let restored = PaperAccount::restore(saved).unwrap();
        let saved = restored.save();
        assert_eq!(
            (
                saved.leverage,
                saved.commission,
                saved.commission_per_order,
                saved.commission_per_unit
            ),
            (1.0, 0.0, 0.0, 0.0)
        );
        assert!(saved.rules.is_empty());
        assert_eq!(
            [
                saved.finished.len(),
                saved.executions.len(),
                saved.balance_history.len(),
                saved.journal.len()
            ],
            [HISTORY; 4]
        );
        for initial in [true, false] {
            let mut saved = paper.save();
            if initial {
                saved.initial = f64::NAN;
            } else {
                saved.balance = f64::INFINITY;
            }
            assert!(PaperAccount::restore(saved).is_none());
        }
    }

    #[test]
    fn closed_positions_span_a_round_trip_from_its_opening_fill_to_the_close_or_reversal() {
        let paper = PaperAccount::new("USD", 100_000.0);
        let mut restored = PaperAccount::restore(paper.save()).unwrap();
        let at = |minute: i64| DateTime::from_timestamp(1_704_067_200 + 60 * minute, 0).unwrap();
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new(&inbox, 2);
        paper.connect(events);
        use OrderSide::{Buy, Sell};
        for (minute, price, side, quantity) in [
            (0, 100.0, Buy, 2.0),
            (1, 110.0, Buy, 2.0),
            (2, 120.0, Sell, 1.0),
            (3, 90.0, Sell, 5.0),
            (4, 80.0, Buy, 2.0),
            (5, 100.0, Buy, 0.1),
            (6, 100.0, Buy, 0.2),
            (7, 100.0, Sell, 0.3),
        ] {
            for account in [&paper, &restored] {
                account.set_replay_time(Some(at(minute)));
                account.set_price("SYM", price);
                place(
                    account,
                    OrderRequest::new("SYM", side, quantity, OrderType::Market),
                )
                .unwrap();
            }
            assert_eq!(restored.save(), paper.save());
            let json = serde_json::to_string(&restored.save()).unwrap();
            restored = PaperAccount::restore(serde_json::from_str(&json).unwrap()).unwrap();
            assert_eq!(serde_json::to_string(&restored.save()).unwrap(), json);
        }
        let live = inbox
            .drain()
            .answers
            .into_iter()
            .map(|(_, event)| *event.downcast::<AccountEvent>().unwrap())
            .collect();
        for events in [live, snapshot(&paper), snapshot(&restored)] {
            let closed: Vec<_> = events
                .into_iter()
                .filter_map(|event| match event {
                    AccountEvent::ClosedPosition(closed) => Some(closed),
                    _ => None,
                })
                .collect();
            // Long: entries cost 420, exits return 120 + 270 = 390; short: 180 - 160.
            let expected = [
                (Buy, 4.0, 105.0, 97.5, -30.0, 0, 3),
                (Sell, 2.0, 90.0, 80.0, 20.0, 3, 4),
                (Buy, 0.3, 100.0, 100.0, 0.0, 5, 7),
            ];
            assert_eq!(closed.len(), expected.len());
            for (closed, (side, quantity, entry, exit, pnl, opened, end)) in
                closed.iter().zip(expected)
            {
                assert_eq!(closed.symbol, "SYM");
                assert_eq!(closed.side, side);
                assert_eq!(closed.opened, Some(at(opened)));
                assert_eq!(closed.closed, at(end));
                for (actual, expected) in [
                    (closed.quantity, quantity),
                    (closed.average_price, entry),
                    (closed.exit_price, exit),
                    (closed.realized_pnl, pnl),
                ] {
                    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
                }
            }
        }
        let mut saved = restored.save();
        saved.closed_positions = vec![saved.closed_positions[0].clone(); HISTORY + 3];
        let restored = PaperAccount::restore(saved).unwrap();
        assert_eq!(restored.save().closed_positions.len(), HISTORY);
    }

    /// Replay time cannot move before the account's last change, so fills stay in time order;
    /// after a reset, the account trades from the earlier bar alone.
    #[test]
    fn replay_history_stays_in_time_order_and_a_new_account_trades_from_the_earlier_bar() {
        use OrderSide::{Buy, Sell};
        let paper = PaperAccount::new("USD", 10_000.0);
        let bar = |n: i64| DateTime::from_timestamp(1_704_067_200 + n * 60, 0).unwrap();
        let trade = |side, price| {
            paper.set_price("SYM", price);
            place(
                &paper,
                OrderRequest::new("SYM", side, 1.0, OrderType::Market),
            )
            .unwrap();
        };
        let executions = || {
            snapshot(&paper)
                .into_iter()
                .filter_map(|event| match event {
                    AccountEvent::Execution(fill) => Some((fill.time, fill.side, fill.price)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(paper.set_replay_time(Some(bar(100))));
        trade(Buy, 100.0);
        assert!(paper.set_replay_time(Some(bar(120))));
        trade(Sell, 110.0);
        assert_eq!(paper.last_change(), Some(bar(120)));
        assert!(!paper.set_replay_time(Some(bar(90))));
        // A host that feeds the earlier bar anyway fills at the clock the account kept.
        trade(Buy, 95.0);
        assert_eq!(
            executions(),
            [
                (bar(100), Buy, 100.0),
                (bar(120), Sell, 110.0),
                (bar(120), Buy, 95.0)
            ]
        );
        assert!(paper.set_replay_time(Some(bar(120))));

        paper.reset();
        assert_eq!(paper.last_change(), None);
        assert!(paper.set_replay_time(Some(bar(90))));
        trade(Buy, 95.0);
        assert_eq!(executions(), [(bar(90), Buy, 95.0)]);
        let saved = paper.save();
        let held: Vec<_> = saved
            .positions
            .iter()
            .map(|position| (position.quantity, position.average_price))
            .collect();
        assert_eq!(held, [(1.0, 95.0)]);
        assert!(saved.closed_positions.is_empty());
        assert!(saved.journal.iter().all(|entry| entry.time == bar(90)));
    }

    /// A replay report keeps round trips net of fees, splits a reversal's commission by quantity
    /// and marks equity after fills, matching the hand-calculated trades and strategy figures.
    #[test]
    fn a_replay_report_matches_the_strategy_report_of_the_same_trades() {
        use OrderSide::{Buy, Sell};
        let paper = PaperAccount::new("USD", 10_000.0).commission(0.001);
        let bar = |n: i64| DateTime::from_timestamp(1_704_067_200 + n * 60, 0).unwrap();
        for (n, price, order) in [
            (1, 100.0, Some((Buy, 10.0))),
            (2, 105.0, Some((Sell, 4.0))),
            (3, 95.0, Some((Sell, 16.0))),
            (4, 90.0, Some((Buy, 10.0))),
            (5, 92.0, Some((Buy, 5.0))),
            (6, 91.0, None),
        ] {
            assert!(paper.set_replay_time(Some(bar(n))));
            paper.set_price("SYM", price);
            if let Some((side, quantity)) = order {
                place(
                    &paper,
                    OrderRequest::new("SYM", side, quantity, OrderType::Market),
                )
                .unwrap();
            }
        }
        let report = paper.report("Replay trading", "SYM", Timeframe::Min1);
        assert_eq!(report.initial_capital, 10_000.0);
        assert_eq!(report.currency.as_deref(), Some("USD"));
        assert!(report.properties.is_empty());
        let near = |actual: f64, expected: f64| {
            assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
        };
        // The long pays 1 + 0.42 and 6/16 of the reversal's 1.52; the short the rest and 0.9.
        let expected = [
            (Buy, 10.0, 1, 100.0, 1.99, Some((3, 99.0, -11.99))),
            (Sell, 10.0, 3, 95.0, 1.85, Some((4, 90.0, 48.15))),
            (Buy, 5.0, 5, 92.0, 0.46, None),
        ];
        let mut expected_report =
            StrategyReport::new("Replay trading", "SYM", Timeframe::Min1, 10_000.0);
        expected_report.currency = Some("USD".into());
        assert_eq!(report.trades.len(), expected.len());
        for (trade, (side, quantity, entry, price, fees, exit)) in
            report.trades.iter().zip(expected)
        {
            let mut expected_trade = StrategyTrade::new(side, quantity, bar(entry), price);
            expected_trade.fees = fees;
            expected_trade.label = Some("SYM".into());
            expected_trade.exit =
                exit.map(|(time, price, profit)| StrategyExit::new(bar(time), price, profit));
            expected_report.trades.push(expected_trade);
            assert_eq!(trade.side, side);
            assert_eq!(trade.entry_time, bar(entry));
            assert_eq!(trade.label.as_deref(), Some("SYM"));
            near(trade.quantity, quantity);
            near(trade.entry_price, price);
            near(trade.fees, fees);
            assert_eq!(trade.exit.is_some(), exit.is_some());
            if let (Some(actual), Some((time, price, profit))) = (&trade.exit, exit) {
                assert_eq!(actual.time, bar(time));
                near(actual.price, price);
                near(actual.profit, profit);
            }
        }
        // Bars 1 to 3 hold the equity after their fills, not before them.
        let equity = [9_999.0, 10_048.58, 9_987.06, 10_036.16, 10_035.7, 10_030.7];
        assert_eq!(report.equity.len(), equity.len());
        for (n, (&(time, actual), expected)) in (1..).zip(report.equity.iter().zip(equity)) {
            assert_eq!(time, bar(n));
            near(actual, expected);
        }
        expected_report.equity = (1..)
            .zip(equity)
            .map(|(n, equity)| (bar(n), equity))
            .collect();
        let expected_performance = expected_report.performance();
        let performance = report.performance();
        for (actual, expected) in [
            (&performance.all, &expected_performance.all),
            (&performance.long, &expected_performance.long),
            (&performance.short, &expected_performance.short),
        ] {
            assert_eq!(actual.closed_trades, expected.closed_trades);
            assert_eq!(actual.open_trades, expected.open_trades);
            assert_eq!(actual.winning_trades, expected.winning_trades);
            assert_eq!(actual.losing_trades, expected.losing_trades);
            near(actual.net_profit, expected.net_profit);
            near(actual.gross_profit, expected.gross_profit);
            near(actual.gross_loss, expected.gross_loss);
            near(actual.total_fees, expected.total_fees);
        }
        let all = &performance.all;
        assert_eq!(
            (
                all.closed_trades,
                all.open_trades,
                all.winning_trades,
                all.losing_trades
            ),
            (2, 1, 1, 1)
        );
        near(all.net_profit, 36.16);
        near(all.gross_profit, 48.15);
        near(all.gross_loss, 11.99);
        near(all.total_fees, 3.84);
        near(paper.save().balance, 10_035.7);
        near(all.net_profit - 0.46, paper.save().balance - 10_000.0);
        near(all.profit_factor.unwrap(), 48.15 / 11.99);
        near(all.win_rate.unwrap(), 0.5);
        near(performance.max_drawdown.unwrap(), 61.52);
        near(
            performance.max_drawdown.unwrap(),
            expected_performance.max_drawdown.unwrap(),
        );
        #[cfg(feature = "csv")]
        {
            let csv = report.trades_csv();
            let mut reader = csv::Reader::from_reader(csv.as_bytes());
            let headers = reader.headers().unwrap().clone();
            let column = |name| headers.iter().position(|h| h == name).unwrap();
            let rows: Vec<_> = reader.records().map(Result::unwrap).collect();
            assert_eq!(rows.len(), 3);
            for (row, side) in rows.iter().zip(["long", "short", "long"]) {
                assert_eq!(&row[column("side")], side);
            }
            for name in ["exit_time", "exit_price", "profit"] {
                assert!(rows[2][column(name)].is_empty());
            }
            for (row, profit) in rows.iter().zip([-11.99, 48.15]) {
                near(row[column("profit")].parse().unwrap(), profit);
            }
        }
    }

    /// A replay account books its own settings and leaves the live account untouched; invalid
    /// settings cannot reach the account constructor, which rejects non-finite capital.
    #[test]
    fn replay_account_settings_build_a_separate_account_that_books_and_reports_them() {
        let live = PaperAccount::new("USD", 100_000.0);
        live.set_price("SYM", 100.0);
        let before = live.save();
        let session = ReplayAccountSettings::new("EUR", 10_000.0, 0.001)
            .account()
            .unwrap();
        let bar = |n: i64| DateTime::from_timestamp(1_704_067_200 + n * 60, 0).unwrap();
        for (n, price, side) in [(1, 100.0, OrderSide::Buy), (2, 110.0, OrderSide::Sell)] {
            assert!(session.set_replay_time(Some(bar(n))));
            session.set_price("SYM", price);
            place(
                &session,
                OrderRequest::new("SYM", side, 10.0, OrderType::Market),
            )
            .unwrap();
        }
        let near = |actual: f64, expected: f64| {
            assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
        };
        let saved = session.save();
        near(saved.balance, 10_097.9);
        assert_eq!(saved.balance_history.len(), 3);
        for (change, expected) in saved.balance_history.iter().zip([-1.0, 100.0, -1.1]) {
            near(change.amount, expected);
        }
        let report = session.report("Replay trading", "SYM", Timeframe::Min1);
        assert_eq!(report.currency.as_deref(), Some("EUR"));
        assert_eq!(report.initial_capital, 10_000.0);
        assert_eq!(report.trades.len(), 1);
        let trade = &report.trades[0];
        assert_eq!(
            (
                trade.side,
                trade.quantity,
                trade.entry_time,
                trade.entry_price
            ),
            (OrderSide::Buy, 10.0, bar(1), 100.0)
        );
        near(trade.fees, 2.1);
        let exit = trade.exit.as_ref().unwrap();
        assert_eq!((exit.time, exit.price), (bar(2), 110.0));
        near(exit.profit, 97.9);
        near(report.performance().all.net_profit, 97.9);
        assert_eq!(live.save(), before);
        for (currency, capital, commission) in [
            ("  ", 10_000.0, 0.001),
            ("USD", f64::NAN, 0.001),
            ("USD", f64::INFINITY, 0.001),
            ("USD", 0.0, 0.001),
            ("USD", -1.0, 0.001),
            ("USD", 10_000.0, -0.001),
            ("USD", 10_000.0, f64::NAN),
        ] {
            let settings = ReplayAccountSettings::new(currency, capital, commission);
            assert!(!settings.is_valid(), "{settings:?}");
            assert!(settings.account().is_none(), "{settings:?}");
        }
        let settings = ReplayAccountSettings::new(" usd ", 10_000.0, 0.0);
        assert_eq!(settings.account().unwrap().save().currency, "usd");
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
                near(account.margin_buffer.unwrap(), balance + open - margin);
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
                account.margin_buffer,
                Some(account.equity.unwrap() - account.margin.unwrap())
            );
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
        for (held, own_stop, entered, first, partly, second, last, position, balance) in [
            // 1 at 106 earns 6 and 2 at 94 lose 12.
            (
                0.0_f64,
                None,
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
                None,
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
                None,
                vec![limit(105.0, 1.0), limit(110.0, 1.0), stop(95.0, 2.0)],
                111.0,
                vec![],
                111.0,
                vec![],
                0.0,
                10_022.0,
            ),
            // 1 at 106 earns 6; the second level's stop sells 2 at 96, losing 8.
            (
                0.0,
                Some(97.0),
                vec![
                    limit(105.0, 1.0),
                    limit(110.0, 2.0),
                    stop(97.0, 2.0),
                    stop(95.0, 1.0),
                ],
                106.0,
                vec![limit(110.0, 2.0), stop(97.0, 2.0)],
                96.0,
                vec![],
                0.0,
                9_998.0,
            ),
            // The own stop cancels only its take profit: -8, then the shared stop loses 6.
            (
                0.0,
                Some(97.0),
                vec![
                    limit(105.0, 1.0),
                    limit(110.0, 2.0),
                    stop(97.0, 2.0),
                    stop(95.0, 1.0),
                ],
                96.0,
                vec![limit(105.0, 1.0), stop(95.0, 1.0)],
                94.0,
                vec![],
                0.0,
                9_986.0,
            ),
            // Closing a short opens only 2 units: each level and its stop receives 1.
            (
                -1.0,
                Some(97.0),
                vec![
                    limit(105.0, 1.0),
                    limit(110.0, 1.0),
                    stop(97.0, 1.0),
                    stop(95.0, 1.0),
                ],
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
            entry.take_profit_levels[1].stop_loss = own_stop;
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

    /// Cancelling either exit of a level with its own stop loss cancels the other and leaves the
    /// order's other exits as they were, also in a restored account; the order's stop loss and the
    /// levels without their own are cancelled alone. A regression lets the level's stop close a
    /// quantity the user took out of the plan, leaves its take profit unprotected, cancels the
    /// shared stop with a level that has none, or answers Cancel all's second cancel as refused.
    #[test]
    fn cancelling_an_exit_of_a_level_with_its_own_stop_cancels_the_level() {
        let limit = |price, quantity| (OrderType::Limit(price), quantity);
        let stop = |price, quantity| (OrderType::Stop(price), quantity);
        let entered = [
            limit(105.0, 1.0),
            limit(110.0, 2.0),
            stop(97.0, 2.0),
            stop(95.0, 1.0),
        ];
        // (cancelled, its level's other exit, exits left working)
        for (cancelled, partner, left) in [
            (
                limit(110.0, 2.0),
                Some(stop(97.0, 2.0)),
                vec![limit(105.0, 1.0), stop(95.0, 1.0)],
            ),
            (
                stop(97.0, 2.0),
                Some(limit(110.0, 2.0)),
                vec![limit(105.0, 1.0), stop(95.0, 1.0)],
            ),
            // The order's stop loss and the levels without their own are cancelled alone.
            (
                limit(105.0, 1.0),
                None,
                vec![limit(110.0, 2.0), stop(97.0, 2.0), stop(95.0, 1.0)],
            ),
            (
                stop(95.0, 1.0),
                None,
                vec![limit(105.0, 1.0), limit(110.0, 2.0), stop(97.0, 2.0)],
            ),
        ] {
            for restored in [false, true] {
                let mut paper = PaperAccount::new("USD", 10_000.0);
                paper.set_price("SYM", 100.0);
                let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 3.0, OrderType::Market);
                entry.take_profit_levels = vec![
                    TakeProfitLevel::new(105.0, 1.0),
                    TakeProfitLevel::new(110.0, 2.0),
                ];
                entry.take_profit_levels[1].stop_loss = Some(97.0);
                entry.stop_loss = Some(95.0);
                let entry = place(&paper, entry).unwrap();
                if restored {
                    paper = PaperAccount::restore(paper.save()).unwrap();
                }
                let exits = |status: OrderStatus| {
                    let state = paper.lock();
                    (state.working.iter().chain(&state.finished))
                        .filter(|order| order.status == status)
                        .map(|order| (order.request.order_type, order.request.quantity))
                        .collect::<Vec<_>>()
                };
                let id_of = |exit: (OrderType, f64)| {
                    let state = paper.lock();
                    let order = (state.working.iter())
                        .find(|order| (order.request.order_type, order.request.quantity) == exit);
                    order.unwrap().id.clone()
                };
                let cancel = |id| {
                    let inbox = Arc::new(Inbox::default());
                    paper.cancel(id, inbox.reply(0));
                    *inbox
                        .drain()
                        .answers
                        .pop()
                        .unwrap()
                        .1
                        .downcast::<Result<(), FeedError>>()
                        .unwrap()
                };
                assert_eq!(exits(OrderStatus::Working), entered, "restored {restored}");
                let partner_id = partner.map(id_of);
                assert_eq!(cancel(id_of(cancelled)), Ok(()), "restored {restored}");
                assert_eq!(exits(OrderStatus::Working), left, "restored {restored}");
                let gone: Vec<_> = [cancelled].into_iter().chain(partner).collect();
                assert_eq!(exits(OrderStatus::Cancelled), gone, "restored {restored}");
                // Cancel all also cancels the other exit, which the account has cancelled already.
                if let Some(id) = partner_id {
                    assert_eq!(cancel(id), Ok(()), "restored {restored}");
                    assert_eq!(exits(OrderStatus::Working), left, "restored {restored}");
                }
                // An order that filled is no longer working.
                assert!(cancel(entry).is_err(), "restored {restored}");
            }
        }
    }

    /// A position exit's price can move but its quantity cannot change, so its fill never cancels
    /// exits covering more than it closed. A regression lets a reduced stop loss fill and cancel
    /// its take profits in full, leaving part of the position without an exit, or refuses price
    /// changes too.
    #[test]
    fn a_position_exits_quantity_cannot_change_so_its_group_keeps_covering_the_rest() {
        let limit = |price, quantity| (OrderType::Limit(price), quantity);
        let stop = |price, quantity| (OrderType::Stop(price), quantity);
        let bracket = |entry: &mut OrderRequest| {
            entry.take_profit = Some(110.0);
            entry.stop_loss = Some(95.0);
        };
        let level_stop = |entry: &mut OrderRequest| {
            entry.take_profit_levels = vec![
                TakeProfitLevel::new(105.0, 1.0),
                TakeProfitLevel::new(110.0, 2.0),
            ];
            entry.take_profit_levels[1].stop_loss = Some(97.0);
            entry.stop_loss = Some(95.0);
        };
        // (entry's exits, exits working, exit changed, its moved price, fill price, position left,
        // exits left working)
        type Row = (
            fn(&mut OrderRequest),
            Vec<(OrderType, f64)>,
            (OrderType, f64),
            f64,
            f64,
            f64,
            Vec<(OrderType, f64)>,
        );
        let rows: [Row; 3] = [
            (
                bracket,
                vec![limit(110.0, 3.0), stop(95.0, 3.0)],
                stop(95.0, 3.0),
                96.0,
                94.0,
                0.0,
                vec![],
            ),
            (
                level_stop,
                vec![
                    limit(105.0, 1.0),
                    limit(110.0, 2.0),
                    stop(97.0, 2.0),
                    stop(95.0, 1.0),
                ],
                stop(97.0, 2.0),
                98.0,
                96.0,
                1.0,
                vec![limit(105.0, 1.0), stop(95.0, 1.0)],
            ),
            (
                |_| {},
                vec![stop(95.0, 3.0)],
                stop(95.0, 3.0),
                96.0,
                94.0,
                0.0,
                vec![],
            ),
        ];
        for (exits_of, entered, changed, moved, fill, position, left) in rows {
            let paper = PaperAccount::new("USD", 10_000.0);
            paper.set_price("SYM", 100.0);
            let mut entry = OrderRequest::new("SYM", OrderSide::Buy, 3.0, OrderType::Market);
            exits_of(&mut entry);
            place(&paper, entry.clone()).unwrap();
            // An entry without exits gets a stop loss placed on its position.
            if entry.stop_loss.is_none() {
                let mut exit =
                    OrderRequest::new("SYM", OrderSide::Sell, 3.0, OrderType::Stop(95.0));
                exit.position_bracket = Some(PositionBracket::StopLoss);
                place(&paper, exit).unwrap();
            }
            let exits = || {
                let state = paper.lock();
                (state.working.iter())
                    .map(|order| (order.request.order_type, order.request.quantity))
                    .collect::<Vec<_>>()
            };
            let request_of = |exit: (OrderType, f64)| {
                let state = paper.lock();
                let order = (state.working.iter())
                    .find(|order| (order.request.order_type, order.request.quantity) == exit);
                let order = order.unwrap();
                (order.id.clone(), order.request.clone())
            };
            let modify = |id, request| {
                let inbox = Arc::new(Inbox::default());
                paper.modify(id, request, inbox.reply(0));
                *inbox
                    .drain()
                    .answers
                    .pop()
                    .unwrap()
                    .1
                    .downcast::<Result<(), FeedError>>()
                    .unwrap()
            };
            assert_eq!(exits(), entered, "{entered:?}");
            let (id, mut request) = request_of(changed);
            request.order_type = OrderType::Stop(moved);
            assert_eq!(modify(id.clone(), request.clone()), Ok(()), "{entered:?}");
            let before = snapshot(&paper);
            request.quantity = 1.0;
            let refused = Err(FeedError::Other(
                "a position exit's quantity cannot change".into(),
            ));
            assert_eq!(modify(id, request), refused, "{entered:?}");
            assert_eq!(snapshot(&paper), before, "{entered:?}");
            paper.set_price("SYM", fill);
            let state = paper.lock();
            let quantity = state.positions.get("SYM").map_or(0.0, |p| p.quantity);
            assert_eq!(quantity, position, "{entered:?}");
            drop(state);
            assert_eq!(exits(), left, "{entered:?}");
        }
    }

    #[test]
    fn reconnect_keeps_recent_history_within_the_stream_limit() {
        let paper = PaperAccount::new("USD", 1_000_000.0).commission(0.001);
        paper.set_price("SYM", 100.0);
        for i in 0..2_002 {
            place(
                &paper,
                OrderRequest::new(
                    "SYM",
                    if i % 2 == 0 {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    },
                    1.0,
                    OrderType::Market,
                ),
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
                .filter(|e| matches!(e, AccountEvent::ClosedPosition(_)))
                .count(),
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
        let stopped = |mut level: TakeProfitLevel, stop| {
            level.stop_loss = Some(stop);
            level
        };
        for (levels, take_profit) in [
            (
                vec![level(105.0, 1.0), stopped(level(110.0, 2.0), 101.0)],
                None,
            ),
            (
                vec![level(105.0, 1.0), stopped(level(110.0, 2.0), f64::NAN)],
                None,
            ),
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
        let counted = PaperAccount::new("USD", 10_000.0);
        counted.set_price("SYM", 100.0);
        for (count, accepted) in [(4, true), (5, false)] {
            let mut request =
                OrderRequest::new("SYM", OrderSide::Buy, count as f64, OrderType::Market);
            request.take_profit_levels =
                (1..=count).map(|i| level(100.0 + i as f64, 1.0)).collect();
            assert_eq!(place(&counted, request).is_ok(), accepted, "{count} levels");
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

    /// With no partial fills, IOC and FOK either fill in full at placement or cancel without
    /// ever working. Session durations and duration changes to a working order stay unsupported.
    #[test]
    fn immediate_orders_fill_at_once_or_are_cancelled_and_session_orders_are_refused() {
        use OrderSide::{Buy, Sell};
        use TimeInForce::{
            Day, FillOrKill, GoodTillCancelled, GoodTillDate, ImmediateOrCancel, Month, Week,
        };
        let paper = PaperAccount::new("USD", 1_000.0);
        paper.set_price("SYM", 100.0);
        assert_eq!(
            rules_of(&paper, "SYM").time_in_force,
            [GoodTillCancelled, ImmediateOrCancel, FillOrKill]
        );
        let mut request = OrderRequest::new("SYM", Buy, 1.0, OrderType::Limit(90.0));
        request.time_in_force = GoodTillCancelled;
        let id = place(&paper, request.clone()).unwrap();
        let before = snapshot(&paper);
        let inbox = Arc::new(Inbox::default());
        for tif in [
            Day,
            Week,
            Month,
            GoodTillDate,
            ImmediateOrCancel,
            FillOrKill,
        ] {
            request.time_in_force = tif;
            request.expires =
                (tif == GoodTillDate).then(|| "2026-12-01T21:00:00Z".parse().unwrap());
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
        for (tif, kind, side, status) in [
            (
                ImmediateOrCancel,
                OrderType::Limit(101.0),
                Buy,
                Some(OrderStatus::Filled),
            ),
            (
                FillOrKill,
                OrderType::Limit(99.0),
                Sell,
                Some(OrderStatus::Filled),
            ),
            (
                ImmediateOrCancel,
                OrderType::Limit(99.0),
                Buy,
                Some(OrderStatus::Cancelled),
            ),
            (
                FillOrKill,
                OrderType::Limit(101.0),
                Sell,
                Some(OrderStatus::Cancelled),
            ),
            (
                ImmediateOrCancel,
                OrderType::Market,
                Buy,
                Some(OrderStatus::Filled),
            ),
            (
                FillOrKill,
                OrderType::Stop(99.0),
                Buy,
                Some(OrderStatus::Filled),
            ),
            (
                ImmediateOrCancel,
                OrderType::StopLimit {
                    stop: 99.0,
                    limit: 99.5,
                },
                Buy,
                Some(OrderStatus::Cancelled),
            ),
            (
                FillOrKill,
                OrderType::StopLimit {
                    stop: 99.0,
                    limit: 101.0,
                },
                Buy,
                Some(OrderStatus::Filled),
            ),
            (Day, OrderType::Market, Buy, None),
            (Week, OrderType::Market, Buy, None),
            (Month, OrderType::Market, Buy, None),
            (GoodTillDate, OrderType::Market, Buy, None),
        ] {
            let paper = PaperAccount::new("USD", 1_000.0);
            paper.set_price("SYM", 100.0);
            let mut request = OrderRequest::new("SYM", side, 1.0, kind);
            request.time_in_force = tif;
            request.expires =
                (tif == GoodTillDate).then(|| "2026-12-01T21:00:00Z".parse().unwrap());
            paper.place(request, inbox.reply(0));
            let result = *inbox
                .drain()
                .answers
                .pop()
                .unwrap()
                .1
                .downcast::<Result<OrderId, FeedError>>()
                .unwrap();
            let events = snapshot(&paper);
            if let Some(status) = status {
                let id = result.unwrap();
                let state = paper.lock();
                assert!(state.working.is_empty(), "{tif:?} {kind:?}");
                assert_eq!(state.finished.len(), 1);
                let order = &state.finished[0];
                assert_eq!((&order.id, &order.status), (&id, &status));
                if status == OrderStatus::Filled {
                    assert_eq!((order.filled, order.fill_price), (1.0, Some(100.0)));
                    assert_eq!(state.executions.len(), 1);
                } else {
                    assert_eq!((order.filled, order.fill_price), (0.0, None));
                    assert!(state.positions.is_empty() && state.executions.is_empty());
                }
            } else {
                assert_eq!(result, Err(FeedError::NotSupported));
                assert_eq!(events.len(), 1);
                assert!(matches!(events[0], AccountEvent::Account(_)));
            }
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

    /// The events the account pushed to `inbox` since the last call.
    fn pushed(inbox: &Inbox) -> Vec<AccountEvent> {
        inbox
            .drain()
            .answers
            .into_iter()
            .map(|(_, event)| *event.downcast::<AccountEvent>().unwrap())
            .collect()
    }

    /// The stop of order `id` the account last pushed among `events`, else `stop`, and the
    /// price of any fill.
    fn trail_of(id: &OrderId, events: Vec<AccountEvent>, stop: &mut Option<f64>) -> Option<f64> {
        let mut fill = None;
        for event in events {
            match event {
                AccountEvent::Order(order)
                    if order.id == *id && order.status == OrderStatus::Working =>
                {
                    *stop = order.trailing_stop;
                }
                AccountEvent::Execution(execution) => fill = Some(execution.price),
                _ => {}
            }
        }
        fill
    }

    /// A trailing stop follows the best last price from its activation, or from placement, by
    /// its distance in each unit, never back, and fills as a stop at the first price that
    /// reaches it. A new distance moves the stop at once from the best price so far, and a
    /// restored account keeps that best price.
    #[test]
    fn trailing_stops_trail_the_best_price_and_fill_at_their_stop() {
        use OrderSide::{Buy, Sell};
        use TrailDistance::{Percent, Price, Ticks};
        let trailing = |distance, activation| OrderType::TrailingStop {
            distance,
            activation,
        };
        let ticks = |ticks, tick_size| Ticks { ticks, tick_size };
        // The stops are those after placement and after each price but the last, which fills.
        for (side, distance, activation, start, prices, stops, fill) in [
            (
                Sell,
                Price(3.0),
                None,
                100.0,
                &[102.0, 101.0, 103.0, 100.5, 100.0][..],
                &[Some(97.0), Some(99.0), Some(99.0), Some(100.0), Some(100.0)][..],
                100.0,
            ),
            (
                Buy,
                Price(1.5),
                Some(97.0),
                100.0,
                &[99.0, 97.0, 96.0, 97.5],
                &[None, None, Some(98.5), Some(97.5)],
                97.5,
            ),
            (
                Buy,
                ticks(4, 0.25),
                Some(98.0),
                100.0,
                &[99.0, 98.0, 98.5, 97.0, 98.0],
                &[None, None, Some(99.0), Some(99.0), Some(98.0)],
                98.0,
            ),
            // 100.3 - 0.01 is 100.28999999999999 before it is settled, which 100.29 misses.
            (
                Sell,
                ticks(1, 0.01),
                None,
                100.3,
                &[100.29],
                &[Some(100.29)],
                100.29,
            ),
            (
                Sell,
                Percent(2.0),
                Some(105.0),
                100.0,
                &[104.0, 106.0, 110.0, 108.0, 107.8],
                &[None, None, Some(103.88), Some(107.8), Some(107.8)],
                107.8,
            ),
            (
                Sell,
                Percent(1.0),
                None,
                200.0,
                &[210.0, 208.0, 207.9],
                &[Some(198.0), Some(207.9), Some(207.9)],
                207.9,
            ),
            // A price beyond the stop fills there.
            (
                Buy,
                Price(2.0),
                None,
                100.0,
                &[103.0],
                &[Some(102.0)],
                103.0,
            ),
        ] {
            let paper = PaperAccount::new("USD", 100_000.0);
            let inbox = Arc::new(Inbox::default());
            let (_listener, events) = Listener::new(&inbox, 2);
            paper.connect(events);
            paper.set_price("SYM", start);
            pushed(&inbox);
            let request = OrderRequest::new("SYM", side, 1.0, trailing(distance, activation));
            let id = place(&paper, request).unwrap();
            let mut stop = None;
            let mut seen = Vec::new();
            let mut filled = trail_of(&id, pushed(&inbox), &mut stop);
            for &price in prices {
                seen.push(stop);
                paper.set_price("SYM", price);
                filled = filled.or(trail_of(&id, pushed(&inbox), &mut stop));
            }
            assert_eq!(seen, stops, "{side:?} {distance:?} from {activation:?}");
            assert_eq!(
                filled,
                Some(fill),
                "{side:?} {distance:?} from {activation:?}"
            );
        }

        // A sell trailing 3 from 100, moved by 104 to a stop of 101.
        let moved = || {
            let paper = PaperAccount::new("USD", 100_000.0);
            paper.set_price("SYM", 100.0);
            let request = OrderRequest::new("SYM", Sell, 1.0, trailing(Price(3.0), None));
            let id = place(&paper, request.clone()).unwrap();
            paper.set_price("SYM", 104.0);
            (paper, id, request)
        };
        let (paper, id, mut request) = moved();
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new(&inbox, 2);
        paper.connect(events);
        paper.set_price("SYM", 103.5);
        let mut stop = None;
        assert_eq!(trail_of(&id, pushed(&inbox), &mut stop), None);
        assert_eq!(stop, Some(101.0));
        request.order_type = trailing(Price(1.0), None);
        request.time_in_force = TimeInForce::GoodTillCancelled;
        let reply = Arc::new(Inbox::default());
        paper.modify(id.clone(), request, reply.reply(0));
        let answer = reply.drain().answers.pop().unwrap().1;
        assert_eq!(*answer.downcast::<Result<(), FeedError>>().unwrap(), Ok(()));
        assert_eq!(trail_of(&id, pushed(&inbox), &mut stop), None);
        assert_eq!(stop, Some(103.0), "from the best price of 104");
        paper.set_price("SYM", 103.0);
        assert_eq!(trail_of(&id, pushed(&inbox), &mut stop), Some(103.0));

        let restored = PaperAccount::restore(moved().0.save()).unwrap();
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new(&inbox, 2);
        restored.connect(events);
        restored.set_price("SYM", 103.0);
        let mut stop = None;
        assert_eq!(trail_of(&id, pushed(&inbox), &mut stop), None);
        assert_eq!(stop, Some(101.0), "from the restored best price of 104");
        restored.set_price("SYM", 101.0);
        assert_eq!(trail_of(&id, pushed(&inbox), &mut stop), Some(101.0));

        let base = OrderRequest::new("SYM", Sell, 1.0, trailing(Price(3.0), None));
        let paper = PaperAccount::new("USD", 100_000.0);
        paper.set_price("SYM", 100.0);
        for change in [
            "percent",
            "price",
            "not finite",
            "no ticks",
            "no tick size",
            "activation",
            "take profit",
            "stop loss",
        ] {
            let mut request = base.clone();
            match change {
                "percent" => request.order_type = trailing(Percent(100.0), None),
                "price" => request.order_type = trailing(Price(0.0), None),
                "not finite" => request.order_type = trailing(Price(f64::NAN), None),
                "no ticks" => request.order_type = trailing(ticks(0, 0.25), None),
                "no tick size" => request.order_type = trailing(ticks(1, 0.0), None),
                "activation" => request.order_type = trailing(Price(3.0), Some(f64::NAN)),
                "take profit" => request.take_profit = Some(90.0),
                _ => request.stop_loss = Some(110.0),
            }
            assert!(place(&paper, request).is_err(), "{change}");
        }
        assert!(place(&paper, base).is_ok());
    }

    /// The same price path gives a trailing stop the stop and fill of a backtest's trailing
    /// exit, for a long with an activation price and a short without one.
    #[cfg(feature = "strategies")]
    #[test]
    fn a_trailing_stop_fills_where_a_backtest_trailing_exit_does() {
        use crate::model::Bar;
        use crate::strategies::{Backtest, OrderSize, Strategy, StrategyContext};
        use OrderSide::{Buy, Sell};
        struct Trail(OrderSide, f64, f64);
        impl Strategy for Trail {
            fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
                if context.bars().len() == 1 {
                    context.entry("A", self.0, None, OrderType::Market);
                    context.exit_trailing("X", self.0, None, self.1, self.2);
                }
            }
        }
        let time = |n: i64| DateTime::from_timestamp(1_704_067_200 + n * 60, 0).unwrap();
        let mut backtest = Backtest::new("Trail", "SYM", Timeframe::Min1, 1_000.0);
        backtest.settings.order_size = OrderSize::Fixed(2.0);
        // Each path runs from the open through the nearer extreme, the other one, to the close.
        for (side, activation, offset, bars, path, stops, exit) in [
            (
                Buy,
                4.0,
                3.0,
                &[(100.0, 103.0, 99.0, 99.5), (99.5, 107.0, 99.0, 104.0)][..],
                &[99.0, 103.0, 99.5, 99.0, 107.0, 104.0][..],
                &[None, None, None, None, None, Some(104.0)][..],
                104.0,
            ),
            (
                Sell,
                0.0,
                2.0,
                &[(100.0, 101.0, 96.0, 98.0)],
                &[101.0, 96.0, 98.0],
                &[Some(102.0), Some(102.0), Some(98.0)],
                98.0,
            ),
        ] {
            let mut prices = vec![(100.0, 100.0, 100.0, 100.0)];
            prices.extend_from_slice(bars);
            let bars: Vec<_> = (0..)
                .zip(prices)
                .map(|(n, (open, high, low, close))| Bar::new(time(n), open, high, low, close, 1.0))
                .collect();
            let report = backtest.run(&mut Trail(side, activation, offset), &bars, &[]);
            let trade = &report.trades[0];
            assert_eq!(trade.entry_price, 100.0);
            assert_eq!(trade.exit.as_ref().map(|exit| exit.price), Some(exit));

            let paper = PaperAccount::new("USD", 1_000.0);
            let inbox = Arc::new(Inbox::default());
            let (_listener, events) = Listener::new(&inbox, 2);
            paper.connect(events);
            paper.set_price("SYM", 100.0);
            place(
                &paper,
                OrderRequest::new("SYM", side, 2.0, OrderType::Market),
            )
            .unwrap();
            let distance = TrailDistance::Price(offset);
            let activation = (activation > 0.0).then_some(100.0 + activation);
            let order_type = OrderType::TrailingStop {
                distance,
                activation,
            };
            let exit_side = if side == Buy { Sell } else { Buy };
            let id = place(&paper, OrderRequest::new("SYM", exit_side, 2.0, order_type));
            let id = id.unwrap();
            let (mut stop, mut seen, mut filled) = (None, Vec::new(), None);
            // These events also hold the market entry's fill.
            trail_of(&id, pushed(&inbox), &mut stop);
            for &price in path {
                seen.push(stop);
                paper.set_price("SYM", price);
                filled = filled.or(trail_of(&id, pushed(&inbox), &mut stop));
            }
            assert_eq!(seen, stops, "{side:?}");
            assert_eq!(filled, Some(exit), "{side:?}");
        }
    }
}
