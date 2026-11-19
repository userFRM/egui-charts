use super::*;
use crate::model::{EquityRange, StrategyExit, StrategyTrade};
use egui::{Role, Vec2};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn report_replacement_selection_export_and_clear() {
    let start = DateTime::UNIX_EPOCH + chrono::TimeDelta::milliseconds(250);
    let mut report = StrategyReport::new("First run", "FIRST", Timeframe::Min1, 1000.0);
    let mut closed = StrategyTrade::new(OrderSide::Buy, 1.0, start, 0.00001234);
    closed.exit = Some(StrategyExit::new(
        start + chrono::TimeDelta::minutes(1),
        98.92959251407622,
        10.0,
    ));
    let open = StrategyTrade::new(
        OrderSide::Sell,
        2.0,
        start + chrono::TimeDelta::minutes(2),
        110.0,
    );
    let mut invalid = closed.clone();
    invalid.quantity = 0.0;
    report.trades = vec![open, invalid, closed];
    report.equity = vec![
        (start, 1000.0),
        (start + chrono::TimeDelta::minutes(1), 1010.0),
    ];
    report.risk_free_rate = f64::MAX;
    for (theme, width) in [crate::theme::Theme::light(), crate::theme::Theme::dark()]
        .into_iter()
        .flat_map(|theme| [320.0, 1440.0].map(|width| (theme.clone(), width)))
    {
        let mut panel = StrategyReportPanel::default();
        panel.set_report(Some(report.clone()), &[]);
        let mut h = Harness::builder()
            .with_size(Vec2::new(width, 900.0))
            .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
            .build_ui_state(
                |ui, (panel, actions): &mut (StrategyReportPanel, Vec<StrategyReportAction>)| {
                    let empty = panel.report().is_none();
                    let before = ui.cursor();
                    let right = ui.available_rect_before_wrap().right();
                    let action = panel.show(ui);
                    assert!(
                        ui.min_rect().right() <= right + 1.0,
                        "report exceeds viewport"
                    );
                    if empty {
                        assert_eq!(ui.cursor(), before);
                    }
                    if action != StrategyReportAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            );
        h.run();
        h.get_by_role_and_label(Role::Button, "Export trades CSV")
            .click();
        h.run();
        assert_eq!(
            h.state().1,
            [StrategyReportAction::ExportCsv(report.trades_csv())]
        );
        h.get_by_role_and_label(Role::Button, "Export summary CSV")
            .click();
        h.run();
        assert_eq!(
            h.state().1.last(),
            Some(&StrategyReportAction::ExportSummaryCsv {
                filename: "First-run-FIRST-summary.csv".into(),
                content: report.summary_csv(&report.performance(), &report.comparison(&[])),
            })
        );
        h.get_by_role_and_label(Role::Tab, "List of trades").click();
        h.run();
        h.get_by_label("0.00001234");
        h.get_by_label("1970-01-01 00:00:00.250");
        assert!(h.get_by_label("98.92959251407622").rect().height() < 28.0);
        h.get_by_role_and_label(Role::Button, "1").click();
        h.run();
        assert_eq!(
            h.state().1.last(),
            Some(&StrategyReportAction::SelectTrade(StrategyTradeSelection {
                trade_index: 2,
                symbol: "FIRST".into(),
                timeframe: Timeframe::Min1,
                entry_time: start,
                exit_time: report.trades[2].exit.as_ref().map(|e| e.time),
            }))
        );
        h.get_by_label("Short").click();
        h.run();
        assert!(
            matches!(h.state().1.last(), Some(StrategyReportAction::SelectTrade(s)) if s.trade_index == 0 && s.exit_time.is_none())
        );
        h.get_by_role_and_label(Role::Tab, "Properties").click();
        h.run();
        assert!(h.query_by_label("inf%").is_none());
        let mut replacement = StrategyReport::new("Second run", "SECOND", Timeframe::Day1, 2000.0);
        replacement.unavailable = Some("Exchange rates from USD to EUR are unavailable".into());
        h.state_mut().0.set_report(Some(replacement), &[]);
        h.run();
        assert_eq!(h.state().0.report().unwrap().symbol, "SECOND");
        assert!(h.state().0.selected.is_none());
        assert!(matches!(h.state().0.tab, Tab::Summary));
        // A report without figures says why instead of showing zero metrics or exports.
        h.get_by_label("Exchange rates from USD to EUR are unavailable");
        assert!(h.query_by_label("Net profit").is_none());
        assert!(
            h.query_by_role_and_label(Role::Button, "Export summary CSV")
                .is_none()
        );
        h.state_mut().0.set_report(None, &[]);
        h.run();
        assert!(h.state().0.report().is_none());
        assert!(
            h.query_by_role_and_label(Role::Button, "Export trades CSV")
                .is_none()
        );
    }
}

#[test]
fn equity_curve_uses_the_full_width_at_every_time_resolution() {
    for step in [
        chrono::TimeDelta::nanoseconds(500),
        chrono::TimeDelta::milliseconds(500),
        chrono::TimeDelta::days(365 * 300),
    ] {
        let start = DateTime::UNIX_EPOCH;
        let values = [(start, 100.0), (start + step, 110.0)];
        let color = egui::Color32::GREEN;
        let mut h = Harness::builder()
            .with_size(Vec2::new(320.0, 200.0))
            .build_ui(|ui| plot::show(ui, &values, &[], color, 175.0, 1.0));
        h.run();
        let points = h
            .output()
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Path(path)
                    if path.stroke.color == egui::epaint::ColorMode::Solid(color) =>
                {
                    Some(&path.points)
                }
                _ => None,
            })
            .unwrap();
        assert!(points[1].x - points[0].x > 200.0, "{step}");
    }
}

