//! The order ticket: a panel beside the chart with an Order tab, one order sized by the symbol's
//! trading rules with optional exits, and a DOM tab, the depth ladder.

use serde::{Deserialize, Serialize};

mod form;
mod view;

use super::dom::Ladder;
use super::format::{Numbers, TradingFormatter};
use super::widgets::grouped;
use crate::chart::TradingLineAction;
use crate::localization as l10n;
use crate::model::{
    Capabilities, Order, OrderBook, OrderId, OrderPreview, OrderRequest, OrderSide, OrderType,
    Position, TakeProfitLevel, Tick, TimeInForce, TradingRules,
};
use crate::scales::{decimals, to_step};

/// How a take-profit or stop-loss distance is entered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Bracket {
    /// How [`value`](Self::value) is read.
    pub mode: BracketMode,
    /// A price, a number of ticks, a percentage or an amount of money.
    pub value: f64,
}

impl Bracket {
    /// A bracket of `value`, read as `mode` says.
    ///
    /// ```
    /// use egui_charts::ui::trading::{Bracket, BracketMode, OrderKind, OrderPreset, PresetSizing};
    ///
    /// let mut preset = OrderPreset::new("Scalp", 2.0);
    /// preset.take_profit = Some(Bracket::new(BracketMode::Ticks, 8.0));
    /// preset.sizing = Some(PresetSizing::RiskPercent(1.0));
    /// preset.order_type = Some(OrderKind::Limit);
    /// ```
    pub fn new(mode: BracketMode, value: f64) -> Self {
        Self { mode, value }
    }

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

/// Named ticket parameters the ticket applies in one step. Hosts persist
/// [`Trading::presets`](super::Trading::presets) themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OrderPreset {
    /// Shown in the ticket's preset list.
    pub name: String,
    /// In the symbol's trading units: what [`PresetSizing::Units`] applies, and the quantity
    /// the other sizings gave when the preset was saved.
    pub quantity: f64,
    /// The take profit, if the preset sets one.
    pub take_profit: Option<Bracket>,
    /// The stop loss, if the preset sets one.
    pub stop_loss: Option<Bracket>,
    /// How the preset sizes the order, or `None` to keep the ticket's quantity. Presets saved
    /// without it restore [`quantity`](Self::quantity) units.
    #[serde(default = "units")]
    pub sizing: Option<PresetSizing>,
    /// Whether applying sets both exits, turning off one that is `None`. Presets saved without
    /// this field restore exits.
    #[serde(default = "restores")]
    pub exits: bool,
    /// The order type to select, or `None` to keep the ticket's.
    #[serde(default)]
    pub order_type: Option<Kind>,
    /// The time in force to select, or `None` to keep the ticket's.
    #[serde(default)]
    pub time_in_force: Option<TimeInForce>,
}

fn restores() -> bool {
    true
}

fn units() -> Option<PresetSizing> {
    Some(PresetSizing::Units)
}

/// How an [`OrderPreset`] sizes an order. Value and risk are sized again against the entry,
/// stop loss and balance when the ticket applies it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PresetSizing {
    /// [`OrderPreset::quantity`] units.
    #[default]
    Units,
    /// Units worth this amount in the account's currency at the entry.
    Value(f64),
    /// Units that lose this percent of the account's balance at the stop loss.
    RiskPercent(f64),
}

impl OrderPreset {
    /// A preset of `quantity` without a take profit or a stop loss.
    pub fn new(name: impl Into<String>, quantity: f64) -> Self {
        Self {
            name: name.into(),
            quantity,
            take_profit: None,
            stop_loss: None,
            sizing: Some(PresetSizing::Units),
            exits: true,
            order_type: None,
            time_in_force: None,
        }
    }
}

/// The order types the ticket offers, without their prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Kind {
    /// At the best price available now.
    Market,
    /// At the entered price or better.
    Limit,
    /// As a market order once the market trades at the entered price.
    Stop,
    /// As a limit order at the limit price once the market trades at the stop price.
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

/// Which parts of the ticket a new preset keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PresetFields {
    quantity: bool,
    exits: bool,
    order_type: bool,
    time_in_force: bool,
}

