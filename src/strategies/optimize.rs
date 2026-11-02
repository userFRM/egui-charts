//! Lazy trials over a Cartesian product of strategy input values.

use super::{Backtest, Strategy};
use crate::model::{Bar, StrategyPerformance};
use crate::studies::InputValue;

/// The input values and report metrics of one optimisation trial.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyTrial {
    /// Input ids and the values applied, in the grid's order.
    pub inputs: Vec<(String, InputValue)>,
    /// Metrics computed by [`crate::model::StrategyReport::performance`].
    pub performance: StrategyPerformance,
}

/// Runs one fresh strategy per combination of `inputs`, on the same backtest and bars.
///
/// `create_strategy` must return an independent strategy with the same initial configuration
/// and state each time. For a configured, cloneable strategy, pass `|| Box::new(strategy.clone())`
/// to leave the original unchanged; a [`super::StrategyFactory`] can supply boxed strategies.
/// Returned strategies may borrow host data for the iterator's lifetime.
///
/// No strategy is created or run until `next`. Each `next` runs one complete backtest and
/// computes its report's performance. Consume a few trials per frame, or create and consume
/// the iterator on a worker thread. Dropping it stops the remaining trials.
///
/// Values run in the supplied order, with the last input varying fastest. No inputs means
/// one unmodified trial; any input with no values means no trials. Duplicate ids yield
/// `Err("duplicate strategy input")`. A rejected assignment yields the strategy's exact
/// [`Strategy::set_input`] error without running that trial. Either error ends the iterator.
/// `lower` has the same meaning as in [`Backtest::run`]; pass `&[]` for chart bars alone.
///
/// # Panics
///
/// As with [`Backtest::run`], a trial panics if the starting capital is not finite.
///
/// ```
/// use egui_charts::model::Timeframe;
/// use egui_charts::strategies::{Backtest, StrategyFactory, optimise};
/// use egui_charts::studies::InputValue;
///
/// let factory = StrategyFactory::new();
/// let backtest = Backtest::new("Consecutive Up/Down", "TEST", Timeframe::Min1, 10_000.0);
/// let grid = [("length".into(), vec![InputValue::Length(2), InputValue::Length(3)])];
/// // Supply the host's chart bars and optional finer bars in place of the empty slices.
/// let mut trials = optimise(
///     || factory.create("Consecutive Up/Down").unwrap(), &backtest, &[], &[], &grid,
/// );
/// assert_eq!(trials.next().unwrap().unwrap().inputs[0].1, InputValue::Length(2));
/// assert_eq!(trials.next().unwrap().unwrap().inputs[0].1, InputValue::Length(3));
/// assert!(trials.next().is_none());
/// ```
pub fn optimise<'a>(
    create_strategy: impl Fn() -> Box<dyn Strategy + 'a> + 'a,
    backtest: &'a Backtest,
    bars: &'a [Bar],
    lower: &'a [Bar],
    inputs: &'a [(String, Vec<InputValue>)],
) -> impl Iterator<Item = Result<StrategyTrial, &'static str>> + 'a {
    let mut indices = vec![0; inputs.len()];
    let mut finished = inputs.iter().any(|(_, values)| values.is_empty());
    let duplicate = inputs
        .iter()
        .enumerate()
        .any(|(i, (id, _))| inputs[..i].iter().any(|(other, _)| other == id));
    std::iter::from_fn(move || {
        if finished {
            return None;
        }
        if duplicate {
            finished = true;
            return Some(Err("duplicate strategy input"));
        }
        let mut strategy = create_strategy();
        let mut values = Vec::with_capacity(inputs.len());
        for ((id, options), &index) in inputs.iter().zip(&indices) {
            let value = options[index].clone();
            if let Err(error) = strategy.set_input(id, value.clone()) {
                finished = true;
                return Some(Err(error));
            }
            values.push((id.clone(), value));
        }
        let performance = backtest.run(strategy.as_mut(), bars, lower).performance();
        finished = true;
        for i in (0..indices.len()).rev() {
            indices[i] += 1;
            if indices[i] < inputs[i].1.len() {
                finished = false;
                break;
            }
            indices[i] = 0;
        }
        Some(Ok(StrategyTrial {
            inputs: values,
            performance,
        }))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{DateTime, TimeDelta};

    use crate::model::{Bar, OrderSide, OrderType, Timeframe};
    use crate::strategies::{Backtest, Commission, OrderSize, Strategy, StrategyContext, optimise};
    use crate::studies::InputValue;

    #[derive(Clone)]
    struct Timed<'a> {
        entry: usize,
        hold: usize,
        entered: bool,
        runs: &'a AtomicUsize,
    }

    impl Strategy for Timed<'_> {
        fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
            let target = match id {
                "entry" => &mut self.entry,
                "hold" => &mut self.hold,
                _ => return Err("unknown timed input"),
            };
            let InputValue::Length(value) = value else {
                return Err("expected a length");
            };
            if id == "hold" && value == 0 {
                return Err("hold must be positive");
            }
            *target = value;
            Ok(())
        }

        fn prepare(&mut self, _: &[Bar]) {
            assert!(!self.entered, "each run needs a fresh strategy");
            self.runs.fetch_add(1, Ordering::SeqCst);
        }

        fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
            let index = context.bars().len() - 1;
            if index == self.entry && !self.entered {
                context.entry("Long", OrderSide::Buy, None, OrderType::Market);
                self.entered = true;
            } else if index == self.entry + self.hold {
                context.exit("Close", None, None, None);
            }
        }
    }

    fn axis(id: &str, values: &[usize]) -> (String, Vec<InputValue>) {
        (
            id.into(),
            values.iter().copied().map(InputValue::Length).collect(),
        )
    }

    fn source() -> Vec<Bar> {
        [100.0, 102.0, 107.0, 99.0, 111.0]
            .into_iter()
            .enumerate()
            .map(|(i, price)| {
                Bar::new(
                    DateTime::UNIX_EPOCH + TimeDelta::minutes(i as i64),
                    price,
                    price + 5.0,
                    price - 5.0,
                    price,
                    1.0,
                )
            })
            .collect()
    }

    #[test]
    fn grid_trials_are_lazy_independent_and_match_separate_runs() {
        let bars = source();
        // Only the latter half of each chart bar is supplied, at a different price.
        let lower: Vec<_> = bars
            .iter()
            .enumerate()
            .map(|(i, bar)| {
                let price = bar.close + i as f64;
                Bar::new(
                    bar.time + TimeDelta::seconds(30),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect();
        let grid = [axis("entry", &[0, 1]), axis("hold", &[1, 2])];
        for (fill_on_close, finer, profits) in [
            (true, &[][..], [2.0, 12.0, 8.0, -8.0]),
            (false, &[][..], [8.0, -8.0, -18.0, 6.0]),
            (false, lower.as_slice(), [10.0, -4.0, -16.0, 10.0]),
        ] {
            let runs = AtomicUsize::new(0);
            let base = Timed {
                entry: 0,
                hold: 1,
                entered: false,
                runs: &runs,
            };
            let mut backtest = Backtest::new("Grid", "TEST", Timeframe::Min1, 1_000.0);
            backtest.settings.fill_on_close = fill_on_close;
            backtest.settings.order_size = OrderSize::Fixed(2.0);
            backtest.settings.commission = Commission::PerUnit(0.5);
            let mut trials = optimise(|| Box::new(base.clone()), &backtest, &bars, finer, &grid);
            assert_eq!(base.runs.load(Ordering::SeqCst), 0);
            for (i, (entry, hold)) in [(0, 1), (0, 2), (1, 1), (1, 2)].into_iter().enumerate() {
                let trial = trials.next().unwrap().unwrap();
                assert_eq!(base.runs.load(Ordering::SeqCst), i + 1);
                assert_eq!(
                    trial.inputs,
                    vec![
                        ("entry".into(), InputValue::Length(entry)),
                        ("hold".into(), InputValue::Length(hold)),
                    ]
                );
                // Two units times the exit minus entry price, less 1 on each fill.
                assert_eq!(trial.performance.net_profit, profits[i]);
                let separate_runs = AtomicUsize::new(0);
                let mut separate = Timed {
                    entry,
                    hold,
                    runs: &separate_runs,
                    ..base.clone()
                };
                assert_eq!(
                    trial.performance,
                    backtest.run(&mut separate, &bars, finer).performance()
                );
            }
            assert!(trials.next().is_none());
            assert!(trials.next().is_none());
            assert_eq!((base.entry, base.hold, base.entered), (0, 1, false));
            let mut partial = optimise(|| Box::new(base.clone()), &backtest, &bars, finer, &grid);
            std::thread::scope(|scope| {
                scope
                    .spawn(move || partial.next().unwrap().unwrap())
                    .join()
                    .unwrap();
            });
            assert_eq!(base.runs.load(Ordering::SeqCst), 5);
        }
    }

    #[test]
    fn empty_grids_and_rejected_inputs_end_iteration_without_extra_runs() {
        let bars = source();
        let backtest = Backtest::new("Grid", "TEST", Timeframe::Min1, 1_000.0);
        for (grid, expected_runs, error) in [
            (vec![], 1, None),
            (vec![axis("entry", &[])], 0, None),
            (vec![axis("entry", &[0, 1, 2]), axis("hold", &[])], 0, None),
            (
                vec![axis("unknown", &[0]), axis("hold", &[1])],
                0,
                Some("unknown timed input"),
            ),
            (
                vec![axis("entry", &[0, 1]), axis("unknown", &[1])],
                0,
                Some("unknown timed input"),
            ),
            (
                vec![("hold".into(), vec![InputValue::Toggle(true)])],
                0,
                Some("expected a length"),
            ),
            (
                vec![axis("hold", &[0, 1])],
                0,
                Some("hold must be positive"),
            ),
            (
                vec![axis("hold", &[1, 0, 2])],
                1,
                Some("hold must be positive"),
            ),
            (
                vec![axis("hold", &[1, 2]), axis("hold", &[2])],
                0,
                Some("duplicate strategy input"),
            ),
        ] {
            let runs = AtomicUsize::new(0);
            let base = Timed {
                entry: 0,
                hold: 1,
                entered: false,
                runs: &runs,
            };
            let mut trials = optimise(|| Box::new(base.clone()), &backtest, &bars, &[], &grid);
            assert_eq!(base.runs.load(Ordering::SeqCst), 0);
            for _ in 0..expected_runs {
                trials.next().unwrap().unwrap();
            }
            if let Some(error) = error {
                assert_eq!(trials.next().unwrap().unwrap_err(), error);
            }
            assert!(trials.next().is_none(), "{grid:?}");
            assert!(trials.next().is_none(), "{grid:?}");
            assert_eq!(base.runs.load(Ordering::SeqCst), expected_runs, "{grid:?}");
            assert_eq!((base.entry, base.hold, base.entered), (0, 1, false));
        }
    }
}
