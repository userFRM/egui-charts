//! Signals read public study columns at the signal bar and its predecessor. A crossing
//! requires a strict inequality now and the opposite inequality or equality one bar ago.

use super::{StrategyFactory, length, number, number_input, set_input, timeframe};
use crate::model::{Bar, BarClock, OrderSide, OrderType, Timeframe};
use crate::strategies::backtest::valid_bar;
use crate::strategies::{OrderSize, Strategy, StrategyContext};
use crate::studies::{
    ATR, BollingerBands, Indicator, InputKind, InputValue, KeltnerChannels, MACD, Momentum,
    ParabolicSAR, RSI, SMA, Stochastic, StudyInput, SuperTrend, TechnicalRatings,
};

#[derive(Clone, Copy)]
enum Rule {
    Bollinger,
    DirectedBollinger,
    Keltner,
    Macd,
    Momentum,
    Average,
    TwoAverages,
    Sar,
    Rsi,
    Stochastic,
    SuperTrend,
    Rating,
}

pub(super) fn register(factory: &mut StrategyFactory) {
    for (name, rule) in [
        ("Bollinger Bands", Rule::Bollinger),
        ("Bollinger Bands Directed", Rule::DirectedBollinger),
        ("Keltner Channels", Rule::Keltner),
        ("MACD", Rule::Macd),
        ("Momentum", Rule::Momentum),
        ("Price/Moving Average Cross", Rule::Average),
        ("Two Moving Average Cross", Rule::TwoAverages),
        ("Parabolic SAR", Rule::Sar),
        ("RSI", Rule::Rsi),
        ("Stochastic Slow", Rule::Stochastic),
        ("SuperTrend", Rule::SuperTrend),
        ("Technical Ratings", Rule::Rating),
    ] {
        factory.register(name, move || Box::new(StudyStrategy::new(rule)));
    }
}

struct StudyStrategy {
    rule: Rule,
    studies: Vec<Box<dyn Indicator>>,
    inputs: Vec<StudyInput>,
    signals: Vec<[Option<OrderType>; 2]>,
    timeframe: Option<Timeframe>,
    atr: Vec<Option<f64>>,
}

impl StudyStrategy {
    fn new(rule: Rule) -> Self {
        let study: Box<dyn Indicator> = match rule {
            Rule::Bollinger | Rule::DirectedBollinger => Box::new(BollingerBands::default()),
            Rule::Keltner => Box::new(KeltnerChannels::default()),
            Rule::Macd => Box::new(MACD::default()),
            Rule::Momentum => Box::new(Momentum::default()),
            Rule::Average => Box::new(SMA::default()),
            Rule::TwoAverages => Box::new(SMA::new(9)),
            Rule::Sar => Box::new(ParabolicSAR::default()),
            Rule::Rsi => Box::new(RSI::new(14)),
            Rule::Stochastic => Box::new(Stochastic::default()),
            Rule::SuperTrend => Box::new(SuperTrend::default()),
            Rule::Rating => Box::new(TechnicalRatings::default()),
        };
        let mut studies = vec![study];
        if matches!(rule, Rule::TwoAverages) {
            studies.push(Box::new(SMA::new(26)));
        }
        let inputs = match rule {
            Rule::DirectedBollinger => vec![StudyInput::new(
                "direction",
                "Direction",
                InputValue::Choice(0),
                InputKind::Choice(vec!["Long".into(), "Short".into()]),
            )],
            Rule::Rating => vec![StudyInput::new(
                "timeframe",
                "Indicator timeframe",
                InputValue::Timeframe(None),
                InputKind::Timeframe,
            )],
            Rule::Rsi | Rule::Stochastic => {
                let (low, high) = if matches!(rule, Rule::Rsi) {
                    (30., 70.)
                } else {
                    (20., 80.)
                };
                vec![
                    number_input("oversold", "Oversold", low, 0., 100.),
                    number_input("overbought", "Overbought", high, 0., 100.),
                ]
            }
            _ => Vec::new(),
        };
        Self {
            rule,
            studies,
            inputs,
            signals: Vec::new(),
            timeframe: None,
            atr: Vec::new(),
        }
    }

