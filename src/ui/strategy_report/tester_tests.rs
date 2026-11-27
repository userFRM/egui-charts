use super::*;
use crate::{
    model::{ChartType, Direction, OrderSide, SignalShape},
    strategies::{Commission, OrderSize, StrategyContext},
};
use egui::{Role, Vec2};
use egui_kittest::{Harness, kittest::Queryable};

fn bars() -> Vec<Bar> {
    [100.0, 101.0, 102.0, 103.0, 100.0, 99.0, 98.0, 101.0]
        .into_iter()
        .enumerate()
        .map(|(index, price)| {
            Bar::new(
                DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(index as i64),
                price,
                price,
                price,
                price,
                1.0,
            )
        })
        .collect()
}

/// Rising closes buy at 103, then falling closes reverse at minute 100's open of 99.
fn trend() -> Vec<Bar> {
    (0..102)
        .map(|index| {
            let price = match index {
                0..=3 => 100.0 + index as f64,
                4..=96 => 103.0,
                _ => 199.0 - index as f64,
            };
            Bar::new(
                DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(index),
                price,
                price,
                price,
                price,
                1.0,
            )
        })
        .collect()
}

/// A full run is independent of the retained engine used by the tester.
fn fresh(tester: &StrategyTester, bars: &[Bar], lower: &[Bar]) -> StrategyReport {
    let engine = tester.engine("TEST", Timeframe::Min1);
    engine.run(tester.configured_strategy().unwrap().as_mut(), bars, lower)
}

#[test]
fn a_run_continues_on_new_bars_and_rebuilds_after_corrections() {
    let source = trend();
    let mut tester = StrategyTester::default();
    tester.select_strategy("Consecutive Up/Down").unwrap();
    let mut h =
        Harness::builder()
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                |ui,
                 (tester, bars, actions): &mut (
                    StrategyTester,
                    Vec<Bar>,
                    Vec<StrategyTesterAction>,
                )| {
                    actions.extend(tester.show(ui, "TEST", Timeframe::Min1, bars));
                },
                (tester, source[..100].to_vec(), Vec::new()),
            );
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run();
    let report = h.state().0.report().unwrap();
    assert_eq!(report.trades.len(), 1);
    assert_eq!(report.trades[0].side, OrderSide::Buy);
    assert_eq!(report.trades[0].entry_time, source[4].time);
    assert_eq!(report.trades[0].entry_price, 103.0);
    assert!(report.trades[0].exit.is_none());
    assert_eq!(report.equity.last().unwrap().1, 999_997.0);
    let signals = |actions: &[StrategyTesterAction]| match actions.last().unwrap() {
        StrategyTesterAction::Signals { signals, .. } => signals.len(),
        _ => panic!("missing fill replacement"),
    };
    assert_eq!(signals(&h.state().2), 1);
    assert!(!h.state().0.is_stale());
    let count = h.state().2.len();
    h.run();
    assert_eq!(h.state().2.len(), count, "identical frames emit nothing");
    h.state_mut().0.ranges[0].enabled = true;
    h.state_mut()
        .0
        .request_run("TEST", Timeframe::Min1, &source[..100], true)
        .unwrap();
    h.state_mut().0.report.selected = Some(0);
    h.state_mut().1 = source[..101].to_vec();
    h.run();
    let tester = &h.state().0;
    assert!(!tester.is_stale());
    assert_eq!(tester.report(), Some(&fresh(tester, &source[..101], &[])));
    assert_eq!(tester.report.selected, Some(0));
    let report = tester.report().unwrap();
    assert_eq!(report.trades.len(), 2);
    let exit = report.trades[0].exit.as_ref().unwrap();
    assert_eq!(
        (exit.time, exit.price, exit.profit),
        (source[100].time, 99.0, -4.0)
    );
    assert_eq!(report.trades[1].side, OrderSide::Sell);
    assert_eq!(report.trades[1].entry_time, source[100].time);
    assert_eq!(report.trades[1].entry_price, 99.0);
    assert_eq!(signals(&h.state().2), 3);
    h.state_mut().0.report.tab = super::super::Tab::Trades;
    h.state_mut().0.report.selected = Some(1);
    h.state_mut().0.report.compare = true;
    h.state_mut().0.tab = Tab::Inputs;
    h.state_mut().1 = source.clone();
    h.run();
    let tester = &h.state().0;
    assert_eq!(tester.report(), Some(&fresh(tester, &source, &[])));
    assert_eq!(tester.report().unwrap().equity.last().unwrap().1, 999_997.0);
    assert_eq!(tester.report.tab, super::super::Tab::Trades);
    assert_eq!(tester.report.selected, Some(1));
    assert!(tester.report.compare);
    assert_eq!(tester.tab, Tab::Inputs);
    let mut corrected = source.clone();
    corrected[50] = Bar::new(source[50].time, 103.0, 110.0, 103.0, 110.0, 1.0);
    h.state_mut().1 = corrected.clone();
    h.run();
    let tester = &h.state().0;
    assert!(!tester.is_stale());
    assert_eq!(tester.report(), Some(&fresh(tester, &corrected, &[])));
    assert_eq!(
        tester
            .report()
            .unwrap()
            .equity
            .iter()
            .find(|(t, _)| *t == source[50].time)
            .unwrap()
            .1,
        1_000_007.0
    );
    let before = tester.report().unwrap().clone();
    h.state_mut().1.truncate(99);
    h.run();
    assert!(h.state().0.is_stale());
    h.get_by_label("Result is stale. Run again to update.");
    assert_eq!(h.state().0.report(), Some(&before));
    h.state_mut().1 = corrected;
    h.run();
    assert!(h.state().0.is_stale(), "shorter history ends continuation");
    assert_eq!(h.state().0.report(), Some(&before));
}

