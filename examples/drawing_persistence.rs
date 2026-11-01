//! Save and reload the chart's timestamped drawings as JSON, with errors reported in the UI.
//! The editable document stands in for the host's file or storage service.
//!
//! Run with `cargo run --example drawing_persistence`.

mod support;

use chrono::{TimeDelta, Utc};
use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
use egui_charts::model::{BarData, Timeframe};
use egui_charts::{Chart, theme::Theme};

fn main() -> eframe::Result {
    eframe::run_native(
        "Drawing persistence (sample data)",
        Default::default(),
        Box::new(|cc| {
            egui_charts::theme::apply_to_egui(&cc.egui_ctx, &Theme::dark());
            Ok(Box::new(DrawingPersistenceApp::new()))
        }),
    )
}

struct DrawingPersistenceApp {
    chart: Chart,
    json: String,
    status: String,
}

impl DrawingPersistenceApp {
    fn new() -> Self {
        let bars = support::sample_bars(200, 100.0, TimeDelta::hours(1), Utc::now());
        let mut line = Drawing::new(1, DrawingToolType::TrendLine);
        line.chart_points = vec![
            ChartPoint::new(120.0, bars[120].low),
            ChartPoint::new(180.0, bars[180].high),
        ];
        line.completed = true;
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Hour1);
        chart
            .drawings_mut()
            .add_drawing(line)
            .expect("a valid trend line");
        chart
            .drawings_mut()
            .set_active_tool(Some(DrawingToolType::TrendLine));
        Self {
            chart,
            json: String::new(),
            status: "Draw a line, save, edit the chart, then load to restore the saved drawings."
                .into(),
        }
    }
}

impl eframe::App for DrawingPersistenceApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save drawings").clicked() {
                    match self.chart.save_drawings().and_then(|saved| saved.to_json()) {
                        Ok(json) => {
                            self.json = json;
                            self.status =
                                "Saved timestamped drawings to the document below.".into();
                        }
                        Err(error) => self.status = error.to_string(),
                    }
                }
                if ui.button("Load drawings").clicked() {
                    self.status = match self.chart.load_drawings(&self.json) {
                        Ok(skipped) => {
                            format!("Loaded drawings; {} skipped: {skipped:?}", skipped.len())
                        }
                        Err(error) => format!("Load failed: {error}"),
                    };
                }
            });
            ui.label(&self.status);
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .show(ui, |ui| {
                    let label = ui.label("Saved drawing document (JSON)");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.json)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    )
                    .labelled_by(label.id);
                });
            self.chart.show(ui);
        });
    }
}
