//! Exact delta candles from aggressor-side executions, independently of the bar estimator.

use crate::data::Footprint;
use crate::model::Bar;

/// One bar's trade-based delta, cumulative delta candle, and rolling totals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TradeDeltaValue {
    /// Buy minus sell volume in this bar.
    pub delta: f64,
    /// Cumulative delta before the first execution.
    pub open: f64,
    /// Maximum cumulative delta reached, including the open.
    pub high: f64,
    /// Minimum cumulative delta reached, including the open.
    pub low: f64,
    /// Cumulative delta after the last execution.
    pub close: f64,
    /// Total executed volume over the last N displayed bars, including unknown sides.
    pub rolling_volume: f64,
    /// Buy minus sell volume over the last N displayed bars.
    pub rolling_delta: f64,
}

/// Computes one value per supplied bar, with zero delta for bars without executions.
///
/// `reset(i, bar)` marks the start of a host-defined session or period; the first bar always
/// starts at zero. Rolling totals cover the last `rolling.max(1)` displayed bars and continue
/// across resets. Size filtering belongs to [`Footprint::set_size_filter`], before aggregation.
/// Bars must be chronological, one per slot of the footprint's clock. A timestamp inside
/// the slot is accepted, as in the feed history contract.
/// Historical coverage is exactly the supplied executions; missing trades are never estimated.
pub fn trade_delta(
    footprint: &Footprint,
    bars: &[Bar],
    rolling: usize,
    reset: impl Fn(usize, &Bar) -> bool,
) -> Vec<TradeDeltaValue> {
    let mut values: Vec<TradeDeltaValue> = Vec::with_capacity(bars.len());
    let mut cumulative = 0.0;
    let mut volume_sum = 0.0;
    let mut delta_sum = 0.0;
    let rolling = rolling.max(1);
    for (i, bar) in bars.iter().enumerate() {
        if reset(i, bar) {
            cumulative = 0.0;
        }
        let flow = footprint.bar_at(bar.time);
        let delta = flow.map_or(0.0, |bar| bar.delta());
        let volume = flow.map_or(0.0, |bar| bar.volume());
        if i >= rolling {
            let old = footprint.bar_at(bars[i - rolling].time);
            volume_sum -= old.map_or(0.0, |bar| bar.volume());
            delta_sum -= values[i - rolling].delta;
        }
        volume_sum += volume;
        delta_sum += delta;
        let value = TradeDeltaValue {
            delta,
            open: cumulative,
            high: cumulative + flow.map_or(0.0, |bar| bar.max_delta),
            low: cumulative + flow.map_or(0.0, |bar| bar.min_delta),
            close: cumulative + delta,
            rolling_volume: volume_sum,
            rolling_delta: delta_sum,
        };
        cumulative = value.close;
        values.push(value);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use chrono::{DateTime, TimeDelta, Utc};

    #[test]
    fn candles_follow_executions_and_reset_without_resetting_the_rolling_window() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..4)
            .map(|i| {
                Bar::new(
                    start + TimeDelta::minutes(i),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    999.0,
                )
            })
            .collect();
        let mut footprint = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        footprint.set_size_filter(2.0, 10.0).unwrap();
        for (second, size, side) in [
            (0, 3.0, OrderSide::Sell),
            (1, 8.0, OrderSide::Buy),
            (2, 2.0, OrderSide::Sell),
            (3, 1.0, OrderSide::Buy),
            (60, 4.0, OrderSide::Buy),
            (180, 6.0, OrderSide::Sell),
        ] {
            footprint
                .push(&Tick::new(
                    start + TimeDelta::seconds(second),
                    100.0,
                    size,
                    Some(side),
                ))
                .unwrap();
        }
        // Bar 0 follows -3, +5, +3; the size-1 buy is filtered. Bar 2 is empty.
        let expected = [
            [3.0, 0.0, 5.0, -3.0, 3.0, 13.0, 3.0],
            [4.0, 3.0, 7.0, 3.0, 7.0, 17.0, 7.0],
            [0.0, 7.0, 7.0, 7.0, 7.0, 4.0, 4.0],
            [-6.0, 0.0, 0.0, -6.0, -6.0, 6.0, -6.0],
        ];
        for (got, expected) in trade_delta(&footprint, &bars, 2, |i, _| i == 3)
            .iter()
            .zip(expected)
        {
            assert_eq!(
                [
                    got.delta,
                    got.open,
                    got.high,
                    got.low,
                    got.close,
                    got.rolling_volume,
                    got.rolling_delta
                ],
                expected
            );
        }
        // A one-bar window replaces its old volume before adding the next; two finite
        // 1e308 volumes must not overflow an intermediate two-bar sum.
        let mut large = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (bar, side) in bars.iter().zip([OrderSide::Buy, OrderSide::Sell]) {
            large
                .push(&Tick::new(bar.time, 100.0, 1e308, Some(side)))
                .unwrap();
        }
        let values = trade_delta(&large, &bars[..2], 1, |_, _| false);
        assert_eq!(
            (
                values[1].rolling_volume,
                values[1].rolling_delta,
                values[1].close
            ),
            (1e308, -1e308, 0.0)
        );
    }
}
