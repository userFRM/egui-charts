//! The order ticket: a panel beside the chart with an Order tab, one order sized by the symbol's
//! trading rules with optional exits, and a DOM tab, the depth ladder.

use serde::{Deserialize, Serialize};

mod form;
mod view;

use super::dom::Ladder;
use super::format::{Numbers, TradingFormatter};
use super::widgets::{grouped, side_name};
use crate::chart::{TradingLineAction, decimals, to_step};
use crate::model::{
    Capabilities, Order, OrderBook, OrderId, OrderPreview, OrderRequest, OrderSide, OrderType,
    Position, TakeProfitLevel, Tick, TimeInForce, TradingRules,
};

/// How a take-profit or stop-loss distance is entered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketMode {
    /// The exit price itself.
    #[default]
    Price,
    /// Ticks of the symbol's tick size away from the entry.
    Ticks,
    /// Percent of the entry price away from the entry.
    Percent,
    /// The profit or loss at the exit in the account's currency.
    Money,
}

impl BracketMode {
    const ALL: [Self; 4] = [Self::Price, Self::Ticks, Self::Percent, Self::Money];

    fn name(self, currency: &str) -> &str {
        match self {
            Self::Price => "price",
            Self::Ticks => "ticks",
            Self::Percent => "%",
            Self::Money if currency.is_empty() => "money",
            Self::Money => currency,
        }
    }
}

/// A take-profit or stop-loss as entered in the ticket.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bracket {
    /// How [`value`](Self::value) is read.
    pub mode: BracketMode,
    /// A price, a number of ticks, a percentage or an amount of money.
    pub value: f64,
}

impl Bracket {
    /// The direction from the entry to the exit: up for a take profit (`profit`) of a buy and a
    /// stop loss of a sell, down for the other two.
    fn sign(side: OrderSide, profit: bool) -> f64 {
        if (side == OrderSide::Buy) == profit {
            1.0
        } else {
            -1.0
        }
    }

    /// The exit price for an order on `side` entering at `entry`, where `per_point` is the profit
    /// or loss of the order's quantity per price unit: a take profit (`profit`) lies above the
    /// entry of a buy and below that of a sell, a stop loss the other way.
    pub(crate) fn price(
        &self,
        side: OrderSide,
        entry: f64,
        tick: f64,
        per_point: f64,
        profit: bool,
    ) -> f64 {
        let sign = Self::sign(side, profit);
        match self.mode {
            BracketMode::Price => self.value,
            BracketMode::Ticks => entry + sign * self.value * tick,
            BracketMode::Percent => entry * (1.0 + sign * self.value / 100.0),
            BracketMode::Money => entry + sign * self.value / per_point,
        }
    }

    /// The bracket in `mode` whose [`price`](Self::price) is `price`.
    fn at(
        mode: BracketMode,
        price: f64,
        side: OrderSide,
        entry: f64,
        tick: f64,
        per_point: f64,
        profit: bool,
    ) -> Self {
        let distance = Self::sign(side, profit) * (price - entry);
        let value = match mode {
            BracketMode::Price => price,
            BracketMode::Ticks => (distance / tick).round(),
            BracketMode::Percent => distance / entry * 100.0,
            BracketMode::Money => distance * per_point,
        };
        Self { mode, value }
    }
}

/// A named quantity and brackets the ticket applies in one step. Hosts persist
/// [`Trading::presets`](super::Trading::presets) themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderPreset {
    /// Shown in the ticket's preset list.
    pub name: String,
    /// In the symbol's trading units.
    pub quantity: f64,
    /// The take profit, if the preset sets one.
    pub take_profit: Option<Bracket>,
    /// The stop loss, if the preset sets one.
    pub stop_loss: Option<Bracket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Market,
    Limit,
    Stop,
    StopLimit,
}

impl Kind {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Market, "Market"),
        (Self::Limit, "Limit"),
        (Self::Stop, "Stop"),
        (Self::StopLimit, "Stop limit"),
    ];

    fn accepted(self, capabilities: &Capabilities) -> bool {
        match self {
            Self::Market => capabilities.market,
            Self::Limit => capabilities.limit,
            Self::Stop => capabilities.stop,
            Self::StopLimit => capabilities.stop_limit,
        }
    }
}

/// How the quantity is entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sizing {
    /// The quantity itself.
    Units,
    /// The order's value in the account's currency.
    Value,
    /// The loss at the stop loss, in percent of the account's balance.
    Risk,
}

/// The ticket's two tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Order,
    Dom,
}