#[test]
fn continued_runs_request_finer_bars_and_use_host_history() {
    fn frame(tester: &mut StrategyTester, bars: &[Bar]) -> Vec<StrategyTesterAction> {
        let mut actions = Vec::new();
        egui::Context::default()
            .run_ui(egui::RawInput::default(), |ui| {
                actions.extend(tester.show(ui, "TEST", Timeframe::Min1, bars));
            })
            .drop_without_applying_deltas();
        actions
    }
    let source = trend();
    let finer = |count: usize| {
        source[..count]
            .iter()
            .flat_map(|bar| {
                [0, 30].map(|seconds| {
                    Bar::new(
                        bar.time + chrono::TimeDelta::seconds(seconds),
                        bar.open,
                        bar.high,
                        bar.low,
                        bar.close,
                        0.5,
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    let three_minutes = Timeframe::Custom(std::num::NonZeroU64::new(180).unwrap());
    let thirds = |count: usize| {
        crate::data::resample(&source[..count], Timeframe::Min1, three_minutes).unwrap()
    };
    // Host requests carry finer fills, rates, dates and other calculation intervals.
    for (lower, converted, dated, calculated) in [
        (true, false, false, false),
        (false, true, false, false),
        (false, false, true, false),
        (false, false, false, true),
    ] {
        let mut tester = StrategyTester::default();
        if calculated {
            tester.select_strategy("Technical Ratings").unwrap();
            tester
                .set_input("timeframe", InputValue::Timeframe(Some(three_minutes)))
                .unwrap();
        } else {
            tester.select_strategy("Consecutive Up/Down").unwrap();
        }
        tester.lower = lower;
        tester.lower_timeframe = Timeframe::Sec30;
        let mut settings = tester.settings().clone();
        if dated {
            settings.start_time = Some(source[2].time);
        }
        if converted {
            tester.set_instrument_currency(Some("USD".into()));
            settings.currency = Some("EUR".into());
        }
        tester.set_settings(settings).unwrap();
        frame(&mut tester, &source[..100]);
        let Some(StrategyTesterAction::RequestHistory(r1)) = tester
            .request_run("TEST", Timeframe::Min1, &source[..100], false)
            .unwrap()
        else {
            panic!("missing initial request");
        };
        assert_eq!(
            r1.calculation_timeframe,
            calculated.then_some(three_minutes)
        );
        if calculated {
            assert_eq!(
                tester.set_history(&r1, source[..100].to_vec(), vec![]),
                Err("Requested history is empty")
            );
        }
        let first_lower = if lower { finer(100) } else { vec![] };
        let rate = |index: usize, price: f64| {
            Bar::new(source[index].time, price, price, price, price, 0.0)
        };
        let rates = if converted {
            vec![rate(0, 0.5)]
        } else {
            vec![]
        };
        // The dated reply already holds the chart's next bar.
        let held = if dated { 101 } else { 100 };
        let from = if dated { 2 } else { 0 };
        tester
            .set_history_with_calculation(
                &r1,
                source[from..held].to_vec(),
                first_lower.clone(),
                rates.clone(),
                if calculated { thirds(100) } else { vec![] },
            )
            .unwrap();
        for _ in 0..3 {
            frame(&mut tester, &source[..100]);
        }
        assert!(!tester.is_stale());
        let before = tester.report().unwrap().clone();
        let actions = frame(&mut tester, &source[..101]);
        if lower || converted || calculated {
            let [StrategyTesterAction::RequestHistory(r2)] = actions.as_slice() else {
                panic!("missing continuation request: {actions:?}");
            };
            assert_eq!(
                (
                    r2.symbol.as_str(),
                    r2.timeframe,
                    r2.start,
                    r2.lower_timeframe,
                    r2.calculation_timeframe,
                    r2.exchange_rate.clone()
                ),
                (
                    "TEST",
                    Timeframe::Min1,
                    source[100].time,
                    lower.then_some(Timeframe::Sec30),
                    calculated.then_some(three_minutes),
                    r1.exchange_rate.clone()
                )
            );
            assert_ne!(r2.id, r1.id);
            assert!(tester.is_stale());
            assert_eq!(tester.report(), Some(&before));
            assert!(frame(&mut tester, &source[..101]).is_empty());
            assert!(tester.set_history(&r1, source.clone(), finer(101)).is_err());
            let all_lower = if lower { finer(101) } else { vec![] };
            let added_rates = if converted {
                vec![rate(100, 0.25)]
            } else {
                vec![]
            };
            // Reply chart bars are deliberately different; only its finer bars and rates are used.
            tester
                .set_history_with_calculation(
                    r2,
                    vec![source[0].clone()],
                    all_lower.get(200..).unwrap_or(&[]).to_vec(),
                    added_rates.clone(),
                    if calculated {
                        thirds(101)[33..].to_vec()
                    } else {
                        vec![]
                    },
                )
                .unwrap();
            let mut actions = Vec::new();
            for _ in 0..3 {
                actions.extend(frame(&mut tester, &source[..101]));
            }
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, StrategyTesterAction::Signals { .. }))
            );
            assert!(!tester.is_stale());
            if calculated {
                assert_eq!(
                    tester.follow.as_ref().unwrap().engine.calculation_bars,
                    thirds(101)
                );
            }
            let mut engine = tester.engine("TEST", Timeframe::Min1);
            engine.calculation_bars = if calculated { thirds(101) } else { vec![] };
            engine.exchange_rates = rates.into_iter().chain(added_rates).collect();
            assert_eq!(
                tester.report(),
                Some(&engine.run(
                    tester.configured_strategy().unwrap().as_mut(),
                    &source[..101],
                    &all_lower
                ))
            );
            if converted {
                assert_eq!(tester.report().unwrap().performance().all.net_profit, -1.0);
            }
            let actions = frame(&mut tester, &source);
            let [StrategyTesterAction::RequestHistory(r3)] = actions.as_slice() else {
                panic!("missing next request");
            };
            assert!(tester.history_failed(r3, "History unavailable"));
            assert!(tester.is_stale());
            assert!(!tester.history_failed(r2, "late failure"));
            assert!(frame(&mut tester, &source).is_empty());
        } else {
            // Closing the bar the host already supplied changes nothing; the next one continues.
            assert!(actions.is_empty());
            assert!(!tester.is_stale());
            assert_eq!(tester.report(), Some(&before));
            let mut actions = Vec::new();
            for _ in 0..3 {
                actions.extend(frame(&mut tester, &source));
            }
            assert!(
                actions
                    .iter()
                    .all(|action| !matches!(action, StrategyTesterAction::RequestHistory(_)))
            );
            assert!(!tester.is_stale());
            assert_eq!(tester.report(), Some(&fresh(&tester, &source[2..], &[])));
            let before = tester.report().unwrap().clone();
            let mut corrected = source.clone();
            corrected[50].close = 110.0;
            corrected[50].high = 110.0;
            frame(&mut tester, &corrected);
            assert!(tester.is_stale());
            assert_eq!(tester.report(), Some(&before));
        }
    }
}

#[test]
fn built_in_run_edits_costs_and_inputs_and_replaces_fill_signals() {
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for width in [240.0, 320.0, 640.0, 1440.0] {
            let mut tester = StrategyTester::default();
            tester.select_strategy("Consecutive Up/Down").unwrap();
            let mut h = Harness::builder()
                .with_size(Vec2::new(width, 900.0))
                .with_context_setup({
                    let theme = theme.clone();
                    move |ctx| crate::theme::apply_to_egui(ctx, &theme)
                })
                .build_ui_state(
                    |ui, (tester, actions): &mut (StrategyTester, Vec<StrategyTesterAction>)| {
                        let right = ui.max_rect().right();
                        actions.extend(tester.show(ui, "TEST", Timeframe::Min1, &bars()));
                        assert!(
                            ui.min_rect().right() <= right + 1.0,
                            "tester widened its host"
                        );
                    },
                    (tester, Vec::new()),
                );
            h.get_by_role_and_label(Role::Button, "Run").click();
            h.run();
            let report = h.state().0.report().unwrap();
            // Three rising closes place the long at bar 3, filled at bar 4's open of 100.
            // Three falling closes reverse it at bar 7's open of 101: one unit earns 1.
            assert_eq!(report.performance().all.net_profit, 1.0);
            assert_eq!(report.trades[0].entry_time, bars()[4].time);
            assert_eq!(report.trades[0].exit.as_ref().unwrap().time, bars()[7].time);
            assert!(!h.state().0.is_stale());
            let Some(StrategyTesterAction::Signals { signals, .. }) = h.state().1.last() else {
                panic!("missing fill replacement");
            };
            assert_eq!(signals.len(), 3, "closed long and open short");
            assert!(matches!(
                signals[2].shape,
                SignalShape::Marker {
                    direction: Direction::Down,
                    ..
                }
            ));
            let previous = report.clone();
            let mut settings = h.state().0.settings().clone();
            settings.commission = Commission::PerUnit(1.0);
            h.state_mut().0.set_settings(settings).unwrap();
            assert!(h.state().0.is_stale());
            assert_eq!(h.state().0.report(), Some(&previous));
            h.run();
            h.get_by_role_and_label(Role::Button, "Run").click();
            h.run();
            assert_eq!(
                h.state().0.report().unwrap().performance().all.net_profit,
                -1.0
            );
            assert!(!h.state().0.is_stale());
            h.state_mut()
                .0
                .set_input("length", InputValue::Length(1))
                .unwrap();
            assert!(h.state().0.is_stale());
            h.run();
            h.get_by_role_and_label(Role::Button, "Add alert").click();
            h.run();
            let Some(StrategyTesterAction::AddAlert {
                strategy,
                inputs,
                settings,
                symbol,
                timeframe,
            }) = h.state().1.last()
            else {
                panic!("missing alert intent");
            };
            assert_eq!(strategy, "Consecutive Up/Down");
            assert_eq!((symbol.as_str(), *timeframe), ("TEST", Timeframe::Min1));
            assert!(inputs.contains(&("length".into(), InputValue::Length(1))));
            assert_eq!(settings, h.state().0.settings());
            assert!(h.state().0.is_stale(), "the intent leaves the run alone");
            h.get_by_role_and_label(Role::Button, "Run").click();
            h.run();
            let report = h.state().0.report().unwrap();
            assert_eq!(report.trades[0].side, OrderSide::Buy);
            assert_eq!(report.trades[0].entry_time, bars()[2].time);
            assert_eq!(
                report.performance().all.net_profit,
                -5.0,
                "102 to 99, less two unit fees"
            );
            h.get_by_role_and_label(Role::Tab, "Backtest settings")
                .click();
            h.run();
            h.get_by_role_and_label(Role::CheckBox, "Fill on close")
                .click();
            h.run();
            assert!(h.state().0.is_stale());
            h.get_by_role_and_label(Role::Button, "Run").click();
            h.run();
            assert_eq!(
                h.state().0.report().unwrap().trades[0].entry_time,
                bars()[1].time
            );
            h.get_by_role_and_label(Role::Tab, "Inputs").click();
            h.run();
            h.get_by_role_and_label(Role::Tab, "Report").scroll_to_me();
            h.run();
            h.get_by_role_and_label(Role::Tab, "Report").click();
            h.run();
            assert!(h.state().0.tab == Tab::Report);
        }
    }
}

#[test]
fn execution_settings_survive_form_edits_undo_saved_state_and_run_properties() {
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let mut tester = StrategyTester::default();
        tester.select_strategy("Consecutive Up/Down").unwrap();
        tester.tab = Tab::Properties;
        let mut h = Harness::builder()
            .with_size(Vec2::new(1440.0, 900.0))
            .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
            .build_ui_state(
                |ui, tester: &mut StrategyTester| {
                    tester.show(ui, "TEST", Timeframe::Min1, &bars());
                },
                tester,
            );
        h.run();
        for label in [
            "Long funding (%)",
            "Short funding (%)",
            "Price increment",
            "Liquidation unit step",
            "Order execution delay (price ticks)",
        ] {
            h.get_by_role_and_label(Role::SpinButton, label);
        }
        h.get_by_role_and_label(Role::TextInput, "End (UTC)")
            .focus();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "End (UTC)")
            .type_text("1970-01-02");
        h.run();
        assert_eq!(
            h.state().settings().end_time,
            Some(DateTime::UNIX_EPOCH + chrono::TimeDelta::days(1))
        );
        h.get_by_role_and_label(Role::Button, "Undo settings")
            .click();
        h.run();
        assert_eq!(h.state().settings().end_time, None);
        let start = h
            .get_by_role_and_label(Role::SpinButton, "Price increment")
            .rect()
            .center();
        h.drag_at(start);
        h.run();
        for distance in [10.0, 20.0, 30.0] {
            h.hover_at(start + Vec2::new(distance, 0.0));
            h.run();
        }
        h.drop_at(start + Vec2::new(30.0, 0.0));
        h.run();
        assert_ne!(h.state().settings().price_increment, 0.01);
        assert!(h.state_mut().undo_settings());
        assert_eq!(h.state().settings().price_increment, 0.01);
        h.get_by_role_and_label(Role::ComboBox, "Limit verification")
            .click();
        h.run();
        h.get_by_label("Cross by one increment").click();
        h.run();
        assert!(h.state().settings().require_limit_cross);
        h.get_by_role_and_label(Role::CheckBox, "Decide on historical price ticks")
            .click();
        h.run();
        assert!(h.state().settings().recalculate_on_history);
        h.get_by_role_and_label(Role::Button, "Undo settings")
            .click();
        h.run();
        assert!(!h.state().settings().recalculate_on_history);
        assert!(h.state().settings().require_limit_cross);
        h.get_by_role_and_label(Role::Button, "Redo settings")
            .click();
        h.run();
        assert!(h.state().settings().recalculate_on_history);
        h.get_by_role_and_label(Role::CheckBox, "Recalculate on every update")
            .click();
        h.run();
        assert!(h.state().settings().recalculate_on_every_update);
        h.get_by_role_and_label(Role::Button, "Undo settings")
            .click();
        h.run();
        assert!(!h.state().settings().recalculate_on_every_update);
        assert!(h.state().settings().recalculate_on_history);
        h.get_by_role_and_label(Role::ComboBox, "Decision candles")
            .click();
        h.run();
        h.get_by_label("Heikin Ashi").click();
        h.run();
        assert_eq!(h.state().settings().chart_type, ChartType::Heikin);
        assert!(!h.state().settings().recalculate_on_history);
        h.get_by_role_and_label(Role::CheckBox, "Execute at original candle prices")
            .click();
        h.run();
        assert!(h.state().settings().original_price_fills);
        let mut tiny = h.state().settings().clone();
        tiny.price_increment = 1e-8;
        tiny.liquidation_increment = 1e-6;
        tiny.leverage = 2.0;
        tiny.long_funding = Some(0.5);
        h.state_mut().set_settings(tiny).unwrap();
        let output = egui::Context::default().run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1440.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                h.state_mut().show(ui, "TEST", Timeframe::Min1, &bars());
            },
        );
        for value in [
            "0.00000001",
            "0.000001",
            "Four times the shortfall at price points",
            "Whole position at the margin crossing",
        ] {
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == value)), "{value} must be shown");
        }
        output.drop_without_applying_deltas();
        let mut settings = h.state().settings().clone();
        settings.long_funding = Some(0.2);
        settings.short_funding = Some(0.75);
        settings.price_increment = 0.25;
        settings.liquidation_increment = 0.1;
        settings.fill_on_close = true;
        settings.order_delay = 3;
        settings.recalculate_on_every_update = true;
        h.state_mut().set_settings(settings.clone()).unwrap();
        let saved = serde_json::to_string(&settings).unwrap();
        let restored = serde_json::from_str::<BacktestSettings>(&saved).unwrap();
        assert_eq!(restored, settings);
        assert_eq!(
            serde_json::from_str::<BacktestSettings>("{}").unwrap(),
            BacktestSettings::default()
        );
        h.state_mut()
            .set_settings(BacktestSettings::default())
            .unwrap();
        h.state_mut().set_settings(restored).unwrap();
        h.run();
        h.get_by_role_and_label(Role::Button, "Run").click();
        h.run();
        let report = h.state().report().unwrap();
        let property = |key: &str| {
            report
                .properties
                .iter()
                .find(|(name, _)| name == key)
                .unwrap()
                .1
                .as_str()
        };
        for (key, expected) in [
            ("Long funding fraction", "0.2"),
            ("Short funding fraction", "0.75"),
            ("Price increment", "0.25"),
            ("Liquidation unit step", "0.1"),
            ("Verify limit crossing", "true"),
            ("Fill on close", "true"),
            ("Order execution delay", "3"),
            ("Decision chart type", "Heikin Ashi"),
            ("Original-price execution", "true"),
            ("Historical tick decisions", "false"),
            ("Recalculate on every update", "true"),
        ] {
            assert_eq!(property(key), expected);
        }
        let previous = report.clone();
        assert!(h.state_mut().undo_settings());
        assert!(h.state().is_stale());
        assert_eq!(h.state().report(), Some(&previous));
        for (long, short, tick, step, kind, history) in [
            (Some(-0.1), None, 0.01, 1.0, ChartType::Candles, false),
            (None, Some(f64::NAN), 0.01, 1.0, ChartType::Candles, false),
            (None, None, 0.0, 1.0, ChartType::Candles, false),
            (None, None, 0.01, 0.0, ChartType::Candles, false),
            (None, None, 0.01, 1.0, ChartType::Heikin, true),
            (None, None, 0.01, 1.0, ChartType::Renko, true),
        ] {
            let mut invalid = settings.clone();
            invalid.long_funding = long;
            invalid.short_funding = short;
            invalid.price_increment = tick;
            invalid.liquidation_increment = step;
            invalid.chart_type = kind;
            invalid.recalculate_on_history = history;
            let before = h.state().settings().clone();
            assert!(h.state_mut().set_settings(invalid).is_err());
            assert_eq!(h.state().settings(), &before);
        }
    }
}

