//! Orders, positions and the account a broker reports.
//!
//! Prices are in the symbol's price units and quantities in its trading units. Every price and
//! quantity is finite. Quantities are positive, except a position's, which is signed, and an
//! order's filled quantity, which starts at 0.

use chrono::{DateTime, Utc};

/// Identifies an order. The broker chooses it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderId(
    /// The broker's own identifier for the order.
    pub String,
);

impl OrderId {
    /// Create a new order ID from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Buy or sell, shared by orders, executions, aggressor trades and strategy positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OrderSide {
    /// Buy side.
    Buy,
    /// Sell side.
    Sell,
}

impl OrderSide {
    /// Get display text
    pub fn label(&self) -> &'static str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }
}

/// How an order executes. Prices are in the symbol's price units.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum OrderType {
    /// At the best price available now.
    Market,
    /// At this price or better.
    Limit(f64),
    /// As a market order once the market trades at this price.
    Stop(f64),
    /// As a limit order at `limit` once the market trades at `stop`.
    StopLimit {
        /// Trigger price for the stop order.
        stop: f64,
        /// Limit price of the order after triggering.
        limit: f64,
    },
}

/// The role of an exit order attached to the net position in its symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionBracket {
    /// A profit-taking limit exit.
    TakeProfit,
    /// A protective stop exit.
    StopLoss,
}

/// An order as the user or the host asks for it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OrderRequest {
    /// The symbol as the feed knows it.
    pub symbol: String,
    /// Buy or sell.
    pub side: OrderSide,
    /// Positive, in the symbol's trading units.
    pub quantity: f64,
    /// How the order executes, with its prices.
    pub order_type: OrderType,
    /// The price at which a take-profit order closes the filled quantity, for an account that
    /// accepts [brackets](Capabilities::brackets): above the entry for a buy, below for a sell.
    pub take_profit: Option<f64>,
    /// The price at which a stop-loss order closes the filled quantity, for an account that
    /// accepts [brackets](Capabilities::brackets): below the entry for a buy, above for a sell.
    pub stop_loss: Option<f64>,
    /// How long the order stays working.
    pub time_in_force: TimeInForce,
    /// Explicit link to the net position in this symbol. Ordinary opposite-side orders are
    /// not brackets. Brokers should preserve this role on updates.
    pub position_bracket: Option<PositionBracket>,
}

impl OrderRequest {
    /// An order for `quantity` of `symbol`, without brackets, working for the day.
    pub fn new(
        symbol: impl Into<String>,
        side: OrderSide,
        quantity: f64,
        order_type: OrderType,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            quantity,
            order_type,
            take_profit: None,
            stop_loss: None,
            time_in_force: TimeInForce::Day,
            position_bracket: None,
        }
    }
}

/// How long an order stays working.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TimeInForce {
    /// Until the end of the trading day.
    #[default]
    Day,
    /// Until the end of the trading week.
    Week,
    /// Until the end of the trading month.
    Month,
    /// Until it fills or is cancelled.
    GoodTillCancelled,
    /// Fills what it can at once and cancels the rest.
    ImmediateOrCancel,
    /// Fills in full at once, or not at all.
    FillOrKill,
}

impl TimeInForce {
    /// The short name a ticket shows, such as "GTC".
    pub fn label(&self) -> &'static str {
        match self {
            Self::Day => "DAY",
            Self::Week => "WEEK",
            Self::Month => "MONTH",
            Self::GoodTillCancelled => "GTC",
            Self::ImmediateOrCancel => "IOC",
            Self::FillOrKill => "FOK",
        }
    }
}

/// An order the broker holds. Pushed again on every change; the terminal replaces the order with
/// the same id.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Order {
    /// The broker's id for the order: an event with the same id replaces it.
    pub id: OrderId,
    /// What was asked for.
    pub request: OrderRequest,
    /// Where the order stands.
    pub status: OrderStatus,
    /// The quantity filled so far, from 0 up to the requested quantity.
    pub filled: f64,
    /// The average price of the filled quantity, when the broker reports it.
    pub fill_price: Option<f64>,
    /// When the order was placed, when the broker reports it.
    pub placed: Option<DateTime<Utc>>,
    /// When the order filled in full, was cancelled or was rejected, when the broker reports it.
    pub closed: Option<DateTime<Utc>>,
}

