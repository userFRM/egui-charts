//! Orders, positions and the account a broker reports.
//!
//! Prices are in the symbol's price units and quantities in its trading units. Every price and
//! quantity is finite. Quantities are positive, except a position's, which is signed, and an
//! order's filled quantity, which starts at 0.
//!
//! Fields added to the serializable trading types follow the compatibility promise on
//! [`SavedPaperAccount`](crate::services::SavedPaperAccount).

use chrono::{DateTime, Utc};

/// A relative bound at `scale` for floating-point comparisons.
/// Partial closes accumulate rounding, so quantities use a `1e-9` relative bound at the original
/// quantity's scale, or the larger of the held and filled quantities without that history.
/// Price-distance ties use four ulps for a single step.
pub(crate) fn rounding_tolerance(scale: f64, relative: f64) -> f64 {
    relative * scale.abs()
}

/// Identifies an order. The broker chooses it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

    /// Whether an exit on this side reduces `position`, a signed net quantity: not once the
    /// position is flat or on this side, as after a close or a reverse.
    pub(crate) fn reduces(self, position: f64) -> bool {
        position != 0.0 && (self == OrderSide::Buy) != (position > 0.0)
    }
}

/// How an order executes. Prices are in the symbol's price units.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// As a stop order whose stop follows the best price since it started trailing, at
    /// `distance` behind it, and never moves back: below the highest price for a sell, above
    /// the lowest for a buy. It starts trailing once the market trades at `activation` or
    /// beyond it (at or above it for a sell, at or below for a buy), or at once from the price
    /// when it is placed without one. [`Order::trailing_stop`] reports where the stop stands.
    TrailingStop {
        /// How far the stop stays behind the best price.
        distance: TrailDistance,
        /// The price at which the stop starts trailing, or `None` to start at once.
        activation: Option<f64>,
    },
}

/// How far a [trailing stop](OrderType::TrailingStop) stays behind the best price since it
/// started trailing.
///
/// ```
/// use egui_charts::model::{OrderRequest, OrderSide, OrderType, TrailDistance};
///
/// let distance = TrailDistance::Ticks { ticks: 8, tick_size: 0.25 };
/// let order_type = OrderType::TrailingStop { distance, activation: Some(4_510.0) };
/// let order = OrderRequest::new("ES", OrderSide::Sell, 1.0, order_type);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum TrailDistance {
    /// In the symbol's price units, positive.
    Price(f64),
    /// A positive number of ticks of `tick_size`, the symbol's positive tick size, which the
    /// order carries so that any account can price it.
    Ticks {
        /// How many ticks.
        ticks: u32,
        /// The price units of one tick.
        tick_size: f64,
    },
    /// A percentage of the best price's magnitude, above 0 and below 100.
    Percent(f64),
}

impl TrailDistance {
    /// The distance in price units from `best`: the price, the ticks times the tick size, or
    /// the percentage of `best`'s magnitude.
    pub(crate) fn amount(self, best: f64) -> f64 {
        match self {
            Self::Price(price) => price,
            Self::Ticks { ticks, tick_size } => f64::from(ticks) * tick_size,
            Self::Percent(percent) => best.abs() * percent / 100.0,
        }
    }

    /// Whether the distance is usable: a positive finite price, a positive number of ticks of a
    /// positive finite tick size with a finite product, or a finite percentage above 0 and
    /// below 100.
    pub(crate) fn is_valid(self) -> bool {
        match self {
            Self::Price(price) => price.is_finite() && price > 0.0,
            Self::Ticks { ticks, tick_size } => {
                ticks > 0
                    && tick_size.is_finite()
                    && tick_size > 0.0
                    && (f64::from(ticks) * tick_size).is_finite()
            }
            Self::Percent(percent) => percent > 0.0 && percent < 100.0,
        }
    }
}

