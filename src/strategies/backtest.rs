//! The strategy trait, the context it trades through and the engine that runs it over bars.

use chrono::{DateTime, TimeDelta, Utc};

use crate::model::rounding_tolerance;
use crate::model::{
    Bar, OrderSide, OrderType, StrategyExit, StrategyReport, StrategyTrade, Timeframe,
};

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
/// let report = Backtest::new("Momentum", "AAPL", Timeframe::Min1, 10_000.0).run(&mut Momentum, &bars);
/// // Bought at the next open after the rise to 101, sold at the next open after the fall to 102.
/// assert_eq!(report.trades.len(), 1);
/// assert_eq!(report.trades[0].entry_price, 103.0);
/// assert_eq!(report.trades[0].exit.as_ref().map(|exit| exit.profit), Some(-20.0));
/// assert_eq!(report.equity.len(), bars.len() + 1);
/// ```
pub trait Strategy {
    /// Computes what the strategy needs from the whole run once, before the first bar, such as
    /// its studies. A value at a bar must depend only on that bar and earlier ones, as a study's
    /// does, or the run looks ahead. Does nothing by default.
    fn prepare(&mut self, bars: &[Bar]) {
        let _ = bars;
    }

    /// Decides on the bar that just closed, the last of [`StrategyContext::bars`], by placing and
    /// cancelling orders.
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
}

impl StrategyContext<'_> {
    /// The bars of the run up to and including the one that just closed, oldest first.
    pub fn bars(&self) -> &[Bar] {
        self.bars
    }

    /// The open position: positive when long, negative when short, 0 when flat.
    pub fn position(&self) -> f64 {
        self.position
    }

    /// Places an entry of `quantity` on `side`. Against an open position on the other side it
    /// closes that position first, then opens `quantity` on `side`; on the same side it adds.
    pub fn entry(
        &mut self,
        id: impl Into<String>,
        side: OrderSide,
        quantity: f64,
        order_type: OrderType,
    ) {
        let finite = match order_type {
            OrderType::Market => true,
            OrderType::Limit(price) | OrderType::Stop(price) => price.is_finite(),
            OrderType::StopLimit { stop, limit } => stop.is_finite() && limit.is_finite(),
        };
        if finite {
            self.place(id.into(), Some(quantity), Kind::Entry(side, order_type));
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
        if quantity.is_some_and(|quantity| !quantity.is_finite() || quantity <= 0.0) {
            return;
        }
        let order = Working { id, quantity, kind };
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
    /// `None` for an exit of the whole position; always set for an entry.
    quantity: Option<f64>,
    kind: Kind,
}

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
/// the path reaches first. With [`fill_on_close`](Self::fill_on_close), market orders fill at the
/// close of the bar they were placed on instead.
///
/// Each entry is a trade, closed by exits and opposite entries first in first out; each partial
/// exit is its own trade with its share of the entry. A trade's label names its entry and exit
/// orders, such as "Long / Take profit", or the entry alone while open. Profit is the price
/// difference times the quantity, without fees. The equity curve holds the initial capital one
/// interval before the first bar, then the account value at each bar's close, stamped with the
/// bar's time. Bar times are used for every fill and equity point.
///
/// Bars are oldest first, in strictly increasing time order. A bar without finite prices, or
/// whose high and low do not contain its open and close, fills no orders and has no equity value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Backtest {
    /// What the report calls the run.
    pub name: String,
    /// The instrument the bars are of.
    pub symbol: String,
    /// The bar interval.
    pub timeframe: Timeframe,
    /// Account value at the start.
    pub initial_capital: f64,
    /// Fill market orders at the close of the bar they are placed on instead of the next open.
    pub fill_on_close: bool,
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
            initial_capital,
            fill_on_close: false,
        }
    }

    /// Runs `strategy` over `bars` and reports its trades and equity.
    pub fn run(&self, strategy: &mut dyn Strategy, bars: &[Bar]) -> StrategyReport {
        let mut report = StrategyReport::new(
            &self.name,
            &self.symbol,
            self.timeframe,
            self.initial_capital,
        );
        let mut run = Run {
            orders: Vec::new(),
            lots: Vec::new(),
            trades: Vec::new(),
            realized: 0.0,
        };
        strategy.prepare(bars);
        if let Some(first) = bars.first() {
            let interval = self.timeframe.duration().max(TimeDelta::milliseconds(1));
            let before = first.time.checked_sub_signed(interval);
            report.equity.push((
                before.unwrap_or(DateTime::<Utc>::MIN_UTC),
                self.initial_capital,
            ));
        }
        for (index, bar) in bars.iter().enumerate() {
            let valid = [bar.open, bar.high, bar.low, bar.close]
                .iter()
                .all(|price| price.is_finite())
                && bar.low <= bar.open.min(bar.close)
                && bar.high >= bar.open.max(bar.close);
            if valid {
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
                    run.fill_along(start, end, bar.time);
                }
            }
            let position = run.position();
            strategy.on_bar(&mut StrategyContext {
                bars: &bars[..=index],
                position,
                orders: &mut run.orders,
            });
            if valid && self.fill_on_close {
                run.fill_markets(bar.close, bar.time);
            }
            let open: f64 = run
                .lots
                .iter()
                .map(|lot| sign(lot.side) * (bar.close - lot.price) * lot.quantity)
                .sum();
            let equity = self.initial_capital + run.realized + open;
            report
                .equity
                .push((bar.time, if valid { equity } else { f64::NAN }));
        }
        report.trades = run.trades;
        report.trades.extend(run.lots.into_iter().map(|lot| {
            let mut trade = StrategyTrade::new(lot.side, lot.quantity, lot.time, lot.price);
            trade.label = Some(lot.entry);
            trade
        }));
        report
    }
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

