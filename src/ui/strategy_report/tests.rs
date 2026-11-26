use super::*;
use crate::model::{EquityRange, StrategyExit, StrategyTrade};
use chrono::Timelike;
use egui::{Role, Vec2};
use egui_kittest::{Harness, kittest::Queryable};

/// The text of every label, in layout order.
fn labels<State>(h: &Harness<'_, State>) -> Vec<String> {
    h.query_all_by_role(Role::Label)
        .filter_map(|n| n.value())
        .collect()
}

/// The `cells` labels after the label `name`: the values of a summary row.
fn row<State>(h: &Harness<'_, State>, name: &str, cells: usize) -> Vec<String> {
    let labels = labels(h);
    let i = labels.iter().position(|l| l == name).expect(name);
    labels[i + 1..=i + cells].to_vec()
}

/// The local time in an export `filename` that starts with `prefix` and ends in `.csv`.
fn export_time(filename: &str, prefix: &str) -> NaiveDateTime {
    let stamp = filename
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(".csv"))
        .unwrap_or_else(|| panic!("{filename}"));
    NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H-%M-%S").unwrap()
}

#[test]
fn report_replacement_selection_export_and_clear() {
    let start = DateTime::UNIX_EPOCH + chrono::TimeDelta::milliseconds(250);
    let mut report = StrategyReport::new("First run/2", "EUR / USD", Timeframe::Min1, 1000.0);
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
        let before = chrono::Local::now()
            .naive_local()
            .with_nanosecond(0)
            .unwrap();
        h.get_by_role_and_label(Role::Button, "Export trades CSV")
            .click();
        h.run();
        let after = chrono::Local::now().naive_local();
        let [StrategyReportAction::ExportCsv { filename, content }] = h.state().1.as_slice() else {
            panic!("{:?}", h.state().1);
        };
        assert_eq!(content, &report.trades_csv());
        let time = export_time(filename, "First_run_2_EUR___USD_1min_trades_");
        assert!((before..=after).contains(&time), "{filename}");
        let before = chrono::Local::now()
            .naive_local()
            .with_nanosecond(0)
            .unwrap();
        h.get_by_role_and_label(Role::Button, "Export summary CSV")
            .click();
        h.run();
        let after = chrono::Local::now().naive_local();
        let Some(StrategyReportAction::ExportSummaryCsv { filename, content }) = h.state().1.last()
        else {
            panic!("{:?}", h.state().1);
        };
        assert_eq!(
            content,
            &report.summary_csv(&report.performance(), &report.comparison(&[]))
        );
        let time = export_time(filename, "First_run_2_EUR___USD_1min_summary_");
        assert!((before..=after).contains(&time), "{filename}");
        h.get_by_role_and_label(Role::Tab, "List of trades").click();
        h.run();
        h.get_by_label("0.00001234");
        h.get_by_label("1970-01-01 00:00:00.250");
        assert!(h.get_by_label("98.9295925140762").rect().height() < 28.0);
        h.get_by_role_and_label(Role::Button, "1").click();
        h.run();
        assert_eq!(
            h.state().1.last(),
            Some(&StrategyReportAction::SelectTrade(StrategyTradeSelection {
                trade_index: 2,
                symbol: "EUR / USD".into(),
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
        h.get_by_role_and_label(Role::Tab, "Properties")
            .scroll_to_me();
        h.run();
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
fn equity_curve_fills_the_width_with_distinct_time_labels() {
    let start = DateTime::UNIX_EPOCH;
    for (step, width) in [
        None,
        Some(chrono::TimeDelta::nanoseconds(500)),
        Some(chrono::TimeDelta::milliseconds(500)),
        Some(chrono::TimeDelta::days(365 * 300)),
    ]
    .into_iter()
    .flat_map(|step| [320.0, 1440.0].map(|width| (step, width)))
    {
        let values: Vec<_> = std::iter::once((start, 100.0))
            .chain(step.map(|step| (start + step, 110.0)))
            .collect();
        let color = egui::Color32::GREEN;
        let mut h = Harness::builder()
            .with_size(Vec2::new(width, 200.0))
            .build_ui(|ui| plot::show(ui, &values, &[], color, 175.0, 1.0, &[], |_, _| {}));
        h.run();
        let texts: Vec<_> = h
            .output()
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect();
        let distinct: std::collections::BTreeSet<_> = texts.iter().collect();
        assert_eq!(distinct.len(), texts.len(), "{texts:?}");
        let Some(step) = step else {
            // A single instant has no width to fill: one label gives its date.
            assert!(texts.contains(&"1970-01-01 00:00"), "{texts:?}");
            continue;
        };
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
fn margin_figures_and_chart_follow_margin_data() {
    let hour = |h| DateTime::UNIX_EPOCH + chrono::TimeDelta::hours(h);
    let mut plain = StrategyReport::new("Leveraged", "LEV", Timeframe::Hour1, 1000.0);
    let mut trade = StrategyTrade::new(OrderSide::Buy, 15.0, hour(2), 100.0);
    trade.exit = Some(StrategyExit::new(hour(3), 200.0 / 3.0, -500.0));
    trade.label = Some("A / Margin call".into());
    plain.trades = vec![trade];
    let curve = |values: [f64; 4]| {
        values
            .into_iter()
            .enumerate()
            .map(|(i, value)| (hour(i as i64), value))
            .collect()
    };
    plain.equity = curve([1000.0, 1000.0, 1000.0, 500.0]);
    let mut leveraged = plain.clone();
    leveraged.margin = curve([0.0, 0.0, 750.0, 0.0]);
    leveraged.peak_margin = curve([0.0, 0.0, 750.0, 750.0]);
    leveraged.margin_calls = vec![(hour(3), 15.0)];
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let mut unavailable = Vec::new();
        for (report, with_margin) in [(&leveraged, true), (&plain, false)] {
            let mut panel = StrategyReportPanel::default();
            panel.set_report(Some(report.clone()), &[]);
            let theme = theme.clone();
            let color = theme.ui.info;
            let mut h = Harness::builder()
                .with_size(Vec2::new(1440.0, 1600.0))
                .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
                .build_ui_state(
                    |ui, panel: &mut StrategyReportPanel| {
                        panel.show(ui);
                    },
                    panel,
                );
            h.run();
            h.get_by_label("Margin calls");
            if with_margin {
                // Peak 750 + drawdown 500; average (0 + 0 + 750 + 0) / 4; -500 / 1250.
                h.get_by_label("1250.00");
                h.get_by_label("187.50");
                h.get_by_label("-40.00%");
            }
            assert_eq!(h.query_by_label("Margin used").is_some(), with_margin);
            unavailable.push(h.query_all_by_label("N/A").count());
            let paths = h
                .output()
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Path(path)
                    if path.stroke.color == egui::epaint::ColorMode::Solid(color))
                })
                .count();
            assert_eq!(paths > 0, with_margin);
        }
        assert_eq!(unavailable[1], unavailable[0] + 8);
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
    // Beside each chart: its largest and latest value and the summary's average phase.
    h.get_by_label("Maximum 3000.00, average 1000.00, current 1000.00");
    h.get_by_label("Maximum 3000.00, average N/A, current 3000.00");
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
    h.get_by_label("Maximum 30.00, average 10.00, current 10.00");
    h.get_by_label("Maximum 30.00, average N/A, current 30.00");
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

#[test]
fn range_limits_summary_charts_and_trades() {
    use chrono::TimeZone;
    let day = |m, d| Utc.with_ymd_and_hms(2024, m, d, 0, 0, 0).unwrap();
    let mut report = StrategyReport::new("Range", "SAMPLE", Timeframe::Min1, 1000.0);
    report.equity = vec![
        (day(1, 1), 1000.0),
        (day(2, 1), 1200.0),
        (day(3, 1), 900.0),
        (day(4, 1), 1000.0),
    ];
    report.equity_ranges = vec![
        crate::model::EquityRange::new(1000.0, 1000.0, 0.0, 0.0),
        crate::model::EquityRange::new(950.0, 1250.0, 50.0, 300.0),
        crate::model::EquityRange::new(800.0, 1200.0, 400.0, 100.0),
        crate::model::EquityRange::new(850.0, 1100.0, 100.0, 250.0),
    ];
    report.margin = vec![
        (day(1, 1), 0.0),
        (day(2, 1), 100.0),
        (day(3, 1), 200.0),
        (day(4, 1), 0.0),
    ];
    report.peak_margin = vec![
        (day(1, 1), 0.0),
        (day(2, 1), 300.0),
        (day(3, 1), 250.0),
        (day(4, 1), 0.0),
    ];
    report.margin_calls = vec![
        (day(1, 10), 5.0),
        (day(2, 15), 2.0),
        (day(3, 1), 1.0),
        (day(4, 2), 10.0),
    ];
    for (entry, exit, profit) in [
        (day(1, 10), day(1, 20), 200.0),
        (day(2, 10), day(2, 20), -300.0),
    ] {
        let mut t = StrategyTrade::new(OrderSide::Buy, 1.0, entry, 100.0);
        t.exit = Some(StrategyExit::new(exit, 100.0, profit));
        report.trades.push(t);
    }
    // Daily bars at a constant price: each trade holds 10 of them.
    let bars: Vec<_> = (0..92)
        .map(|d| {
            let time = day(1, 1) + chrono::TimeDelta::days(d);
            Bar::new(time, 100.0, 100.0, 100.0, 100.0, 1.0)
        })
        .collect();
    for (theme, width) in [crate::theme::Theme::light(), crate::theme::Theme::dark()]
        .into_iter()
        .flat_map(|theme| [320.0, 1440.0].map(|width| (theme.clone(), width)))
    {
        let accent = theme.ui.accent;
        let mut panel = StrategyReportPanel::default();
        panel.set_report(Some(report.clone()), &bars);
        let mut h = Harness::builder()
            .with_size(Vec2::new(width, 900.0))
            .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
            .build_ui_state(
                |ui, (panel, actions): &mut (StrategyReportPanel, Vec<StrategyReportAction>)| {
                    let right = ui.available_rect_before_wrap().right();
                    let action = panel.show(ui);
                    assert!(
                        ui.min_rect().right() <= right + 1.0,
                        "report exceeds viewport"
                    );
                    if action != StrategyReportAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            );
        h.run();
        assert_eq!(row(&h, "Net return", 3), ["-10.00%", "-10.00%", "0.00%"]);
        assert_eq!(
            row(&h, "Maximum drawdown % (close-to-close)", 1),
            ["25.00%"]
        );
        // The open drawdown from 1 February lasts to the curve's end on 1 April.
        assert_eq!(
            row(&h, "Average drawdown duration in days (close-to-close)", 1),
            ["60.00"]
        );
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "From (UTC)")
                .value()
                .as_deref(),
            Some("2024-01-01")
        );
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "To (UTC)")
                .value()
                .as_deref(),
            Some("2024-04-01")
        );
        // The whole run closes 200 and then -300: the last closed trade stands at -100.
        h.get_by_role_and_label(Role::Tab, "List of trades").click();
        h.run();
        let cells = labels(&h);
        let at = |text: &str| cells.iter().position(|l| l == text).expect(text);
        assert_eq!(at("Cumulative profit"), at("Net profit") + 1);
        assert_eq!(at("-100.00"), at("-300.00") + 1);
        // To 15 February the second trade is still open in the range: only the first adds profit.
        h.get_by_role_and_label(Role::TextInput, "To (UTC)").click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.get_by_role_and_label(Role::TextInput, "To (UTC)")
            .type_text("2024-02-15");
        h.run();
        let cells = labels(&h);
        let count = |text: &str| cells.iter().filter(|l| *l == text).count();
        assert_eq!(
            [count("200.00"), count("-300.00"), count("-100.00")],
            [2, 1, 0]
        );
        h.get_by_role_and_label(Role::Button, "Whole run").click();
        h.run();
        h.get_by_role_and_label(Role::Tab, "Summary").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "From (UTC)")
            .click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.get_by_role_and_label(Role::TextInput, "From (UTC)")
            .type_text("2024-02-15");
        h.run();
        assert_eq!(row(&h, "Net return", 3), ["-33.33%", "-33.33%", "0.00%"]);
        assert_eq!(row(&h, "Average run-up (close-to-close)", 1), ["100.00"]);
        h.get_by_label("Maximum 400.00, average N/A, current 350.00");
        let data = h.state().0.data.as_ref().unwrap();
        assert_eq!(data.margin, report.margin[2..]);
        assert_eq!(data.shown.average_margin, Some(100.0));
        assert_eq!(data.shown.largest_margin, Some(250.0));
        assert_eq!(data.shown.margin_calls, Some(2));
        assert_eq!(data.shown.liquidated_quantity, Some(3.0));
        assert_eq!(data.report, report);
        assert_eq!(data.initial_capital, 900.0);
        assert_eq!(data.shown.intrabar_max_drawdown, Some(400.0));
        assert_eq!(data.shown.intrabar_max_run_up, Some(300.0));
        // The trade entered before the range counts its bars from entry, as in the whole run.
        assert_eq!(
            row(&h, "Average bars in trades", 3),
            ["10.00", "10.00", "N/A"]
        );
        assert!(h.query_by_label("-10.00%").is_none());
        assert!(h.query_by_label("25.00%").is_none());
        let points = h
            .output()
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Path(path)
                    if path.stroke.color == egui::epaint::ColorMode::Solid(accent) =>
                {
                    Some(&path.points)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(points.len(), 2);
        assert!(points[1].x - points[0].x > 200.0);
        h.get_by_role_and_label(Role::CheckBox, "Percent of initial capital")
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::CheckBox, "Percent of initial capital")
            .click();
        h.run();
        // Equity 900..1000 has midpoint 950: 950 / the ranged capital 900 * 100.
        assert!(h.output().shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "105.56")
        }));
        h.get_by_role_and_label(Role::Tab, "List of trades").click();
        h.run();
        // From 15 February the list starts at the second trade, and so does its total.
        assert_eq!(labels(&h).iter().filter(|l| *l == "-300.00").count(), 2);
        h.get_by_role_and_label(Role::Button, "1").click();
        h.run();
        assert!(h.query_by_role_and_label(Role::Button, "2").is_none());
        assert!(
            matches!(h.state().1.last(), Some(StrategyReportAction::SelectTrade(s)) if s.trade_index == 1)
        );
        h.get_by_role_and_label(Role::Tab, "Summary").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "From (UTC)")
            .click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.get_by_role_and_label(Role::TextInput, "From (UTC)")
            .type_text("2024-04-02");
        h.run();
        h.get_by_label("Enter dates as YYYY-MM-DD, the start not after the end");
        assert_eq!(row(&h, "Net return", 3), ["-33.33%", "-33.33%", "0.00%"]);
        for label in ["Export trades CSV", "Export summary CSV"] {
            h.get_by_role_and_label(Role::Button, label).click();
            h.run();
        }
        let exports = &h.state().1[1..];
        assert!(matches!(
            exports,
            [
                StrategyReportAction::ExportCsv { .. },
                StrategyReportAction::ExportSummaryCsv { .. }
            ]
        ));
        let contents: Vec<&str> = exports
            .iter()
            .map(|action| match action {
                StrategyReportAction::ExportCsv { content, .. }
                | StrategyReportAction::ExportSummaryCsv { content, .. } => content.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            contents,
            [
                report
                    .trades_csv_with_excursions(&report.comparison(&bars))
                    .as_str(),
                report
                    .summary_csv(&report.performance(), &report.comparison(&bars))
                    .as_str(),
            ]
        );
        h.get_by_role_and_label(Role::TextInput, "To (UTC)").click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.get_by_role_and_label(Role::TextInput, "To (UTC)")
            .type_text("2024-05-01");
        h.run();
        assert!(h.state().0.data.as_ref().unwrap().equity.is_empty());
        assert!(!h.state().0.percent);
        h.get_by_role_and_label(Role::Button, "Whole run").click();
        h.run();
        assert_eq!(row(&h, "Net return", 3), ["-10.00%", "-10.00%", "0.00%"]);
        let data = h.state().0.data.as_ref().unwrap();
        assert_eq!(data.margin, report.margin);
        assert_eq!(data.shown, report.performance());
        assert_eq!(
            h.get_by_role_and_label(Role::TextInput, "From (UTC)")
                .value()
                .as_deref(),
            Some("2024-01-01")
        );
        let mut unavailable = StrategyReport::new("Unavailable", "SAMPLE", Timeframe::Day1, 1000.0);
        unavailable.unavailable = Some("Exchange rate needed".into());
        for (run, messages) in [
            (
                report.clone(),
                ["Months", "Winning and losing runs", "Monday"],
            ),
            (
                StrategyReport::new("Empty", "EMPTY", Timeframe::Day1, 1000.0),
                [
                    "No equity or closed trades",
                    "No closed trades",
                    "No closed trades",
                ],
            ),
            (unavailable, ["Exchange rate needed"; 3]),
        ] {
            let available = run.unavailable.is_none();
            h.state_mut().0.set_report(Some(run), &[]);
            h.run();
            for (label, message) in ["Periods", "Distribution", "Trade times"]
                .into_iter()
                .zip(messages)
            {
                h.get_by_role_and_label(Role::Tab, label).scroll_to_me();
                h.run();
                h.get_by_role_and_label(Role::Tab, label).click();
                h.run();
                h.get_by_label(message);
                if !available {
                    for text in ["No equity or closed trades", "No closed trades"] {
                        assert!(h.query_by_label(text).is_none());
                    }
                    assert!(
                        h.query_by_role_and_label(Role::TextInput, "From (UTC)")
                            .is_none()
                    );
                    for label in ["Export trades CSV", "Export summary CSV"] {
                        assert!(h.query_by_role_and_label(Role::Button, label).is_none());
                    }
                }
            }
        }
    }
}

