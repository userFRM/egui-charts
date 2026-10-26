//! A custom ten-bar price change, drawn in a separate pane and recalculated by the chart.
//!
//! Run with `cargo run --example custom_indicator`.

mod support;

use chrono::{TimeDelta, Utc};
use egui_charts::model::{BarData, Timeframe};
use egui_charts::studies::{CustomIndicator, IndicatorValue};
use egui_charts::{Chart, theme::Theme};

fn main() -> eframe::Result {
    eframe::run_native(
        "Custom indicator (sample data)",
        Default::default(),
        Box::new(|cc| {
            egui_charts::theme::apply_to_egui(&cc.egui_ctx, &Theme::dark());
            Ok(Box::new(CustomIndicatorApp::new()))
        }),
    )
}

struct CustomIndicatorApp {
    chart: Chart,
}

impl CustomIndicatorApp {
    fn new() -> Self {
        let bars = support::sample_bars(300, 100.0, TimeDelta::hours(1), Utc::now());
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Hour1);
        chart.add_indicator(Box::new(
            CustomIndicator::new("Price change (10)", |bars| {
                bars.iter()
                    .enumerate()
                    .map(|(i, bar)| {
                        i.checked_sub(10).map_or(IndicatorValue::None, |before| {
                            IndicatorValue::Single(bar.close - bars[before].close)
                        })
                    })
                    .collect()
            })
            .with_overlay(false)
            .with_color(Theme::dark().ui.accent)
            .with_description("Close minus the close ten bars earlier"),
        ));
        Self { chart }
    }
}

impl eframe::App for CustomIndicatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                "Custom study: close minus the close ten bars earlier. Pan or zoom the chart.",
            );
            self.chart.show(ui);
        });
    }
}