struct Run {
    orders: Vec<Working>,
    /// Open entries, oldest first, all on one side.
    lots: Vec<Lot>,
    trades: Vec<StrategyTrade>,
    realized: f64,
}

impl Run {
    fn position(&self) -> f64 {
        self.lots.iter().fold(0.0, |position, lot| {
            position + sign(lot.side) * lot.quantity
        })
    }

    /// Where `order` fills while the price moves from `start` to `end`, if it does.
    fn fill_price(&self, order: &Working, start: f64, end: f64) -> Option<f64> {
        match order.kind {
            Kind::Entry(side, order_type) => order_type.fill_price(side, start, end),
            Kind::Exit(take_profit, stop_loss, side) => {
                let side = side?;
                if take_profit.is_none() && stop_loss.is_none() {
                    return Some(start);
                }
                // The stop first, so it wins a tie.
                [
                    stop_loss.map(OrderType::Stop),
                    take_profit.map(OrderType::Limit),
                ]
                .into_iter()
                .flatten()
                .filter_map(|order_type| order_type.fill_price(side, start, end))
                .min_by(|a, b| (a - start).abs().total_cmp(&(b - start).abs()))
            }
        }
    }

    /// Fills the working orders the price reaches while it moves from `start` to `end`, nearest
    /// first, each fill changing the position the next one sees.
    fn fill_along(&mut self, mut start: f64, end: f64, time: DateTime<Utc>) {
        loop {
            let mut next: Option<(usize, f64)> = None;
            for (index, order) in self.orders.iter().enumerate() {
                if let Some(price) = self.fill_price(order, start, end)
                    && next.is_none_or(|(_, best)| (price - start).abs() < (best - start).abs())
                {
                    next = Some((index, price));
                }
            }
            let Some((index, price)) = next else {
                return;
            };
            start = price;
            if let Kind::Entry(side, OrderType::StopLimit { limit, .. }) = self.orders[index].kind {
                // Triggered: it works on as a limit order from its stop.
                self.orders[index].kind = Kind::Entry(side, OrderType::Limit(limit));
                continue;
            }
            let order = self.orders.remove(index);
            self.fill(order, price, time);
        }
    }

    /// Fills the market orders, in the order they were first placed, at `price`.
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
                self.fill(order, price, time);
                index = 0;
            } else {
                index += 1;
            }
        }
    }

    fn fill(&mut self, order: Working, price: f64, time: DateTime<Utc>) {
        match order.kind {
            Kind::Entry(side, _) => {
                if self.lots.first().is_some_and(|lot| lot.side != side) {
                    self.close(f64::INFINITY, &order.id, price, time);
                }
                let quantity = order.quantity.unwrap_or_default();
                self.lots.push(Lot {
                    entry: order.id,
                    side,
                    quantity,
                    entry_quantity: quantity,
                    time,
                    price,
                });
            }
            Kind::Exit(..) => {
                self.close(
                    order.quantity.unwrap_or(f64::INFINITY),
                    &order.id,
                    price,
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
    }

    /// Closes up to `quantity` of the open lots, oldest first, as trades exited by `exit`.
    fn close(&mut self, mut quantity: f64, exit: &str, price: f64, time: DateTime<Utc>) {
        let exit_quantity = quantity;
        while quantity > 0.0
            && let Some(lot) = self.lots.first_mut()
        {
            let closed = quantity.min(lot.quantity);
            let profit = sign(lot.side) * (price - lot.price) * closed + 0.0;
            let mut trade = StrategyTrade::new(lot.side, closed, lot.time, lot.price);
            trade.exit = Some(StrategyExit::new(time, price, profit));
            trade.label = Some(format!("{} / {exit}", lot.entry));
            self.trades.push(trade);
            self.realized += profit;
            quantity -= closed;
            lot.quantity -= closed;
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
        let backtest = Backtest::new("Script", "TEST", Timeframe::Hour1, 10_000.0);
        let report = backtest.run(&mut Script(script), &bars);

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
        let first = backtest.run(&mut Script(script), &bars[..4]);
        assert_eq!(first.equity, report.equity[..5]);
        assert_eq!(first.trades[0], report.trades[0]);
        assert_eq!(first.trades[1], trade(Sell, 5.0, 3, 102.5, None, "Short"));

        // Placing the long's exit first only changes which of the two closes the long at 103.
        let stop_first = backtest.run(&mut Script(stop_first), &bars);
        assert_eq!(stop_first.equity, report.equity);
        let mut trades = report.trades.clone();
        trades[8].label = Some("Long4 / Stop".to_owned());
        assert_eq!(stop_first.trades, trades);

        // On close, the first market entry fills at the close of the bar it was placed on.
        let on_close = Backtest {
            fill_on_close: true,
            ..backtest.clone()
        }
        .run(&mut Script(script), &bars);
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
                    context.entry("A", Buy, 3.2, OrderType::Market);
                    context.entry("B", Buy, 3.2, OrderType::Market);
                }
                1..=128 => context.exit("X", Some(0.05), None, None),
                129 => assert_eq!(context.position(), 0.0),
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
                // 2 * 3.2 - 128 * 0.05 = 0: 128 breakevens, flat equity and no open lot.
                pyramided_scale_out,
                vec![(100.0, 100.0, 100.0, 100.0); 130],
                (2..=129)
                    .map(|exit| {
                        let label = if exit <= 65 { "A / X" } else { "B / X" };
                        trade(Buy, 0.05, 1, 100.0, Some((exit, 100.0, 0.0)), label)
                    })
                    .collect(),
                vec![10_000.0; 131],
                128,
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
            let report = backtest.run(&mut Script(script), &source);
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
    }
}
