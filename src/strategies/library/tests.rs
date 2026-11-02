use super::*;
use crate::model::{Bar, OrderSide, StrategyReport, Timeframe};
use crate::strategies::{Backtest, StrategyContext};
use chrono::{DateTime, TimeDelta, Utc};

fn time(bar: usize) -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + TimeDelta::minutes(bar as i64)
}

fn bars(prices: &[[f64; 4]]) -> Vec<Bar> {
    prices
        .iter()
        .enumerate()
        .map(|(i, &[open, high, low, close])| Bar::new(time(i), open, high, low, close, 1.0))
        .collect()
}

fn points(prices: &[f64]) -> Vec<[f64; 4]> {
    prices.iter().map(|&p| [p; 4]).collect()
}

type Entry = (usize, OrderSide, f64);

fn entries(report: &StrategyReport) -> Vec<Entry> {
    let mut entries: Vec<_> = report
        .trades
        .iter()
        .map(|trade| {
            (
                ((trade.entry_time - time(0)).num_minutes() as usize),
                trade.side,
                trade.entry_price,
            )
        })
        .collect();
    entries.sort_by_key(|entry| entry.0);
    entries
}

fn configured(name: &str, inputs: &[(&str, InputValue)]) -> Box<dyn Strategy> {
    let mut strategy = StrategyFactory::new().create(name).unwrap();
    for (id, value) in inputs {
        strategy.set_input(id, value.clone()).unwrap();
    }
    strategy
}

fn assert_prefixes(name: &str, inputs: &[(&str, InputValue)], bars: &[Bar], backtest: &Backtest) {
    let full = backtest.run(configured(name, inputs).as_mut(), bars, &[]);
    for end in 1..=bars.len() {
        let prefix = backtest.run(configured(name, inputs).as_mut(), &bars[..end], &[]);
        let expected: Vec<_> = entries(&full)
            .into_iter()
            .filter(|entry| entry.0 < end)
            .collect();
        assert_eq!(entries(&prefix), expected, "{name}, prefix {end}");
        for trade in &prefix.trades {
            let full_trade = full
                .trades
                .iter()
                .find(|t| t.entry_time == trade.entry_time && t.side == trade.side)
                .unwrap();
            let expected = full_trade
                .exit
                .as_ref()
                .filter(|exit| exit.time <= bars[end - 1].time);
            assert_eq!(trade.exit, expected.cloned(), "{name}, exit prefix {end}");
        }
    }
}

