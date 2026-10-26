//! # Chart with Drawing Tools
//!
//! Demonstrates how to use the drawing tools subsystem to add technical analysis
//! annotations to a chart. This example:
//!
//! - Creates a chart, whose `DrawingManager` holds its drawings
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

mod support;

use chrono::{TimeDelta, Utc};
use egui_charts::ChartType;
use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
use egui_charts::model::{BarData, Timeframe};
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
}

impl DrawingApp {
    fn new() -> Self {
        let bars = support::sample_bars(200, 2_000.0, TimeDelta::hours(4), Utc::now());
        let data = BarData::from_bars(bars);

        let mut chart = Chart::new(data);
        chart.set_chart_type(ChartType::Candles);
        chart.set_visible_bars(100);
        chart.set_symbol("ETHUSDT");
        chart.set_timeframe(Timeframe::Hour4);

        let drawing_manager = chart.drawings_mut();

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

        Self { chart }
    }
}

impl eframe::App for DrawingApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ui.ctx(), &theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ui.ctx().global_style());

        egui::CentralPanel::default().show(ui, |ui| {
            // The chart renders its drawings and handles interactive drawing input.
            self.chart.show(ui);
        });
    }
}
