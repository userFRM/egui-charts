//! Forward-only pointer scripts over the demo's own controls.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use pointer::CaptureApp;

pub(super) fn scenes() -> Vec<Scene> {
    use Step::*;
    vec![
        Scene {
            name: "tour",
            steps: vec![
                ClearStudies,
                Grid(ChartLayoutMode::Four),
                UnsyncTime,
                Slot(0),
                AddStudy("BB"),
                // Its twelve averages would fill this pane with their inputs.
                Study(study::<studies::MARibbon>),
                AddStudy("Vol Profile VR"),
                Visible(60),
                Slot(1),
                ChartType(ModelChartType::VolumeFootprint),
                Visible(7),
                Slot(2),
                ChartType(ModelChartType::TimePriceOpportunity),
                AddStudy("VWAP"),
                Visible(1200),
                Slot(3),
                Heatmap,
                Visible(28),
                Slot(0),
            ],
            pointer: Some(vec2(0.5, 0.62)),
        },
        Scene {
            name: "liquidity",
            steps: vec![ClearStudies, Visible(28), Heatmap],
            pointer: Some(vec2(0.3, 0.4)),
        },
        Scene {
            name: "drawings",
            steps: vec![ClearStudies, AddStudy("EMA"), Visible(65)],
            pointer: Some(vec2(0.3, 0.4)),
        },
        Scene {
            name: "replay",
            steps: vec![ClearStudies, AddStudy("EMA"), Visible(80)],
            pointer: Some(vec2(0.3, 0.4)),
        },
    ]
}

struct Timeline {
    h: Harness<'static, CaptureApp>,
    out: std::path::PathBuf,
    frame: usize,
    pointer: Pos2,
    live: bool,
    start: chrono::DateTime<chrono::Utc>,
}

impl Timeline {
    fn frame(&mut self) {
        let elapsed = self.frame as i64 * 1_000_000_000 / 60;
        let now = self.start + chrono::Duration::nanoseconds(elapsed);
        wall_time(now, Some(1_000_000_000_000 + elapsed));
        self.h.input_mut().time = Some(1_000.0 + self.frame as f64 / 60.0);
        if self.live && self.frame.is_multiple_of(6) {
            self.h.state_mut().advance_sample_to(
                self.start + chrono::Duration::milliseconds(self.frame as i64 * 1000 / 60),
            );
        }
        self.h.hover_at(self.pointer);
        let pointing_at_row = [
            egui::accesskit::Role::MenuItem,
            egui::accesskit::Role::MenuItemRadio,
            egui::accesskit::Role::ListBoxOption,
            egui::accesskit::Role::RadioButton,
        ]
        .into_iter()
        .any(|role| {
            self.h
                .query_all_by_role(role)
                .any(|node| node.rect().contains(self.pointer))
        });
        self.h.state_mut().pointing_at_row = pointing_at_row;
        self.h.step();
        wall_time(now, None);
        save_frame(
            &mut self.h,
            &self.out.join(format!("{:04}.png", self.frame)),
            CompressionType::Fast,
            FilterType::NoFilter,
        );
        self.frame += 1;
        if self.frame.is_multiple_of(120) {
            eprintln!("{}: {} frames", self.out.display(), self.frame);
        }
    }

    fn hold(&mut self, ms: usize) {
        for _ in 0..(ms * 60).div_ceil(1000) {
            self.frame();
        }
    }

    fn move_to(&mut self, to: Pos2, ms: usize) {
        let from = self.pointer;
        let count = (ms * 60).div_ceil(1000).max(1);
        for i in 1..=count {
            let t = i as f32 / count as f32;
            let eased = t * t * (3.0 - 2.0 * t);
            let arc = vec2(0.0, (std::f32::consts::PI * eased).sin() * 9.0);
            self.pointer = from.lerp(to, eased) + arc;
            self.frame();
        }
    }

    fn button(&mut self, pressed: bool) {
        self.h.event(Event::PointerButton {
            pos: self.pointer,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        self.frame();
    }

    fn click(&mut self) {
        self.button(true);
        self.button(false);
        self.hold(100);
    }

    fn label(&mut self, label: &str, ms: usize) {
        let to = self.h.get_by_label(label).rect().center();
        self.move_to(to, ms);
        self.click();
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        for pressed in [true, false] {
            self.h.event(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
            self.frame();
        }
    }

    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.h.event(Event::Text(ch.to_string()));
            self.hold(83);
        }
    }

    fn scroll(&mut self, delta: f32) {
        for _ in 0..30 {
            self.h.event(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: vec2(0.0, delta / 30.0),
                modifiers: Modifiers::NONE,
            });
            self.frame();
        }
    }

    fn drag(&mut self, to: Pos2, ms: usize) {
        self.button(true);
        self.move_to(to, ms);
        self.button(false);
    }

