//! Studies on a live feed of 10,000 trades a second, with frames paced at 60 Hz.
//! `FRAME_TIME_FRAMES` sets the measured frames (default 300) after studies catch up and
//! 60 warm-up frames; arguments select scenarios by name as in `frame_time`.
//! `-- tail` measures direct last-bar updates and appends at three history lengths.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use chrono::{DurationRound, TimeDelta, Utc};
use egui::{Color32, Event, Pos2, RawInput, Rect, pos2, vec2};
use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
use egui_charts::model::{Bar, Tick, Timeframe};
use egui_charts::studies::*;
use egui_charts::{ChartBuilder, ChartType};

#[expect(
    dead_code,
    reason = "this bench measures paced frames and only reports frame time"
)]
mod frame;

fn main() {
    let frames: usize =
        std::env::var("FRAME_TIME_FRAMES").map_or(300, |v| v.parse().expect("a frame count"));
    assert!(frames > 0, "at least one measured frame");
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if filters.iter().any(|f| f == "tail") {
        tail_updates();
        return;
    }
    println!("| scenario | frame p50 ms | frame p99 ms | frame max ms | full recomputes/s |");
    println!("|---|---:|---:|---:|---:|");
    for (name, bars, studies, streaming) in [
        ("common 10, 100,000 bars, feed idle", 100_000, 10, false),
        ("common 10, 100,000 bars, streaming", 100_000, 10, true),
        ("common 10, 1,000,000 bars, feed idle", 1_000_000, 10, false),
        ("common 10, 1,000,000 bars, streaming", 1_000_000, 10, true),
        ("common 30, 1,000,000 bars, feed idle", 1_000_000, 30, false),
        ("common 30, 1,000,000 bars, streaming", 1_000_000, 30, true),
        ("KST, 1,000,000 bars, streaming", 1_000_000, 1, true),
    ] {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        let mut chart = ChartBuilder::new()
            .with_symbol("BENCH")
            .with_timeframe(Timeframe::Min1)
            .with_feed(Arc::new(Feed::new(bars, streaming)))
            .with_initial_bars(bars)
            .build();
        chart.set_chart_type(ChartType::Candles);
        chart.set_visible_bars(200);
        let calls = Arc::new(AtomicUsize::new(0));
        let indicators: Vec<Box<dyn Indicator>> = if studies == 1 {
            vec![Box::new(Counted(KnowSureThing::default(), calls.clone()))]
        } else {
            let mut common: Vec<Box<dyn Indicator>> = vec![
                Box::new(SMA::default()),
                Box::new(EMA::default()),
                Box::new(BollingerBands::default()),
                Box::new(RSI::default()),
                Box::new(MACD::default()),
                Box::new(Stochastic::default()),
                Box::new(ATR::default()),
                Box::new(VolumeWeightedAvgPrice::default()),
                Box::new(OnBalanceVolume::default()),
                Box::new(SuperTrend::default()),
            ];
            if studies == 30 {
                common.extend(vec![
                    Box::new(WMA::default()) as Box<dyn Indicator>,
                    Box::new(DEMA::default()),
                    Box::new(TEMA::default()),
                    Box::new(HMA::default()),
                    Box::new(KeltnerChannels::default()),
                    Box::new(DonchianChannels::default()),
                    Box::new(ADX::default()),
                    Box::new(CCI::default()),
                    Box::new(WilliamsR::default()),
                    Box::new(MoneyFlowIndex::default()),
                    Box::new(RateOfChange::default()),
                    Box::new(ParabolicSAR::default()),
                    Box::new(StochasticRSI::default()),
                    Box::new(TRIX::default()),
                    Box::new(Aroon::default()),
                    Box::new(ChaikinMoneyFlow::default()),
                    Box::new(AwesomeOscillator::default()),
                    Box::new(Envelopes::default()),
                    Box::new(StandardDeviation::default()),
                    Box::new(VWMA::default()),
                ]);
            }
            common
        };
        for study in indicators {
            chart.add_indicator(study);
        }
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut warm = None;
        let mut measured = None;
        let mut samples = Vec::with_capacity(frames);
        for k in 0.. {
            let due = start + Duration::from_secs_f64(k as f64 / 60.0);
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            if warm == Some(0) && measured.is_none() {
                calls.store(0, Ordering::Relaxed);
                measured = Some(Instant::now());
            }
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
                time: Some(start.elapsed().as_secs_f64()),
                events: vec![Event::PointerMoved(pos2(900.0, 350.0))],
                ..Default::default()
            };
            let sample = frame::measure(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    chart.show(ui);
                });
            });
            if measured.is_some() {
                samples.push(sample.frame);
                if samples.len() == frames {
                    break;
                }
            } else if let Some(left) = &mut warm {
                *left -= 1;
            } else if chart.state().data().len() >= bars
                && chart
                    .indicators()
                    .all(|(_, study)| study.values().len() == chart.state().data().len())
            {
                warm = Some(60);
            }
        }
        let elapsed = measured.unwrap().elapsed().as_secs_f64();
        samples.sort();
        println!(
            "| {name} | {:.3} | {:.3} | {:.3} | {} |",
            samples[frames / 2].as_secs_f64() * 1e3,
            samples[(frames * 99).div_ceil(100) - 1].as_secs_f64() * 1e3,
            samples[frames - 1].as_secs_f64() * 1e3,
            if studies == 1 {
                format!("{:.3}", calls.load(Ordering::Relaxed) as f64 / elapsed)
            } else {
                "-".into()
            }
        );
    }
}

