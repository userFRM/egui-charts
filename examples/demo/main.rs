//! egui-charts demo: a charting application built from the crate's widgets.
//!
//! The chart with the `ui` toolbars, dialogs, side panels, context menus and bar
//! replay, wired in [`controller`]. Controls that need a backend the demo does not
//! have show a notice, and some context menu entries do nothing yet.
//!
//! Build (web):    `trunk build --release` from this directory
//! Serve (web):    `trunk serve`
//! Run (native):   `cargo run` from this directory

mod controller;
mod panel_alerts;
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
            .get_element_by_id("landing_canvas")
            .or_else(|| document.get_element_by_id("demo_canvas"))
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        let landing = canvas.id() == "landing_canvas";

        wasm_bindgen_futures::spawn_local(async move {
            let result = eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(move |cc| {
                        signal_first_frame(&cc.egui_ctx);
                        if landing {
                            Ok(Box::new(controller::landing::LandingChart::new(cc)))
                        } else {
                            Ok(DemoApp::new(cc))
                        }
                    }),
                )
                .await;
            if let Err(error) = result {
                let message = error.as_string().unwrap_or_else(|| {
                    "Check your connection and graphics support, then retry.".into()
                });
                startup_state(
                    "error",
                    Some(&format!("Could not start the chart: {message}")),
                );
            }
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
            Box::new(|cc| Ok(DemoApp::new(cc))),
        )
    }
}

#[cfg(target_arch = "wasm32")]
fn startup_state(state: &str, error: Option<&str>) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        if let Some(error) = error {
            let _ = root.set_attribute("data-demo-error", error);
        }
        let _ = root.set_attribute("data-demo-state", state);
    }
}

#[cfg(target_arch = "wasm32")]
fn signal_first_frame(ctx: &eframe::egui::Context) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let painted = Arc::new(AtomicBool::new(false));
    ctx.on_end_pass(
        "demo_startup",
        Arc::new(move |ui| {
            if painted.load(Ordering::Relaxed) {
                return;
            }
            let painted = painted.clone();
            // A paint callback runs after the chart's shapes, including on the embedded page.
            ui.painter().add(eframe::egui::PaintCallback {
                rect: ui.ctx().content_rect(),
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_, _| {
                    if !painted.swap(true, Ordering::Relaxed) {
                        startup_state("ready", None);
                    }
                })),
            });
        }),
    );
}
