//! Frame and tessellation time of large and multiple charts: headless egui frames on a
//! 1920 by 1080 window with the pointer over the charts, on the sample bars of
//! `examples/bare_chart.rs`. Prints a Markdown table of the medians per scenario.
//! `FRAME_TIME_FRAMES` sets the measured frames (default 300) after 60 warm-up frames;
//! arguments run only the scenarios whose name contains one of them.
//! A second table prints first, steady, tick and new-bar frame times for candles and the five
//! price-based chart types on 1,000,000 one-minute bars; `-- price-based` runs it alone.
//! A third prints the frame of volume bubbles from 100,000 trades; `-- bubbles` runs it alone.
//! A fourth prints the frame of a zoomed-out daily TPO chart at 400 and at 1,600 sessions;
//! `-- tpo` runs it alone.
//! A fifth draws a host-supplied footprint with executions on all 100,000 visible bars;
//! `-- footprint-host` runs it alone.
//! `--regression <scenario>` runs a compact workload for `benches/check-regressions.sh`.

use egui::{Rect, UiBuilder, pos2, vec2};
use egui_charts::ChartType;
use egui_charts::drawings::{ChartPoint, Drawing, DrawingInteractionMode, DrawingToolType};
use egui_charts::model::{BarData, EventKind, Marker, TimeScaleEvent};
use egui_charts::studies::{
    ADX, ATR, BollingerBands, EMA, IchimokuCloud, MACD, ParabolicSAR, RSI, SMA, Stochastic,
};
use egui_charts::widget::{Chart, SeriesId};

#[path = "../examples/bare_chart.rs"]
#[expect(dead_code, reason = "the bench draws only the example's sample bars")]
mod example;
mod frame;
mod regression;

/// Fix Linux hash seeds for repeatable instruction counts, including dependencies' maps.
/// This interposed symbol belongs only to the benchmark executable.
#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
unsafe extern "C" fn getrandom(
    buffer: *mut std::ffi::c_void,
    length: usize,
    _flags: std::ffi::c_uint,
) -> isize {
    if length != 0 {
        // SAFETY: the caller supplies a buffer writable for `length` bytes.
        unsafe { std::ptr::write_bytes(buffer.cast::<u8>(), 0x42, length) };
    }
    length as isize
}

/// A scenario: its name, how many bars each chart has and shows, and how many charts.
struct Scenario {
    name: &'static str,
    bars: usize,
    visible: usize,
    charts: usize,
    columns: usize,
    studies: bool,
    drawings: Drawings,
    /// Smallest bar spacing in pixels, below the default 0.5 to zoom out further.
    min_bar_spacing: f32,
    /// The chart type drawn.
    chart_type: ChartType,
    /// What the chart marks on every bar.
    annotations: Annotations,
}

/// Where the 500 drawings of a scenario are anchored, every drawing tool in turn.
#[derive(PartialEq)]
enum Drawings {
    None,
    InView,
    AcrossHistory,
}

/// What a scenario marks on every bar of its history.
#[derive(PartialEq)]
enum Annotations {
    None,
    /// A one-letter marker on every bar.
    Markers,
    /// A news event on every bar.
    Events,
}

