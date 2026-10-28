//! Captures the README images from the demo app in `examples/demo`, rendered offscreen by egui_kittest's wgpu
//! renderer. The demo starts at a fixed timestamp with its sample feed paused. Each scene sets its state through the
//! controller's own action handlers and the public chart API, so no step depends on where a control is drawn.
//!
//! Run from this directory: `cargo run --release -- <out-dir> [scene-filter]`
//! Output: `<scene>-<theme>.png` for the dark and light themes, from a 1600x1000 window at 1.5 pixels per point.
//! Each capture sets its theme on the app and with `Chart::set_theme` before the first step. `docs/images` holds
//! them converted unchanged to lossless WebP: `for f in <out-dir>/*.png; do cwebp -quiet -lossless -z 9 "$f" -o
//! docs/images/$(basename "${f%.png}").webp; done`.

#[path = "../../examples/demo/controller.rs"]
#[expect(dead_code, reason = "only the captured scenes are exercised")]
mod controller;
#[path = "../../examples/demo/panel_alerts.rs"]
mod panel_alerts;
#[path = "../../examples/demo/sampledata.rs"]
#[expect(dead_code, reason = "only the captured scenes are exercised")]
mod sampledata;

use controller::DemoApp;
use egui::containers::panel::PanelState;
use egui::{Context, Rect, Vec2, ViewportId, vec2};
use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
use egui_charts::model::{Alert, AlertCondition, OrderRequest, OrderSide, OrderType};
use egui_charts::model::{Bar, ChartType as ModelChartType, Timeframe};
use egui_charts::studies::{self, Indicator};
use egui_charts::theme::Theme;
use egui_charts::ui::drawing_toolbar::DrawingToolbarAction;
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
use egui_charts::ui::widget_bar::RightBarIcon;
use egui_charts::widget::ChartLayoutMode;
use egui_charts::widget::SeriesId;
use egui_kittest::{Harness, kittest::Queryable};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use std::path::Path;

/// The demo catalog's neutral instrument, so no real ticker is shown with synthetic prices.
const SAMPLE: &str = "SAMPLE";
/// Warn early when a PNG might need a smaller WebP encoding.
const MAX_BYTES: u64 = 400_000;

enum Step {
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
    /// Stop the charts of a layout following each other's time range.
    UnsyncTime,
    /// Zoom the active chart to this many bars, ending at the latest.
    Visible(usize),
    Indicators,
    Paper,
    Position,
    Limit,
    Ticket,
    Dom,
    Alert,
    Heatmap,
    Replay(usize),
}

struct Scene {
    name: &'static str,
    steps: Vec<Step>,
    /// Pointer position as a fraction of the chart area, which shows the crosshair there.
    pointer: Option<Vec2>,
}

fn study<T: Indicator + Default + 'static>() -> Box<dyn Indicator> {
    Box::new(T::default())
}

/// Logical window size of every capture.
const SIZE: Vec2 = vec2(1600.0, 1000.0);

