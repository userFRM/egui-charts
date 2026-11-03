//! Isolated guide scenes with deterministic bars, also runnable natively.

mod first_chart;
mod scenes {
    pub mod chart_types;
    pub mod series;
}

use eframe::egui;
use egui_charts::Chart;

fn scene(name: &str) -> Option<Chart> {
    Some(match name {
        "first-chart" => first_chart::first_chart(),
        "series" => scenes::series::lifecycle(),
        "series-candles" => scenes::series::candles(),
        "series-line" => scenes::series::line(),
        "series-area" => scenes::series::area(),
        "series-baseline" => scenes::series::baseline(),
        "series-histogram" => scenes::series::histogram(),
        "series-heikin" => scenes::chart_types::heikin(),
        "series-renko" => scenes::chart_types::renko(),
        "series-point-and-figure" => scenes::chart_types::point_and_figure(),
        _ => return None,
    })
}

struct ExampleApp {
    chart: Chart,
    #[cfg(target_arch = "wasm32")]
    dark: Option<bool>,
}

impl eframe::App for ExampleApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;

            let window = web_sys::window().unwrap();
            let document = window.document().unwrap();
            let root = document.document_element().unwrap();
            let dark = root.get_attribute("data-theme").as_deref() != Some("light");
            if self.dark != Some(dark) {
                self.dark = Some(dark);
                let theme = if dark {
                    egui_charts::theme::Theme::dark()
                } else {
                    egui_charts::theme::Theme::light()
                };
                self.chart.set_theme(ui.ctx(), &theme);
                ui.set_style(ui.ctx().global_style());
            }
            // Embedded resize notifications can report CSS pixels instead of device pixels.
            let canvas = document
                .get_element_by_id("example_canvas")
                .unwrap()
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .unwrap();
            let ratio = window.device_pixel_ratio();
            let width = (f64::from(canvas.client_width()) * ratio).round() as u32;
            let height = (f64::from(canvas.client_height()) * ratio).round() as u32;
            if canvas.width() != width || canvas.height() != height {
                canvas.set_width(width);
                canvas.set_height(height);
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        egui::CentralPanel::default().show(ui, |ui| self.chart.show(ui));
    }
}

fn main() -> eframe::Result {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;

        let document = web_sys::window().unwrap().document().unwrap();
        let root = document.document_element().unwrap();
        let canvas = document
            .get_element_by_id("example_canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        wasm_bindgen_futures::spawn_local(async move {
            let result = match scene(&root.get_attribute("data-scene").unwrap_or_default()) {
                Some(chart) => {
                    eframe::WebRunner::new()
                        .start(
                            canvas,
                            eframe::WebOptions::default(),
                            Box::new(move |_| Ok(Box::new(ExampleApp { chart, dark: None }))),
                        )
                        .await
                }
                None => Err(wasm_bindgen::JsValue::from_str("Unknown example scene")),
            };
            match result {
                Ok(()) => {
                    // The hidden text input otherwise takes Tab and returns focus to the canvas.
                    if let Ok(Some(input)) = document.query_selector("#example_canvas + input") {
                        let _ = input.set_attribute("tabindex", "-1");
                    }
                    let _ = root.set_attribute("data-example-state", "ready");
                }
                Err(_) => {
                    let _ = root.set_attribute("data-example-state", "error");
                    if let Some(message) = document.get_element_by_id("loading") {
                        message.set_text_content(Some(
                            "The example could not start. Read the chart setup in the guide.",
                        ));
                    }
                }
            }
        });
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    eframe::run_native(
        "First chart",
        Default::default(),
        Box::new(|_| {
            Ok(Box::new(ExampleApp {
                chart: scene(
                    &std::env::args()
                        .nth(1)
                        .unwrap_or_else(|| "first-chart".into()),
                )
                .expect("unknown example scene"),
            }))
        }),
    )
}