const SCENARIOS: [Scenario; 11] = [
    Scenario {
        name: "1 chart, 5,000,000 bars, zoomed out",
        bars: 5_000_000,
        visible: 5_000_000,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "1 chart, 1,000,000 bars, all in view",
        bars: 1_000_000,
        visible: 1_000_000,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.0,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "1 chart, 1,000,000 bars, all in view, volume footprint",
        bars: 1_000_000,
        visible: 1_000_000,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.0,
        chart_type: ChartType::VolumeFootprint,
        annotations: Annotations::None,
    },
    Scenario {
        name: "1 chart, 200,000 bars",
        bars: 200_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "1 chart, 200,000 bars, a marker on every bar",
        bars: 200_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::Markers,
    },
    Scenario {
        name: "1 chart, 200,000 bars, a news event on every bar",
        bars: 200_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::Events,
    },
    Scenario {
        name: "1 chart, 5,000,000 bars, zoomed in",
        bars: 5_000_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "4 charts, 100,000 bars",
        bars: 100_000,
        visible: 200,
        charts: 4,
        columns: 2,
        studies: false,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "8 charts, 100,000 bars, 10 studies each",
        bars: 100_000,
        visible: 200,
        charts: 8,
        columns: 4,
        studies: true,
        drawings: Drawings::None,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "1 chart, 10,000 bars, 500 drawings in view",
        bars: 10_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::InView,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
    Scenario {
        name: "1 chart, 10,000 bars, 500 drawings across the history",
        bars: 10_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: Drawings::AcrossHistory,
        min_bar_spacing: 0.5,
        chart_type: ChartType::Candles,
        annotations: Annotations::None,
    },
];

fn main() {
    let frames: usize =
        std::env::var("FRAME_TIME_FRAMES").map_or(300, |v| v.parse().expect("a frame count"));
    let args: Vec<_> = std::env::args().collect();
    if let Some(at) = args.iter().position(|arg| arg == "--regression") {
        let name = args.get(at + 1).expect("a regression scenario");
        if matches!(name.as_str(), "tpo-400" | "tpo-1600") {
            tpo(std::slice::from_ref(name), frames);
        } else {
            regression::run(name, frames);
        }
        return;
    }
    // `cargo bench` passes `--bench`; anything else selects scenarios.
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    println!("| scenario | shapes | vertices | frame ms (median) | tessellation ms (median) |");
    println!("|---|---:|---:|---:|---:|");
    for scenario in &SCENARIOS {
        if !filters.is_empty() && !filters.iter().any(|f| scenario.name.contains(f.as_str())) {
            continue;
        }
        let mut charts: Vec<Chart> = (0..scenario.charts).map(|_| chart(scenario)).collect();
        let rows = scenario.charts.div_ceil(scenario.columns);
        let samples = frame::run(vec2(1920.0, 1080.0), pos2(900.0, 350.0), frames, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let full = ui.max_rect();
                let size = vec2(
                    full.width() / scenario.columns as f32,
                    full.height() / rows as f32,
                );
                for (i, chart) in charts.iter_mut().enumerate() {
                    let cell = vec2(
                        (i % scenario.columns) as f32 * size.x,
                        (i / scenario.columns) as f32 * size.y,
                    );
                    let rect = Rect::from_min_size(full.min + cell, size);
                    ui.scope_builder(UiBuilder::new().id_salt(i).max_rect(rect), |ui| {
                        chart.show(ui);
                    });
                }
            });
        });
        let median = |time: fn(&frame::Sample) -> std::time::Duration| {
            let mut times: Vec<_> = samples.iter().map(time).collect();
            times.sort();
            times
                .get(times.len() / 2)
                .map_or(0.0, |d| d.as_secs_f64() * 1e3)
        };
        println!(
            "| {} | {} | {} | {:.3} | {:.3} |",
            scenario.name,
            samples.last().map_or(0, |s| s.shapes),
            samples.last().map_or(0, |s| s.vertices),
            median(|s| s.frame),
            median(|s| s.tessellate),
        );
    }
    println!();
    price_based(&filters, frames);
    println!();
    bubbles(&filters, frames);
    println!();
    tpo(&filters, frames);
    println!();
    footprint(&filters, frames);
}

/// A host footprint, without the feed's history budget, with executions on every visible bar.
fn footprint(filters: &[String], frames: usize) {
    use egui_charts::chart::overlays::FootprintConfig;
    use egui_charts::data::Footprint;
    use egui_charts::model::{BarClock, OrderSide, Tick, Timeframe};

    let name = "footprint-host, 100,000 bars, all in view";
    if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
        return;
    }
    let bars: Vec<_> = (0..100_000).map(|i| example::sample_bar(i, 0)).collect();
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
    for bar in &bars {
        for (price, size, side) in [
            (bar.low, 2.0, OrderSide::Sell),
            (bar.close, 3.0, OrderSide::Buy),
            (bar.high, 1.0, OrderSide::Buy),
        ] {
            flow.push(&Tick::new(bar.time, price, size, Some(side)))
                .unwrap();
        }
    }
    assert_eq!(flow.bars().len(), bars.len());
    let mut options = egui_charts::config::ChartOptions::default();
    options.time_scale.min_bar_spacing = 0.0;
    let mut chart = Chart::new(BarData::from_bars(bars)).with_chart_options(options);
    chart.set_visible_bars(100_000);
    let config = FootprintConfig::default();
    let samples = frame::run(vec2(1440.0, 900.0), pos2(700.0, 400.0), frames, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            chart.show_footprint(ui, &flow, &config);
        });
    });
    let median = |time: fn(&frame::Sample) -> std::time::Duration| {
        let mut times: Vec<_> = samples.iter().map(time).collect();
        times.sort();
        times
            .get(times.len() / 2)
            .map_or(0.0, |d| d.as_secs_f64() * 1e3)
    };
    println!(
        "| host footprint | shapes | vertices | frame ms (median) | tessellation ms (median) |"
    );
    println!("|---|---:|---:|---:|---:|");
    println!(
        "| {name} | {} | {} | {:.3} | {:.3} |",
        samples.last().map_or(0, |s| s.shapes),
        samples.last().map_or(0, |s| s.vertices),
        median(|s| s.frame),
        median(|s| s.tessellate),
    );
}

