//! Closed-bar price rules. Channels exclude the signal bar for close breakouts; stop
//! channels include it because their entries first become eligible on the following bar.
//! Pivots prefer the newest equal extreme and become usable only at confirmation.

use super::{StrategyFactory, length, length_input, number, number_input, set_input};
use crate::model::{Bar, OrderSide, OrderType};
use crate::strategies::backtest::valid_bar;
use crate::strategies::{Strategy, StrategyContext};
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
}

impl PriceAction {
    fn new(rule: Rule) -> Self {
        let mut inputs = match rule {
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
        }
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

    fn prepare(&mut self, bars: &[Bar]) {
        self.pivot_high = None;
        self.pivot_low = None;
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