impl Order {
    /// An order with nothing filled yet.
    pub fn new(id: OrderId, request: OrderRequest, status: OrderStatus) -> Self {
        Self {
            id,
            request,
            status,
            filled: 0.0,
            fill_price: None,
            placed: None,
            closed: None,
        }
    }
}

/// Where an order stands. `Filled`, `Cancelled` and `Rejected` end the order.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum OrderStatus {
    /// Accepted and waiting to fill, possibly partly filled.
    Working,
    /// Filled in full.
    Filled,
    /// Cancelled, possibly after a partial fill; `filled` says how much.
    Cancelled,
    /// Refused by the broker after it accepted the order and gave it an id, with its reason. An
    /// order refused before that is answered with an error from
    /// [`Broker::place`](crate::services::Broker::place) and never pushed.
    Rejected(String),
}

/// The net position in one symbol.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Position {
    /// The symbol as the feed knows it.
    pub symbol: String,
    /// In the symbol's trading units, positive when long, negative when short. 0 means flat and
    /// removes the position.
    pub quantity: f64,
    /// The average price the open quantity was entered at, in the symbol's price units.
    pub average_price: f64,
    /// The profit or loss at the current price in the account's currency, when the broker
    /// reports it.
    pub unrealized_pnl: Option<f64>,
    /// The price the position is valued at, when the broker reports it.
    pub last_price: Option<f64>,
    /// The open quantity at its average price in the account's currency, when the broker reports
    /// it.
    pub trade_value: Option<f64>,
    /// The open quantity at the last price in the account's currency, when the broker reports
    /// it.
    pub market_value: Option<f64>,
    /// How many times the margin the position's market value is, such as 100 for 100:1, when
    /// the broker reports it.
    pub leverage: Option<f64>,
    /// The margin the position holds in the account's currency, when the broker reports it.
    pub margin: Option<f64>,
    /// When the instrument expires, for futures and options.
    pub expiration: Option<DateTime<Utc>>,
}

impl Position {
    /// A position without a reported profit or loss.
    pub fn new(symbol: impl Into<String>, quantity: f64, average_price: f64) -> Self {
        Self {
            symbol: symbol.into(),
            quantity,
            average_price,
            unrealized_pnl: None,
            last_price: None,
            trade_value: None,
            market_value: None,
            leverage: None,
            margin: None,
            expiration: None,
        }
    }
}

/// One fill of an order.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Execution {
    /// The order this fill belongs to.
    pub order: OrderId,
    /// When the fill happened.
    pub time: DateTime<Utc>,
    /// The side of the order.
    pub side: OrderSide,
    /// The price of the fill, in the symbol's price units.
    pub price: f64,
    /// Positive, in the symbol's trading units.
    pub quantity: f64,
}

impl Execution {
    /// A fill of `quantity` at `price`.
    pub fn new(
        order: OrderId,
        time: DateTime<Utc>,
        side: OrderSide,
        price: f64,
        quantity: f64,
    ) -> Self {
        Self {
            order,
            time,
            side,
            price,
            quantity,
        }
    }
}

/// The account behind one [`Broker`](crate::services::Broker) value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Account {
    /// The currency balances are in, such as "USD".
    pub currency: String,
    /// The cash balance in `currency`.
    pub balance: f64,
    /// The balance plus the open positions' profit or loss, when the broker reports it.
    pub equity: Option<f64>,
    /// The profit or loss of the closed trades, net of commissions, when the broker reports it.
    pub realized_pnl: Option<f64>,
    /// The margin the open positions hold, when the broker reports it.
    pub margin: Option<f64>,
    /// The margin the working orders would hold once filled, when the broker reports it.
    pub orders_margin: Option<f64>,
    /// The equity less the margin of the positions and the working orders, when the broker
    /// reports it.
    pub available_funds: Option<f64>,
    /// The order types the account accepts.
    pub capabilities: Capabilities,
}

impl Account {
    /// An account without a reported equity.
    pub fn new(currency: impl Into<String>, balance: f64, capabilities: Capabilities) -> Self {
        Self {
            currency: currency.into(),
            balance,
            equity: None,
            realized_pnl: None,
            margin: None,
            orders_margin: None,
            available_funds: None,
            capabilities,
        }
    }
}

/// A change of the account's cash balance, such as a closed trade's profit or a commission.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BalanceChange {
    /// When the balance changed.
    pub time: DateTime<Utc>,
    /// The balance after the change, in the account's currency.
    pub balance: f64,
    /// The change, positive for a credit and negative for a debit.
    pub amount: f64,
    /// What changed it, such as "Commission for order 12".
    pub action: String,
}

