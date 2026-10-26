//! A host-run buy-and-hold backtest and the library's strategy report metrics.
//! Buys two units at the first open and exits at the last close, with one currency unit
//! of commission on each fill. Equity is marked at every bar close after incurred fees.
//!
//! Run headlessly with `cargo run --example backtesting --no-default-features`.

mod support;

use chrono::{TimeDelta, TimeZone, Utc};
use egui_charts::model::{
    Bar, StrategyExit, StrategyReport, StrategySide, StrategyTrade, Timeframe,
};

fn buy_and_hold(bars: &[Bar]) -> StrategyReport {
    let mut report = StrategyReport::new("Buy and hold", "SAMPLE", Timeframe::Hour1, 10_000.0);
    report.currency = Some("USD".into());
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return report;
    };
    let quantity = 2.0;
    let commission = 1.0;
    let profit = (last.close - first.open) * quantity - 2.0 * commission;
    let mut trade = StrategyTrade::new(StrategySide::Long, quantity, first.time, first.open);
    trade.fees = 2.0 * commission;
    trade.exit = Some(StrategyExit::new(
        last.time + TimeDelta::hours(1),
        last.close,
        profit,
    ));
    report.trades.push(trade);
    // Include starting capital so the first bar's loss and entry fee enter drawdown metrics.
    report.equity.push((first.time, report.initial_capital));
    report
        .equity
        .extend(bars.iter().enumerate().map(|(i, bar)| {
            let fees = commission + if i + 1 == bars.len() { commission } else { 0.0 };
            (
                bar.time + TimeDelta::hours(1),
                report.initial_capital + (bar.close - first.open) * quantity - fees,
            )
        }));
    report
}

fn main() {
    let end = Utc.with_ymd_and_hms(2026, 1, 19, 0, 0, 0).unwrap();
    let bars = support::sample_bars(600, 100.0, TimeDelta::hours(1), end);
    let report = buy_and_hold(&bars);
    let metrics = report.performance();
    println!(
        "{} on {} (generated sample data)",
        report.name, report.symbol
    );
    println!("Closed trades: {}", metrics.closed_trades);
    println!("Net profit after fees: {:.2} USD", metrics.net_profit);
    println!(
        "Maximum drawdown: {:.2} USD",
        metrics
            .max_drawdown
            .expect("sample has a valid equity curve")
    );
    println!("{}", report.trades_csv());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equity_includes_entry_and_exit_fees() {
        for (prices, expected_equity, expected_profit, expected_drawdown) in [
            (vec![], vec![], 0.0, None),
            (
                vec![(100.0_f64, 90.0_f64)],
                vec![10_000.0, 9_978.0],
                -22.0,
                Some(22.0),
            ),
            (
                vec![(100.0, 110.0), (110.0, 90.0), (90.0, 120.0)],
                vec![10_000.0, 10_019.0, 9_979.0, 10_038.0],
                38.0,
                Some(40.0),
            ),
        ] {
            let bars: Vec<_> = prices
                .into_iter()
                .enumerate()
                .map(|(i, (open, close))| {
                    Bar::new(
                        chrono::DateTime::UNIX_EPOCH + TimeDelta::hours(i as i64),
                        open,
                        open.max(close),
                        open.min(close),
                        close,
                        1.0,
                    )
                })
                .collect();
            let report = buy_and_hold(&bars);
            let metrics = report.performance();
            assert_eq!(metrics.closed_trades, usize::from(!bars.is_empty()));
            assert_eq!(metrics.net_profit, expected_profit);
            assert_eq!(metrics.max_drawdown, expected_drawdown);
            assert_eq!(
                report
                    .equity
                    .iter()
                    .map(|(_, equity)| *equity)
                    .collect::<Vec<_>>(),
                expected_equity
            );
            if let Some(trade) = report.trades.first() {
                assert_eq!(trade.fees, 2.0);
                assert_eq!(
                    trade.exit.as_ref().unwrap().time,
                    bars.last().unwrap().time + TimeDelta::hours(1)
                );
            }
        }
    }
}
