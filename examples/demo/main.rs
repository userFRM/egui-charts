//! egui-charts demo — full showcase with toolbars, indicators, and drawing tools
//!
//! Build: `trunk build --release` (from this directory)
//! Serve: `trunk serve`

use eframe::egui;
use egui_charts::ChartBuilder;
use egui_charts::model::{Bar, BarData, ChartType, Timeframe};
use egui_charts::studies::{BollingerBands, EMA, IndicatorRegistry, MACD, RSI, SMA};
use egui_charts::theme::Theme;
use egui_charts::ui::app_state::ChartAppState;
use egui_charts::ui::drawing_toolbar::{DrawingToolbar, DrawingToolbarAction};
use egui_charts::ui::symbol_header::{OhlcData, SymbolHeader, SymbolInfo};
use egui_charts::ui::timeframe_toolbar::TimeframeToolbar;
use egui_charts::ui::top_toolbar::{TopToolbar, TopToolbarAction};
use egui_charts::ui::widget_bar::WidgetBar;

fn main() -> eframe::Result {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        eframe::WebLogger::init(log::LevelFilter::Info).ok();

        use wasm_bindgen::JsCast;
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("demo_canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();

        // Hide loading overlay
        if let Some(el) = document.get_element_by_id("loading") {
            let _ = el.set_attribute("style", "display:none");
        }

        wasm_bindgen_futures::spawn_local(async move {
            eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
                )
                .await
                .expect("Failed to start eframe");
        });
        return Ok(());
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        eframe::run_native(
            "egui-charts demo",
            eframe::NativeOptions::default(),
            Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
        )
    }
}

// ---------------------------------------------------------------------------
// App state — implements ChartAppState so toolbars can read symbol/tf/type
// ---------------------------------------------------------------------------

struct DemoAppState {
    symbol: String,
    timeframe: Timeframe,
    chart_type: ChartType,
}

impl ChartAppState for DemoAppState {
    fn active_symbol(&self) -> &str {
        &self.symbol
    }
    fn active_timeframe(&self) -> &Timeframe {
        &self.timeframe
    }
    fn chart_type(&self) -> ChartType {
        self.chart_type
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn is_replay_active(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Main demo app
// ---------------------------------------------------------------------------

struct DemoApp {
    trading_chart: egui_charts::chart::builder::TradingChart,
    state: DemoAppState,
    bars: Vec<Bar>,

    // UI components
    top_toolbar: TopToolbar,
    drawing_toolbar: DrawingToolbar,
    timeframe_toolbar: TimeframeToolbar,
    widget_bar: WidgetBar,
    symbol_header: SymbolHeader,
    theme: Theme,
}

impl DemoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let theme = Theme::dark();
        let timeframe = Timeframe::Day1;

        // Build indicators
        let mut registry = IndicatorRegistry::new();
        registry.add(Box::new(SMA::new(20)));
        registry.add(Box::new(EMA::new(50)));
        registry.add(Box::new(BollingerBands::new(20, 2.0)));
        registry.add(Box::new(RSI::new(14)));
        registry.add(Box::new(MACD::new(12, 26, 9)));

        // Build chart with drawing tools + indicators
        let mut tc = ChartBuilder::extended()
            .with_symbol("BTCUSD")
            .with_timeframe(timeframe)
            .with_theme(theme.clone())
            .with_chart_type(ChartType::Candles)
            .with_indicators(registry)
            .with_visible_candles(100)
            .with_initial_bars(None)
            .build();

        // Generate sample data
        let bars = generate_sample_bars(500, timeframe);
        let data = BarData::from_bars(bars.clone());
        tc.chart.update_data(data);
        tc.indicators.calculate_all(&bars);

        // Symbol header with last bar OHLC
        let last = bars.last().unwrap();
        let symbol_info = SymbolInfo {
            symbol: "BTCUSD".into(),
            company_name: "Bitcoin / US Dollar".into(),
            exchange: "Crypto".into(),
            timeframe: "1D".into(),
        };
        let symbol_header = SymbolHeader::new(symbol_info).with_ohlc(OhlcData::new(
            last.open,
            last.high,
            last.low,
            last.close,
            last.volume,
        ));

        Self {
            trading_chart: tc,
            state: DemoAppState {
                symbol: "BTCUSD".into(),
                timeframe,
                chart_type: ChartType::Candles,
            },
            bars,
            top_toolbar: TopToolbar::new(),
            drawing_toolbar: DrawingToolbar::default(),
            timeframe_toolbar: TimeframeToolbar::default(),
            widget_bar: WidgetBar::default(),
            symbol_header,
            theme,
        }
    }

    /// Regenerate bars for a new timeframe and recalculate indicators.
    fn switch_timeframe(&mut self, tf: Timeframe) {
        self.state.timeframe = tf;
        let count = bar_count_for_timeframe(tf);
        self.bars = generate_sample_bars(count, tf);
        let data = BarData::from_bars(self.bars.clone());
        self.trading_chart.chart.update_data(data);
        self.trading_chart.indicators.calculate_all(&self.bars);
        self.update_symbol_header();
    }

    fn update_symbol_header(&mut self) {
        if let Some(last) = self.bars.last() {
            self.symbol_header.ohlc =
                OhlcData::new(last.open, last.high, last.low, last.close, last.volume);
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme each frame
        egui_charts::theme::apply_to_egui(ctx, &self.theme);

        let panel_bg = ctx.style().visuals.panel_fill;

        // --- Top toolbar (38px) ---
        egui::TopBottomPanel::top("toolbar")
            .exact_height(38.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(panel_bg)
                    .stroke(egui::Stroke::NONE)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                let action = self.top_toolbar.show_with_state(ui, &self.state);
                match action {
                    TopToolbarAction::ChartStyleSelected(name) => {
                        if let Some(ct) = chart_type_from_name(&name) {
                            self.state.chart_type = ct;
                            self.trading_chart.set_chart_type(ct);
                        }
                    }
                    TopToolbarAction::IntervalSelected(display) => {
                        if let Some(tf) = timeframe_from_display(&display) {
                            self.switch_timeframe(tf);
                        }
                    }
                    TopToolbarAction::Undo => {
                        if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                            dm.undo();
                        }
                    }
                    TopToolbarAction::Redo => {
                        if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                            dm.redo();
                        }
                    }
                    _ => {}
                }
            });