impl BalanceChange {
    /// A change of `amount` that left the balance at `balance`.
    pub fn new(time: DateTime<Utc>, balance: f64, amount: f64, action: impl Into<String>) -> Self {
        Self {
            time,
            balance,
            amount,
            action: action.into(),
        }
    }
}

/// One line of the account's trading journal: an order placed, changed, cancelled or filled, a
/// position closed, or anything else the broker logs.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct JournalEntry {
    /// When it happened.
    pub time: DateTime<Utc>,
    /// What happened, as a sentence.
    pub text: String,
}

impl JournalEntry {
    /// An entry saying `text`.
    pub fn new(time: DateTime<Utc>, text: impl Into<String>) -> Self {
        Self {
            time,
            text: text.into(),
        }
    }
}

/// The order types the account accepts. The terminal hides the controls for the others. By
/// default the account accepts none; set the fields it accepts:
///
/// ```
/// use egui_charts::model::Capabilities;
///
/// let mut capabilities = Capabilities::default();
/// capabilities.market = true;
/// capabilities.limit = true;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// Accepts market orders.
    pub market: bool,
    /// Accepts limit orders.
    pub limit: bool,
    /// Accepts stop orders.
    pub stop: bool,
    /// Accepts stop-limit orders.
    pub stop_limit: bool,
    /// Accepts a take-profit and a stop-loss with an order.
    pub brackets: bool,
    /// Closes a position with an opposite market order, so the Close and Flatten controls work.
    pub close: bool,
    /// Reverses a position with an opposite market order of twice its size, so the Reverse
    /// controls work.
    pub reverse: bool,
    /// Answers [`Broker::preview`](crate::services::Broker::preview), so the ticket offers to
    /// preview an order before it is placed.
    pub preview: bool,
    /// Changes working orders through [`Broker::modify`](crate::services::Broker::modify), so
    /// their lines on the chart can be dragged.
    pub modify: bool,
}

impl Capabilities {
    /// Whether the account accepts orders of this type.
    pub fn accepts(&self, order_type: &OrderType) -> bool {
        match order_type {
            OrderType::Market => self.market,
            OrderType::Limit(_) => self.limit,
            OrderType::Stop(_) => self.stop,
            OrderType::StopLimit { .. } => self.stop_limit,
        }
    }
}

/// What a broker estimates for an order it has not accepted, from
/// [`Broker::preview`](crate::services::Broker::preview). Every estimate is optional.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct OrderPreview {
    /// The estimated cost of the order in the account's currency.
    pub cost: Option<f64>,
    /// The estimated commission in the account's currency.
    pub commission: Option<f64>,
    /// The margin the order would need in the account's currency.
    pub margin: Option<f64>,
    /// What the broker warns of, such as a position the order would reverse.
    pub warnings: Vec<String>,
}

/// How a symbol trades at the broker. The order ticket sizes orders by these rules.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TradingRules {
    /// The symbol as the feed knows it.
    pub symbol: String,
    /// Positive: quantities are whole multiples of it.
    pub quantity_step: f64,
    /// The smallest quantity, positive.
    pub min_quantity: f64,
    /// The largest quantity, if the broker limits it.
    pub max_quantity: Option<f64>,
    /// The profit or loss in the account's currency of one unit moving by one price unit: 1 for
    /// a share, the contract multiplier for a future.
    pub point_value: f64,
    /// The profit or loss in the account's currency of one unit moving by one pip. `None` when the
    /// broker reports none: the ticket then takes the instrument's
    /// [`pip_size`](crate::model::Instrument::pip_size) times
    /// [`point_value`](Self::point_value), and shows no pip value for an instrument without pips.
    pub pip_value: Option<f64>,
    /// The times in force the broker accepts for the symbol, the default first. Never empty.
    pub time_in_force: Vec<TimeInForce>,
    /// The trade value per unit of margin, such as 20 for 20:1, if the broker trades the symbol
    /// on margin. The order ticket estimates the margin an order needs from it.
    pub leverage: Option<f64>,
}

impl TradingRules {
    /// Whole units from 1 without a maximum, a point value of 1 and day orders: the rules the
    /// ticket assumes when the broker reports none.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            quantity_step: 1.0,
            min_quantity: 1.0,
            max_quantity: None,
            point_value: 1.0,
            pip_value: None,
            time_in_force: vec![TimeInForce::Day],
            leverage: None,
        }
    }
}
