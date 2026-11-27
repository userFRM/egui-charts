//! A script in the library's scripting language, compiled into a chart study.
//!
//! The language is the `egui-charts-scripting` package in `crates/scripting`, which builds this
//! example until the examples move: run it with
//! `cargo run --manifest-path crates/scripting/Cargo.toml --example scripted_study`.
//! Apply a script to replace its study; a script that fails to compile leaves the current study
//! and shows the error at its line and column. The package's own example,
//! `crates/scripting/examples/scripting.rs`, also waits for the new study's calculation and keeps
//! the current study when the script stops with a runtime error or at a limit.

mod support;

use chrono::{TimeDelta, Utc};
use egui_charts::model::{BarData, Timeframe};
use egui_charts::studies::{CustomIndicator, IndicatorFactory};
use egui_charts::widget::SeriesId;
use egui_charts::{Chart, theme::Theme};
use egui_charts_scripting::{Script, ScriptError};

fn compile_script(source: &str) -> Result<CustomIndicator, ScriptError> {
    Script::compile("Script", source, &IndicatorFactory::new()).map(|script| script.study())
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Scripted study (sample data)",
        Default::default(),
        Box::new(|cc| {
            egui_charts::theme::apply_to_egui(&cc.egui_ctx, &Theme::dark());
            Ok(Box::new(ScriptingApp::new()))
        }),
    )
}

struct ScriptingApp {
    chart: Chart,
    source: String,
    study: SeriesId,
    status: String,
}

impl ScriptingApp {
    fn new() -> Self {
        let bars = support::sample_bars(300, 100.0, TimeDelta::hours(1), Utc::now());
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Hour1);
        let source = "plot(\"Difference\", close - close[10]);".to_owned();
        let study = chart.add_indicator(Box::new(compile_script(&source).unwrap()));
        Self {
            chart,
            source,
            study,
            status: "Applied".into(),
        }
    }
}

impl eframe::App for ScriptingApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Edit the script and apply it to recalculate.");
            ui.horizontal_wrapped(|ui| {
                let label = ui.label("Script");
                ui.add(
                    egui::TextEdit::singleline(&mut self.source)
                        .desired_width(ui.available_width().min(280.0)),
                )
                .labelled_by(label.id);
                if ui.button("Apply script").clicked() {
                    match compile_script(&self.source) {
                        Ok(study) => {
                            self.chart.remove_series(self.study);
                            self.study = self.chart.add_indicator(Box::new(study));
                            self.status = "Applied".into();
                        }
                        Err(error) => self.status = error.to_string(),
                    }
                }
            });
            ui.label(&self.status);
            self.chart.show(ui);
        });
    }
}
