//! Compact, deterministic workloads. Zero measured frames retain the same setup and warm-up
//! so Cachegrind can subtract their instructions from a measured run.

use egui::{pos2, vec2};
use egui_charts::studies::{EMA, Indicator, RSI, SMA};
use egui_charts::widget::SeriesId;
use std::hint::black_box;

pub fn run(name: &str, frames: usize) {
    if name == "trade-replay" {
        let fills: Vec<_> = (0..100_000)
            .map(|id| {
                egui_charts::model::Tick::new(
                    chrono::DateTime::from_timestamp(id, 0).unwrap(),
                    100.0,
                    1.0,
                    None,
                )
                .with_id(id as u64)
            })
            .collect();
        let mut replay = fills.clone();
        replay.extend(fills.iter().cloned().map(|fill| {
            let id = fill.id.unwrap();
            fill.with_aggressor_order_id(id + 1)
        }));
        let aggregate = |fills: &[_]| {
            egui_charts::data::aggregate_trades(
                black_box(fills),
                egui_charts::model::Timeframe::Min1,
            )
            .unwrap()
        };
        let expected = aggregate(&fills);
        for _ in 0..frames {
            assert_eq!(aggregate(&replay), expected);
        }
        return;
    }
    let bars = match name {
        "frame-1000" => 1_000,
        "frame-1000000" => 1_000_000,
        "frame-100000" | "heikin" | "tick" | "tick-studies" | "append" | "append-studies"
        | "studies" | "drawings" => 100_000,
        _ => panic!("unknown regression scenario: {name}"),
    };
    if name == "studies" {
        let data: Vec<_> = (0..bars)
            .map(|i| super::example::sample_bar(i, 0))
            .collect();
        let mut studies: [Box<dyn Indicator>; 3] = [
            Box::new(SMA::new(20)),
            Box::new(EMA::new(20)),
            Box::new(RSI::new(14)),
        ];
        // Keep allocation and initial calculation in both runs.
        for _ in 0..1 + frames {
            for study in &mut studies {
                study.calculate(black_box(&data));
                black_box(study.values());
            }
        }
        return;
    }
    let mut chart = super::chart(&super::Scenario {
        name: "regression",
        bars,
        visible: 200,
        charts: 1,
        columns: 1,
        studies: false,
        drawings: if name == "drawings" {
            super::Drawings::InView
        } else {
            super::Drawings::None
        },
        min_bar_spacing: 0.5,
    });
    let mut config = chart.config().clone();
    config.show_volume = false;
    chart.update_config(config);
    if name.ends_with("-studies") {
        chart.add_indicator(Box::new(SMA::new(20)));
        chart.add_indicator(Box::new(EMA::new(20)));
        chart.add_indicator(Box::new(RSI::new(14)));
    }
    if name == "heikin" {
        chart.set_chart_type(egui_charts::ChartType::Heikin);
    }
    let mut last = super::example::sample_bar(bars - 1, 0);
    let mut tick = 0;
    let samples = super::frame::run(vec2(1440.0, 900.0), pos2(700.0, 350.0), frames, |ui| {
        if name.starts_with("tick") || name.starts_with("append") {
            if name.starts_with("append") {
                last.time += chrono::Duration::minutes(1);
            }
            last.close += if tick % 2 == 0 { 0.01 } else { -0.01 };
            last.high = last.high.max(last.close);
            last.low = last.low.min(last.close);
            chart.update(SeriesId::MAIN, last.clone());
            tick += 1;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            chart.show(ui);
        });
    });
    black_box(samples);
    assert_eq!(
        chart.indicators().count(),
        if name.ends_with("-studies") { 3 } else { 0 },
        "the workload must contain only the requested studies"
    );
}