    fn finish(&mut self, seconds: usize) {
        assert!(
            self.frame <= seconds * 60,
            "script exceeds its duration: {}",
            self.frame
        );
        while self.frame < seconds * 60 {
            self.frame();
        }
    }
}

fn draw_line(t: &mut Timeline) {
    let chart = &t.h.state().chart;
    let bars = chart.bars(SeriesId::MAIN);
    let n = bars.len();
    let mapping = chart.get_chart_mapping();
    // Keep gestures clear of the legend and the volume overlay's own scale.
    let mut clear = chart.get_rendered_price_rect();
    let height = clear.height();
    clear.min.y += if chart.config().show_quote_boxes {
        110.0
    } else {
        65.0
    };
    if chart.config().show_volume {
        clear.max.y -= height * chart.config().volume_height_fraction;
    }
    let choose = |from: usize, to: usize, high: bool| {
        let points = (from..to)
            .map(|i| (i, if high { bars[i].high } else { bars[i].low }))
            .filter(|&(i, price)| clear.contains(mapping.to_screen(i, price)));
        if high {
            points.max_by(|a, b| a.1.total_cmp(&b.1))
        } else {
            points.min_by(|a, b| a.1.total_cmp(&b.1))
        }
        .expect("a visible swing point away from chart controls")
    };
    let low = choose(n - 54, n - 35, false);
    let high = choose(n - 34, n - 18, true);
    let moved = choose(n - 17, n - 6, true);
    let a = mapping.to_screen(low.0, low.1);
    let b = mapping.to_screen(high.0, high.1);
    let c = mapping.to_screen(moved.0, moved.1);
    t.label("Magnet mode", 300);
    if t.h.query_by_label("Got it").is_some() {
        t.label("Got it", 250);
    }
    t.label("Trend Line", 300);
    let row =
        t.h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Trend Line")
            .rect()
            .center();
    t.move_to(row, 250);
    t.click();
    t.move_to(a + vec2(1.0, 2.0), 600);
    t.click();
    t.move_to(b, 900);
    t.click();
    let d =
        t.h.state()
            .chart
            .drawings()
            .drawings
            .last()
            .expect("pointer placed a line");
    assert_eq!(d.tool_type, DrawingToolType::TrendLine);
    assert!(d.completed);
    for (point, expected) in d.chart_points.iter().zip([low, high]) {
        assert!((point.bar_idx - expected.0 as f32).abs() < 0.01);
        assert!(
            (point.price - expected.1).abs() < 0.0001,
            "magnet must snap to OHLC"
        );
    }
    t.hold(300);
    t.drag(c, 800);
    let d = t.h.state().chart.drawings().drawings.last().unwrap();
    assert!(
        (d.chart_points[1].bar_idx - moved.0 as f32).abs() < 0.01,
        "handle must move"
    );
}

/// With the replay button armed, clicks the bar `back` bars before the latest to start replay there.
fn click_replay_bar(t: &mut Timeline, back: usize, ms: usize) {
    let chart = &t.h.state().chart;
    let bars = chart.bars(SeriesId::MAIN);
    let index = bars.len().checked_sub(back + 1).expect("replay bar exists");
    let bar = &bars[index];
    let to = chart
        .get_chart_mapping()
        .to_screen(index, (bar.open + bar.close) * 0.5);
    assert!(
        chart.get_rendered_price_rect().contains(to),
        "replay candle must be visible"
    );
    t.move_to(to, ms);
    t.click();
    assert!(
        t.h.state().chart.replay_time().is_some(),
        "clicking the candle must start replay"
    );
}