#[cfg(feature = "strategies")]
#[test]
fn continuation_keeps_the_range_and_refreshes_its_figures() {
    let day = |d| DateTime::UNIX_EPOCH + TimeDelta::days(d);
    let bars: Vec<_> = (0..5)
        .map(|d| Bar::new(day(d), 100.0, 100.0, 100.0, 100.0, 1.0))
        .collect();
    let mut report = StrategyReport::new("Continued", "TEST", Timeframe::Day1, 1000.0);
    report.trades = vec![StrategyTrade::new(OrderSide::Buy, 1.0, day(1), 100.0)];
    report.equity = vec![(day(0), 1000.0), (day(1), 1000.0)];
    let mut panel = StrategyReportPanel::default();
    panel.set_report(Some(report.clone()), &bars[..2]);
    let range = day(1)..day(3);
    panel.data.as_mut().unwrap().select(Some(range.clone()));
    panel.range_text = ["1970-01-02".into(), "1970-01-03".into()];
    panel.tab = Tab::Periods;
    panel.compare = true;
    panel.selected = Some(0);

    report.trades[0].exit = Some(StrategyExit::new(day(2), 110.0, 10.0));
    report
        .trades
        .push(StrategyTrade::new(OrderSide::Sell, 1.0, day(3), 100.0));
    report
        .equity
        .extend([(day(2), 1010.0), (day(3), 1020.0), (day(4), 1030.0)]);
    panel.continue_report(report.clone(), &bars);

    assert_eq!(panel.report(), Some(&report));
    assert_eq!(panel.tab, Tab::Periods);
    assert!(panel.compare);
    assert_eq!(panel.selected, Some(0));
    assert_eq!(panel.range_text, ["1970-01-02", "1970-01-03"]);
    let data = panel.data.as_ref().unwrap();
    assert_eq!(data.range, Some(range));
    assert_eq!(data.equity, [(day(1), 1000.0), (day(2), 1010.0)]);
    assert_eq!(data.trades, [0]);
    assert_eq!(data.shown.all.net_profit, 10.0);
    assert_eq!(data.analysis.months[0].trades.closed_trades, 1);
    assert_eq!(data.shown_comparison.buy_and_hold_equity.len(), 2);

    panel.set_report(Some(report), &bars);
    assert!(panel.data.as_ref().unwrap().range.is_none());
    assert_eq!(panel.range_text, ["1970-01-01", "1970-01-05"]);
    assert_eq!(panel.tab, Tab::Summary);
    assert!(panel.selected.is_none());
}

