//! Matched raw and prepared history, including admission, steady frames and live updates.

#[expect(
    dead_code,
    reason = "this bench reports total frame time, not tessellation separately"
)]
mod frame;
use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::{
    Datafeed, FeedError, Footprint, FootprintBar, FootprintBarView, FootprintLevel, FootprintPage,
    FootprintRequest, FootprintRows, FootprintSource, HistoryPage, HistoryRequest, Reply, Sink,
    Subscription, TradePage, TradeRequest,
};
use egui_charts::model::{Bar, BarClock, ChartType, Instrument, OrderSide, Tick, Timeframe};
use egui_charts::widget::{Chart, ChartBuilder, SeriesId};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const BARS: usize = 3_000;
const ROWS: usize = 16;

struct Feed {
    bars: Vec<Bar>,
    trades: Mutex<Option<Vec<Tick>>>,
    rows: Mutex<Option<FootprintPage>>,
    sink: Mutex<Option<Sink>>,
}

impl Datafeed for Feed {
    fn resolve(&self, _: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new("SAMPLE").with_tick_size(0.25)));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        assert!(request.before.is_none());
        reply.send(Ok(HistoryPage::new(self.bars.clone(), true)));
    }

    fn subscribe(&self, subscription: Subscription) {
        *self.sink.lock().unwrap() = Some(subscription.sink);
    }

    fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
        // Prepared input retains only the boundary trade; a second request exhausts it.
        let trades = self.trades.lock().unwrap().take().unwrap_or_default();
        let from_start = trades.is_empty() || trades.len() > 1;
        reply.send(Ok(TradePage::new(trades, from_start)));
    }

    fn footprints(&self, _: FootprintRequest, reply: Reply<FootprintPage>) {
        reply.send(
            self.rows
                .lock()
                .unwrap()
                .take()
                .ok_or(FeedError::NotSupported),
        );
    }
}

struct NativeRows {
    bars: Vec<FootprintBar>,
    rows: Vec<FootprintLevel>,
    offsets: Vec<usize>,
}
impl NativeRows {
    fn new(mut bars: Vec<FootprintBar>) -> Self {
        let mut rows = Vec::with_capacity(bars.iter().map(|bar| bar.levels.len()).sum());
        let mut offsets = Vec::with_capacity(bars.len() + 1);
        for bar in &mut bars {
            offsets.push(rows.len());
            rows.extend(std::mem::take(&mut bar.levels).into_values());
        }
        offsets.push(rows.len());
        Self {
            bars,
            rows,
            offsets,
        }
    }
}
impl FootprintSource for NativeRows {
    fn len(&self) -> usize {
        self.bars.len()
    }
    fn bar(&self, index: usize) -> FootprintBarView<'_> {
        let mut bar = FootprintBarView::from(&self.bars[index]);
        bar.levels = FootprintRows::Slice(&self.rows[self.offsets[index]..self.offsets[index + 1]]);
        bar
    }
}

fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    }
}

fn text_signature(ctx: &egui::Context, chart: &mut Chart) -> u64 {
    fn hash(shape: &egui::Shape, into: &mut std::hash::DefaultHasher) {
        match shape {
            egui::Shape::Text(text) => text.galley.text().hash(into),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| hash(shape, into)),
            _ => (),
        }
    }
    let mut output = ctx.run_ui(input(), |ui| {
        chart.show(ui);
    });
    output.textures_delta.clear();
    let mut result = std::hash::DefaultHasher::new();
    output
        .shapes
        .iter()
        .for_each(|shape| hash(&shape.shape, &mut result));
    result.finish()
}

fn milliseconds(samples: &[frame::Sample], percentile: f64) -> f64 {
    let mut times: Vec<_> = samples
        .iter()
        .map(|sample| sample.frame.as_secs_f64() * 1e3)
        .collect();
    times.sort_by(f64::total_cmp);
    times[((times.len() - 1) as f64 * percentile) as usize]
}