fn scenes() -> Vec<Scene> {
    use Step::*;
    let scene = |name, steps| Scene {
        name,
        steps,
        pointer: None,
    };
    let grid = |styles: [ModelChartType; 4], intervals: [Timeframe; 4]| {
        let mut steps = vec![ClearStudies, Grid(ChartLayoutMode::Four), UnsyncTime];
        for (slot, (style, interval)) in styles.into_iter().zip(intervals).enumerate() {
            steps.extend([
                Slot(slot),
                Interval(interval),
                ChartType(style),
                Visible(65),
            ]);
        }
        steps.push(Slot(0));
        steps
    };
    vec![
        Scene {
            name: "hero",
            steps: vec![
                ClearStudies,
                Study(|| Box::new(studies::EMA::new(9))),
                Study(|| {
                    Box::new(
                        studies::EMA::new(21).with_color(egui::Color32::from_rgb(255, 174, 67)),
                    )
                }),
                Study(study::<studies::VolumeWeightedAvgPrice>),
                Study(study::<studies::RSI>),
                Visible(100),
                Drawings(position_plan),
                Paper,
                Position,
                Limit,
                Alert,
                Panel(RightBarIcon::Watchlist),
            ],
            pointer: Some(vec2(0.45, 0.33)),
        },
        scene(
            "studies",
            vec![
                ClearStudies,
                Study(study::<studies::BollingerBands>),
                Study(study::<studies::EMA>),
                Study(study::<studies::VolumeWeightedAvgPrice>),
                Study(study::<studies::RSI>),
                Study(study::<studies::MACD>),
                Visible(120),
                Indicators,
            ],
        ),
        scene(
            "drawings",
            vec![ClearStudies, Visible(150), Drawings(analysis)],
        ),
        Scene {
            name: "layouts",
            steps: grid(
                [
                    ModelChartType::Candles,
                    ModelChartType::Heikin,
                    ModelChartType::Bars,
                    ModelChartType::Area,
                ],
                [
                    Timeframe::Min1,
                    Timeframe::Min5,
                    Timeframe::Min15,
                    Timeframe::Min1,
                ],
            ),
            pointer: Some(vec2(0.34, 0.25)),
        },
        scene(
            "chart-types",
            grid(
                [
                    ModelChartType::Heikin,
                    ModelChartType::Renko,
                    ModelChartType::Range,
                    ModelChartType::Kagi,
                ],
                [Timeframe::Min1; 4],
            ),
        ),
        scene(
            "trading",
            vec![
                ClearStudies,
                Study(study::<studies::EMA>),
                Visible(90),
                Paper,
                Position,
                Limit,
                Ticket,
            ],
        ),
        scene(
            "depth",
            vec![
                ClearStudies,
                Visible(70),
                Paper,
                Position,
                Limit,
                Ticket,
                Dom,
            ],
        ),
        scene(
            "replay",
            vec![
                ClearStudies,
                Study(study::<studies::EMA>),
                Replay(30),
                Visible(100),
                Paper,
                Position,
                Limit,
            ],
        ),
        scene("liquidity", vec![ClearStudies, Visible(45), Heatmap]),
        scene(
            "footprint",
            vec![
                ClearStudies,
                ChartType(ModelChartType::VolumeFootprint),
                Visible(14),
            ],
        ),
        scene(
            "profile",
            vec![
                ClearStudies,
                Interval(Timeframe::Min5),
                Study(|| {
                    let mut profile = studies::VolProfileSession::default();
                    profile.set_colors(vec![
                        egui::Color32::from_rgb(41, 98, 255),
                        egui::Color32::from_rgba_unmultiplied(38, 166, 154, 65),
                        egui::Color32::from_rgba_unmultiplied(239, 83, 80, 65),
                    ]);
                    Box::new(profile)
                }),
                Study(study::<studies::EMA>),
                Visible(72),
            ],
        ),
    ]
}

// ---------------------------------------------------------------- drawings, placed from the data

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

fn position_plan(bars: &[Bar]) -> Vec<Drawing> {
    let n = bars.len();
    let entry = bars[n - 67].close;
    vec![drawing(
        DrawingToolType::LongPos,
        BLUE,
        &[
            (n - 67, entry),
            (n - 45, entry + 1.5),
            (n - 45, entry - 0.75),
        ],
    )]
}

fn analysis(bars: &[Bar]) -> Vec<Drawing> {
    let n = bars.len();
    assert!(n >= 120);
    let (a, _) = extremes(bars, n - 112, n - 93);
    let (_, b) = extremes(bars, n - 92, n - 70);
    let (c, _) = extremes(bars, n - 69, n - 52);
    let (_, d) = extremes(bars, n - 51, n - 30);
    let (e, _) = extremes(bars, n - 29, n - 8);
    let mut fork = drawing(DrawingToolType::Pitchfork, BLUE, &[a, b, c]);
    fork.stroke_width = 1.0;
    let mut fib = drawing(
        DrawingToolType::FibonacciExtension,
        [255, 174, 67, 255],
        &[c, d, e],
    );
    fib.extend_right = true;
    let mut pattern = drawing(
        DrawingToolType::XABCDPattern,
        [151, 115, 230, 255],
        &[a, b, c, d, e],
    );
    pattern.fill_color = Some([151, 115, 230, 24]);
    let mut callout = drawing(
        DrawingToolType::Callout,
        [38, 166, 154, 255],
        &[e, (n - 18, e.1 + 1.2)],
    );
    callout.text = Some("Watch the retest".into());
    let mut zone = drawing(
        DrawingToolType::Rect,
        [38, 166, 154, 255],
        &[(n - 32, e.1 - 0.15), (n - 3, e.1 + 0.45)],
    );
    zone.fill_color = Some([38, 166, 154, 24]);
    vec![fork, fib, pattern, zone, callout]
}