/// Fill prices read at the run's price increment, or finer where exact, and quantities
/// without binary rounding digits, in the list of trades, the trade markers and the summary.
#[cfg(feature = "strategies")]
#[test]
fn trades_and_markers_show_fills_without_rounding_digits() {
    use crate::model::OrderType;
    use crate::strategies::{Backtest, OrderSize, Slippage, Strategy, StrategyContext};
    struct InThenOut;
    impl Strategy for InThenOut {
        fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
            match context.bars().len() {
                1 => context.entry("In", OrderSide::Buy, None, OrderType::Market),
                2 => context.exit("Out", None, None, None),
                _ => {}
            }
        }
    }
    let minute = |m| DateTime::UNIX_EPOCH + TimeDelta::minutes(m);
    let bars: Vec<_> = [100.0, 100.1, 101.02]
        .into_iter()
        .enumerate()
        .map(|(i, price)| Bar::new(minute(i as i64), price, price, price, price, 1.0))
        .collect();
    let mut engine = Backtest::new("Digits", "TEST", Timeframe::Min1, 10_000.0);
    engine.settings.slippage = Slippage::Increments(2);
    engine.settings.order_size = OrderSize::EquityFraction(0.5);
    let report = engine.run(&mut InThenOut, &bars, &[]);
    // 100.1 + 2 * 0.01, 101.02 - 2 * 0.01 and 10_000 * 0.5 / 100.12, as binary arithmetic gives them.
    let trade = &report.trades[0];
    assert_eq!(trade.entry_price, 100.11999999999999);
    assert_eq!(trade.exit.as_ref().unwrap().price, 101.0);
    assert_eq!(trade.quantity, 49.94007191370356);
    let tooltips: Vec<_> = report
        .signals()
        .into_iter()
        .filter_map(|s| s.tooltip)
        .collect();
    assert!(
        tooltips[0].contains("49.94007191 units @ 100.12"),
        "{tooltips:?}"
    );
    assert!(
        tooltips[1].contains("49.94007191 units @ 101.00"),
        "{tooltips:?}"
    );
    let mut panel = StrategyReportPanel::default();
    panel.set_report(Some(report), &bars);
    panel.tab = Tab::Trades;
    let mut h = Harness::builder()
        .with_size(Vec2::new(1440.0, 600.0))
        .build_ui_state(
            |ui, panel: &mut StrategyReportPanel| {
                panel.show(ui);
            },
            panel,
        );
    h.run();
    for cell in ["49.94007191", "100.12", "101.00"] {
        h.get_by_label(cell);
    }
    // The summary's largest position writes the quantity as the list does.
    h.state_mut().tab = Tab::Summary;
    h.run();
    assert!(h.query_all_by_label("49.94007191").count() > 0);
}

