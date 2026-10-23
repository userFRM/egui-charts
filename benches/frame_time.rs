//! Frame and tessellation time of large and multiple charts: headless egui frames on a
//! 1920 by 1080 window with the pointer over the charts, on the sample bars of
//! `examples/bare_chart.rs`. Prints a Markdown table of the medians per scenario.
//! `FRAME_TIME_FRAMES` sets the measured frames (default 300) after 60 warm-up frames;
//! arguments run only the scenarios whose name contains one of them.
//! A second table prints first, steady, tick and new-bar frame times for candles and the five
//! price-based chart types on 1,000,000 one-minute bars; `-- price-based` runs it alone.

use egui::{Rect, UiBuilder, pos2, vec2};
use egui_charts::ChartType;
use egui_charts::model::BarData;
use egui_charts::studies::{
    ADX, ATR, BollingerBands, EMA, IchimokuCloud, MACD, ParabolicSAR, RSI, SMA, Stochastic,
};
use egui_charts::widget::Chart;

#[path = "../examples/bare_chart.rs"]
#[expect(dead_code, reason = "the bench draws only the example's sample bars")]
mod example;
mod frame;

/// A scenario: its name, how many bars each chart has and shows, and how many charts.
struct Scenario {
    name: &'static str,
    bars: usize,
    visible: usize,
    charts: usize,
    columns: usize,
    studies: bool,
}

const SCENARIOS: [Scenario; 4] = [
    Scenario {
        name: "1 chart, 5,000,000 bars, zoomed out",
        bars: 5_000_000,
        visible: 5_000_000,
        charts: 1,
        columns: 1,
        studies: false,
    },
    Scenario {
        name: "1 chart, 5,000,000 bars, zoomed in",
        bars: 5_000_000,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
    },
    Scenario {
        name: "4 charts, 100,000 bars",
        bars: 100_000,
        visible: 200,
        charts: 4,
        columns: 2,
        studies: false,
    },
    Scenario {
        name: "8 charts, 100,000 bars, 10 studies each",
        bars: 100_000,
        visible: 200,
        charts: 8,
        columns: 4,
        studies: true,
    },
];

fn main() {
    let frames: usize =
        std::env::var("FRAME_TIME_FRAMES").map_or(300, |v| v.parse().expect("a frame count"));
    // `cargo bench` passes `--bench`; anything else selects scenarios.
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    println!("| scenario | shapes | frame ms (median) | tessellation ms (median) |");
    println!("|---|---:|---:|---:|");
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
            "| {} | {} | {:.3} | {:.3} |",
            scenario.name,
            samples.last().map_or(0, |s| s.shapes),
            median(|s| s.frame),
            median(|s| s.tessellate),
        );
    }
    println!();
    price_based(&filters, frames);
}

fn chart(scenario: &Scenario) -> Chart {
    let bars = (0..scenario.bars)
        .map(|i| example::sample_bar(i, 0))
        .collect();
    let mut chart = Chart::new(BarData::from_bars(bars));
    chart.set_chart_type(ChartType::Candles);
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
