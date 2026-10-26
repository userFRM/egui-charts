//! Captures the README images from the demo app in `examples/demo`, rendered offscreen by egui_kittest's wgpu
//! renderer. The demo's controller and sample data are compiled in unchanged. Each scene sets its state through the
//! controller's own action handlers and the public chart API, so no step depends on where a control is drawn.
//!
//! Run from this directory: `cargo run --release -- <out-dir> [scene-filter]`
//! Output: `<scene>-<theme>@2x.png` for the dark and light themes. Each capture sets its theme on the app and with
//! `Chart::set_theme` before the first step. `docs/images` holds the chart scenes as written. `hero-<theme>.png`
//! and `themes-<theme>.png` are the `hero` and `themes` window captures placed in a frame after capture, without
//! changing a pixel inside the windows.

#[path = "../../examples/demo/controller.rs"]
#[expect(dead_code, reason = "the capture tool uses part of the demo")]
mod controller;
#[path = "../../examples/demo/panel_alerts.rs"]
mod panel_alerts;
#[path = "../../examples/demo/sampledata.rs"]
#[expect(dead_code, reason = "the capture tool uses part of the demo")]
mod sampledata;

use controller::DemoApp;
use egui::containers::panel::PanelState;
use egui::{Context, Rect, Vec2, ViewportId, vec2};
use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
use egui_charts::model::{Bar, ChartType as ModelChartType, Timeframe};
use egui_charts::studies::{self, Indicator};
use egui_charts::theme::Theme;
use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_kittest::Harness;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use std::path::Path;

/// The demo catalog's neutral instrument, so no real ticker is shown with synthetic prices.
const SAMPLE: &str = "SAMPLE";
/// Images above this size are reported, since each one is downloaded with the README.
const MAX_BYTES: u64 = 1_000_000;

enum Step {
    /// Switch symbol, as picking one in symbol search does.
    Symbol(&'static str),
    /// Pick an interval in the top toolbar's interval menu.
    Interval(Timeframe),
    /// Pick a style in the top toolbar's chart type menu.
    ChartType(ModelChartType),
    /// "Remove all indicators" in the drawing toolbar.
    ClearStudies,
    /// Add a study with its default parameters.
    Study(fn() -> Box<dyn Indicator>),
    /// Add drawings placed from the bars on the chart.
    Drawings(fn(&[Bar]) -> Vec<Drawing>),
}

enum Crop {
    Window,
    /// The chart and its panes, between the demo's toolbars and side bars.
    Chart,
}

struct Scene {
    name: &'static str,
    size: Vec2,
    crop: Crop,
    steps: Vec<Step>,
    /// Pointer position as a fraction of the chart area, which shows the crosshair there.
    pointer: Option<Vec2>,
}

fn study<T: Indicator + Default + 'static>() -> Box<dyn Indicator> {
    Box::new(T::default())
}

/// Logical window sizes: a full-width README image, and one of a pair shown side by side.
const WIDE: Vec2 = vec2(1100.0, 700.0);
const HALF: Vec2 = vec2(900.0, 560.0);