#[test]
fn distribution_zero_edge_stays_centred_for_extreme_finite_returns() {
    let mut report = StrategyReport::new("Extremes", "TEST", Timeframe::Min1, 1000.0);
    for profit in [-f64::MAX, f64::MAX] {
        let mut trade = StrategyTrade::new(OrderSide::Buy, 1.0, DateTime::UNIX_EPOCH, 1.0);
        trade.exit = Some(StrategyExit::new(DateTime::UNIX_EPOCH, 1.0, profit));
        report.trades.push(trade);
    }
    // Width 2e307 gives bins -9 through 8: zero is edge 9 of 18, at the centre.
    // The first lower edge, -1.8e308, cannot be represented as a finite f64.
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let (win, loss) = (theme.ui.success, theme.ui.error);
        let mut panel = StrategyReportPanel::default();
        panel.set_report(Some(report.clone()), &[]);
        panel.tab = Tab::Distribution;
        let mut h = Harness::builder()
            .with_size(Vec2::new(320.0, 400.0))
            .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
            .build_ui_state(
                |ui, panel: &mut StrategyReportPanel| {
                    panel.show(ui);
                },
                panel,
            );
        h.run();
        let shapes = &h.output().shapes;
        let zero = shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "0.00%" => {
                    Some(text.pos.x + text.galley.size().x / 2.0)
                }
                _ => None,
            })
            .expect("the zero edge is labelled");
        let mut left = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        for shape in shapes {
            if let egui::Shape::Rect(rect) = &shape.shape
                && (rect.fill == win || rect.fill == loss)
            {
                left = left.min(rect.rect.left());
                right = right.max(rect.rect.right());
            }
        }
        assert!(zero.is_finite() && (zero - (left + right) / 2.0).abs() < 1.0);
    }
}