/// What the ticket needs from the chart and the account for one frame.
pub(crate) struct Market<'a> {
    /// The chart's symbol, which the host's formatter receives with each number.
    pub symbol: &'a str,
    pub formatter: Option<&'a dyn TradingFormatter>,
    /// The value in the account's currency of one pip for one unit, if known.
    pub pip_value: Option<f64>,
    /// The last price, the entry of a market order without a quote.
    pub last: Option<f64>,
    /// The best bid and ask, the entries of a market sell and buy.
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    /// The symbol's tick size.
    pub tick: f64,
    /// Decimals of prices.
    pub decimals: usize,
    /// The account's currency.
    pub currency: &'a str,
    /// The account's balance: risk sizing takes a percentage of it,
    /// and an order's margin is set against it.
    pub balance: Option<f64>,
    pub capabilities: Capabilities,
    pub rules: &'a TradingRules,
    pub rules_ready: bool,
    /// The sizes the depth ladder shows: the book summed over the orders the chart's order book
    /// view accepts.
    pub ladder: Option<std::borrow::Cow<'a, OrderBook>>,
    /// Why the ladder has no sizes when the view needs orders the feed does not send.
    pub ladder_unavailable: Option<crate::data::OrderFilterUnavailable>,
    /// Prices whose largest known individual order reaches a tier of the view, as (is bid,
    /// price, colour).
    pub highlights: Vec<(bool, f64, egui::Color32)>,
    /// The account's working orders and open positions, of every symbol.
    pub orders: &'a [Order],
    pub positions: &'a [Position],
    /// The retained feed trades, oldest first.
    pub trades: &'a [&'a Tick],
    /// Opening instant of the exchange session of the newest retained trade.
    pub session_open: Option<chrono::DateTime<chrono::Utc>>,
    /// The latest account fill for this symbol.
    pub last_fill: Option<f64>,
}

impl<'a> Market<'a> {
    /// How the numbers of the chart's symbol are written.
    pub fn numbers(&self) -> Numbers<'a> {
        Numbers {
            symbol: self.symbol,
            decimals: self.decimals,
            formatter: self.formatter,
        }
    }
}

/// What the user asked the ticket for.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Send this order now.
    Place(OrderRequest),
    /// Ask the broker to estimate this order without placing it.
    Preview(OrderRequest),
    /// Cancel these working orders now.
    Cancel(Vec<OrderId>),
    /// Ask the user to confirm this change before sending it.
    Confirm(Box<TradingLineAction>),
}

/// The state of the order ticket panel.
pub(crate) struct OrderTicket {
    pub open: bool,
    pub symbol: String,
    side: OrderSide,
    kind: Kind,
    /// The limit or stop price, and the stop price of a stop-limit order.
    price: f64,
    /// The limit price of a stop-limit order.
    limit: f64,
    /// The price field takes ticks from the ask or bid instead of the price.
    price_in_ticks: bool,
    sizing: Sizing,
    quantity: f64,
    /// The value or risk entered for [`Sizing::Value`] and [`Sizing::Risk`].
    amount: f64,
    take_profit: (bool, Bracket),
    /// The take-profit levels after the first, each with its share of the order quantity in
    /// percent. The first level, `take_profit`, takes the rest.
    levels: Vec<(Bracket, f64)>,
    stop_loss: (bool, Bracket),
    time_in_force: TimeInForce,
    preset_name: String,
    tab: Tab,
    exits_open: bool,
    ladder: Ladder,
    /// An order from the ladder, waiting for the user to confirm it.
    confirm: Option<OrderRequest>,
    cancel: Option<Vec<OrderId>>,
    /// Why the ladder refused its last order, shown on the DOM tab.
    dom_error: Option<String>,
}

impl Default for OrderTicket {
    fn default() -> Self {
        let bracket = Bracket {
            mode: BracketMode::Price,
            value: 0.0,
        };
        Self {
            open: false,
            symbol: String::new(),
            side: OrderSide::Buy,
            kind: Kind::Market,
            price: 0.0,
            limit: 0.0,
            price_in_ticks: false,
            sizing: Sizing::Units,
            quantity: 1.0,
            amount: 1_000.0,
            take_profit: (false, bracket),
            levels: Vec::new(),
            stop_loss: (false, bracket),
            time_in_force: TimeInForce::Day,
            preset_name: String::new(),
            tab: Tab::Order,
            exits_open: true,
            ladder: Ladder::default(),
            confirm: None,
            cancel: None,
            dom_error: None,
        }
    }
}