fn scenes() -> Vec<Scene> {
    use Step::*;
    let scene = |name, size, crop, steps| Scene {
        name,
        size,
        crop,
        steps,
        pointer: None,
    };
    let chart_type = |name, style| {
        scene(
            name,
            HALF,
            Crop::Chart,
            vec![Symbol(SAMPLE), ClearStudies, ChartType(style)],
        )
    };
    let picks = [
        study::<studies::IchimokuCloud>,
        study::<studies::Stochastic>,
        study::<studies::ADX>,
    ];
    vec![
        Scene {
            name: "hero",
            size: vec2(1440.0, 900.0),
            crop: Crop::Window,
            steps: vec![Symbol(SAMPLE), Drawings(retracement)],
            pointer: Some(vec2(0.62, 0.3)),
        },
        chart_type("type-heikin-ashi", ModelChartType::Heikin),
        chart_type("type-baseline", ModelChartType::Baseline),
        chart_type("type-kagi", ModelChartType::Kagi),
        chart_type("type-point-and-figure", ModelChartType::PointAndFigure),
        scene(
            "studies",
            WIDE,
            Crop::Chart,
            [Symbol(SAMPLE), ClearStudies]
                .into_iter()
                .chain(picks.map(Study))
                .collect(),
        ),
        scene(
            "drawings",
            WIDE,
            Crop::Window,
            vec![Symbol(SAMPLE), ClearStudies, Drawings(analysis)],
        ),
        scene(
            "themes",
            WIDE,
            Crop::Window,
            vec![
                Symbol(SAMPLE),
                ClearStudies,
                Study(study::<studies::SuperTrend>),
                Study(study::<studies::RSI>),
            ],
        ),
        scene(
            "tpo",
            HALF,
            Crop::Chart,
            vec![
                Symbol(SAMPLE),
                Interval(Timeframe::Min30),
                ClearStudies,
                ChartType(ModelChartType::TimePriceOpportunity),
            ],
        ),
        scene(
            "volume-profile",
            HALF,
            Crop::Chart,
            vec![
                Symbol(SAMPLE),
                Interval(Timeframe::Min30),
                ClearStudies,
                Study(study::<studies::CVD>),
                Drawings(profile),
            ],
        ),
    ]
}

// ---------------------------------------------------------------- drawings, placed from the data

/// The demo shows the last 100 bars.
const VISIBLE: usize = 100;

/// Index and price of the lowest low and of the highest high among `bars[from..to]`.
fn extremes(bars: &[Bar], from: usize, to: usize) -> ((usize, f64), (usize, f64)) {
    let mut low = (from, f64::INFINITY);
    let mut high = (from, f64::NEG_INFINITY);
    for (i, b) in bars.iter().enumerate().take(to).skip(from) {
        if b.low < low.1 {
            low = (i, b.low);
        }
        if b.high > high.1 {
            high = (i, b.high);
        }
    }
    (low, high)
}

fn drawing(tool: DrawingToolType, color: [u8; 4], points: &[(usize, f64)]) -> Drawing {
    let mut d = Drawing::with_color(0, tool, color);
    d.chart_points = points
        .iter()
        .map(|&(i, p)| ChartPoint::new(i as f32, p))
        .collect();
    d.completed = true;
    d
}

const BLUE: [u8; 4] = [41, 98, 255, 255];
const AMBER: [u8; 4] = [255, 152, 0, 255];

/// A retracement over the largest swing on screen, drawn from its first extreme to its second.
fn retracement(bars: &[Bar]) -> Vec<Drawing> {
    let n = bars.len();
    let (low, high) = extremes(bars, n.saturating_sub(VISIBLE), n);
    let (a, b) = if low.0 < high.0 {
        (low, high)
    } else {
        (high, low)
    };
    vec![drawing(
        DrawingToolType::FibonacciRetracement,
        AMBER,
        &[a, b],
    )]
}

/// The retracement plus a trend line through the lows of each half of the screen and a box around the last bars.
fn analysis(bars: &[Bar]) -> Vec<Drawing> {
    let n = bars.len();
    let start = n.saturating_sub(VISIBLE);
    let mid = start + VISIBLE / 2;
    let (first_low, _) = extremes(bars, start, mid);
    let (second_low, _) = extremes(bars, mid, n);
    let (box_low, box_high) = extremes(bars, n.saturating_sub(12), n);
    let mut out = retracement(bars);
    let mut trend = drawing(DrawingToolType::TrendLine, BLUE, &[first_low, second_low]);
    trend.extend_right = true;
    out.push(trend);
    let mut range = drawing(
        DrawingToolType::Rect,
        BLUE,
        &[(box_low.0.min(box_high.0), box_high.1), (n - 1, box_low.1)],
    );
    range.fill_color = Some([41, 98, 255, 40]);
    out.push(range);
    out
}

/// A fixed-range volume profile over the last session on screen.
fn profile(bars: &[Bar]) -> Vec<Drawing> {
    let n = bars.len();
    let (low, _) = extremes(bars, n.saturating_sub(48), n);
    vec![drawing(
        DrawingToolType::FixedRangeVolumeProfile,
        BLUE,
        &[(n.saturating_sub(48), low.1), (n - 1, low.1)],
    )]
}