#[test]
fn summary_shows_trade_figures_for_all_long_and_short() {
    let start = DateTime::UNIX_EPOCH;
    let mut report = StrategyReport::new("Sides", "SIDES", Timeframe::Hour1, 1000.0);
    for (side, profit) in [(OrderSide::Buy, 50.25), (OrderSide::Sell, -30.5)] {
        let mut trade = StrategyTrade::new(side, 1.0, start, 100.0);
        trade.exit = Some(StrategyExit::new(
            start + chrono::TimeDelta::hours(1),
            100.0,
            profit,
        ));
        report.trades.push(trade);
    }
    let mut panel = StrategyReportPanel::default();
    panel.set_report(Some(report), &[]);
    let mut h = Harness::builder()
        .with_size(Vec2::new(1440.0, 1200.0))
        .with_context_setup(|ctx| crate::theme::apply_to_egui(ctx, &crate::theme::Theme::light()))
        .build_ui_state(
            |ui, (panel, actions): &mut (StrategyReportPanel, Vec<StrategyReportAction>)| {
                let action = panel.show(ui);
                if action != StrategyReportAction::None {
                    actions.push(action);
                }
            },
            (panel, Vec::new()),
        );
    h.run();
    let labels = labels(&h);
    let header = labels.iter().position(|l| l == "All").unwrap();
    assert_eq!(&labels[header..header + 3], ["All", "Long", "Short"]);
    assert_eq!(row(&h, "Net profit", 3), ["19.75", "50.25", "-30.50"]);
    assert_eq!(
        row(&h, "Average losing trade", 3),
        ["30.50", "N/A", "30.50"]
    );
    h.get_by_role_and_label(Role::Button, "Export summary CSV")
        .click();
    h.run();
    let Some(StrategyReportAction::ExportSummaryCsv { content, .. }) = h.state().1.last() else {
        panic!("summary export action");
    };
    assert!(content.contains("summary,Net profit,50.25,money,,long\n"));
    assert!(content.contains("summary,Net profit,-30.5,money,,short\n"));
}

