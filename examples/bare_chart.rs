//! One candlestick chart with live updates, built from the `widget` feature alone:
//!
//! ```sh
//! cargo run --release --example bare_chart --no-default-features --features widget
//! ```
//!
//! On `wasm32` it fills a canvas it adds to the page, in the dark theme, whose Roboto is the one
//! font of its web build; `scripts/web-size.sh` builds it for the web.
//!
//! The prices are generated sample data. `benches/bare_chart.rs` runs this example's
//! frame headlessly to measure its cost, and `benches/frame_time.rs` draws its sample bars.

use chrono::{TimeZone, Utc};
use egui_charts::model::{Bar, BarData};
use egui_charts::widget::{Chart, SeriesId};

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    struct WebApp(BareChart);
    impl eframe::App for WebApp {
        fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
            self.0.ui(ui);
            ui.ctx().request_repaint(); // a live chart: one sample tick per frame
        }
    }

    let document = eframe::web_sys::window()
        .and_then(|window| window.document())
        .expect("a document");
    let canvas: eframe::web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .expect("a canvas element")
        .unchecked_into();
    canvas
        .set_attribute(
            "style",
            "position: fixed; inset: 0; width: 100%; height: 100%",
        )
        .expect("a canvas style");
    document
        .body()
        .expect("a body")
        .append_child(&canvas)
        .expect("the canvas on the page");
    wasm_bindgen_futures::spawn_local(async move {
        eframe::WebRunner::new()
            .start(
                canvas,
                Default::default(),
                Box::new(|cc| {
                    egui_charts::theme::apply_to_egui(
                        &cc.egui_ctx,
                        &egui_charts::theme::Theme::dark(),
                    );
                    Ok(Box::new(WebApp(BareChart::new())))
                }),
            )
            .await
            .expect("the chart starts");
    });
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
