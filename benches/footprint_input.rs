//! Owned read snapshots and producer blocking, on generated one-minute footprints.
//! Query timing includes mutex acquisition/release, but not snapshot destruction.
//! Snapshot CSV: bars, rows per bar, range, p50 us, p95 us, allocation calls,
//! allocated bytes, copied price rows. Writer CSV: queries enabled, samples,
//! p50 us, p95 us, p99 us, max us. Ingestion CSV: fixture, elapsed ms, allocations, bytes.
//! `--prepared` uses a shared page admitted by an actual candle chart with a profile study
//! (requires `widget,studies`); fixture construction and admission are outside capture timing.

use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::OrderFlowInput;
use egui_charts::model::{BarClock, Tick, Timeframe};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Counting;
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: Counting = Counting;

// SAFETY: calls forward unchanged to System; counters only observe successful allocations.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let ptr = unsafe { System.realloc(ptr, layout, size) };
        if !ptr.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size, Ordering::Relaxed);
        }
        ptr
    }
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction) as usize]
}

#[cfg(all(feature = "widget", feature = "studies"))]
fn prepared_input(start: DateTime<Utc>, bars: i64, rows: i64) -> OrderFlowInput {
    use egui_charts::data::{
        Datafeed, Footprint, FootprintBar, FootprintPage, FootprintRequest, HistoryPage,
        HistoryRequest, Reply, Subscription, TradePage, TradeRequest,
    };
    use egui_charts::model::{Bar, Instrument};
    use egui_charts::studies::IndicatorFactory;
    use egui_charts::widget::ChartBuilder;

    struct Feed {
        bars: Vec<Bar>,
        rows: Arc<Vec<FootprintBar>>,
        boundary: Tick,
    }
    impl Datafeed for Feed {
        fn resolve(&self, _: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new("SAMPLE").with_tick_size(0.25)));
        }
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::new(self.bars.clone(), true)));
        }
        fn subscribe(&self, subscription: Subscription) {
            subscription.sink.push_tick(self.boundary.clone());
        }
        fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
            reply.send(Ok(TradePage::new(vec![self.boundary.clone()], false)));
        }
        fn footprints(&self, _: FootprintRequest, reply: Reply<FootprintPage>) {
            reply.send(Ok(FootprintPage::from_shared(
                self.rows.clone(),
                self.boundary.ts,
                true,
            )));
        }
    }
    let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
    let mut candles = Vec::new();
    for bar in 0..bars {
        let time = start + TimeDelta::minutes(bar);
        let high = 100.0 + (rows - 1) as f64 * 0.25;
        candles.push(Bar::new(time, 100.0, high, 100.0, high, rows as f64));
        for row in 0..rows {
            flow.push(&Tick::new(
                time + TimeDelta::microseconds(row),
                100.0 + row as f64 * 0.25,
                1.0,
                None,
            ))
            .unwrap();
        }
    }
    let feed = Arc::new(Feed {
        bars: candles,
        rows: Arc::new(flow.bars().values().cloned().collect()),
        boundary: Tick::new(
            start + TimeDelta::minutes(bars - 1) + TimeDelta::seconds(30),
            100.0,
            1.0,
            None,
        )
        .with_id(1),
    });
    let mut chart = ChartBuilder::new()
        .with_feed(feed)
        .with_symbol("SAMPLE")
        .with_initial_bars(bars as usize)
        .build();
    let study = IndicatorFactory::new().create("Vol Profile FR").unwrap();
    let input = study.order_flow_inputs()[0].clone();
    chart.add_indicator(study);
    let ctx = egui::Context::default();
    for _ in 0..6 {
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            chart.show(ui);
        });
    }
    assert_eq!(
        input
            .footprint_snapshot(start)
            .bars(..)
            .map(|bar| bar.volume())
            .sum::<f64>(),
        (bars * rows + 1) as f64
    );
    input
}

