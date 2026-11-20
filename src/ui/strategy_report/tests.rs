use super::*;
use crate::model::{StrategyExit, StrategyTrade};
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
            .build_ui(|ui| plot::show(ui, &values, &[], color, 175.0));
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
        assert_eq!(row(&h, "Maximum drawdown %", 1), ["25.00%"]);
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
        h.get_by_role_and_label(Role::TextInput, "From (UTC)")
            .click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.get_by_role_and_label(Role::TextInput, "From (UTC)")
            .type_text("2024-02-15");
        h.run();
        assert_eq!(row(&h, "Net return", 3), ["-33.33%", "-33.33%", "0.00%"]);
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
        h.get_by_role_and_label(Role::Tab, "List of trades").click();
        h.run();
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
        assert_eq!(
            &h.state().1[1..],
            &[
                StrategyReportAction::ExportCsv(
                    report.trades_csv_with_excursions(&report.comparison(&bars))
                ),
                StrategyReportAction::ExportSummaryCsv {
                    filename: "Range-SAMPLE-summary.csv".into(),
                    content: report.summary_csv(&report.performance(), &report.comparison(&bars)),
                },
            ]
        );
        h.get_by_role_and_label(Role::Button, "Whole run").click();
        h.run();
        assert_eq!(row(&h, "Net return", 3), ["-10.00%", "-10.00%", "0.00%"]);
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
