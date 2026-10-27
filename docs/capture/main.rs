//! Captures the README images from the demo app in `examples/demo`, rendered offscreen by egui_kittest's wgpu
//! renderer. The demo's controller and sample data are compiled in unchanged. Each scene sets its state through the
//! controller's own action handlers and the public chart API, so no step depends on where a control is drawn.
//!
//! Run from this directory: `cargo run --release -- <out-dir> [scene-filter]`
//! Output: `<scene>-<theme>.png` for the dark and light themes, from a 1440x900 window at one pixel per point.
//! Each capture sets its theme on the app and with `Chart::set_theme` before the first step. `docs/images` holds
//! them converted unchanged to lossless WebP: `for f in <out-dir>/*.png; do cwebp -quiet -lossless -z 9 "$f" -o
//! docs/images/$(basename "${f%.png}").webp; done`.

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
use egui_charts::model::{OrderRequest, OrderSide, OrderType};
use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
use egui_charts::ui::widget_bar::RightBarIcon;
use egui_charts::widget::{ChartLayoutMode, SeriesId};
use egui_kittest::Harness;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use std::path::Path;

/// The demo catalog's neutral instrument, so no real ticker is shown with synthetic prices.
const SAMPLE: &str = "SAMPLE";
/// Images above this size are reported, since each one is downloaded with the README.
const MAX_BYTES: u64 = 400_000;

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
    /// Pick an arrangement in the layout menu.
    Grid(ChartLayoutMode),
    /// Make the chart in this slot of the layout the active one, as clicking it does.
    Slot(usize),
    /// Open or close a side panel from the right-hand bar.
    Panel(RightBarIcon),
    /// Show the liquidity heatmap and volume bubbles, as the toolbar toggle does.
    Heatmap,
    /// Start replay this many bars before the last one, as picking that bar does.
    Replay(usize),
    /// Connect the paper account in the broker picker.
    Paper,
    /// Stop the charts of a layout following each other's time range.
    UnsyncTime,
    /// Zoom the active chart to this many bars, ending at the latest.
    Visible(usize),
    /// Open the order ticket for a limit order this far below the last close, in percent.
    Ticket(f64),
}

enum Crop {
    Window,
    /// The chart and its panes, between the demo's toolbars and side bars.
    Chart,
}

struct Scene {
    name: &'static str,
    crop: Crop,
    steps: Vec<Step>,
    /// Pointer position as a fraction of the chart area, which shows the crosshair there.
    pointer: Option<Vec2>,
}

fn study<T: Indicator + Default + 'static>() -> Box<dyn Indicator> {
    Box::new(T::default())
}

/// Logical window size of every capture, written at one pixel per point.
const SIZE: Vec2 = vec2(1440.0, 900.0);