        // --- Symbol header (38px) ---
        egui::TopBottomPanel::top("symbol_header")
            .exact_height(38.0)
            .show_separator_line(true)
            .frame(
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                self.symbol_header.show(ui);
            });

        // --- Drawing toolbar — left sidebar (52px) ---
        egui::SidePanel::left("drawing_toolbar")
            .resizable(false)
            .default_width(52.0)
            .show_separator_line(true)
            .frame(
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                let action = self.drawing_toolbar.show_with_action(ui);
                match action {
                    DrawingToolbarAction::SelectTool(tool) => {
                        if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                            dm.set_active_tool(Some(tool));
                        }
                    }
                    DrawingToolbarAction::ClearSelection => {
                        if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                            dm.set_active_tool(None);
                        }
                    }
                    DrawingToolbarAction::ToggleMagnet => {
                        // DrawingToolbar manages its own magnet state visually
                    }
                    DrawingToolbarAction::ClearAllDrawings => {
                        if let Some(dm) = self.trading_chart.drawing_manager.as_mut() {
                            dm.clear_all();
                        }
                    }
                    DrawingToolbarAction::ZoomIn | DrawingToolbarAction::ZoomOut => {
                        // Zoom mode is handled internally by the chart widget
                        // via set_zoom_mode — the toolbar toggles zoom_mode_active
                        // and the chart applies it on next click-drag.
                    }
                    _ => {}
                }
            });

        // --- Widget bar — right sidebar (52px) ---
        egui::SidePanel::right("widget_bar")
            .resizable(false)
            .exact_width(52.0)
            .show_separator_line(true)
            .frame(
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                self.widget_bar.show(ui);
            });

        // --- Bottom timeframe toolbar (38px) ---
        egui::TopBottomPanel::bottom("bottom")
            .exact_height(38.0)
            .show_separator_line(true)
            .frame(
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                self.timeframe_toolbar.show(ui);
            });

        // --- Central panel — chart ---
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                self.trading_chart.show(ui);
            });

        // Update header OHLC from last bar each frame
        if let Some(last) = self.trading_chart.get_last_bar() {
            self.symbol_header.ohlc =
                OhlcData::new(last.open, last.high, last.low, last.close, last.volume);
        }
    }
}

// ---------------------------------------------------------------------------
// Sample data generation
// ---------------------------------------------------------------------------