#[test]
fn summary_lists_open_profit_and_charts_switch_to_percent() {
    let hour = |h| DateTime::UNIX_EPOCH + chrono::TimeDelta::hours(h);
    let mut report = StrategyReport::new("Held", "HELD", Timeframe::Hour1, 10_000.0);
    report.trades = vec![StrategyTrade::new(OrderSide::Buy, 100.0, hour(1), 100.0)];
    report.equity = vec![(hour(0), 10_000.0), (hour(1), 10_000.0), (hour(2), 9000.0)];
    report.equity_ranges = vec![
        EquityRange::new(10_000.0, 10_000.0, 0.0, 0.0),
        EquityRange::new(7000.0, 10_000.0, 3000.0, 3000.0),
        EquityRange::new(9000.0, 10_000.0, 1000.0, 0.0),
    ];
    let mut panel = StrategyReportPanel::default();
    panel.set_report(Some(report), &[]);
    let mut h = Harness::builder()
        .with_size(Vec2::new(1440.0, 1800.0))
        .build_ui_state(
            |ui, panel: &mut StrategyReportPanel| {
                panel.show(ui);
            },
            panel,
        );
    h.run();
    h.get_by_label("Open profit");
    h.get_by_label("-1000.00");
    let texts = |h: &Harness<'_, StrategyReportPanel>| -> Vec<String> {
        h.output()
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    };
    // Independently worked middle labels for equity [10000,10000,9000], drawdown [0,-3000,-1000]
    // and run-up [0,3000,3000]. Percent multiplies all three series by 100 / 10000.
    let money = ["9500.00", "-1500.00", "1500.00"];
    let percent = ["95.00", "-15.00", "15.00"];
    let has = |shown: &[String], label: &str| shown.iter().any(|t| t == label);
    let shown = texts(&h);
    assert!(
        money.iter().all(|l| has(&shown, l)) && !percent.iter().any(|l| has(&shown, l)),
        "{shown:?}"
    );
    h.get_by_role_and_label(Role::CheckBox, "Percent of initial capital")
        .click();
    h.run();
    let shown = texts(&h);
    assert!(
        percent.iter().all(|l| has(&shown, l)) && !money.iter().any(|l| has(&shown, l)),
        "{shown:?}"
    );
    let replacement = h.state().report().unwrap().clone();
    h.state_mut().set_report(Some(replacement), &[]);
    h.run();
    assert!(!h.state().percent);
    assert!(money.iter().all(|l| has(&texts(&h), l)));
}