/// Best price since reaching `level`, in direction `s` (1 long, -1 short).
pub(crate) fn trailed(
    best: Option<f64>,
    level: f64,
    s: f64,
    points: impl IntoIterator<Item = f64>,
) -> Option<f64> {
    points.into_iter().fold(best, |best, point| match best {
        Some(best) => Some(if s * (point - best) > 0.0 {
            point
        } else {
            best
        }),
        None => (s * (point - level) >= 0.0).then_some(point),
    })
}

/// The stop `distance` price units behind `best` in direction `s` (1 long, -1 short), at 15
/// significant digits, so a stop one tick behind a decimal price lands on that tick's price.
pub(crate) fn trailing_stop(best: f64, s: f64, distance: f64) -> f64 {
    settled(best - s * distance)
}

/// `value` at 15 significant digits, the most an f64 writes faithfully, so a computed
/// 0.3 - 0.1 is 0.2, not 0.19999999999999998.
pub(crate) fn settled(value: f64) -> f64 {
    format!("{value:.14e}").parse().unwrap_or(value)
}

impl OrderType {
    /// Where an order of this type on `side` fills while the price moves from `start` to `end`,
    /// or `None` if it does not: a market order at `start`, a limit or stop already reached at
    /// `start` there, one the movement reaches at its price. A stop-limit order returns the
    /// point where its stop triggered; the caller continues it from there as a limit order. A
    /// trailing stop returns `None`: its stop is not part of its type, so callers price it from
    /// its current stop.
    pub(crate) fn fill_price(&self, side: OrderSide, start: f64, end: f64) -> Option<f64> {
        let buy = side == OrderSide::Buy;
        let reach = |price: f64, at_or_below: bool| {
            let reached = |at: f64| {
                if at_or_below {
                    at <= price
                } else {
                    at >= price
                }
            };
            if reached(start) {
                Some(start)
            } else if reached(end) {
                Some(price)
            } else {
                None
            }
        };
        match *self {
            OrderType::Market => Some(start),
            OrderType::Limit(limit) => reach(limit, buy),
            OrderType::Stop(stop) | OrderType::StopLimit { stop, .. } => reach(stop, !buy),
            OrderType::TrailingStop { .. } => None,
        }
    }

    /// A trailing stop's best price after the market trades at `price`, from `best` before it,
    /// and its stop there; `None` before it starts trailing and for other order types. The
    /// direction is the position the order protects: up for a sell, down for a buy.
    pub(crate) fn trail(
        &self,
        side: OrderSide,
        best: Option<f64>,
        price: f64,
    ) -> Option<(f64, f64)> {
        let OrderType::TrailingStop {
            distance,
            activation,
        } = *self
        else {
            return None;
        };
        let s = if side == OrderSide::Sell { 1.0 } else { -1.0 };
        let best = trailed(best, activation.unwrap_or(price), s, [price])?;
        Some((best, trailing_stop(best, s, distance.amount(best))))
    }
}

/// The role of an exit order attached to the net position in its symbol. The set is closed: an
/// attached exit either takes profit or stops a loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PositionBracket {
    /// A profit-taking limit exit.
    TakeProfit,
    /// A protective stop exit.
    StopLoss,
}

/// A take profit that closes its own part of an order, with an optional stop loss of its own:
/// one of [`OrderRequest::take_profit_levels`].
///
/// ```
/// use egui_charts::model::TakeProfitLevel;
///
/// let mut level = TakeProfitLevel::new(110.0, 2.0);
/// level.stop_loss = Some(95.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TakeProfitLevel {
    /// The price at which it closes [`quantity`](Self::quantity): above the entry for a buy,
    /// below for a sell.
    pub price: f64,
    /// Positive, in the symbol's trading units.
    pub quantity: f64,
    /// The price at which a stop loss closes this level's quantity, for an account that accepts
    /// [level stops](Capabilities::take_profit_level_stops): below the entry for a buy, above
    /// for a sell. When either fills or is cancelled, the other is cancelled. `None` leaves the
    /// level to the order's [`stop_loss`](OrderRequest::stop_loss).
    #[serde(default)]
    pub stop_loss: Option<f64>,
}

