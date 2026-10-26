//! Order flow of a busy instrument through a chart's feed: a synthetic book of 200 levels a side
//! with 2,000 trades a second, each followed by the depth delta it caused, 6 orders added or
//! cancelled near the touch and a quote, 18,000 messages a second in all. Every 1/60 s of market
//! time the bench pushes that frame's messages into the chart's sink and draws one frame, which
//! applies them. It prints the messages pushed per second of push time, the frame time, the heap
//! allocations per message pushed, and the heap the chart holds at the end, with the depth
//! history sampled every second and with one filled to all its resolutions.
//!
//! `ORDER_FLOW_SECONDS` sets the market time simulated (default 300); arguments select scenarios
//! by name as in `frame_time`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{TimeDelta, Utc};
use egui::{Pos2, RawInput, Rect, vec2};
use egui_charts::ChartBuilder;
use egui_charts::data::{
    Datafeed, DepthHistory, DepthHistoryConfig, HistoryPage, HistoryRequest, Reply, Sink,
    Subscription, SubscriptionId,
};
use egui_charts::model::{Instrument, Level1Quote, OrderBook, OrderSide, Tick, Timeframe};
use egui_charts::widget::SeriesId;

#[expect(dead_code, reason = "this bench reports only the frame time")]
mod frame;
#[path = "../examples/demo/market.rs"]
mod market;

const TRADES_PER_SECOND: usize = 2_000;
const CHURN_PER_TRADE: usize = 6;
const TICK_SIZE: f64 = 0.25;

/// Counts the bytes in use and the allocations made.
struct Counting;

static IN_USE: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn grew(bytes: usize) {
    IN_USE.fetch_add(bytes, Ordering::Relaxed);
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
}

// SAFETY: every call forwards to `System` with the caller's arguments; the counters only observe.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grew(layout.size());
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
            grew(new_size);
        }
        new
    }
}

fn main() {
    let seconds: usize = std::env::var("ORDER_FLOW_SECONDS")
        .map_or(300, |v| v.parse().expect("a number of seconds"));
    assert!(seconds > 0, "at least one second of market time");
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    println!(
        "| scenario | messages | messages/s pushed | frame p50 ms | frame max ms | allocations per message | heap held MB | most heap held MB | history columns |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (name, cadence) in [
        ("depth history every second", TimeDelta::seconds(1)),
        ("depth history filled", TimeDelta::milliseconds(1)),
    ] {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        run(name, cadence, seconds);
    }
}

