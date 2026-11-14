//! The landing page's chart uses the demo's synthetic history and live feed.

use super::live::LiveFeed;
use crate::sampledata::{SAMPLE_LOGO, SAMPLE_LOGO_SVG};
use chrono::Utc;
use eframe::egui;
use egui_charts::config::ChartOptions;
use egui_charts::theme::{Theme, apply_to_egui};
use egui_charts::{Chart, ChartBuilder};
use std::sync::Arc;
use wasm_bindgen::JsCast;

pub(crate) struct LandingChart {
    chart: Chart,
    feed: Arc<LiveFeed>,
    dark: bool,
}

impl LandingChart {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.include_bytes(SAMPLE_LOGO, SAMPLE_LOGO_SVG);
        let dark = dark_theme(&cc.egui_ctx);
        let theme = if dark { Theme::dark() } else { Theme::light() };
        apply_to_egui(&cc.egui_ctx, &theme);
        let feed = Arc::new(LiveFeed::new("SAMPLE", Utc::now()));
        let mut options = ChartOptions::default();
        options.time_scale.lock_visible_time_range_on_resize = true;
        let chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("SAMPLE")
            .with_timeframe(egui_charts::model::Timeframe::Min1)
            .with_theme(theme)
            .with_chart_options(options)
            .with_visible_candles(80)
            .build();
        Self { chart, feed, dark }
    }
}

impl eframe::App for LandingChart {
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        resize_canvas(&ctx);
        let dark = dark_theme(&ctx);
        if dark != self.dark {
            self.dark = dark;
            let theme = if dark { Theme::dark() } else { Theme::light() };
            apply_to_egui(&ctx, &theme);
            self.chart.set_theme(&ctx, &theme);
        }
        let delay = self.feed.advance(Utc::now());
        ctx.request_repaint_after(delay);
        // The figure caption identifies the sample when its in-chart readout cannot fit.
        let compact = ui.available_width() < 480.0;
        let config = self.chart.config_mut();
        config.show_symbol_labels = !compact;
        config.show_ohlc_info = !compact;
        config.show_ohlc_change = !compact;
        config.show_ohlc_change_percent = !compact;
        config.show_quote_boxes = !compact;
        config.show_legend_volume = !compact;
        egui::CentralPanel::default().show(ui, |ui| self.chart.show(ui));
    }
}

fn resize_canvas(ctx: &egui::Context) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(canvas) = window
        .document()
        .and_then(|document| document.get_element_by_id("landing_canvas"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        return;
    };
    // Embedded resize notifications can report CSS pixels in place of device pixels.
    let ratio = window.device_pixel_ratio();
    let width = (f64::from(canvas.client_width()) * ratio).round() as u32;
    let height = (f64::from(canvas.client_height()) * ratio).round() as u32;
    if canvas.width() != width || canvas.height() != height {
        canvas.set_width(width);
        canvas.set_height(height);
        ctx.request_repaint();
    }
}

fn dark_theme(ctx: &egui::Context) -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .and_then(|root| root.get_attribute("data-theme"))
        .map(|theme| theme == "dark")
        .unwrap_or_else(|| ctx.system_theme() == Some(egui::Theme::Dark))
}