/// Daily TPO sessions, all in view, including the workload used by the regression check.
fn tpo(filters: &[String], frames: usize) {
    println!("| daily TPO | shapes | frame ms (median) | us per session |");
    println!("|---|---:|---:|---:|");
    for (name, sessions) in [("tpo-400", 400), ("tpo-1600", 1_600)] {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f)) {
            continue;
        }
        let bars = (0..sessions)
            .map(|i| {
                let mut bar = example::sample_bar(i, 0);
                bar.time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64);
                bar
            })
            .collect();
        let mut options = egui_charts::config::ChartOptions::default();
        options.time_scale.min_bar_spacing = 0.0001;
        let mut chart = Chart::new(BarData::from_bars(bars)).with_chart_options(options);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        chart.set_visible_bars(sessions);
        let mut fit = true;
        let samples = frame::run(vec2(800.0, 600.0), pos2(400.0, 300.0), frames, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
            if fit {
                chart.fit_content();
                fit = false;
            }
        });
        let mut times: Vec<_> = samples.iter().map(|s| s.frame).collect();
        times.sort();
        let ms = times
            .get(times.len() / 2)
            .map_or(0.0, |d| d.as_secs_f64() * 1e3);
        println!(
            "| {name} | {} | {ms:.3} | {:.3} |",
            samples.last().map_or(0, |s| s.shapes),
            ms * 1e3 / sessions as f64,
        );
    }
}

fn chart(scenario: &Scenario) -> Chart {
    let bars = (0..scenario.bars)
        .map(|i| example::sample_bar(i, 0))
        .collect();
    let mut options = egui_charts::config::ChartOptions::default();
    options.time_scale.min_bar_spacing = scenario.min_bar_spacing;
    let mut chart = Chart::new(BarData::from_bars(bars)).with_chart_options(options);
    chart.set_chart_type(scenario.chart_type);
    chart.set_visible_bars(scenario.visible);
    chart.set_symbol("SAMPLE");
    if scenario.studies {
        // Five price overlays and five studies in panes below the price pane.
        chart.add_indicator(Box::new(SMA::default()));
        chart.add_indicator(Box::new(EMA::default()));
        chart.add_indicator(Box::new(BollingerBands::default()));
        chart.add_indicator(Box::new(IchimokuCloud::default()));
        chart.add_indicator(Box::new(ParabolicSAR::default()));
        chart.add_indicator(Box::new(RSI::default()));
        chart.add_indicator(Box::new(MACD::default()));
        chart.add_indicator(Box::new(Stochastic::default()));
        chart.add_indicator(Box::new(ATR::default()));
        chart.add_indicator(Box::new(ADX::default()));
    }
    let tools: Vec<DrawingToolType> = DrawingToolType::all()
        .iter()
        .copied()
        .filter(|tool| tool.interaction_mode() != DrawingInteractionMode::NotDrawing)
        .collect();
    for k in 0..if scenario.drawings == Drawings::None {
        0
    } else {
        500
    } {
        let tool = tools[k % tools.len()];
        // Anchors at most 35 bars apart, from a bar in view or anywhere in the history.
        let first = match scenario.drawings {
            Drawings::InView => scenario.bars - scenario.visible + k * 7 % (scenario.visible - 40),
            _ => k * 19 % (scenario.bars - 40),
        };
        let mut drawing = Drawing::new(k, tool);
        drawing.text = Some("Note".into());
        // Curves store their controls as well as the two clicks that place them.
        let anchors = match tool {
            DrawingToolType::Curve => 3,
            DrawingToolType::DoubleCurve => 4,
            _ => tool.required_points().unwrap_or(8),
        };
        drawing.chart_points = (0..anchors)
            .map(|j| {
                let price = 100.0 + 6.0 * ((k + j) as f64).sin();
                ChartPoint::new((first + 5 * j) as f32, price)
            })
            .collect();
        chart
            .drawings_mut()
            .add_synced_drawing(drawing)
            .expect("bench drawings are valid");
    }
    if scenario.annotations != Annotations::None {
        let times: Vec<_> = chart
            .bars(SeriesId::MAIN)
            .iter()
            .map(|bar| bar.time)
            .collect();
        match scenario.annotations {
            Annotations::Markers => {
                chart.set_markers(
                    SeriesId::MAIN,
                    times
                        .into_iter()
                        .map(|time| Marker::new(time).with_text("B"))
                        .collect(),
                );
            }
            Annotations::Events => {
                chart.set_time_scale_events(
                    times
                        .into_iter()
                        .enumerate()
                        .map(|(index, time)| TimeScaleEvent {
                            id: index.to_string(),
                            time,
                            kind: EventKind::News,
                            title: "Headline".into(),
                            details: String::new(),
                        })
                        .collect(),
                );
            }
            Annotations::None => {}
        }
    }
    chart
}