/// The tour, in milliseconds and logical points of the 1280x720 demo.
fn tour(t: &mut Timeline, home: Pos2) {
    t.hold(500);
    t.move_to(pos2(440.0, 245.0), 900);
    t.scroll(-500.0);
    t.drag(pos2(490.0, 245.0), 650);
    t.move_to(pos2(800.0, 530.0), 900);
    t.label("Chart layout", 350);
    t.hold(400);
    t.label("1 Left, 3 Right", 400);
    t.hold(300);
    t.label("Indicators", 400);
    t.hold(300);
    let input =
        t.h.get_by_role(egui::accesskit::Role::TextInput)
            .rect()
            .center();
    t.move_to(input, 300);
    t.click();
    t.type_text("RSI");
    t.hold(400);
    let count = t.h.state().chart.indicators().count();
    t.label("Relative Strength Index", 300);
    assert_eq!(
        t.h.state().chart.indicators().count(),
        count + 1,
        "study row must add the study"
    );
    t.key(Key::Escape, Modifiers::NONE);
    t.key(Key::Escape, Modifiers::NONE);
    t.hold(500);
    t.label("Chart layout", 300);
    t.label("Single", 350);
    t.hold(200);
    t.label("Candles", 250);
    t.label("Bars", 250);
    t.hold(400);
    t.label("Bars", 250);
    t.label("Candles", 250);
    t.move_to(pos2(850.0, 330.0), 250);
    t.scroll(300.0);
    t.hold(400);
    t.live = false;
    draw_line(t);
    t.key(Key::Escape, Modifiers::NONE);
    t.label("Watchlist and details", 400);
    t.hold(300);
    // Accessible row bounds can be outside the list's clip; reveal the last symbol first.
    t.move_to(pos2(1120.0, 260.0), 250);
    t.scroll(-500.0);
    let row =
        t.h.get_by_role_and_label(egui::accesskit::Role::ListBoxOption, "SAMPLE")
            .rect()
            .center();
    assert!(
        (110.0..380.0).contains(&row.y),
        "scroll the symbol into the visible watchlist before pointing at it"
    );
    t.move_to(row, 300);
    t.hold(500);
    // Replay and its restoration then stay in the active chart's own synchronization group.
    t.h.state_mut()
        .layout
        .set_sync_group(0, Some("replay".to_owned()));
    t.label("Replay", 400);
    t.hold(400);
    click_replay_bar(t, 10, 500);
    t.hold(300);
    for _ in 0..3 {
        t.label("Next bar (Right arrow)", 200);
        t.hold(150);
    }
    let before = t.h.state().chart.replay_time().expect("replay is active");
    t.label("Play (Space)", 250);
    t.hold(3000);
    t.label("Pause (Space)", 250);
    assert!(
        t.h.state().chart.replay_time().unwrap() > before,
        "automatic playback must advance the historical bars"
    );
    t.label("Watchlist and details", 350);
    t.key(Key::Z, Modifiers::COMMAND);
    t.key(Key::Z, Modifiers::COMMAND);
    t.label("Chart layout", 350);
    t.label("2x2 Grid", 350);
    t.label("Exit replay", 300);
    t.h.state_mut().layout.set_sync_group(0, None);
    t.move_to(home, 500);
    t.finish(30);
}

fn wall_time(now: chrono::DateTime<chrono::Utc>, monotonic: Option<i64>) {
    let path = std::env::var("CAPTURE_CLOCK").expect("motion capture needs regenerate.sh's clock");
    std::fs::write(
        path,
        format!(
            "{} {} {}",
            now.timestamp(),
            now.timestamp_subsec_nanos(),
            monotonic.unwrap_or(-1)
        ),
    )
    .unwrap();
}

pub(super) fn shoot(scene: &Scene, light: bool, out: &Path) {
    wall_time("2026-10-27T19:29:50Z".parse().unwrap(), None);
    let mut h = prepare(scene, light, 2.0, true);
    let start = if matches!(scene.name, "tour" | "liquidity") {
        "2026-10-27T19:54:50Z"
    } else {
        "2026-10-27T19:29:50Z"
    }
    .parse()
    .unwrap();
    wall_time(start, None);
    let theme = if light { "light" } else { "dark" };
    let out = out.join(format!("{}-{theme}", scene.name));
    std::fs::create_dir_all(&out).unwrap();
    let rect = h.state().chart.get_rendered_price_rect();
    let pointer = rect.min + rect.size() * scene.pointer.expect("motion scenes set a pointer");
    // Initialization work can take a variable number of passes; settle the UI on a fixed clock.
    for i in 0..60 {
        h.input_mut().time = Some(999.0 + i as f64 / 60.0);
        h.hover_at(pointer);
        h.step();
    }
    let mut t = Timeline {
        h,
        out,
        frame: 0,
        pointer,
        live: matches!(scene.name, "tour" | "liquidity"),
        start,
    };
    match scene.name {
        "tour" => tour(&mut t, pointer),
        "drawings" => {
            t.hold(500);
            draw_line(&mut t);
            t.hold(800);
            t.key(Key::Z, Modifiers::COMMAND);
            t.hold(200);
            t.key(Key::Z, Modifiers::COMMAND);
            t.move_to(pointer, 600);
            t.finish(7);
        }
        "liquidity" => {
            let bars = t.h.state().chart.bars(SeriesId::MAIN).to_vec();
            t.hold(500);
            t.move_to(rect.min + rect.size() * vec2(0.8, 0.5), 3000);
            t.move_to(pointer, 3000);
            t.finish(7);
            assert_ne!(
                t.h.state().chart.bars(SeriesId::MAIN),
                bars,
                "feed must advance"
            );
        }
        "replay" => {
            t.hold(400);
            t.label("Replay", 400);
            // Close to the latest bar, so the loop's dissolve meets almost the same view.
            click_replay_bar(&mut t, 6, 700);
            let started = t.h.state().chart.replay_time();
            t.hold(300);
            for _ in 0..5 {
                t.label("Next bar (Right arrow)", 250);
                t.hold(300);
            }
            assert!(
                t.h.state().chart.replay_time() > started,
                "replay must advance"
            );
            t.move_to(pointer, 700);
            t.finish(7);
        }
        _ => unreachable!(),
    }
    eprintln!("captured {} {theme}: {} frames", scene.name, t.frame);
}
