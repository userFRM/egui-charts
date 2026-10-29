//! Compact, deterministic workloads. Zero measured frames retain the same setup and warm-up
//! so Cachegrind can subtract their instructions from a measured run.

use egui::{pos2, vec2};
use egui_charts::studies::{EMA, Indicator, RSI, SMA};
use egui_charts::widget::SeriesId;
use std::hint::black_box;

pub fn run(name: &str, frames: usize) {
    if name == "session-resample" {
        use chrono::{Duration, NaiveTime, Weekday::*};
        use egui_charts::data::resample;
        use egui_charts::model::{Bar, BarClock, Session, SessionWindow, Timeframe, TimezoneId};

        let source = BarClock::new(Timeframe::Hour1).with_session(
            TimezoneId::named("America/New_York").unwrap(),
            Session::new([SessionWindow::new(
                [Mon, Tue, Wed, Thu, Fri],
                NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
                NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            )]),
            false,
        );
        let start = "2024-07-08T13:30:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap();
        let data: Vec<_> = (0..4)
            .flat_map(|week| (0..5).map(move |day| week * 7 + day))
            .flat_map(|day| {
                (0..7).map(move |hour| start + Duration::days(day) + Duration::hours(hour))
            })
            .map(|time| Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0))
            .collect();
        // Include each day's shortened 15:30 bar as well as its regular hourly bars.
        for _ in 0..1 + frames {
            let output = resample(black_box(&data), source.clone(), Timeframe::Day1).unwrap();
            assert_eq!(output.len(), 20);
            black_box(output);
        }
        return;
    }
    if name == "depth" {
        depth(frames);
        return;
    }
    if name == "bubbles" {
        // One 390 bar session on an exchange clock, whose bar opens are costly to look up.
        let count = 23_400;
        let mut chart = super::bubble_chart(count, true);
        let samples = super::frame::run(vec2(1440.0, 900.0), pos2(700.0, 400.0), frames, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
        });
        black_box(samples);
        assert_eq!(
            chart.trades(SeriesId::MAIN).count(),
            count as usize,
            "every trade reaches the chart"
        );
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

/// Small updates near the touch and a best-first batch that exceeds the book's bound.
fn depth(frames: usize) {
    use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Sink, Subscription};
    use egui_charts::model::{Instrument, OrderBook, OrderBookLevel, Timeframe};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Feed(Mutex<Option<Sink>>);
    impl Datafeed for Feed {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
        }
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            let mut page = HistoryPage::default();
            page.bars = (0..200).map(|i| super::example::sample_bar(i, 0)).collect();
            page.end_of_history = true;
            reply.send(Ok(page));
        }
        fn subscribe(&self, subscription: Subscription) {
            *self.0.lock().unwrap() = Some(subscription.sink);
        }
    }
    let feed = Arc::new(Feed::default());
    let mut chart = egui_charts::ChartBuilder::new()
        .with_symbol("DEPTH")
        .with_timeframe(Timeframe::Min1)
        .with_feed(feed.clone())
        .build();
    let level = |price| OrderBookLevel {
        price,
        size: 1.0,
        order_cnt: None,
    };
    let snapshot = OrderBook {
        ts: chrono::DateTime::UNIX_EPOCH,
        symbol: "DEPTH".into(),
        bids: (0..1000).map(|i| level(1000.0 - i as f64)).collect(),
        asks: (0..1000).map(|i| level(1001.0 + i as f64)).collect(),
    };
    let mut small = OrderBook {
        bids: snapshot.bids[..8].to_vec(),
        asks: snapshot.asks[..8].to_vec(),
        ..snapshot.clone()
    };
    let oversized = OrderBook {
        bids: (1..=20_000).map(|i| level(-(i as f64))).collect(),
        asks: Vec::new(),
        ..snapshot.clone()
    };
    let mut sequence = 0;
    let samples = super::frame::run(vec2(1440.0, 900.0), pos2(700.0, 350.0), frames, |ui| {
        if let Some(sink) = feed.0.lock().unwrap().as_ref() {
            sequence += 1;
            sink.push_depth_snapshot(sequence, snapshot.clone());
            for i in 0..100 {
                for level in small.bids.iter_mut().chain(&mut small.asks) {
                    level.size = 1.0 + (i % 2) as f64;
                }
                sequence += 1;
                sink.push_depth_delta(sequence, &small);
            }
            sequence += 1;
            sink.push_depth_delta(sequence, &oversized);
        }
        egui::CentralPanel::default().show(ui, |ui| {
            chart.show(ui);
        });
    });
    if !samples.is_empty() {
        let mean = samples
            .iter()
            .map(|sample| sample.frame.as_secs_f64())
            .sum::<f64>()
            * 1e6
            / samples.len() as f64;
        println!("depth: {mean:.2} us/frame");
    }
    black_box(samples);
}