#[test]
fn price_action_entries_match_hand_worked_bars_without_lookahead() {
    use InputValue::{Length, Number};
    use OrderSide::{Buy, Sell};
    struct Case {
        name: &'static str,
        slippage: f64,
        prices: Vec<[f64; 4]>,
        inputs: Vec<(&'static str, InputValue)>,
        expected: Vec<Entry>,
        exits: Vec<(usize, usize, f64)>,
    }
    let cases = [
        Case {
            slippage: 0.0,
            name: "Bar Up/Down",
            inputs: vec![],
            prices: vec![
                [10., 11., 9., 10.],
                [11., 13., 11., 12.],
                [12., 13., 11., 12.],
                [11., 11., 9., 10.],
                [10.; 4],
            ],
            expected: vec![(2, Buy, 12.), (4, Sell, 10.)],
            exits: vec![(0, 4, 10.)],
        },
        Case {
            slippage: 0.0,
            name: "Consecutive Up/Down",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 11., 10., 10.]),
            expected: vec![(3, Buy, 11.), (5, Sell, 10.)],
            exits: vec![(0, 5, 10.)],
        },
        // Repeated qualifying closes honour the engine's two-entry pyramiding setting.
        Case {
            slippage: 0.0,
            name: "Consecutive Up/Down",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 13., 13.]),
            expected: vec![(3, Buy, 13.), (4, Buy, 13.)],
            exits: vec![],
        },
        Case {
            slippage: 0.0,
            name: "Gap Continuation",
            inputs: vec![
                ("profit_ticks", Length(100)),
                ("loss_ticks", Length(100)),
                ("tick_size", Number(1.)),
            ],
            prices: points(&[10., 11., 12., 13., 13., 11., 10.]),
            expected: vec![(2, Buy, 12.), (3, Buy, 13.), (6, Sell, 10.)],
            exits: vec![(0, 6, 10.)],
        },
        // The old long stops at 11 before the queued long fills at 11. Its own stop
        // at 10 must then protect it later in the same execution bar.
        Case {
            slippage: 0.0,
            name: "Gap Continuation",
            inputs: vec![
                ("profit_ticks", Length(100)),
                ("loss_ticks", Length(1)),
                ("tick_size", Number(1.)),
            ],
            prices: vec![[10.; 4], [11.; 4], [12.; 4], [11., 11., 9., 11.]],
            expected: vec![(2, Buy, 12.), (3, Buy, 11.)],
            exits: vec![(0, 3, 11.), (1, 3, 10.)],
        },
        // An old long stops at 11 before a queued short fills. The pending short's
        // bracket must survive the flat position, then stop that short at 12.
        Case {
            slippage: 0.0,
            name: "Gap Continuation",
            inputs: vec![
                ("profit_ticks", Length(100)),
                ("loss_ticks", Length(1)),
                ("tick_size", Number(1.)),
            ],
            prices: vec![
                [10.; 4],
                [11., 13., 11., 13.],
                [12., 14., 12., 14.],
                [11., 13., 11., 11.],
            ],
            expected: vec![(2, Buy, 12.), (3, Sell, 11.)],
            exits: vec![(0, 3, 11.), (1, 3, 12.)],
        },
        // A previous bracket stops out at the open before a queued entry fills.
        // Each queued entry still gets its own side's bracket, including the reversal.
        Case {
            slippage: 0.0,
            name: "Gap Continuation",
            inputs: vec![
                ("profit_ticks", Length(100)),
                ("loss_ticks", Length(1)),
                ("tick_size", Number(1.)),
            ],
            prices: vec![
                [10.; 4],
                [11.; 4],
                [12.; 4],
                [11.; 4],
                [10.; 4],
                [10., 11., 10., 10.],
            ],
            expected: vec![(2, Buy, 12.), (3, Buy, 11.), (4, Sell, 10.), (5, Sell, 10.)],
            exits: vec![(0, 3, 11.), (1, 4, 10.), (2, 5, 11.), (3, 5, 11.)],
        },
        // Pyramided fills at 12 and 13 average 12.5; four half-unit ticks target 14.5.
        Case {
            slippage: 0.0,
            name: "Gap Continuation",
            inputs: vec![
                ("profit_ticks", Length(4)),
                ("loss_ticks", Length(100)),
                ("tick_size", Number(0.5)),
            ],
            prices: vec![[10.; 4], [11.; 4], [12.; 4], [13., 14.5, 13., 13.]],
            expected: vec![(2, Buy, 12.), (3, Buy, 13.)],
            exits: vec![(0, 3, 14.5), (1, 3, 14.5)],
        },
        // Half a unit of entry slippage moves the target by the same amount.
        Case {
            slippage: 0.5,
            name: "Inside Bar",
            inputs: vec![
                ("profit_ticks", Length(2)),
                ("loss_ticks", Length(2)),
                ("tick_size", Number(0.5)),
            ],
            prices: vec![
                [10., 15., 5., 10.],
                [10., 12., 8., 11.],
                [20., 22., 20., 21.],
            ],
            expected: vec![(2, Buy, 20.5)],
            exits: vec![(0, 2, 21.5)],
        },
        // The entry gaps to 20. Two half-unit ticks take profit at 21, on the fill bar.
        Case {
            slippage: 0.0,
            name: "Inside Bar",
            inputs: vec![
                ("profit_ticks", Length(2)),
                ("loss_ticks", Length(2)),
                ("tick_size", Number(0.5)),
            ],
            prices: vec![
                [10., 15., 5., 10.],
                [10., 12., 8., 11.],
                [20., 22., 19.5, 21.],
            ],
            expected: vec![(2, Buy, 20.)],
            exits: vec![(0, 2, 21.)],
        },
        Case {
            slippage: 0.0,
            name: "Inside Bar",
            inputs: vec![
                ("profit_ticks", Length(2)),
                ("loss_ticks", Length(2)),
                ("tick_size", Number(0.5)),
            ],
            prices: vec![
                [10., 15., 5., 10.],
                [10., 12., 8., 9.],
                [20., 20., 18., 19.],
            ],
            expected: vec![(2, Sell, 20.)],
            exits: vec![(0, 2, 19.)],
        },
        Case {
            slippage: 0.0,
            name: "Outside Bar",
            inputs: vec![
                ("profit_ticks", Length(2)),
                ("loss_ticks", Length(2)),
                ("tick_size", Number(0.5)),
            ],
            prices: vec![
                [10., 12., 8., 10.],
                [10., 15., 5., 11.],
                [20., 22., 19.5, 21.],
            ],
            expected: vec![(2, Buy, 20.)],
            exits: vec![(0, 2, 21.)],
        },
        Case {
            slippage: 0.0,
            name: "Channel Breakout",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 9.]),
            expected: vec![(2, Buy, 12.), (3, Sell, 9.)],
            exits: vec![(0, 3, 9.)],
        },
        Case {
            slippage: 0.0,
            name: "Price Channel Break",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 9., 8.]),
            expected: vec![(3, Buy, 9.), (4, Sell, 8.)],
            exits: vec![(0, 4, 8.)],
        },
        Case {
            slippage: 0.0,
            name: "Pivot Reversal",
            inputs: vec![("left", Length(1)), ("right", Length(1))],
            prices: points(&[10., 12., 11., 13., 9., 8.]),
            // The retained low at 11 places another short stop, which gaps to 8 on bar 5.
            expected: vec![(3, Buy, 13.), (4, Sell, 9.), (5, Sell, 8.)],
            exits: vec![(0, 4, 9.)],
        },
        Case {
            slippage: 0.0,
            name: "Pivot Extension",
            inputs: vec![("left", Length(1)), ("right", Length(1))],
            prices: points(&[10., 12., 11., 13., 9., 8.]),
            expected: vec![(3, Sell, 13.), (4, Buy, 9.), (5, Sell, 8.)],
            exits: vec![(0, 4, 9.)],
        },
        // True ranges 0,2,3,4 give Wilder ATR(2) = missing,1,2,3.
        // Stops at 12 +/- 1 gap to 15; the next short stop is 15 - 2 = 13.
        Case {
            slippage: 0.0,
            name: "Volatility Expansion Close",
            inputs: vec![("length", Length(2)), ("multiplier", Number(1.))],
            prices: points(&[10., 12., 15., 11.]),
            expected: vec![(2, Buy, 15.), (3, Sell, 11.)],
            exits: vec![(0, 3, 11.)],
        },
    ];
    for case in cases {
        let bars = bars(&case.prices);
        let mut backtest = Backtest::new(case.name, "TEST", Timeframe::Min1, 10_000.);
        backtest.settings.pyramiding = 2;
        backtest.settings.slippage = crate::strategies::Slippage::Ticks {
            ticks: 1,
            tick_size: case.slippage,
        };
        let mut strategy = configured(case.name, &case.inputs);
        let report = backtest.run(strategy.as_mut(), &bars, &[]);
        assert_eq!(entries(&report), case.expected, "{}", case.name);
        for (trade, bar, price) in case.exits {
            let exit = report.trades[trade].exit.as_ref().unwrap();
            assert_eq!((exit.time, exit.price), (time(bar), price), "{}", case.name);
        }
        assert_prefixes(case.name, &case.inputs, &bars, &backtest);
        // A reused instance and fill callbacks must not duplicate a closed-bar signal.
        backtest.settings.recalculate_after_fills = true;
        let rerun = backtest.run(strategy.as_mut(), &bars, &[]);
        assert_eq!(
            entries(&rerun),
            case.expected,
            "{} with fill callbacks",
            case.name
        );
    }
}