impl TakeProfitLevel {
    /// A take profit that closes `quantity` at `price`, without a stop loss of its own.
    pub fn new(price: f64, quantity: f64) -> Self {
        Self {
            price,
            quantity,
            stop_loss: None,
        }
    }
}

/// An order as the user or the host asks for it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// `None` while [`take_profit_levels`](Self::take_profit_levels) are set.
    pub take_profit: Option<f64>,
    /// Several take profits, each closing its own quantity, for an account that accepts
    /// [take-profit levels](Capabilities::take_profit_levels), in the order given. Their
    /// quantities add up to [`quantity`](Self::quantity). Empty for one take profit or none,
    /// which [`take_profit`](Self::take_profit) holds. No more than
    /// [`max_take_profit_levels`](Capabilities::max_take_profit_levels).
    pub take_profit_levels: Vec<TakeProfitLevel>,
    /// The price at which a stop-loss order closes the filled quantity, for an account that
    /// accepts [brackets](Capabilities::brackets): below the entry for a buy, above for a sell.
    /// With [`take_profit_levels`](Self::take_profit_levels) it closes the quantity of the levels
    /// without a [stop loss of their own](TakeProfitLevel::stop_loss).
    pub stop_loss: Option<f64>,
    /// How long the order stays working.
    pub time_in_force: TimeInForce,
    /// When a [`GoodTillDate`](TimeInForce::GoodTillDate) order expires, in UTC. `None` for
    /// every other time in force.
    pub expires: Option<DateTime<Utc>>,
    /// Explicit link to the net position in this symbol. Ordinary opposite-side orders are
    /// not brackets. Brokers should preserve this role on updates. The terminal does not change
    /// the quantity of such an order: the broker sizes it.
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
            take_profit_levels: Vec::new(),
            stop_loss: None,
            time_in_force: TimeInForce::Day,
            expires: None,
            position_bracket: None,
        }
    }
}

/// How long an order stays working.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
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
    /// Until it fills, is cancelled or reaches [`OrderRequest::expires`].
    GoodTillDate,
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
            Self::GoodTillDate => "GTD",
            Self::ImmediateOrCancel => "IOC",
            Self::FillOrKill => "FOK",
        }
    }
}

/// An order the broker holds. Pushed again on every change; the terminal replaces the order with
/// the same id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Where a working [trailing stop](OrderType::TrailingStop)'s stop stands now, as the
    /// broker reports it. `None` before it starts trailing, for other order types and for a
    /// broker that does not report it.
    #[serde(default)]
    pub trailing_stop: Option<f64>,
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
            trailing_stop: None,
        }
    }
}

/// Where an order stands. `Filled`, `Cancelled` and `Rejected` end the order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// The fill's symbol when the broker supplies it. The chart and account panel prefer the
    /// order's symbol when known, otherwise this one, including for older history pages.
    /// Keep it identical in the stream and history pages: repeated fills are matched by value.
    pub symbol: Option<String>,
}

impl Execution {
    /// A fill of `quantity` at `price`, without a symbol.
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
            symbol: None,
        }
    }
}