/// Daily equity 1000, 1010, 1010, 990, 990; a long of 1 at 100 from day 1 to day 2 (profit 10)
/// and a short of 2 at 110 from day 3 to day 4 (profit -20), with the bars that give them
/// MAE 5, MFE 15 and MAE 30, MFE 20.
fn two_trade_report() -> (StrategyReport, Vec<Bar>) {
    let day = |d| DateTime::UNIX_EPOCH + chrono::TimeDelta::days(d);
    let mut report = StrategyReport::new("Two trades", "PAIR", Timeframe::Day1, 1000.0);
    report.equity = [1000.0, 1010.0, 1010.0, 990.0, 990.0]
        .into_iter()
        .zip(0..)
        .map(|(value, d)| (day(d), value))
        .collect();
    for (side, quantity, entry, price, exit_price, profit) in [
        (OrderSide::Buy, 1.0, 1, 100.0, 110.0, 10.0),
        (OrderSide::Sell, 2.0, 3, 110.0, 120.0, -20.0),
    ] {
        let mut trade = StrategyTrade::new(side, quantity, day(entry), price);
        trade.exit = Some(StrategyExit::new(day(entry + 1), exit_price, profit));
        report.trades.push(trade);
    }
    let bars = [
        [100.0, 100.0, 100.0, 100.0],
        [100.0, 115.0, 95.0, 110.0],
        [110.0, 110.0, 110.0, 110.0],
        [110.0, 125.0, 100.0, 120.0],
        [120.0, 120.0, 120.0, 120.0],
    ]
    .into_iter()
    .zip(0..)
    .map(|([open, high, low, close], d)| Bar::new(day(d), open, high, low, close, 1.0))
    .collect();
    (report, bars)
}

