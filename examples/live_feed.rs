//! # Live feed
//!
//! Drives a chart from a `Datafeed`. `SyntheticFeed` serves pages of history from memory, the
//! newest bars before each request's `before`, and streams trades from a thread once the chart
//! subscribes, which the chart draws as volume bubbles. Scroll back to load older pages. To
//! connect your backend, answer `history` through the `Reply` and push live data into the
//! `Sink`, from any thread or task.
//!
//! Run with:
//! ```sh
//! cargo run --example live_feed
//! ```

mod support;

use chrono::{DurationRound, TimeDelta, Utc};
use egui_charts::ChartBuilder;
use egui_charts::chart::overlays::VolumeBubblesConfig;
use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
use egui_charts::model::{Bar, OrderSide, Tick, Timeframe};
use egui_charts::theme::Theme;
use egui_charts::widget::Chart;
use rand::RngExt;
use std::sync::Arc;
use std::time::Duration;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui-charts: live feed",
        options,
        Box::new(|_cc| Ok(Box::new(LiveFeedApp::new()))),
    )
}

struct LiveFeedApp {
    chart: Chart,
}

impl LiveFeedApp {
    fn new() -> Self {
        let mut chart = ChartBuilder::new()
            .with_symbol("SYNTH")
            .with_timeframe(Timeframe::Min1)
            .with_feed(Arc::new(SyntheticFeed::new(20_000)))
            .with_initial_bars(500)
            .build();
        chart.set_volume_bubbles(Some(VolumeBubblesConfig {
            price_step: Some(0.1),
            ..VolumeBubblesConfig::default()
        }));
        Self { chart }
    }
}

impl eframe::App for LiveFeedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui_charts::theme::apply_to_egui(ui.ctx(), &Theme::dark());
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ui.ctx().global_style());

        egui::CentralPanel::default().show(ui, |ui| {
            // Asks the feed for what the chart needs and applies what it sent.
            self.chart.show(ui);
        });
    }
}

/// One-minute bars of a random walk up to the current minute, oldest first.
struct SyntheticFeed {
    bars: Vec<Bar>,
}

impl SyntheticFeed {
    fn new(count: usize) -> Self {
        let now = Utc::now().duration_trunc(TimeDelta::minutes(1)).unwrap();
        let bars = support::sample_bars(count, 100.0, TimeDelta::minutes(1), now);
        Self { bars }
    }
}

impl Datafeed for SyntheticFeed {
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        // The newest `count` bars that open before `before`. A feed that fetches remotely keeps
        // the reply and sends it from its own thread or task.
        let before = |bar: &Bar| request.before.is_none_or(|before| bar.time < before);
        let end = self.bars.partition_point(before);
        let start = end.saturating_sub(request.count);
        reply.send(Ok(HistoryPage {
            bars: self.bars[start..end].to_vec(),
            end_of_history: start == 0,
        }));
    }

    fn subscribe(&self, subscription: Subscription) {
        let sink = subscription.sink;
        let mut price = self.bars.last().map_or(100.0, |bar| bar.close);
        std::thread::spawn(move || {
            let mut rng = rand::rng();
            // The chart ends the subscription when its symbol or timeframe changes or it is
            // dropped.
            while sink.is_active() {
                price *= 1.0 + rng.random_range(-0.0005..0.0005);
                let size = rng.random_range(0.1..5.0);
                let side = [OrderSide::Buy, OrderSide::Sell][rng.random_range(0..2)];
                sink.push_tick(Tick::new(Utc::now(), price, size, Some(side)));
                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }
}