impl OrderTicket {
    /// Fills the ticket from `request` and opens it on its Order tab.
    pub fn open(&mut self, request: &OrderRequest) {
        self.confirm = None;
        self.cancel = None;
        if self.symbol != request.symbol {
            self.ladder = Ladder::default();
            self.take_profit = (
                false,
                Bracket {
                    mode: BracketMode::Price,
                    value: 0.0,
                },
            );
            self.stop_loss = self.take_profit;
        }
        self.open = true;
        self.tab = Tab::Order;
        self.symbol.clone_from(&request.symbol);
        self.side = request.side;
        self.sizing = Sizing::Units;
        self.quantity = request.quantity;
        (self.kind, self.price, self.limit) = match request.order_type {
            OrderType::Limit(price) => (Kind::Limit, price, price),
            OrderType::Stop(price) => (Kind::Stop, price, price),
            OrderType::StopLimit { stop, limit } => (Kind::StopLimit, stop, limit),
            _ => (Kind::Market, 0.0, 0.0),
        };
        self.time_in_force = request.time_in_force;
        let at_price = |value| Bracket {
            mode: BracketMode::Price,
            value,
        };
        let (first, rest) = match request.take_profit_levels.split_first() {
            Some((first, rest)) => (Some(first.price), rest),
            None => (request.take_profit, &[][..]),
        };
        self.levels = rest
            .iter()
            .map(|level| {
                let share = level.quantity / request.quantity * 100.0;
                (at_price(level.price), share)
            })
            .collect();
        for (target, value) in [
            (&mut self.take_profit, first),
            (&mut self.stop_loss, request.stop_loss),
        ] {
            target.0 = value.is_some();
            if let Some(value) = value {
                target.1 = at_price(value);
            }
        }
    }

    /// Discards price and confirmation state when the chart changes symbols.
    pub(super) fn change_symbol(&mut self, symbol: &str) {
        let request = OrderRequest::new(symbol, self.side, self.quantity, OrderType::Market);
        self.open(&request);
    }

    /// The ask for a buy and the bid for a sell, if the market quotes them.
    fn touch(&self, market: &Market<'_>) -> Option<f64> {
        match self.side {
            OrderSide::Buy => market.ask,
            OrderSide::Sell => market.bid,
        }
        .filter(|price| price.is_finite())
    }

    /// The price the order enters at: its limit price, its stop price, or for a market order the
    /// ask of a buy or the bid of a sell, or the last price without them.
    fn entry(&self, market: &Market<'_>) -> Option<f64> {
        match self.kind {
            Kind::Market => self.touch(market).or(market.last),
            Kind::Limit | Kind::Stop => Some(self.price),
            Kind::StopLimit => Some(self.limit),
        }
        .filter(|price| price.is_finite() && *price > 0.0)
    }

    /// The enabled take profit or stop loss of an order entering at `entry`, snapped to the tick,
    /// or none without brackets. Fails when it is not on its side of the entry. Without a
    /// `quantity`, a stop loss in money cannot be priced.
    fn exit(
        &self,
        market: &Market<'_>,
        side: OrderSide,
        entry: f64,
        quantity: Option<f64>,
        profit: bool,
    ) -> Result<Option<f64>, String> {
        let (enabled, bracket) = if profit {
            self.take_profit
        } else {
            self.stop_loss
        };
        if !(market.capabilities.brackets && enabled) {
            return Ok(None);
        }
        let per_point = match (bracket.mode, quantity) {
            (BracketMode::Money, None) => {
                return Err("Sizing by risk needs the stop loss as a price, ticks or %".into());
            }
            (_, quantity) => quantity.unwrap_or(1.0) * market.rules.point_value,
        };
        exit_price(market, side, entry, bracket, per_point, profit).map(Some)
    }

    /// The enabled take profits of an order of `quantity` entering at `entry`: one price, or
    /// for an account that accepts levels and a ticket with more than one, the levels, each
    /// rounded down to the quantity step with the rest on the first. Fails like `exit`, or when
    /// a share is not above 0 or a level falls below the minimum quantity.
    fn take_profits(
        &self,
        market: &Market<'_>,
        side: OrderSide,
        entry: f64,
        quantity: f64,
    ) -> Result<(Option<f64>, Vec<TakeProfitLevel>), String> {
        let capabilities = &market.capabilities;
        if !(self.take_profit.0 && capabilities.brackets && capabilities.take_profit_levels)
            || self.levels.is_empty()
        {
            let take_profit = self.exit(market, side, entry, Some(quantity), true)?;
            return Ok((take_profit, Vec::new()));
        }
        let rules = market.rules;
        let mut levels = Vec::with_capacity(self.levels.len() + 1);
        levels.push((self.take_profit.1, 0.0));
        for (bracket, share) in &self.levels {
            if !share.is_finite() || *share <= 0.0 {
                return Err("Enter a share above 0% for each take-profit level".into());
            }
            levels.push((
                *bracket,
                round_down(quantity * share / 100.0, rules.quantity_step),
            ));
        }
        let extra: f64 = levels.iter().map(|(_, quantity)| quantity).sum();
        levels[0].1 = to_step(quantity - extra, rules.quantity_step, f64::round);
        let levels = levels
            .into_iter()
            .map(|(bracket, quantity)| {
                if quantity < rules.min_quantity {
                    return Err(format!(
                        "Each take-profit level needs at least the minimum quantity of {}",
                        rules.min_quantity
                    ));
                }
                let per_point = quantity * rules.point_value;
                let price = exit_price(market, side, entry, bracket, per_point, true)?;
                Ok(TakeProfitLevel::new(price, quantity))
            })
            .collect::<Result<_, String>>()?;
        Ok((None, levels))
    }