/// A completed position, from the fill that opened it on a flat symbol or reversed the previous
/// position to the fill that flattened or reversed it. Includes partial closes along the way.
/// Prices use the symbol's price units and may be negative; quantity is positive.
///
/// ```
/// use chrono::DateTime;
/// use egui_charts::model::{ClosedPosition, OrderSide};
///
/// let opened = DateTime::from_timestamp(1_704_207_600, 0).unwrap();
/// let closed = DateTime::from_timestamp(1_704_209_400, 0).unwrap();
/// let mut position = ClosedPosition::new("AAPL", OrderSide::Buy, 3.0, 110.0, 125.0, 45.0, closed);
/// position.opened = Some(opened);
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ClosedPosition {
    /// The symbol as the feed knows it.
    pub symbol: String,
    /// [`Buy`](OrderSide::Buy) for long, [`Sell`](OrderSide::Sell) for short.
    pub side: OrderSide,
    /// Total quantity closed over the position's lifetime, positive, in its symbol's trading units.
    pub quantity: f64,
    /// The quantity-weighted average entry price.
    pub average_price: f64,
    /// The quantity-weighted average exit price.
    pub exit_price: f64,
    /// Realised profit or loss in the account's currency, as reported by the broker. The
    /// [paper account](crate::services::PaperAccount) excludes commissions, booked separately.
    pub realized_pnl: f64,
    /// The commissions of the position's fills in the account's currency, when the broker
    /// reports them: its opening, adding and closing fills, with a reversing fill's commission
    /// shared in proportion to the quantity it closes and opens. `realized_pnl` excludes them.
    #[serde(default)]
    pub commission: Option<f64>,
    /// The opening fill's time, when the broker reports it.
    pub opened: Option<DateTime<Utc>>,
    /// The time of the fill that flattened or reversed the position.
    pub closed: DateTime<Utc>,
}

impl ClosedPosition {
    /// A completed position with its side, total quantity, average entry and exit prices,
    /// realised profit or loss and closing time, without a reported opening time or commission.
    pub fn new(
        symbol: impl Into<String>,
        side: OrderSide,
        quantity: f64,
        average_price: f64,
        exit_price: f64,
        realized_pnl: f64,
        closed: DateTime<Utc>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            quantity,
            average_price,
            exit_price,
            realized_pnl,
            commission: None,
            opened: None,
            closed,
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
    /// The equity left above the [`margin`](Self::margin) the open positions hold, in
    /// `currency`, when the broker reports it; negative once equity no longer covers that margin.
    pub margin_buffer: Option<f64>,
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
            margin_buffer: None,
            orders_margin: None,
            available_funds: None,
            capabilities,
        }
    }
}

/// A change of the account's cash balance, such as a closed trade's profit or a commission.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Accepts trailing stop orders. Without it the order ticket offers none.
    pub trailing_stop: bool,
    /// Accepts a take-profit and a stop-loss with an order, and with [`modify`](Self::modify)
    /// adds them to a working order.
    pub brackets: bool,
    /// Accepts several take profits with an order, each closing its own quantity, through
    /// [`OrderRequest::take_profit_levels`]. Needs [`brackets`](Self::brackets); without it the
    /// order ticket offers one take profit.
    pub take_profit_levels: bool,
    /// Accepts a stop loss of its own for each take-profit level, through
    /// [`TakeProfitLevel::stop_loss`]. Needs [`take_profit_levels`](Self::take_profit_levels);
    /// without it the order's single stop loss protects every level.
    pub take_profit_level_stops: bool,
    /// The largest number of take-profit levels an order may carry, counting the first, or
    /// `None` without a limit. Below 2 the order ticket offers no levels.
    pub max_take_profit_levels: Option<usize>,
    /// Closes a position with an opposite market order, so the Close and Flatten controls work.
    pub close: bool,
    /// Closes part of a position with a smaller opposite market order. With
    /// [`close`](Self::close), Close on a position line, unless orders are instant, and on an
    /// account row asks for the quantity to close; without this flag it closes the whole
    /// position.
    pub partial_close: bool,
    /// Reverses a position with an opposite market order of twice its size, so the Reverse
    /// controls work.
    pub reverse: bool,
    /// Answers [`Broker::preview`](crate::services::Broker::preview), so the ticket offers to
    /// preview an order before it is placed.
    pub preview: bool,
    /// Changes working orders through [`Broker::modify`](crate::services::Broker::modify), so
    /// their lines on the chart can be dragged, and with [`brackets`](Self::brackets) exits
    /// added to them.
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
            OrderType::TrailingStop { .. } => self.trailing_stop,
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// The first is never [`GoodTillDate`](TimeInForce::GoodTillDate), which needs an entered
    /// expiry: orders sent without the ticket, such as Buy and Sell, take the first.
    pub time_in_force: Vec<TimeInForce>,
    /// The trade value per unit of margin, such as 20 for 20:1, if the broker trades the symbol
    /// on margin. The order ticket estimates the margin an order needs from it.
    pub leverage: Option<f64>,
}

