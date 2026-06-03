//! egui-charts demo — a complete TradingView-style application.
//!
//! Every toolbar button, menu item, dialog, side panel, right-click action, and
//! the bar-replay engine is wired to real chart behavior in [`controller`].
//!
//! Build (web):    `trunk build --release` from this directory
//! Serve (web):    `trunk serve`
//! Run (native):   `cargo run` from this directory

mod controller;
mod sampledata;

use controller::DemoApp;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;

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

        // Hide loading overlay.
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
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
            ..Default::default()
        };
        eframe::run_native(
            "egui-charts demo",
            options,
            Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
        )
    }
}