// ---------------------------------------------------------------- capture

fn apply(app: &mut DemoApp, ctx: &Context, step: &Step) {
    match step {
        Step::Symbol(s) => app.switch_symbol(s),
        Step::Interval(tf) => app.handle_top_toolbar(ctx, TopToolbarAction::IntervalSelected(*tf)),
        Step::ChartType(t) => app.handle_top_toolbar(ctx, TopToolbarAction::ChartStyleSelected(*t)),
        Step::ClearStudies => app.handle_drawing_toolbar(DrawingToolbarAction::RemoveAllIndicators),
        Step::Study(make) => {
            app.chart.add_indicator(make());
        }
        Step::Drawings(place) => {
            let dm = app.chart.drawings_mut();
            place(&app.bars)
                .into_iter()
                .for_each(|d| dm.add_synced_drawing(d));
        }
    }
}

/// The chart area: the window minus the demo's toolbars and side bars, from the panel rects egui stored last frame.
fn chart_rect(ctx: &Context) -> Rect {
    let panel = |id: &str| {
        // The demo shows its panels in the root Ui, and egui keys a panel's state by its parent Ui.
        PanelState::load(ctx, ViewportId::ROOT.root_ui_id().with(id))
            .unwrap_or_else(|| panic!("no demo panel `{id}`"))
            .outer_rect
    };
    let mut r = ctx.content_rect();
    r.min.y = panel("symbol_header").max.y.max(panel("toolbar").max.y);
    r.min.x = panel("drawing_toolbar").max.x;
    r.max.x = panel("widget_bar").min.x;
    r.max.y = panel("bottom").min.y;
    r
}

fn shoot(scene: &Scene, light: bool, scale: f32, out: &Path) {
    let mut h = Harness::builder()
        .with_size(scene.size)
        .with_pixels_per_point(scale)
        // The chart draws its own crosshair; kittest would paint a pointer arrow over it.
        .with_render_cursor(false)
        // Glyphs no bundled font has (arrows, small triangles in the toolbars) stay boxes, as the demo shows them.
        .allow_missing_glyphs()
        // A capture run, not an accessibility test: the demo has icon buttons without an accessible name.
        .with_accessibility_check(false)
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| DemoApp::new(cc));
    h.run_steps(3);
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    app.theme = if light { Theme::light() } else { Theme::dark() };
    app.chart.set_theme(&ctx, &app.theme);
    for step in &scene.steps {
        apply(app, &ctx, step);
    }
    h.run_steps(3);
    let area = chart_rect(&ctx);
    if let Some(f) = scene.pointer {
        h.hover_at(area.min + area.size() * f);
        h.run_steps(2);
    }
    let mut img = h
        .render()
        .unwrap_or_else(|e| panic!("render {}: {e}", scene.name));
    if let Crop::Chart = scene.crop {
        let px = area * scale;
        let [x, y, width, height] =
            [px.min.x, px.min.y, px.width(), px.height()].map(|v| v.round() as u32);
        img = image::imageops::crop_imm(&img, x, y, width, height).to_image();
    }
    let theme = if light { "light" } else { "dark" };
    let path = out.join(format!("{}-{theme}@{scale}x.png", scene.name));
    let file = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    img.write_with_encoder(PngEncoder::new_with_quality(
        file,
        CompressionType::Best,
        FilterType::Adaptive,
    ))
    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let bytes = std::fs::metadata(&path).unwrap().len();
    if bytes > MAX_BYTES {
        eprintln!(
            "warning: {} is {bytes} bytes, above {MAX_BYTES}",
            path.display()
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(out) = args.get(1) else {
        eprintln!("usage: capture <out-dir> [scene-filter]");
        std::process::exit(2);
    };
    let filter = args.get(2).map(String::as_str).unwrap_or("");
    let out = Path::new(out);
    std::fs::create_dir_all(out).unwrap();
    for scene in scenes().iter().filter(|s| s.name.contains(filter)) {
        for light in [false, true] {
            shoot(scene, light, 2.0, out);
        }
        eprintln!("captured {}", scene.name);
    }
}
