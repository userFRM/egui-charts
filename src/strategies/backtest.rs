//! The strategy trait, the context it trades through and the engine that runs it over bars.

use chrono::{DateTime, TimeDelta, Utc};

use crate::model::rounding_tolerance;
use crate::model::{
    Bar, OrderSide, OrderType, StrategyExit, StrategyReport, StrategyTrade, Timeframe,
};

/// The default entry size, converted to units at the execution price.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum OrderSize {
    /// A fixed number of units.
    Fixed(f64),
    /// A fraction of current marked equity, such as 0.1 for 10%.
    EquityFraction(f64),
    /// An amount in the account currency.
    Cash(f64),
}

/// Commission charged once on the entire quantity of a fill, including a reversal.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Commission {
    /// A fraction of absolute fill value, such as 0.001 for 0.1%.
    Fraction(f64),
    /// An amount per unit filled.
    PerUnit(f64),
    /// An amount per filled order.
    PerOrder(f64),
}

/// Adverse execution adjustment for market and stop fills. Limits never slip.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Slippage {
    /// A number of ticks, each of `tick_size` price units.
    Ticks {
        /// Number of ticks.
        ticks: u32,
        /// Price units per tick.
        tick_size: f64,
    },
    /// A fraction of absolute price.
    Fraction(f64),
}

/// Account and execution assumptions of a [`Backtest`].
///
/// Leverage below one or non-finite counts as one. Negative or non-finite commission amounts
/// count as zero, as in the paper account; slippage amounts follow the same rule. Invalid entry
/// sizes are rejected.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BacktestSettings {
    /// Starting account value in `currency`. Must be finite.
    pub initial_capital: f64,
    /// Currency of capital, equity and fees. Display only; no conversion is performed.
    pub currency: Option<String>,
    /// Annual risk-free fraction passed to the report's performance metrics.
    pub risk_free_rate: f64,
    /// Size used by entries that do not supply their own quantity.
    pub order_size: OrderSize,
    /// Maximum simultaneously open entries in one direction. Zero disables entries.
    pub pyramiding: usize,
    /// Fee paid on each fill and allocated to trades by quantity.
    pub commission: Commission,
    /// Adverse market and stop execution adjustment.
    pub slippage: Slippage,
    /// Margin is absolute position market value divided by this value.
    pub leverage: f64,
    /// Fill market orders at the close of the bar they are placed on.
    pub fill_on_close: bool,
    /// Decide again after every fill, at its execution price, without exposing future bars.
    pub recalculate_after_fills: bool,
    /// First eligible chart-bar open. Earlier bars warm studies and strategy state without trading.
    pub start_time: Option<DateTime<Utc>>,
}

impl Default for BacktestSettings {
    fn default() -> Self {
        Self {
            initial_capital: 10_000.0,
            currency: None,
            risk_free_rate: 0.0,
            order_size: OrderSize::Fixed(1.0),
            pyramiding: 1,
            commission: Commission::Fraction(0.0),
            slippage: Slippage::Fraction(0.0),
            leverage: 1.0,
            fill_on_close: false,
            recalculate_after_fills: false,
            start_time: None,
        }
    }
}

fn nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

impl BacktestSettings {
    fn fee(&self, quantity: f64, price: f64) -> f64 {
        match self.commission {
            Commission::Fraction(rate) => quantity * price.abs() * nonnegative(rate),
            Commission::PerUnit(amount) => quantity * nonnegative(amount),
            Commission::PerOrder(amount) => nonnegative(amount),
        }
    }

    fn slipped(&self, price: f64, side: OrderSide) -> f64 {
        let amount = match self.slippage {
            Slippage::Ticks { ticks, tick_size } => f64::from(ticks) * nonnegative(tick_size),
            Slippage::Fraction(rate) => price.abs() * nonnegative(rate),
        };
        price + sign(side) * amount
    }

    fn leverage(&self) -> f64 {
        if self.leverage.is_finite() {
            self.leverage.max(1.0)
        } else {
            1.0
        }
    }
}

/// A trading strategy that decides on closed bars.
///
/// # Example
///
/// ```
/// use egui_charts::model::{Bar, OrderSide, OrderType, Timeframe};
/// use egui_charts::strategies::{Backtest, Strategy, StrategyContext};
///
/// /// Long while the close is above the previous close, flat otherwise.
/// struct Momentum;
///
/// impl Strategy for Momentum {
///     fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
///         let [.., previous, bar] = context.bars() else {
///             return;
///         };
///         if bar.close > previous.close && context.position() == 0.0 {
///             context.entry("Long", OrderSide::Buy, 10.0, OrderType::Market);
///         } else if bar.close < previous.close && context.position() > 0.0 {
///             context.exit("Close", None, None, None);
///         }
///     }
/// }
///
/// let bar = |minute: i64, price: f64| {
///     let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minute);
///     Bar::new(time, price, price, price, price, 0.0)
/// };
/// let bars = vec![bar(0, 100.0), bar(1, 101.0), bar(2, 103.0), bar(3, 102.0), bar(4, 101.0)];
/// let report = Backtest::new("Momentum", "AAPL", Timeframe::Min1, 10_000.0).run(&mut Momentum, &bars, &[]);
/// // Bought at the next open after the rise to 101, sold at the next open after the fall to 102.
/// assert_eq!(report.trades.len(), 1);
/// assert_eq!(report.trades[0].entry_price, 103.0);
/// assert_eq!(report.trades[0].exit.as_ref().map(|exit| exit.profit), Some(-20.0));
/// assert_eq!(report.equity.len(), bars.len() + 1);
/// ```
pub trait Strategy {
    /// Named strategy inputs, copied to the report's properties before the run.
    fn inputs(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Computes what the strategy needs from the whole run once, before the first bar, such as
    /// its studies. A value at a bar must depend only on that bar and earlier ones, as a study's
    /// does, or the run looks ahead. Does nothing by default.
    fn prepare(&mut self, bars: &[Bar]) {
        let _ = bars;
    }

    /// Decides on the bar that just closed, the last of [`StrategyContext::bars`], by placing and
    /// cancelling orders. With fill recalculation enabled this also runs immediately after a fill;
    /// use the context's price and fill flag rather than reading an unfinished bar. Calls before
    /// the start time warm the strategy's state, but cannot place orders.
    fn on_bar(&mut self, context: &mut StrategyContext<'_>);
}

/// What a [`Strategy`] sees on a closed bar: the bars up to it and the open position. It places
/// and cancels working orders by id.
///
/// An order placed with the id of a working order replaces it, keeping its original place
/// among orders reached at the same point. Working orders stay until they fill or are cancelled. A
/// quantity must be positive and finite and a price finite; an order with any other is ignored.
pub struct StrategyContext<'a> {
    bars: &'a [Bar],
    position: f64,
    orders: &'a mut Vec<Working>,
    price: f64,
    equity: f64,
    after_fill: bool,
    trading: bool,
    tick: u64,
}

impl StrategyContext<'_> {
    /// Closed chart bars, oldest first. During fill recalculation the current bar is excluded.
    pub fn bars(&self) -> &[Bar] {
        self.bars
    }

    /// The open position: positive when long, negative when short, 0 when flat.
    pub fn position(&self) -> f64 {
        self.position
    }

    /// The close on a bar decision, or the execution price immediately after a fill.
    pub fn price(&self) -> f64 {
        self.price
    }

    /// Account equity marked at [`Self::price`], after any fill commission.
    pub fn equity(&self) -> f64 {
        self.equity
    }

    /// Whether this is a recalculation immediately after a fill rather than a closed bar.
    pub fn is_after_fill(&self) -> bool {
        self.after_fill
    }

    /// Places an entry of `quantity` on `side`. Against an open position on the other side it
    /// closes that position first, then opens `quantity` on `side`; on the same side it adds.
    /// Pass `None` to use the settings' order size, calculated when the order fills.
    pub fn entry(
        &mut self,
        id: impl Into<String>,
        side: OrderSide,
        quantity: impl Into<Option<f64>>,
        order_type: OrderType,
    ) {
        let finite = match order_type {
            OrderType::Market => true,
            OrderType::Limit(price) | OrderType::Stop(price) => price.is_finite(),
            OrderType::StopLimit { stop, limit } => stop.is_finite() && limit.is_finite(),
        };
        if finite {
            self.place(id.into(), quantity.into(), Kind::Entry(side, order_type));
        }
    }

    /// Places an exit that reduces the open position by `quantity`, or closes it with `None`. It
    /// fills at the `take_profit` limit or the `stop_loss` stop, whichever the price reaches
    /// first, or at market without either. An exit only reduces the position open when it is
    /// placed or, placed while flat, the position the next entry opens; once that position is
    /// closed or reversed, the exit is cancelled.
    pub fn exit(
        &mut self,
        id: impl Into<String>,
        quantity: Option<f64>,
        take_profit: Option<f64>,
        stop_loss: Option<f64>,
    ) {
        if take_profit.into_iter().chain(stop_loss).all(f64::is_finite) {
            let side = exit_side(self.position);
            self.place(
                id.into(),
                quantity,
                Kind::Exit(take_profit, stop_loss, side),
            );
        }
    }

    /// Cancels the working order with this id, if any.
    pub fn cancel(&mut self, id: &str) {
        self.orders.retain(|order| order.id != id);
    }

    fn place(&mut self, id: String, quantity: Option<f64>, kind: Kind) {
        if !self.trading {
            return;
        }
        if quantity.is_some_and(|quantity| !quantity.is_finite() || quantity <= 0.0) {
            return;
        }
        let order = Working {
            id,
            quantity,
            kind,
            deferred: self.after_fill.then_some(self.tick),
        };
        match self
            .orders
            .iter_mut()
            .find(|working| working.id == order.id)
        {
            Some(working) => *working = order,
            None => self.orders.push(order),
        }
    }
}

/// A working order of a backtest.
struct Working {
    id: String,
    /// `None` for an exit of the whole position or an entry using the default size.
    quantity: Option<f64>,
    kind: Kind,
    /// Orders created after a fill wait for a subsequent price point.
    deferred: Option<u64>,
}

