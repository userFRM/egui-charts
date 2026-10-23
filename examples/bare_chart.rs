//! One candlestick chart with live updates, built from the `widget` feature alone:
//!
//! ```sh
//! cargo run --release --example bare_chart --no-default-features --features widget
//! ```
//!
//! The prices are generated sample data. `benches/bare_chart.rs` runs this example's
//! frame headlessly to measure its cost, and `benches/frame_time.rs` draws its sample bars.

use chrono::{TimeZone, Utc};
use egui_charts::model::{Bar, BarData};
use egui_charts::widget::{Chart, SeriesId};

fn main() -> eframe::Result {
    let mut app = BareChart::new();
    eframe::run_ui_native(
        "Bare chart (sample data)",
        Default::default(),
        move |ui, _| {
            app.ui(ui);
            ui.ctx().request_repaint(); // a live chart: one sample tick per frame
        },
    )
}

pub(crate) struct BareChart {
    chart: Chart,
    ticks: u64,
}

impl BareChart {
    pub(crate) fn new() -> Self {
        let mut chart = Chart::new(BarData::from_bars(
            (0..500).map(|i| sample_bar(i, 0)).collect(),
        ));
        chart.set_symbol("SAMPLE");
        Self { chart, ticks: 0 }
    }

    /// One frame: a sample tick moves the forming bar, and every 60th tick opens a new one.
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        self.ticks += 1;
        let n = self.chart.bars(SeriesId::MAIN).len();
        let bar = if self.ticks.is_multiple_of(60) {
            sample_bar(n, 0)
        } else {
            sample_bar(n - 1, self.ticks)
        };
        self.chart.update(SeriesId::MAIN, bar);
        egui::CentralPanel::default().show(ui, |ui| {
            self.chart.show(ui);
        });
    }
}

/// Sample bar `i`: a deterministic wave, not market data. `tick` moves its close while it forms.
pub(crate) fn sample_bar(i: usize, tick: u64) -> Bar {
    let t = i as f64;
    let open = 100.0 + 10.0 * (t / 40.0).sin() + 3.0 * (t / 7.0).sin();
    let close = open + 1.5 * (t * 1.7 + tick as f64 * 0.3).sin();
    let time = Utc.timestamp_opt(1_700_000_000 + 60 * i as i64, 0).unwrap();
    Bar::new(
        time,
        open,
        open.max(close) + 0.8,
        open.min(close) - 0.8,
        close,
        1000.0 + 500.0 * (t / 3.0).cos(),
    )
}