#[test]
fn catalogue_accepts_host_strategies_and_validates_typed_inputs() {
    struct Host;
    impl Strategy for Host {
        fn on_bar(&mut self, _: &mut StrategyContext<'_>) {}
    }
    let mut factory = StrategyFactory::new();
    let builtins = if cfg!(feature = "studies") { 22 } else { 10 };
    assert_eq!(factory.list().len(), builtins);
    assert!(factory.list().windows(2).all(|names| names[0] < names[1]));
    assert!(factory.create("Unknown").is_none());
    factory.register("Host", || Box::new(Host));
    assert!(factory.create("Host").is_some());
    factory.register("Host", || Box::new(Host));
    assert_eq!(factory.list().len(), builtins + 1);
    let mut strategy = factory.create("Consecutive Up/Down").unwrap();
    for invalid in [
        InputValue::Length(0),
        InputValue::Number(3.),
        InputValue::Number(f64::NAN),
    ] {
        assert!(strategy.set_input("length", invalid).is_err());
    }
    assert!(
        strategy
            .set_input("unknown", InputValue::Length(2))
            .is_err()
    );
    strategy.set_input("length", InputValue::Length(2)).unwrap();
    let input = strategy.study_inputs().remove(0);
    assert_eq!(input.default, InputValue::Length(3));
    assert_eq!(input.value, InputValue::Length(2));
    assert_eq!(
        strategy.inputs(),
        vec![("Consecutive closes".into(), "2".into())]
    );
    let mut bracket = factory.create("Inside Bar").unwrap();
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        assert!(
            bracket
                .set_input("tick_size", InputValue::Number(invalid))
                .is_err()
        );
    }
    assert_eq!(bracket.study_inputs()[2].value, InputValue::Number(0.01));
    assert!(input.accepts(&input.value));
    assert_eq!(
        factory
            .create("Consecutive Up/Down")
            .unwrap()
            .study_inputs()[0]
            .value,
        InputValue::Length(3)
    );
}

