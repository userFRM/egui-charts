//! Host script evaluation connected to a chart through `CustomIndicator`.
//!
//! This example's host accepts only `close - close[N]`, with a positive lookback up to
//! 10,000 bars. Replace its evaluator with your own runtime; the library supplies no language.
//! Apply a script to replace its study; an invalid script leaves the current study intact.
//!
//! Run with `cargo run --example scripting`.

mod support;

use chrono::{TimeDelta, Utc};
use egui_charts::model::{BarData, Timeframe};
use egui_charts::studies::{CustomIndicator, IndicatorValue};
use egui_charts::widget::SeriesId;
use egui_charts::{Chart, theme::Theme};

fn compile_script(source: &str) -> Result<CustomIndicator, &'static str> {
    let lookback = source
        .trim()
        .strip_prefix("close - close[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|number| number.parse::<usize>().ok())
        .filter(|lookback| (1..=10_000).contains(lookback))
        .ok_or("Expected close - close[N], with N from 1 to 10000")?;
    Ok(CustomIndicator::new(source.trim(), move |bars| {
        bars.iter()
            .enumerate()
            .map(|(i, bar)| {
                i.checked_sub(lookback)
                    .map_or(IndicatorValue::None, |before| {
                        IndicatorValue::Single(bar.close - bars[before].close)
                    })
            })
            .collect()
    })
    .with_overlay(false)
    .with_color(Theme::dark().ui.accent))
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Host scripting (sample data)",
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
        let source = "close - close[10]".to_owned();
        let study = chart.add_indicator(Box::new(compile_script(&source).unwrap()));
        Self {
            chart,
            source,
            study,
            status: "Applied close - close[10]".into(),
        }
    }
}

impl eframe::App for ScriptingApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Example host language: close - close[N]. Edit N and apply to recalculate.");
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
                            self.status = format!("Applied {}", self.source.trim());
                        }
                        Err(error) => self.status = error.into(),
                    }
                }
            });
            ui.label(&self.status);
            self.chart.show(ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::{model::Bar, studies::Indicator};

    #[test]
    fn scripts_preserve_warmup_and_reject_invalid_lookbacks() {
        let bars: Vec<_> = [100.0, 103.0, 101.0]
            .into_iter()
            .map(|close| {
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH,
                    close,
                    close,
                    close,
                    close,
                    1.0,
                )
            })
            .collect();
        for (source, expected) in [
            (" close - close[1] ", [None, Some(3.0), Some(-2.0)]),
            ("close - close[2]", [None, None, Some(1.0)]),
            ("close - close[10000]", [None; 3]),
        ] {
            let mut study = compile_script(source).unwrap();
            study.calculate(&bars);
            for (i, value) in expected.into_iter().enumerate() {
                assert_eq!(study.values().value(0, i), value, "{source}, bar {i}");
            }
        }
        for source in [
            "",
            "close",
            "close - close[0]",
            "close - close[-1]",
            "close - close[10001]",
            "close - close[999999999999999999999999999999]",
            "close - close[1]extra",
        ] {
            assert!(compile_script(source).is_err(), "{source}");
        }
    }
}
