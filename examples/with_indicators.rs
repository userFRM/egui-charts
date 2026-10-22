//! # Chart with Technical Indicators
//!
//! Shows how to add technical indicators to a candlestick chart, which
//! computes, updates and draws them. This example adds:
//!
//! - **SMA(20)** -- 20-period Simple Moving Average (price overlay)
//! - **EMA(50)** -- 50-period Exponential Moving Average (price overlay)
//! - **RSI(14)** -- 14-period Relative Strength Index (separate pane)
//! - **Bollinger Bands(20, 2.0)** -- volatility bands (price overlay)
//!
//! Each indicator is calculated when it is added, and the chart updates it
//! from the first changed bar whenever its bars change.
//!
//! Run with:
//! ```sh
//! cargo run --example with_indicators
//! ```

use chrono::{Duration, Utc};
use egui_charts::ChartType;
use egui_charts::model::{Bar, BarData};
use egui_charts::studies::{BollingerBands, EMA, RSI, SMA};
use egui_charts::theme::Theme;
use egui_charts::widget::Chart;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-charts: Indicators",
        options,
        Box::new(|_cc| Ok(Box::new(IndicatorApp::new()))),
    )
}

struct IndicatorApp {
    chart: Chart,
}

impl IndicatorApp {
    fn new() -> Self {
        // Generate enough bars for the longest indicator warmup period
        let bars = generate_synthetic_bars(300, 50_000.0);
        let data = BarData::from_bars(bars);

        // Build chart
        let mut chart = Chart::new(data);
        chart.set_chart_type(ChartType::Candles);
        chart.set_visible_bars(120);
        chart.set_symbol("BTCUSDT");
        chart.set_timeframe_label("1h");

        // Add indicators, each calculated on the bars at once
        chart.add_indicator(Box::new(SMA::new(20)));
        chart.add_indicator(Box::new(EMA::new(50)));
        chart.add_indicator(Box::new(RSI::new(14)));
        chart.add_indicator(Box::new(BollingerBands::new(20, 2.0)));

        Self { chart }
    }
}

impl eframe::App for IndicatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ui.ctx(), &theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ui.ctx().global_style());

        egui::CentralPanel::default().show(ui, |ui| {
            // Overlay indicators (SMA, EMA, BB) are drawn on the price pane
            // and the others (RSI) in panes below it.
            self.chart.show(ui);
        });
    }
}

// ---------------------------------------------------------------------------
// Synthetic data generation
// ---------------------------------------------------------------------------

fn generate_synthetic_bars(count: usize, base_price: f64) -> Vec<Bar> {
    use rand::RngExt;

    let mut rng = rand::rng();
    let mut bars = Vec::with_capacity(count);
    let mut price = base_price;
    let now = Utc::now();
    let interval = Duration::hours(1);

    for i in 0..count {
        let change_pct = rng.random_range(-0.015_f64..0.015);
        let close = price * (1.0 + change_pct);
        let open = price;
        let high = open.max(close) * (1.0 + rng.random_range(0.001..0.008));
        let low = open.min(close) * (1.0 - rng.random_range(0.001..0.008));
        let volume = 500.0 + rng.random_range(0.0..3000.0);

        let time = now - interval * (count - i) as i32;
        bars.push(Bar::new(time, open, high, low, close, volume));
        price = close;
    }

    bars
}