/// A report panel harness in `theme`, tall enough to show the whole summary.
fn summary_harness(
    report: StrategyReport,
    bars: &[Bar],
    theme: crate::theme::Theme,
) -> Harness<'static, StrategyReportPanel> {
    let mut panel = StrategyReportPanel::default();
    panel.set_report(Some(report), bars);
    let mut h = Harness::builder()
        .with_size(Vec2::new(1440.0, 1800.0))
        .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
        .build_ui_state(
            |ui, panel: &mut StrategyReportPanel| {
                panel.show(ui);
            },
            panel,
        );
    h.run();
    h
}

fn type_from<State>(h: &mut Harness<'_, State>, date: &str) {
    h.get_by_role_and_label(Role::TextInput, "From (UTC)")
        .click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_by_role_and_label(Role::TextInput, "From (UTC)")
        .type_text(date);
    h.run();
}

#[test]
fn equity_tooltip_names_the_last_closed_trade() {
    let (report, bars) = two_trade_report();
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let accent = theme.ui.accent;
        let mut h = summary_harness(report.clone(), &bars, theme);
        let hover = |h: &mut Harness<'_, StrategyReportPanel>, i: usize| {
            let points = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(path)
                        if path.stroke.color == egui::epaint::ColorMode::Solid(accent) =>
                    {
                        Some(path.points.clone())
                    }
                    _ => None,
                })
                .unwrap();
            let y = points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32;
            h.hover_at(egui::pos2(points[i].x, y));
            h.run_steps(90);
        };
        hover(&mut h, 4);
        h.get_by_label("Value: 990.00 · 1970-01-05 00:00:00 UTC");
        h.get_by_label("Trade 2: Short");
        // 10 from the long, then -20 from the short.
        h.get_by_label("Cumulative profit: -10.00");
        // Between the trades the long is the last closed.
        hover(&mut h, 3);
        h.get_by_label("Trade 1: Long");
        h.get_by_label("Cumulative profit: 10.00");
        // Before the first close no trade is named.
        hover(&mut h, 1);
        h.get_by_label("Value: 1010.00 · 1970-01-02 00:00:00 UTC");
        assert!(h.query_by_label_contains("Trade 1").is_none());
        // From day 3 the range lists only the short, as its first trade.
        type_from(&mut h, "1970-01-04");
        hover(&mut h, 1);
        h.get_by_label("Trade 1: Short");
        h.get_by_label("Cumulative profit: -20.00");
    }
}

