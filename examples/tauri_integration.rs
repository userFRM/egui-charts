//! # Tauri Integration Skeleton
//!
//! Shows the recommended architecture for embedding `egui-charts` inside a
//! [Tauri](https://tauri.app) desktop application. This file compiles as a
//! standalone eframe app for demonstration, but the patterns translate directly
//! to a Tauri + egui setup.
//!
//! ## Key integration points
//!
//! 1. **DataSource** -- Implement the `DataSource` trait to bridge your Tauri
//!    backend (IPC commands, WebSocket relay) to the chart's data layer.
//!
//! 2. **ChartBuilder** -- Use `ChartBuilder::extended()` for a full trading
//!    terminal, or `ChartBuilder::new()` for a simpler chart.
//!
//! 3. **Theme** -- Call `apply_to_egui()` once per frame to propagate the
//!    design-token theme into egui's visual system.
//!
//! 4. **Update loop** -- Call `trading_chart.update()` then
//!    `trading_chart.show(ui)` inside your `eframe::App::update`.
//!
//! ## In a real Tauri app
//!
//! Replace `SampleDataSource` below with your own `DataSource` implementation
//! that fetches data via Tauri IPC commands or a WebSocket connection managed
//! by the Rust backend.
//!
//! Run with:
//! ```sh
//! cargo run --example tauri_integration
//! ```

use chrono::{Duration, Utc};
use egui_charts::data::{
    Bar, DataSource, DataSourceError, DataUpdate, HistoricalDataRequest, Timeframe,
};
use egui_charts::model::BarData;
use egui_charts::theme::Theme;
use egui_charts::{ChartBuilder, TradingChart};

fn main() -> eframe::Result {
    // In a real Tauri app, this would be inside your eframe integration.
    // See: https://docs.rs/tauri-plugin-egui or use tauri::Builder with egui.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-charts: Tauri Integration",
        options,
        Box::new(|_cc| Ok(Box::new(TauriApp::new()))),
    )
}

// ===========================================================================
// Application state
// ===========================================================================

struct TauriApp {
    trading_chart: TradingChart,
}

impl TauriApp {
    fn new() -> Self {
        // In Tauri, you would pass your real data source here.
        // For example:
        //   let data_source = TauriIpcDataSource::new(app_handle.clone());
        let data_source = SampleDataSource::new();

        // Build a full-featured trading chart with data source, drawing tools,
        // and indicator support.
        let trading_chart = ChartBuilder::extended()
            .with_symbol("BTCUSDT")
            .with_timeframe(Timeframe::Hour1)
            .with_theme(Theme::dark())
            .with_data_src(Box::new(data_source))
            .with_visible_candles(120)
            .build();

        Self { trading_chart }
    }
}

impl eframe::App for TauriApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Apply theme at the start of each frame
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ctx, &theme);

        // 2. Poll data source for updates and handle progressive loading
        self.trading_chart.update();

        // 3. Render the chart
        egui::CentralPanel::default().show(ctx, |ui| {
            self.trading_chart.show(ui);
        });

        // In Tauri, you might also handle IPC events here:
        // - Symbol changes from the frontend: trading_chart.set_symbol("ETHUSDT")
        // - Timeframe changes: trading_chart.set_timeframe(Timeframe::Min15)
        // - Chart type switches: trading_chart.set_chart_type(ChartType::Line)
    }
}

// ===========================================================================
// Sample DataSource implementation
// ===========================================================================
//
// In a real Tauri application, replace this with a DataSource that:
// - Calls Tauri IPC commands to fetch historical data from your backend
// - Receives live bar updates via a WebSocket or SSE connection
// - Implements symbol search by querying your backend API

struct SampleDataSource {
    bars: Vec<Bar>,
    timeframe: Option<Timeframe>,
}

impl SampleDataSource {
    fn new() -> Self {
        Self {
            bars: generate_synthetic_bars(500, 42_000.0),
            timeframe: None,
        }
    }
}

impl DataSource for SampleDataSource {
    fn symbols(&self) -> Vec<String> {
        vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
    }

    fn subscribe(&mut self, _symbol: String, timeframe: Timeframe) -> Result<(), DataSourceError> {
        self.timeframe = Some(timeframe);
        Ok(())
    }

    fn unsubscribe(&mut self, _symbol: String) -> Result<(), DataSourceError> {
        self.timeframe = None;
        Ok(())
    }

    fn poll(&mut self) -> Vec<DataUpdate> {
        // In a real implementation, check your WebSocket receiver here
        // and return DataUpdate::NewBars for live bar updates.
        Vec::new()
    }

    fn fetch_historical(
        &mut self,
        request: HistoricalDataRequest,
    ) -> Result<Vec<Bar>, DataSourceError> {
        // Return bars that are older than the requested end timestamp
        let filtered: Vec<Bar> = self
            .bars
            .iter()
            .filter(|b| b.time.timestamp_millis() < request.end_ts_millis)
            .take(request.limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    fn supports_historical(&self) -> bool {
        true
    }

    fn get_timeframe(&self, _symbol: &str) -> Option<Timeframe> {
        self.timeframe
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
    let interval = Duration::hours(1);

    for i in 0..count {
        let change_pct = rng.random_range(-0.012_f64..0.012);
        let close = price * (1.0 + change_pct);
        let open = price;
        let high = open.max(close) * (1.0 + rng.random_range(0.001..0.006));
        let low = open.min(close) * (1.0 - rng.random_range(0.001..0.006));
        let volume = 100.0 + rng.random_range(0.0..2000.0);

        let time = now - interval * (count - i) as i32;
        bars.push(Bar::new(time, open, high, low, close, volume));
        price = close;
    }

    bars
}