    /// `order` from the depth ladder with the ticket's enabled take profit and stop loss, priced
    /// from the order's own side, entry and quantity.
    fn with_brackets(
        &self,
        mut order: OrderRequest,
        market: &Market<'_>,
    ) -> Result<OrderRequest, String> {
        let entry = match order.order_type {
            OrderType::Limit(price) | OrderType::Stop(price) => Some(price),
            OrderType::StopLimit { limit, .. } => Some(limit),
            OrderType::Market => match order.side {
                OrderSide::Buy => market.ask,
                OrderSide::Sell => market.bid,
            }
            .or(market.last),
        }
        .map(|price| to_step(price, market.tick, f64::round))
        .filter(|price| price.is_finite() && *price > 0.0)
        .ok_or("Enter a positive price")?;
        order.stop_loss = self.exit(market, order.side, entry, Some(order.quantity), false)?;
        (order.take_profit, order.take_profit_levels) =
            self.take_profits(market, order.side, entry, order.quantity)?;
        Ok(order)
    }

    /// The order the ticket describes, or what is wrong with it.
    pub fn request(&self, market: &Market<'_>) -> Result<OrderRequest, String> {
        if !market.rules_ready {
            return Err("Waiting for the symbol's trading rules".into());
        }
        if !self.kind.accepted(&market.capabilities) {
            return Err("The account does not accept this order type".into());
        }
        let snap = |value| to_step(value, market.tick, f64::round);
        let entry = self
            .entry(market)
            .map(snap)
            .filter(|price| price.is_finite() && *price > 0.0)
            .ok_or("Enter a positive price")?;
        let order_type = match self.kind {
            Kind::Market => OrderType::Market,
            Kind::Limit => OrderType::Limit(snap(self.price)),
            Kind::Stop => OrderType::Stop(snap(self.price)),
            Kind::StopLimit if self.price.is_finite() && snap(self.price) > 0.0 => {
                OrderType::StopLimit {
                    stop: snap(self.price),
                    limit: snap(self.limit),
                }
            }
            Kind::StopLimit => return Err("Enter a positive stop price".into()),
        };
        let rules = market.rules;
        // Units and value size the order before its exits; risk sizing needs the stop loss first.
        let mut quantity = match self.sizing {
            Sizing::Units => Some(order_quantity(self.quantity, rules)?),
            Sizing::Value => Some(order_quantity(
                self.amount / (entry * rules.point_value),
                rules,
            )?),
            Sizing::Risk => None,
        };
        let stop_loss = self.exit(market, self.side, entry, quantity, false)?;
        let quantity = match quantity.take() {
            Some(quantity) => quantity,
            None => {
                let stop = stop_loss.ok_or("Sizing by risk needs a stop loss")?;
                let balance = market
                    .balance
                    .filter(|balance| balance.is_finite() && *balance > 0.0)
                    .ok_or("Sizing by risk needs the account's balance")?;
                let risk = balance * self.amount / 100.0;
                order_quantity(risk / ((entry - stop).abs() * rules.point_value), rules)?
            }
        };
        let (take_profit, levels) = self.take_profits(market, self.side, entry, quantity)?;

        let mut request = OrderRequest::new(&self.symbol, self.side, quantity, order_type);
        request.take_profit = take_profit;
        request.take_profit_levels = levels;
        request.stop_loss = stop_loss;
        request.time_in_force = if rules.time_in_force.contains(&self.time_in_force) {
            self.time_in_force
        } else {
            rules.time_in_force[0]
        };
        Ok(request)
    }

    fn apply(&mut self, preset: &OrderPreset) {
        self.sizing = Sizing::Units;
        self.quantity = preset.quantity;
        self.take_profit = preset
            .take_profit
            .map_or((false, self.take_profit.1), |bracket| (true, bracket));
        self.levels.clear();
        self.stop_loss = preset
            .stop_loss
            .map_or((false, self.stop_loss.1), |bracket| (true, bracket));
    }

    fn preset(&self, name: String, market: &Market<'_>) -> OrderPreset {
        let quantity = self
            .request(market)
            .map_or(self.quantity, |request| request.quantity);
        OrderPreset {
            name,
            quantity,
            take_profit: self.take_profit.0.then_some(self.take_profit.1),
            stop_loss: self.stop_loss.0.then_some(self.stop_loss.1),
        }
    }
}

/// The price of `bracket` for an order on `side` entering at `entry`, snapped to the tick, where
/// `per_point` is the profit or loss of the exit's quantity per price unit. Fails when it is not
/// on its side of the entry.
fn exit_price(
    market: &Market<'_>,
    side: OrderSide,
    entry: f64,
    bracket: Bracket,
    per_point: f64,
    profit: bool,
) -> Result<f64, String> {
    let price = to_step(
        bracket.price(side, entry, market.tick, per_point, profit),
        market.tick,
        f64::round,
    );
    // A buy exits above its entry for a profit and below it for a loss; a sell the other way.
    let above = (price > entry) == (side == OrderSide::Buy);
    if !price.is_finite() || price <= 0.0 || price == entry || above != profit {
        return Err(if profit {
            "The take profit must lie on the profit side of the entry"
        } else {
            "The stop loss must lie on the loss side of the entry"
        }
        .into());
    }
    Ok(price)
}