impl Default for PresetFields {
    fn default() -> Self {
        Self {
            quantity: true,
            exits: true,
            order_type: true,
            time_in_force: true,
        }
    }
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
    /// The broker's status while it reports that it is not live, which the header shows; the
    /// ticket sends nothing meanwhile. `None` while it is live, before it reports a status and
    /// without a broker.
    pub offline: Option<crate::ui::model::ConnectionState>,
    /// The sizes the depth ladder shows: the book summed over the orders the chart's order book
    /// view accepts.
    pub ladder: Option<std::borrow::Cow<'a, OrderBook>>,
    /// Why the ladder has no sizes when the view needs orders the feed does not send.
    pub ladder_unavailable: Option<crate::data::OrderFilterUnavailable>,
    /// Prices whose largest known individual order reaches a tier of the view, as (is bid,
    /// price, size of that order, colour).
    pub highlights: Vec<(bool, f64, f64, egui::Color32)>,
    /// The account's working orders and open positions, of every symbol.
    pub orders: &'a [Order],
    pub positions: &'a [Position],
    /// The retained feed trades, oldest first.
    pub trades: &'a [&'a Tick],
    /// Opening instant of the exchange session of the newest retained trade.
    pub session_open: Option<chrono::DateTime<chrono::Utc>>,
    /// The latest account fill for this symbol.
    pub last_fill: Option<f64>,
    /// The position profit unit, shared by its chart label and the depth ladder.
    pub pnl_unit: crate::chart::PnlUnit,
    /// URI of the symbol logo, from the chart instrument or the host description of the symbol.
    pub logo: Option<&'a str>,
    /// The broker account whose presets the ticket offers.
    pub account: &'a str,
    /// The chart display zone, in which the expiry is entered.
    pub timezone: crate::scales::TimezoneMode,
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
    /// Open this working order on the Order tab to modify it.
    Edit(Order),
}

/// The state of the order ticket panel.
#[derive(Debug)]
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
    expires: Option<chrono::DateTime<chrono::Utc>>,
    preset_name: String,
    preset_fields: PresetFields,
    replacing: Option<OrderPreset>,
    renaming: Option<(String, String)>,
    tab: Tab,
    exits_open: bool,
    ladder: Ladder,
    /// An order from the ladder, waiting for the user to confirm it.
    confirm: Option<OrderRequest>,
    cancel: Option<Vec<OrderId>>,
    /// Why the ladder refused its last order, shown on the DOM tab.
    dom_error: Option<String>,
    /// The working order the Order tab modifies instead of placing a new one.
    pub(super) editing: Option<Order>,
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
            expires: None,
            preset_name: String::new(),
            preset_fields: PresetFields::default(),
            replacing: None,
            renaming: None,
            tab: Tab::Order,
            exits_open: true,
            ladder: Ladder::default(),
            confirm: None,
            cancel: None,
            dom_error: None,
            editing: None,
        }
    }
}

