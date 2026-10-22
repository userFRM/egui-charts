//! # Data source
//!
//! Builds a `TradingChart` with `ChartBuilder::extended()` and a sample `DataSource` that serves
//! history from memory. Replace `SampleDataSource` with a source that reads your backend: IPC, a
//! WebSocket or a database.
//!
//! Run with:
//! ```sh
//! cargo run --example data_source
//! ```

use chrono::{Duration, Utc};
use egui_charts::data::{
    Bar, DataSource, DataSourceError, DataUpdate, HistoricalDataRequest, Timeframe,
};
use egui_charts::theme::Theme;
use egui_charts::{ChartBuilder, TradingChart};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-charts: data source",
        options,
        Box::new(|_cc| Ok(Box::new(DataSourceApp::new()))),
    )
}

// ===========================================================================
// Application state
// ===========================================================================

struct DataSourceApp {
    trading_chart: TradingChart,
}

impl DataSourceApp {
    fn new() -> Self {
        // In your application, pass your own data source here.
        let data_source = SampleDataSource::new();

        // A chart with the data source, drawing tools and an indicator registry.
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

impl eframe::App for DataSourceApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 1. Apply theme at the start of each frame
        let theme = Theme::dark();
        egui_charts::theme::apply_to_egui(ui.ctx(), &theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ui.ctx().global_style());

        // 2. Poll data source for updates and handle progressive loading
        self.trading_chart.update();

        // 3. Render the chart
        egui::CentralPanel::default().show(ui, |ui| {
            self.trading_chart.show(ui);
        });

        // In your application, you might also react to your backend here:
        // - Symbol changes from the frontend: trading_chart.set_symbol("ETHUSDT")
        // - Timeframe changes: trading_chart.set_timeframe(Timeframe::Min15)
        // - Chart type switches: trading_chart.set_chart_type(ChartType::Line)
    }
}

// ===========================================================================
// Sample DataSource implementation
// ===========================================================================
//
// In your application, replace this with a DataSource that:
// - Fetches historical data from your backend
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
        // The newest `limit` bars before the requested end, oldest first.
        let end = self
            .bars
            .partition_point(|bar| bar.time.timestamp_millis() < request.end_ts_millis);
        Ok(self.bars[end.saturating_sub(request.limit)..end].to_vec())
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
    use rand::RngExt;

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
