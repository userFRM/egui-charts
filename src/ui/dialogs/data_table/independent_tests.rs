use super::*;
use crate::model::{Bar, BarData};
use egui::{Event, Id, LayerId, Order, Role};
use egui_kittest::{Harness, kittest::Queryable};

struct Host {
    dialogs: [DataTableDialog; 3],
    actions: Vec<(usize, DataTableAction)>,
    typed: Vec<String>,
}

// Select the actual control under the window showing this cutoff, not the first Copy/Close.
fn click(h: &mut Harness<'_, Host>, rows: usize, control: &str) {
    let count = format!("{rows} loaded bars");
    h.get_all_by_label("Chart data")
        .find(|w| w.query_by_label(&count).is_some())
        .expect("window for this chart")
        .get_by_label(control)
        .click_accesskit();
    h.step();
}

#[test]
fn regular_table_and_two_fixed_replays_keep_input_columns_and_exports_separate() {
    let time = |i| DateTime::UNIX_EPOCH + chrono::Duration::minutes(i);
    let charts = [None, Some(2), Some(3)].map(|cutoff| {
        let bars = (0..4)
            .map(|i| Bar::new(time(i), 10.0, 15.0, 9.0, 11.0 + i as f64, 100.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAME");
        chart.set_replay_time(cutoff.map(time));
        chart
    });
    let ids = [Id::unique("first replay"), Id::unique("second replay")];
    let fixed = |id| DataTableDialog::new().with_id(id).with_fixed_symbol();
    let mut dialogs = [DataTableDialog::new(), fixed(ids[0]), fixed(ids[1])];
    for dialog in &mut dialogs {
        dialog.open();
    }
    let mut h = Harness::builder()
        .with_size(vec2(3000.0, 1600.0))
        .with_context_setup(egui_extras::install_image_loaders)
        .build_ui_state(
            |ui, host: &mut Host| {
                for (i, (dialog, chart)) in host.dialogs.iter_mut().zip(&charts).enumerate() {
                    let action = dialog.show(ui.ctx(), chart);
                    if action != DataTableAction::None {
                        host.actions.push((i, action));
                    }
                }
                ui.input(|input| {
                    for event in &input.events {
                        if let Event::Text(text) = event {
                            host.typed.push(text.clone());
                        }
                    }
                });
            },
            Host {
                dialogs,
                actions: Vec::new(),
                typed: Vec::new(),
            },
        );
    h.run();
    assert_eq!(
        h.query_all_by_role_and_label(Role::Button, "SAME").count(),
        1
    );
    assert_eq!(
        h.query_all_by_role_and_label(Role::Label, "SAME").count(),
        2
    );
    h.ctx.move_to_top(LayerId::new(Order::Middle, ids[0]));
    h.step();
    h.event(Event::Text("Q".into()));
    h.step();
    assert_eq!(h.state().typed, ["Q"]);
    assert!(h.state().actions.is_empty());

    h.get_all_by_label("Chart data")
        .find(|w| w.query_by_label("2 loaded bars").is_some())
        .unwrap()
        .get_by_label("Columns")
        .click();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "volume").click();
    h.step();
    egui::Popup::close_all(&h.ctx);
    h.step();
    for (i, count) in [4, 2, 3].into_iter().enumerate() {
        click(&mut h, count, "Copy");
        let [egui::OutputCommand::CopyText(tsv)] = h.output().platform_output.commands.as_slice()
        else {
            panic!("expected the table's clipboard output");
        };
        let tsv = tsv.clone();
        click(&mut h, count, "Export CSV");
        let Some((owner, DataTableAction::Export(csv))) = h.state_mut().actions.pop() else {
            panic!("expected a CSV export");
        };
        assert_eq!(owner, i);
        assert_eq!(csv.trim_end().replace(',', "\t"), tsv);
        let mut lines = tsv.lines();
        let header = if count == 2 {
            "time\topen\thigh\tlow\tclose"
        } else {
            "time\topen\thigh\tlow\tclose\tvolume"
        };
        assert_eq!(lines.next(), Some(header));
        for minute in (0..count as i64).rev() {
            let cells: Vec<_> = lines.next().unwrap().split('\t').collect();
            assert_eq!(cells[0], time(minute).to_rfc3339());
            assert_eq!(cells[4].parse::<f64>().unwrap(), 11.0 + minute as f64);
        }
        assert_eq!(lines.next(), None);
    }
    click(&mut h, 2, "Close");
    assert_eq!(
        h.state_mut().actions.pop(),
        Some((1, DataTableAction::Close))
    );
    h.step();
    assert!(h.query_by_label("2 loaded bars").is_none());
    click(&mut h, 3, "Close");
    assert_eq!(
        h.state_mut().actions.pop(),
        Some((2, DataTableAction::Close))
    );
    h.run();
    h.event(Event::Text("Z".into()));
    h.step();
    assert_eq!(
        h.state_mut().actions.pop(),
        Some((0, DataTableAction::SearchSymbol("Z".into())))
    );
    assert_eq!(h.state().typed, ["Q"]);
}
