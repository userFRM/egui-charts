//! Closed-bar price rules. Channels exclude the signal bar for close breakouts; stop
//! channels include it because their entries first become eligible on the following bar.
//! Pivots prefer the newest equal extreme and become usable only at confirmation.

use chrono::{DateTime, Utc};

use super::{StrategyFactory, length, length_input, number, number_input, set_input};
use crate::model::{Bar, OrderSide, OrderType};
use crate::strategies::backtest::valid_bar;
use crate::strategies::{OrderSize, Strategy, StrategyContext};
use crate::studies::{InputValue, StudyInput, helpers};

#[derive(Clone, Copy)]
enum Rule {
    BarUpDown,
    Consecutive,
    Gap,
    Inside,
    Outside,
    Channel,
    PriceChannel,
    PivotReversal,
    PivotExtension,
    Volatility,
}

pub(super) fn register(factory: &mut StrategyFactory) {
    for (name, rule) in [
        ("Bar Up/Down", Rule::BarUpDown),
        ("Consecutive Up/Down", Rule::Consecutive),
        ("Gap Continuation", Rule::Gap),
        ("Inside Bar", Rule::Inside),
        ("Outside Bar", Rule::Outside),
        ("Channel Breakout", Rule::Channel),
        ("Price Channel Break", Rule::PriceChannel),
        ("Pivot Reversal", Rule::PivotReversal),
        ("Pivot Extension", Rule::PivotExtension),
        ("Volatility Expansion Close", Rule::Volatility),
    ] {
        factory.register(name, move || Box::new(PriceAction::new(rule)));
    }
}

struct PriceAction {
    rule: Rule,
    inputs: Vec<StudyInput>,
    atr: Vec<f64>,
    pivot_high: Option<f64>,
    pivot_low: Option<f64>,
    day: Option<Day>,
}

/// The trading day of the latest decision, for the Max Intraday Loss input.
#[derive(Clone, Copy)]
struct Day {
    /// Open of the run's daily bar holding the decision bar, see
    /// [`StrategyContext::trading_day`].
    date: DateTime<Utc>,
    /// Equity at the last decision before this day, or at the run's first decision.
    start: f64,
    /// Highest equity at the day's start and at its decisions.
    peak: f64,
    /// Equity at the latest decision with a finite value.
    equity: f64,
    /// Whether the day's loss went past the limit.
    halted: bool,
}

impl PriceAction {
    fn new(rule: Rule) -> Self {
        let mut inputs = match rule {
            Rule::BarUpDown => vec![number_input(
                "max_intraday_loss",
                "Max Intraday Loss (%)",
                1.0,
                0.0,
                f64::MAX,
            )],
            Rule::Consecutive => vec![length_input("length", "Consecutive closes", 3)],
            Rule::Channel | Rule::PriceChannel => {
                vec![length_input("length", "Channel length", 20)]
            }
            Rule::PivotReversal | Rule::PivotExtension => vec![
                length_input("left", "Pivot left bars", 4),
                length_input("right", "Pivot confirmation bars", 2),
            ],
            Rule::Volatility => vec![
                length_input("length", "ATR length", 14),
                number_input("multiplier", "ATR multiplier", 2.0, 0.0, f64::MAX),
            ],
            _ => Vec::new(),
        };
        if matches!(rule, Rule::Gap | Rule::Inside | Rule::Outside) {
            inputs.extend([
                length_input("profit_ticks", "Take profit ticks", 10),
                length_input("loss_ticks", "Stop loss ticks", 10),
                number_input("tick_size", "Tick size", 0.01, f64::MIN_POSITIVE, f64::MAX),
            ]);
        }
        Self {
            rule,
            inputs,
            atr: Vec::new(),
            pivot_high: None,
            pivot_low: None,
            day: None,
        }
    }

