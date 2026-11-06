//! Incremental execution over host-supplied closed and forming bars.

use super::*;

/// One executed order, including an entry, an exit or a margin liquidation.
/// A reversal is one fill whose quantity includes closing the previous position.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyFill {
    /// Increasing identifier within this live run, starting at one. Retained across rebuilds.
    pub sequence: u64,
    /// Source bar time, or the finer bar time for a historical magnified execution.
    pub time: DateTime<Utc>,
    /// Identifier placed through the strategy context, or `"Margin call"` for liquidation.
    pub order_id: String,
    /// Executed order side.
    pub side: OrderSide,
    /// Execution price, including applicable slippage.
    pub price: f64,
    /// Total executed units, always positive.
    pub quantity: f64,
    /// Signed position immediately after this fill, before any fill callback orders execute.
    pub position: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LiveState {
    bars: Vec<Bar>,
    lower: Vec<Bar>,
    settings: BacktestSettings,
    inputs: Vec<(String, String)>,
    report: StrategyReport,
    processed: usize,
    forming: Option<(DateTime<Utc>, f64)>,
    sequence: u64,
    orders: Vec<Working>,
    lots: Vec<Lot>,
    trades: Vec<StrategyTrade>,
    realized: f64,
    rejected: usize,
    tick: u64,
}

// Compare bits so unchanged invalid feed values do not cause an endless rebuild.
fn same_bar(a: &Bar, b: &Bar) -> bool {
    a.time == b.time
        && [a.open, a.high, a.low, a.close, a.volume].map(f64::to_bits)
            == [b.open, b.high, b.low, b.close, b.volume].map(f64::to_bits)
}

fn same_bars(a: &[Bar], b: &[Bar]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_bar(a, b))
}

