//! Exact delta candles from aggressor-side executions, independently of the bar estimator.

use crate::data::Footprint;
use crate::model::Bar;

/// One bar's trade-based delta, cumulative delta candle, and rolling totals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TradeDeltaValue {
    /// Buy minus sell volume in this bar.
    pub delta: f64,
    /// Highest running delta of this bar's executions, starting from zero: zero or more.
    pub delta_high: f64,
    /// Lowest running delta of this bar's executions, starting from zero: zero or less.
    pub delta_low: f64,
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
        let (delta_low, delta_high) = flow.map_or((0.0, 0.0), |bar| (bar.min_delta, bar.max_delta));
        if i >= rolling {
            let old = footprint.bar_at(bars[i - rolling].time);
            volume_sum -= old.map_or(0.0, |bar| bar.volume());
            delta_sum -= values[i - rolling].delta;
        }
        volume_sum += volume;
        delta_sum += delta;
        let value = TradeDeltaValue {
            delta,
            delta_high,
            delta_low,
            open: cumulative,
            high: cumulative + delta_high,
            low: cumulative + delta_low,
            close: cumulative + delta,
            rolling_volume: volume_sum,
            rolling_delta: delta_sum,
        };
        cumulative = value.close;
        values.push(value);
    }
    values
}

/// Closed-bar detection state; the live bar never enters a run or confirms a pivot.
#[derive(Clone)]
pub(super) struct CvdMarkers {
    pub divergence: bool,
    pub swing_bars: usize,
    pub exhaustion: bool,
    pub exhaustion_bars: usize,
    pub labels: Vec<crate::studies::StudyLabel>,
    processed: usize,
    pivots: [Option<(usize, f64, f64)>; 2],
    runs: [usize; 2],
    previous_delta: f64,
}

impl Default for CvdMarkers {
    fn default() -> Self {
        Self {
            divergence: false,
            swing_bars: 3,
            exhaustion: false,
            exhaustion_bars: 3,
            labels: Vec::new(),
            processed: 0,
            pivots: [None; 2],
            runs: [0; 2],
            previous_delta: 0.0,
        }
    }
}

impl CvdMarkers {
    pub fn enabled(&self) -> bool {
        self.divergence || self.exhaustion
    }

    pub fn start(&self, len: usize, from: usize) -> usize {
        if from < self.processed || len <= self.processed {
            0
        } else {
            self.processed
        }
    }

    pub fn refresh(
        &mut self,
        data: &[Bar],
        from: usize,
        rows: &[Option<[f64; 3]>],
        values: &mut crate::studies::IndicatorValues,
        reset: impl Fn(usize) -> bool,
    ) {
        use crate::studies::{LabelPlacement, StudyLabel};
        if from == 0 {
            self.labels.clear();
            self.pivots = [None; 2];
            self.runs = [0; 2];
            self.previous_delta = 0.0;
        }
        if let Some(live) = data.len().checked_sub(1) {
            for line in 3..7 {
                values.set_value(line, live, f64::NAN);
            }
        }
        let valid_price =
            |bar: &Bar| bar.high.is_finite() && bar.low.is_finite() && bar.high >= bar.low;
        for at in from..data.len().saturating_sub(1) {
            for line in 3..7 {
                values.set_value(line, at, 0.0);
            }
            let bar = &data[at];
            let delta = rows[at - from].map(|r| r[0]);
            if delta.is_none_or(|d| !d.is_finite()) || !valid_price(bar) {
                self.runs = [0; 2];
            } else {
                let delta = delta.unwrap();
                if self.exhaustion && at > 0 && valid_price(&data[at - 1]) {
                    for side in 0..2 {
                        let (new_extreme, signed) = if side == 0 {
                            (bar.low < data[at - 1].low, -delta)
                        } else {
                            (bar.high > data[at - 1].high, delta)
                        };
                        self.runs[side] = if new_extreme && signed > 0.0 {
                            if self.runs[side] > 0 && delta.abs() < self.previous_delta.abs() {
                                self.runs[side].saturating_add(1)
                            } else {
                                1
                            }
                        } else {
                            0
                        };
                        if self.runs[side] >= self.exhaustion_bars {
                            values.set_value(5 + side, at, 1.0);
                            self.labels.push(StudyLabel {
                                bar: at,
                                price: if side == 0 { bar.low } else { bar.high },
                                text: if side == 0 {
                                    "Bull exhaustion"
                                } else {
                                    "Bear exhaustion"
                                }
                                .to_owned(),
                                placement: if side == 0 {
                                    LabelPlacement::Below
                                } else {
                                    LabelPlacement::Above
                                },
                                plot: 1 + side,
                            });
                        }
                    }
                }
                self.previous_delta = delta;
            }
            let span = self.swing_bars.saturating_mul(2);
            if self.divergence
                && at >= span
                && (at - span..=at).all(|i| valid_price(&data[i]))
                && let Some(cvd) = values.value(0, at - self.swing_bars)
            {
                let pivot = at - self.swing_bars;
                let p = &data[pivot];
                for side in 0..2 {
                    let price = if side == 0 { p.low } else { p.high };
                    let is_pivot = (at - span..=at).filter(|&i| i != pivot).all(|i| {
                        if side == 0 {
                            data[i].low > price
                        } else {
                            data[i].high < price
                        }
                    });
                    if !is_pivot {
                        continue;
                    }
                    if self.pivots[side].is_some_and(|(previous, previous_price, previous_cvd)| {
                        let divergent = if side == 0 {
                            price < previous_price && cvd > previous_cvd
                        } else {
                            price > previous_price && cvd < previous_cvd
                        };
                        // A later boundary still permits confirmation of these two swings.
                        divergent
                            && (previous + 1..=pivot).all(|i| {
                                values.value(0, i).is_some() && valid_price(&data[i]) && !reset(i)
                            })
                    }) {
                        values.set_value(3 + side, at, 1.0);
                        let label = StudyLabel {
                            bar: pivot,
                            price,
                            text: if side == 0 {
                                "Bull divergence"
                            } else {
                                "Bear divergence"
                            }
                            .to_owned(),
                            placement: if side == 0 {
                                LabelPlacement::Below
                            } else {
                                LabelPlacement::Above
                            },
                            plot: 1 + side,
                        };
                        // A delayed pivot may precede exhaustion labels already emitted.
                        let insert = self.labels.partition_point(|m| m.bar <= pivot);
                        if let Some(existing) =
                            insert.checked_sub(1).and_then(|i| self.labels.get_mut(i))
                            && existing.bar == label.bar
                            && existing.price == label.price
                        {
                            existing.text.push_str(", ");
                            existing.text.push_str(&label.text);
                        } else {
                            self.labels.insert(insert, label);
                        }
                    }
                    self.pivots[side] = Some((pivot, price, cvd));
                }
            }
        }
        self.processed = data.len().saturating_sub(1);
    }
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
