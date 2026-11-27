//! The relative strength index of 100,000 bars computed natively, restated in a script and read by
//! a script with `study`, then the other examples and a state ring buffer read 200 times a bar.
//! Prints a Markdown table: the fastest of 9 full calculations and the median of 501 updates by one
//! new bar.

use egui_charts::model::Bar;
use egui_charts::studies::{Calculation, Indicator, IndicatorFactory, RSI};
use egui_charts_scripting::{Script, examples};
use std::time::{Duration, Instant};

const BARS: usize = 100_000;
const UPDATES: usize = 501;

/// A state ring buffer of the last 1,000 closes, read 200 times on every bar.
const RING: &str = "state ring = 0;\nstate at = 0;\n\
    if bar_index == 0 { ring = []; ring.pad(1000, 0.0); }\n\
    ring[at] = close * 1.0;\nat = (at + 1) % 1000;\nlet sum = 0.0;\n\
    for i in 0..200 { sum += ring[(at + i) % 1000]; }\nplot(\"Sum\", sum);";

fn main() {
    let bars = bars();
    println!("| study | full calculation ms | one new bar µs |");
    println!("|---|---:|---:|");
    row("native", &bars, || Box::new(RSI::new(14)));
    let restated = compile(examples()[0].1);
    row("restated in a script", &bars, || Box::new(restated.study()));
    let called = compile("plot(\"RSI\", study(\"RSI\", period = 14));");
    row("read with `study`", &bars, || Box::new(called.study()));
    for (name, source) in &examples()[1..] {
        let script = compile(source);
        row(name, &bars, || Box::new(script.study()));
    }
    let ring = compile(RING);
    row("state ring buffer read 200 times a bar", &bars, || {
        Box::new(ring.study())
    });
}

fn compile(source: &str) -> Script {
    Script::compile("Bench", source, &IndicatorFactory::new()).expect("the script compiles")
}

/// Print the times of `study`.
fn row(name: &str, bars: &[Bar], study: impl Fn() -> Box<dyn Indicator>) {
    let mut full: Vec<_> = (0..9)
        .map(|_| calculate(study().as_mut(), &bars[..BARS], 0))
        .collect();
    full.sort_unstable();
    let mut study = study();
    calculate(study.as_mut(), &bars[..BARS], 0);
    let mut updates: Vec<_> = (BARS..bars.len())
        .map(|last| calculate(study.as_mut(), &bars[..=last], last))
        .collect();
    updates.sort_unstable();
    println!(
        "| {name} | {:.2} | {:.1} |",
        full[0].as_secs_f64() * 1e3,
        updates[UPDATES / 2].as_secs_f64() * 1e6
    );
}

/// How long `study` takes to calculate after the bars from `from` changed.
fn calculate(study: &mut dyn Indicator, bars: &[Bar], from: usize) -> Duration {
    let start = Instant::now();
    study.update(bars, from);
    loop {
        match study.calculation() {
            Some(Calculation::Running(_)) => study.update(bars, bars.len()),
            Some(Calculation::Stopped(message)) => panic!("{message}"),
            _ => return start.elapsed(),
        }
    }
}

/// One-minute bars of a xorshift random walk around 100.
fn bars() -> Vec<Bar> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 2001) as f64 / 1000.0 - 1.0
    };
    let mut close = 100.0;
    (0..BARS + UPDATES)
        .map(|i| {
            let open = close;
            close += next();
            let high = open.max(close) + next().abs() * 0.5;
            let low = open.min(close) - next().abs() * 0.5;
            let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i as i64);
            Bar::new(time, open, high, low, close, 1000.0 + next() * 400.0)
        })
        .collect()
}
