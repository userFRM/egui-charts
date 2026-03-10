//! # Chart with Drawing Tools
//!
//! Demonstrates how to use the drawing tools subsystem to add technical analysis
//! annotations to a chart. This example:
//!
//! - Creates a chart with `DrawingManager` enabled
//! - Programmatically adds a **trend line** between two price points
//! - Programmatically adds a **Fibonacci retracement** between a swing low and high
//! - Shows how to set the active tool for interactive drawing via mouse
//!
//! In a real application, users would draw interactively by selecting a tool
//! from a toolbar and clicking on the chart. This example shows both the
//! programmatic API and the interactive setup.
//!
//! Run with:
//! ```sh
//! cargo run --example drawing_tools
//! ```

use chrono::{Duration, Utc};
use egui_charts::ChartType;
use egui_charts::drawings::{ChartPoint, Drawing, DrawingManager, DrawingToolType};
use egui_charts::model::{Bar, BarData};
use egui_charts::theme::Theme;
use egui_charts::widget::Chart;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-charts: Drawing Tools",
        options,
        Box::new(|_cc| Ok(Box::new(DrawingApp::new()))),
    )
}

struct DrawingApp {
    chart: Chart,
    drawing_manager: DrawingManager,
}

impl DrawingApp {
    fn new() -> Self {
        let bars = generate_synthetic_bars(200, 2_000.0);
        let data = BarData::from_bars(bars);

        let mut chart = Chart::new(data);
        chart.set_chart_type(ChartType::Candles);
        chart.set_visible_bars(100);
        chart.set_symbol("ETHUSDT");
        chart.set_timeframe_label("4h");

        let mut drawing_manager = DrawingManager::new();

        // --- Programmatic drawing: Trend Line ---
        // A trend line from bar index 30 at price 1950 to bar index 80 at price 2100
        let mut trend_line = Drawing::new(1, DrawingToolType::TrendLine);
        trend_line.chart_points.push(ChartPoint::new(30.0, 1950.0));
        trend_line.chart_points.push(ChartPoint::new(80.0, 2100.0));
        trend_line.color = [33, 150, 243, 255]; // Material Blue
        trend_line.stroke_width = 2.0;
        trend_line.completed = true;
        drawing_manager.add_synced_drawing(trend_line);

        // --- Programmatic drawing: Fibonacci Retracement ---
        // From a swing low at bar 50 to a swing high at bar 120
        let mut fib = Drawing::new(2, DrawingToolType::FibonacciRetracement);
        fib.chart_points.push(ChartPoint::new(50.0, 1900.0)); // swing low
        fib.chart_points.push(ChartPoint::new(120.0, 2150.0)); // swing high
        fib.color = [255, 193, 7, 255]; // Amber
        fib.stroke_width = 1.5;
        fib.completed = true;
        drawing_manager.add_synced_drawing(fib);

        // Set the active tool for interactive drawing (users can click the chart)
        // In a full app, this would be controlled by a toolbar.
        drawing_manager.set_active_tool(Some(DrawingToolType::TrendLine));

        Self {
            chart,
            drawing_manager,
        }
    }
}

impl eframe::App for DrawingApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ctx, &theme);

        egui::CentralPanel::default().show(ctx, |ui| {
            // Pass the drawing manager to the chart so it renders annotations
            // and handles interactive drawing input.
            self.chart
                .show_with_indicators(ui, Some(&mut self.drawing_manager), None);
        });
    }
}

// ---------------------------------------------------------------------------
// Synthetic data generation
// ---------------------------------------------------------------------------

fn generate_synthetic_bars(count: usize, base_price: f64) -> Vec<Bar> {
    use rand::Rng;

    let mut rng = rand::rng();
    let mut bars = Vec::with_capacity(count);
    let mut price = base_price;
    let now = Utc::now();
    let interval = Duration::hours(4);

    for i in 0..count {
        let change_pct = rng.random_range(-0.02_f64..0.02);
        let close = price * (1.0 + change_pct);
        let open = price;
        let high = open.max(close) * (1.0 + rng.random_range(0.001..0.01));
        let low = open.min(close) * (1.0 - rng.random_range(0.001..0.01));
        let volume = 200.0 + rng.random_range(0.0..2000.0);

        let time = now - interval * (count - i) as i32;
        bars.push(Bar::new(time, open, high, low, close, volume));
        price = close;
    }

    bars
}