impl OrderTicket {
    /// Fills the ticket from `request` and opens it on its Order tab.
    pub fn open(&mut self, request: &OrderRequest) {
        self.editing = None;
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
        self.expires = request.expires;
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

    /// Opens the working order with its quantity, prices, time in force and brackets.
    pub(super) fn edit(&mut self, order: &Order) {
        self.open(&order.request);
        self.editing = Some(order.clone());
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
        ctx: &egui::Context,
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
                    return Err(l10n::format_text(
                        ctx,
                        "Each take-profit level needs at least the minimum quantity of {minimum}",
                        &[(
                            "minimum",
                            limit_text(
                                &market.numbers(),
                                rules.min_quantity,
                                decimals(rules.quantity_step),
                            ),
                        )],
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
        ctx: &egui::Context,
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
            self.take_profits(ctx, market, order.side, entry, order.quantity)?;
        if order.time_in_force == TimeInForce::GoodTillDate {
            order.expires = Some(
                self.expires
                    .ok_or("Enter the order's expiry on the Order tab")?,
            );
        }
        Ok(order)
    }

    /// The order the ticket describes, or what is wrong with it.
    pub fn request(
        &self,
        ctx: &egui::Context,
        market: &Market<'_>,
    ) -> Result<OrderRequest, String> {
        if market.offline.is_some() {
            return Err("Waiting for the broker to connect".into());
        }
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
        let reason = |limit: QuantityLimit| {
            limit.message(ctx, &market.numbers(), decimals(rules.quantity_step))
        };
        // Units and value size the order before its exits; risk sizing needs the stop loss first.
        let mut quantity = match self.sizing {
            Sizing::Units => Some(order_quantity(self.quantity, rules).map_err(reason)?),
            Sizing::Value => Some(
                order_quantity(self.amount / (entry * rules.point_value), rules).map_err(reason)?,
            ),
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
                order_quantity(risk / ((entry - stop).abs() * rules.point_value), rules)
                    .map_err(reason)?
            }
        };
        let (take_profit, levels) = self.take_profits(ctx, market, self.side, entry, quantity)?;

        let mut request = OrderRequest::new(&self.symbol, self.side, quantity, order_type);
        request.take_profit = take_profit;
        request.take_profit_levels = levels;
        request.stop_loss = stop_loss;
        request.time_in_force = if rules.time_in_force.contains(&self.time_in_force) {
            self.time_in_force
        } else {
            rules.time_in_force[0]
        };
        if request.time_in_force == TimeInForce::GoodTillDate {
            request.expires = Some(self.expires.ok_or("Enter the expiry as YYYY-MM-DD HH:MM")?);
        }
        Ok(request)
    }

    /// Selects a type as its tab does, seeding an unentered price from the quote.
    fn select_kind(&mut self, kind: Kind, market: &Market<'_>) {
        if self.editing.is_some() && kind != self.kind {
            return;
        }
        if self.kind == Kind::Market && kind != Kind::Market && self.price == 0.0 {
            self.price = self.touch(market).or(market.last).unwrap_or_default();
            self.limit = self.price;
        }
        self.kind = kind;
    }

    fn apply(&mut self, preset: &OrderPreset, market: &Market<'_>) {
        match preset.sizing {
            None => {}
            Some(PresetSizing::Value(amount)) => {
                (self.sizing, self.amount) = (Sizing::Value, amount)
            }
            // Risk needs a stop loss, which an account without brackets cannot take.
            Some(PresetSizing::RiskPercent(percent)) if market.capabilities.brackets => {
                (self.sizing, self.amount) = (Sizing::Risk, percent);
            }
            Some(_) => self.sizing = Sizing::Units,
        }
        if preset.sizing.is_some() {
            self.quantity = preset.quantity;
        }
        if preset.exits {
            self.take_profit = preset
                .take_profit
                .map_or((false, self.take_profit.1), |bracket| (true, bracket));
            self.levels.clear();
            self.stop_loss = preset
                .stop_loss
                .map_or((false, self.stop_loss.1), |bracket| (true, bracket));
        }
        if let Some(kind) = preset
            .order_type
            .filter(|kind| kind.accepted(&market.capabilities))
        {
            self.select_kind(kind, market);
        }
        if let Some(value) = preset
            .time_in_force
            .filter(|value| market.rules.time_in_force.contains(value))
        {
            self.time_in_force = value;
        }
    }

    fn preset(&self, ctx: &egui::Context, name: String, market: &Market<'_>) -> OrderPreset {
        let quantity = self
            .request(ctx, market)
            .map_or(self.quantity, |request| request.quantity);
        OrderPreset {
            name,
            quantity,
            take_profit: (self.preset_fields.exits && self.take_profit.0)
                .then_some(self.take_profit.1),
            stop_loss: (self.preset_fields.exits && self.stop_loss.0).then_some(self.stop_loss.1),
            sizing: self.preset_fields.quantity.then_some(match self.sizing {
                Sizing::Units => PresetSizing::Units,
                Sizing::Value => PresetSizing::Value(self.amount),
                Sizing::Risk => PresetSizing::RiskPercent(self.amount),
            }),
            exits: self.preset_fields.exits,
            order_type: self.preset_fields.order_type.then_some(self.kind),
            time_in_force: self
                .preset_fields
                .time_in_force
                .then_some(self.time_in_force),
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
    // Allow division noise at both small step counts and large quantities converted from money.
    to_step(quantity, step, |steps| {
        (steps + 1e-9 + steps.abs() * 1e-12).floor()
    })
}

/// A minimum or maximum order quantity of the symbol's rules that a quantity breaks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum QuantityLimit {
    /// Below this minimum.
    Minimum(f64),
    /// Above this maximum.
    Maximum(f64),
}

impl QuantityLimit {
    /// What the ticket shows for this limit, translated for `ctx`, with the limit written as
    /// [`limit_text`] writes it.
    pub(crate) fn message(
        self,
        ctx: &egui::Context,
        numbers: &Numbers<'_>,
        decimals: usize,
    ) -> String {
        match self {
            Self::Minimum(minimum) => l10n::format_text(
                ctx,
                "The quantity is below the minimum of {minimum}",
                &[("minimum", limit_text(numbers, minimum, decimals))],
            ),
            Self::Maximum(maximum) => l10n::format_text(
                ctx,
                "The quantity is above the maximum of {maximum}",
                &[("maximum", limit_text(numbers, maximum, decimals))],
            ),
        }
    }
}

/// A quantity limit in the host's quantity text, or at `decimals`, the quantity decimals the
/// panel shows, and as many more as the limit needs to read exactly.
fn limit_text(numbers: &Numbers<'_>, limit: f64, decimals: usize) -> String {
    numbers.quantity(limit, decimals.max(crate::scales::decimals(limit)))
}

/// Rounds a quantity down to the symbol's step and checks its bounds.
pub(super) fn order_quantity(quantity: f64, rules: &TradingRules) -> Result<f64, QuantityLimit> {
    let quantity = round_down(quantity, rules.quantity_step);
    if !quantity.is_finite() || quantity < rules.min_quantity {
        return Err(QuantityLimit::Minimum(rules.min_quantity));
    }
    if let Some(max) = rules.max_quantity.filter(|max| quantity > *max) {
        return Err(QuantityLimit::Maximum(max));
    }
    Ok(quantity)
}

/// An order as the send button and the confirmation write it: "1 ES @ 4,000.25 LIMIT".
fn summary(
    ctx: &egui::Context,
    request: &OrderRequest,
    numbers: &Numbers<'_>,
    quantity_decimals: usize,
) -> String {
    let (key, price) = match request.order_type {
        OrderType::Limit(price) => ("{quantity} {symbol} @ {price} LIMIT", Some(price)),
        OrderType::Stop(price) => ("{quantity} {symbol} @ {price} STOP", Some(price)),
        OrderType::StopLimit { stop, .. } => {
            ("{quantity} {symbol} @ {price} STOP LIMIT", Some(stop))
        }
        _ => ("{quantity} {symbol} MKT", None),
    };
    let quantity = numbers.quantity(request.quantity, quantity_decimals);
    let price = price.map(|price| numbers.price(price)).unwrap_or_default();
    l10n::format_text(
        ctx,
        key,
        &[
            ("quantity", quantity.as_str()),
            ("symbol", request.symbol.as_str()),
            ("price", price.as_str()),
        ],
    )
}

/// The question that confirms an order: "Buy 1 ES @ 4,000.25 LIMIT?", then its exits.
fn confirmation(
    ctx: &egui::Context,
    request: &OrderRequest,
    numbers: &Numbers<'_>,
    quantity_decimals: usize,
) -> String {
    let order = summary(ctx, request, numbers, quantity_decimals);
    let question = match request.side {
        OrderSide::Buy => "Buy {order}?",
        OrderSide::Sell => "Sell {order}?",
    };
    let mut text = l10n::format_text(ctx, question, &[("order", order)]);
    let mut line = |key: &str, values: &[(&str, String)]| {
        text.push('\n');
        text.push_str(&l10n::format_text(ctx, key, values));
    };
    if let Some(price) = request.take_profit {
        line("Take profit {price}", &[("price", numbers.price(price))]);
    }
    for level in &request.take_profit_levels {
        line(
            "Take profit {price} for {quantity}",
            &[
                ("price", numbers.price(level.price)),
                (
                    "quantity",
                    numbers.quantity(level.quantity, quantity_decimals),
                ),
            ],
        );
    }
    if let Some(price) = request.stop_loss {
        line("Stop loss {price}", &[("price", numbers.price(price))]);
    }
    text
}

/// What the dialog shows for a previewed order: its confirmation, then the broker's estimates
/// and warnings.
pub(super) fn preview_message(
    ctx: &egui::Context,
    request: &OrderRequest,
    estimate: &OrderPreview,
    rules: &TradingRules,
    numbers: &Numbers<'_>,
    currency: &str,
) -> String {
    let mut text = confirmation(ctx, request, numbers, decimals(rules.quantity_step));
    for (key, value) in [
        ("Cost: {amount} {currency}", estimate.cost),
        ("Commission: {amount} {currency}", estimate.commission),
        ("Margin: {amount} {currency}", estimate.margin),
    ] {
        if let Some(value) = value.filter(|value| value.is_finite()) {
            text.push('\n');
            text.push_str(&l10n::format_text(
                ctx,
                key,
                &[
                    ("amount", grouped(value, 2, false).as_str()),
                    ("currency", currency),
                ],
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

    pub(super) fn market(rules: &TradingRules) -> Market<'_> {
        Market {
            symbol: "ES",
            formatter: None,
            pip_value: None,
            last: Some(100.0),
            bid: None,
            ask: None,
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: Some(10_000.0),
            capabilities: Capabilities {
                market: true,
                limit: true,
                stop: true,
                brackets: true,
                ..Capabilities::default()
            },
            rules,
            rules_ready: true,
            offline: None,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &[],
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
            logo: None,
            account: "",
            timezone: crate::scales::TimezoneMode::Utc,

            pnl_unit: crate::chart::PnlUnit::Money,
        }
    }

    /// Applying a preset sizes value and risk again at the new entry and stop; accounts without
    /// brackets use the saved units instead. Fixed units would double the risk in the first row.
    #[test]
    fn presets_size_again_by_their_sizing() {
        let ctx = egui::Context::default();
        let rules = TradingRules::new("ES");
        for (sizing, amount, stop, brackets, saved, applied) in [
            (Sizing::Risk, 1.0, true, true, 50.0, 25.0),
            (Sizing::Value, 10_000.0, false, true, 100.0, 50.0),
            (Sizing::Units, 3.0, false, true, 3.0, 3.0),
            (Sizing::Risk, 1.0, true, false, 50.0, 50.0),
        ] {
            let mut market = market(&rules);
            let ticket = OrderTicket {
                symbol: "ES".into(),
                kind: Kind::Limit,
                price: 100.0,
                sizing,
                amount,
                quantity: amount,
                stop_loss: (stop, Bracket::new(BracketMode::Percent, 2.0)),
                ..OrderTicket::default()
            };
            let preset = ticket.preset(&ctx, "Scalp".into(), &market);
            assert_eq!(preset.quantity, saved, "{sizing:?}");
            market.capabilities.brackets = brackets;
            let mut ticket = OrderTicket {
                symbol: "ES".into(),
                kind: Kind::Limit,
                price: 200.0,
                ..OrderTicket::default()
            };
            ticket.apply(&preset, &market);
            let request = ticket.request(&ctx, &market).unwrap();
            assert_eq!(request.quantity, applied, "{sizing:?}, brackets {brackets}");
            assert_eq!(request.stop_loss, (stop && brackets).then_some(196.0));
        }
    }

    /// Unselected fields survive applying a preset, unsupported selections are skipped, and
    /// older saved presets still restore their units and exits.
    #[test]
    fn presets_restore_only_the_fields_they_keep() {
        let ctx = egui::Context::default();
        let ticks = |v| Bracket::new(BracketMode::Ticks, v);
        let all = PresetFields::default();
        let quantity = PresetFields {
            quantity: true,
            exits: false,
            order_type: false,
            time_in_force: false,
        };
        let kinds = PresetFields {
            quantity: false,
            exits: false,
            order_type: true,
            time_in_force: true,
        };
        // Literal produced by serializing OrderPreset on origin/main.
        let old = r#"{"name":"Scalp","quantity":2.0,"take_profit":null,"stop_loss":{"mode":"Ticks","value":8.0}}"#;
        for (fields, accepts, legacy, kind, tif, units, profit, stop) in [
            (
                all,
                true,
                false,
                Kind::Limit,
                TimeInForce::GoodTillCancelled,
                2.0,
                Some(ticks(8.0)),
                None,
            ),
            (
                quantity,
                true,
                false,
                Kind::Stop,
                TimeInForce::Day,
                2.0,
                None,
                Some(ticks(4.0)),
            ),
            (
                kinds,
                true,
                false,
                Kind::Limit,
                TimeInForce::GoodTillCancelled,
                1.0,
                None,
                Some(ticks(4.0)),
            ),
            (
                all,
                false,
                false,
                Kind::Stop,
                TimeInForce::Day,
                2.0,
                Some(ticks(8.0)),
                None,
            ),
            (
                all,
                true,
                true,
                Kind::Stop,
                TimeInForce::Day,
                2.0,
                None,
                Some(ticks(8.0)),
            ),
        ] {
            let mut rules = TradingRules::new("ES");
            rules.time_in_force.push(TimeInForce::GoodTillCancelled);
            let mut market = market(&rules);
            let saving = OrderTicket {
                symbol: "ES".into(),
                kind: Kind::Limit,
                price: 100.0,
                time_in_force: TimeInForce::GoodTillCancelled,
                quantity: 2.0,
                take_profit: (true, ticks(8.0)),
                preset_fields: fields,
                ..OrderTicket::default()
            };
            let saved = if legacy {
                old.to_owned()
            } else {
                serde_json::to_string(&saving.preset(&ctx, "Scalp".into(), &market)).unwrap()
            };
            let preset: OrderPreset = serde_json::from_str(&saved).unwrap();
            market.capabilities.limit = accepts;
            let mut restricted = rules.clone();
            if !accepts {
                restricted.time_in_force = vec![TimeInForce::Day];
            }
            market.rules = &restricted;
            let mut ticket = OrderTicket {
                symbol: "ES".into(),
                kind: Kind::Stop,
                price: 101.0,
                stop_loss: (true, ticks(4.0)),
                levels: vec![(ticks(12.0), 50.0)],
                ..OrderTicket::default()
            };
            ticket.apply(&preset, &market);
            assert_eq!(
                (
                    ticket.kind,
                    ticket.time_in_force,
                    ticket.sizing,
                    ticket.quantity
                ),
                (kind, tif, Sizing::Units, units),
                "{saved}, accepts {accepts}"
            );
            assert_eq!(ticket.take_profit.0.then_some(ticket.take_profit.1), profit);
            assert_eq!(ticket.stop_loss.0.then_some(ticket.stop_loss.1), stop);
            assert_eq!(ticket.levels.is_empty(), preset.exits);
        }
        let rules = TradingRules::new("ES");
        let market = market(&rules);
        let mut ticket = OrderTicket::default();
        let mut preset = OrderPreset::new("Limit", 1.0);
        preset.order_type = Some(Kind::Limit);
        ticket.apply(&preset, &market);
        assert_eq!(
            (ticket.price, ticket.limit),
            (100.0, 100.0),
            "selecting seeds the entry like its tab"
        );
        let working = Order::new(
            OrderId::new("working"),
            OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Stop(101.0)),
            crate::model::OrderStatus::Working,
        );
        ticket.edit(&working);
        ticket.apply(&preset, &market);
        assert_eq!(
            ticket.kind,
            Kind::Stop,
            "a preset respects the working order type lock"
        );
    }

    /// GTD needs an offered validity and an expiry; other validities never carry a stale expiry.
    /// Reopening a request and orders from the ladder keep the same UTC instant.
    #[test]
    fn good_till_date_needs_the_rules_and_an_expiry() {
        let ctx = egui::Context::default();
        let time = "2026-12-01T21:00:00Z".parse().unwrap();
        for (offered, tif, expires, expected) in [
            (
                false,
                TimeInForce::GoodTillDate,
                Some(time),
                Ok((TimeInForce::Day, None)),
            ),
            (
                true,
                TimeInForce::GoodTillDate,
                Some(time),
                Ok((TimeInForce::GoodTillDate, Some(time))),
            ),
            (
                true,
                TimeInForce::GoodTillDate,
                None,
                Err("Enter the expiry as YYYY-MM-DD HH:MM"),
            ),
            (
                true,
                TimeInForce::Day,
                Some(time),
                Ok((TimeInForce::Day, None)),
            ),
        ] {
            let mut rules = TradingRules::new("ES");
            if offered {
                rules.time_in_force.push(TimeInForce::GoodTillDate);
            }
            let market = market(&rules);
            let mut ticket = OrderTicket {
                symbol: "ES".into(),
                time_in_force: tif,
                expires,
                ..OrderTicket::default()
            };
            let request = ticket.request(&ctx, &market);
            assert_eq!(
                request
                    .as_ref()
                    .map(|r| (r.time_in_force, r.expires))
                    .map_err(String::as_str),
                expected
            );
            if let Ok(request) = request {
                ticket.open(&request);
                assert_eq!(ticket.request(&ctx, &market), Ok(request));
            }
        }
        let mut rules = TradingRules::new("ES");
        rules.time_in_force.push(TimeInForce::GoodTillDate);
        let market = market(&rules);
        let mut order = OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(99.0));
        order.time_in_force = TimeInForce::GoodTillDate;
        for expires in [Some(time), None] {
            let ticket = OrderTicket {
                expires,
                ..OrderTicket::default()
            };
            let got = ticket.with_brackets(&ctx, order.clone(), &market);
            let expected = expires.ok_or("Enter the order's expiry on the Order tab");
            assert_eq!(
                got.as_ref()
                    .map(|r| r.expires.unwrap())
                    .map_err(String::as_str),
                expected
            );
        }
    }

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

    /// A money exit needs a quantity above 0 before it can show a price, and an exit keeps its
    /// value when switched to or from money without one.
    #[test]
    fn money_exits_without_a_quantity_keep_their_value_when_switching_modes() {
        let rules = TradingRules::new("ES");
        let mut capabilities = Capabilities::default();
        (capabilities.market, capabilities.brackets) = (true, true);
        let market = Market {
            symbol: "ES",
            formatter: None,
            pip_value: None,
            last: Some(100.0),
            bid: None,
            ask: None,
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: Some(10_000.0),
            capabilities,
            rules: &rules,
            rules_ready: true,
            offline: None,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &[],
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
            pnl_unit: crate::chart::PnlUnit::Money,
            logo: None,
            account: "",
            timezone: crate::scales::TimezoneMode::Utc,
        };
        for quantity in [0.0, f64::NAN, -1.0] {
            for profit in [true, false] {
                let ctx = egui::Context::default();
                let mut ticket = OrderTicket {
                    quantity,
                    ..OrderTicket::default()
                };
                let bracket = Bracket::new(BracketMode::Money, 25.0);
                if profit {
                    ticket.take_profit = (true, bracket);
                } else {
                    ticket.stop_loss = (true, bracket);
                }
                let frame = |ticket: &mut OrderTicket, events| {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(340.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| ticket.form(ui, &market),
                    );
                    let texts: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some((
                                text.galley.text().to_owned(),
                                text.galley.rect.translate(text.pos.to_vec2()).center(),
                            )),
                            _ => None,
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                    texts
                };
                let find = |texts: &[(String, egui::Pos2)], wanted: &str| {
                    texts.iter().find(|(text, _)| text == wanted).unwrap().1
                };
                let click = |at| {
                    let button = |pressed| egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    [
                        vec![egui::Event::PointerMoved(at)],
                        vec![button(true)],
                        vec![button(false)],
                        vec![],
                    ]
                };
                frame(&mut ticket, vec![]);
                let texts = frame(&mut ticket, vec![]);
                let non_finite = texts
                    .iter()
                    .filter(|(text, _)| matches!(text.as_str(), "inf" | "-inf" | "NaN"))
                    .count();
                assert_eq!(
                    non_finite,
                    usize::from(quantity.is_nan()),
                    "only the invalid quantity itself is shown"
                );
                let exit = if profit { "Take profit" } else { "Stop loss" };
                // Picks `to` from the menu of the exit while it is in mode `from`.
                let switch = |ticket: &mut OrderTicket, from: &str, to: &str| {
                    let mut texts = frame(ticket, vec![]);
                    for events in click(find(&texts, &format!("{exit}, {from}"))) {
                        texts = frame(ticket, events);
                    }
                    for events in click(find(&texts, to)) {
                        frame(ticket, events);
                    }
                    if profit {
                        ticket.take_profit.1
                    } else {
                        ticket.stop_loss.1
                    }
                };
                let price = Bracket::new(BracketMode::Price, 25.0);
                assert_eq!(
                    switch(&mut ticket, "USD", "price"),
                    price,
                    "{quantity} {profit}"
                );
                assert_eq!(
                    switch(&mut ticket, "price", "USD"),
                    bracket,
                    "{quantity} {profit}"
                );

                // Once the quantity is fixed, money exits show their finite price again.
                ticket.quantity = 1.0;
                let texts = frame(&mut ticket, vec![]);
                let expected = if profit { "125.00" } else { "75.00" };
                find(&texts, expected);
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
            offline: None,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &[],
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
            pnl_unit: crate::chart::PnlUnit::Money,
            logo: None,
            account: "",
            timezone: crate::scales::TimezoneMode::Utc,
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
        let ctx = egui::Context::default();
        let request = ticket.request(&ctx, &market).unwrap();
        assert_eq!(
            (request.quantity, request.take_profit, request.stop_loss),
            (50.0, Some(103.0), Some(97.0))
        );
        // The control is on the Order tab for the pending kind.
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ticket.show(
                ui,
                &market,
                &mut std::collections::BTreeMap::new(),
                &mut false,
            );
        });
        let risk = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Risk, % balance")
        });
        output.drop_without_applying_deltas();
        assert!(risk, "limit orders offer the risk control");
        // A sell stop from the ladder: exits lie the other way from the buy above.
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        let order = ticket.with_brackets(&ctx, order, &market).unwrap();
        assert_eq!(
            (order.take_profit, order.stop_loss),
            (Some(94.0), Some(100.0))
        );
        assert_eq!(
            confirmation(&ctx, &order, &market.numbers(), 0),
            "Sell 3 ES @ 98.00 STOP?\nTake profit 94.00\nStop loss 100.00"
        );
        // A level at 24 ticks = 6 points for half of 3: 1.5 rounds down to 1 and the first takes 2.
        ticket.levels = vec![(ticks(24.0).1, 50.0)];
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        let order = ticket.with_brackets(&ctx, order, &market).unwrap();
        assert_eq!(order.take_profit, None);
        assert_eq!(
            order.take_profit_levels,
            [
                TakeProfitLevel::new(94.0, 2.0),
                TakeProfitLevel::new(92.0, 1.0)
            ]
        );
        assert_eq!(
            confirmation(&ctx, &order, &market.numbers(), 0),
            "Sell 3 ES @ 98.00 STOP?\nTake profit 94.00 for 2\nTake profit 92.00 for 1\nStop loss 100.00"
        );
        // Each line reaches the host's lookup as one template.
        let marked = egui::Context::default();
        crate::localization::mark_keys(&marked);
        assert_eq!(
            confirmation(&marked, &order, &market.numbers(), 0),
            "~~3~ES~98.00~~\n~94.00~2~\n~92.00~1~\n~100.00~"
        );
        ticket.stop_loss = ticks(0.0);
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        assert!(ticket.with_brackets(&ctx, order, &market).is_err());
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
            offline: None,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &[],
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
            pnl_unit: crate::chart::PnlUnit::Money,
            logo: None,
            account: "",
            timezone: crate::scales::TimezoneMode::Utc,
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
                .request(&egui::Context::default(), &market)
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
                    .request(&egui::Context::default(), &market)
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
            let request = ticket.request(&egui::Context::default(), &market);
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
                assert_eq!(
                    reopened.request(&egui::Context::default(), &market),
                    Ok(request)
                );
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
            let request = ticket.request(&egui::Context::default(), &quoted).unwrap();
            assert_eq!(request.take_profit, Some(take_profit), "{side:?}");
        }
        let mut rules = TradingRules::new("BTC");
        rules.quantity_step = 1e-8;
        rules.min_quantity = 1e-8;
        let market = Market {
            symbol: "BTC",
            rules: &rules,
            last: Some(65_000.0),
            bid: None,
            ask: None,
            ..quoted
        };
        // 6,302,184 steps cost 4,096.4196 at 65,000. Reading that value back must keep
        // every step, while an amount genuinely below the boundary still rounds down.
        for (quantity, expected) in [(0.06302184, 0.06302184), (0.06302183999, 0.06302183)] {
            let ticket = OrderTicket {
                symbol: "BTC".into(),
                sizing: Sizing::Value,
                amount: 65_000.0 * (quantity * rules.point_value),
                ..OrderTicket::default()
            };
            let request = ticket.request(&egui::Context::default(), &market).unwrap();
            assert_eq!(request.quantity, expected, "amount from {quantity}");
        }
    }

    /// Guards the reasons the ticket gives for a quantity outside the symbol's limits: the limit
    /// is written as the panel writes quantities, grouped at the step's decimals and exact when
    /// it is finer, or in the host's quantity text. A regression writes it with `to_string` or
    /// passes it by the price decimals.
    #[test]
    fn quantity_limit_reasons_use_the_panel_quantity_text() {
        struct Lots;
        impl TradingFormatter for Lots {
            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "EURUSD").then(|| format!("{:.2} lots", quantity / 1_000.0))
            }
        }
        let mut rules = TradingRules::new("EURUSD");
        rules.min_quantity = 1_000.0;
        let mut capabilities = Capabilities::default();
        (
            capabilities.market,
            capabilities.brackets,
            capabilities.take_profit_levels,
        ) = (true, true, true);
        let ticks = |value| Bracket {
            mode: BracketMode::Ticks,
            value,
        };
        let below = "The quantity is below the minimum of ";
        let above = "The quantity is above the maximum of ";
        let level = "Each take-profit level needs at least the minimum quantity of ";
        // (maximum, units, a second take-profit level with half, formatter) -> reason
        let cases: [(f64, f64, bool, bool, String); 7] = [
            (5_000.0, 500.0, false, false, format!("{below}1,000")),
            (5_000.0, 6_000.0, false, false, format!("{above}5,000")),
            // 1,500 split in half leaves 750 for each level, below 1,000.
            (5_000.0, 1_500.0, true, false, format!("{level}1,000")),
            // A limit finer than the step keeps its decimals.
            (2_500.5, 3_000.0, false, false, format!("{above}2,500.5")),
            (5_000.0, 500.0, false, true, format!("{below}1.00 lots")),
            (5_000.0, 6_000.0, false, true, format!("{above}5.00 lots")),
            (5_000.0, 1_500.0, true, true, format!("{level}1.00 lots")),
        ];
        for (maximum, quantity, split, host, expected) in cases {
            rules.max_quantity = Some(maximum);
            let market = Market {
                symbol: "EURUSD",
                formatter: host.then_some(&Lots as &dyn TradingFormatter),
                pip_value: None,
                last: Some(1.1),
                bid: None,
                ask: None,
                tick: 0.00001,
                decimals: 5,
                currency: "USD",
                balance: None,
                capabilities,
                rules: &rules,
                rules_ready: true,
                offline: None,
                ladder: None,
                ladder_unavailable: None,
                highlights: Vec::new(),
                orders: &[],
                positions: &[],
                trades: &[],
                session_open: None,
                last_fill: None,
                pnl_unit: crate::chart::PnlUnit::Money,
                logo: None,
                account: "",
                timezone: crate::scales::TimezoneMode::Utc,
            };
            let ticket = OrderTicket {
                symbol: "EURUSD".into(),
                quantity,
                take_profit: (split, ticks(10.0)),
                levels: if split {
                    vec![(ticks(20.0), 50.0)]
                } else {
                    Vec::new()
                },
                ..OrderTicket::default()
            };
            let reason = ticket.request(&egui::Context::default(), &market);
            assert_eq!(reason, Err(expected), "{quantity} of at most {maximum}");
        }
        let numbers = Numbers {
            symbol: "EURUSD",
            decimals: 5,
            formatter: None,
        };
        for (limit, step, expected) in [
            (10.0, 0.5, "10.0"),
            (1_000.0, 0.01, "1,000.00"),
            (1e-8, 1e-8, "0.00000001"),
            (1e15, 1.0, "1,000,000,000,000,000"),
        ] {
            assert_eq!(limit_text(&numbers, limit, decimals(step)), expected);
        }
    }
}