/// Compare tick and append work at several history lengths, without layout or feed timing.
fn tail_updates() {
    println!("| study | bars | tick median us | append median us | incremental |");
    println!("|---|---:|---:|---:|---|");
    let mut factory = IndicatorFactory::new();
    factory.register("C2C Vol", || Box::new(CloseToCloseVol::default()));
    factory.register("Price Channel", || Box::new(PriceChannel::default()));
    factory.register("HLC3", || Box::new(HLC3::default()));
    factory.register("HL2", || Box::new(HL2::default()));
    factory.register("CVD", || Box::new(CVD::default()));
    factory.register("Ehlers Fisher", || Box::new(EhlersFisher::default()));
    let mut names: Vec<_> = factory
        .list()
        .into_iter()
        .filter(|name| !name.starts_with("Vol Profile ") && name != "Visible Average Price")
        .collect();
    names.sort_unstable();
    for count in [1_000, 100_000, 1_000_000] {
        let mut bars: Vec<_> = (0..count + 5)
            .map(|i| {
                let p = 100.0 + (i as f64 / 7.0).sin();
                Bar::new(
                    chrono::DateTime::<Utc>::UNIX_EPOCH + TimeDelta::minutes(i as i64),
                    p,
                    p + 1.0,
                    p - 1.0,
                    p,
                    100.0,
                )
            })
            .collect();
        for name in &names {
            let mut study = factory.create(name).expect("a built-in study");
            study.calculate(&bars[..count]);
            let mut ticks = Vec::new();
            for _ in 0..5 {
                bars[count - 1].close += 0.001;
                let start = Instant::now();
                study.update(std::hint::black_box(&bars[..count]), count - 1);
                ticks.push(start.elapsed());
            }
            let mut appends = Vec::new();
            for end in count..count + 5 {
                let start = Instant::now();
                study.update(std::hint::black_box(&bars[..=end]), end);
                appends.push(start.elapsed());
            }
            ticks.sort();
            appends.sort();
            println!(
                "| {name} | {count} | {:.3} | {:.3} | {} |",
                ticks[2].as_secs_f64() * 1e6,
                appends[2].as_secs_f64() * 1e6,
                study.is_incremental()
            );
        }
    }
}

struct Rng(u64);
impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64
    }
}

struct Feed {
    bars: Vec<Bar>,
    streaming: bool,
}
impl Feed {
    fn new(count: usize, streaming: bool) -> Self {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let now = Utc::now().duration_trunc(TimeDelta::minutes(1)).unwrap();
        let mut price = 100.0_f64;
        let bars = (0..count)
            .map(|i| {
                let open = price;
                price *= 1.0 + (rng.unit() - 0.5) * 0.008;
                let high = open.max(price) * (1.0 + rng.unit() * 0.002);
                let low = open.min(price) * (1.0 - rng.unit() * 0.002);
                Bar::new(
                    now - TimeDelta::minutes((count - 1 - i) as i64),
                    open,
                    high,
                    low,
                    price,
                    10.0 + rng.unit() * 490.0,
                )
            })
            .collect();
        Self { bars, streaming }
    }
}
impl Datafeed for Feed {
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let end = self
            .bars
            .partition_point(|bar| request.before.is_none_or(|t| bar.time < t));
        let start = end.saturating_sub(request.count);
        reply.send(Ok(HistoryPage::new(
            self.bars[start..end].to_vec(),
            start == 0,
        )));
    }
    fn subscribe(&self, subscription: Subscription) {
        if !self.streaming {
            return;
        }
        let sink = subscription.sink;
        let mut price = self.bars.last().unwrap().close;
        std::thread::spawn(move || {
            let start = Instant::now();
            let mut sent = 0;
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            while sink.is_active() {
                let due = (start.elapsed().as_secs_f64() * 10_000.0) as u64;
                while sent < due && sink.is_active() {
                    price *= 1.0 + (rng.unit() - 0.5) * 0.001;
                    sink.push_tick(Tick::new(Utc::now(), price, 0.1 + rng.unit() * 4.9, None));
                    sent += 1;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
    }
}

#[derive(Clone)]
struct Counted(KnowSureThing, Arc<AtomicUsize>);
impl Indicator for Counted {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn calculate(&mut self, data: &[Bar]) {
        if !data.is_empty() {
            self.1.fetch_add(1, Ordering::Relaxed);
        }
        self.0.calculate(data);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        if !data.is_empty() && (from == 0 || !self.0.is_incremental()) {
            self.1.fetch_add(1, Ordering::Relaxed);
        }
        self.0.update(data, from);
    }
    fn is_incremental(&self) -> bool {
        self.0.is_incremental()
    }
    fn values(&self) -> &IndicatorValues {
        self.0.values()
    }
    fn colors(&self) -> Vec<Color32> {
        self.0.colors()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        self.0.set_colors(colors);
    }
    fn is_overlay(&self) -> bool {
        self.0.is_overlay()
    }
    fn line_names(&self) -> Vec<String> {
        self.0.line_names()
    }
    fn is_visible(&self) -> bool {
        self.0.is_visible()
    }
    fn set_visible(&mut self, visible: bool) {
        self.0.set_visible(visible);
    }
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}