#[test]
fn undo_and_redo_restore_finer_fill_edits_and_supersede_their_requests() {
    let tester = StrategyTester {
        tab: Tab::Properties,
        ..Default::default()
    };
    let mut h = Harness::builder()
        .with_size(Vec2::new(1440.0, 900.0))
        .build_ui_state(
            |ui, (tester, actions): &mut (StrategyTester, Vec<StrategyTesterAction>)| {
                actions.extend(tester.show(ui, "TEST", Timeframe::Min1, &bars()));
            },
            (tester, Vec::new()),
        );
    h.run();
    let fills = |h: &Harness<'_, (StrategyTester, Vec<StrategyTesterAction>)>| {
        let tester = &h.state().0;
        (
            tester.lower,
            tester.lower_timeframe,
            tester.settings().initial_capital,
        )
    };
    let click = |h: &mut Harness<'_, (StrategyTester, Vec<StrategyTesterAction>)>, label| {
        h.get_by_role_and_label(Role::Button, label).click();
        h.run();
    };
    let run = |h: &mut Harness<'_, (StrategyTester, Vec<StrategyTesterAction>)>| {
        h.state_mut().1.clear();
        h.get_by_role_and_label(Role::Button, "Run").click();
        h.run();
        let [StrategyTesterAction::RequestHistory(request)] = h.state().1.as_slice() else {
            panic!("one history request, got {:?}", h.state().1);
        };
        request.clone()
    };
    h.get_by_role_and_label(Role::SpinButton, "Initial capital")
        .set_value_accesskit(12345.0);
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Lower-interval fills")
        .click();
    h.run();
    assert_eq!(fills(&h), (true, Timeframe::Sec1, 12345.0));
    click(&mut h, "Undo settings");
    assert_eq!(fills(&h), (false, Timeframe::Sec1, 12345.0));
    click(&mut h, "Redo settings");
    assert_eq!(fills(&h), (true, Timeframe::Sec1, 12345.0));
    h.get_by_role_and_label(Role::ComboBox, "Fill interval")
        .click();
    h.run();
    h.get_by_label("30s").click();
    h.run();
    assert_eq!(fills(&h), (true, Timeframe::Sec30, 12345.0));
    let edited = run(&mut h);
    assert_eq!(edited.lower_timeframe, Some(Timeframe::Sec30));
    click(&mut h, "Undo settings");
    assert_eq!(fills(&h), (true, Timeframe::Sec1, 12345.0));
    assert_eq!(
        h.state_mut().0.set_history(&edited, bars(), bars()),
        Err("History request is no longer current")
    );
    let undone = run(&mut h);
    assert_eq!(undone.lower_timeframe, Some(Timeframe::Sec1));
    h.state_mut()
        .0
        .set_history(&undone, bars(), bars())
        .unwrap();
    h.run();
    assert!(!h.state().0.is_stale());
    // A completed run shows the report; reopen the form.
    h.get_by_role_and_label(Role::Tab, "Backtest settings")
        .click();
    h.run();
    click(&mut h, "Redo settings");
    assert_eq!(fills(&h), (true, Timeframe::Sec30, 12345.0));
    assert!(h.state().0.is_stale());
    assert!(h.state().0.report().is_some());
    assert_eq!(run(&mut h).lower_timeframe, Some(Timeframe::Sec30));
}