/// The distance from `entry` to each take profit times the quantity it closes, or none
/// without a take profit: the reward per unit of point value.
fn reward(request: &OrderRequest, entry: f64) -> Option<f64> {
    if request.take_profit_levels.is_empty() {
        let price = request.take_profit?;
        return Some((price - entry).abs() * request.quantity);
    }
    let levels = request.take_profit_levels.iter();
    Some(
        levels
            .map(|level| (level.price - entry).abs() * level.quantity)
            .sum(),
    )
}

/// `quantity` rounded down to a whole multiple of `step`, at the step's decimals.
fn round_down(quantity: f64, step: f64) -> f64 {
    // The tolerance keeps 0.3 / 0.1 = 2.9999999999999996 at 3 steps.
    to_step(quantity, step, |steps| (steps + 1e-9).floor())
}

/// Rounds a quantity down to the symbol's step and checks its bounds.
pub(super) fn order_quantity(quantity: f64, rules: &TradingRules) -> Result<f64, String> {
    let quantity = round_down(quantity, rules.quantity_step);
    if !quantity.is_finite() || quantity < rules.min_quantity {
        return Err(format!(
            "The quantity is below the minimum of {}",
            rules.min_quantity
        ));
    }
    if rules.max_quantity.is_some_and(|max| quantity > max) {
        return Err(format!(
            "The quantity is above the maximum of {}",
            rules.max_quantity.unwrap_or_default()
        ));
    }
    Ok(quantity)
}

/// An order as the send button and the confirmation write it: "1 ES @ 4,000.25 LIMIT".
fn summary(request: &OrderRequest, numbers: &Numbers<'_>, quantity_decimals: usize) -> String {
    let at = |price: f64, kind: &str| format!(" @ {} {kind}", numbers.price(price));
    let order_type = match request.order_type {
        OrderType::Limit(price) => at(price, "LIMIT"),
        OrderType::Stop(price) => at(price, "STOP"),
        OrderType::StopLimit { stop, .. } => at(stop, "STOP LIMIT"),
        _ => " MKT".to_owned(),
    };
    format!(
        "{} {}{order_type}",
        numbers.quantity(request.quantity, quantity_decimals),
        request.symbol
    )
}

/// The question that confirms an order: "Buy 1 ES @ 4,000.25 LIMIT?", then its exits.
fn confirmation(request: &OrderRequest, numbers: &Numbers<'_>, quantity_decimals: usize) -> String {
    let mut text = format!(
        "{} {}?",
        side_name(request.side),
        summary(request, numbers, quantity_decimals)
    );
    if let Some(price) = request.take_profit {
        text.push_str(&format!("\nTake profit {}", numbers.price(price)));
    }
    for level in &request.take_profit_levels {
        text.push_str(&format!(
            "\nTake profit {} for {}",
            numbers.price(level.price),
            numbers.quantity(level.quantity, quantity_decimals)
        ));
    }
    if let Some(price) = request.stop_loss {
        text.push_str(&format!("\nStop loss {}", numbers.price(price)));
    }
    text
}