#[test]
fn trade_marks_follow_excursions_range_and_percent() {
    let (report, bars) = two_trade_report();
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let label = "Trade run-up and drawdown";
        let h = summary_harness(report.clone(), &[], theme.clone());
        assert!(h.query_by_role_and_label(Role::CheckBox, label).is_none());
        let [rise, fall] = [theme.ui.success, theme.ui.error].map(|c| c.gamma_multiply(0.35));
        let mut h = summary_harness(report.clone(), &bars, theme);
        // Run-up and drawdown rectangles, each sorted by left edge.
        let marks = |h: &Harness<'_, StrategyReportPanel>| {
            [rise, fall].map(|fill| {
                let mut rects: Vec<_> = h
                    .output()
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
                rects
            })
        };
        let heights = |rects: &[egui::Rect]| rects.iter().map(|r| r.height()).collect::<Vec<_>>();
        let near = |actual: Vec<f32>, expected: &[f32]| {
            assert_eq!(actual.len(), expected.len(), "{actual:?}");
            for (a, e) in actual.iter().zip(expected) {
                assert!((a - e).abs() < 0.01, "{actual:?} != {expected:?}");
            }
        };
        assert_eq!(marks(&h), [vec![], vec![]]);
        h.get_by_role_and_label(Role::CheckBox, label)
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::CheckBox, label).click();
        h.run();
        // The axis spans 980 (1010 - 30) to 1030 (1010 + 20) over 143 px: 2.86 px per unit.
        let check = |h: &Harness<'_, StrategyReportPanel>| {
            let [ups, downs] = marks(h);
            near(heights(&ups), &[42.9, 57.2]);
            near(heights(&downs), &[14.3, 85.8]);
            // The long rests on 1000 before its entry and the short on 1010, 10 units higher.
            near(vec![ups[0].bottom() - ups[1].bottom()], &[28.6]);
            near(
                vec![
                    ups[0].bottom() - downs[0].top(),
                    ups[1].bottom() - downs[1].top(),
                ],
                &[0.0, 0.0],
            );
            // Each trade holds one day of four; the short enters two days after the long.
            near(
                vec![ups[1].width(), ups[1].left() - ups[0].left()],
                &[ups[0].width(), 2.0 * ups[0].width()],
            );
        };
        check(&h);
        h.get_by_role_and_label(Role::CheckBox, "Percent of initial capital")
            .click();
        h.run();
        check(&h);
        h.get_by_role_and_label(Role::CheckBox, "Percent of initial capital")
            .click();
        h.run();
        // From day 3 only the short shows, resting on the range's first equity of 990: 960 to 1010.
        type_from(&mut h, "1970-01-04");
        let [ups, downs] = marks(&h);
        near(heights(&ups), &[57.2]);
        near(heights(&downs), &[85.8]);
    }
}

#[test]
fn export_filenames_name_the_run_kind_and_second() {
    let t = NaiveDate::from_ymd_opt(2026, 11, 21)
        .unwrap()
        .and_hms_opt(14, 3, 7)
        .unwrap();
    let (long, long_expected) = (
        "a".repeat(70),
        format!("{}_S_90min_trades_2026-11-21_14-03-07.csv", "a".repeat(64)),
    );
    let minutes_90 = Timeframe::Custom(std::num::NonZeroU64::new(5400).unwrap());
    let run = "First run/2";
    let pair = "EUR / USD";
    for (name, symbol, timeframe, kind, seconds, expected) in [
        (
            run,
            pair,
            Timeframe::Min1,
            "trades",
            0,
            "First_run_2_EUR___USD_1min_trades_2026-11-21_14-03-07.csv",
        ),
        (
            run,
            pair,
            Timeframe::Min1,
            "trades",
            1,
            "First_run_2_EUR___USD_1min_trades_2026-11-21_14-03-08.csv",
        ),
        (
            run,
            pair,
            Timeframe::Hour1,
            "trades",
            0,
            "First_run_2_EUR___USD_1h_trades_2026-11-21_14-03-07.csv",
        ),
        (
            run,
            pair,
            Timeframe::Min1,
            "summary",
            0,
            "First_run_2_EUR___USD_1min_summary_2026-11-21_14-03-07.csv",
        ),
        (
            run,
            pair,
            Timeframe::Min1,
            "summary",
            1,
            "First_run_2_EUR___USD_1min_summary_2026-11-21_14-03-08.csv",
        ),
        (
            run,
            pair,
            Timeframe::Hour1,
            "summary",
            0,
            "First_run_2_EUR___USD_1h_summary_2026-11-21_14-03-07.csv",
        ),
        (
            "Ré_x-1",
            "",
            Timeframe::Day1,
            "summary",
            0,
            "R__x-1_1D_summary_2026-11-21_14-03-07.csv",
        ),
        (
            long.as_str(),
            "S",
            minutes_90,
            "trades",
            0,
            long_expected.as_str(),
        ),
    ] {
        let report = StrategyReport::new(name, symbol, timeframe, 1000.0);
        let time = t + TimeDelta::seconds(seconds);
        assert_eq!(
            export_filename(&report, kind, time),
            expected,
            "{name} {timeframe} {kind}"
        );
    }
}