#[test]
fn tick_slippage_follows_the_price_increment_until_a_size_is_typed() {
    // Three rising closes buy at market at bar 4's open of 100, two ticks higher.
    for (order, entry) in [
        ("increment first", 100.5),
        ("increment after", 100.5),
        ("host increment after", 100.5),
        ("typed size", 101.0),
    ] {
        let mut tester = StrategyTester::default();
        tester.select_strategy("Consecutive Up/Down").unwrap();
        tester.tab = Tab::Properties;
        let mut h = Harness::builder()
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                |ui, tester: &mut StrategyTester| {
                    tester.show(ui, "TEST", Timeframe::Min1, &bars());
                },
                tester,
            );
        h.run();
        let increment = |h: &mut Harness<'_, StrategyTester>| {
            h.get_by_role_and_label(Role::SpinButton, "Price increment")
                .set_value_accesskit(0.25);
            h.run();
        };
        if order == "increment first" {
            increment(&mut h);
        }
        h.get_by_role_and_label(Role::ComboBox, "Slippage model")
            .click();
        h.run();
        h.get_by_label("Ticks").click();
        h.run();
        h.get_by_role_and_label(Role::SpinButton, "Slippage ticks")
            .set_value_accesskit(2.0);
        h.run();
        if order == "typed size" {
            h.get_by_role_and_label(Role::SpinButton, "Tick size")
                .set_value_accesskit(0.5);
            h.run();
        }
        if order == "host increment after" {
            let mut settings = h.state().settings().clone();
            settings.price_increment = 0.25;
            h.state_mut().set_settings(settings).unwrap();
            h.run();
        } else if order != "increment first" {
            increment(&mut h);
        }
        h.get_by_role_and_label(Role::Button, "Run").click();
        h.run();
        assert_eq!(
            h.state().report().unwrap().trades[0].entry_price,
            entry,
            "{order}"
        );
    }
}

