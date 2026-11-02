//! The landing page's chart uses the demo's synthetic history and live feed.

use super::live::LiveFeed;
use crate::sampledata::{SAMPLE_LOGO, SAMPLE_LOGO_SVG};
use chrono::Utc;
use eframe::egui;
use egui_charts::theme::{Theme, apply_to_egui};
use egui_charts::{Chart, ChartBuilder};
use std::sync::Arc;

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
        let chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("SAMPLE")
            .with_timeframe(egui_charts::model::Timeframe::Min1)
            .with_theme(theme)
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
        let dark = dark_theme(&ctx);
        if dark != self.dark {
            self.dark = dark;
            let theme = if dark { Theme::dark() } else { Theme::light() };
            apply_to_egui(&ctx, &theme);
            self.chart.set_theme(&ctx, &theme);
        }
        let delay = self.feed.advance(Utc::now());
        ctx.request_repaint_after(delay);
        egui::CentralPanel::default().show(ui, |ui| self.chart.show(ui));
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
