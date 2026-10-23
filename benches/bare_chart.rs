//! Frame cost of `examples/bare_chart.rs`: headless egui frames (layout, painting and
//! tessellation; no window or GPU) on its sample data, with the pointer over the chart.
//! `BARE_CHART_FRAMES` sets the measured frames (default 600) after 60 warm-up frames.

use std::time::Duration;

use egui::{pos2, vec2};

#[path = "../examples/bare_chart.rs"]
#[expect(
    dead_code,
    reason = "the bench drives the example's frame, not its window"
)]
mod example;
#[expect(dead_code, reason = "this bench reports the whole frame only")]
mod frame;

fn main() {
    let frames: usize =
        std::env::var("BARE_CHART_FRAMES").map_or(600, |v| v.parse().expect("a frame count"));
    let mut app = example::BareChart::new();
    let mut times: Vec<Duration> =
        frame::run(vec2(1280.0, 720.0), pos2(640.0, 300.0), frames, |ui| {
            app.ui(ui)
        })
        .iter()
        .map(|s| s.frame)
        .collect();
    times.sort();
    let us = |p: f64| {
        times
            .get(((times.len().max(1) - 1) as f64 * p) as usize)
            .map_or(0.0, |d| d.as_secs_f64() * 1e6)
    };
    println!(
        "frames={frames} median_us={:.1} p95_us={:.1}",
        us(0.5),
        us(0.95)
    );
}