#[test]
fn selecting_a_strategy_applies_its_declared_order_size() {
    let mut tester = StrategyTester::default();
    for (name, size) in [
        ("SuperTrend", OrderSize::EquityFraction(0.15)),
        ("Technical Ratings", OrderSize::EquityFraction(0.05)),
        ("Consecutive Up/Down", OrderSize::Fixed(1.0)),
        ("Bar Up/Down", OrderSize::EquityFraction(0.1)),
    ] {
        let mut settings = tester.settings().clone();
        settings.order_size = OrderSize::Cash(500.0);
        settings.commission = Commission::PerOrder(2.0);
        tester.set_settings(settings).unwrap();
        tester.select_strategy(name).unwrap();
        assert_eq!(tester.settings().order_size, size, "{name}");
        assert_eq!(
            tester.settings().commission,
            Commission::PerOrder(2.0),
            "{name}"
        );
        assert!(tester.undo_settings(), "{name}");
        assert_eq!(
            tester.settings().order_size,
            OrderSize::Cash(500.0),
            "{name}"
        );
        assert_eq!(tester.strategy(), name);
    }
    tester.settings.start_time = Some(DateTime::UNIX_EPOCH + chrono::TimeDelta::days(5));
    tester.settings.end_time = Some(DateTime::UNIX_EPOCH + chrono::TimeDelta::days(1));
    tester.start = "1970-01-0".into();
    tester.end = "1970-01-02".into();
    let before = tester.settings().clone();
    let undo_steps = tester.undo_settings.len();
    assert!(!tester.redo_settings.is_empty());
    assert_eq!(tester.select_strategy("SuperTrend"), Ok(()));
    assert_eq!(tester.strategy(), "SuperTrend");
    assert_eq!(
        tester.settings().order_size,
        OrderSize::EquityFraction(0.15)
    );
    assert_eq!(tester.start, "1970-01-0");
    assert_eq!(tester.end, "1970-01-02");
    assert!(!tester.redo_settings());
    assert_eq!(tester.undo_settings.len(), undo_steps + 1);
    assert!(tester.undo_settings());
    assert_eq!(tester.settings(), &before);
    assert_eq!(tester.undo_settings.len(), undo_steps);
    assert_eq!(tester.strategy(), "SuperTrend");

    struct Sized(OrderSize);
    impl Strategy for Sized {
        fn default_order_size(&self) -> Option<OrderSize> {
            Some(self.0)
        }
        fn on_bar(&mut self, _: &mut StrategyContext<'_>) {}
    }
    // A host strategy listed first sets the starting size; one the form refuses gives way.
    for (declared, size) in [
        (OrderSize::Cash(250.0), OrderSize::Cash(250.0)),
        (OrderSize::Fixed(f64::NAN), OrderSize::Fixed(1.0)),
    ] {
        let mut factory = StrategyFactory::new();
        factory.register("A host strategy", move || Box::new(Sized(declared)));
        let tester = StrategyTester::new(factory);
        assert_eq!(tester.strategy(), "A host strategy");
        assert_eq!(tester.settings().order_size, size);
    }
}

/// A factory set later keeps the selection, its inputs and the settings while it builds the
/// strategy and accepts the inputs; otherwise it selects the strategy of that name, or its first,
/// at its defaults.
#[test]
fn a_new_factory_keeps_the_selection_it_still_offers() {
    struct Host;
    impl Strategy for Host {
        fn on_bar(&mut self, _: &mut StrategyContext<'_>) {}
    }
    let mut tester = StrategyTester::new(StrategyFactory::new());
    tester.select_strategy("Consecutive Up/Down").unwrap();
    tester.set_input("length", InputValue::Length(5)).unwrap();
    let mut settings = tester.settings().clone();
    settings.initial_capital = 20_000.0;
    tester.set_settings(settings.clone()).unwrap();
    let mut hosted = StrategyFactory::new();
    hosted.register("Host", || Box::new(Host));
    tester.set_factory(hosted);
    assert_eq!(tester.strategy(), "Consecutive Up/Down");
    assert_eq!(tester.inputs[0].value, InputValue::Length(5));
    assert_eq!(tester.settings(), &settings);
    tester.select_strategy("Host").unwrap();

    tester.set_factory(StrategyFactory::new());
    let values = |inputs: &[StudyInput]| -> Vec<_> {
        (inputs.iter())
            .map(|input| (input.id.clone(), input.value.clone()))
            .collect()
    };
    let first = StrategyFactory::new().list()[0].to_owned();
    let defaults = StrategyFactory::new()
        .create(&first)
        .unwrap()
        .study_inputs();
    assert_eq!(tester.strategy(), first);
    assert_eq!(values(&tester.inputs), values(&defaults));
    assert_eq!(tester.settings(), &settings);

    // A strategy of the same name that refuses the inputs is selected at its own.
    tester.select_strategy("Consecutive Up/Down").unwrap();
    tester.set_input("length", InputValue::Length(5)).unwrap();
    let mut renamed = StrategyFactory::new();
    renamed.register("Consecutive Up/Down", || Box::new(Host));
    tester.set_factory(renamed);
    assert_eq!(tester.strategy(), "Consecutive Up/Down");
    assert!(tester.inputs.is_empty());
}

#[test]
fn run_failures_remain_visible_above_scrolling_content() {
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for tab in [Tab::Report, Tab::Properties, Tab::Optimisation] {
            let mut tester = StrategyTester::default();
            tester.set_report(
                Some(StrategyReport::new(
                    "Previous",
                    "TEST",
                    Timeframe::Min1,
                    1000.0,
                )),
                &bars(),
            );
            tester.scope = Some(("TEST".into(), Timeframe::Min1));
            tester.tab = tab;
            tester.error = Some("History unavailable".into());
            let mut h = Harness::builder()
                .with_size(Vec2::new(640.0, 330.0))
                .with_context_setup({
                    let theme = theme.clone();
                    move |ctx| crate::theme::apply_to_egui(ctx, &theme)
                })
                .build_ui_state(
                    |ui, tester: &mut StrategyTester| {
                        tester.show(ui, "TEST", Timeframe::Min1, &bars());
                    },
                    tester,
                );
            h.run();
            assert!(h.get_by_label("History unavailable").rect().bottom() <= 330.0);
        }
    }
}

#[test]
fn requested_history_is_scoped_and_superseded_by_edits_and_chart_changes() {
    let source = bars();
    let mut tester = StrategyTester::default();
    let mut settings = tester.settings().clone();
    settings.start_time = Some(source[2].time);
    settings.end_time = settings.start_time;
    assert_eq!(
        tester.set_settings(settings.clone()),
        Err("Use an end after the start")
    );
    settings.end_time = Some(source[6].time);
    tester.set_settings(settings).unwrap();
    tester.lower = true;
    tester.lower_timeframe = Timeframe::Sec30;
    let request = match tester
        .request_run("TEST", Timeframe::Min1, &source, false)
        .unwrap()
        .unwrap()
    {
        StrategyTesterAction::RequestHistory(request) => request,
        _ => unreachable!(),
    };
    assert_eq!(
        (
            request.symbol.as_str(),
            request.timeframe,
            request.start,
            request.end,
            request.lower_timeframe
        ),
        (
            "TEST",
            Timeframe::Min1,
            source[2].time,
            Some(source[6].time),
            Some(Timeframe::Sec30)
        )
    );
    assert!(
        tester
            .set_history(&request, source.clone(), Vec::new())
            .is_err()
    );
    assert!(
        tester.pending.is_some(),
        "empty finer history must not silently fall back"
    );
    tester.select_strategy("Consecutive Up/Down").unwrap();
    assert!(
        tester
            .set_history(&request, source.clone(), source.clone())
            .is_err()
    );
    assert!(!tester.history_failed(&request, "late failure"));
    let replacement = match tester
        .request_run("TEST", Timeframe::Min1, &source, false)
        .unwrap()
        .unwrap()
    {
        StrategyTesterAction::RequestHistory(request) => request,
        _ => unreachable!(),
    };
    assert_ne!(replacement.id, request.id);
    tester
        .set_history(&replacement, source.clone(), source.clone())
        .unwrap();
    assert!(tester.job.is_some());
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        tester.show(ui, "OTHER", Timeframe::Hour1, &source);
    })
    .drop_without_applying_deltas();
    assert!(tester.job.is_none());
    assert!(
        tester.report().is_none(),
        "old scope must not publish after navigation"
    );
    let mut h = Harness::builder().with_max_steps(32).build_ui_state(
        |ui, tester: &mut StrategyTester| {
            tester.show(ui, "TEST", Timeframe::Min1, &bars());
        },
        tester,
    );
    let Some(StrategyTesterAction::RequestHistory(request)) = h
        .state_mut()
        .request_run("TEST", Timeframe::Min1, &source, false)
        .unwrap()
    else {
        panic!("missing history request");
    };
    h.state_mut()
        .set_history(&request, source.clone(), source)
        .unwrap();
    h.run();
    let comparison = &h.state().report.data.as_ref().unwrap().comparison;
    assert_eq!(
        comparison.buy_and_hold_equity.first().unwrap().0,
        bars()[2].time
    );
    assert_eq!(
        comparison.buy_and_hold_equity.last().unwrap().0,
        bars()[5].time
    );
    assert!((comparison.buy_and_hold_return.unwrap() - (99.0 / 102.0 - 1.0)).abs() < 1e-12);
    assert!(
        h.state()
            .report()
            .unwrap()
            .properties
            .contains(&("End time".into(), "1970-01-01T00:06:00+00:00".into()))
    );
}

