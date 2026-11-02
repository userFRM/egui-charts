use super::*;
use crate::{
    model::{Direction, OrderSide, SignalShape},
    strategies::Commission,
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
            assert_eq!(report.performance().net_profit, 1.0);
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
            assert_eq!(h.state().0.report().unwrap().performance().net_profit, -1.0);
            assert!(!h.state().0.is_stale());
            h.state_mut()
                .0
                .set_input("length", InputValue::Length(1))
                .unwrap();
            assert!(h.state().0.is_stale());
            h.run();
            h.get_by_role_and_label(Role::Button, "Run").click();
            h.run();
            let report = h.state().0.report().unwrap();
            assert_eq!(report.trades[0].side, OrderSide::Buy);
            assert_eq!(report.trades[0].entry_time, bars()[2].time);
            assert_eq!(
                report.performance().net_profit,
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
            request.lower_timeframe
        ),
        (
            "TEST",
            Timeframe::Min1,
            source[2].time,
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
    assert!((comparison.buy_and_hold_return.unwrap() - (101.0 / 102.0 - 1.0)).abs() < 1e-12);
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
        .start_job("TEST", Timeframe::Min1, bars(), Vec::new(), true)
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
