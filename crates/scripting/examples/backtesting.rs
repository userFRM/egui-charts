//! A strategy written as a script beside the same strategy in Rust, backtested on the same bars.
//!
//! Run with `cargo run --example backtesting` from `crates/scripting`.

use chrono::{DateTime, TimeDelta};
use egui_charts::model::{Bar, OrderSide, OrderType, Timeframe};
use egui_charts::strategies::{Backtest, Strategy, StrategyContext};
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::Script;

/// Long while the close rises, flat once it falls.
const MOMENTUM: &str = r#"// Long while the close rises, flat once it falls.
if close > close[1] && position() == 0.0 {
    entry("Long", "long", #{ qty: 10 });
} else if close < close[1] && position() > 0.0 {
    exit_order("Close");
}"#;

/// The script's rule in Rust.
struct Momentum;

impl Strategy for Momentum {
    fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
        let [.., previous, bar] = context.bars() else {
            return;
        };
        if bar.close > previous.close && context.position() == 0.0 {
            context.entry("Long", OrderSide::Buy, 10.0, OrderType::Market);
        } else if bar.close < previous.close && context.position() > 0.0 {
            context.exit("Close", None, None, None);
        }
    }
}

/// 300 hourly bars of a smooth walk around 100.
fn bars() -> Vec<Bar> {
    let price = |i: usize| 100.0 + (i as f64 * 0.37).sin() * 4.0 + (i as f64 * 0.05).cos() * 3.0;
    (0..300)
        .map(|i| {
            let (open, close) = (price(i), price(i + 1));
            let time = DateTime::UNIX_EPOCH + TimeDelta::hours(i as i64);
            Bar::new(
                time,
                open,
                open.max(close) + 0.3,
                open.min(close) - 0.3,
                close,
                1000.0,
            )
        })
        .collect()
}

fn main() {
    let bars = bars();
    let script = Script::compile("Momentum", MOMENTUM, &IndicatorFactory::new())
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(script.places_orders());
    let backtest = Backtest::new("Momentum", "SAMPLE", Timeframe::Hour1, 10_000.0);
    let native = backtest.run(&mut Momentum, &bars, &[]);
    let scripted = backtest.run(&mut script.strategy(), &bars, &[]);
    for (name, report) in [("Rust", &native), ("Script", &scripted)] {
        let profit = report.performance().all.net_profit;
        println!(
            "{name}: {} trades, net profit {profit:.2}",
            report.trades.len()
        );
    }
    assert_eq!(scripted.trades, native.trades);
}