impl TradingRules {
    /// Whether the rules are usable: a positive finite step, minimum and point value, a maximum
    /// no smaller than the minimum, a positive finite leverage and pip value where set, and at
    /// least one time in force.
    pub(crate) fn is_valid(&self) -> bool {
        let positive = |value: f64| value.is_finite() && value > 0.0;
        positive(self.quantity_step)
            && positive(self.min_quantity)
            && positive(self.point_value)
            && self
                .max_quantity
                .is_none_or(|max| positive(max) && max >= self.min_quantity)
            && self.leverage.is_none_or(positive)
            && self.pip_value.is_none_or(positive)
            && !self.time_in_force.is_empty()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_price_follows_the_side_and_the_movement() {
        use OrderSide::{Buy, Sell};
        let stop_limit = OrderType::StopLimit {
            stop: 100.0,
            limit: 101.0,
        };
        // (order type, side, start, end, fill)
        let cases = [
            (OrderType::Market, Buy, 103.0, 97.0, Some(103.0)),
            (OrderType::Market, Sell, 97.0, 103.0, Some(97.0)),
            // Gap: already beyond the price at the start, so filled there.
            (OrderType::Limit(100.0), Buy, 98.0, 95.0, Some(98.0)),
            (OrderType::Limit(100.0), Sell, 102.0, 105.0, Some(102.0)),
            (OrderType::Stop(100.0), Buy, 102.0, 105.0, Some(102.0)),
            (OrderType::Stop(100.0), Sell, 98.0, 95.0, Some(98.0)),
            (stop_limit, Buy, 102.0, 105.0, Some(102.0)),
            // Through: the movement reaches the price, so filled at it.
            (OrderType::Limit(100.0), Buy, 103.0, 97.0, Some(100.0)),
            (OrderType::Limit(100.0), Buy, 103.0, 100.0, Some(100.0)),
            (OrderType::Limit(100.0), Sell, 97.0, 103.0, Some(100.0)),
            (OrderType::Stop(100.0), Buy, 97.0, 103.0, Some(100.0)),
            (OrderType::Stop(100.0), Sell, 103.0, 97.0, Some(100.0)),
            (stop_limit, Buy, 97.0, 103.0, Some(100.0)),
            (stop_limit, Sell, 103.0, 97.0, Some(100.0)),
            // Not reached, including a movement away from the price.
            (OrderType::Limit(100.0), Buy, 103.0, 101.0, None),
            (OrderType::Limit(100.0), Sell, 97.0, 99.0, None),
            (OrderType::Limit(100.0), Sell, 99.0, 97.0, None),
            (OrderType::Stop(100.0), Buy, 97.0, 99.0, None),
            (OrderType::Stop(100.0), Sell, 103.0, 101.0, None),
            (stop_limit, Buy, 97.0, 99.0, None),
            (stop_limit, Sell, 103.0, 101.0, None),
            // A single price, as the paper account passes it.
            (OrderType::Limit(100.0), Buy, 100.0, 100.0, Some(100.0)),
            (OrderType::Limit(100.0), Buy, 100.5, 100.5, None),
            (OrderType::Stop(100.0), Sell, 100.0, 100.0, Some(100.0)),
            (OrderType::Stop(100.0), Sell, 100.5, 100.5, None),
        ];
        for (order_type, side, start, end, fill) in cases {
            assert_eq!(
                order_type.fill_price(side, start, end),
                fill,
                "{side:?} {order_type:?} from {start} to {end}"
            );
        }
    }
}