// ---------------------------------------------------------------- capture

fn apply(app: &mut DemoApp, ctx: &Context, step: &Step) {
    match step {
        Step::Interval(tf) => app.handle_top_toolbar(ctx, TopToolbarAction::IntervalSelected(*tf)),
        Step::ChartType(t) => {
            let mut params = app.chart.chart_type_params().clone();
            params.renko_brick_size = 0.35;
            params.range_size = 0.65;
            params.kagi_reversal = 0.65;
            app.chart.set_chart_type_params(params);
            app.handle_top_toolbar(ctx, TopToolbarAction::ChartStyleSelected(*t));
            app.chart.config_mut().show_volume = !matches!(
                t,
                ModelChartType::Renko | ModelChartType::Kagi | ModelChartType::VolumeFootprint
            );
        }
        Step::ClearStudies => {
            app.handle_drawing_toolbar(DrawingToolbarAction::RemoveAllIndicators);
            app.chart.config_mut().show_volume = true;
        }
        Step::Study(make) => {
            app.chart.add_indicator(make());
        }
        Step::Drawings(place) => {
            let placed = place(app.chart.bars(SeriesId::MAIN));
            let dm = app.chart.drawings_mut();
            placed.into_iter().for_each(|d| dm.add_synced_drawing(d));
            dm.deselect();
        }
        Step::Grid(mode) => app.handle_top_toolbar(
            ctx,
            TopToolbarAction::Layout(LayoutAction::SelectGrid(*mode)),
        ),
        Step::Slot(slot) => app.activate_chart_slot(*slot),
        Step::Panel(icon) => app.toggle_panel(*icon),
        Step::UnsyncTime => {
            app.layout.sync.sync_time_axis = false;
            app.top_toolbar.chart_grid_selector.sync.sync_time_axis = false;
        }
        Step::Visible(count) => app.chart.set_visible_bars(*count),
        Step::Indicators => app.handle_top_toolbar(ctx, TopToolbarAction::OpenIndicators),
        Step::Paper => app.connect_paper(),
        Step::Replay(back) => app.start_replay_at(app.chart.bars(SeriesId::MAIN).len() - back),
        Step::Alert => {
            let price = app.chart.bars(SeriesId::MAIN).last().unwrap().close - 1.7;
            let mut alert = Alert::new(SAMPLE, AlertCondition::CrossingDown(price));
            alert.name = "Retest of the breakout level".into();
            app.alerts.save(alert);
        }
        Step::Position | Step::Limit | Step::Ticket | Step::Dom | Step::Heatmap => {}
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
        .build_eframe(|cc| {
            let now = "2026-10-27T19:29:50Z".parse().unwrap();
            DemoApp::new_at(cc, now, true)
        });
    h.run_steps(3);
    let ctx = h.ctx.clone();
    h.state_mut().toggle_panel(RightBarIcon::Notifications);
    h.run_steps(3);
    h.state_mut().toggle_panel(RightBarIcon::Notifications);
    let app = h.state_mut();
    app.theme = if light { Theme::light() } else { Theme::dark() };
    app.chart.set_theme(&ctx, &app.theme);
    // The demo's call to action belongs to the host application, not to the library.
    app.top_toolbar.call_to_action = None;
    app.paper
        .set_replay_time(Some("2026-10-27T19:29:50Z".parse().unwrap()));
    app.chart.config_mut().show_countdown = false;
    let mut options = app.chart.chart_options().clone();
    options.crosshair.vert_line_color = egui::Color32::from_gray(if light { 100 } else { 170 });
    options.crosshair.horz_line_color = options.crosshair.vert_line_color;
    app.chart.set_chart_options(options);
    app.trading.display.notifications = egui_charts::widget::settings::TradingNotifications::Off;
    app.timeframe_toolbar
        .set_control_visible(egui_charts::ui::Control::TimeZone, false);
    app.handle_top_toolbar(
        &ctx,
        TopToolbarAction::Layout(LayoutAction::Rename("Unnamed".into(), "Intraday".into())),
    );
    for step in &scene.steps {
        apply(h.state_mut(), &ctx, step);
        h.run_steps(4);
        match step {
            Step::Position | Step::Limit => {
                let price = h.state().chart.bars(SeriesId::MAIN).last().unwrap().close;
                let position = matches!(step, Step::Position);
                let mut order = OrderRequest::new(
                    SAMPLE,
                    OrderSide::Buy,
                    if position { 100.0 } else { 50.0 },
                    if position {
                        OrderType::Market
                    } else {
                        OrderType::Limit((price * 100.0).round() / 100.0 - 1.8)
                    },
                );
                if position {
                    order.take_profit = Some((price * 100.0).round() / 100.0 + 4.5);
                    order.stop_loss = Some((price * 100.0).round() / 100.0 - 2.5);
                }
                h.state_mut().trading.open_ticket(order);
                h.run_steps(5);
                h.get_by_label_contains(if position { "Buy 100" } else { "Buy 50" })
                    .click();
                h.run_steps(5);
                h.state_mut().trading.toggle_ticket(SAMPLE.into());
                if position && scene.name == "replay" {
                    for _ in 0..3 {
                        h.key_press(egui::Key::ArrowRight);
                        h.run_steps(3);
                    }
                }
                if position && scene.name != "replay" {
                    let now = "2026-10-27T19:31:50Z".parse().unwrap();
                    h.state_mut().paper.set_replay_time(Some(now));
                    h.state_mut().advance_sample_to(now);
                }
            }
            Step::Ticket => {
                let price = h.state().chart.bars(SeriesId::MAIN).last().unwrap().close;
                let mut order = OrderRequest::new(
                    SAMPLE,
                    OrderSide::Buy,
                    25.0,
                    OrderType::Limit((price * 100.0).round() / 100.0 - 0.6),
                );
                order.take_profit = Some((price * 100.0).round() / 100.0 + 1.2);
                order.stop_loss = Some((price * 100.0).round() / 100.0 - 3.7);
                h.state_mut().trading.open_ticket(order);
            }
            Step::Dom => {
                h.get_by_label("DOM").click();
                h.get_by_label("Orders 3").click();
            }
            Step::Heatmap => {
                h.state_mut().chart.set_liquidity_heatmap(true);
                let start: chrono::DateTime<chrono::Utc> = "2026-10-27T19:29:50Z".parse().unwrap();
                for minute in 1..=25 {
                    h.state_mut()
                        .advance_sample_to(start + chrono::Duration::minutes(minute));
                    h.run_steps(2);
                }
                h.state_mut().chart.set_volume_bubbles(Some(
                    egui_charts::chart::overlays::VolumeBubblesConfig {
                        min_size_filter: Some(80.0),
                        max_diameter: 20.0,
                        ..Default::default()
                    },
                ));
            }
            _ => {}
        }
        h.run_steps(5);
    }
    if !matches!(scene.name, "layouts" | "chart-types") {
        let count = scene.steps.iter().rev().find_map(|s| {
            if let Step::Visible(n) = s {
                Some(*n)
            } else {
                None
            }
        });
        if let Some(count) = count {
            h.state_mut().chart.set_visible_bars(count);
        }
    }
    if matches!(scene.name, "trading" | "depth") {
        h.state_mut()
            .chart
            .price_scale_mut()
            .options_mut()
            .scale_margins
            .top = 0.28;
    }
    if scene.name == "footprint" {
        h.state_mut().chart.scroll_to_position(0.5, false);
    }
    if scene.name == "hero" {
        h.run_steps(3);
        h.get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        h.get_by_role(egui::accesskit::Role::MultilineTextInput)
            .type_text(
                "Intraday plan\nWatch 770.00 for acceptance.\nKeep risk below the pullback low.",
            );
    }
    // Let feed replies and pane sizes settle before rendering.
    h.run_steps(10);
    if scene
        .steps
        .iter()
        .any(|step| matches!(step, Step::Position))
    {
        assert!(
            h.query_by_label("No open positions").is_none(),
            "{} lost its position",
            scene.name
        );
    }
    let area = chart_rect(&ctx);
    if let Some(f) = scene.pointer {
        h.hover_at(area.min + area.size() * f);
        h.run_steps(2);
    }
    let img = h
        .render()
        .unwrap_or_else(|e| panic!("render {}: {e}", scene.name));
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
            shoot(scene, light, 1.5, out);
        }
        eprintln!("captured {}", scene.name);
    }
}