fn main() {
    let frames: usize =
        std::env::var("FRAME_TIME_FRAMES").map_or(300, |v| v.parse().expect("a frame count"));
    assert!(frames > 0, "at least one measured frame");
    let args: Vec<_> = std::env::args().skip(1).collect();
    let all = args.iter().any(|arg| arg == "--all-studies");
    let delta = args.iter().any(|arg| arg == "--delta-poc");
    let stacks = args.iter().any(|arg| arg == "--stacks");
    let annotations = !args.iter().any(|arg| arg == "--no-annotations");
    let statistics = !args.iter().any(|arg| arg == "--no-statistics");
    let filters: Vec<_> = args.iter().filter(|arg| !arg.starts_with("--")).collect();
    println!(
        "\n| footprint-feed | load ms | steady p50 ms | steady p95 ms | live p50 ms | live p95 ms | shapes | vertices |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|");
    let mut reference = None;
    for (name, prepared, shared, kind) in [
        ("raw", false, false, ChartType::VolumeFootprint),
        ("prepared", true, false, ChartType::VolumeFootprint),
        ("shared", true, true, ChartType::VolumeFootprint),
        ("candles", false, false, ChartType::Candles),
    ] {
        if !filters.is_empty() && !filters.iter().any(|filter| name.contains(filter.as_str())) {
            continue;
        }
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..BARS)
            .map(|i| {
                Bar::new(
                    start + TimeDelta::minutes(i as i64),
                    100.0,
                    103.75,
                    100.0,
                    103.75,
                    ROWS as f64,
                )
            })
            .collect();
        let mut trades: Vec<_> = (0..BARS * ROWS)
            .map(|i| {
                Tick::new(
                    start
                        + TimeDelta::minutes((i / ROWS) as i64)
                        + TimeDelta::seconds((i % ROWS) as i64),
                    100.0 + (i % ROWS) as f64 * 0.25,
                    1.0,
                    [Some(OrderSide::Buy), Some(OrderSide::Sell), None][i % 3],
                )
                .with_id(i as u64)
            })
            .collect();
        let rows = prepared.then(|| {
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
            for trade in &trades {
                flow.push(trade).unwrap();
            }
            assert_eq!(
                flow.bars().values().map(|bar| bar.volume()).sum::<f64>(),
                (BARS * ROWS) as f64
            );
            flow.bars().values().cloned().collect()
        });
        let cutoff = bars.last().unwrap().time + TimeDelta::seconds(30);
        let boundary = Tick::new(cutoff, 100.0, 1.0, None).with_id((BARS * ROWS) as u64);
        // The raw reply proves the first streamed instant; the stream later repeats its id.
        if prepared {
            trades.clear();
        }
        trades.push(boundary.clone());
        let rows = rows.map(|bars| {
            if shared {
                FootprintPage::from_shared(Arc::new(NativeRows::new(bars)), cutoff, true)
            } else {
                FootprintPage::new(bars, cutoff, true)
            }
        });
        let feed = Arc::new(Feed {
            bars,
            trades: Mutex::new(Some(trades)),
            rows: Mutex::new(rows),
            sink: Mutex::new(None),
        });
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("SAMPLE")
            .with_initial_bars(BARS)
            .with_chart_type(kind)
            .build();
        chart.config_mut().show_countdown = false;
        let mut params = chart.chart_type_params().clone();
        params.footprint_statistics = statistics;
        params.footprint_naked_poc = annotations && (all || (!delta && !stacks));
        params.footprint_delta_poc = annotations && (all || delta);
        params.footprint_stacked_imbalances = annotations && (all || stacks);
        params.footprint_unfinished_auctions = annotations && all;
        chart.set_chart_type_params(params);
        chart.set_server_time(Some(cutoff));
        let ctx = egui::Context::default();
        let begun = Instant::now();
        for _ in 0..4 {
            chart.apply_synced_time_scale(80.0, 1.0);
            frame::measure(&ctx, input(), |ui| {
                chart.show(ui);
            });
        }
        let sink = feed.sink.lock().unwrap().clone().unwrap();
        sink.push_tick(boundary);
        frame::measure(&ctx, input(), |ui| {
            chart.show(ui);
        });
        let load = begun.elapsed().as_secs_f64() * 1e3;
        assert_eq!(chart.bars(SeriesId::MAIN).len(), BARS);
        let before = text_signature(&ctx, &mut chart);
        let steady = frame::run(
            egui::vec2(1440.0, 900.0),
            egui::pos2(720.0, 400.0),
            frames,
            |ui| {
                chart.show(ui);
            },
        );
        let mut tick = 0;
        let live = frame::run(
            egui::vec2(1440.0, 900.0),
            egui::pos2(720.0, 400.0),
            frames,
            |ui| {
                tick += 1;
                sink.push_tick(
                    Tick::new(
                        cutoff + TimeDelta::milliseconds(tick),
                        100.0 + (tick % ROWS as i64) as f64 * 0.25,
                        1.0,
                        None,
                    )
                    .with_id((BARS * ROWS) as u64 + tick as u64),
                );
                chart.show(ui);
            },
        );
        let last = live.last().unwrap();
        let after = text_signature(&ctx, &mut chart);
        let signature = (before, after, last.shapes, last.vertices);
        if kind == ChartType::VolumeFootprint {
            if let Some(reference) = reference {
                assert_eq!(signature, reference, "raw/prepared/shared output parity");
            } else {
                reference = Some(signature);
            }
        }
        assert_eq!(
            chart.trades(SeriesId::MAIN).count(),
            if prepared { 1 } else { BARS * ROWS + 1 } + frame::WARMUP + frames
        );
        println!("signature {name}: {before:016x} {after:016x}");
        println!(
            "| {name} | {load:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |",
            milliseconds(&steady, 0.5),
            milliseconds(&steady, 0.95),
            milliseconds(&live, 0.5),
            milliseconds(&live, 0.95),
            last.shapes,
            last.vertices
        );
    }
}
