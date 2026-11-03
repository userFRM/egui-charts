//! Two government yield curves, by maturity:
//!
//! ```sh
//! cargo run --example yield_curve
//! ```
//!
//! The horizontal axis is a [`MaturityScale`]. Actual samples are spaced linearly by maturity
//! without placeholder rows. The yields are generated sample data, not market data.

use egui_charts::model::{Bar, BarData};
use egui_charts::scales::{MaturityScale, key_time};
use egui_charts::widget::{Chart, SeriesOptions};
use std::sync::Arc;

/// Maturities in months, from one month to 30 years.
const MATURITIES: [f64; 13] = [
    1.0, 2.0, 3.0, 4.0, 6.0, 12.0, 24.0, 36.0, 60.0, 84.0, 120.0, 240.0, 360.0,
];

fn main() -> eframe::Result {
    let mut app = YieldCurve::new();
    eframe::run_ui_native(
        "Yield curve (sample data)",
        Default::default(),
        move |ui, _| app.ui(ui),
    )
}

pub(crate) struct YieldCurve {
    pub(crate) chart: Chart,
}

impl YieldCurve {
    pub(crate) fn new() -> Self {
        let bars = |curve: fn(f64) -> f64| {
            MATURITIES
                .iter()
                .filter_map(|&months| Some(Bar::from_value(key_time(months)?, curve(months))))
                .collect()
        };
        let mut chart = Chart::new(BarData::from_bars(bars(today)));
        chart.set_chart_type(egui_charts::ChartType::Line);
        chart.set_horizontal_scale(Some(Arc::new(MaturityScale::default())));
        chart.set_symbol("Today, yield %");
        chart.config_mut().show_volume = false;
        let previous = chart.add_series(SeriesOptions {
            name: "A year ago".into(),
            color: egui::Color32::from_rgb(242, 54, 69),
            ..Default::default()
        });
        chart.set_data(previous, bars(a_year_ago));
        // Three spacing units beyond the samples leave room for the right offset.
        chart.set_visible_bars(MATURITIES.len() + 3);
        Self { chart }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            self.chart.show(ui);
        });
    }
}

/// An inverted short end that rises again beyond five years.
fn today(months: f64) -> f64 {
    let years = months / 12.0;
    3.9 + 1.4 * (-years / 1.5).exp() + 0.012 * years
}

/// A normal, upward-sloping curve.
fn a_year_ago(months: f64) -> f64 {
    let years = months / 12.0;
    3.2 + 1.1 * (1.0 - (-years / 4.0).exp())
}
