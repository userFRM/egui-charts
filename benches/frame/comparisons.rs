//! Pointer-only frames with independently fed comparison histories on a percentage scale.

use chrono::{DateTime, TimeDelta};
use egui::{Color32, Event, Pos2, RawInput, Rect, vec2};
use egui_charts::model::{Bar, BarData};
use egui_charts::scales::PriceScaleMode;
use egui_charts::widget::{Chart, SeriesId};

pub fn run(filters: &[String], frames: usize) {
    if !filters.is_empty()
        && !filters
            .iter()
            .any(|filter| "comparisons".contains(filter.as_str()))
    {
        return;
    }
    println!(
        "| comparisons, 200 bars in view | history bars per series | frame us median/p95 | tessellation us median | vertices |"
    );
    println!("|---:|---:|---:|---:|---:|");
    for bars in [100_000, 1_000_000] {
        for count in [0, 1, 2, 5] {
            let samples = samples(bars, count, frames);
            let percentile = |time: fn(&super::frame::Sample) -> std::time::Duration, p: usize| {
                let mut times: Vec<_> = samples.iter().map(time).collect();
                times.sort();
                times
                    .get(times.len().saturating_sub(1) * p / 100)
                    .map_or(0.0, |time| time.as_secs_f64() * 1e6)
            };
            println!(
                "| {count} | {bars} | {:.1}/{:.1} | {:.1} | {} |",
                percentile(|sample| sample.frame, 50),
                percentile(|sample| sample.frame, 95),
                percentile(|sample| sample.tessellate, 50),
                samples.last().map_or(0, |sample| sample.vertices),
            );
        }
    }
}

/// The same setup and warm-up for the timing table and the instruction regression check.
pub fn samples(bars: usize, count: usize, frames: usize) -> Vec<super::frame::Sample> {
    let history = |series: usize| {
        (0..bars)
            .map(|i| {
                let time = DateTime::UNIX_EPOCH
                    + TimeDelta::minutes(i as i64)
                    + TimeDelta::seconds((series % 2) as i64 * 30);
                let close = (100.0 + (i as f64 * 0.01).sin()) * (series + 1) as f64;
                Bar::new(time, close, close + 0.1, close - 0.1, close, 10.0)
            })
            .collect()
    };
    let mut chart = Chart::new(BarData::from_bars(history(0)));
    chart
        .series_price_scale_mut(SeriesId::MAIN)
        .unwrap()
        .options_mut()
        .mode = PriceScaleMode::Percentage;
    for series in 1..=count {
        let id = chart.add_comparison(format!("Comparison {series}"), Color32::BLUE);
        chart.set_data(id, history(series));
    }
    chart.set_visible_bars(200);
    let ctx = egui::Context::default();
    // Keep setup-only and short measured runs on the same heap layout.
    let mut samples = Vec::with_capacity(frames.max(60));
    for i in 0..60 + frames {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            time: Some(i as f64 / 60.0),
            events: vec![Event::PointerMoved(egui::pos2(
                300.0 + (i % 700) as f32,
                400.0,
            ))],
            ..Default::default()
        };
        let sample = super::frame::measure(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
        });
        if i >= 60 {
            samples.push(sample);
        }
    }
    samples
}