#[cfg(feature = "studies")]
#[test]
fn study_entries_match_hand_placed_crossings_without_lookahead() {
    use InputValue::{Choice, Length, Number, Toggle};
    use OrderSide::{Buy, Sell};
    struct Case {
        name: &'static str,
        prices: Vec<[f64; 4]>,
        inputs: Vec<(&'static str, InputValue)>,
        expected: Vec<Entry>,
    }
    let closes = || points(&[10., 8., 10., 12., 10., 8., 10., 10.]);
    let bb = || {
        vec![
            [10.; 4],
            [8.; 4],
            [10.; 4],
            [8., 12., 8., 12.],
            [10.; 4],
            [12., 12., 8., 8.],
            [10.; 4],
            [8., 10., 8., 10.],
        ]
    };
    let cases = [
        // SMA(2) +/- 0.5 population deviations gives bands 8.5/9.5 at bars 1,2;
        // 10.5/11.5 at bars 3,4; 8.5/9.5 at bars 5,6.
        Case {
            name: "Bollinger Bands",
            prices: bb(),
            inputs: vec![("period", Length(2)), ("std_dev", Number(0.5))],
            expected: vec![(3, Buy, 8.5), (5, Sell, 11.5), (7, Buy, 8.5)],
        },
        Case {
            name: "Bollinger Bands Directed",
            prices: bb(),
            inputs: vec![("period", Length(2)), ("std_dev", Number(0.5))],
            expected: vec![(3, Buy, 8.5), (7, Buy, 8.5)],
        },
        Case {
            name: "Bollinger Bands Directed",
            prices: bb(),
            inputs: vec![
                ("period", Length(2)),
                ("std_dev", Number(0.5)),
                ("direction", Choice(1)),
            ],
            expected: vec![(5, Sell, 11.5)],
        },
        // Simple basis(2), ATR(1), multiplier 1/4: upper[2]=9.5, lower[4]=10.5,
        // upper[6]=9.5. Each following bar starts before the stop and reaches it.
        Case {
            name: "Keltner Channels",
            prices: vec![
                [10.; 4],
                [8.; 4],
                [10.; 4],
                [9., 12., 9., 12.],
                [10.; 4],
                [11., 11., 8., 8.],
                [10.; 4],
                [9., 10., 9., 10.],
            ],
            inputs: vec![
                ("ema_period", Length(2)),
                ("atr_period", Length(1)),
                ("multiplier", Number(0.25)),
                ("average", Choice(1)),
            ],
            expected: vec![(3, Buy, 9.5), (5, Sell, 10.5), (7, Buy, 9.5)],
        },
        // SMA(1)-SMA(2), SMA signal(2): histogram[2..=7] = -0.5,1,0,-1,0,1.
        Case {
            name: "MACD",
            prices: points(&[10., 10., 8., 10., 12., 10., 8., 10., 10.]),
            inputs: vec![
                ("fast_period", Length(1)),
                ("slow_period", Length(2)),
                ("signal_period", Length(2)),
                ("oscillator_average", Choice(1)),
                ("signal_average", Choice(1)),
            ],
            expected: vec![(4, Buy, 12.), (6, Sell, 8.), (8, Buy, 10.)],
        },
        // Momentum(1) = missing,-1,1,2,-1,-3,2,0. Its change agrees at 2,3,4,5,6.
        Case {
            name: "Momentum",
            prices: points(&[10., 9., 10., 12., 11., 8., 10., 10.]),
            inputs: vec![("period", Length(1))],
            expected: vec![(3, Buy, 12.), (5, Sell, 8.), (7, Buy, 10.)],
        },
        // Close - SMA(2) at 1..=7 = -1,1,1,-1,-1,1,0.
        Case {
            name: "Price/Moving Average Cross",
            prices: closes(),
            inputs: vec![("period", Length(2))],
            expected: vec![(3, Buy, 12.), (5, Sell, 8.), (7, Buy, 10.)],
        },
        Case {
            name: "Two Moving Average Cross",
            prices: closes(),
            inputs: vec![("fast.period", Length(1)), ("slow.period", Length(2))],
            expected: vec![(3, Buy, 12.), (5, Sell, 8.), (7, Buy, 10.)],
        },
        // The initial up SAR is 10. At reversals 2,3,4,5 the previous extremes
        // become SAR: 12,8,14,7. The next opens are already through those stops.
        Case {
            name: "Parabolic SAR",
            prices: vec![
                [10.; 4],
                [12.; 4],
                [8.; 4],
                [11., 14., 11., 14.],
                [9., 9., 7., 7.],
                [13., 15., 13., 15.],
                [15.; 4],
            ],
            inputs: vec![],
            expected: vec![(3, Sell, 11.), (4, Buy, 9.), (5, Sell, 13.), (6, Buy, 15.)],
        },
        // RSI(1) at 1..=7 = 0,100,100,0,0,100,50: leaves its zones at 2,4,6,7.
        Case {
            name: "RSI",
            prices: closes(),
            inputs: vec![("period", Length(1))],
            expected: vec![(3, Buy, 12.), (5, Sell, 8.), (7, Buy, 10.)],
        },
        // Range 0..100 makes K the close. K/D at 1,2 = 5/7.5,10/7.5;
        // at 4,5 = 95/92.5,90/92.5. Both crossings are inside their zones.
        Case {
            name: "Stochastic Slow",
            prices: [10., 5., 10., 90., 95., 90., 10., 10.]
                .into_iter()
                .map(|c| [c, 100., 0., c])
                .collect(),
            inputs: vec![
                ("k_period", Length(1)),
                ("k_smooth", Length(1)),
                ("d_period", Length(2)),
            ],
            expected: vec![(3, Buy, 90.), (6, Sell, 10.)],
        },
        // ATR(1), multiplier 1: the active lines at 1..=5 are up10,down12,up8,down14,up7.
        Case {
            name: "SuperTrend",
            prices: points(&[10., 12., 8., 14., 7., 15., 15.]),
            inputs: vec![("atr_period", Length(1)), ("multiplier", Number(1.))],
            expected: vec![(3, Sell, 14.), (4, Buy, 7.), (5, Sell, 15.), (6, Buy, 15.)],
        },
        // Only SMA lengths 1,2,2 vote: scores 0,-2/3,+2/3,+2/3,-2/3,-2/3,+2/3,0.
        Case {
            name: "Technical Ratings",
            prices: closes(),
            inputs: vec![
                ("fast", Length(1)),
                ("medium", Length(2)),
                ("slow", Length(2)),
                ("exponential", Toggle(false)),
                ("rsi", Toggle(false)),
                ("stochastic", Toggle(false)),
                ("cci", Toggle(false)),
                ("macd", Toggle(false)),
                ("momentum", Toggle(false)),
            ],
            expected: vec![(2, Sell, 10.), (3, Buy, 12.), (5, Sell, 8.), (7, Buy, 10.)],
        },
    ];
    for case in cases {
        let bars = bars(&case.prices);
        let mut backtest = Backtest::new(case.name, "TEST", Timeframe::Min1, 10_000.);
        let mut strategy = configured(case.name, &case.inputs);
        let report = backtest.run(strategy.as_mut(), &bars, &[]);
        assert_eq!(entries(&report), case.expected, "{}", case.name);
        assert_prefixes(case.name, &case.inputs, &bars, &backtest);
        backtest.settings.recalculate_after_fills = true;
        let rerun = backtest.run(strategy.as_mut(), &bars, &[]);
        assert_eq!(entries(&rerun), case.expected, "{} reused", case.name);
    }
}

#[test]
fn every_default_strategy_is_causal_and_declares_valid_defaults() {
    let bars: Vec<_> = (0..80)
        .map(|i| {
            let close = 100. + (i as f64 * 0.4).sin() * 8.;
            let open = close + (i as f64 * 0.9).cos();
            Bar::new(
                time(i),
                open,
                close.max(open) + 1.,
                close.min(open) - 1.,
                close,
                1.,
            )
        })
        .collect();
    let backtest = Backtest::new("Prefix", "TEST", Timeframe::Min1, 10_000.);
    let mut gapped = bars.clone();
    for bar in &mut gapped[..20] {
        bar.close = f64::NAN;
    }
    for name in StrategyFactory::new().list() {
        let mut strategy = configured(name, &[]);
        for input in strategy.study_inputs() {
            assert!(input.accepts(&input.value), "{name}: {}", input.id);
            assert!(
                input.accepts(&input.default),
                "{name}: {} default",
                input.id
            );
            strategy.set_input(&input.id, input.value).unwrap();
        }
        assert_prefixes(name, &[], &bars, &backtest);
        assert_prefixes(name, &[], &gapped, &backtest);
    }
}