/// Prints the first, steady, tick and new-bar frame times of candles and the five price-based
/// chart types on 1,000,000 one-minute bars.
fn price_based(filters: &[String], frames: usize) {
    use chrono::{Duration, TimeZone, Utc};
    use egui_charts::model::{Bar, ChartTypeParams, RenkoConfig};
    let mut seed = 0x1234_5678_u64;
    let mut step = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed as f64 / (u64::MAX as f64 + 1.0)) * 0.2 - 0.1
    };
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let mut close: f64 = 100.0;
    let bars: Vec<Bar> = (0..1_000_000)
        .map(|i| {
            let open = close;
            close += step();
            Bar {
                time: start + Duration::minutes(i),
                open,
                close,
                high: open.max(close) + 0.05,
                low: open.min(close) - 0.05,
                volume: 100.0,
                colors: None,
                payload: None,
            }
        })
        .collect();
    let size = RenkoConfig::from_atr(&bars, 14, 0.5).brick_size;
    let params = ChartTypeParams {
        renko_brick_size: size,
        kagi_reversal: size,
        range_size: size,
        pnf_box_size: size,
        pnf_reversal: 3,
        line_break_count: 3,
        ..Default::default()
    };
    println!(
        "| price-based, 1,000,000 bars | first ms | steady us median/p95 | tick us median/p95 | new bar us median/p95 |"
    );
    println!("|---|---:|---:|---:|---:|");
    for (name, kind) in [
        ("candles", ChartType::Candles),
        ("renko", ChartType::Renko),
        ("kagi", ChartType::Kagi),
        ("line break", ChartType::LineBreak),
        ("point and figure", ChartType::PointAndFigure),
        ("range", ChartType::Range),
    ] {
        if !filters.is_empty()
            && !filters
                .iter()
                .any(|f| format!("price-based {name}").contains(f))
        {
            continue;
        }
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_chart_type(kind);
        chart.set_chart_type_params(params.clone());
        let ctx = egui::Context::default();
        let mut frame_idx = 0;
        let mut draw = |chart: &mut Chart| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 720.0))),
                time: Some(frame_idx as f64 / 60.0),
                events: vec![egui::Event::PointerMoved(pos2(640.0, 360.0))],
                ..Default::default()
            };
            frame_idx += 1;
            frame::measure(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    chart.show(ui);
                });
            })
            .frame
        };
        let first = draw(&mut chart);
        for _ in 0..60 {
            draw(&mut chart);
        }
        let mut columns = Vec::new();
        let mut last = bars.last().unwrap().clone();
        for mode in 0..3 {
            let mut times = Vec::with_capacity(frames);
            for _ in 0..frames {
                let start = std::time::Instant::now();
                if mode > 0 {
                    if mode == 2 {
                        last.time += Duration::minutes(1);
                        last.open = last.close;
                        last.high = last.close + 0.05;
                        last.low = last.close - 0.05;
                    }
                    last.close += step();
                    last.high = last.high.max(last.close + 0.05);
                    last.low = last.low.min(last.close - 0.05);
                    chart.update(egui_charts::widget::SeriesId::MAIN, last.clone());
                }
                let update = start.elapsed();
                times.push(update + draw(&mut chart));
            }
            times.sort();
            let us = |p: f64| {
                times
                    .get(((times.len().max(1) - 1) as f64 * p) as usize)
                    .map_or(0.0, |t| t.as_secs_f64() * 1e6)
            };
            columns.push(format!("{:.1}/{:.1}", us(0.5), us(0.95)));
        }
        println!(
            "| {name} | {:.3} | {} | {} | {} |",
            first.as_secs_f64() * 1e3,
            columns[0],
            columns[1],
            columns[2]
        );
    }
}