fn scenes() -> Vec<Scene> {
    use Step::*;
    let scene = |name, crop, steps| Scene {
        name,
        crop,
        steps,
        pointer: None,
    };
    // One chart per slot of a four-chart grid, each with its own style.
    let grid = |styles: [ModelChartType; 4], interval| {
        let mut steps = vec![Symbol(SAMPLE), Interval(interval), ClearStudies, Grid(ChartLayoutMode::Four)];
        for (slot, style) in styles.into_iter().enumerate() {
            steps.extend([Slot(slot), ChartType(style)]);
        }
        steps.push(Slot(0));
        steps
    };
    let picks = [
        study::<studies::IchimokuCloud>,
        study::<studies::Stochastic>,
        study::<studies::ADX>,
    ];
    vec![
        Scene {
            name: "hero",
            crop: Crop::Window,
            steps: vec![Symbol(SAMPLE), Interval(Timeframe::Min5), Drawings(retracement)],
            pointer: Some(vec2(0.62, 0.3)),
        },
        scene(
            "chart-types",
            Crop::Chart,
            grid(
                [
                    ModelChartType::Heikin,
                    ModelChartType::Renko,
                    ModelChartType::Kagi,
                    ModelChartType::PointAndFigure,
                ],
                Timeframe::Hour1,
            ),
        ),
        scene(
            "studies",
            Crop::Chart,
            [Symbol(SAMPLE), Interval(Timeframe::Min5), ClearStudies]
                .into_iter()
                .chain(picks.map(Study))
                .collect(),
        ),
        scene(
            "drawings",
            Crop::Window,
            vec![
                Symbol(SAMPLE), Interval(Timeframe::Min5),
                ClearStudies,
                Drawings(analysis),
                Panel(RightBarIcon::ObjectTree),
            ],
        ),
        scene(
            "order-flow",
            Crop::Chart,
            vec![
                Symbol(SAMPLE),
                Interval(Timeframe::Min5),
                ClearStudies,
                Grid(ChartLayoutMode::TwoHorizontal),
                UnsyncTime,
                Slot(0),
                ChartType(ModelChartType::VolumeFootprint),
                Interval(Timeframe::Min1),
                Visible(14),
                Slot(1),
                Interval(Timeframe::Min1),
                Visible(70),
                Heatmap,
                Slot(0),
            ],
        ),
        scene(
            "profiles",
            Crop::Chart,
            vec![
                Symbol(SAMPLE),
                Interval(Timeframe::Min30),
                ClearStudies,
                Grid(ChartLayoutMode::TwoHorizontal),
                UnsyncTime,
                Slot(0),
                ChartType(ModelChartType::TimePriceOpportunity),
                Slot(1),
                ChartType(ModelChartType::SessionVolumeProfile),
                Slot(0),
            ],
        ),
        scene(
            "trading",
            Crop::Window,
            vec![Symbol(SAMPLE), Interval(Timeframe::Min5), ClearStudies, Paper, Ticket(0.5)],
        ),
        scene(
            "layouts",
            Crop::Window,
            vec![
                Symbol(SAMPLE), Interval(Timeframe::Min5),
                ClearStudies,
                Study(study::<studies::RSI>),
                Grid(ChartLayoutMode::Three),
                Slot(1),
                Interval(Timeframe::Min15),
                Slot(2),
                ChartType(ModelChartType::Line),
                Slot(0),
            ],
        ),
        scene(
            "replay",
            Crop::Window,
            vec![Symbol(SAMPLE), Interval(Timeframe::Min5), ClearStudies, Replay(120)],
        ),
        scene(
            "themes",
            Crop::Window,
            vec![
                Symbol(SAMPLE), Interval(Timeframe::Min5),
                ClearStudies,
                Study(study::<studies::SuperTrend>),
                Study(study::<studies::RSI>),
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
        Step::Grid(mode) => app.handle_top_toolbar(ctx, TopToolbarAction::Layout(LayoutAction::SelectGrid(*mode))),
        Step::Slot(slot) => app.activate_chart_slot(*slot),
        Step::Panel(icon) => app.toggle_panel(*icon),
        Step::Heatmap => {
            app.chart.set_liquidity_heatmap(true);
            app.chart.set_volume_bubbles(Some(Default::default()));
        }
        Step::Replay(back) => {
            let n = app.chart.bars(SeriesId::MAIN).len();
            app.start_replay_at(n.saturating_sub(*back + 1));
        }
        Step::UnsyncTime => app.layout.sync.sync_time_axis = false,
        Step::Visible(count) => app.chart.set_visible_bars(*count),
        Step::Paper => app.connect_paper(),
        Step::Ticket(below) => {
            let close = app.chart.bars(SeriesId::MAIN).last().map_or(0.0, |b| b.close);
            let price = (close * (1.0 - below / 100.0) * 100.0).round() / 100.0;
            let request = OrderRequest::new(SAMPLE, OrderSide::Buy, 10.0, OrderType::Limit(price));
            app.trading.open_ticket(request);
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
    r.min.y = panel("toolbar").max.y;
    r.min.x = panel("drawing_toolbar").max.x;
    r.max.x = panel("widget_bar").min.x;
    r.max.y = panel("bottom").min.y;
    r
}

fn shoot(scene: &Scene, light: bool, scale: f32, out: &Path) {
    let mut h = Harness::builder()
        .with_size(SIZE)
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
    // The watchlist, open at start, lists real tickers beside synthetic prices.
    app.toggle_panel(RightBarIcon::Watchlist);
    // The demo's call to action belongs to the host application, not to the library.
    app.top_toolbar.call_to_action = None;
    for step in &scene.steps {
        apply(app, &ctx, step);
    }
    // Enough frames for the paper broker's replies and the replay window to arrive.
    h.run_steps(10);
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
    let path = out.join(format!("{}-{theme}.png", scene.name));
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
            shoot(scene, light, 1.0, out);
        }
        eprintln!("captured {}", scene.name);
    }
}