/// A different trade-based interval has no scheduled close for a time-based chart.
#[test]
fn single_runs_check_inputs_on_the_chart_interval_before_requesting_history() {
    for start_time in [Some(bars()[2].time), None] {
        let mut tester = StrategyTester::default();
        tester.select_strategy("Technical Ratings").unwrap();
        tester
            .set_input(
                "timeframe",
                InputValue::Timeframe(Some(Timeframe::Ticks(
                    std::num::NonZeroU64::new(100).unwrap(),
                ))),
            )
            .unwrap();
        let mut settings = tester.settings().clone();
        settings.start_time = start_time;
        tester.set_settings(settings).unwrap();
        let previous = StrategyReport::new("Previous", "TEST", Timeframe::Min1, 1000.0);
        let mut h = Harness::builder()
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                |ui, (tester, actions): &mut (StrategyTester, Vec<StrategyTesterAction>)| {
                    actions.extend(tester.show(ui, "TEST", Timeframe::Min1, &bars()));
                },
                (tester, Vec::new()),
            );
        let tester = &mut h.state_mut().0;
        tester.set_report(Some(previous.clone()), &bars());
        tester.stale = false;
        tester.trials.push(Trial {
            inputs: Vec::new(),
            label: "Previous trial".into(),
            profit: 1.0,
            drawdown: Some(2.0),
            trades: 3,
        });
        tester.skipped.push(("Previous refusal", 2));
        assert!(!h.state().0.is_stale());
        h.get_by_role_and_label(Role::Button, "Run").click();
        h.run();
        assert!(
            !h.state()
                .1
                .iter()
                .any(|action| matches!(action, StrategyTesterAction::RequestHistory(_))),
            "a refused run requests no history"
        );
        h.get_by_label(
            "indicator timeframe must be the chart interval or a time interval on a time-based chart",
        );
        assert_eq!(h.state().0.report(), Some(&previous));
        assert!(!h.state().0.is_stale());
        assert_eq!(h.state().0.trials.len(), 1);
        assert_eq!(h.state().0.trials[0].label, "Previous trial");
        assert_eq!(h.state().0.skipped, [("Previous refusal", 2)]);
    }
}