fn record_short_bars() {
    let start: DateTime<Utc> = "2026-01-05T00:00:00Z".parse().unwrap();
    for timeframe in [Timeframe::Min1, "1t".parse().unwrap()] {
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(0.25));
        let clock = BarClock::new(timeframe);
        let ticks: Vec<_> = (0..100_000)
            .map(|index| {
                let offset = if timeframe == Timeframe::Min1 {
                    TimeDelta::minutes(index)
                } else {
                    TimeDelta::microseconds(index)
                };
                Tick::new(start + offset, 100.0, 1.0, None)
            })
            .collect();
        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        let begin = Instant::now();
        for tick in &ticks {
            input.push_trade(&clock, tick);
        }
        let ms = begin.elapsed().as_secs_f64() * 1e3;
        let calls = CALLS.load(Ordering::Relaxed);
        let bytes = BYTES.load(Ordering::Relaxed);
        let held = input.footprints_from(start);
        assert_eq!(held.bars.len(), 10_911);
        assert_eq!(
            held.bars.values().map(|bar| bar.volume()).sum::<f64>(),
            10_911.0
        );
        println!("short_bars,{timeframe},100000,{ms:.3},{calls},{bytes}");
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--short-bars") {
        record_short_bars();
        return;
    }
    let clock = BarClock::new(Timeframe::Min1);
    let prepared = std::env::args().any(|arg| arg == "--prepared");
    let start: DateTime<Utc> = "2026-01-05T00:00:00Z".parse().unwrap();
    for (bars, rows) in [(64, 16), (480, 64), (1_500, 64)] {
        let input = if prepared {
            #[cfg(all(feature = "widget", feature = "studies"))]
            {
                prepared_input(start, bars, rows)
            }
            #[cfg(not(all(feature = "widget", feature = "studies")))]
            panic!("--prepared requires widget,studies");
        } else {
            OrderFlowInput::footprint()
        };
        input.set_tick_size(Some(0.25));
        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        let begin = Instant::now();
        for bar in 0..if prepared { 0 } else { bars } {
            for row in 0..rows {
                input.push_trade(
                    &clock,
                    &Tick::new(
                        start + TimeDelta::minutes(bar) + TimeDelta::microseconds(row),
                        100.0 + row as f64 * 0.25,
                        1.0,
                        None,
                    ),
                );
            }
        }
        let recording_ms = begin.elapsed().as_secs_f64() * 1e3;
        let calls = CALLS.load(Ordering::Relaxed);
        let bytes = BYTES.load(Ordering::Relaxed);
        if !prepared {
            println!("recording,{bars},{rows},{recording_ms:.3},{calls},{bytes}");
        }
        for (name, from, expected) in [
            ("all", start, bars * rows),
            ("last_bar", start + TimeDelta::minutes(bars - 1), rows),
        ] {
            let snapshot = input.footprint_snapshot(from);
            assert_eq!(
                snapshot.bars(..).map(|bar| bar.levels.len()).sum::<usize>(),
                expected as usize
            );
            drop(snapshot);
            for _ in 0..10 {
                black_box(input.footprint_snapshot(from));
            }
            let mut samples = Vec::with_capacity(300);
            CALLS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            for _ in 0..300 {
                let begin = Instant::now();
                let snapshot = black_box(input.footprint_snapshot(from));
                samples.push(begin.elapsed().as_secs_f64() * 1e6);
                drop(snapshot);
            }
            let calls = CALLS.load(Ordering::Relaxed) / samples.len();
            let bytes = BYTES.load(Ordering::Relaxed) / samples.len();
            let p50 = percentile(&mut samples, 0.50);
            let p95 = percentile(&mut samples, 0.95);
            println!("snapshot,{bars},{rows},{name},{p50:.3},{p95:.3},{calls},{bytes},{rows}");
        }
        if bars != 1_500 {
            continue;
        }
        let retained = input.footprint_snapshot(start);
        for read in [false, true] {
            let done = Arc::new(AtomicBool::new(false));
            let stop = done.clone();
            let producer = input.clone();
            let clock = clock.clone();
            let trade = Tick::new(
                start + TimeDelta::minutes(bars - 1) + TimeDelta::seconds(31),
                100.0,
                1.0,
                None,
            );
            let writer = std::thread::spawn(move || {
                let mut samples = Vec::with_capacity(10_000);
                while !stop.load(Ordering::Acquire) {
                    let begin = Instant::now();
                    producer.push_trade(&clock, &trade);
                    samples.push(begin.elapsed().as_secs_f64() * 1e6);
                    std::thread::sleep(Duration::from_micros(200));
                }
                samples
            });
            let begin = Instant::now();
            for frame in 1..=90 {
                if read {
                    black_box(input.footprint_snapshot(start));
                }
                let deadline = begin + Duration::from_secs_f64(frame as f64 / 60.0);
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            }
            done.store(true, Ordering::Release);
            let mut samples = writer.join().unwrap();
            assert!(!samples.is_empty());
            let p50 = percentile(&mut samples, 0.50);
            let p95 = percentile(&mut samples, 0.95);
            let p99 = percentile(&mut samples, 0.99);
            let max = samples.last().unwrap();
            println!(
                "writer,{read},{},{p50:.3},{p95:.3},{p99:.3},{max:.3}",
                samples.len()
            );
        }
        assert_eq!(
            retained.bars(..).map(|bar| bar.volume()).sum::<f64>(),
            (bars * rows + i64::from(prepared)) as f64
        );
        let latest = start + TimeDelta::minutes(bars - 1) + TimeDelta::seconds(32);
        for (name, time) in [
            ("current", latest),
            ("rollover", start + TimeDelta::minutes(bars)),
        ] {
            CALLS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            input.push_trade(&clock, &Tick::new(time, 100.0, 1.0, None));
            let calls = CALLS.load(Ordering::Relaxed);
            let bytes = BYTES.load(Ordering::Relaxed);
            println!("retained_write,{name},{calls},{bytes}");
        }
    }
}
