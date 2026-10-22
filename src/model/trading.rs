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

/// Side of the order: buy or sell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    /// Long / buy order.
    Buy,
    /// Short / sell order.
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
    StopLimit { stop: f64, limit: f64 },
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
}

impl OrderRequest {
    /// An order for `quantity` of `symbol`.
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
}

impl Order {
    /// An order with nothing filled yet.
    pub fn new(id: OrderId, request: OrderRequest, status: OrderStatus) -> Self {
        Self {
            id,
            request,
            status,
            filled: 0.0,
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
}

impl Position {
    /// A position without a reported profit or loss.
    pub fn new(symbol: impl Into<String>, quantity: f64, average_price: f64) -> Self {
        Self {
            symbol: symbol.into(),
            quantity,
            average_price,
            unrealized_pnl: None,
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
            capabilities,
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