#[test]
fn runs_report_the_instrument_currency_and_convert_into_another_at_requested_rates() {
    let mut tester = StrategyTester::default();
    tester.select_strategy("Consecutive Up/Down").unwrap();
    tester.set_instrument_currency(Some("USD".into()));
    let mut h = Harness::builder().with_max_steps(32).build_ui_state(
        |ui, tester: &mut StrategyTester| {
            tester.show(ui, "TEST", Timeframe::Min1, &bars());
        },
        tester,
    );
    // Without an account currency the run needs no history and reports the instrument's.
    assert_eq!(
        h.state_mut()
            .request_run("TEST", Timeframe::Min1, &bars(), false),
        Ok(None)
    );
    h.run();
    assert_eq!(h.state().report().unwrap().currency.as_deref(), Some("USD"));
    let mut settings = h.state().settings().clone();
    settings.currency = Some("EUR".into());
    h.state_mut().set_settings(settings).unwrap();
    let Some(StrategyTesterAction::RequestHistory(request)) = h
        .state_mut()
        .request_run("TEST", Timeframe::Min1, &bars(), false)
        .unwrap()
    else {
        panic!("missing exchange rate request");
    };
    assert_eq!(
        request.exchange_rate,
        Some(StudyDataRequest::exchange_rate("USD", "EUR"))
    );
    let rate = Bar::new(bars()[0].time, 0.5, 0.5, 0.5, 0.5, 0.0);
    h.state_mut()
        .set_history_with_rates(&request, bars(), Vec::new(), vec![rate])
        .unwrap();
    h.run();
    let report = h.state().report().unwrap();
    assert_eq!(report.currency.as_deref(), Some("EUR"));
    // The long from 100 to 101 earns 1 USD, 0.5 EUR.
    assert_eq!(report.performance().all.net_profit, 0.5);
    // An optimisation without the rates skips each trial instead of listing zero results.
    h.state_mut().ranges[0].enabled = true;
    h.state_mut().ranges[0].from = InputValue::Length(1);
    h.state_mut().ranges[0].to = InputValue::Length(2);
    let engine = h.state().engine("TEST", Timeframe::Min1);
    h.state_mut()
        .start_job(engine, bars(), Vec::new(), true, None)
        .unwrap();
    h.run();
    assert!(h.state().trials.is_empty());
    assert_eq!(h.state().skipped, [("Exchange rates unavailable", 2)]);
}