    /// Applies Max Intraday Loss at a decision. Once the loss since the start of the trading
    /// day exceeds that share of the day's highest equity, cancels the entries, closes the
    /// position and returns `true` until the trading day changes.
    fn day_halted(&mut self, context: &mut StrategyContext<'_>) -> bool {
        let Some(date) = context.trading_day() else {
            return false;
        };
        let equity = context.equity();
        if equity.is_finite() {
            let mut day = match self.day {
                Some(day) if day.date == date => day,
                previous => {
                    let start = previous.map_or(equity, |day| day.equity);
                    Day {
                        date,
                        start,
                        peak: start,
                        equity,
                        halted: false,
                    }
                }
            };
            day.peak = day.peak.max(equity);
            day.equity = equity;
            day.halted |=
                day.start - equity > number(&self.inputs, "max_intraday_loss") / 100.0 * day.peak;
            self.day = Some(day);
        }
        let halted = self.day.is_some_and(|day| day.date == date && day.halted);
        if halted {
            context.cancel("Long");
            context.cancel("Short");
            if context.position() != 0.0 {
                context.exit("Close", None, None, None);
            }
        }
        halted
    }

    fn entry(&self, context: &mut StrategyContext<'_>, side: OrderSide, order: OrderType) {
        context.entry(
            if side == OrderSide::Buy {
                "Long"
            } else {
                "Short"
            },
            side,
            None,
            order,
        );
        if matches!(self.rule, Rule::Gap | Rule::Inside | Rule::Outside) {
            let tick = number(&self.inputs, "tick_size");
            context.exit_relative(
                if side == OrderSide::Buy {
                    "Long bracket"
                } else {
                    "Short bracket"
                },
                side,
                length(&self.inputs, "profit_ticks") as f64 * tick,
                length(&self.inputs, "loss_ticks") as f64 * tick,
            );
        }
    }
}

impl Strategy for PriceAction {
    fn study_inputs(&self) -> Vec<StudyInput> {
        self.inputs.clone()
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if matches!(self.rule, Rule::Gap | Rule::Inside | Rule::Outside) {
            let mut inputs = self.inputs.clone();
            set_input(&mut inputs, id, value.clone())?;
            let tick = number(&inputs, "tick_size");
            if ["profit_ticks", "loss_ticks"]
                .iter()
                .any(|id| !(length(&inputs, id) as f64 * tick).is_finite())
            {
                return Err("tick distance must be finite");
            }
        }
        set_input(&mut self.inputs, id, value)?;
        self.atr.clear();
        self.pivot_high = None;
        self.pivot_low = None;
        Ok(())
    }

    fn default_order_size(&self) -> Option<OrderSize> {
        matches!(self.rule, Rule::BarUpDown).then_some(OrderSize::EquityFraction(0.1))
    }

    fn prepare(&mut self, bars: &[Bar]) {
        self.pivot_high = None;
        self.pivot_low = None;
        self.day = None;
        self.prepare_update(bars);
    }

    fn prepare_update(&mut self, bars: &[Bar]) {
        self.atr.clear();
        if matches!(self.rule, Rule::Volatility) {
            let mut previous: Option<f64> = None;
            let ranges: Vec<_> = bars
                .iter()
                .map(|bar| {
                    if !valid_bar(bar) {
                        return f64::NAN;
                    }
                    let range = previous.map_or(bar.high - bar.low, |close| {
                        (bar.high - bar.low)
                            .max((bar.high - close).abs())
                            .max((bar.low - close).abs())
                    });
                    previous = Some(bar.close);
                    range
                })
                .collect();
            self.atr = helpers::rma(&ranges, length(&self.inputs, "length"));
        }
    }

    fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
        if context.is_after_fill() {
            return;
        }
        if matches!(self.rule, Rule::BarUpDown) && self.day_halted(context) {
            return;
        }
        let bars = context.bars();
        let Some(bar) = bars.last() else {
            return;
        };
        if !valid_bar(bar) {
            return;
        }
        let n = bars.len();
        let entries = match self.rule {
            Rule::BarUpDown | Rule::Gap | Rule::Inside | Rule::Outside => {
                let [.., previous, bar] = bars else {
                    return;
                };
                if !valid_bar(previous) {
                    return;
                }
                let (up, down) = match self.rule {
                    Rule::BarUpDown => (
                        bar.close > bar.open && bar.open > previous.close,
                        bar.close < bar.open && bar.open < previous.close,
                    ),
                    // Opening gaps are measured from the previous close.
                    Rule::Gap => (bar.open > previous.close, bar.open < previous.close),
                    _ => {
                        let pattern = if matches!(self.rule, Rule::Inside) {
                            bar.high < previous.high && bar.low > previous.low
                        } else {
                            bar.high > previous.high && bar.low < previous.low
                        };
                        (
                            pattern && bar.close > bar.open,
                            pattern && bar.close < bar.open,
                        )
                    }
                };
                [
                    up.then_some(OrderType::Market),
                    down.then_some(OrderType::Market),
                ]
            }
            Rule::Consecutive => {
                let count = length(&self.inputs, "length");
                if n <= count {
                    return;
                }
                let window = &bars[n - count - 1..];
                if !window.iter().all(valid_bar) {
                    return;
                }
                [
                    window
                        .windows(2)
                        .all(|b| b[1].close > b[0].close)
                        .then_some(OrderType::Market),
                    window
                        .windows(2)
                        .all(|b| b[1].close < b[0].close)
                        .then_some(OrderType::Market),
                ]
            }
            Rule::Channel | Rule::PriceChannel => {
                let count = length(&self.inputs, "length");
                let end = if matches!(self.rule, Rule::Channel) {
                    n
                } else {
                    n - 1
                };
                if end < count {
                    return;
                }
                let window = &bars[end - count..end];
                if !window.iter().all(valid_bar) {
                    return;
                }
                let high = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                if matches!(self.rule, Rule::Channel) {
                    [Some(OrderType::Stop(high)), Some(OrderType::Stop(low))]
                } else {
                    [
                        (bar.close > high).then_some(OrderType::Market),
                        (bar.close < low).then_some(OrderType::Market),
                    ]
                }
            }
            Rule::PivotReversal | Rule::PivotExtension => {
                let left = length(&self.inputs, "left");
                let right = length(&self.inputs, "right");
                if n <= right || n - right <= left {
                    return;
                }
                let pivot = n - right - 1;
                let before = &bars[pivot - left..pivot];
                let after = &bars[pivot + 1..];
                if !bars[pivot - left..].iter().all(valid_bar) {
                    return;
                }
                let high = bars[pivot].high;
                let low = bars[pivot].low;
                let high_confirmed =
                    before.iter().all(|b| b.high <= high) && after.iter().all(|b| b.high < high);
                let low_confirmed =
                    before.iter().all(|b| b.low >= low) && after.iter().all(|b| b.low > low);
                if high_confirmed {
                    self.pivot_high = Some(high);
                }
                if low_confirmed {
                    self.pivot_low = Some(low);
                }
                if matches!(self.rule, Rule::PivotReversal) {
                    [
                        self.pivot_high.map(OrderType::Stop),
                        self.pivot_low.map(OrderType::Stop),
                    ]
                } else {
                    [
                        low_confirmed.then_some(OrderType::Market),
                        high_confirmed.then_some(OrderType::Market),
                    ]
                }
            }
            Rule::Volatility => {
                let Some(&atr) = self.atr.get(n - 1).filter(|atr| atr.is_finite()) else {
                    return;
                };
                let distance = atr * number(&self.inputs, "multiplier");
                [
                    Some(OrderType::Stop(bar.close + distance)),
                    Some(OrderType::Stop(bar.close - distance)),
                ]
            }
        };
        for (side, entry) in [OrderSide::Buy, OrderSide::Sell].into_iter().zip(entries) {
            if let Some(order) = entry {
                self.entry(context, side, order);
            }
        }
    }
}