#[derive(Clone, Copy)]
enum Kind {
    Entry(OrderSide, OrderType),
    /// Take profit, stop loss and the exit's side, `None` until a position opens for it to reduce.
    Exit(Option<f64>, Option<f64>, Option<OrderSide>),
}

/// An entry's quantity that is still open.
struct Lot {
    entry: String,
    side: OrderSide,
    quantity: f64,
    entry_quantity: f64,
    time: DateTime<Utc>,
    price: f64,
    fees: f64,
}

/// Runs a [`Strategy`] over bars and reports the result as a [`StrategyReport`].
///
/// The strategy decides on each bar once it closes. Its orders fill no earlier than the next
/// bar, along that bar's price path: from the open to the extreme nearer the open (the high when
/// both are as near within a few ulps), then to the other extreme, then to the close. A market
/// order fills at the open. A limit, stop or stop-limit order fills where the path reaches it, at
/// its limit or stop price, or at the open when the bar opened beyond it; a triggered stop-limit
/// order works on as a limit order from its stop. Orders the path reaches at the same point fill
/// in the order they were first placed. An exit with a take profit and a stop loss fills whichever
/// the path reaches first. With `settings.fill_on_close`, market orders fill at the
/// close of the bar they were placed on instead.
///
/// Each entry is a trade, closed by exits and opposite entries first in first out; each partial
/// exit is its own trade with its share of the entry. A trade's label names its entry and exit
/// orders, such as "Long / Take profit", or the entry alone while open. Profit is the price
/// difference times the quantity, less its allocated fees. Without a start time, the equity curve
/// holds the initial capital one interval before the first bar, then the account value at each
/// bar's close, stamped with the bar's time. With a start time it begins there and subsequent
/// marks use chart-bar closing times. Fills use the times of their chart or finer bars.
///
/// Bars are oldest first, in strictly increasing time order. A bar without finite prices, or
/// whose high and low do not contain its open and close, fills no orders and has no equity value.
/// Settings apply sizing, pyramiding, costs and margin at each execution; a margin deficit closes
/// the position along the path. Optional finer bars determine fills within each chart bar.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Backtest {
    /// What the report calls the run.
    pub name: String,
    /// The instrument the bars are of.
    pub symbol: String,
    /// The bar interval.
    pub timeframe: Timeframe,
    /// Account and execution assumptions, also recorded in the report's properties.
    pub settings: BacktestSettings,
}

impl Backtest {
    /// A backtest that fills market orders at the next bar's open.
    pub fn new(
        name: impl Into<String>,
        symbol: impl Into<String>,
        timeframe: Timeframe,
        initial_capital: f64,
    ) -> Self {
        Self {
            name: name.into(),
            symbol: symbol.into(),
            timeframe,
            settings: BacktestSettings {
                initial_capital,
                ..Default::default()
            },
        }
    }

    /// Runs `strategy` over `bars` and reports its trades, equity and assumptions.
    ///
    /// `lower` supplies finer bars of the same symbol. Valid bars are walked in time order inside
    /// each chart bar; a chart bar without any falls back to its own path. Decisions and equity
    /// remain on the chart interval. Pass `&[]` to use chart bars alone.
    ///
    /// With a start time, `prepare` still receives all bars, but chart bars opening before it
    /// cannot trade. Equity starts at the requested time and subsequent points use chart-bar
    /// closes, so the starting value and first close have distinct timestamps. A final execution
    /// bar without a known end labels its equity mark one nanosecond after its last source time.
    ///
    /// # Panics
    ///
    /// Panics if the starting capital is not finite.
    pub fn run(&self, strategy: &mut dyn Strategy, bars: &[Bar], lower: &[Bar]) -> StrategyReport {
        let settings = &self.settings;
        assert!(
            settings.initial_capital.is_finite(),
            "starting capital must be finite"
        );
        let mut report = StrategyReport::new(
            &self.name,
            &self.symbol,
            self.timeframe,
            settings.initial_capital,
        );
        report.currency = settings.currency.clone();
        report.risk_free_rate = settings.risk_free_rate;
        let size = match settings.order_size {
            OrderSize::Fixed(value) => format!("Fixed units: {value}"),
            OrderSize::EquityFraction(value) => format!("Equity fraction: {value}"),
            OrderSize::Cash(value) => format!("Cash: {value}"),
        };
        let commission = match settings.commission {
            Commission::Fraction(value) => format!("Fraction: {}", nonnegative(value)),
            Commission::PerUnit(value) => format!("Per unit: {}", nonnegative(value)),
            Commission::PerOrder(value) => format!("Per order: {}", nonnegative(value)),
        };
        let slippage = match settings.slippage {
            Slippage::Fraction(value) => format!("Fraction: {}", nonnegative(value)),
            Slippage::Ticks { ticks, tick_size } => {
                format!("{ticks} ticks of {}", nonnegative(tick_size))
            }
        };
        report.properties = vec![
            ("Order size".into(), size),
            ("Pyramiding".into(), settings.pyramiding.to_string()),
            ("Commission".into(), commission),
            ("Slippage".into(), slippage),
            ("Leverage".into(), settings.leverage().to_string()),
            ("Fill on close".into(), settings.fill_on_close.to_string()),
            (
                "Recalculate after fills".into(),
                settings.recalculate_after_fills.to_string(),
            ),
        ];
        report.properties.push((
            "Start time".into(),
            settings
                .start_time
                .map_or_else(|| "All supplied history".into(), |time| time.to_rfc3339()),
        ));
        report
            .properties
            .push(("Lower-interval bars".into(), lower.len().to_string()));
        report.properties.extend(
            strategy
                .inputs()
                .into_iter()
                .map(|(name, value)| (format!("Input: {name}"), value)),
        );
        let mut lower: Vec<_> = lower.iter().filter(|bar| valid_bar(bar)).collect();
        lower.sort_by_key(|bar| bar.time);
        lower.dedup_by_key(|bar| bar.time);
        strategy.prepare(bars);
        let mut run = Run {
            orders: Vec::new(),
            lots: Vec::new(),
            trades: Vec::new(),
            realized: 0.0,
            rejected: 0,
            settings,
            strategy,
            history: &[],
            trading: false,
            tick: 0,
        };
        if let Some(first) = bars
            .iter()
            .find(|bar| settings.start_time.is_none_or(|start| bar.time >= start))
        {
            let interval = self.timeframe.duration().max(TimeDelta::milliseconds(1));
            let before = first.time.checked_sub_signed(interval);
            report.equity.push((
                settings
                    .start_time
                    .unwrap_or(before.unwrap_or(DateTime::<Utc>::MIN_UTC)),
                settings.initial_capital,
            ));
        }
        for (index, bar) in bars.iter().enumerate() {
            run.trading = settings.start_time.is_none_or(|start| bar.time >= start);
            if !run.trading {
                run.history = &bars[..=index];
                run.decide(bar.close, false);
                continue;
            }
            let end = bar_end(self.timeframe, bar, bars.get(index + 1));
            run.history = &bars[..index];
            let valid = valid_bar(bar);
            let from = lower.partition_point(|lower| lower.time < bar.time);
            let to = lower.partition_point(|lower| end.is_none_or(|end| lower.time < end));
            let last_price_time = if from < to {
                lower[to - 1].time
            } else {
                bar.time
            };
            if valid {
                if from < to {
                    for lower in &lower[from..to] {
                        run.fill_bar(lower);
                    }
                } else {
                    run.fill_bar(bar);
                }
            }
            run.history = &bars[..=index];
            run.decide(bar.close, false);
            if valid && settings.fill_on_close {
                let close_time = if from < to || settings.start_time.is_some() {
                    end.unwrap_or(last_price_time)
                } else {
                    bar.time
                };
                run.fill_markets(bar.close, close_time);
            }
            report.equity.push((
                if settings.start_time.is_some() {
                    // A final execution bar has no known end; keep its mark after its last price.
                    end.unwrap_or_else(|| {
                        last_price_time
                            .checked_add_signed(TimeDelta::nanoseconds(1))
                            .unwrap_or(last_price_time)
                    })
                } else {
                    bar.time
                },
                if valid {
                    run.equity(bar.close)
                } else {
                    f64::NAN
                },
            ));
        }
        report
            .properties
            .push(("Rejected orders".into(), run.rejected.to_string()));
        report.trades = run.trades;
        report.trades.extend(run.lots.into_iter().map(|lot| {
            let mut trade = StrategyTrade::new(lot.side, lot.quantity, lot.time, lot.price);
            trade.label = Some(lot.entry);
            trade.fees = lot.fees;
            trade
        }));
        report
    }
}

/// Calendar months have calendar boundaries; execution bars end at the next chart open.
fn bar_end(timeframe: Timeframe, bar: &Bar, next: Option<&Bar>) -> Option<DateTime<Utc>> {
    match timeframe {
        Timeframe::Month1 => bar.time.checked_add_months(chrono::Months::new(1)),
        Timeframe::Months(count) => bar
            .time
            .checked_add_months(chrono::Months::new(count.get())),
        _ if timeframe.is_trade_based() => next.map(|bar| bar.time),
        _ => bar.time.checked_add_signed(timeframe.duration()),
    }
}

fn valid_bar(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close]
        .iter()
        .all(|price| price.is_finite())
        && bar.low <= bar.open.min(bar.close)
        && bar.high >= bar.open.max(bar.close)
}

/// The side of an exit from `position`, `None` while flat.
fn exit_side(position: f64) -> Option<OrderSide> {
    if position > 0.0 {
        Some(OrderSide::Sell)
    } else if position < 0.0 {
        Some(OrderSide::Buy)
    } else {
        None
    }
}

fn sign(side: OrderSide) -> f64 {
    match side {
        OrderSide::Buy => 1.0,
        OrderSide::Sell => -1.0,
    }
}

struct Run<'a> {
    orders: Vec<Working>,
    /// Open entries, oldest first, all on one side.
    lots: Vec<Lot>,
    trades: Vec<StrategyTrade>,
    /// Realized price profits less every commission paid, including open entries' fees.
    realized: f64,
    rejected: usize,
    settings: &'a BacktestSettings,
    strategy: &'a mut dyn Strategy,
    history: &'a [Bar],
    trading: bool,
    tick: u64,
}