#[test]
fn optimisation_counts_decimal_grid_yields_sorts_and_applies_inputs() {
    use crate::studies::InputKind;
    let input = StudyInput::new(
        "n",
        "N",
        InputValue::Number(0.0),
        InputKind::Number(0.0, 1.0),
    );
    let mut range = Range::new(&input).unwrap();
    range.enabled = true;
    range.to = InputValue::Number(0.3);
    range.step = InputValue::Number(0.1);
    assert_eq!(
        range.values().unwrap(),
        vec![
            InputValue::Number(0.0),
            InputValue::Number(0.1),
            InputValue::Number(0.2),
            InputValue::Number(0.3)
        ]
    );
    let mut signed = Range::new(&StudyInput::new(
        "s",
        "Signed",
        InputValue::Integer(-2),
        InputKind::Integer(-2, 2),
    ))
    .unwrap();
    signed.enabled = true;
    signed.to = InputValue::Integer(2);
    signed.step = InputValue::Integer(2);
    assert_eq!(
        signed.values().unwrap(),
        vec![
            InputValue::Integer(-2),
            InputValue::Integer(0),
            InputValue::Integer(2)
        ]
    );
    assert_eq!(
        optimisation::grid(&[range, signed]).unwrap().0,
        12,
        "four decimal values times three signed values"
    );
    let mut range = Range::new(&input).unwrap();
    range.to = InputValue::Number(0.3);
    // (-1.5e308 .. 1e308) / 1e308 has three values. The span and the third
    // intermediate product overflow f64, but the grid values themselves are finite.
    let extreme = StudyInput::new(
        "wide",
        "Wide",
        InputValue::Number(-1.5e308),
        InputKind::Number(-f64::MAX, f64::MAX),
    );
    let mut wide = Range::new(&extreme).unwrap();
    wide.to = InputValue::Number(1e308);
    wide.step = InputValue::Number(1e308);
    let values = wide.values().unwrap();
    assert_eq!(values.len(), 3);
    for (actual, expected) in values.iter().zip([-1.5e308, -0.5e308, 0.5e308]) {
        let InputValue::Number(actual) = actual else {
            unreachable!()
        };
        assert!(((actual - expected) / expected).abs() < 1e-14);
    }
    range.step = InputValue::Number(0.0);
    assert!(range.values().is_err());
    range.step = InputValue::Number(f64::MIN_POSITIVE);
    assert!(range.values().is_err());
    let mut tester = StrategyTester::default();
    tester.select_strategy("Consecutive Up/Down").unwrap();
    tester.ranges[0].enabled = true;
    tester.ranges[0].from = InputValue::Length(1);
    tester.ranges[0].to = InputValue::Length(3);
    assert_eq!(optimisation::grid(&tester.ranges).unwrap().0, 3);
    tester
        .start_job(
            tester.engine("TEST", Timeframe::Min1),
            bars(),
            Vec::new(),
            true,
            None,
        )
        .unwrap();
    assert!(
        tester
            .job
            .as_mut()
            .unwrap()
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "optimisation must yield before finishing on the frame"
    );
    let mut h = Harness::builder()
        .with_max_steps(16)
        .with_size(Vec2::new(1440.0, 900.0))
        .build_ui_state(
            |ui, tester: &mut StrategyTester| {
                tester.show(ui, "TEST", Timeframe::Min1, &bars());
            },
            tester,
        );
    // Establishing the scope cancels work that belongs to an unmounted tester.
    h.state_mut().scope = Some(("TEST".into(), Timeframe::Min1));
    h.state_mut()
        .request_run("TEST", Timeframe::Min1, &bars(), false)
        .unwrap();
    h.run();
    assert!(!h.state().is_stale());
    h.state_mut().tab = Tab::Optimisation;
    h.run();
    h.get_by_role_and_label(Role::Button, "Run optimisation")
        .click();
    h.run();
    assert!(
        !h.state().is_stale(),
        "optimising does not change the displayed run's inputs"
    );
    assert_eq!(h.state().completed.get(), 3);
    // Length 1 buys at 102 and sells at 99: -3. Length 2 buys at 103 and sells at 98: -5.
    // Length 3 buys at 100 and sells at 101: +1. Every fill is the next bar's open.
    assert_eq!(
        h.state()
            .trials
            .iter()
            .map(|trial| trial.profit)
            .collect::<Vec<_>>(),
        vec![1.0, -3.0, -5.0]
    );
    h.state_mut().tab = Tab::Optimisation;
    h.run();
    h.get_by_role_and_label(Role::Button, "Net profit").click();
    h.run();
    assert_eq!(h.state().trials[0].profit, -5.0);
    h.get_all_by_role_and_label(Role::Button, "Apply inputs")
        .next()
        .unwrap()
        .click();
    h.run();
    assert_eq!(h.state().inputs[0].value, InputValue::Length(2));
    assert!(h.state().is_stale());
    assert!(h.state().trials.is_empty());
    h.state_mut().tab = Tab::Optimisation;
    h.run();
    h.get_by_role_and_label(Role::Button, "Run optimisation")
        .click();
    h.run();
    assert_eq!(h.state().trials.len(), 3);
    h.get_by_role_and_label(Role::CheckBox, "Consecutive closes")
        .click();
    h.run();
    assert!(
        h.state().trials.is_empty(),
        "range edits discard previous trials"
    );
}

#[test]
fn zone_levels_run_in_either_order_and_optimisation_skips_refused_pairs() {
    use InputValue::Number;
    // Each oversold level is above the strategy's default overbought level.
    for (name, oversold) in [("RSI", 75.0), ("Stochastic Slow", 85.0)] {
        let mut tester = StrategyTester::default();
        tester.select_strategy(name).unwrap();
        tester.set_input("overbought", Number(90.0)).unwrap();
        tester.set_input("oversold", Number(oversold)).unwrap();
        let mut h = Harness::builder()
            .with_max_steps(16)
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                |ui, tester: &mut StrategyTester| {
                    tester.show(ui, "TEST", Timeframe::Min1, &bars());
                },
                tester,
            );
        h.get_by_role_and_label(Role::Button, "Run").click();
        h.run();
        let properties = &h.state().report().expect(name).properties;
        assert!(
            properties.contains(&("Input: Oversold".into(), oversold.to_string())),
            "{name}"
        );
        assert!(
            properties.contains(&("Input: Overbought".into(), "90".into())),
            "{name}"
        );
        for range in &mut h.state_mut().ranges {
            (range.enabled, range.from, range.to, range.step) = match range.input.id.as_str() {
                "oversold" => (true, Number(20.0), Number(40.0), Number(20.0)),
                "overbought" => (true, Number(30.0), Number(70.0), Number(40.0)),
                _ => continue,
            };
        }
        h.state_mut().tab = Tab::Optimisation;
        h.run();
        h.get_by_role_and_label(Role::Button, "Run optimisation")
            .click();
        h.run();
        // Oversold varies fastest: (20, 30), (40, 30), (20, 70), (40, 70); (40, 30) is refused.
        let level = |trial: &Trial, id: &str| match trial.inputs.iter().find(|i| i.id == id) {
            Some(StudyInput {
                value: Number(value),
                ..
            }) => *value,
            _ => panic!("{name}: {id}"),
        };
        let mut pairs: Vec<_> = h
            .state()
            .trials
            .iter()
            .map(|trial| (level(trial, "oversold"), level(trial, "overbought")))
            .collect();
        pairs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(pairs, [(20.0, 30.0), (20.0, 70.0), (40.0, 70.0)], "{name}");
        h.get_by_label("1 skipped: oversold must be below overbought");
        h.state_mut().set_input("overbought", Number(95.0)).unwrap();
        assert!(
            h.state().skipped.is_empty() && h.state().trials.is_empty(),
            "{name}"
        );
    }
}