/// The chart of the volume bubbles workload: `count` trades at 60 a one-minute bar, over 40 price
/// levels, 200 bars in view. With `session` the bars are the New York regular session of one day
/// and the chart's clock follows it; without, they start at midnight UTC.
fn bubble_chart(count: i64, session: bool) -> egui_charts::widget::Chart {
    use chrono::{Duration, TimeZone, Utc};
    use egui_charts::ChartBuilder;
    use egui_charts::chart::overlays::{BubbleGrouping, VolumeBubblesConfig};
    use egui_charts::data::{
        Datafeed, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest,
    };
    use egui_charts::model::{
        Bar, Instrument, OrderSide, Session, SessionWindow, Tick, Timeframe, TimezoneId,
    };
    use std::sync::Arc;

    struct Feed(Vec<Bar>, Vec<Tick>);
    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::new(self.0.clone(), true)));
        }
        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            let mut page = TradePage::default();
            page.trades = self.1.clone();
            page.end_of_history = true;
            reply.send(Ok(page));
        }
    }

    let (levels, step) = (40, 0.05);
    let start = if session {
        Utc.with_ymd_and_hms(2024, 1, 2, 14, 30, 0).unwrap()
    } else {
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
    };
    let mut seed = 0x1234_5678_u64;
    let mut unit = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as f64 / (u64::MAX as f64 + 1.0)
    };
    let trades: Vec<Tick> = (0..count)
        .map(|i| {
            let side = if unit() < 0.5 {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };
            let time = start + Duration::minutes(i / 60) + Duration::seconds(i % 60);
            let price = 100.0 + (i % levels) as f64 * step;
            Tick::new(time, price, 1.0 + unit() * 99.0, Some(side))
        })
        .collect();
    let top = 100.0 + (levels - 1) as f64 * step;
    let bars: Vec<Bar> = (0..(count + 59) / 60)
        .map(|i| Bar::new(start + Duration::minutes(i), 100.0, top, 100.0, top, 100.0))
        .collect();
    let len = bars.len();
    let mut chart = ChartBuilder::new()
        .with_symbol("BUBBLES")
        .with_timeframe(Timeframe::Min1)
        .with_feed(Arc::new(Feed(bars, trades)))
        .with_initial_bars(len)
        .build();
    if session {
        use chrono::Weekday::*;
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let clock = |hour, minute| chrono::NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        chart.set_instrument(Some(
            Instrument::new("BUBBLES")
                .with_time_zone(TimezoneId::named("America/New_York").unwrap())
                .with_session(Session::new([SessionWindow::new(
                    weekdays,
                    clock(9, 30),
                    clock(16, 0),
                )])),
        ));
    }
    chart.set_chart_type(ChartType::Candles);
    chart.set_visible_bars(200);
    chart.set_volume_bubbles(Some(VolumeBubblesConfig {
        grouping: BubbleGrouping::PriceStep(step),
        ..VolumeBubblesConfig::default()
    }));
    chart
}

/// Prints the frame of volume bubbles from 100,000 trades over 1,667 one-minute bars, 40 price
/// levels a bar, 200 bars in view on a 1440 by 900 window.
fn bubbles(filters: &[String], frames: usize) {
    let name = "volume bubbles, 100,000 trades, 200 bars in view";
    if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
        return;
    }
    let count = 100_000_i64;
    let mut chart = bubble_chart(count, false);
    let samples = frame::run(vec2(1440.0, 900.0), pos2(700.0, 400.0), frames, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            chart.show(ui);
        });
    });
    let held = chart.trades(egui_charts::widget::SeriesId::MAIN).count();
    assert_eq!(held, count as usize, "every trade reaches the chart");
    let median = |time: fn(&frame::Sample) -> std::time::Duration| {
        let mut times: Vec<_> = samples.iter().map(time).collect();
        times.sort();
        times[times.len() / 2].as_secs_f64() * 1e3
    };
    println!("| volume bubbles | vertices | frame ms (median) | tessellation ms (median) |");
    println!("|---|---:|---:|---:|");
    println!(
        "| {name} | {} | {:.3} | {:.3} |",
        samples.last().map_or(0, |s| s.vertices),
        median(|s| s.frame),
        median(|s| s.tessellate),
    );
}
