//! Scripted README loops. Only pointer input goes through screen coordinates; chart state uses its public API.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_charts::model::LogicalRange;
use egui_charts::ui::drawing_toolbar::MagnetType;

const HALF_FRAMES: usize = 80;

pub(super) fn scenes() -> Vec<Scene> {
    super::scenes()
        .into_iter()
        .filter(|scene| matches!(scene.name, "hero" | "liquidity" | "drawings" | "replay"))
        .map(|mut scene| {
            // Keep gallery tiles focused on the interaction, with room for legible chart labels.
            scene.steps.retain(|step| match step {
                Step::Paper | Step::Position | Step::Limit | Step::Alert | Step::Panel(_) => false,
                Step::Drawings(_) => !matches!(scene.name, "hero" | "drawings"),
                _ => true,
            });
            if scene.name == "drawings" {
                scene.steps.push(Step::Visible(65));
            }
            scene.pointer = Some(vec2(0.25, 0.45));
            scene
        })
        .collect()
}

fn eased(frame: usize, from: usize, to: usize) -> f32 {
    let t = (frame.saturating_sub(from) as f32 / (to - from) as f32).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn button(h: &Harness<'_, DemoApp>, pos: Pos2, pressed: bool) {
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

fn click(h: &mut Harness<'_, DemoApp>, pos: Pos2) {
    button(h, pos, true);
    h.step();
    button(h, pos, false);
    h.step();
}

pub(super) fn shoot(scene: &Scene, light: bool, out: &Path) {
    let mut h = prepare(scene, light, 0.75, true);
    let theme = if light { "light" } else { "dark" };
    let out = out.join(format!("{}-{theme}", scene.name));
    std::fs::create_dir_all(&out).unwrap();
    let rect = h.state().chart.get_rendered_price_rect();
    let start = rect.min + rect.size() * vec2(0.25, 0.45);
    let across = rect.min + rect.size() * vec2(0.75, 0.40);
    let range = h.state().chart.visible_logical_range().unwrap();
    let now: chrono::DateTime<chrono::Utc> = if scene.name == "liquidity" {
        "2026-10-27T19:54:50Z"
    } else {
        "2026-10-27T19:29:50Z"
    }
    .parse()
    .unwrap();
    let bars = h.state().chart.bars(SeriesId::MAIN);
    let n = bars.len();
    let (low, _) = extremes(bars, n - 54, n - 42);
    let (_, high) = extremes(bars, n - 30, n - 18);
    let (_, moved) = extremes(bars, n - 16, n - 6);
    let mapping = h.state().chart.get_chart_mapping();
    let a = mapping.to_screen(low.0, low.1);
    let b = mapping.to_screen(high.0, high.1);
    let c = mapping.to_screen(moved.0, moved.1);
    // Click just outside the low wick so the OHLC assertion also proves that the magnet snapped.
    let first_click = a + vec2(1.0, 2.0);
    let mut pointer = start;
    h.hover_at(pointer);
    // Ignore the variable number of initialization passes when timing UI animations.
    for warmup in 0..10 {
        h.input_mut().time = Some(999.5 + warmup as f64 * 0.05);
        h.step();
    }
    let first_bars = h.state().chart.bars(SeriesId::MAIN).to_vec();
    for frame in 0..=HALF_FRAMES {
        h.input_mut().time = Some(1_000.0 + frame as f64 * 0.05);
        match scene.name {
            "hero" => {
                if frame <= 32 {
                    h.state_mut().advance_sample_to(
                        now + chrono::Duration::milliseconds(frame as i64 * 250),
                    );
                    pointer = start.lerp(across, eased(frame, 0, 32));
                } else if frame <= 56 {
                    let t = eased(frame, 32, 56);
                    assert!(h.state_mut().chart.set_visible_logical_range(LogicalRange {
                        left: range.left + 28.0 * t,
                        right: range.right - 4.0 * t,
                    }));
                } else {
                    if frame == 57 {
                        button(&h, pointer, true);
                    }
                    pointer = across + vec2(-100.0 * eased(frame, 56, 80), 0.0);
                    if frame == 80 {
                        button(&h, pointer, false);
                    }
                }
            }
            "liquidity" => {
                h.state_mut()
                    .advance_sample_to(now + chrono::Duration::milliseconds(frame as i64 * 500));
                pointer = start.lerp(across, eased(frame, 0, HALF_FRAMES));
            }
            "drawings" => {
                if frame == 1 {
                    h.state_mut()
                        .handle_drawing_toolbar(DrawingToolbarAction::SetMagnetType(
                            MagnetType::Strong,
                        ));
                    h.state_mut()
                        .handle_drawing_toolbar(DrawingToolbarAction::SelectTool(
                            DrawingToolType::TrendLine,
                        ));
                }
                pointer = if frame <= 16 {
                    start.lerp(first_click, eased(frame, 0, 16))
                } else if frame <= 48 {
                    first_click.lerp(b, eased(frame, 16, 48))
                } else {
                    b.lerp(c, eased(frame, 56, 76))
                };
                h.hover_at(pointer);
                h.step();
                if frame == 16 || frame == 48 {
                    click(&mut h, pointer);
                }
                if frame == 48 {
                    let drawings = &h.state().chart.drawings().drawings;
                    let d = drawings.last().expect("pointer placed a trend line");
                    assert_eq!(d.tool_type, DrawingToolType::TrendLine);
                    assert!(d.completed);
                    for (point, expected) in d.chart_points.iter().zip([low, high]) {
                        assert!((point.bar_idx - expected.0 as f32).abs() < 0.01);
                        // Screen projection is f32; allow far less than the sample's 0.01 tick.
                        assert!(
                            (point.price - expected.1).abs() < 0.0001,
                            "magnet must snap to OHLC: bar {}, actual {}, expected {}",
                            point.bar_idx,
                            point.price,
                            expected.1
                        );
                    }
                }
                if frame == 56 {
                    button(&h, pointer, true);
                }
                if frame == 77 {
                    button(&h, pointer, false);
                    h.step();
                    let d = h.state().chart.drawings().drawings.last().unwrap();
                    assert!(
                        (d.chart_points[1].bar_idx - moved.0 as f32).abs() < 0.01,
                        "handle must move"
                    );
                }
            }
            "replay" => {
                // Paused replay steps through the controller's keyboard transport, never its wall clock.
                if frame > 0 && frame % 8 == 0 {
                    h.key_press(Key::ArrowRight);
                }
                pointer = start.lerp(across, eased(frame, 0, HALF_FRAMES));
            }
            _ => unreachable!(),
        }
        h.hover_at(pointer);
        h.step();
        // A paused drawing scene should show the finished line before its rewind and placement.
        let index = if scene.name == "drawings" {
            HALF_FRAMES - frame
        } else {
            frame
        };
        save_frame(&mut h, &out.join(format!("{index:04}.png")));
    }
    if matches!(scene.name, "hero" | "liquidity" | "replay") {
        assert_ne!(
            h.state().chart.bars(SeriesId::MAIN),
            first_bars,
            "{} must advance its data",
            scene.name
        );
    }
    // Rewind the rendered trajectory, including the feed, without jumping the clock or rebuilding the UI. The loop
    // wraps from the last copy to frame 0, so frame 0 is not repeated at the end.
    for (offset, frame) in (1..HALF_FRAMES).rev().enumerate() {
        std::fs::copy(
            out.join(format!("{frame:04}.png")),
            out.join(format!("{:04}.png", HALF_FRAMES + 1 + offset)),
        )
        .unwrap();
    }
}