fn same_lower(a: &[Bar], b: &[Bar], before: Option<DateTime<Utc>>) -> bool {
    let historical = |bar: &&Bar| before.is_none_or(|at| bar.time < at);
    let mut a = a.iter().filter(historical);
    let mut b = b.iter().filter(historical);
    loop {
        match (a.next(), b.next()) {
            (Some(a), Some(b)) if same_bar(a, b) => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

impl Backtest {
    /// The current live report, available after the first [`Self::update`].
    /// With tick recalculation, trades include forming-bar fills; equity marks are closed bars.
    /// [`Self::run`] remains an independent historical calculation and does not change this state.
    pub fn report(&self) -> Option<&StrategyReport> {
        self.live.as_ref().map(|live| &live.report)
    }

    /// Continues a live strategy on `bars`, oldest first, with the last bar still forming.
    /// Pass the same strategy instance and the full retained history on each call. The first
    /// call processes the closed history and returns its fills; discard that batch when priming
    /// a run that should alert only on subsequent fills. `lower` has the meaning it has in `run`.
    ///
    /// With `settings.recalculate_on_every_update` off, only newly closed bars execute, exactly
    /// as in `run` over `bars` without its last bar. With it on, an observed forming bar walks
    /// from its open to its close, then from its previous close to each new close, and decides
    /// again with that forming bar included in the context. Its fills and orders are retained
    /// when the bar closes; unseen intrabar extremes are never replayed over observed ticks.
    /// Identical snapshots do nothing. Live fill callbacks exclude the unfinished bar;
    /// historical tick callbacks see their developing candle.
    ///
    /// A correction before the previous forming bar, removed history, changed settings or inputs
    /// rebuilds from the start using `Strategy::prepare` to reset decision state. Replayed fills
    /// before the previous forming bar are not returned again. An observed forming bar can be
    /// revised freely without a rebuild. Start a new `Backtest` for a different strategy instance.
    ///
    /// # Panics
    ///
    /// Panics if starting capital is not finite or bar timestamps are not strictly increasing.
    pub fn update(
        &mut self,
        strategy: &mut dyn Strategy,
        bars: &[Bar],
        lower: &[Bar],
    ) -> Vec<StrategyFill> {
        assert!(
            bars.windows(2).all(|pair| pair[0].time < pair[1].time),
            "bar times must be strictly increasing"
        );
        let inputs = strategy.inputs();
        let previous = self.live.take();
        let replay_before = previous
            .as_ref()
            .and_then(|live| live.bars.last().map(|bar| bar.time));
        let replayed_forming = previous.as_ref().is_some_and(|live| live.forming.is_some());
        let replayed = |time| {
            replay_before
                .is_some_and(|before| time < before || (replayed_forming && time == before))
        };
        let sequence = previous.as_ref().map_or(0, |live| live.sequence);
        let rebuild = previous.as_ref().is_none_or(|live| {
            let prefix = live.bars.len().saturating_sub(1);
            bars.len() < live.bars.len()
                || !same_bars(&live.bars[..prefix], &bars[..prefix.min(bars.len())])
                || live
                    .bars
                    .last()
                    .zip(bars.get(prefix))
                    .is_some_and(|(a, b)| a.time != b.time)
                || live.settings != self.settings
                || live.inputs != inputs
                || live.report.name != self.name
                || live.report.symbol != self.symbol
                || live.report.timeframe != self.timeframe
                || !same_lower(&live.lower, lower, replay_before)
        });
        let mut live = if rebuild {
            Box::new(LiveState {
                bars: Vec::new(),
                lower: Vec::new(),
                settings: self.settings.clone(),
                inputs,
                report: self.new_report(strategy, lower.len()),
                processed: 0,
                forming: None,
                sequence,
                orders: Vec::new(),
                lots: Vec::new(),
                trades: Vec::new(),
                realized: 0.0,
                rejected: 0,
                tick: 0,
            })
        } else {
            previous.expect("an existing live run")
        };
        if let Some((_, count)) = live
            .report
            .properties
            .iter_mut()
            .find(|(name, _)| name == "Lower-interval bars")
        {
            *count = lower.len().to_string();
        }
        let closed = bars.len().saturating_sub(1);
        let tick_changed =
            self.settings.recalculate_on_every_update && !same_bars(&live.bars, bars);
        let changed = rebuild || live.processed < closed || tick_changed;
        if !changed {
            live.bars = bars.to_vec();
            live.lower = lower.to_vec();
            self.live = Some(live);
            return Vec::new();
        }
        let candles = (self.settings.chart_type == ChartType::Heikin)
            .then(|| crate::model::BarData::from_bars(bars.to_vec()).to_heikin_ashi());
        let decision_bars = candles.as_ref().map_or(bars, |data| data.bars.as_slice());
        let calculation_bars = if self.settings.recalculate_on_every_update {
            decision_bars
        } else if self.settings.recalculate_on_history
            && !self.settings.chart_type.transforms_data()
        {
            &[]
        } else {
            &decision_bars[..closed]
        };
        if rebuild {
            strategy.prepare(calculation_bars);
        } else {
            strategy.prepare_update(calculation_bars);
        }
        let mut fills = Vec::new();
        let mut run = Run {
            orders: std::mem::take(&mut live.orders),
            lots: std::mem::take(&mut live.lots),
            trades: std::mem::take(&mut live.trades),
            realized: live.realized,
            rejected: live.rejected,
            settings: &self.settings,
            strategy,
            history: Cow::Borrowed(&[]),
            prices: bars,
            trading: false,
            tick: live.tick,
            open_tick: 0,
            fills: Some(&mut fills),
            forming: false,
        };
        let mut finer: Vec<_> = lower.iter().filter(|bar| valid_bar(bar)).collect();
        finer.sort_by_key(|bar| bar.time);
        finer.dedup_by_key(|bar| bar.time);
        // Execution intervals learn the previous final bar's end when the next bar closes.
        if self.timeframe.is_trade_based()
            && self.settings.start_time.is_some()
            && live.processed > 0
            && live.processed < closed
            && let Some(mark) = live.report.equity.last_mut()
        {
            mark.0 = bars[live.processed].time;
        }
        if live.report.equity.is_empty() {
            run.start_equity(self.timeframe, &bars[..closed], &mut live.report);
        }
        let bar_closed = live.processed < closed;
        for index in live.processed..closed {
            let bar = &bars[index];
            let observed = live.forming.is_some_and(|(time, _)| time == bar.time);
            if observed {
                if live.bars.get(index).is_none_or(|old| !same_bar(old, bar)) {
                    run.forming_bar(
                        decision_bars,
                        index,
                        live.forming.map(|(_, price)| price),
                        false,
                    );
                }
                live.forming = None;
            }
            run.closed_bar(
                &decision_bars[..closed],
                index,
                &finer,
                &mut live.report,
                !observed,
            );
            if rebuild
                && replayed(bar.time)
                && let Some(fills) = &mut run.fills
            {
                fills.clear();
            }
        }
        live.processed = closed;
        if self.settings.recalculate_on_every_update
            && let Some(bar) = bars.last()
        {
            let previous_price = live
                .forming
                .filter(|(time, _)| *time == bar.time)
                .map(|(_, price)| price);
            // Closing a bar left the calculations at that bar, one short of this decision's bars.
            if bar_closed
                && self.settings.recalculate_on_history
                && !self.settings.chart_type.transforms_data()
            {
                run.strategy.prepare_update(decision_bars);
            }
            if run.forming_bar(decision_bars, closed, previous_price, true) {
                let execution = if self.settings.chart_type == ChartType::Heikin
                    && !self.settings.original_price_fills
                {
                    &decision_bars[closed]
                } else {
                    bar
                };
                live.forming = Some((bar.time, execution.close));
            }
            if rebuild
                && replayed(bar.time)
                && let Some(fills) = &mut run.fills
            {
                fills.clear();
            }
        }
        run.report_trades(&mut live.report);
        live.orders = run.orders;
        live.lots = run.lots;
        live.trades = run.trades;
        live.realized = run.realized;
        live.rejected = run.rejected;
        live.tick = run.tick;
        for fill in &mut fills {
            live.sequence += 1;
            fill.sequence = live.sequence;
        }
        live.bars = bars.to_vec();
        live.lower = lower.to_vec();
        self.live = Some(live);
        fills
    }
}

impl<'a> Run<'a> {
    fn forming_bar(
        &mut self,
        bars: &'a [Bar],
        index: usize,
        previous: Option<f64>,
        decide: bool,
    ) -> bool {
        let bar = &bars[index];
        self.trading = self
            .settings
            .start_time
            .is_none_or(|start| bar.time >= start);
        if !valid_bar(bar) || !valid_bar(&self.prices[index]) {
            return false;
        }
        self.history = Cow::Borrowed(&bars[..index]);
        self.forming = true;
        let execution = if self.settings.chart_type == ChartType::Heikin
            && self.settings.original_price_fills
        {
            &self.prices[index]
        } else {
            bar
        };
        if self.trading {
            if previous.is_none() {
                self.open_bar();
            }
            self.fill_along(
                previous.unwrap_or(execution.open),
                execution.close,
                bar.time,
            );
        }
        if decide {
            self.history = Cow::Borrowed(&bars[..=index]);
            self.decide(bar.close, false);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::StrategyFactory;

    fn bar(minute: i64, close: f64) -> Bar {
        Bar::new(
            DateTime::UNIX_EPOCH + TimeDelta::minutes(minute),
            close,
            close + 2.0,
            close - 2.0,
            close,
            1.0,
        )
    }

    #[derive(Default)]
    struct Script {
        decisions: Vec<usize>,
        resets: usize,
    }

    impl Strategy for Script {
        fn prepare(&mut self, _: &[Bar]) {
            self.decisions.clear();
            self.resets += 1;
        }
        fn prepare_update(&mut self, _: &[Bar]) {}
        fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
            if context.is_after_fill() {
                return;
            }
            let index = context.bars().len();
            self.decisions.push(index);
            match index {
                1 => context.entry("Long", OrderSide::Buy, 2.0, OrderType::Market),
                3 => context.exit("Part", Some(1.0), None, None),
                5 => context.entry("Short", OrderSide::Sell, 3.0, OrderType::Market),
                7 => context.exit("Flat", None, None, None),
                _ => {}
            }
        }
    }

    #[test]
    fn closed_updates_match_full_runs_without_repeating_decisions() {
        let bars: Vec<_> = [100., 101., 104., 102., 98., 95., 97., 99., 96.]
            .into_iter()
            .enumerate()
            .map(|(i, price)| bar(i as i64, price))
            .collect();
        for (fill_on_close, start_time, magnifier, timeframe) in [
            (false, None, false, Timeframe::Min1),
            (true, None, false, Timeframe::Min1),
            (false, Some(bars[2].time), false, Timeframe::Min1),
            (true, Some(bars[2].time), true, Timeframe::Min1),
            (
                false,
                Some(bars[0].time),
                false,
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
            ),
            (
                true,
                Some(bars[0].time),
                false,
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
            ),
            (
                true,
                None,
                true,
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
            ),
            (
                false,
                Some(bars[2].time + TimeDelta::seconds(30)),
                false,
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
            ),
        ] {
            let mut engine = Backtest::new("Live", "SYM", timeframe, 10_000.0);
            engine.settings.fill_on_close = fill_on_close;
            engine.settings.start_time = start_time;
            engine.settings.commission = Commission::PerOrder(1.0);
            engine.settings.slippage = Slippage::Ticks {
                ticks: 1,
                tick_size: 0.25,
            };
            engine.settings.recalculate_after_fills = true;
            let mut script = Script::default();
            let mut sequence = 0;
            for count in 0..=bars.len() {
                let lower = if magnifier {
                    bars[..count.saturating_sub(1)].to_vec()
                } else {
                    vec![]
                };
                let fills = engine.update(&mut script, &bars[..count], &lower);
                for fill in fills {
                    sequence += 1;
                    assert_eq!(fill.sequence, sequence);
                }
                let full = engine.run(
                    &mut Script::default(),
                    &bars[..count.saturating_sub(1)],
                    &lower,
                );
                let asynchronous = {
                    let mut strategy = Script::default();
                    let mut future = std::pin::pin!(engine.run_async(
                        &mut strategy,
                        &bars[..count.saturating_sub(1)],
                        &lower,
                    ));
                    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                    loop {
                        if let std::task::Poll::Ready(report) =
                            std::future::Future::poll(future.as_mut(), &mut context)
                        {
                            break report;
                        }
                    }
                };
                assert_eq!(asynchronous, full, "async prefix {count}");
                assert_eq!(
                    engine.report(),
                    Some(&full),
                    "prefix {count}, close {fill_on_close}, start {start_time:?}"
                );
                assert_eq!(script.decisions, (1..count).collect::<Vec<_>>());
                assert!(
                    engine
                        .update(&mut script, &bars[..count], &lower)
                        .is_empty()
                );
                assert_eq!(script.decisions, (1..count).collect::<Vec<_>>());
            }
            assert_eq!(script.resets, 1);
            // Correcting closed history resets decisions, but does not repeat its fill events.
            let mut corrected = bars.clone();
            corrected[1] = bar(1, 120.0);
            assert!(engine.update(&mut script, &corrected, &[]).is_empty());
            assert_eq!(script.resets, 2);
            assert_eq!(
                engine.report(),
                Some(&engine.run(&mut Script::default(), &corrected[..8], &[]))
            );
        }
        // Every built-in refreshes its calculations while retaining its decision state.
        let factory = StrategyFactory::new();
        let source: Vec<_> = (0..80)
            .map(|i| bar(i, 100.0 + f64::from((i % 13) as i32) * 2.0))
            .collect();
        for name in factory.list() {
            let mut strategy = factory.create(name).unwrap();
            let mut engine = Backtest::new(name, "SYM", Timeframe::Min1, 10_000.0);
            for count in 1..=source.len() {
                engine.update(strategy.as_mut(), &source[..count], &[]);
            }
            let full = engine.run(
                factory.create(name).unwrap().as_mut(),
                &source[..source.len() - 1],
                &[],
            );
            assert_eq!(engine.report(), Some(&full), "{name}");
        }
    }

    #[test]
    fn forming_ticks_keep_fills_and_do_not_replay_extremes_at_close() {
        struct TickStrategy;
        impl Strategy for TickStrategy {
            fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
                if context.is_after_fill() {
                    return;
                }
                if context.price() >= 105.0 && context.position() == 0.0 {
                    context.entry("Long", OrderSide::Buy, 2.0, OrderType::Market);
                    context.exit("Target", None, Some(110.0), None);
                }
            }
        }
        let mut engine = Backtest::new("Ticks", "SYM", Timeframe::Min1, 10_000.0);
        engine.settings.recalculate_on_every_update = true;
        let mut bars = vec![bar(0, 100.0)];
        let mut strategy = TickStrategy;
        assert!(engine.update(&mut strategy, &bars, &[]).is_empty());
        bars[0] = Bar::new(bars[0].time, 100., 120., 90., 106., 2.);
        assert!(engine.update(&mut strategy, &bars, &[]).is_empty());
        assert!(engine.update(&mut strategy, &bars, &[]).is_empty());
        bars[0].close = 108.;
        let fills = engine.update(&mut strategy, &bars, &[]);
        assert_eq!(fills.len(), 1);
        // A market order created at 106 fills at the next observed tick, 108.
        assert_eq!(
            (fills[0].price, fills[0].quantity, fills[0].position),
            (108., 2., 2.)
        );
        assert_eq!(
            (&*fills[0].order_id, fills[0].side, fills[0].time),
            ("Long", OrderSide::Buy, bars[0].time)
        );
        bars.push(bar(1, 108.));
        assert!(engine.update(&mut strategy, &bars, &[]).is_empty());
        // Unseen high 120 never closes the live position; its close equals the entry at 108.
        let report = engine.report().unwrap();
        assert_eq!(report.equity.last().unwrap().1, 10_000.);
        assert!(report.trades[0].exit.is_none());
        bars[1].high = 112.;
        bars[1].close = 112.;
        let fills = engine.update(&mut strategy, &bars, &[]);
        assert_eq!(fills.len(), 1);
        assert_eq!(
            (
                fills[0].sequence,
                fills[0].price,
                fills[0].quantity,
                fills[0].position
            ),
            (2, 110., 2., 0.)
        );
        assert_eq!(fills[0].side, OrderSide::Sell);
        assert_eq!(
            engine.report().unwrap().trades[0]
                .exit
                .as_ref()
                .unwrap()
                .profit,
            4.
        );
        // A historical rebuild must not announce the observed forming-bar executions again.
        bars[0].high = 121.;
        assert!(engine.update(&mut strategy, &bars, &[]).is_empty());
    }

    /// A fill carries no study output, so a plot placeholder in a fill message is missing.
    #[test]
    #[cfg(feature = "alert-engine")]
    fn strategy_fill_message_reads_no_study_value() {
        use crate::model::{Alert, AlertCondition};
        use crate::services::{AlertEngine, AlertEvent};
        use std::sync::{Arc, Mutex};
        let mut alert = Alert::new("SYM", AlertCondition::StrategyFills("run-1".into()));
        alert.message = "{{value}} rsi {{plot_3_0}}".into();
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let sink = delivered.clone();
        let engine = AlertEngine::new()
            .with_alerts(vec![alert])
            .on_trigger(move |_, event| sink.lock().unwrap().push(event.clone()));
        let fill = StrategyFill {
            sequence: 1,
            time: DateTime::UNIX_EPOCH,
            order_id: "Long".into(),
            side: OrderSide::Buy,
            price: 187.5,
            quantity: 2.0,
            position: 2.0,
        };
        engine.strategy_fills(
            "run-1",
            "SYM",
            None,
            Timeframe::Min1,
            &[fill],
            DateTime::UNIX_EPOCH,
        );
        let events = delivered.lock().unwrap();
        let [AlertEvent::TriggeredForSymbol { message, .. }] = events.as_slice() else {
            panic!("one fill");
        };
        assert_eq!(message, "187.5 rsi NaN");
    }

    #[test]
    #[cfg(feature = "alert-engine")]
    fn strategy_alerts_deliver_each_fill_once_even_at_the_same_timestamp() {
        use crate::model::{Alert, AlertCondition};
        use crate::services::{AlertEngine, AlertEvent};
        use std::sync::{Arc, Mutex};
        struct Reverse;
        impl Strategy for Reverse {
            fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
                match context.bars().len() {
                    1 => {
                        context.entry("A", OrderSide::Buy, 2., OrderType::Market);
                        context.entry("B", OrderSide::Buy, 2., OrderType::Market);
                    }
                    2 => context.entry("Reverse", OrderSide::Sell, 3., OrderType::Market),
                    3 => context.exit("Flat", None, None, None),
                    _ => {}
                }
            }
        }
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let sink = delivered.clone();
        let mut alert = Alert::new("SYM", AlertCondition::StrategyFills("run-1".into()));
        alert.timeframe = Some(Timeframe::Min1);
        let alerts = AlertEngine::new()
            .with_alerts(vec![alert])
            .on_trigger(move |_, event| {
                sink.lock().unwrap().push(event.clone());
            });
        let mut engine = Backtest::new("Run", "SYM", Timeframe::Min1, 10_000.);
        engine.settings.pyramiding = 2;
        let bars: Vec<_> = [187.5, 187.5, 180., 175., 175.]
            .into_iter()
            .enumerate()
            .map(|(i, p)| bar(i as i64, p))
            .collect();
        let mut all = vec![];
        for count in 1..=bars.len() {
            let fills = engine.update(&mut Reverse, &bars[..count], &[]);
            alerts.strategy_fills(
                "other-run",
                "SYM",
                None,
                Timeframe::Min1,
                &fills,
                bars[count - 1].time,
            );
            alerts.strategy_fills(
                "run-1",
                "OTHER",
                None,
                Timeframe::Min1,
                &fills,
                bars[count - 1].time,
            );
            alerts.strategy_fills(
                "run-1",
                "SYM",
                None,
                Timeframe::Hour1,
                &fills,
                bars[count - 1].time,
            );
            assert_eq!(
                delivered.lock().unwrap().len(),
                all.len(),
                "other subjects do not notify"
            );
            alerts.strategy_fills(
                "run-1",
                "SYM",
                None,
                Timeframe::Min1,
                &fills,
                bars[count - 1].time,
            );
            alerts.strategy_fills(
                "run-1",
                "SYM",
                None,
                Timeframe::Min1,
                &fills,
                bars[count - 1].time,
            );
            all.extend(fills);
        }
        assert_eq!(
            all.iter()
                .map(|fill| (fill.side, fill.price, fill.quantity, fill.position))
                .collect::<Vec<_>>(),
            [
                (OrderSide::Buy, 187.5, 2., 2.),
                (OrderSide::Buy, 187.5, 2., 4.),
                (OrderSide::Sell, 180., 7., -3.),
                (OrderSide::Buy, 175., 3., 0.),
            ]
        );
        let events = delivered.lock().unwrap();
        assert_eq!(events.len(), 4);
        for (event, fill) in events.iter().zip(&all) {
            let AlertEvent::TriggeredForSymbol {
                time,
                price,
                message,
                ..
            } = event
            else {
                panic!("fill trigger");
            };
            assert_eq!((*time, *price), (fill.time, Some(fill.price)));
            assert!(!message.is_empty());
        }
        let AlertEvent::TriggeredForSymbol { message, .. } = &events[1] else {
            panic!("fill trigger");
        };
        assert_eq!(message, "Buy 2 at 187.50, position 4");
        drop(events);
        // Saving edits and rearming keep the execution watermark for the same run.
        use crate::data::inbox::Inbox;
        use crate::services::Alerts;
        let inbox = Arc::new(Inbox::default());
        for status in [
            crate::model::AlertStatus::Disabled,
            crate::model::AlertStatus::Active,
        ] {
            let mut edited = alerts.alerts()[0].clone();
            edited.name = "Renamed".into();
            edited.status = status;
            alerts.save(edited, inbox.reply(0));
            alerts.strategy_fills("run-1", "SYM", None, Timeframe::Min1, &all, bars[4].time);
            assert_eq!(delivered.lock().unwrap().len(), 4);
        }
        let mut next = all[0].clone();
        next.sequence = 5;
        alerts.strategy_fills("run-1", "SYM", None, Timeframe::Min1, &[next], bars[4].time);
        assert_eq!(delivered.lock().unwrap().len(), 5);
        // Fill alerts obey the same lifecycle rules, without needing a price subscription.
        use crate::model::{
            AlertFrequency, AlertStatus, NotificationSchedule, NotificationWindow, TimezoneId,
        };
        type Edit = fn(&mut Alert);
        for (edit, count) in [
            (
                (|alert: &mut Alert| alert.frequency = AlertFrequency::Once) as Edit,
                1,
            ),
            (|alert| alert.status = AlertStatus::Disabled, 0),
            (|alert| alert.expires = Some(DateTime::UNIX_EPOCH), 0),
            (
                |alert| {
                    alert.schedule = Some(Box::new(NotificationSchedule {
                        timezone: TimezoneId::utc(),
                        days: vec![chrono::Weekday::Thu],
                        windows: vec![NotificationWindow {
                            start: chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
                            end: chrono::NaiveTime::from_hms_opt(13, 0, 0).unwrap(),
                        }],
                    }))
                },
                0,
            ),
        ] {
            let mut alert = Alert::new("SYM", AlertCondition::StrategyFills("run-1".into()));
            edit(&mut alert);
            let delivered = Arc::new(Mutex::new(Vec::new()));
            let sink = delivered.clone();
            let engine = AlertEngine::new()
                .with_alerts(vec![alert])
                .on_trigger(move |_, event| {
                    sink.lock().unwrap().push(event.clone());
                });
            assert!(
                engine
                    .subjects(engine.alerts()[0].id.as_ref().unwrap())
                    .is_empty()
            );
            engine.strategy_fills("run-1", "SYM", None, Timeframe::Min1, &all, bars[4].time);
            assert_eq!(delivered.lock().unwrap().len(), count);
            // Opening the schedule must not repeat fills observed outside it.
            engine.strategy_fills(
                "run-1",
                "SYM",
                None,
                Timeframe::Min1,
                &all,
                DateTime::UNIX_EPOCH + TimeDelta::hours(12),
            );
            assert_eq!(delivered.lock().unwrap().len(), count);
        }
        for (message, expected) in [
            ("", "Buy 2 at 0.00025, position 2"),
            ("{{symbol}} {{value}}", "SYM 0.00025"),
        ] {
            let mut alert = Alert::new("SYM", AlertCondition::StrategyFills("run-1".into()));
            alert.message = message.into();
            let delivered = Arc::new(Mutex::new(Vec::new()));
            let sink = delivered.clone();
            let engine = AlertEngine::new()
                .with_alerts(vec![alert])
                .on_trigger(move |_, event| {
                    sink.lock().unwrap().push(event.clone());
                });
            let mut fill = all[0].clone();
            fill.price = 0.00025;
            engine.strategy_fills("run-1", "SYM", None, Timeframe::Min1, &[fill], bars[4].time);
            let events = delivered.lock().unwrap();
            let [AlertEvent::TriggeredForSymbol { message, .. }] = events.as_slice() else {
                panic!("one fill");
            };
            assert_eq!(message, expected);
        }
    }

    #[test]
    fn forming_decision_after_a_close_sees_refreshed_calculations() {
        /// Records decisions whose bars differ from the bars of the last calculation.
        #[derive(Default)]
        struct Prefix {
            prepared: usize,
            stale: Vec<(usize, usize)>,
        }
        impl Strategy for Prefix {
            fn prepare(&mut self, bars: &[Bar]) {
                self.prepared = bars.len();
            }
            fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
                if context.bars().len() != self.prepared {
                    self.stale.push((context.bars().len(), self.prepared));
                }
            }
        }
        let bars: Vec<_> = (0..3).map(|i| bar(i, 100.0 + i as f64)).collect();
        let mut engine = Backtest::new("Live", "SYM", Timeframe::Min1, 1000.0);
        engine.settings.recalculate_on_every_update = true;
        engine.settings.recalculate_on_history = true;
        let mut strategy = Prefix::default();
        for count in 1..=bars.len() {
            engine.update(&mut strategy, &bars[..count], &[]);
        }
        assert_eq!(strategy.stale, Vec::<(usize, usize)>::new());
    }
}
