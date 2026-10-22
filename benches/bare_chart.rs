//! Frame cost of `examples/bare_chart.rs`: headless egui frames (layout, painting and
//! tessellation; no window or GPU) on its sample data, with the pointer over the chart.
//! `BARE_CHART_FRAMES` sets the measured frames (default 600) after 60 warm-up frames.

use std::time::{Duration, Instant};

use egui::{Event, Pos2, RawInput, Rect, pos2, vec2};

#[path = "../examples/bare_chart.rs"]
#[expect(
    dead_code,
    reason = "the bench drives the example's frame, not its window"
)]
mod example;

const WARMUP: usize = 60;

fn main() {
    let frames: usize =
        std::env::var("BARE_CHART_FRAMES").map_or(600, |v| v.parse().expect("a frame count"));
    let ctx = egui::Context::default();
    let mut app = example::BareChart::new();
    let mut times: Vec<Duration> = Vec::with_capacity(frames);
    for i in 0..WARMUP + frames {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0))),
            time: Some(i as f64 / 60.0),
            events: vec![Event::PointerMoved(pos2(640.0, 300.0))],
            ..Default::default()
        };
        let start = Instant::now();
        let mut out = ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear(); // no renderer uploads the font atlas
        std::hint::black_box(ctx.tessellate(out.shapes, out.pixels_per_point));
        if i >= WARMUP {
            times.push(start.elapsed());
        }
    }
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