    /// Whether the setup of each working entry, long then short, is gone at this decision.
    /// Forming-bar decisions keep entries working until their bar closes.
    fn expired(
        &self,
        signals: [Option<OrderType>; 2],
        context: &StrategyContext<'_>,
        at: usize,
    ) -> [bool; 2] {
        if context.is_forming_bar() {
            return [false; 2];
        }
        match self.rule {
            // A band crossing holds for one bar: its stop works through the next bar only.
            Rule::Bollinger | Rule::DirectedBollinger => signals.map(|signal| signal.is_none()),
            // A Keltner stop works until the close falls back through the basis.
            Rule::Keltner => {
                let close = context.bars()[at].close;
                self.studies[0]
                    .values()
                    .value(0, at)
                    .map_or([false; 2], |basis| [close < basis, close > basis])
            }
            _ => [false; 2],
        }
    }
}

fn crosses(a: &[f64], b: &[f64], at: usize) -> bool {
    let Some(previous) = at.checked_sub(1) else {
        return false;
    };
    let (Some(&a), Some(&b), Some(&pa), Some(&pb)) =
        (a.get(at), b.get(at), a.get(previous), b.get(previous))
    else {
        return false;
    };
    [a, b, pa, pb].iter().all(|v| v.is_finite()) && a > b && pa <= pb
}

/// Whether a time interval is at least as long as the chart's interval.
fn longer_interval(chart: Timeframe, interval: Timeframe) -> bool {
    !chart.is_trade_based()
        && !interval.is_trade_based()
        && interval.duration_ms() >= chart.duration_ms()
}

/// Builds higher bars and maps each chart bar to the last higher bar closed by its end.
/// Returns `None` when the chart bars cannot nest in the requested interval.
fn closed_higher_bars(
    bars: &[Bar],
    chart: Timeframe,
    interval: Timeframe,
) -> Option<(Vec<Bar>, Vec<Option<usize>>)> {
    let valid: Vec<Bar> = bars.iter().filter(|bar| valid_bar(bar)).cloned().collect();
    let built = crate::data::resample(&valid, chart, interval)?;
    let (chart, higher) = (BarClock::from(chart), BarClock::from(interval));
    let read = bars
        .iter()
        .map(|bar| {
            let open = chart.bar_open(bar.time);
            let slot = higher.bar_open(open);
            let before = built.partition_point(|built| built.time < slot);
            let closed = built.get(before).is_some_and(|built| built.time == slot)
                && chart
                    .next_open(open)
                    .is_some_and(|end| higher.bar_open(end) != slot);
            if closed {
                Some(before)
            } else {
                before.checked_sub(1)
            }
        })
        .collect();
    Some((built, read))
}

impl Strategy for StudyStrategy {
    fn study_inputs(&self) -> Vec<StudyInput> {
        let mut inputs = Vec::new();
        for (index, study) in self.studies.iter().enumerate() {
            for mut input in study.inputs() {
                // A strategy draws no study table, so its look and rows are not inputs here.
                if matches!(self.rule, Rule::Rating) && input.id.starts_with("table_") {
                    continue;
                }
                // The signals read only the RSI line, so the study's smoothing, bands and
                // divergence are not inputs here; `set_input` still passes them to the study.
                if matches!(self.rule, Rule::Rsi) && input.id != "period" {
                    continue;
                }
                if matches!(self.rule, Rule::TwoAverages) {
                    let prefix = if index == 0 { "fast" } else { "slow" };
                    input.id = format!("{prefix}.{}", input.id);
                    input.label = format!(
                        "{} {}",
                        if index == 0 { "Fast" } else { "Slow" },
                        input.label
                    );
                    // These are strategy defaults; all bounds and types come from the study.
                    input.default = InputValue::Length(if index == 0 { 9 } else { 26 });
                }
                inputs.push(input);
            }
        }
        inputs.extend(self.inputs.clone());
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if self.inputs.iter().any(|input| input.id == id) {
            set_input(&mut self.inputs, id, value)?;
        } else if matches!(self.rule, Rule::TwoAverages) {
            let (prefix, id) = id.split_once('.').ok_or("unknown strategy input")?;
            let index = match prefix {
                "fast" => 0,
                "slow" => 1,
                _ => return Err("unknown strategy input"),
            };
            self.studies[index]
                .set_input(id, value)
                .map_err(|_| "invalid strategy study input")?;
        } else {
            self.studies[0]
                .set_input(id, value)
                .map_err(|_| "invalid strategy study input")?;
        }
        self.signals.clear();
        Ok(())
    }

