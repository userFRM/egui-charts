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
        let last = (bars[end - 1].time - time(0)).num_minutes() as usize;
        let expected: Vec<_> = entries(&full)
            .into_iter()
            .filter(|entry| entry.0 <= last)
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
    use crate::model::{BarClock, Session, SessionWindow, TimezoneId};
    use InputValue::{Length, Number};
    use OrderSide::{Buy, Sell};
    use chrono::{NaiveTime, Weekday::*};
    struct Case {
        name: &'static str,
        slippage: f64,
        clock: Option<BarClock>,
        first_minute: i64,
        prices: Vec<[f64; 4]>,
        inputs: Vec<(&'static str, InputValue)>,
        expected: Vec<Entry>,
        exits: Vec<(usize, usize, f64)>,
    }
    let cases = [
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
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
        // From 23:56: the long bought at 102 is marked at 90, a loss of 12 against the day's
        // limit 0.1% of 10,000 = 10, so it closes at the next open, 91, and the up bar at 23:59
        // adds nothing. The up bar at midnight starts a new day and buys at the next open.
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 1436,
            name: "Bar Up/Down",
            inputs: vec![("max_intraday_loss", Number(0.1))],
            prices: vec![
                [100.; 4],
                [101., 102., 101., 102.],
                [102., 102., 90., 90.],
                [91., 95., 91., 94.],
                [95., 97., 95., 96.],
                [96.; 4],
            ],
            expected: vec![(1438, Buy, 102.), (1441, Buy, 96.)],
            exits: vec![(0, 1439, 91.)],
        },
        // On a weekday session of a calendar 3 minutes behind UTC, Thursday's trading day runs to
        // 00:03 UTC on Friday. The loss at 23:58 UTC keeps the up bars at 23:59 and 00:00 UTC
        // flat; Friday's session opens at 00:03 UTC and its first up bar buys at the next open.
        Case {
            slippage: 0.0,
            clock: Some(BarClock::new(Timeframe::Min1).with_session(
                TimezoneId::fixed_offset(-180).unwrap(),
                Session::new([SessionWindow::new(
                    [Mon, Tue, Wed, Thu, Fri],
                    NaiveTime::MIN,
                    NaiveTime::from_hms_opt(23, 59, 0).unwrap(),
                )]),
                false,
            )),
            first_minute: 1436,
            name: "Bar Up/Down",
            inputs: vec![("max_intraday_loss", Number(0.1))],
            prices: vec![
                [100.; 4],
                [101., 102., 101., 102.],
                [102., 102., 90., 90.],
                [91., 95., 91., 94.],
                [95., 97., 95., 96.],
                [96.; 4],
                [97.; 4],
                [98., 99., 98., 99.],
                [99.; 4],
            ],
            expected: vec![(1438, Buy, 102.), (1444, Buy, 99.)],
            exits: vec![(0, 1439, 91.)],
        },
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
            name: "Consecutive Up/Down",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 11., 10., 10.]),
            expected: vec![(3, Buy, 11.), (5, Sell, 10.)],
            exits: vec![(0, 5, 10.)],
        },
        // Repeated qualifying closes honour the engine's two-entry pyramiding setting.
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
            name: "Consecutive Up/Down",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 13., 13.]),
            expected: vec![(3, Buy, 13.), (4, Buy, 13.)],
            exits: vec![],
        },
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
            name: "Channel Breakout",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 9.]),
            expected: vec![(2, Buy, 12.), (3, Sell, 9.)],
            exits: vec![(0, 3, 9.)],
        },
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
            name: "Price Channel Break",
            inputs: vec![("length", Length(2))],
            prices: points(&[10., 11., 12., 9., 8.]),
            expected: vec![(3, Buy, 9.), (4, Sell, 8.)],
            exits: vec![(0, 4, 8.)],
        },
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
            name: "Pivot Reversal",
            inputs: vec![("left", Length(1)), ("right", Length(1))],
            prices: points(&[10., 12., 11., 13., 9., 8.]),
            // The retained low at 11 places another short stop, which gaps to 8 on bar 5.
            expected: vec![(3, Buy, 13.), (4, Sell, 9.), (5, Sell, 8.)],
            exits: vec![(0, 4, 9.)],
        },
        Case {
            slippage: 0.0,
            clock: None,
            first_minute: 0,
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
            clock: None,
            first_minute: 0,
            name: "Volatility Expansion Close",
            inputs: vec![("length", Length(2)), ("multiplier", Number(1.))],
            prices: points(&[10., 12., 15., 11.]),
            expected: vec![(2, Buy, 15.), (3, Sell, 11.)],
            exits: vec![(0, 3, 11.)],
        },
    ];
    for case in cases {
        let mut bars = bars(&case.prices);
        for bar in &mut bars {
            bar.time += TimeDelta::minutes(case.first_minute);
        }
        let mut backtest = Backtest::new(case.name, "TEST", Timeframe::Min1, 10_000.);
        backtest.bar_clock = case.clock;
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
    let gap = || {
        [
            points(&[13., 12., 11., 10., 11., 10.]),
            vec![[10., 11., 10., 11.]],
        ]
        .concat()
    };
    let keltner = || {
        vec![
            [10.; 4],
            [8.; 4],
            [10.; 4],
            [9., 12., 9., 12.],
            [10.; 4],
            [11., 11., 8., 8.],
            [10.; 4],
            [9., 11., 9., 10.],
        ]
    };
    let keltner_inputs = || {
        vec![
            ("ema_period", Length(2)),
            ("atr_period", Length(1)),
            ("multiplier", Number(0.25)),
            ("average", Choice(1)),
        ]
    };
    let cases = [
        // SMA(2) +/- 0.5 population deviations gives bands 8.5/9.5 at bars 1,2;
        // 10.5/11.5 at bars 3,4; 8.5/9.5 at bars 5,6. Each entry fills at the next open, beyond
        // the band at bars 3, 5 and 7.
        Case {
            name: "Bollinger Bands",
            prices: bb(),
            inputs: vec![("period", Length(2)), ("std_dev", Number(0.5))],
            expected: vec![(3, Buy, 8.), (5, Sell, 12.), (7, Buy, 8.)],
        },
        Case {
            name: "Bollinger Bands Directed",
            prices: bb(),
            inputs: vec![
                ("period", Length(2)),
                ("std_dev", Number(0.5)),
                ("direction", Choice(0)),
            ],
            expected: vec![(3, Buy, 8.), (7, Buy, 8.)],
        },
        // Both, the default, takes the undirected strategy's entries on the same bars.
        Case {
            name: "Bollinger Bands Directed",
            prices: bb(),
            inputs: vec![("period", Length(2)), ("std_dev", Number(0.5))],
            expected: vec![(3, Buy, 8.), (5, Sell, 12.), (7, Buy, 8.)],
        },
        Case {
            name: "Bollinger Bands Directed",
            prices: bb(),
            inputs: vec![
                ("period", Length(2)),
                ("std_dev", Number(0.5)),
                ("direction", Choice(1)),
            ],
            expected: vec![(5, Sell, 12.)],
        },
        // SMA(3) +/- 1 population deviation: at bar 4 the close 11 crosses back above the
        // lower band (32 - sqrt 2) / 3 = 10.195. Bar 5 opens at 10, beyond the band, and never
        // reaches it; the entry fills at that open. No later bar crosses.
        Case {
            name: "Bollinger Bands",
            prices: gap(),
            inputs: vec![("period", Length(3)), ("std_dev", Number(1.))],
            expected: vec![(5, Buy, 10.)],
        },
        Case {
            name: "Bollinger Bands Directed",
            prices: gap(),
            inputs: vec![("period", Length(3)), ("std_dev", Number(1.))],
            expected: vec![(5, Buy, 10.)],
        },
        Case {
            name: "Bollinger Bands Directed",
            prices: gap(),
            inputs: vec![
                ("period", Length(3)),
                ("std_dev", Number(1.)),
                ("direction", Choice(0)),
            ],
            expected: vec![(5, Buy, 10.)],
        },
        // The same bars mirrored around 10: bar 5 opens at 10 above the short band
        // (28 + sqrt 2) / 3 = 9.805 and fills there.
        Case {
            name: "Bollinger Bands Directed",
            prices: [
                points(&[7., 8., 9., 10., 9., 10.]),
                vec![[10., 10., 9., 9.]],
            ]
            .concat(),
            inputs: vec![
                ("period", Length(3)),
                ("std_dev", Number(1.)),
                ("direction", Choice(1)),
            ],
            expected: vec![(5, Sell, 10.)],
        },
        // Simple basis(2), ATR(1), multiplier 1/4: the close crosses upper[2]=9.5, lower[4]=10.5
        // and upper[6]=9.5. The stops wait one quarter increment beyond those bars, at 10.25,
        // 9.75 and 10.25; each following bar starts before its stop and reaches it.
        Case {
            name: "Keltner Channels",
            prices: keltner(),
            inputs: keltner_inputs(),
            expected: vec![(3, Buy, 10.25), (5, Sell, 9.75), (7, Buy, 10.25)],
        },
        // The close 10 crosses the upper band 9.5 at bar 2. Bar 3 misses the stop 10.25 with a
        // high of 9.45 and closes at 9.4, under the basis 9.7; bar 4 would reach the stop.
        Case {
            name: "Keltner Channels",
            prices: vec![
                [10.; 4],
                [8.; 4],
                [10.; 4],
                [9.4, 9.45, 8., 9.4],
                [9.4, 10.5, 9.3, 9.5],
            ],
            inputs: keltner_inputs(),
            expected: vec![],
        },
        // Simple basis(3), ATR(1), multiplier 1/4: close 10 crosses the upper band
        // 26/3 + 0.5 at bar 3, so the stop waits at 10.25. Bar 4 misses it and closes at 9.1,
        // above its basis 27.1/3; bar 5 reaches the original stop.
        Case {
            name: "Keltner Channels",
            prices: vec![
                [8.; 4],
                [8.; 4],
                [8.; 4],
                [10.; 4],
                [9.1, 9.15, 9.05, 9.1],
                [9.1, 10.5, 9.1, 9.2],
            ],
            inputs: vec![
                ("ema_period", Length(3)),
                ("atr_period", Length(1)),
                ("multiplier", Number(0.25)),
                ("average", Choice(1)),
            ],
            expected: vec![(5, Buy, 10.25)],
        },
        // Source Open: the basis averages the opens. Bar 2 opens at 10 above its upper band
        // 9.5 and closes at 9 below it, so its stop waits at 10.25. Bar 3 opens at 9.6, under
        // its basis 9.8, and cancels the stop while its close 10 stays above; bar 4 would
        // reach it. Bar 5 opens at 10.5 above its upper band 10.3375 and closes at 10 below
        // it; bar 6 reaches its stop 10.75.
        Case {
            name: "Keltner Channels",
            prices: vec![
                [10.; 4],
                [8.; 4],
                [10., 10., 9., 9.],
                [9.6, 10., 9.6, 10.],
                [9.8, 10.5, 9.7, 9.8],
                [10.5, 10.5, 9.75, 10.],
                [10.25, 11., 10.25, 11.],
            ],
            inputs: [keltner_inputs(), vec![("source", Choice(0))]].concat(),
            expected: vec![(6, Buy, 10.75)],
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
        // A saved state may still set the study's smoothing and divergence; no trade changes.
        Case {
            name: "RSI",
            prices: closes(),
            inputs: vec![
                ("period", Length(1)),
                ("smoothing", Choice(2)),
                ("smoothing_length", Length(3)),
                ("smoothing_deviation", Number(1.)),
                ("divergence", Toggle(true)),
            ],
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
        // ATR(1), multiplier 1: the active lines at 0..=5 are down10,up10,down12,up8,down14,up7.
        Case {
            name: "SuperTrend",
            prices: points(&[10., 12., 8., 14., 7., 15., 15.]),
            inputs: vec![("atr_period", Length(1)), ("multiplier", Number(1.))],
            expected: vec![
                (2, Buy, 8.),
                (3, Sell, 14.),
                (4, Buy, 7.),
                (5, Sell, 15.),
                (6, Buy, 15.),
            ],
        },
        // Only the six EMAs vote: scores 0, -1, +1, +1, -1, -1, +1, +1. Every bar after 0 is
        // strong; the repeats at 3 and 5 add nothing at pyramiding 1.
        Case {
            name: "Technical Ratings",
            prices: closes(),
            inputs: vec![],
            expected: vec![(2, Sell, 10.), (3, Buy, 12.), (5, Sell, 8.), (7, Buy, 10.)],
        },
        // Each EMA votes by the close against its previous value: summaries 0, -1, 1/3, 2/3,
        // -2/3, -2/3. Only -1, 2/3 and -2/3 are strong; each fills at the next open.
        Case {
            name: "Technical Ratings",
            prices: points(&[10., 8., 9.94, 9.97, 9.8, 9.8]),
            inputs: vec![],
            expected: vec![(2, Sell, 9.94), (4, Buy, 9.8), (5, Sell, 9.8)],
        },
    ];
    for case in cases {
        let bars = bars(&case.prices);
        let mut backtest = Backtest::new(case.name, "TEST", Timeframe::Min1, 10_000.);
        // Only the Keltner stops read the increment; a quarter keeps their prices exact.
        backtest.settings.price_increment = 0.25;
        let mut strategy = configured(case.name, &case.inputs);
        let report = backtest.run(strategy.as_mut(), &bars, &[]);
        assert_eq!(entries(&report), case.expected, "{}", case.name);
        assert_prefixes(case.name, &case.inputs, &bars, &backtest);
        backtest.settings.recalculate_after_fills = true;
        let rerun = backtest.run(strategy.as_mut(), &bars, &[]);
        assert_eq!(entries(&rerun), case.expected, "{} reused", case.name);
    }
    // Without a usable increment the stops wait at the crossing bar's high or low. At 0.01, the
    // stops one tick beyond the high 1.12 and the low 1.13 are 1.13 and 1.12 exactly, which the
    // next bars reach.
    for (increment, prices, expected) in [
        (
            f64::INFINITY,
            keltner(),
            vec![(3, Buy, 10.), (5, Sell, 10.), (7, Buy, 10.)],
        ),
        (
            -0.25,
            keltner(),
            vec![(3, Buy, 10.), (5, Sell, 10.), (7, Buy, 10.)],
        ),
        (
            0.01,
            vec![
                [1.1; 4],
                [1.08; 4],
                [1.1, 1.12, 1.1, 1.12],
                [1.12, 1.13, 1.12, 1.13],
                [1.15; 4],
                [1.15, 1.15, 1.13, 1.13],
                [1.13, 1.13, 1.12, 1.12],
            ],
            vec![(3, Buy, 1.13), (6, Sell, 1.12)],
        ),
    ] {
        let mut backtest = Backtest::new("Keltner Channels", "TEST", Timeframe::Min1, 10_000.);
        backtest.settings.price_increment = increment;
        let mut strategy = configured("Keltner Channels", &keltner_inputs());
        let report = backtest.run(strategy.as_mut(), &bars(&prices), &[]);
        assert_eq!(entries(&report), expected, "increment {increment}");
    }
    let seconds = |count| Timeframe::Custom(std::num::NonZeroU64::new(count).unwrap());
    // Other Indicator timeframes rate the instrument's own bars of that interval, each from the
    // chart bar whose close ends it. Hourly bars trade 09:30 to 16:00 UTC on weekdays.
    {
        use crate::model::{BarClock, Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday::*};
        type IntervalRow = (
            &'static str,
            Timeframe,
            Timeframe,
            Option<BarClock>,
            Vec<Bar>,
            Vec<Bar>,
            Vec<Entry>,
        );
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        let priced = |times: Vec<DateTime<Utc>>, prices: Vec<[f64; 4]>| -> Vec<Bar> {
            (times.into_iter().zip(prices))
                .map(|(time, [open, high, low, close])| Bar::new(time, open, high, low, close, 1.0))
                .collect()
        };
        let closes = |closes: &[f64]| closes.iter().map(|&close| [close; 4]).collect::<Vec<_>>();
        let hours = |day: u32, count: i64| -> Vec<DateTime<Utc>> {
            (0..count)
                .map(|hour| at(&format!("2026-01-{day:02}T09:30:00Z")) + TimeDelta::hours(hour))
                .collect()
        };
        let minutes = |minute: i64| time(0) + TimeDelta::minutes(minute);
        let hm = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let days = [Mon, Tue, Wed, Thu, Fri];
        let session = Session::new([SessionWindow::new(days, hm(9, 30), hm(16, 0))]);
        let clock = BarClock::new(Timeframe::Hour1).with_session(TimezoneId::utc(), session, false);
        let rows: Vec<IntervalRow> = vec![
            // 5-minute slots first become readable at chart bars 4, 9, 14, 19.
            (
                "5 minutes on 1 minute",
                Timeframe::Min1,
                Timeframe::Min5,
                None,
                bars(&points(
                    &[vec![10.; 5], vec![8.; 5], vec![10.; 5], vec![12.; 6]].concat(),
                )),
                priced(
                    (0..4).map(|slot| minutes(5 * slot)).collect(),
                    closes(&[10., 8., 10., 12.]),
                ),
                vec![(10, Sell, 10.), (15, Buy, 12.)],
            ),
            // The 09:30 bars close at 13:30 (read from 12:30), the 13:30 bars at 16:00 (read
            // from 15:30): the 05 13:30 rating sells from 15:30, the 06 09:30 one buys from 12:30.
            (
                "4 hours on session hours",
                Timeframe::Hour1,
                Timeframe::Hour4,
                Some(clock.clone()),
                priced(
                    [hours(5, 7), hours(6, 7)].concat(),
                    closes(&[vec![10.; 4], vec![8.; 3], vec![10.; 4], vec![12.; 3]].concat()),
                ),
                priced(
                    [
                        "2026-01-05T09:30:00Z",
                        "2026-01-05T13:30:00Z",
                        "2026-01-06T09:30:00Z",
                        "2026-01-06T13:30:00Z",
                    ]
                    .map(at)
                    .to_vec(),
                    closes(&[10., 8., 10., 12.]),
                ),
                vec![(7, Sell, 10.), (11, Buy, 12.)],
            ),
            // Day 06 sells from its last hour, 15:30; day 07 has not closed and is never read.
            (
                "1 day on session hours",
                Timeframe::Hour1,
                Timeframe::Day1,
                Some(clock),
                priced(
                    [hours(5, 7), hours(6, 7), hours(7, 2)].concat(),
                    closes(&[vec![10.; 7], vec![8.; 7], vec![12.; 2]].concat()),
                ),
                priced(
                    [
                        "2026-01-05T00:00:00Z",
                        "2026-01-06T00:00:00Z",
                        "2026-01-07T00:00:00Z",
                    ]
                    .map(at)
                    .to_vec(),
                    closes(&[10., 8., 12.]),
                ),
                vec![(14, Sell, 12.)],
            ),
            // A month is read from the week that ends after it: January from 26 January,
            // February from 23 February, March from 30 March.
            (
                "1 month on 1 week",
                Timeframe::Week1,
                Timeframe::Month1,
                None,
                priced(
                    (0..14)
                        .map(|week| at("2026-01-05T00:00:00Z") + TimeDelta::weeks(week))
                        .collect(),
                    closes(&[vec![10.; 4], vec![8.; 4], vec![10.; 5], vec![12.; 1]].concat()),
                ),
                priced(
                    [
                        "2026-01-01T00:00:00Z",
                        "2026-02-01T00:00:00Z",
                        "2026-03-01T00:00:00Z",
                    ]
                    .map(at)
                    .to_vec(),
                    closes(&[10., 8., 10.]),
                ),
                vec![(8, Sell, 10.), (13, Buy, 12.)],
            ),
            // 00:05-00:10 closes inside the 00:09 bar, 00:10-00:15 with the 00:12 bar.
            (
                "5 minutes on 3 minutes",
                seconds(180),
                Timeframe::Min5,
                None,
                priced(
                    (0..7).map(|bar| minutes(3 * bar)).collect(),
                    vec![
                        [10.; 4],
                        [10., 10., 8., 8.],
                        [8.; 4],
                        [8., 10., 8., 10.],
                        [10.; 4],
                        [10., 12., 10., 12.],
                        [12.; 4],
                    ],
                ),
                priced(
                    (0..4).map(|slot| minutes(5 * slot)).collect(),
                    closes(&[10., 8., 10., 12.]),
                ),
                vec![(4, Sell, 10.), (5, Buy, 10.)],
            ),
            // Each minute reads its second half: minute 1 the -1 at 01:30, minute 2 the +1 at 02:30.
            (
                "30 seconds on 1 minute",
                Timeframe::Min1,
                Timeframe::Sec30,
                None,
                bars(&[[10.; 4], [10., 10., 8., 8.], [8., 10., 8., 10.], [12.; 4]]),
                priced(
                    (0..8)
                        .map(|half| time(0) + TimeDelta::seconds(30 * half))
                        .collect(),
                    closes(&[10., 10., 10., 8., 8., 10., 12., 12.]),
                ),
                vec![(2, Sell, 8.), (3, Buy, 12.)],
            ),
        ];
        for (name, chart, interval, clock, bars, calculation, expected) in rows {
            let inputs = [("timeframe", InputValue::Timeframe(Some(interval)))];
            let mut backtest = Backtest::new("Technical Ratings", "TEST", chart, 10_000.);
            backtest.bar_clock = clock;
            backtest.calculation_bars = calculation;
            let report = backtest.run(
                configured("Technical Ratings", &inputs).as_mut(),
                &bars,
                &[],
            );
            let expected: Vec<_> = (expected.into_iter())
                .map(|(at, side, price)| {
                    (
                        (bars[at].time - time(0)).num_minutes() as usize,
                        side,
                        price,
                    )
                })
                .collect();
            assert_eq!(entries(&report), expected, "{name}");
            assert_prefixes("Technical Ratings", &inputs, &bars, &backtest);
            backtest.calculation_bars.clear();
            assert!(
                backtest
                    .run(
                        configured("Technical Ratings", &inputs).as_mut(),
                        &bars,
                        &[]
                    )
                    .trades
                    .is_empty(),
                "{name} must not substitute chart bars for missing calculation history"
            );
        }
    }
    // The histories agree through minute 9's open. Its 5-minute close cannot cause an entry
    // before that minute closes, including when historical fills refresh the calculations.
    for (historical, after_fills) in [(false, false), (true, false), (true, true)] {
        let mut early = Vec::new();
        for last in [8., 10.] {
            let source = bars(
                &[
                    vec![[10.; 4]; 9],
                    vec![[10., 10., last, last]],
                    vec![[last; 4]; 5],
                    vec![[12.; 4]; 6],
                ]
                .concat(),
            );
            let inputs = [("timeframe", InputValue::Timeframe(Some(Timeframe::Min5)))];
            let mut backtest = Backtest::new("Technical Ratings", "TEST", Timeframe::Min1, 10_000.);
            backtest.settings.recalculate_on_history = historical;
            backtest.settings.recalculate_after_fills = after_fills;
            backtest.calculation_bars =
                crate::data::resample(&source, Timeframe::Min1, Timeframe::Min5).unwrap();
            let report = backtest.run(
                configured("Technical Ratings", &inputs).as_mut(),
                &source,
                &[],
            );
            let found = entries(&report);
            early.push(
                found
                    .iter()
                    .copied()
                    .filter(|entry| entry.0 <= 9)
                    .collect::<Vec<_>>(),
            );
            if last == 8. {
                assert_eq!(
                    found.first(),
                    Some(&(10, Sell, 8.)),
                    "historical: {historical}, after fills: {after_fills}"
                );
            }
            backtest.update(
                configured("Technical Ratings", &inputs).as_mut(),
                &source,
                &[],
            );
            let continued = entries(backtest.report().unwrap());
            assert_eq!(
                continued
                    .into_iter()
                    .filter(|entry| entry.0 <= 10)
                    .collect::<Vec<_>>(),
                found
                    .into_iter()
                    .filter(|entry| entry.0 <= 10)
                    .collect::<Vec<_>>(),
                "live replay, historical: {historical}, after fills: {after_fills}"
            );
        }
        assert_eq!(early[0], early[1], "after fills: {after_fills}");
        assert!(early[0].is_empty());
    }
    // The RSI signals read only the RSI line, so its smoothing and divergence are not inputs.
    let ids: Vec<_> = StrategyFactory::new()
        .create("RSI")
        .unwrap()
        .study_inputs()
        .into_iter()
        .map(|input| input.id)
        .collect();
    assert_eq!(ids, ["period", "oversold", "overbought"]);
    let tick = Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap());
    for (chart, interval, valid) in [
        // Other time intervals use their own supplied history.
        (seconds(180), Timeframe::Min5, true),
        (Timeframe::Week1, Timeframe::Month1, true),
        // The last 7-minute bar of a day ends early, at midnight.
        (seconds(420), Timeframe::Day1, true),
        (Timeframe::Day1, Timeframe::Week1, true),
        (Timeframe::Min1, Timeframe::Sec30, true),
        (Timeframe::Min1, Timeframe::Min5, true),
        (Timeframe::Min1, Timeframe::Min1, true),
        (tick, Timeframe::Min1, false),
        (Timeframe::Min1, tick, false),
        (tick, tick, true),
    ] {
        let mut strategy = configured(
            "Technical Ratings",
            &[("timeframe", InputValue::Timeframe(Some(interval)))],
        );
        strategy.set_timeframe(chart);
        assert_eq!(
            strategy.validate_inputs(),
            if valid {
                Ok(())
            } else {
                Err(
                    "indicator timeframe must be the chart interval or a time interval on a time-based chart",
                )
            },
            "{chart} into {interval}",
        );
    }
}

/// ATR(14) is 2 from bar 13: true ranges are all 2, so the distances are 6, 10 and 4.
/// Mirroring prices around 100 gives the same protection for a short position.
#[cfg(feature = "studies")]
#[test]
fn technical_ratings_exits_at_atr_stops() {
    let rising = |count: usize| -> Vec<[f64; 4]> {
        (0..count)
            .map(|i| {
                let close = 100. + 0.5 * i as f64;
                [close - 0.5, close + 1., close - 1., close]
            })
            .collect()
    };
    let backtest = Backtest::new("Technical Ratings", "TEST", Timeframe::Min1, 10_000.);
    for (count, last, exit_price, profit) in [
        // Entry at bar 2's open 100.5; fixed stop 100.5 - 6 = 94.5.
        (14, [106.5, 106.5, 94., 95.], 94.5, -6.),
        // Bar 19 reaches activation 110.5 exactly. Best 111 at bar 20 gives stop 107.
        (21, [110., 110., 100., 101.], 107., 6.5),
    ] {
        for short in [false, true] {
            let mut prices = rising(count);
            prices.push(last);
            if short {
                for bar in &mut prices {
                    let [open, high, low, close] = *bar;
                    *bar = [200. - open, 200. - low, 200. - high, 200. - close];
                }
            }
            let bars = bars(&prices);
            let report = backtest.run(configured("Technical Ratings", &[]).as_mut(), &bars, &[]);
            let first = &report.trades[0];
            assert_eq!(
                first.side,
                if short {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                }
            );
            assert_eq!(first.quantity, 1.);
            assert_eq!(first.entry_time, time(2));
            assert_eq!(first.entry_price, if short { 99.5 } else { 100.5 });
            let exit = first.exit.as_ref().expect("protective exit");
            assert_eq!(exit.time, time(count));
            assert_eq!(
                exit.price,
                if short { 200. - exit_price } else { exit_price }
            );
            assert_eq!(exit.profit, profit);
            assert_eq!(
                first.label.as_deref(),
                Some(if short {
                    "Short / Short stop"
                } else {
                    "Long / Long stop"
                })
            );
            assert!(
                report
                    .properties
                    .contains(&("Input: Indicator timeframe".into(), "Chart interval".into()))
            );
            assert_prefixes("Technical Ratings", &[], &bars, &backtest);
        }
    }
}

#[cfg(feature = "studies")]
#[test]
fn optimise_checks_zone_levels_as_a_pair_in_any_order() {
    use crate::strategies::optimise;
    use InputValue::Number;
    let backtest = Backtest::new("Levels", "TEST", Timeframe::Min1, 10_000.);
    let factory = StrategyFactory::new();
    // Overbought is listed first, and equal levels are refused.
    let grid = [
        ("overbought".to_owned(), vec![Number(25.), Number(70.)]),
        (
            "oversold".to_owned(),
            vec![Number(20.), Number(40.), Number(70.)],
        ),
    ];
    let refused: Result<Vec<InputValue>, _> = Err("oversold must be below overbought");
    for name in ["RSI", "Stochastic Slow"] {
        let items: Vec<_> = optimise(|| factory.create(name).unwrap(), &backtest, &[], &[], &grid)
            .map(|item| item.map(|trial| trial.inputs.into_iter().map(|(_, v)| v).collect()))
            .collect();
        assert_eq!(
            items,
            vec![
                Ok(vec![Number(25.), Number(20.)]),
                refused.clone(),
                refused.clone(),
                Ok(vec![Number(70.), Number(20.)]),
                Ok(vec![Number(70.), Number(40.)]),
                refused.clone(),
            ],
            "{name}"
        );
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
            // A strategy draws no study table.
            assert!(!input.id.starts_with("table_"), "{name}: {}", input.id);
            assert!(input.accepts(&input.value), "{name}: {}", input.id);
            assert!(
                input.accepts(&input.default),
                "{name}: {} default",
                input.id
            );
            strategy.set_input(&input.id, input.value).unwrap();
        }
        strategy
            .validate_inputs()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_prefixes(name, &[], &bars, &backtest);
        assert_prefixes(name, &[], &gapped, &backtest);
    }
}
