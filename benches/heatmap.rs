//! The liquidity heatmap of a large book through a chart's feed: a synthetic book of up to 1,000
//! levels a side, changed by depth deltas at a fixed rate, sampled into the depth history at a
//! 50 ms cadence and drawn behind the price series in a 1440 by 900 window showing the last three one-minute bars. Every
//! 1/60 s of market time the bench pushes that frame's deltas into the chart's sink and draws one
//! frame, which applies them. It prints the sampling cadence, the time the sink takes per delta
//! (the aggregation of the book into depth columns), the frame time with tessellation, the
//! texture data uploaded per second of market time and the most in one frame, the textures and
//! the heap the chart holds, and the same with a history filled to all its resolutions.
//!
//! `HEATMAP_SECONDS` sets the market time simulated (default 60); arguments select scenarios by
//! name as in `frame_time`. The numbers depend on the machine, so quote it with them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Timelike, Utc};
use egui::{Pos2, RawInput, Rect, TextureId, vec2};
use egui_charts::ChartBuilder;
use egui_charts::data::{
    Datafeed, DepthHistory, DepthHistoryConfig, HistoryPage, HistoryRequest, Reply, Sink,
    Subscription,
};
use egui_charts::model::{Bar, Instrument, OrderBook, OrderBookLevel, OrderSide, Tick, Timeframe};

const TICK_SIZE: f64 = 0.25;
/// Depth deltas a second, each changing a few levels, most of them near the touch.
const DELTAS_PER_SECOND: usize = 400;
const LEVELS_CHANGED: usize = 8;
const MID: i64 = 20_000;

/// Counts the bytes in use.
struct Counting;

static IN_USE: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: Counting = Counting;

// SAFETY: every call forwards to `System` with the caller's arguments; the counter only observes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            IN_USE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) };
        IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
            IN_USE.fetch_add(new_size, Ordering::Relaxed);
        }
        new
    }
}

fn main() {
    let seconds: usize =
        std::env::var("HEATMAP_SECONDS").map_or(60, |v| v.parse().expect("a number of seconds"));
    assert!(seconds > 0, "at least one second of market time");
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    println!(
        "| scenario | cadence ms | deltas pushed | sink µs per delta | frame p50 ms | frame p95 ms | frame max ms | upload KB/s | upload max KB a frame | texture MB | peak heap MB | retained heap MB | history columns |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (name, levels, columns, cadence, warmup) in [
        (
            "100 levels, 1 s (defaults)",
            100,
            600,
            TimeDelta::seconds(1),
            0,
        ),
        (
            "1,000 levels, 50 ms (documented)",
            1000,
            240,
            TimeDelta::milliseconds(50),
            0,
        ),
        (
            "1,000 levels, 50 ms, history filled",
            1000,
            240,
            TimeDelta::milliseconds(50),
            3600,
        ),
    ] {
        if filters.is_empty() || filters.iter().any(|f| name.contains(f.as_str())) {
            run(name, levels, columns, cadence, warmup, seconds);
        }
    }
}

/// One xorshift step.
fn next(random: &mut u64) -> u64 {
    *random ^= *random << 13;
    *random ^= *random >> 7;
    *random ^= *random << 17;
    *random
}

/// The size resting `distance` ticks from the touch.
fn resting(random: &mut u64, distance: i64) -> f64 {
    (1 + next(random) % 20) as f64 * (1.0 + (distance as f64 / 10.0).ln_1p())
}

fn level(price: i64, size: f64) -> OrderBookLevel {
    OrderBookLevel {
        price: price as f64 * TICK_SIZE,
        size,
        order_cnt: None,
    }
}

/// A book of `levels` a side around [`MID`], bids best first.
fn snapshot(random: &mut u64, now: DateTime<Utc>, levels: i64) -> OrderBook {
    OrderBook {
        ts: now,
        symbol: "BIG".into(),
        bids: (0..levels)
            .map(|d| level(MID - 1 - d, resting(random, d)))
            .collect(),
        asks: (0..levels)
            .map(|d| level(MID + 1 + d, resting(random, d)))
            .collect(),
    }
}