/// What the dialog shows for a previewed order: its confirmation, then the broker's estimates
/// and warnings.
pub(super) fn preview_message(
    request: &OrderRequest,
    estimate: &OrderPreview,
    rules: &TradingRules,
    numbers: &Numbers<'_>,
    currency: &str,
) -> String {
    let mut text = confirmation(request, numbers, decimals(rules.quantity_step));
    for (label, value) in [
        ("Cost", estimate.cost),
        ("Commission", estimate.commission),
        ("Margin", estimate.margin),
    ] {
        if let Some(value) = value.filter(|value| value.is_finite()) {
            text.push_str(&format!(
                "\n{label}: {} {currency}",
                grouped(value, 2, false)
            ));
        }
    }
    for warning in &estimate.warnings {
        text.push_str(&format!("\n{warning}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_ticket_replaces_brackets_and_prices() {
        let mut ticket = OrderTicket::default();
        let mut request = OrderRequest::new("ES", OrderSide::Buy, 2.0, OrderType::Limit(100.0));
        request.take_profit = Some(105.0);
        request.stop_loss = Some(95.0);
        for brackets in [true, false] {
            if !brackets {
                request = OrderRequest::new("NQ", OrderSide::Sell, 1.0, OrderType::Market);
            }
            ticket.confirm = Some(request.clone());
            ticket.cancel = Some(vec![OrderId::new("old")]);
            ticket.open(&request);
            assert!(ticket.confirm.is_none());
            assert!(ticket.cancel.is_none());
            assert_eq!(ticket.take_profit.0, brackets);
            assert_eq!(ticket.stop_loss.0, brackets);
            if brackets {
                assert_eq!(
                    ticket.take_profit.1,
                    Bracket {
                        mode: BracketMode::Price,
                        value: 105.0
                    }
                );
                assert_eq!(
                    ticket.stop_loss.1,
                    Bracket {
                        mode: BracketMode::Price,
                        value: 95.0
                    }
                );
            } else {
                assert_eq!((ticket.price, ticket.limit), (0.0, 0.0));
            }
        }
    }

    /// A pending entry is sized by its own price like a market entry, the risk control is offered
    /// for every order type, and an order from the ladder takes the ticket's brackets from its own
    /// side, entry and quantity and shows them in its confirmation. A regression sizes pending
    /// orders from the market price, hides the control, or sends ladder orders without exits.
    #[test]
    fn pending_entries_are_sized_by_risk_and_ladder_orders_carry_the_brackets() {
        let rules = TradingRules::new("ES");
        let mut capabilities = Capabilities::default();
        (capabilities.limit, capabilities.stop, capabilities.brackets) = (true, true, true);
        capabilities.take_profit_levels = true;
        let market = Market {
            symbol: "ES",
            formatter: None,
            pip_value: None,
            last: Some(100.0),
            bid: Some(99.75),
            ask: Some(100.25),
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: Some(10_000.0),
            capabilities,
            rules: &rules,
            rules_ready: true,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &[],
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
        };
        let ticks = |value| {
            (
                true,
                Bracket {
                    mode: BracketMode::Ticks,
                    value,
                },
            )
        };
        // 1% of 10,000 = 100 at a stop 8 ticks = 2 points from the entry: 100 / 2 = 50 units.
        let mut ticket = OrderTicket {
            symbol: "ES".into(),
            kind: Kind::Limit,
            price: 99.0,
            sizing: Sizing::Risk,
            amount: 1.0,
            take_profit: ticks(16.0),
            stop_loss: ticks(8.0),
            ..OrderTicket::default()
        };
        let request = ticket.request(&market).unwrap();
        assert_eq!(
            (request.quantity, request.take_profit, request.stop_loss),
            (50.0, Some(103.0), Some(97.0))
        );
        // The control is on the Order tab for the pending kind.
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ticket.show(ui, &market, &mut vec![], &mut false);
        });
        let risk = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Risk, % balance")
        });
        output.drop_without_applying_deltas();
        assert!(risk, "limit orders offer the risk control");
        // A sell stop from the ladder: exits lie the other way from the buy above.
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        let order = ticket.with_brackets(order, &market).unwrap();
        assert_eq!(
            (order.take_profit, order.stop_loss),
            (Some(94.0), Some(100.0))
        );
        assert_eq!(
            confirmation(&order, &market.numbers(), 0),
            "Sell 3 ES @ 98.00 STOP?\nTake profit 94.00\nStop loss 100.00"
        );
        // A level at 24 ticks = 6 points for half of 3: 1.5 rounds down to 1 and the first takes 2.
        ticket.levels = vec![(ticks(24.0).1, 50.0)];
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        let order = ticket.with_brackets(order, &market).unwrap();
        assert_eq!(order.take_profit, None);
        assert_eq!(
            order.take_profit_levels,
            [
                TakeProfitLevel::new(94.0, 2.0),
                TakeProfitLevel::new(92.0, 1.0)
            ]
        );
        assert_eq!(
            confirmation(&order, &market.numbers(), 0),
            "Sell 3 ES @ 98.00 STOP?\nTake profit 94.00 for 2\nTake profit 92.00 for 1\nStop loss 100.00"
        );
        ticket.stop_loss = ticks(0.0);
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        assert!(ticket.with_brackets(order, &market).is_err());
    }

    /// Guards sizing and brackets against the published definitions: value sizing buys
    /// value / (price * point value) units, risk sizing loses at most the risk at the stop, both
    /// round down to the quantity step, and ticks, percentages and money place a take profit
    /// above a buy's entry and below a sell's (a stop loss the other way). A sign flip sends a
    /// bracket to the wrong side; rounding to nearest risks more than asked. Writing an exit in
    /// another way keeps its price, or switching how it is entered moves it.
    #[test]
    fn tickets_size_and_place_brackets_by_the_rules() {
        let mut rules = TradingRules::new("ES");
        rules.quantity_step = 0.5;
        rules.min_quantity = 0.5;
        rules.max_quantity = Some(10.0);
        rules.point_value = 50.0;
        let mut capabilities = Capabilities::default();
        (
            capabilities.market,
            capabilities.limit,
            capabilities.brackets,
        ) = (true, true, true);
        let mut market = Market {
            symbol: "ES",
            formatter: None,
            pip_value: None,
            last: Some(4_000.0),
            bid: None,
            ask: None,
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: Some(100_000.0),
            capabilities,
            rules: &rules,
            rules_ready: true,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &[],
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
        };
        let ticks = |value| Bracket {
            mode: BracketMode::Ticks,
            value,
        };
        let percent = |value| Bracket {
            mode: BracketMode::Percent,
            value,
        };
        let money = |value| Bracket {
            mode: BracketMode::Money,
            value,
        };
        // (side, sizing, amount or quantity, take profit, stop loss) -> (quantity, tp, sl)
        let cases = [
            // 1,000,000 / (4,000 * 50) = 5 units.
            (
                OrderSide::Buy,
                Sizing::Value,
                1_000_000.0,
                None,
                None,
                Ok((5.0, None, None)),
            ),
            // 1,100,000 / 200,000 = 5.5, a whole number of 0.5 steps.
            (
                OrderSide::Buy,
                Sizing::Value,
                1_100_000.0,
                None,
                None,
                Ok((5.5, None, None)),
            ),
            // 1,140,000 / 200,000 = 5.7, rounded down to 5.5.
            (
                OrderSide::Sell,
                Sizing::Value,
                1_140_000.0,
                None,
                None,
                Ok((5.5, None, None)),
            ),
            // 1% of 100,000 = 1,000 at a stop 8 ticks = 2 points below: 1,000 / (2 * 50) = 10.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1.0,
                Some(ticks(16.0)),
                Some(ticks(8.0)),
                Ok((10.0, Some(4_004.0), Some(3_998.0))),
            ),
            // A sell's stop lies above: 1% of 4,000 = 40 points, 1,000 / (40 * 50) = 0.5.
            (
                OrderSide::Sell,
                Sizing::Risk,
                1.0,
                Some(percent(2.0)),
                Some(percent(1.0)),
                Ok((0.5, Some(3_920.0), Some(4_040.0))),
            ),
            // 900 / 2,000 = 0.45, below the minimum of 0.5.
            (
                OrderSide::Sell,
                Sizing::Risk,
                0.9,
                None,
                Some(percent(1.0)),
                Err(()),
            ),
            // Stop 4 ticks = 1 point below: 1,500 / 50 = 30 units, above the maximum of 10.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1.5,
                None,
                Some(ticks(4.0)),
                Err(()),
            ),
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                None,
                None,
                Ok((2.0, None, None)),
            ),
            (OrderSide::Buy, Sizing::Risk, 1.0, None, None, Err(())),
            (
                OrderSide::Buy,
                Sizing::Units,
                1.0,
                None,
                Some(ticks(0.0)),
                Err(()),
            ),
            (
                OrderSide::Sell,
                Sizing::Units,
                1.0,
                Some(ticks(0.0)),
                None,
                Err(()),
            ),
            // Percent exits and explicit prices snap before sizing by their risk: 0.025% of
            // 100,000 = 25 at 0.5 points: 25 / (0.5 * 50) = 1.
            (
                OrderSide::Buy,
                Sizing::Risk,
                0.025,
                Some(percent(0.014)),
                Some(Bracket {
                    mode: BracketMode::Price,
                    value: 3999.6,
                }),
                Ok((1.0, Some(4000.5), Some(3999.5))),
            ),
            // 2 units of 50 a point: 500 is 5 points of profit, 250 is 2.5 points of loss.
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                Some(money(500.0)),
                Some(money(250.0)),
                Ok((2.0, Some(4_005.0), Some(3_997.5))),
            ),
            // A stop loss in money needs the quantity that risk sizing takes from it.
            (
                OrderSide::Sell,
                Sizing::Risk,
                1.0,
                None,
                Some(money(250.0)),
                Err(()),
            ),
        ];
        for (side, sizing, amount, take_profit, stop_loss, expected) in cases {
            let ticket = |take_profit: Option<Bracket>, stop_loss: Option<Bracket>| OrderTicket {
                symbol: "ES".into(),
                side,
                sizing,
                quantity: amount,
                amount,
                take_profit: (take_profit.is_some(), take_profit.unwrap_or(ticks(1.0))),
                stop_loss: (stop_loss.is_some(), stop_loss.unwrap_or(ticks(1.0))),
                ..OrderTicket::default()
            };
            let actual = ticket(take_profit, stop_loss)
                .request(&market)
                .map(|r| (r.quantity, r.take_profit, r.stop_loss))
                .map_err(|_| ());
            assert_eq!(actual, expected, "{side:?} {sizing:?} {amount}");
            let Ok((quantity, Some(take_profit), Some(stop_loss))) = expected else {
                continue;
            };
            if sizing == Sizing::Risk {
                continue;
            }
            for mode in BracketMode::ALL {
                let at = |price, profit| {
                    let per_point = quantity * rules.point_value;
                    Bracket::at(mode, price, side, 4_000.0, 0.25, per_point, profit)
                };
                let actual = ticket(Some(at(take_profit, true)), Some(at(stop_loss, false)))
                    .request(&market)
                    .map(|r| (r.quantity, r.take_profit, r.stop_loss));
                assert_eq!(actual, Ok(expected.unwrap()), "{side:?} {mode:?}");
            }
        }
        // (side, sizing, amount or quantity, take profit, stop loss, further levels with their
        // shares, whether the account accepts levels) -> (take profit, levels, reward * 50). A
        // regression sizes levels from the typed quantity instead of the risk-sized one, prices
        // money from the whole order, or loses a level when the preview reopens the ticket.
        let level = TakeProfitLevel::new;
        let cases = [
            // 2 units split in half: 1 * 1 + 2 * 1 = 3 points of reward.
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                ticks(4.0),
                None,
                vec![(ticks(8.0), 50.0)],
                true,
                Ok((None, vec![level(4_001.0, 1.0), level(4_002.0, 1.0)], 150.0)),
            ),
            // Half of 1.5 rounds down to 0.5 and the first takes the rest: 1 + 1 = 2 points.
            (
                OrderSide::Buy,
                Sizing::Units,
                1.5,
                ticks(4.0),
                None,
                vec![(ticks(8.0), 50.0)],
                true,
                Ok((None, vec![level(4_001.0, 1.0), level(4_002.0, 0.5)], 100.0)),
            ),
            // 30% of 1 rounds down to 0, below the minimum of 0.5.
            (
                OrderSide::Buy,
                Sizing::Units,
                1.0,
                ticks(4.0),
                None,
                vec![(ticks(8.0), 30.0)],
                true,
                Err(()),
            ),
            // Nothing is left for the first level.
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                ticks(4.0),
                None,
                vec![(ticks(8.0), 50.0), (ticks(12.0), 50.0)],
                true,
                Err(()),
            ),
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                ticks(4.0),
                None,
                vec![(ticks(8.0), f64::NAN)],
                true,
                Err(()),
            ),
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                ticks(4.0),
                None,
                vec![(ticks(8.0), 0.0)],
                true,
                Err(()),
            ),
            // Each level makes 1.5 * 50 = 75 a point: 150 is 2 points, 300 is 4. Reward 3 + 6.
            (
                OrderSide::Sell,
                Sizing::Units,
                3.0,
                money(150.0),
                None,
                vec![(money(300.0), 50.0)],
                true,
                Ok((None, vec![level(3_998.0, 1.5), level(3_996.0, 1.5)], 450.0)),
            ),
            // Without the capability the first take profit covers the whole quantity.
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                ticks(4.0),
                None,
                vec![(ticks(8.0), 50.0)],
                false,
                Ok((Some(4_001.0), vec![], 100.0)),
            ),
            // 1% of 100,000 at 2 points of 50 sizes 10 units: 4 * 7.5 + 6 * 2.5 = 45 points.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1.0,
                ticks(16.0),
                Some(ticks(8.0)),
                vec![(ticks(24.0), 25.0)],
                true,
                Ok((
                    None,
                    vec![level(4_004.0, 7.5), level(4_006.0, 2.5)],
                    2_250.0,
                )),
            ),
        ];
        for (side, sizing, amount, take_profit, stop_loss, levels, accepted, expected) in cases {
            market.capabilities.take_profit_levels = accepted;
            let ticket = OrderTicket {
                symbol: "ES".into(),
                side,
                sizing,
                quantity: amount,
                amount,
                take_profit: (true, take_profit),
                levels,
                stop_loss: (stop_loss.is_some(), stop_loss.unwrap_or(ticks(1.0))),
                ..OrderTicket::default()
            };
            let request = ticket.request(&market);
            let actual = request.as_ref().map_err(|_| ()).map(|request| {
                (
                    request.take_profit,
                    request.take_profit_levels.clone(),
                    reward(request, 4_000.0).unwrap() * rules.point_value,
                )
            });
            assert_eq!(actual, expected, "{side:?} {sizing:?} {amount}");
            if let Ok(request) = request {
                let mut reopened = OrderTicket::default();
                reopened.open(&request);
                assert_eq!(reopened.request(&market), Ok(request));
            }
        }
        // A quoted market order enters a buy at the ask and a sell at the bid: 4 ticks = 1 point.
        let quoted = Market {
            bid: Some(3_999.75),
            ask: Some(4_000.25),
            ..market
        };
        for (side, take_profit) in [(OrderSide::Buy, 4_001.25), (OrderSide::Sell, 3_998.75)] {
            let ticket = OrderTicket {
                symbol: "ES".into(),
                side,
                take_profit: (true, ticks(4.0)),
                ..OrderTicket::default()
            };
            let request = ticket.request(&quoted).unwrap();
            assert_eq!(request.take_profit, Some(take_profit), "{side:?}");
        }
    }
}
