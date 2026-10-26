//! # Basic Candlestick Chart
//!
//! Demonstrates the minimal setup needed to display an interactive candlestick
//! chart with `egui-charts`. This example:
//!
//! - Generates 200 synthetic OHLCV bars resembling realistic price action
//! - Creates a `Chart` widget directly with synthetic data
//! - Marks an entry price with a price line
//! - Renders the chart inside an `eframe` window with the dark theme
//!
//! Run with:
//! ```sh
//! cargo run --example basic_chart
//! ```

mod support;

use chrono::{TimeDelta, Utc};
use egui_charts::ChartType;
use egui_charts::model::{BarData, PriceLine, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::widget::{Chart, ChartEvent, SeriesId};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-charts: Basic Chart",
        options,
        Box::new(|_cc| Ok(Box::new(BasicChartApp::new()))),
    )
}

struct BasicChartApp {
    chart: Chart,
    last_click: Option<(chrono::DateTime<Utc>, f64)>,
}

impl BasicChartApp {
    fn new() -> Self {
        // Generate synthetic candlestick data
        let bars = support::sample_bars(200, 100.0, TimeDelta::hours(1), Utc::now());
        // Bar 150 is inside the 100 visible bars, so the line is on screen.
        let entry = PriceLine::new(bars[150].close, Theme::dark().ui.accent).with_title("Entry");
        let data = BarData::from_bars(bars);

        // Create the chart widget with data
        let mut chart = Chart::new(data);
        chart.set_chart_type(ChartType::Candles);
        chart.set_visible_bars(100);
        chart.set_symbol("SYNTH");
        chart.set_timeframe(Timeframe::Hour1);
        chart.set_price_lines(SeriesId::MAIN, vec![entry]);
        // Local axis sizing and palette, independent of the surrounding application's theme.
        let mut style = Theme::dark();
        style.text.label = 14.0;
        chart.set_theme_override(Some(style));

        Self {
            chart,
            last_click: None,
        }
    }
}

impl eframe::App for BasicChartApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Apply the dark theme to egui's visual system
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ui.ctx(), &theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ui.ctx().global_style());

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(self.last_click.map_or_else(
                || "Click the plot to inspect time and price".to_owned(),
                |(time, price)| format!("Clicked {time}: {price:.2}"),
            ));
            // Render the chart -- it handles pan, zoom, and crosshair automatically
            self.chart.show(ui);
            for event in self.chart.drain_events() {
                if let ChartEvent::Click { position, .. } = event {
                    self.last_click = Some((position.time, position.price));
                    ui.ctx().request_repaint();
                }
            }
        });
    }
}