/// A delta changing [`LEVELS_CHANGED`] levels, at geometric distances from the touch, sorted as
/// the contract wants: bids descending, asks ascending.
fn delta(random: &mut u64, now: DateTime<Utc>, levels: i64) -> OrderBook {
    let (mut bids, mut asks) = (Vec::new(), Vec::new());
    for _ in 0..LEVELS_CHANGED {
        let mut distance = 0;
        while distance < levels - 1 && next(random) % 10 < 9 {
            distance += 1;
        }
        let size = resting(random, distance);
        if next(random).is_multiple_of(2) {
            bids.push(level(MID - 1 - distance, size));
        } else {
            asks.push(level(MID + 1 + distance, size));
        }
    }
    bids.sort_by(|a, b| b.price.total_cmp(&a.price));
    bids.dedup_by(|a, b| a.price == b.price);
    asks.sort_by(|a, b| a.price.total_cmp(&b.price));
    asks.dedup_by(|a, b| a.price == b.price);
    OrderBook {
        ts: now,
        symbol: "BIG".into(),
        bids,
        asks,
    }
}

fn run(name: &str, levels: i64, columns: usize, cadence: TimeDelta, warmup: usize, seconds: usize) {
    let now = Utc::now().with_nanosecond(0).unwrap();
    let feed = Arc::new(Feed {
        sink: Mutex::default(),
        minute: now - TimeDelta::seconds(i64::from(now.second())),
    });
    let mut chart = ChartBuilder::new()
        .with_symbol("BIG")
        .with_timeframe(Timeframe::Min1)
        .with_feed(feed.clone())
        .build();
    let config = DepthHistoryConfig {
        cadence,
        rows: levels as usize,
        columns,
        ..Default::default()
    };
    chart.set_depth_history(Some(DepthHistory::new(config).unwrap()));
    chart.set_liquidity_heatmap(true);
    let ctx = egui::Context::default();
    // Every texture the chart holds, by id, so the bench knows how many are alive.
    let textures = RefCell::new(HashSet::new());
    let (uploaded, upload_max) = (Cell::new(0), Cell::new(0));
    let show = |chart: &mut egui_charts::Chart, time: f64| {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            time: Some(time),
            ..Default::default()
        };
        let start = Instant::now();
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
        });
        let mut bytes = 0;
        for (id, deltas) in &out.textures_delta.set {
            // The font atlas is the egui context's, not the heatmap's.
            if *id == TextureId::Managed(0) {
                continue;
            }
            for delta in deltas {
                let [w, h] = delta.image.size();
                bytes += w * h * 4;
                if delta.pos.is_none() {
                    textures.borrow_mut().insert(*id);
                }
            }
        }
        for id in &out.textures_delta.free {
            textures.borrow_mut().remove(id);
        }
        uploaded.set(uploaded.get() + bytes);
        upload_max.set(upload_max.get().max(bytes));
        out.textures_delta.clear();
        let primitives = std::hint::black_box(ctx.tessellate(out.shapes, out.pixels_per_point));
        drop(primitives);
        start.elapsed()
    };
    let mut frames = 0;
    let sink = loop {
        show(&mut chart, frames as f64 / 60.0);
        frames += 1;
        if let Some(sink) = feed.sink.lock().unwrap().clone() {
            break sink;
        }
    };
    // Zoomed to the last three minutes once the history has loaded, where the recent columns are.
    for _ in 0..5 {
        show(&mut chart, frames as f64 / 60.0);
        frames += 1;
    }
    chart.set_visible_bars(3);
    show(&mut chart, frames as f64 / 60.0);
    frames += 1;
    let baseline = IN_USE.load(Ordering::Relaxed);
    let mut random = 0x9e37_79b9_7f4a_7c15_u64;
    let mut now = now;
    sink.set_server_time(now);
    sink.push_depth_snapshot(1, snapshot(&mut random, now, levels));
    let (mut sequence, mut pushed, mut push_time) = (1, 0, Duration::ZERO);
    let per_frame = DELTAS_PER_SECOND / 60;
    let step = TimeDelta::microseconds(1_000_000 / (60 * per_frame) as i64);
    // Market time before the measured seconds, one delta a cadence and a frame every 30 s to
    // drain the columns, so that the history is full when they start.
    for n in 0..warmup * 20 {
        now += TimeDelta::milliseconds(50);
        sequence += 1;
        sink.set_server_time(now);
        sink.push_depth_delta(sequence, &delta(&mut random, now, levels));
        if n % 600 == 599 {
            show(&mut chart, frames as f64 / 60.0);
            frames += 1;
        }
    }
    let mut samples = Vec::new();
    let mut held = 0;
    uploaded.set(0);
    upload_max.set(0);
    for _ in 0..seconds * 60 {
        let deltas: Vec<_> = (0..per_frame)
            .map(|_| {
                now += step;
                delta(&mut random, now, levels)
            })
            .collect();
        sink.set_server_time(now);
        sink.push_tick(Tick::new(
            now,
            MID as f64 * TICK_SIZE,
            1.0,
            Some(OrderSide::Buy),
        ));
        let start = Instant::now();
        for delta in &deltas {
            sequence += 1;
            sink.push_depth_delta(sequence, delta);
        }
        push_time += start.elapsed();
        pushed += deltas.len();
        samples.push(show(&mut chart, frames as f64 / 60.0));
        frames += 1;
        held = held.max(IN_USE.load(Ordering::Relaxed));
    }
    let state = chart
        .feed_state(egui_charts::widget::SeriesId::MAIN)
        .unwrap();
    assert_eq!(state.rejected_depth, 0, "{name}: a delta was rejected");
    let history = chart
        .depth_history(egui_charts::widget::SeriesId::MAIN)
        .expect("a depth history");
    // The feed updates more often than the cadence, so every period has a column of its own.
    let newest = history.columns().next_back().expect("a column");
    assert_eq!(
        newest.span, cadence,
        "{name}: the newest column spans one cadence"
    );
    let columns = history.columns().count();
    samples.sort();
    let ms = |at: usize| samples[at].as_secs_f64() * 1e3;
    let mb = |bytes: usize| bytes.saturating_sub(baseline) as f64 / 1e6;
    println!(
        "| {name} | {} | {pushed} | {:.2} | {:.3} | {:.3} | {:.3} | {:.0} | {:.0} | {:.1} | {:.1} | {:.1} | {columns} |",
        cadence.num_milliseconds(),
        push_time.as_secs_f64() * 1e6 / pushed as f64,
        ms(samples.len() / 2),
        ms(samples.len() * 95 / 100),
        ms(samples.len() - 1),
        uploaded.get() as f64 / 1e3 / seconds as f64,
        upload_max.get() as f64 / 1e3,
        textures.borrow().len() as f64 * 256.0 * 256.0 * 4.0 / 1e6,
        mb(held),
        mb(IN_USE.load(Ordering::Relaxed)),
    );
    drop(chart);
}

/// A feed with a flat hour of one-minute bars spanning the whole book, and the sink handed to the
/// bench.
struct Feed {
    sink: Mutex<Option<Sink>>,
    minute: DateTime<Utc>,
}

impl Datafeed for Feed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol).with_tick_size(TICK_SIZE)));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let mid = MID as f64 * TICK_SIZE;
        let bars = (1..=60)
            .map(|n| {
                Bar::new(
                    self.minute - TimeDelta::minutes(n),
                    mid,
                    mid + 260.0,
                    mid - 260.0,
                    mid,
                    1.0,
                )
            })
            .filter(|bar| request.before.is_none_or(|before| bar.time < before))
            .collect();
        reply.send(Ok(HistoryPage::new(bars, true)));
    }

    fn subscribe(&self, subscription: Subscription) {
        *self.sink.lock().unwrap() = Some(subscription.sink);
    }
}
