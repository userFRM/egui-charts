//! Isolated guide scenes with deterministic bars, also runnable natively.

mod first_chart;
mod scenes;

use eframe::egui;
use egui_charts::Chart;

struct ExampleApp {
    chart: Chart,
    scene: Scene,
    #[cfg(target_arch = "wasm32")]
    dark: Option<bool>,
}

enum Scene {
    Chart,
    PriceScale(scenes::price_scale::PriceScale),
    TimeScale,
    Theme(scenes::theme::Presentation),
    Panes(scenes::panes::Panes),
    Live(scenes::studies::LiveData),
    HostReadout(Box<scenes::recipes::HostReadout>),
    Synchronized(Box<scenes::recipes::Synchronized>),
    Export(scenes::recipes::Export),
    OrderFlow(Box<scenes::order_flow::ExecutionDepth>),
    RecordingReplay(scenes::recording_replay::RecordingReplay),
    HostReport(scenes::host_report::HostReport),
}

impl ExampleApp {
    fn new(name: &str) -> Option<Self> {
        let mut scene = Scene::Chart;
        let chart = match name {
            "first-chart" => first_chart::first_chart(),
            "order-flow" => {
                let (chart, flow) = scenes::order_flow::ExecutionDepth::new();
                scene = Scene::OrderFlow(Box::new(flow));
                chart
            }
            "recording-replay" => {
                let (chart, replay) = scenes::recording_replay::RecordingReplay::new();
                scene = Scene::RecordingReplay(replay);
                chart
            }
            "host-report" => {
                scene = Scene::HostReport(scenes::host_report::HostReport::default());
                first_chart::first_chart()
            }
            "series" => scenes::series::lifecycle(),
            "annotations" => {
                let mut chart = first_chart::first_chart();
                scenes::recipes::annotations(&mut chart);
                chart
            }
            "price-volume" => {
                let mut chart = first_chart::first_chart();
                scenes::recipes::price_volume(&mut chart);
                chart
            }
            "second-series" => {
                let mut chart = first_chart::first_chart();
                scenes::recipes::second_series(&mut chart);
                chart
            }
            "host-readout" => {
                scene = Scene::HostReadout(Box::default());
                first_chart::first_chart()
            }
            "synchronized" => {
                scene = Scene::Synchronized(Box::new(scenes::recipes::Synchronized::new(
                    first_chart::first_chart(),
                )));
                first_chart::first_chart()
            }
            "export" => {
                scene = Scene::Export(scenes::recipes::Export::default());
                first_chart::first_chart()
            }
            "series-candles" => scenes::series::candles(),
            "series-line" => scenes::series::line(),
            "series-area" => scenes::series::area(),
            "series-baseline" => scenes::series::baseline(),
            "series-histogram" => scenes::series::histogram(),
            "series-heikin" => scenes::chart_types::heikin(),
            "series-renko" => scenes::chart_types::renko(),
            "series-point-and-figure" => scenes::chart_types::point_and_figure(),
            "volatility-smile" => scenes::extensions::volatility_smile(),
            "yield-curve" => scenes::extensions::yield_curve(),
            "primitives" => scenes::extensions::primitives(),
            "custom-series" => scenes::extensions::custom_series(),
            "price-scale" => {
                let mut chart = first_chart::first_chart();
                scene = Scene::PriceScale(scenes::price_scale::PriceScale::new(&mut chart));
                chart
            }
            "time-scale" => {
                scene = Scene::TimeScale;
                first_chart::first_chart()
            }
            "theme" => {
                scene = Scene::Theme(scenes::theme::Presentation::default());
                first_chart::first_chart()
            }
            "panes" => {
                let mut chart = first_chart::first_chart();
                scene = Scene::Panes(scenes::panes::Panes::new(&mut chart));
                chart
            }
            "studies" => scenes::studies::builtins(),
            "custom-study" => scenes::studies::custom(),
            "live-data" => {
                let (chart, live) = scenes::studies::LiveData::new();
                scene = Scene::Live(live);
                chart
            }
            _ => return None,
        };
        Some(Self {
            chart,
            scene,
            #[cfg(target_arch = "wasm32")]
            dark: None,
        })
    }
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
        egui::CentralPanel::default().show(ui, |ui| {
            match &mut self.scene {
                Scene::Chart => {}
                Scene::PriceScale(scene) => scene.show(ui, &mut self.chart),
                Scene::TimeScale => scenes::time_scale::show(ui, &mut self.chart),
                Scene::Theme(scene) => scene.show(ui, &mut self.chart),
                Scene::Panes(scene) => scene.show(ui, &mut self.chart),
                Scene::Live(scene) => scene.show(ui, &mut self.chart),
                Scene::HostReadout(scene) => return scene.show(ui, &mut self.chart),
                Scene::Synchronized(scene) => return scene.show(ui, &mut self.chart),
                Scene::Export(scene) => return scene.show(ui, &mut self.chart),
                Scene::OrderFlow(scene) => return scene.show(ui, &mut self.chart),
                Scene::RecordingReplay(scene) => return scene.show(ui, &mut self.chart),
                Scene::HostReport(scene) => return scene.show(ui),
            }
            self.chart.show(ui);
        });
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
            let result = match root
                .get_attribute("data-scene")
                .as_deref()
                .and_then(ExampleApp::new)
            {
                Some(app) => {
                    eframe::WebRunner::new()
                        .start(
                            canvas,
                            eframe::WebOptions::default(),
                            Box::new(|_| Ok(Box::new(app))),
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
        "Guide example",
        Default::default(),
        Box::new(|_| {
            let name = std::env::args()
                .nth(1)
                .unwrap_or_else(|| "first-chart".into());
            let app = ExampleApp::new(&name).ok_or("Unknown example scene")?;
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guide_scene_can_start() {
        for name in [
            "first-chart",
            "order-flow",
            "recording-replay",
            "host-report",
            "series",
            "series-candles",
            "series-line",
            "series-area",
            "series-baseline",
            "series-histogram",
            "series-heikin",
            "series-renko",
            "series-point-and-figure",
            "price-scale",
            "time-scale",
            "theme",
            "panes",
            "studies",
            "custom-study",
            "live-data",
            "annotations",
            "price-volume",
            "host-readout",
            "synchronized",
            "export",
            "second-series",
            "volatility-smile",
            "yield-curve",
            "primitives",
            "custom-series",
        ] {
            assert!(ExampleApp::new(name).is_some(), "{name}");
        }
        assert!(ExampleApp::new("unknown").is_none());
    }
}