impl Run<'_> {
    fn position(&self) -> f64 {
        self.lots.iter().fold(0.0, |position, lot| {
            position + sign(lot.side) * lot.quantity
        })
    }

    fn equity(&self, price: f64) -> f64 {
        self.settings.initial_capital
            + self.realized
            + self
                .lots
                .iter()
                .map(|lot| sign(lot.side) * (price - lot.price) * lot.quantity)
                .sum::<f64>()
    }

    fn decide(&mut self, price: f64, after_fill: bool) {
        let position = self.position();
        let equity = self.equity(price);
        self.strategy.on_bar(&mut StrategyContext {
            bars: self.history,
            position,
            orders: &mut self.orders,
            price,
            equity,
            after_fill,
            trading: self.trading,
            tick: self.tick,
        });
    }

    fn fill_bar(&mut self, bar: &Bar) {
        self.tick += 1;
        let up = bar.high - bar.open;
        let down = bar.open - bar.low;
        let near = if up - down
            <= rounding_tolerance(bar.high.abs().max(bar.low.abs()), 4.0 * f64::EPSILON)
        {
            bar.high
        } else {
            bar.low
        };
        let far = if near == bar.high { bar.low } else { bar.high };
        for (start, end) in [(bar.open, near), (near, far), (far, bar.close)] {
            self.fill_along(start, end, bar.time);
        }
    }

    /// Trigger price and whether the execution slips. Stop-limit triggers are not executions.
    fn fill_price(&self, order: &Working, start: f64, end: f64) -> Option<(f64, bool)> {
        let reached = match order.kind {
            Kind::Entry(side, order_type) => order_type.fill_price(side, start, end).map(|price| {
                (
                    price,
                    matches!(order_type, OrderType::Market | OrderType::Stop(_)),
                )
            }),
            Kind::Exit(take_profit, stop_loss, side) => {
                let side = side?;
                if take_profit.is_none() && stop_loss.is_none() {
                    Some((start, true))
                } else {
                    // The stop first, so it wins a tie.
                    [
                        (stop_loss.map(OrderType::Stop), true),
                        (take_profit.map(OrderType::Limit), false),
                    ]
                    .into_iter()
                    .filter_map(|(order_type, slips)| {
                        order_type?
                            .fill_price(side, start, end)
                            .map(|price| (price, slips))
                    })
                    .min_by(|(a, _), (b, _)| (a - start).abs().total_cmp(&(b - start).abs()))
                }
            }
        }?;
        if order.deferred == Some(self.tick) && reached.0 == start {
            // A newly placed marketable order waits for the next price point, avoiding a
            // strategy repeatedly entering and exiting on the same tick.
            if start == end {
                return None;
            }
            let next = Working {
                id: String::new(),
                quantity: order.quantity,
                kind: order.kind,
                deferred: None,
            };
            self.fill_price(&next, end, end)
        } else {
            Some(reached)
        }
    }

    /// First point where equity would fall below market-value margin on this linear segment.
    fn margin_call(&self, start: f64, end: f64) -> Option<f64> {
        if self.lots.is_empty() {
            return None;
        }
        let available = |price: f64| {
            self.equity(price) - self.position().abs() * price.abs() / self.settings.leverage()
        };
        let mut from = start;
        // Absolute market value changes slope at zero.
        for to in [
            if start.min(end) < 0.0 && start.max(end) > 0.0 {
                0.0
            } else {
                end
            },
            end,
        ] {
            let a = available(from);
            let b = available(to);
            if a < 0.0 {
                return Some(from);
            }
            if b < 0.0 {
                return Some(from + (to - from) * (a / (a - b)));
            }
            from = to;
        }
        None
    }

    /// Fills orders and margin calls in price order along a bar segment.
    fn fill_along(&mut self, mut start: f64, end: f64, time: DateTime<Utc>) {
        let mut at_end = false;
        loop {
            let mut next: Option<(usize, f64, bool)> = None;
            for (index, order) in self.orders.iter().enumerate() {
                if let Some((price, slips)) = self.fill_price(order, start, end)
                    && next.is_none_or(|(_, best, _)| (price - start).abs() < (best - start).abs())
                {
                    next = Some((index, price, slips));
                }
            }
            // An order at the boundary can avert a deficit; an existing deficit wins a tie.
            if let Some(price) = self.margin_call(start, end)
                && next.is_none_or(|(_, best, _)| {
                    (price - start).abs() < (best - start).abs()
                        || (price == start
                            && self.equity(start)
                                < self.position().abs() * start.abs() / self.settings.leverage())
                })
            {
                if price != start {
                    self.tick += 1;
                    at_end = price == end;
                }
                start = price;
                self.fill(
                    Working {
                        id: "Margin call".into(),
                        quantity: None,
                        kind: Kind::Exit(None, None, exit_side(self.position())),
                        deferred: None,
                    },
                    price,
                    true,
                    time,
                );
                if !self.lots.is_empty() {
                    return;
                }
                continue;
            }
            let Some((index, price, slips)) = next else {
                if !at_end {
                    // Equal OHLC prices still denote successive points in the bar.
                    self.tick += 1;
                    start = end;
                    at_end = true;
                    continue;
                }
                return;
            };
            if price != start {
                self.tick += 1;
                at_end = price == end;
            }
            start = price;
            if let Kind::Entry(side, OrderType::StopLimit { limit, .. }) = self.orders[index].kind {
                self.orders[index].kind = Kind::Entry(side, OrderType::Limit(limit));
                self.orders[index].deferred = None;
                continue;
            }
            let order = self.orders.remove(index);
            self.fill(order, price, slips, time);
        }
    }

    /// Fill only market orders on the closing tick, then check the account's margin.
    fn fill_markets(&mut self, price: f64, time: DateTime<Utc>) {
        let mut index = 0;
        while index < self.orders.len() {
            let order = &self.orders[index];
            let market = matches!(
                order.kind,
                Kind::Entry(_, OrderType::Market) | Kind::Exit(None, None, _)
            );
            if market && self.fill_price(order, price, price).is_some() {
                let order = self.orders.remove(index);
                self.fill(order, price, true, time);
                index = 0;
            } else {
                index += 1;
            }
        }
        if self.margin_call(price, price).is_some() {
            self.fill(
                Working {
                    id: "Margin call".into(),
                    quantity: None,
                    kind: Kind::Exit(None, None, exit_side(self.position())),
                    deferred: None,
                },
                price,
                true,
                time,
            );
        }
    }

    fn fill(&mut self, order: Working, mark: f64, slips: bool, time: DateTime<Utc>) {
        let side = match order.kind {
            Kind::Entry(side, _) => side,
            Kind::Exit(.., Some(side)) => side,
            Kind::Exit(.., None) => return,
        };
        let price = if slips {
            self.settings.slipped(mark, side)
        } else {
            mark
        };
        let held = self.position().abs();
        match order.kind {
            Kind::Entry(..) => {
                let reversing = self.lots.first().is_some_and(|lot| lot.side != side);
                if self.settings.pyramiding == 0
                    || (!reversing && self.lots.len() >= self.settings.pyramiding)
                {
                    return;
                }
                let quantity = order
                    .quantity
                    .unwrap_or_else(|| match self.settings.order_size {
                        OrderSize::Fixed(value) => value,
                        OrderSize::EquityFraction(value) => self.equity(mark) * value / price.abs(),
                        OrderSize::Cash(value) => value / price.abs(),
                    });
                let filled = quantity + if reversing { held } else { 0.0 };
                let fee = self.settings.fee(filled, price);
                let after = quantity + if reversing { 0.0 } else { held };
                let equity = self.equity(mark) + sign(side) * (mark - price) * filled;
                let needed = after * mark.abs() / self.settings.leverage() + fee;
                if !quantity.is_finite()
                    || quantity <= 0.0
                    || !price.is_finite()
                    || !needed.is_finite()
                    || !equity.is_finite()
                    || needed > equity
                {
                    self.rejected += 1;
                    return;
                }
                self.realized -= fee;
                if reversing {
                    self.close(f64::INFINITY, &order.id, price, fee, filled, time);
                }
                self.lots.push(Lot {
                    entry: order.id,
                    side,
                    quantity,
                    entry_quantity: quantity,
                    time,
                    price,
                    fees: fee * (quantity / filled),
                });
            }
            Kind::Exit(..) => {
                let quantity = order.quantity.unwrap_or(held).min(held);
                if quantity <= 0.0 {
                    return;
                }
                let fee = self.settings.fee(quantity, price);
                if !price.is_finite() || !fee.is_finite() {
                    return;
                }
                self.realized -= fee;
                self.close(
                    order.quantity.unwrap_or(f64::INFINITY),
                    &order.id,
                    price,
                    fee,
                    quantity,
                    time,
                );
            }
        }
        let position = self.position();
        self.orders.retain_mut(|order| match &mut order.kind {
            Kind::Exit(.., side) => {
                *side = side.or(exit_side(position));
                side.is_none_or(|side| side.reduces(position))
            }
            Kind::Entry(..) => true,
        });
        if self.settings.recalculate_after_fills {
            self.decide(price, true);
        }
    }

    /// Close FIFO lots, sharing entry and exit commissions by quantity without charging again.
    fn close(
        &mut self,
        mut quantity: f64,
        exit: &str,
        price: f64,
        fee: f64,
        filled_quantity: f64,
        time: DateTime<Utc>,
    ) {
        let exit_quantity = quantity;
        while quantity > 0.0
            && let Some(lot) = self.lots.first_mut()
        {
            let closed = quantity.min(lot.quantity);
            let entry_fee = if lot.quantity - closed <= rounding_tolerance(lot.entry_quantity, 1e-9)
            {
                lot.fees
            } else {
                lot.fees * (closed / lot.quantity)
            };
            let fees = entry_fee + fee * (closed / filled_quantity);
            let gross = sign(lot.side) * (price - lot.price) * closed;
            let mut trade = StrategyTrade::new(lot.side, closed, lot.time, lot.price);
            trade.exit = Some(StrategyExit::new(time, price, gross - fees + 0.0));
            trade.fees = fees;
            trade.label = Some(format!("{} / {exit}", lot.entry));
            self.trades.push(trade);
            self.realized += gross;
            quantity -= closed;
            lot.quantity -= closed;
            lot.fees -= entry_fee;
            if quantity.is_finite() && quantity <= rounding_tolerance(exit_quantity, 1e-9) {
                quantity = 0.0;
            }
            if lot.quantity <= rounding_tolerance(lot.entry_quantity, 1e-9) {
                self.lots.remove(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Places the orders of a script on the bars it names.
    struct Script(fn(usize, &mut StrategyContext<'_>));

    impl Strategy for Script {
        fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
            if context.position() == 0.0 {
                assert!(context.position().is_sign_positive());
            }
            (self.0)(context.bars().len() - 1, context);
        }
    }

    fn hour(hour: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + TimeDelta::hours(hour)
    }

    fn script(bar: usize, context: &mut StrategyContext<'_>) {
        use OrderSide::{Buy, Sell};
        match bar {
            0 => {
                context.entry("Long", Buy, 10.0, OrderType::Market);
                context.exit("Bracket", None, Some(105.0), Some(98.0));
            }
            2 => {
                let stop_limit = OrderType::StopLimit {
                    stop: 102.0,
                    limit: 102.5,
                };
                context.entry("Short", Sell, 5.0, stop_limit);
                context.entry("Long2", Buy, 4.0, OrderType::Limit(97.0));
            }
            3 => context.entry("Long2", Buy, 4.0, OrderType::Limit(100.0)),
            4 => {
                context.entry("Add", Buy, 2.0, OrderType::Market);
                context.exit("Part", Some(5.0), None, None);
            }
            5 => {
                context.exit("Flat", Some(100.0), None, None);
                context.entry("Last", Buy, 1.0, OrderType::Market);
            }
            6 => {
                context.exit("Guard", None, Some(110.0), Some(90.0));
                context.entry("Flip", Sell, 1.0, OrderType::Market);
            }
            7 => context.exit("Cover", None, Some(96.0), Some(103.0)),
            8 => {
                context.entry("Long3", Buy, 1.0, OrderType::Market);
                context.exit("Target", None, Some(104.0), Some(90.0));
            }
            9 => context.exit("Out", None, None, None),
            10 => context.entry("Long4", Buy, 1.0, OrderType::Market),
            11 => {
                context.entry("Reverse", Sell, 1.0, OrderType::Stop(103.0));
                context.exit("Stop", None, Some(108.0), Some(103.0));
            }
            _ => {}
        }
    }

    /// The script with the long's exit placed before the stop and reverse entry on bar 11.
    fn stop_first(bar: usize, context: &mut StrategyContext<'_>) {
        if bar == 11 {
            context.exit("Stop", None, Some(108.0), Some(103.0));
        }
        script(bar, context);
    }

    // Worked by hand from the path rule documented on `Backtest`.
    #[test]
    fn run_fills_along_the_bar_path_and_pairs_trades_first_in_first_out() {
        let bars: Vec<_> = [
            (100.0, 101.0, 99.0, 100.0),
            // Low first: the bracket's stop at 98 and take profit at 105 are not reached.
            (101.0, 103.0, 100.0, 102.0),
            // High first: the take profit at 105 fills before the low reaches the stop.
            (102.0, 106.0, 97.0, 105.0),
            // The short's stop triggers at 102 on the way down, its limit fills at 102.5 on the
            // way up; the replaced buy limit at 97 is not reached.
            (104.0, 105.0, 101.0, 103.0),
            // Gap below the buy limit at 100: fills at the open, closing the short first. The
            // replaced limit at 97 would fill on the way down.
            (98.0, 100.0, 95.0, 99.0),
            // At the open, Add fills, then Part closes Long2's 4 and 1 of Add.
            (99.0, 101.0, 98.0, 100.0),
            // At the open, Flat closes only the remaining 1 of Add, then Last opens.
            (100.0, 101.0, 99.0, 100.5),
            // At the open, Flip closes Last and opens a short. Guard, placed for Last, is
            // cancelled rather than closing the short at once.
            (100.0, 101.0, 99.0, 100.0),
            // Low first: the short's take profit at 96 fills before the high reaches its stop.
            (100.0, 105.0, 96.0, 97.0),
            // Long3 opens; Target, placed while flat, protects it.
            (97.0, 98.0, 96.0, 97.5),
            // Out closes Long3, which cancels Target.
            (98.0, 99.0, 97.0, 98.0),
            // Long4 opens above Target's take profit at 104 and stays open.
            (105.0, 106.0, 104.5, 105.5),
            // Low first: at 103 Reverse closes Long4 and opens a short, which cancels Stop, so
            // neither its stop at 103 nor its take profit at 108 closes the short.
            (105.0, 108.5, 102.0, 102.5),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close))| Bar::new(hour(i as i64), open, high, low, close, 0.0))
        .collect();
        let mut backtest = Backtest::new("Script", "TEST", Timeframe::Hour1, 10_000.0);
        backtest.settings.pyramiding = usize::MAX;
        let report = backtest.run(&mut Script(script), &bars, &[]);

        let trade = |side,
                     quantity,
                     entry: i64,
                     entry_price,
                     exit: Option<(i64, f64, f64)>,
                     label: &str| {
            let mut trade = StrategyTrade::new(side, quantity, hour(entry), entry_price);
            trade.exit =
                exit.map(|(time, price, profit)| StrategyExit::new(hour(time), price, profit));
            trade.label = Some(label.to_owned());
            trade
        };
        use OrderSide::{Buy, Sell};
        assert_eq!(
            report.trades,
            [
                trade(
                    Buy,
                    10.0,
                    1,
                    101.0,
                    Some((2, 105.0, 40.0)),
                    "Long / Bracket"
                ),
                trade(Sell, 5.0, 3, 102.5, Some((4, 98.0, 22.5)), "Short / Long2"),
                trade(Buy, 4.0, 4, 98.0, Some((5, 99.0, 4.0)), "Long2 / Part"),
                trade(Buy, 1.0, 5, 99.0, Some((5, 99.0, 0.0)), "Add / Part"),
                trade(Buy, 1.0, 5, 99.0, Some((6, 100.0, 1.0)), "Add / Flat"),
                trade(Buy, 1.0, 6, 100.0, Some((7, 100.0, 0.0)), "Last / Flip"),
                trade(Sell, 1.0, 7, 100.0, Some((8, 96.0, 4.0)), "Flip / Cover"),
                trade(Buy, 1.0, 9, 97.0, Some((10, 98.0, 1.0)), "Long3 / Out"),
                trade(
                    Buy,
                    1.0,
                    11,
                    105.0,
                    Some((12, 103.0, -2.0)),
                    "Long4 / Reverse"
                ),
                trade(Sell, 1.0, 12, 103.0, None, "Reverse"),
            ]
        );
        let equity = [
            10_000.0, 10_000.0, 10_010.0, 10_040.0, 10_037.5, 10_066.5, 10_067.5, 10_068.0,
            10_067.5, 10_071.5, 10_072.0, 10_072.5, 10_073.0, 10_071.0,
        ];
        let times = std::iter::once(hour(-1)).chain((0..13).map(hour));
        assert_eq!(report.equity, times.zip(equity).collect::<Vec<_>>());
        assert_eq!(report.equity_curve(), report.equity);
        // Closed profits: 40 + 22.5 + 4 + 0 + 1 + 0 + 4 + 1 - 2 = 70.5.
        let metrics = report.performance();
        assert_eq!(
            (
                metrics.closed_trades,
                metrics.open_trades,
                metrics.skipped_trades
            ),
            (9, 1, 0)
        );
        assert_eq!((metrics.winning_trades, metrics.losing_trades), (6, 1));
        assert_eq!(
            (metrics.net_profit, metrics.gross_profit, metrics.gross_loss),
            (70.5, 72.5, 2.0)
        );
        assert_eq!(metrics.win_rate, Some(6.0 / 9.0));
        assert_eq!(metrics.profit_factor, Some(36.25));

        // Without the later bars the run sees and does the same up to its last bar.
        let first = backtest.run(&mut Script(script), &bars[..4], &[]);
        assert_eq!(first.equity, report.equity[..5]);
        assert_eq!(first.trades[0], report.trades[0]);
        assert_eq!(first.trades[1], trade(Sell, 5.0, 3, 102.5, None, "Short"));

        // Placing the long's exit first only changes which of the two closes the long at 103.
        let stop_first = backtest.run(&mut Script(stop_first), &bars, &[]);
        assert_eq!(stop_first.equity, report.equity);
        let mut trades = report.trades.clone();
        trades[8].label = Some("Long4 / Stop".to_owned());
        assert_eq!(stop_first.trades, trades);

        // On close, the first market entry fills at the close of the bar it was placed on.
        let mut on_close = backtest.clone();
        on_close.settings.fill_on_close = true;
        let on_close = on_close.run(&mut Script(script), &bars, &[]);
        assert_eq!(
            on_close.trades[0],
            trade(
                Buy,
                10.0,
                0,
                100.0,
                Some((2, 105.0, 50.0)),
                "Long / Bracket"
            )
        );

        fn decimals(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => {
                    context.entry("A", Buy, 0.1, OrderType::Market);
                    context.entry("B", Buy, 0.2, OrderType::Market);
                }
                1 => context.exit("X", Some(0.3), None, None),
                _ => {}
            }
        }
        fn thirds(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, 0.3, OrderType::Market),
                1 => context.entry("B", Buy, 0.5, OrderType::Market),
                2 => context.exit("X1", Some(0.1), None, None),
                3 => context.exit("X2", Some(0.1), None, None),
                4 => context.exit("X3", Some(0.1), None, None),
                _ => {}
            }
        }
        fn tenths(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, 1.0, OrderType::Market),
                1..=10 => context.exit("X", Some(0.1), None, None),
                11 => assert_eq!(context.position(), 0.0),
                _ => {}
            }
        }
        fn pyramided_scale_out(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => {
                    context.entry("A", Buy, 1.4, OrderType::Market);
                    context.entry("B", Buy, 3.2, OrderType::Market);
                }
                1..=14 => context.exit("X", Some(0.1), None, None),
                15..=78 => context.exit("X", Some(0.05), None, None),
                79 => assert_eq!(context.position(), 0.0),
                _ => {}
            }
        }
        fn decimal_tie(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("L", Buy, 1.0, OrderType::Market);
                context.exit("B", None, Some(1.2), Some(1.1));
            }
        }
        fn invalid_range(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("L", Buy, 1.0, OrderType::Limit(104.0));
            }
        }
        fn short_breakeven(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("S", Sell, 1.0, OrderType::Market),
                1 => context.exit("X", None, None, None),
                _ => {}
            }
        }
        fn low_first_stop(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("L", Buy, 1.0, OrderType::Market),
                1 => context.exit("B", None, Some(104.0), Some(98.0)),
                _ => {}
            }
        }
        fn short_stop(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("S", Sell, 1.0, OrderType::Market),
                1 => context.exit("B", None, Some(97.0), Some(101.0)),
                _ => {}
            }
        }
        fn stop_limit_gap(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry(
                    "G",
                    Buy,
                    1.0,
                    OrderType::StopLimit {
                        stop: 102.0,
                        limit: 103.0,
                    },
                );
            }
        }
        fn replaced_first(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, 1.0, OrderType::Market);
                context.entry("B", Buy, 1.0, OrderType::Market);
                context.exit("X", Some(1.0), None, None);
                context.entry("A", Buy, 2.0, OrderType::Market);
            }
        }
        // Decimal lots close at their entry price: two breakevens, flat equity and no open lot.
        // Thirds realise 3 * 0.1 * (102 - 100) = 0.6, all winners. B keeps 0.5 at 104,
        // so its mark at 102 is -1 and final equity is 9999.6. No fourth closed trade.
        // Decimal tie: high first closes 1 long at 1.2 from 1.15, profit 0.05, one winner.
        // Invalid open and close bars do not fill or value the account. The next valid bar
        // opens the long at 103, with no closed trades or realised profit and equity 10000.
        for (script, prices, expected, equity, closed, open, wins, profit) in [
            (
                decimals as fn(usize, &mut StrategyContext<'_>),
                vec![(100.0, 100.0, 100.0, 100.0); 4],
                vec![
                    trade(Buy, 0.1, 1, 100.0, Some((2, 100.0, 0.0)), "A / X"),
                    trade(Buy, 0.2, 1, 100.0, Some((2, 100.0, 0.0)), "B / X"),
                ],
                vec![10_000.0; 5],
                2,
                0,
                0,
                0.0,
            ),
            (
                thirds,
                [100.0, 100.0, 104.0, 102.0, 102.0, 102.0]
                    .into_iter()
                    .map(|p| (p, p, p, p))
                    .collect(),
                vec![
                    trade(Buy, 0.1, 1, 100.0, Some((3, 102.0, 0.2)), "A / X1"),
                    trade(Buy, 0.1, 1, 100.0, Some((4, 102.0, 0.2)), "A / X2"),
                    trade(Buy, 0.1, 1, 100.0, Some((5, 102.0, 0.2)), "A / X3"),
                    trade(Buy, 0.5, 2, 104.0, None, "B"),
                ],
                vec![
                    10_000.0, 10_000.0, 10_000.0, 10_001.2, 9999.6, 9999.6, 9999.6,
                ],
                3,
                1,
                3,
                0.6,
            ),
            (
                // 1 - 10 * 0.1 = 0: ten breakevens, flat equity and no open lot.
                tenths,
                vec![(100.0, 100.0, 100.0, 100.0); 12],
                (2..=11)
                    .map(|exit| trade(Buy, 0.1, 1, 100.0, Some((exit, 100.0, 0.0)), "A / X"))
                    .collect(),
                vec![10_000.0; 13],
                10,
                0,
                0,
                0.0,
            ),
            (
                // 1.4 - 14 * 0.1 = 0, 3.2 - 64 * 0.05 = 0: 78 breakevens and flat equity.
                pyramided_scale_out,
                vec![(100.0, 100.0, 100.0, 100.0); 80],
                (2..=79)
                    .map(|exit| {
                        let (quantity, label) = if exit <= 15 {
                            (0.1, "A / X")
                        } else {
                            (0.05, "B / X")
                        };
                        trade(Buy, quantity, 1, 100.0, Some((exit, 100.0, 0.0)), label)
                    })
                    .collect(),
                vec![10_000.0; 81],
                78,
                0,
                0,
                0.0,
            ),
            (
                decimal_tie,
                vec![
                    (1.15, 1.16, 1.14, 1.15),
                    (1.15, 1.16, 1.14, 1.15),
                    (1.15, 1.2, 1.1, 1.15),
                ],
                vec![trade(Buy, 1.0, 1, 1.15, Some((2, 1.2, 0.05)), "L / B")],
                vec![10_000.0, 10_000.0, 10_000.0, 10_000.05],
                1,
                0,
                1,
                0.05,
            ),
            (
                invalid_range,
                vec![
                    (100.0, 101.0, 99.0, 100.0),
                    (105.0, 101.0, 99.0, 100.0),
                    (100.0, 101.0, 99.0, 102.0),
                    (103.0, 104.0, 102.0, 103.0),
                ],
                vec![trade(Buy, 1.0, 3, 103.0, None, "L")],
                vec![10_000.0, 10_000.0, f64::NAN, f64::NAN, 10_000.0],
                0,
                1,
                0,
                0.0,
            ),
            // A short closed at entry breaks even: positive zero profit, flat equity.
            (
                short_breakeven,
                vec![(100.0, 100.0, 100.0, 100.0); 3],
                vec![trade(Sell, 1.0, 1, 100.0, Some((2, 100.0, 0.0)), "S / X")],
                vec![10_000.0; 4],
                1,
                0,
                0,
                0.0,
            ),
            // Low first: the long's raised stop wins before its target. Profit = 98 - 90 = 8.
            (
                low_first_stop,
                vec![(90.0, 90.0, 90.0, 90.0); 2]
                    .into_iter()
                    .chain([(100.0, 105.0, 97.0, 100.0)])
                    .collect(),
                vec![trade(Buy, 1.0, 1, 90.0, Some((2, 98.0, 8.0)), "L / B")],
                vec![10_000.0, 10_000.0, 10_000.0, 10_008.0],
                1,
                0,
                1,
                8.0,
            ),
            // High first: the short's lowered stop wins before its target. Profit = 110 - 101 = 9.
            (
                short_stop,
                vec![(110.0, 110.0, 110.0, 110.0); 2]
                    .into_iter()
                    .chain([(100.0, 102.0, 96.0, 100.0)])
                    .collect(),
                vec![trade(Sell, 1.0, 1, 110.0, Some((2, 101.0, 9.0)), "S / B")],
                vec![10_000.0, 10_000.0, 10_000.0, 10_009.0],
                1,
                0,
                1,
                9.0,
            ),
            // Empty and one-bar runs cannot fill next-bar orders.
            (decimals, vec![], vec![], vec![], 0, 0, 0, 0.0),
            (
                decimals,
                vec![(100.0, 100.0, 100.0, 100.0)],
                vec![],
                vec![10_000.0; 2],
                0,
                0,
                0,
                0.0,
            ),
            // A NaN bar keeps the limit pending, just like an invalid range.
            (
                invalid_range,
                vec![
                    (100.0, 101.0, 99.0, 100.0),
                    (100.0, f64::NAN, 99.0, 100.0),
                    (103.0, 104.0, 102.0, 103.0),
                ],
                vec![trade(Buy, 1.0, 2, 103.0, None, "L")],
                vec![10_000.0, 10_000.0, f64::NAN, 10_000.0],
                0,
                1,
                0,
                0.0,
            ),
            // A gap triggers the stop but cannot fill above the limit. The next bar fills
            // at 103 and marks at 104: one open lot, equity +1 and no realised profit.
            (
                stop_limit_gap,
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (105.0, 106.0, 104.0, 105.0),
                    (104.0, 105.0, 102.0, 104.0),
                ],
                vec![trade(Buy, 1.0, 2, 103.0, None, "G")],
                vec![10_000.0, 10_000.0, 10_000.0, 10_001.0],
                0,
                1,
                0,
                0.0,
            ),
            // Replaced A still precedes B and X. X closes 1 of A on the entry bar;
            // the other 1 of A and 1 of B remain, with flat equity and a breakeven close.
            (
                replaced_first,
                vec![(100.0, 100.0, 100.0, 100.0); 2],
                vec![
                    trade(Buy, 1.0, 1, 100.0, Some((1, 100.0, 0.0)), "A / X"),
                    trade(Buy, 1.0, 1, 100.0, None, "A"),
                    trade(Buy, 1.0, 1, 100.0, None, "B"),
                ],
                vec![10_000.0; 3],
                1,
                2,
                0,
                0.0,
            ),
        ] {
            let source: Vec<_> = prices
                .into_iter()
                .enumerate()
                .map(|(i, (open, high, low, close))| {
                    Bar::new(hour(i as i64), open, high, low, close, 0.0)
                })
                .collect();
            let report = backtest.run(&mut Script(script), &source, &[]);
            assert_eq!(report.trades.len(), expected.len());
            for (mut actual, expected) in report.trades.iter().cloned().zip(expected) {
                assert!((actual.quantity - expected.quantity).abs() < 1e-15);
                actual.quantity = expected.quantity;
                if let (Some(actual), Some(expected)) = (&mut actual.exit, &expected.exit) {
                    assert!((actual.profit - expected.profit).abs() < 1e-15);
                    if actual.profit == 0.0 {
                        assert!(actual.profit.is_sign_positive());
                    }
                    actual.profit = expected.profit;
                }
                assert_eq!(actual, expected);
            }
            assert_eq!(report.equity.len(), equity.len());
            let skipped = equity.iter().filter(|value| value.is_nan()).count();
            for ((time, actual), (expected_time, expected)) in report
                .equity
                .iter()
                .zip((-1..source.len() as i64).map(hour).zip(equity))
            {
                assert_eq!(*time, expected_time);
                assert!(*actual == expected || (actual.is_nan() && expected.is_nan()));
            }
            assert_eq!(report.performance().skipped_equity_points, skipped);
            assert_eq!(report.comparison(&source).skipped_bars, skipped);
            let metrics = report.performance();
            assert_eq!(
                (
                    metrics.closed_trades,
                    metrics.open_trades,
                    metrics.winning_trades,
                    metrics.losing_trades
                ),
                (closed, open, wins, 0)
            );
            assert!((metrics.net_profit - profit).abs() < 1e-15);
            assert_eq!(
                metrics.win_rate,
                (closed > 0).then(|| wins as f64 / closed as f64)
            );
            assert_eq!(metrics.profit_factor, None);
            assert!(!report.trades_csv().contains(",-0"));
        }

        fn sized(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, None, OrderType::Market),
                1 => context.exit("X", None, None, None),
                _ => {}
            }
        }
        fn override_size(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, 4.0, OrderType::Market);
            } else if bar == 1 {
                context.exit("X", None, None, None);
            }
        }
        fn repeat(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, None, OrderType::Market),
                1 => context.exit("X", None, None, None),
                2 => context.entry("B", Buy, None, OrderType::Market),
                _ => {}
            }
        }
        fn stack_in_time(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, None, OrderType::Market),
                1 => context.entry("B", Buy, None, OrderType::Market),
                _ => {}
            }
        }
        fn stack(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, None, OrderType::Market);
                context.entry("B", Buy, None, OrderType::Market);
            } else if bar == 1 {
                context.entry("C", Buy, None, OrderType::Market);
            }
        }
        fn hold(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, None, OrderType::Market);
            }
        }
        fn hold_short(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Sell, None, OrderType::Market);
            }
        }
        fn split(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, 1.0, OrderType::Market);
                context.entry("B", Buy, 3.0, OrderType::Market);
            } else if bar == 1 {
                context.exit("X", Some(2.0), None, None);
            }
        }
        fn reverse(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, 2.0, OrderType::Market),
                1 => context.entry("B", Sell, 3.0, OrderType::Market),
                2 => context.exit("X", None, None, None),
                _ => {}
            }
        }
        fn stops(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, None, OrderType::Stop(105.0));
                context.exit("X", None, None, Some(95.0));
            }
        }
        fn limits(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Buy, None, OrderType::Limit(100.0));
                context.exit("X", None, Some(110.0), None);
            }
        }
        fn recalculate(bar: usize, context: &mut StrategyContext<'_>) {
            if context.is_after_fill() {
                assert_eq!(
                    context.bars().len(),
                    1,
                    "current bar must not leak future prices"
                );
                if context.position() > 0.0 {
                    assert_eq!(context.price(), 100.0);
                    assert_eq!(context.equity(), 1000.0);
                    context.exit("X", None, Some(context.price() + 5.0), Some(98.0));
                } else {
                    assert_eq!(context.price(), 105.0);
                }
            } else if bar == 0 {
                context.entry("A", Buy, None, OrderType::Market);
            }
        }
        fn cycle(bar: usize, context: &mut StrategyContext<'_>) {
            if context.is_after_fill() {
                if context.position() > 0.0 {
                    context.exit("X", None, None, None);
                } else {
                    context.entry("A", Buy, None, OrderType::Market);
                }
            } else if bar == 0 {
                context.entry("A", Buy, None, OrderType::Market);
            }
        }
        fn sized_short(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry("A", Sell, None, OrderType::Market);
            } else if bar == 1 {
                context.exit("X", None, None, None);
            }
        }
        fn stop_limits(bar: usize, context: &mut StrategyContext<'_>) {
            if bar == 0 {
                context.entry(
                    "A",
                    Buy,
                    None,
                    OrderType::StopLimit {
                        stop: 105.0,
                        limit: 106.0,
                    },
                );
                context.exit("X", None, Some(110.0), None);
            }
        }
        fn boundary_stop(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, None, OrderType::Market),
                1 => context.exit("X", None, None, Some(100.0)),
                _ => {}
            }
        }
        let terms = BacktestSettings {
            initial_capital: 1000.0,
            currency: Some("USD".into()),
            risk_free_rate: 0.02,
            order_size: OrderSize::Fixed(2.0),
            ..Default::default()
        };
        let flat = |prices: &[f64]| prices.iter().map(|&p| (p, p, p, p)).collect::<Vec<_>>();
        let fee_trade = |side, quantity, entry, entry_price, exit, fees, label| {
            let mut result = trade(side, quantity, entry, entry_price, exit, label);
            result.fees = fees;
            result
        };
        // All amounts are in account units. For example, 2 units from 100 to 110 earn 20;
        // a 1% fee costs 2 + 2.2, and two ticks of 0.5 reduce that gross profit to 16.
        // At 2x leverage, 15 long from 100 have equity 15p-500 and margin 7.5p:
        // the call is at p=200/3, equity=500, even if the bar later recovers to 100.
        for (name, script, settings, prices, expected, equity, rejected) in [
            (
                "fixed size",
                sized as fn(usize, &mut StrategyContext<'_>),
                terms.clone(),
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 2.0, 1, 100.0, Some((2, 110.0, 20.0)), "A / X")],
                vec![1000.0, 1000.0, 1000.0, 1020.0],
                0,
            ),
            (
                "cash size",
                sized,
                BacktestSettings {
                    order_size: OrderSize::Cash(200.0),
                    ..terms.clone()
                },
                flat(&[100.0, 125.0, 130.0]),
                vec![trade(Buy, 1.6, 1, 125.0, Some((2, 130.0, 8.0)), "A / X")],
                vec![1000.0, 1000.0, 1000.0, 1008.0],
                0,
            ),
            (
                "equity size",
                sized,
                BacktestSettings {
                    order_size: OrderSize::EquityFraction(0.5),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 5.0, 1, 100.0, Some((2, 110.0, 50.0)), "A / X")],
                vec![1000.0, 1000.0, 1000.0, 1050.0],
                0,
            ),
            (
                "current equity size",
                repeat,
                BacktestSettings {
                    order_size: OrderSize::EquityFraction(0.5),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 120.0, 100.0, 110.0]),
                vec![
                    trade(Buy, 5.0, 1, 100.0, Some((2, 120.0, 100.0)), "A / X"),
                    trade(Buy, 5.5, 3, 100.0, None, "B"),
                ],
                vec![1000.0, 1000.0, 1000.0, 1100.0, 1100.0, 1155.0],
                0,
            ),
            // At the second fill equity is 1000+5*(120-100)=1100, so half buys 550/120.
            (
                "equity sizing marks existing lots",
                stack_in_time,
                BacktestSettings {
                    order_size: OrderSize::EquityFraction(0.5),
                    pyramiding: 2,
                    leverage: 2.0,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 120.0]),
                vec![
                    trade(Buy, 5.0, 1, 100.0, None, "A"),
                    trade(Buy, 55.0 / 12.0, 2, 120.0, None, "B"),
                ],
                vec![1000.0, 1000.0, 1000.0, 1100.0],
                0,
            ),
            (
                "cash cannot size at zero",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Cash(200.0),
                    ..terms.clone()
                },
                flat(&[100.0, 0.0, 0.0]),
                vec![],
                vec![1000.0; 4],
                1,
            ),
            (
                "quantity override",
                override_size,
                terms.clone(),
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 4.0, 1, 100.0, Some((2, 110.0, 40.0)), "A / X")],
                vec![1000.0, 1000.0, 1000.0, 1040.0],
                0,
            ),
            (
                "default pyramiding",
                stack,
                terms.clone(),
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 2.0, 1, 100.0, None, "A")],
                vec![1000.0, 1000.0, 1000.0, 1020.0],
                0,
            ),
            (
                "two entries",
                stack,
                BacktestSettings {
                    pyramiding: 2,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![
                    trade(Buy, 2.0, 1, 100.0, None, "A"),
                    trade(Buy, 2.0, 1, 100.0, None, "B"),
                ],
                vec![1000.0, 1000.0, 1000.0, 1040.0],
                0,
            ),
            (
                "disabled entries",
                stack,
                BacktestSettings {
                    pyramiding: 0,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![],
                vec![1000.0; 4],
                0,
            ),
            (
                "fraction commission",
                sized,
                BacktestSettings {
                    commission: Commission::Fraction(0.01),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![fee_trade(
                    Buy,
                    2.0,
                    1,
                    100.0,
                    Some((2, 110.0, 15.8)),
                    4.2,
                    "A / X",
                )],
                vec![1000.0, 1000.0, 998.0, 1015.8],
                0,
            ),
            (
                "unit commission",
                sized,
                BacktestSettings {
                    commission: Commission::PerUnit(0.5),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![fee_trade(
                    Buy,
                    2.0,
                    1,
                    100.0,
                    Some((2, 110.0, 18.0)),
                    2.0,
                    "A / X",
                )],
                vec![1000.0, 1000.0, 999.0, 1018.0],
                0,
            ),
            (
                "order commission",
                sized,
                BacktestSettings {
                    commission: Commission::PerOrder(2.0),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![fee_trade(
                    Buy,
                    2.0,
                    1,
                    100.0,
                    Some((2, 110.0, 16.0)),
                    4.0,
                    "A / X",
                )],
                vec![1000.0, 1000.0, 998.0, 1016.0],
                0,
            ),
            (
                "fees shared across FIFO lots",
                split,
                BacktestSettings {
                    commission: Commission::PerOrder(4.0),
                    pyramiding: 2,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![
                    fee_trade(Buy, 1.0, 1, 100.0, Some((2, 110.0, 4.0)), 6.0, "A / X"),
                    fee_trade(
                        Buy,
                        1.0,
                        1,
                        100.0,
                        Some((2, 110.0, 20.0 / 3.0)),
                        10.0 / 3.0,
                        "B / X",
                    ),
                    fee_trade(Buy, 2.0, 1, 100.0, None, 8.0 / 3.0, "B"),
                ],
                vec![1000.0, 1000.0, 992.0, 1028.0],
                0,
            ),
            (
                "reversal is one fill",
                reverse,
                BacktestSettings {
                    commission: Commission::PerOrder(4.0),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0, 100.0]),
                vec![
                    fee_trade(Buy, 2.0, 1, 100.0, Some((2, 110.0, 14.4)), 5.6, "A / B"),
                    fee_trade(Sell, 3.0, 2, 110.0, Some((3, 100.0, 23.6)), 6.4, "B / X"),
                ],
                vec![1000.0, 1000.0, 996.0, 1012.0, 1038.0],
                0,
            ),
            (
                "tick slippage",
                sized,
                BacktestSettings {
                    slippage: Slippage::Ticks {
                        ticks: 2,
                        tick_size: 0.5,
                    },
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 2.0, 1, 101.0, Some((2, 109.0, 16.0)), "A / X")],
                vec![1000.0, 1000.0, 998.0, 1016.0],
                0,
            ),
            (
                "fraction slippage",
                sized,
                BacktestSettings {
                    slippage: Slippage::Fraction(0.01),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 2.0, 1, 101.0, Some((2, 108.9, 15.8)), "A / X")],
                vec![1000.0, 1000.0, 998.0, 1015.8],
                0,
            ),
            (
                "stop slippage",
                stops,
                BacktestSettings {
                    slippage: Slippage::Ticks {
                        ticks: 2,
                        tick_size: 0.5,
                    },
                    ..terms.clone()
                },
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 110.0, 99.0, 108.0),
                    (108.0, 109.0, 90.0, 100.0),
                ],
                vec![trade(Buy, 2.0, 1, 106.0, Some((2, 94.0, -24.0)), "A / X")],
                vec![1000.0, 1000.0, 1004.0, 976.0],
                0,
            ),
            (
                "limits never slip",
                limits,
                BacktestSettings {
                    slippage: Slippage::Fraction(0.1),
                    ..terms.clone()
                },
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (101.0, 103.0, 99.0, 102.0),
                    (108.0, 111.0, 107.0, 110.0),
                ],
                vec![trade(Buy, 2.0, 1, 100.0, Some((2, 110.0, 20.0)), "A / X")],
                vec![1000.0, 1000.0, 1004.0, 1020.0],
                0,
            ),
            (
                "insufficient margin",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(11.0),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![],
                vec![1000.0; 4],
                1,
            ),
            (
                "commission enters buying power",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(10.0),
                    commission: Commission::PerOrder(1.0),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![],
                vec![1000.0; 4],
                1,
            ),
            (
                "leverage",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(15.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![trade(Buy, 15.0, 1, 100.0, None, "A")],
                vec![1000.0, 1000.0, 1000.0, 1150.0],
                0,
            ),
            (
                "stop at the margin boundary averts a call",
                boundary_stop,
                BacktestSettings {
                    order_size: OrderSize::Fixed(20.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 100.0, 100.0, 100.0),
                    (101.0, 102.0, 90.0, 100.0),
                ],
                vec![trade(Buy, 20.0, 1, 100.0, Some((2, 100.0, 0.0)), "A / X")],
                vec![1000.0; 4],
                0,
            ),
            (
                "an existing margin deficit wins a stop tie",
                boundary_stop,
                BacktestSettings {
                    order_size: OrderSize::Fixed(20.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 90.0]),
                vec![trade(
                    Buy,
                    20.0,
                    1,
                    100.0,
                    Some((2, 90.0, -200.0)),
                    "A / Margin call",
                )],
                vec![1000.0, 1000.0, 1000.0, 800.0],
                0,
            ),
            (
                "intrabar long margin call",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(15.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 105.0, 50.0, 100.0),
                ],
                vec![trade(
                    Buy,
                    15.0,
                    1,
                    100.0,
                    Some((2, 200.0 / 3.0, -500.0)),
                    "A / Margin call",
                )],
                vec![1000.0, 1000.0, 1000.0, 500.0],
                0,
            ),
            (
                "intrabar short margin call",
                hold_short,
                BacktestSettings {
                    order_size: OrderSize::Fixed(10.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 150.0, 95.0, 100.0),
                ],
                vec![trade(
                    Sell,
                    10.0,
                    1,
                    100.0,
                    Some((2, 400.0 / 3.0, -1000.0 / 3.0)),
                    "A / Margin call",
                )],
                vec![1000.0, 1000.0, 1000.0, 2000.0 / 3.0],
                0,
            ),
            (
                "gap margin call",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(15.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 50.0]),
                vec![trade(
                    Buy,
                    15.0,
                    1,
                    100.0,
                    Some((2, 50.0, -750.0)),
                    "A / Margin call",
                )],
                vec![1000.0, 1000.0, 1000.0, 250.0],
                0,
            ),
            // Rescaling prices by 1e-200 and units by 1e200 preserves every cash amount.
            (
                "margin crossings do not multiply tiny prices",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(5e200),
                    leverage: 2.0,
                    ..terms.clone()
                },
                vec![
                    (1e-198, 1e-198, 1e-198, 1e-198),
                    (1e-198, 1e-198, 1e-198, 1e-198),
                    (1e-198, 1.05e-198, -1e-198, 1e-198),
                ],
                vec![trade(
                    Buy,
                    5e200,
                    1,
                    1e-198,
                    Some((2, -200.0 / 3.0 * 1e-200, -2500.0 / 3.0)),
                    "A / Margin call",
                )],
                vec![1000.0, 1000.0, 1000.0, 500.0 / 3.0],
                0,
            ),
            (
                "negative price margin call",
                hold,
                BacktestSettings {
                    order_size: OrderSize::Fixed(5.0),
                    leverage: 2.0,
                    ..terms.clone()
                },
                vec![
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 100.0, 100.0, 100.0),
                    (100.0, 105.0, -100.0, 100.0),
                ],
                vec![trade(
                    Buy,
                    5.0,
                    1,
                    100.0,
                    Some((2, -200.0 / 3.0, -2500.0 / 3.0)),
                    "A / Margin call",
                )],
                vec![1000.0, 1000.0, 1000.0, 500.0 / 3.0],
                0,
            ),
            (
                "short slippage",
                sized_short,
                BacktestSettings {
                    slippage: Slippage::Ticks {
                        ticks: 2,
                        tick_size: 0.5,
                    },
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 90.0]),
                vec![trade(Sell, 2.0, 1, 99.0, Some((2, 91.0, 16.0)), "A / X")],
                vec![1000.0, 1000.0, 998.0, 1016.0],
                0,
            ),
            (
                "stop-limit never slips",
                stop_limits,
                BacktestSettings {
                    slippage: Slippage::Fraction(0.1),
                    ..terms.clone()
                },
                vec![(100.0, 100.0, 100.0, 100.0), (100.0, 110.0, 99.0, 108.0)],
                vec![trade(Buy, 2.0, 1, 105.0, Some((1, 110.0, 10.0)), "A / X")],
                vec![1000.0, 1000.0, 1010.0],
                0,
            ),
            (
                "tiny quantity retains order fees",
                sized,
                BacktestSettings {
                    order_size: OrderSize::Fixed(1e-300),
                    commission: Commission::PerOrder(1.0),
                    ..terms.clone()
                },
                flat(&[100.0, 100.0, 110.0]),
                vec![fee_trade(
                    Buy,
                    1e-300,
                    1,
                    100.0,
                    Some((2, 110.0, -2.0)),
                    2.0,
                    "A / X",
                )],
                vec![1000.0, 1000.0, 999.0, 998.0],
                0,
            ),
            (
                "recalculation waits for the next point",
                cycle,
                BacktestSettings {
                    recalculate_after_fills: true,
                    ..terms.clone()
                },
                vec![(100.0, 100.0, 100.0, 100.0), (100.0, 101.0, 98.0, 100.0)],
                vec![
                    trade(Buy, 2.0, 1, 100.0, Some((1, 101.0, 2.0)), "A / X"),
                    trade(Buy, 2.0, 1, 98.0, Some((1, 100.0, 4.0)), "A / X"),
                ],
                vec![1000.0, 1000.0, 1006.0],
                0,
            ),
            (
                "flat bars retain successive fill points",
                cycle,
                BacktestSettings {
                    recalculate_after_fills: true,
                    ..terms.clone()
                },
                flat(&[100.0, 100.0]),
                vec![
                    trade(Buy, 2.0, 1, 100.0, Some((1, 100.0, 0.0)), "A / X"),
                    trade(Buy, 2.0, 1, 100.0, Some((1, 100.0, 0.0)), "A / X"),
                ],
                vec![1000.0; 3],
                0,
            ),
            (
                "recalculate after fills",
                recalculate,
                BacktestSettings {
                    recalculate_after_fills: true,
                    ..terms.clone()
                },
                vec![(100.0, 100.0, 100.0, 100.0), (100.0, 110.0, 99.0, 108.0)],
                vec![trade(Buy, 2.0, 1, 100.0, Some((1, 105.0, 10.0)), "A / X")],
                vec![1000.0, 1000.0, 1010.0],
                0,
            ),
        ] {
            let source: Vec<_> = prices
                .into_iter()
                .enumerate()
                .map(|(i, (o, h, l, c))| Bar::new(hour(i as i64), o, h, l, c, 0.0))
                .collect();
            let mut engine =
                Backtest::new("Terms", "TEST", Timeframe::Hour1, settings.initial_capital);
            engine.settings = settings;
            let report = engine.run(&mut Script(script), &source, &[]);
            assert_eq!(report.currency, Some("USD".into()), "{name}");
            assert_eq!(report.risk_free_rate, 0.02, "{name}");
            assert!(
                report
                    .properties
                    .contains(&("Rejected orders".into(), rejected.to_string())),
                "{name}"
            );
            assert_eq!(report.trades.len(), expected.len(), "{name}");
            for (mut actual, expected) in report.trades.iter().cloned().zip(expected) {
                assert!(
                    (actual.quantity - expected.quantity).abs() < 1e-10,
                    "{name}"
                );
                assert!((actual.fees - expected.fees).abs() < 1e-10, "{name}");
                actual.quantity = expected.quantity;
                actual.fees = expected.fees;
                if let (Some(a), Some(e)) = (&mut actual.exit, &expected.exit) {
                    assert!((a.price - e.price).abs() < 1e-10, "{name}");
                    assert!((a.profit - e.profit).abs() < 1e-10, "{name}");
                    a.price = e.price;
                    a.profit = e.profit;
                }
                assert_eq!(actual, expected, "{name}");
            }
            assert_eq!(report.equity.len(), equity.len(), "{name}");
            for ((time, actual), (expected_time, expected)) in report
                .equity
                .iter()
                .zip((-1..source.len() as i64).map(hour).zip(equity))
            {
                assert_eq!(*time, expected_time, "{name}");
                assert!(
                    (actual - expected).abs() < 1e-10,
                    "{name}: {actual} != {expected}"
                );
            }
        }

        struct History {
            prepared: usize,
            averages: Vec<f64>,
            decisions: Vec<usize>,
            entry_bar: usize,
        }
        impl Strategy for History {
            fn inputs(&self) -> Vec<(String, String)> {
                vec![("Length".into(), "2".into())]
            }
            fn prepare(&mut self, bars: &[Bar]) {
                self.prepared = bars.len();
                self.averages = bars
                    .windows(2)
                    .map(|pair| (pair[0].close + pair[1].close) / 2.0)
                    .collect();
            }
            fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
                let index = context.bars().len() - 1;
                self.decisions.push(index);
                if index <= self.entry_bar {
                    if index == 1 {
                        assert_eq!(self.averages[0], 104.0, "warm-up bar feeds the study");
                    }
                    context.entry("A", Buy, 2.0, OrderType::Market);
                    context.exit("X", None, Some(110.0), Some(95.0));
                }
            }
        }
        let source = vec![
            Bar::new(hour(0), 100.0, 100.0, 100.0, 100.0, 0.0),
            Bar::new(hour(1), 100.0, 111.0, 94.0, 108.0, 0.0),
            Bar::new(hour(2), 108.0, 108.0, 108.0, 108.0, 0.0),
        ];
        let first = Bar::new(hour(1), 100.0, 111.0, 99.0, 110.0, 0.0);
        let second = Bar::new(
            hour(1) + TimeDelta::minutes(30),
            110.0,
            110.0,
            94.0,
            108.0,
            0.0,
        );
        let mut later_target = trade(Buy, 2.0, 1, 100.0, Some((1, 110.0, 20.0)), "A / X");
        later_target.exit.as_mut().unwrap().time += TimeDelta::minutes(30);
        // Chart path: 100 -> 94 -> 111 -> 108 stops at 95, losing 2*(100-95)=10.
        // Finer path: 100 -> 99 -> 111 reaches the 110 target first, earning 2*10=20.
        // A start at 01:00 warms on 00:00, decides at 01:00, and buys at 02:00 for 108.
        for (name, lower, start, entry_bar, expected, times, decisions) in [
            (
                "chart path",
                vec![],
                None,
                0,
                vec![trade(Buy, 2.0, 1, 100.0, Some((1, 95.0, -10.0)), "A / X")],
                vec![hour(-1), hour(0), hour(1), hour(2)],
                vec![0, 1, 2],
            ),
            (
                "finer path",
                vec![first.clone(), second.clone()],
                None,
                0,
                vec![trade(Buy, 2.0, 1, 100.0, Some((1, 110.0, 20.0)), "A / X")],
                vec![hour(-1), hour(0), hour(1), hour(2)],
                vec![0, 1, 2],
            ),
            (
                "only fills use finer timestamps",
                vec![
                    Bar::new(hour(1), 100.0, 101.0, 99.0, 100.0, 0.0),
                    Bar::new(
                        hour(1) + TimeDelta::minutes(30),
                        100.0,
                        111.0,
                        99.0,
                        108.0,
                        0.0,
                    ),
                ],
                None,
                0,
                vec![later_target],
                vec![hour(-1), hour(0), hour(1), hour(2)],
                vec![0, 1, 2],
            ),
            (
                "finer bars are ordered",
                vec![second.clone(), first.clone()],
                None,
                0,
                vec![trade(Buy, 2.0, 1, 100.0, Some((1, 110.0, 20.0)), "A / X")],
                vec![hour(-1), hour(0), hour(1), hour(2)],
                vec![0, 1, 2],
            ),
            (
                "uncovered bar falls back",
                vec![Bar::new(hour(2), 108.0, 108.0, 108.0, 108.0, 0.0)],
                None,
                0,
                vec![trade(Buy, 2.0, 1, 100.0, Some((1, 95.0, -10.0)), "A / X")],
                vec![hour(-1), hour(0), hour(1), hour(2)],
                vec![0, 1, 2],
            ),
            (
                "invalid finer bar falls back",
                vec![Bar::new(hour(1), 100.0, f64::NAN, 99.0, 100.0, 0.0)],
                None,
                0,
                vec![trade(Buy, 2.0, 1, 100.0, Some((1, 95.0, -10.0)), "A / X")],
                vec![hour(-1), hour(0), hour(1), hour(2)],
                vec![0, 1, 2],
            ),
            (
                "start warms studies",
                vec![],
                Some(hour(1)),
                1,
                vec![trade(Buy, 2.0, 2, 108.0, None, "A")],
                vec![hour(1), hour(2), hour(3)],
                vec![0, 1, 2],
            ),
            (
                "start between chart bars",
                vec![first.clone(), second.clone()],
                Some(hour(0) + TimeDelta::minutes(30)),
                1,
                vec![trade(Buy, 2.0, 2, 108.0, None, "A")],
                vec![hour(0) + TimeDelta::minutes(30), hour(2), hour(3)],
                vec![0, 1, 2],
            ),
            (
                "start after history",
                vec![first, second],
                Some(hour(3)),
                3,
                vec![],
                vec![],
                vec![0, 1, 2],
            ),
        ] {
            let mut strategy = History {
                prepared: 0,
                averages: vec![],
                decisions: vec![],
                entry_bar,
            };
            let mut engine = Backtest::new("History", "TEST", Timeframe::Hour1, 1000.0);
            engine.settings.start_time = start;
            let report = engine.run(&mut strategy, &source, &lower);
            assert_eq!(strategy.prepared, source.len(), "{name}");
            assert_eq!(strategy.decisions, decisions, "{name}");
            assert_eq!(report.trades, expected, "{name}");
            assert_eq!(
                report
                    .equity
                    .iter()
                    .map(|(time, _)| *time)
                    .collect::<Vec<_>>(),
                times,
                "{name}"
            );
            let expected_last = expected
                .first()
                .and_then(|trade| trade.exit.as_ref())
                .map_or(1000.0, |exit| 1000.0 + exit.profit);
            if let Some((_, last)) = report.equity.last() {
                assert_eq!(*last, expected_last, "{name}");
            }
            let csv = report.summary_csv(&report.performance(), &report.comparison(&source));
            for setting in ["Initial capital", "Currency", "Risk-free rate"] {
                assert_eq!(
                    csv.lines()
                        .filter(|row| row.starts_with(&format!("properties,{setting},")))
                        .count(),
                    1,
                    "{name}: {setting} must appear once"
                );
            }
            assert!(
                report
                    .properties
                    .contains(&("Input: Length".into(), "2".into()))
            );
            for setting in [
                "Order size",
                "Pyramiding",
                "Commission",
                "Slippage",
                "Leverage",
                "Fill on close",
                "Recalculate after fills",
                "Start time",
                "Lower-interval bars",
                "Rejected orders",
            ] {
                assert!(
                    report.properties.iter().any(|(key, _)| key == setting),
                    "{name}: missing {setting}"
                );
            }
            if let Some(start) = start {
                assert!(report.trades.iter().all(|trade| trade.entry_time >= start));
                assert!(report.equity.iter().all(|(time, _)| *time >= start));
                assert_eq!(report.equity_curve(), report.equity, "{name}");
            }
        }

        fn stop_then_close(bar: usize, context: &mut StrategyContext<'_>) {
            match bar {
                0 => context.entry("A", Buy, 2.0, OrderType::Stop(105.0)),
                1 => context.exit("X", None, None, None),
                _ => {}
            }
        }
        // An on-close exit follows a finer intrabar entry in real time, rather than
        // inheriting the chart open's timestamp and becoming an invalid backwards trade.
        for (lower, entry_time, exit_time) in [
            (vec![], hour(1), hour(1)),
            (
                vec![
                    Bar::new(hour(1), 100.0, 101.0, 99.0, 100.0, 0.0),
                    Bar::new(
                        hour(1) + TimeDelta::minutes(30),
                        100.0,
                        111.0,
                        99.0,
                        108.0,
                        0.0,
                    ),
                ],
                hour(1) + TimeDelta::minutes(30),
                hour(2),
            ),
        ] {
            let mut engine = Backtest::new("On close", "TEST", Timeframe::Hour1, 1000.0);
            engine.settings.fill_on_close = true;
            let report = engine.run(&mut Script(stop_then_close), &source, &lower);
            assert_eq!(report.performance().closed_trades, 1);
            assert_eq!(report.trades[0].entry_time, entry_time);
            let exit = report.trades[0].exit.as_ref().unwrap();
            assert_eq!(exit.time, exit_time);
            assert_eq!(exit.profit, 6.0);
        }

        // The last execution bar has no next open. Its provisional mark is one clock tick
        // after its last source time, so the starting capital and every close stay ordered.
        for (count, lower, times) in [
            (
                1,
                vec![],
                vec![hour(0), hour(0) + TimeDelta::nanoseconds(1)],
            ),
            (
                2,
                vec![Bar::new(
                    hour(1) + TimeDelta::minutes(30),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    0.0,
                )],
                vec![
                    hour(0),
                    hour(1),
                    hour(1) + TimeDelta::minutes(30) + TimeDelta::nanoseconds(1),
                ],
            ),
        ] {
            let source: Vec<_> = (0..count)
                .map(|i| Bar::new(hour(i), 100.0, 100.0, 100.0, 100.0, 0.0))
                .collect();
            let mut engine = Backtest::new(
                "Execution",
                "TEST",
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
                1000.0,
            );
            engine.settings = BacktestSettings {
                start_time: Some(hour(0)),
                fill_on_close: true,
                commission: Commission::PerOrder(1.0),
                ..terms.clone()
            };
            let report = engine.run(&mut Script(hold), &source, &lower);
            assert_eq!(
                report
                    .equity
                    .iter()
                    .map(|(time, _)| *time)
                    .collect::<Vec<_>>(),
                times
            );
            assert_eq!(report.equity_curve(), report.equity);
            assert_eq!(report.performance().max_drawdown, Some(1.0));
            assert_eq!(report.equity.last().unwrap().1, 999.0);
            assert_eq!(report.trades[0].fees, 1.0);
        }
    }
}