fn generate_sample_bars(count: usize, timeframe: Timeframe) -> Vec<Bar> {
    use chrono::{TimeZone, Utc};

    let mut bars = Vec::with_capacity(count);
    let mut price = 42_500.0_f64; // Realistic BTC starting price
    let base_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let step_secs = timeframe.as_seconds().max(1);

    // State for realistic price action
    let mut trend = 0.0_f64;
    let mut volatility = 1.0_f64;

    for i in 0..count {
        let r1 = hash_f64(i, 0); // main return driver
        let r2 = hash_f64(i, 1000); // upper wick extension
        let r3 = hash_f64(i, 2000); // lower wick extension
        let r4 = hash_f64(i, 3000); // volume base
        let r5 = hash_f64(i, 4000); // trend drift

        // Slow-moving trend with mean reversion (momentum effect)
        trend = trend * 0.985 + (r5 - 0.5) * 0.003;

        // Return: trend + noise scaled by volatility
        let return_pct = trend + (r1 - 0.5) * 0.025 * volatility;

        // Volatility clustering (big moves beget big moves)
        volatility = 0.93 * volatility + 0.07 * (1.0 + return_pct.abs() * 40.0);
        volatility = volatility.clamp(0.5, 3.0);

        let open = price;
        let close = open * (1.0 + return_pct);

        // Wicks proportional to body, with randomized extension
        let body_high = open.max(close);
        let body_low = open.min(close);
        let body_range = (body_high - body_low).max(open * 0.0005);

        let high = body_high + r2 * body_range * 2.0;
        let low = body_low - r3 * body_range * 2.0;

        // Volume: correlated with absolute return (big moves = high volume)
        let base_vol = 600.0 + r4 * 1400.0;
        let vol_spike = 1.0 + return_pct.abs() * 30.0;
        let volume = base_vol * vol_spike;

        let time = base_time + chrono::Duration::seconds(step_secs * i as i64);

        bars.push(Bar {
            time,
            open,
            high,
            low,
            close,
            volume,
        });
        price = close;
    }

    bars
}

fn bar_count_for_timeframe(tf: Timeframe) -> usize {
    match tf {
        Timeframe::Min1 | Timeframe::Min5 => 1000,
        Timeframe::Min15 | Timeframe::Min30 => 800,
        Timeframe::Hour1 | Timeframe::Hour4 => 600,
        Timeframe::Day1 | Timeframe::Week1 => 500,
        Timeframe::Month1 => 120,
        _ => 500,
    }
}

// ---------------------------------------------------------------------------
// Mapping helpers — toolbar strings → engine types
// ---------------------------------------------------------------------------

fn chart_type_from_name(name: &str) -> Option<ChartType> {
    match name.to_lowercase().as_str() {
        "bars" => Some(ChartType::Bars),
        "candles" | "candlestick" => Some(ChartType::Candles),
        "hollow candles" | "hollow" => Some(ChartType::HollowCandles),
        "line" => Some(ChartType::Line),
        "line with markers" => Some(ChartType::LineWithMarkers),
        "step line" => Some(ChartType::StepLine),
        "area" => Some(ChartType::Area),
        "hlc area" => Some(ChartType::HlcArea),
        "baseline" => Some(ChartType::Baseline),
        "heikin ashi" | "heikin" => Some(ChartType::Heikin),
        "renko" => Some(ChartType::Renko),
        "kagi" => Some(ChartType::Kagi),
        "line break" => Some(ChartType::LineBreak),
        "point and figure" | "point & figure" => Some(ChartType::PointAndFigure),
        "range" => Some(ChartType::Range),
        "high-low" | "high low" => Some(ChartType::HighLow),
        "volume candles" => Some(ChartType::VolumeCandles),
        _ => None,
    }
}

fn timeframe_from_display(display: &str) -> Option<Timeframe> {
    // TopToolbar returns display strings like "1", "5", "15", "30", "60", "240", "1D", "1W", "1M"
    Timeframe::from_resolution(display)
}

/// Deterministic pseudo-random f64 in [0, 1) with two seeds — no `rand` needed
fn hash_f64(index: usize, salt: usize) -> f64 {
    let mut x = (index.wrapping_mul(2654435761) ^ salt) as u32;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x = x.wrapping_mul(1597334677);
    (x % 100_000) as f64 / 100_000.0
}