    fn validate_inputs(&self) -> Result<(), &'static str> {
        if matches!(self.rule, Rule::Rsi | Rule::Stochastic)
            && number(&self.inputs, "oversold") >= number(&self.inputs, "overbought")
        {
            return Err("oversold must be below overbought");
        }
        if matches!(self.rule, Rule::Rating)
            && let (Some(chart), Some(interval)) =
                (self.timeframe, timeframe(&self.inputs, "timeframe"))
            && interval != chart
            && !longer_interval(chart, interval)
        {
            return Err("indicator timeframe must be the chart interval or a longer time interval");
        }
        Ok(())
    }

    fn default_order_size(&self) -> Option<OrderSize> {
        match self.rule {
            Rule::SuperTrend => Some(OrderSize::EquityFraction(0.15)),
            Rule::Rating => Some(OrderSize::EquityFraction(0.05)),
            _ => None,
        }
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
        self.signals.clear();
    }

    fn prepare(&mut self, bars: &[Bar]) {
        // Read a higher rating only once its bar has closed by this chart bar's end.
        let mut read: Option<Vec<Option<usize>>> = None;
        let mut higher = Vec::new();
        if matches!(self.rule, Rule::Rating)
            && let Some(interval) = timeframe(&self.inputs, "timeframe")
            && Some(interval) != self.timeframe
        {
            read = Some(
                self.timeframe
                    .filter(|chart| longer_interval(*chart, interval))
                    .and_then(|chart| closed_higher_bars(bars, chart, interval))
                    .map_or_else(
                        || vec![None; bars.len()],
                        |(built, read)| {
                            higher = built;
                            read
                        },
                    ),
            );
        }
        for study in &mut self.studies {
            study.calculate(if read.is_some() { &higher } else { bars });
        }
        self.atr = if matches!(self.rule, Rule::Rating) {
            let mut atr = ATR::new(14);
            atr.calculate(bars);
            (0..bars.len()).map(|i| atr.values().value(0, i)).collect()
        } else {
            Vec::new()
        };
        let close: Vec<_> = bars.iter().map(|bar| bar.close).collect();
        let zero = vec![0.; bars.len()];
        let values = self.studies[0].values();
        let line = |column| values.line(column);
        // Some studies reveal their seeded history only after their whole-run warm-up.
        // Do not trade those earlier values before a prefix could have exposed them.
        let schema = self.studies[0].inputs();
        let warmup = match self.rule {
            Rule::Macd => length(&schema, "slow_period").max(length(&schema, "signal_period")),
            Rule::SuperTrend => length(&schema, "atr_period").saturating_add(1),
            _ => 1,
        };
        let mut usable = 0;
        self.signals = (0..bars.len())
            .map(|i| {
                let bar = &bars[i];
                let valid = match self.rule {
                    Rule::SuperTrend => {
                        [bar.high, bar.low, bar.close].iter().all(|v| v.is_finite())
                    }
                    _ => bar.close.is_finite(),
                };
                usable += usize::from(valid);
                if usable < warmup {
                    return [None, None];
                }
                let previous = i.checked_sub(1);
                let value = |column| values.value(column, i);
                let before = |column| previous.and_then(|i| values.value(column, i));
                let stop = |column| value(column).map(OrderType::Stop);
                let market = |condition: bool| condition.then_some(OrderType::Market);
                match self.rule {
                    Rule::Bollinger | Rule::DirectedBollinger => [
                        stop(2).filter(|_| crosses(&close, line(2), i)),
                        stop(0).filter(|_| crosses(line(0), &close, i)),
                    ],
                    Rule::Keltner => [
                        stop(1).filter(|_| crosses(&close, line(1), i)),
                        stop(2).filter(|_| crosses(line(2), &close, i)),
                    ],
                    Rule::Macd => [
                        market(crosses(line(2), &zero, i)),
                        market(crosses(&zero, line(2), i)),
                    ],
                    Rule::Momentum => {
                        let momentum = value(0).zip(before(0));
                        [
                            market(momentum.is_some_and(|(now, prev)| now > 0. && now > prev)),
                            market(momentum.is_some_and(|(now, prev)| now < 0. && now < prev)),
                        ]
                    }
                    Rule::Average => [
                        market(crosses(&close, line(0), i)),
                        market(crosses(line(0), &close, i)),
                    ],
                    Rule::TwoAverages => {
                        let slow = self.studies[1].values().line(0);
                        [
                            market(crosses(line(0), slow, i)),
                            market(crosses(slow, line(0), i)),
                        ]
                    }
                    Rule::Sar => [before(1).and(stop(0)), before(0).and(stop(1))],
                    Rule::Rsi => {
                        let low = number(&self.inputs, "oversold");
                        let high = number(&self.inputs, "overbought");
                        let pair = value(0).zip(before(0));
                        [
                            market(pair.is_some_and(|(now, prev)| now > low && prev <= low)),
                            market(pair.is_some_and(|(now, prev)| now < high && prev >= high)),
                        ]
                    }
                    Rule::Stochastic => {
                        let low = number(&self.inputs, "oversold");
                        let high = number(&self.inputs, "overbought");
                        let pair = value(0).zip(value(1));
                        [
                            market(
                                crosses(line(0), line(1), i)
                                    && pair.is_some_and(|(k, d)| k < low && d < low),
                            ),
                            market(
                                crosses(line(1), line(0), i)
                                    && pair.is_some_and(|(k, d)| k > high && d > high),
                            ),
                        ]
                    }
                    Rule::SuperTrend => [
                        market(value(0).is_some() && before(1).is_some()),
                        market(value(1).is_some() && before(0).is_some()),
                    ],
                    // Strong Buy and Strong Sell, strictly beyond 0.5. Every such bar places the
                    // entry; the pyramiding limit keeps a held position from growing.
                    Rule::Rating => {
                        let rating = read.as_ref().map_or_else(
                            || value(2),
                            |read| read[i].and_then(|at| values.value(2, at)),
                        );
                        [
                            market(rating.is_some_and(|rating| rating > 0.5)),
                            market(rating.is_some_and(|rating| rating < -0.5)),
                        ]
                    }
                }
            })
            .collect();
    }

    fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
        if context.is_after_fill() {
            return;
        }
        let Some(at) = context.bars().len().checked_sub(1) else {
            return;
        };
        let Some(&signals) = self.signals.get(at) else {
            return;
        };
        let expired = self.expired(signals, context, at);
        if matches!(self.rule, Rule::DirectedBollinger) {
            let InputValue::Choice(direction) = self.inputs[0].value else {
                unreachable!()
            };
            if signals[1 - direction].is_some() {
                context.cancel(if direction == 0 { "Long" } else { "Short" });
                if context.position() != 0. {
                    context.exit("Close", None, None, None);
                }
            }
            if let Some(order) = signals[direction] {
                let side = if direction == 0 {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                context.entry(
                    if direction == 0 { "Long" } else { "Short" },
                    side,
                    None,
                    order,
                );
            } else if expired[direction] {
                context.cancel(if direction == 0 { "Long" } else { "Short" });
            }
            return;
        }
        for (index, side) in [OrderSide::Buy, OrderSide::Sell].into_iter().enumerate() {
            if let Some(order) = signals[index] {
                context.cancel(if index == 0 { "Short" } else { "Long" });
                context.entry(if index == 0 { "Long" } else { "Short" }, side, None, order);
            } else if expired[index] {
                context.cancel(if index == 0 { "Long" } else { "Short" });
            }
        }
        if matches!(self.rule, Rule::Rating)
            && let Some(atr) = self.atr.get(at).copied().flatten().filter(|atr| *atr > 0.0)
        {
            for (id, side) in [
                ("Long stop", OrderSide::Buy),
                ("Short stop", OrderSide::Sell),
            ] {
                context.exit_trailing(id, side, Some(3.0 * atr), 5.0 * atr, 2.0 * atr);
            }
        }
    }
}
