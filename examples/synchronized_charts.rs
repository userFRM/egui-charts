//! Synchronize UTC crosshair and view ranges between charts with different intervals.
//! The chart under the pointer leads; targets retain their own price scales.
//!
//! Run with `cargo run --example synchronized_charts`.

mod support;

use chrono::{TimeDelta, TimeZone, Utc};
use egui_charts::data::resample;
use egui_charts::model::{BarClock, BarData, Timeframe};
use egui_charts::widget::VisibleTimeRange;
use egui_charts::{Chart, theme::Theme};

fn main() -> eframe::Result {
    eframe::run_native(
        "Synchronized charts (sample data)",
        Default::default(),
        Box::new(|cc| {
            egui_charts::theme::apply_to_egui(&cc.egui_ctx, &Theme::dark());
            Ok(Box::new(SynchronizedChartsApp::new()))
        }),
    )
}

struct SynchronizedChartsApp {
    charts: [Chart; 2],
    last_source: Option<(usize, VisibleTimeRange)>,
    last_crosshair: Option<(usize, chrono::DateTime<Utc>)>,
}

impl SynchronizedChartsApp {
    fn new() -> Self {
        let end = Utc.with_ymd_and_hms(2026, 1, 19, 0, 0, 0).unwrap();
        let hourly = support::sample_bars(600, 100.0, TimeDelta::hours(1), end);
        let four_hour =
            resample(&hourly, Timeframe::Hour1, BarClock::new(Timeframe::Hour4)).unwrap();
        let make = |bars, timeframe| {
            let mut chart = Chart::new(BarData::from_bars(bars));
            chart.set_symbol("SAMPLE");
            chart.set_timeframe(timeframe);
            chart
        };
        Self {
            charts: [
                make(hourly, Timeframe::Hour1),
                make(four_hour, Timeframe::Hour4),
            ],
            last_source: None,
            last_crosshair: None,
        }
    }
}

impl eframe::App for SynchronizedChartsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                "SAMPLE: 1 hour / 4 hours. Hover either chart, then pan or zoom to synchronize.",
            );
            let mut source = None;
            ui.columns(2, |columns| {
                for (i, (chart, column)) in self.charts.iter_mut().zip(columns).enumerate() {
                    chart.show(column);
                    if column.rect_contains_pointer(column.max_rect()) {
                        source = Some(i);
                    }
                }
            });
            if let Some(i) = source {
                let time = self.charts[i].hover_time();
                let range = self.charts[i].visible_time_range();
                let crosshair = time.map(|time| (i, time));
                if crosshair != self.last_crosshair {
                    self.charts[i].set_synced_crosshair_time(None);
                    self.charts[1 - i].set_synced_crosshair_time(time);
                    self.last_crosshair = crosshair;
                    ui.ctx().request_repaint();
                }
                let current = range.map(|range| (i, range));
                if current != self.last_source {
                    if let Some(range) = range {
                        self.charts[1 - i].apply_synced_time_range(range);
                    }
                    self.last_source = current;
                    ui.ctx().request_repaint();
                }
            } else {
                if self.last_crosshair.take().is_some() {
                    for chart in &mut self.charts {
                        chart.set_synced_crosshair_time(None);
                    }
                    ui.ctx().request_repaint();
                }
                self.last_source = None;
            }
        });
    }
}