fn run(name: &str, cadence: TimeDelta, seconds: usize) {
    let feed = Arc::new(Feed::default());
    let mut chart = ChartBuilder::new()
        .with_symbol("BUSY")
        .with_timeframe(Timeframe::Min1)
        .with_feed(feed.clone())
        .build();
    let config = DepthHistoryConfig {
        cadence,
        ..Default::default()
    };
    chart.set_depth_history(Some(DepthHistory::new(config).unwrap()));
    let ctx = egui::Context::default();
    let mut now = Utc::now();
    let show = |chart: &mut egui_charts::Chart, time: f64| {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            time: Some(time),
            ..Default::default()
        };
        frame::measure(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
        })
        .frame
    };
    let mut frames = 0;
    let sink = loop {
        show(&mut chart, frames as f64 / 60.0);
        frames += 1;
        if let Some(sink) = feed.sink.lock().unwrap().clone() {
            break sink;
        }
    };
    // The heap held from here on is the chart's, but for the generator's book and one frame's
    // messages, both small.
    let baseline = IN_USE.load(Ordering::Relaxed);
    let mut market = market::Market::new("BUSY", TICK_SIZE, 20_000, 0x5eed);
    sink.set_server_time(now);
    let (sequence, book) = market.snapshot(now);
    sink.push_depth_snapshot(sequence, book);

    let mut price = 20_000;
    let mut random = 0x9e37_79b9_7f4a_7c15_u64;
    let mut batch = Batch::default();
    let trades_per_frame = TRADES_PER_SECOND / 60;
    let step = TimeDelta::microseconds(1_000_000 / TRADES_PER_SECOND as i64);
    let (mut pushed, mut push_time, mut allocations) = (0, Duration::ZERO, 0);
    let mut samples = Vec::new();
    let mut held = 0;
    for _ in 0..seconds * 60 {
        batch.clear();
        for _ in 0..trades_per_frame {
            now += step;
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let ticks = (random % 5) as i64 - 2;
            price += ticks;
            let side = if ticks >= 0 {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };
            let size = (1 + random % 20) as f64;
            let trade = Tick::new(now, price as f64 * TICK_SIZE, size, Some(side));
            batch.push(Message::Trade(trade));
            let (sequence, delta) = market.trade(now, price, side, size);
            batch.push_delta(sequence, delta);
            for _ in 0..CHURN_PER_TRADE {
                let (sequence, delta) = market.churn(now);
                batch.push_delta(sequence, delta);
            }
            batch.push(Message::Quote(market.quote(now)));
        }
        sink.set_server_time(now);
        pushed += batch.len();
        let before = ALLOCATIONS.load(Ordering::Relaxed);
        let start = Instant::now();
        batch.send(&sink);
        push_time += start.elapsed();
        allocations += ALLOCATIONS.load(Ordering::Relaxed) - before;
        samples.push(show(&mut chart, frames as f64 / 60.0));
        frames += 1;
        held = held.max(IN_USE.load(Ordering::Relaxed));
    }

    // The generator and the book must agree: a gap or an invalid update would have lost the book.
    let (_, expected) = market.snapshot(now);
    let book = chart.order_book(SeriesId::MAIN).expect("a book");
    let levels = |book: &OrderBook| -> Vec<(f64, f64)> {
        let side = |levels: &[egui_charts::model::OrderBookLevel]| {
            Vec::from_iter(levels.iter().map(|level| (level.price, level.size)))
        };
        [side(&book.bids), side(&book.asks)].concat()
    };
    assert_eq!(levels(book), levels(&expected), "{name}: the chart's book");
    assert!(!book.is_crossed(), "{name}: crossed");
    let state = chart.feed_state(SeriesId::MAIN).unwrap();
    assert_eq!(state.rejected_depth, 0, "{name}");
    assert_eq!(feed.resnapshots.load(Ordering::Relaxed), 0, "{name}");
    let columns = chart
        .depth_history(SeriesId::MAIN)
        .map_or(0, |history| history.columns().count());

    samples.sort();
    let mb = |bytes: usize| bytes.saturating_sub(baseline) as f64 / 1e6;
    println!(
        "| {name} | {pushed} | {:.0} | {:.3} | {:.3} | {:.4} | {:.1} | {:.1} | {columns} |",
        pushed as f64 / push_time.as_secs_f64(),
        samples[samples.len() / 2].as_secs_f64() * 1e3,
        samples[samples.len() - 1].as_secs_f64() * 1e3,
        allocations as f64 / pushed as f64,
        mb(IN_USE.load(Ordering::Relaxed)),
        mb(held),
    );
    drop(chart);
}

enum Message {
    Trade(Tick),
    /// The index of a delta in [`Batch::deltas`].
    Delta(usize),
    Quote(Level1Quote),
}

/// One frame's messages, the deltas copied into books kept from frame to frame, so neither the
/// generator nor the bench allocates for them while they are pushed.
#[derive(Default)]
struct Batch {
    messages: Vec<Message>,
    deltas: Vec<(u64, OrderBook)>,
    used: usize,
}

impl Batch {
    fn clear(&mut self) {
        self.messages.clear();
        self.used = 0;
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    fn push_delta(&mut self, sequence: u64, delta: &OrderBook) {
        if self.used == self.deltas.len() {
            self.deltas.push((sequence, delta.clone()));
        } else {
            let slot = &mut self.deltas[self.used];
            slot.0 = sequence;
            slot.1.clone_from(delta);
        }
        self.messages.push(Message::Delta(self.used));
        self.used += 1;
    }

    fn send(&mut self, sink: &Sink) {
        for message in self.messages.drain(..) {
            match message {
                Message::Trade(trade) => sink.push_tick(trade),
                Message::Delta(at) => {
                    let (sequence, delta) = &self.deltas[at];
                    sink.push_depth_delta(*sequence, delta);
                }
                Message::Quote(quote) => sink.push_quote(quote),
            }
        }
    }
}

/// A feed without history that hands its subscription's sink to the bench.
#[derive(Default)]
struct Feed {
    sink: Mutex<Option<Sink>>,
    resnapshots: AtomicUsize,
}

impl Datafeed for Feed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Instrument::new(symbol).with_tick_size(TICK_SIZE)));
    }

    fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
        reply.send(Ok(HistoryPage {
            bars: Vec::new(),
            end_of_history: true,
        }));
    }

    fn subscribe(&self, subscription: Subscription) {
        *self.sink.lock().unwrap() = Some(subscription.sink);
    }

    fn request_depth_snapshot(&self, _: SubscriptionId) {
        self.resnapshots.fetch_add(1, Ordering::Relaxed);
    }
}
