//! # Basic Candlestick Chart
//!
//! Demonstrates the minimal setup needed to display an interactive candlestick
//! chart with `egui-charts`. This example:
//!
//! - Generates 200 synthetic OHLCV bars resembling realistic price action
//! - Creates a `Chart` widget directly with synthetic data
//! - Renders the chart inside an `eframe` window with the dark theme
//!
//! Run with:
//! ```sh
//! cargo run --example basic_chart
//! ```

use chrono::{Duration, Utc};
use egui_charts::ChartType;
use egui_charts::model::{Bar, BarData};
use egui_charts::theme::Theme;
use egui_charts::widget::Chart;

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
}

impl BasicChartApp {
    fn new() -> Self {
        // Generate synthetic candlestick data
        let bars = generate_synthetic_bars(200, 100.0);
        let data = BarData::from_bars(bars);

        // Create the chart widget with data
        let mut chart = Chart::new(data);
        chart.set_chart_type(ChartType::Candles);
        chart.set_visible_bars(100);
        chart.set_symbol("SYNTH");
        chart.set_timeframe_label("1h");

        Self { chart }
    }
}

impl eframe::App for BasicChartApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply the dark theme to egui's visual system
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ctx, &theme);

        egui::CentralPanel::default().show(ctx, |ui| {
            // Render the chart -- it handles pan, zoom, and crosshair automatically
            self.chart.show(ui);
        });
    }
}

// ---------------------------------------------------------------------------
// Synthetic data generation
// ---------------------------------------------------------------------------

/// Generate `count` synthetic OHLCV bars starting from `base_price`.
///
/// Uses a simple random walk with realistic wick/body proportions and
/// volume that correlates loosely with price movement.
fn generate_synthetic_bars(count: usize, base_price: f64) -> Vec<Bar> {
    use rand::Rng;

    let mut rng = rand::rng();
    let mut bars = Vec::with_capacity(count);
    let mut price = base_price;
    let now = Utc::now();
    let interval = Duration::hours(1);

    for i in 0..count {
        // Random walk for the close price
        let change_pct = rng.random_range(-0.02_f64..0.02);
        let close = price * (1.0 + change_pct);

        // Derive OHLC from the walk
        let open = price;
        let high = open.max(close) * (1.0 + rng.random_range(0.001..0.01));
        let low = open.min(close) * (1.0 - rng.random_range(0.001..0.01));
        let volume = 1000.0 + rng.random_range(0.0..5000.0) * (1.0 + change_pct.abs() * 10.0);

        let time = now - interval * (count - i) as i32;

        bars.push(Bar::new(time, open, high, low, close, volume));
        price = close;
    }

    bars
}
