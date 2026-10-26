use super::*;
use crate::model::{StrategyExit, StrategyTrade};
use egui::{Role, Vec2};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn report_replacement_selection_export_and_clear() {
    let start = DateTime::UNIX_EPOCH;
    let mut report = StrategyReport::new("First run", "FIRST", Timeframe::Min1, 1000.0);
    let mut closed = StrategyTrade::new(StrategySide::Long, 1.0, start, 100.0);
    closed.exit = Some(StrategyExit::new(
        start + chrono::TimeDelta::minutes(1),
        110.0,
        10.0,
    ));
    let open = StrategyTrade::new(
        StrategySide::Short,
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
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let mut panel = StrategyReportPanel::default();
        panel.set_report(Some(report.clone()), &[]);
        let mut h = Harness::builder()
            .with_size(Vec2::new(1440.0, 900.0))
            .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
            .build_ui_state(
                |ui, (panel, actions): &mut (StrategyReportPanel, Vec<StrategyReportAction>)| {
                    let empty = panel.report().is_none();
                    let before = ui.cursor();
                    let action = panel.show(ui);
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
        h.get_by_role_and_label(Role::Tab, "List of trades").click();
        h.run();
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
        let replacement = StrategyReport::new("Second run", "SECOND", Timeframe::Day1, 2000.0);
        h.state_mut().0.set_report(Some(replacement), &[]);
        h.run();
        assert_eq!(h.state().0.report().unwrap().symbol, "SECOND");
        assert!(h.state().0.selected.is_none());
        assert!(matches!(h.state().0.tab, Tab::Summary));
        h.state_mut().0.set_report(None, &[]);
        h.run();
        assert!(h.state().0.report().is_none());
        assert!(
            h.query_by_role_and_label(Role::Button, "Export trades CSV")
                .is_none()
        );
    }
}
