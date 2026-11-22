//! Scripts compiled into chart studies.
//!
//! Edit the script and apply it. The new study calculates off the chart, away from the UI
//! thread, and replaces the chart's study once its calculation ends without an error; a script
//! that fails to compile or to calculate leaves the current study and shows the error at its line
//! and column.
//!
//! Run with `cargo run --example scripting` from `crates/scripting`.

use chrono::{TimeDelta, Utc};
use egui_charts::model::{Bar, BarData, Timeframe};
use egui_charts::studies::{Calculation, CustomIndicator, Indicator, IndicatorFactory};
use egui_charts::widget::SeriesId;
use egui_charts::{Chart, theme::Theme};
use egui_charts_scripting::Script;

/// A script being checked before it replaces the chart's study.
#[derive(Debug, Default)]
struct Apply {
    pending: Option<(Script, CustomIndicator)>,
    status: String,
}

impl Apply {
    /// Compile `source` and start its check on `bars`.
    fn start(&mut self, source: &str, bars: &[Bar]) {
        match Script::compile("Script", source, &IndicatorFactory::new()) {
            Ok(script) => {
                let mut study = script.study();
                study.calculate(bars);
                self.pending = Some((script, study));
                self.status = "Calculating".into();
            }
            Err(error) => {
                self.pending = None;
                self.status = error.to_string();
            }
        }
    }

    /// Advance the check by one frame; the study that replaces the chart's once it ends cleanly.
    fn frame(&mut self, bars: &[Bar]) -> Option<CustomIndicator> {
        let (script, study) = self.pending.as_mut()?;
        match study.calculation() {
            Some(Calculation::Running(done)) => {
                study.update(bars, bars.len());
                self.status = format!("Calculating, {:.0}%", done * 100.0);
                None
            }
            Some(Calculation::Stopped(message)) => {
                self.status = message;
                self.pending = None;
                None
            }
            _ => match script.last_error() {
                Some(error) => {
                    self.status = error.to_string();
                    self.pending = None;
                    None
                }
                None => {
                    self.status = "Applied".into();
                    self.pending.take().map(|(_, study)| study)
                }
            },
        }
    }
}

/// `count` hourly bars of a smooth walk around 100, ending now.
fn sample_bars(count: usize) -> Vec<Bar> {
    let end = Utc::now();
    (0..count)
        .map(|i| {
            let open = 100.0 + (i as f64 * 0.11).sin() * 6.0 + i as f64 * 0.02;
            let close = 100.0 + ((i + 1) as f64 * 0.11).sin() * 6.0 + (i + 1) as f64 * 0.02;
            let time = end - TimeDelta::hours((count - i) as i64);
            Bar::new(
                time,
                open,
                open.max(close) + 0.4,
                open.min(close) - 0.4,
                close,
                1000.0,
            )
        })
        .collect()
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Scripted studies (sample data)",
        Default::default(),
        Box::new(|cc| {
            egui_charts::theme::apply_to_egui(&cc.egui_ctx, &Theme::dark());
            Ok(Box::new(ScriptingApp::new()))
        }),
    )
}

struct ScriptingApp {
    chart: Chart,
    bars: Vec<Bar>,
    source: String,
    study: Option<SeriesId>,
    apply: Apply,
}

impl ScriptingApp {
    fn new() -> Self {
        let bars = sample_bars(300);
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Hour1);
        let source = "plot(\"Difference\", close - close[10]);".to_owned();
        let mut apply = Apply::default();
        apply.start(&source, &bars);
        Self {
            chart,
            bars,
            source,
            study: None,
            apply,
        }
    }
}

impl eframe::App for ScriptingApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if let Some(study) = self.apply.frame(&self.bars) {
            if let Some(old) = self.study.take() {
                self.chart.remove_series(old);
            }
            self.study = Some(self.chart.add_indicator(Box::new(study)));
        }
        if self.apply.pending.is_some() {
            ui.ctx().request_repaint();
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let label = ui.label("Script");
            ui.add(
                egui::TextEdit::multiline(&mut self.source)
                    .code_editor()
                    .desired_rows(4)
                    .desired_width(ui.available_width()),
            )
            .labelled_by(label.id);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Apply script").clicked() {
                    self.apply.start(&self.source, &self.bars);
                }
                ui.label(&self.apply.status);
            });
            self.chart.show(ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run frames until the applied script's check ends, as the app does once a frame.
    fn applied(apply: &mut Apply, source: &str, bars: &[Bar]) -> Option<CustomIndicator> {
        apply.start(source, bars);
        while apply.pending.is_some() {
            if let Some(study) = apply.frame(bars) {
                return Some(study);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        None
    }

    #[test]
    fn applying_replaces_the_study_or_keeps_it_with_the_error() {
        let bars: Vec<_> = [100.0, 103.0, 101.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| {
                let time = chrono::DateTime::UNIX_EPOCH + TimeDelta::minutes(i as i64);
                Bar::new(time, close, close, close, close, 1.0)
            })
            .collect();
        let values = |study: &CustomIndicator| -> Vec<Option<f64>> {
            (0..3).map(|bar| study.values().value(0, bar)).collect()
        };
        let mut apply = Apply::default();

        let current = applied(&mut apply, "plot(\"Difference\", close - close[1]);", &bars)
            .expect("a valid script replaces the study");
        assert_eq!(values(&current), [None, Some(3.0), Some(-2.0)]);
        assert_eq!(apply.status, "Applied");

        assert!(applied(&mut apply, "plot(\"Difference\", close - close[);", &bars).is_none());
        assert!(
            apply.status.starts_with("line 1, column 34: "),
            "{}",
            apply.status
        );
        assert_eq!(values(&current), [None, Some(3.0), Some(-2.0)]);

        let current = applied(&mut apply, "plot(\"Difference\", close - close[2]);", &bars)
            .expect("a valid script replaces the study");
        assert_eq!(values(&current), [None, None, Some(1.0)]);
    }
}
