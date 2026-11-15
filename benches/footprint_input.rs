//! Owned read snapshots and producer blocking, on generated one-minute footprints.
//! Query timing includes mutex acquisition/release, but not snapshot destruction.
//! Snapshot CSV: bars, rows per bar, range, p50 us, p95 us, allocation calls,
//! allocated bytes, copied price rows. Writer CSV: queries enabled, samples,
//! p50 us, p95 us, p99 us, max us. Ingestion CSV: fixture, elapsed ms, allocations, bytes.

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
    let start: DateTime<Utc> = "2026-01-05T00:00:00Z".parse().unwrap();
    for (bars, rows) in [(64, 16), (480, 64), (1_500, 64)] {
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(0.25));
        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        let begin = Instant::now();
        for bar in 0..bars {
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
        println!("recording,{bars},{rows},{recording_ms:.3},{calls},{bytes}");
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
                start + TimeDelta::minutes(bars - 1) + TimeDelta::seconds(1),
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
            (bars * rows) as f64
        );
        let latest = start + TimeDelta::minutes(bars - 1) + TimeDelta::seconds(2);
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
