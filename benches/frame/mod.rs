//! The headless frame the benches time: layout and painting through `Context::run_ui`, then
//! `Context::tessellate`, with no window or GPU.

use std::time::{Duration, Instant};

use egui::{Event, Pos2, RawInput, Rect, Shape, Vec2};

/// Frames run before the measured ones.
const WARMUP: usize = 60;

/// One measured frame.
pub struct Sample {
    /// Layout, painting and tessellation.
    pub frame: Duration,
    pub tessellate: Duration,
    /// Shapes painted, counting each member of a `Shape::Vec`.
    pub shapes: usize,
}

/// Runs `ui` for 60 warm-up frames and then `frames` measured ones on a `screen`-sized
/// window with the pointer at `pointer`, and returns the measured frames.
pub fn run(
    screen: Vec2,
    pointer: Pos2,
    frames: usize,
    mut ui: impl FnMut(&mut egui::Ui),
) -> Vec<Sample> {
    let ctx = egui::Context::default();
    let mut samples = Vec::with_capacity(frames);
    for i in 0..WARMUP + frames {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)),
            time: Some(i as f64 / 60.0),
            events: vec![Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let start = Instant::now();
        let mut out = ctx.run_ui(input, &mut ui);
        let painted = start.elapsed();
        out.textures_delta.clear(); // no renderer uploads the font atlas
        let shapes = out.shapes.iter().map(|s| count(&s.shape)).sum();
        let start = Instant::now();
        // Dropped inside the timing, as a renderer drops it every frame.
        std::hint::black_box(ctx.tessellate(out.shapes, out.pixels_per_point));
        let tessellate = start.elapsed();
        if i >= WARMUP {
            samples.push(Sample {
                frame: painted + tessellate,
                tessellate,
                shapes,
            });
        }
    }
    samples
}

fn count(shape: &Shape) -> usize {
    match shape {
        Shape::Vec(shapes) => shapes.iter().map(count).sum(),
        _ => 1,
    }
}
