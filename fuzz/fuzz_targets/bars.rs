//! Bar ingestion and one frame: appends, live updates, prepends and replacements of hostile
//! bars, then a headless frame of every chart type.

#![no_main]

use arbitrary::Arbitrary;
use chrono::Duration;
use egui::{CentralPanel, Pos2, RawInput, Rect, Vec2};
use egui_charts::ChartType;
use egui_charts::model::BarData;
use egui_charts::widget::{Chart, SeriesId};
use libfuzzer_sys::fuzz_target;

#[path = "../common.rs"]
mod common;
use common::{FuzzBar, Prices, origin};

#[derive(Arbitrary, Debug)]
enum Op {
    /// A new bar one minute after the last.
    Append(Prices),
    /// An update of the last bar.
    Live(Prices),
    /// A bar at any time: new, repeated or older than the last.
    Any(FuzzBar),
    /// Bars in front of the stored ones.
    Prepend(Vec<FuzzBar>),
    /// All bars replaced.
    Replace(Vec<FuzzBar>),
}

#[derive(Arbitrary, Debug)]
struct Input {
    start: Vec<FuzzBar>,
    ops: Vec<Op>,
    chart_type: u8,
    visible_bars: u16,
}

fuzz_target!(|input: Input| {
    let bars = input.start.iter().map(FuzzBar::bar).collect();
    let mut chart = Chart::new(BarData::from_bars(bars));
    for op in &input.ops {
        let last = chart
            .bars(SeriesId::MAIN)
            .last()
            .map_or_else(origin, |b| b.time);
        match op {
            Op::Append(prices) => {
                let next = last.checked_add_signed(Duration::minutes(1));
                chart.update(SeriesId::MAIN, prices.at(next.unwrap_or(last)));
            }
            Op::Live(prices) => {
                chart.update(SeriesId::MAIN, prices.at(last));
            }
            Op::Any(bar) => {
                chart.update(SeriesId::MAIN, bar.bar());
            }
            Op::Prepend(older) => {
                let mut bars: Vec<_> = older.iter().map(FuzzBar::bar).collect();
                bars.extend_from_slice(chart.bars(SeriesId::MAIN));
                chart.set_data(SeriesId::MAIN, bars);
            }
            Op::Replace(bars) => {
                chart.set_data(SeriesId::MAIN, bars.iter().map(FuzzBar::bar).collect());
            }
        }
    }
    let types = ChartType::all();
    chart.set_chart_type(types[usize::from(input.chart_type) % types.len()]);
    chart.set_visible_bars(usize::from(input.visible_bars));

    let ctx = egui::Context::default();
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
        time: Some(0.0),
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| {
        CentralPanel::default().show(ui, |ui| {
            chart.show(ui);
        });
    });
    output.drop_without_applying_deltas();
});
