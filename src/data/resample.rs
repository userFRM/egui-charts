//! Bars of a coarser interval built from bars of a finer one.

use crate::model::{Bar, BarClock, Timeframe};
use chrono::TimeDelta;

/// Builds bars of interval `to` from `bars` of interval `from`, which must be in time order.
/// Prices and volumes are expected to be finite, as stored series keep them.
///
/// Each output bar gathers the consecutive input bars that fall in one bar of
/// [`BarClock::new(to)`](BarClock::new) and is stamped with that bar's open time. It takes the open
/// of the first input bar, the highest high, the lowest low, the close of the last input bar and
/// the summed volume. A period without input bars has no output bar. When `from == to`, the bars
/// are returned unchanged.
///
/// Returns `None` when `from` bars do not nest in `to` bars: weekly and monthly bars into any
/// other interval, or an input bar that would span two output bars, as a 5-minute bar opening at
/// 00:05 does in 7-minute bars. Also returns `None` when the bars of one slot do not combine into a
/// valid bar, as when their volumes add up to infinity.
///
/// ```
/// use chrono::{DateTime, TimeDelta, Utc};
/// use egui_charts::data::{Bar, Timeframe, resample};
///
/// let start: DateTime<Utc> = "2024-03-15T10:00:00Z".parse().unwrap();
/// let minutes: Vec<Bar> = (0..10)
///     .map(|i| {
///         let price = 100.0 + i as f64;
///         Bar::new(start + TimeDelta::minutes(i), price, price + 1.0, price - 1.0, price + 0.5, 10.0)
///     })
///     .collect();
///
/// let bars = resample(&minutes, Timeframe::Min1, Timeframe::Min5).unwrap();
/// assert_eq!(bars.len(), 2);
/// let last = &bars[1];
/// assert_eq!(last.time, start + TimeDelta::minutes(5));
/// assert_eq!(
///     (last.open, last.high, last.low, last.close, last.volume),
///     (105.0, 110.0, 104.0, 109.5, 50.0)
/// );
/// ```
pub fn resample(bars: &[Bar], from: Timeframe, to: Timeframe) -> Option<Vec<Bar>> {
    if from == to {
        return Some(bars.to_vec());
    }
    // Weeks do not nest in months or in fixed multi-week intervals, and a month has no fixed length.
    if matches!(from, Timeframe::Week1 | Timeframe::Month1) {
        return None;
    }
    let (source, target) = (BarClock::new(from), BarClock::new(to));
    let mut resampled: Vec<Bar> = Vec::new();
    for bar in bars {
        let open = source.bar_open(bar.time);
        // The next open rather than `open + from`: the last bar of a day is short when `from`
        // does not divide a day.
        let end = source.bar_open(open.checked_add_signed(from.duration())?);
        let slot = target.bar_open(open);
        if target.bar_open(end - TimeDelta::milliseconds(1)) != slot {
            return None;
        }
        match resampled.last_mut() {
            Some(last) if last.time == slot => *last = super::merge::fold(last, bar)?,
            _ => resampled.push(Bar {
                time: slot,
                ..bar.clone()
            }),
        }
    }
    Some(resampled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn bar(time: &str, [open, high, low, close, volume]: [f64; 5]) -> Bar {
        let time: DateTime<Utc> = time.parse().unwrap();
        Bar::new(time, open, high, low, close, volume)
    }

    /// Guards the OHLCV aggregation, the alignment of output bars to `BarClock`, and the refusal of
    /// intervals whose bars would be split across two output bars.
    #[test]
    fn resamples_into_bar_clock_slots() {
        let min7: Timeframe = "7min".parse().unwrap();
        #[rustfmt::skip]
        let cases = [
            // The seven-minute bar that opens at 23:55 is short: the next one opens at midnight.
            (Timeframe::Min1, min7, vec![
                bar("2024-03-15T23:54:00Z", [10.0, 12.0, 9.0, 11.0, 1.0]),
                bar("2024-03-15T23:55:00Z", [11.0, 13.0, 10.0, 12.0, 2.0]),
                bar("2024-03-15T23:56:00Z", [12.0, 15.0, 11.0, 14.0, 3.0]),
                bar("2024-03-15T23:57:00Z", [14.0, 14.0, 8.0, 9.0, 4.0]),
                bar("2024-03-15T23:58:00Z", [9.0, 11.0, 9.0, 10.0, 5.0]),
                bar("2024-03-15T23:59:00Z", [10.0, 12.0, 10.0, 11.0, 6.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 11.0, 10.0, 10.0, 7.0]),
                bar("2024-03-16T00:01:00Z", [10.0, 13.0, 10.0, 12.0, 8.0]),
            ], Some(vec![
                bar("2024-03-15T23:48:00Z", [10.0, 12.0, 9.0, 11.0, 1.0]),
                bar("2024-03-15T23:55:00Z", [11.0, 15.0, 8.0, 11.0, 20.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 13.0, 10.0, 12.0, 15.0]),
            ])),
            // The short 23:55 bar ends at midnight, so it fits in the day.
            (min7, Timeframe::Day1, vec![
                bar("2024-03-15T23:48:00Z", [10.0, 12.0, 9.0, 11.0, 1.0]),
                bar("2024-03-15T23:55:00Z", [11.0, 15.0, 8.0, 11.0, 20.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 13.0, 10.0, 12.0, 15.0]),
            ], Some(vec![
                bar("2024-03-15T00:00:00Z", [10.0, 15.0, 8.0, 11.0, 21.0]),
                bar("2024-03-16T00:00:00Z", [11.0, 13.0, 10.0, 12.0, 15.0]),
            ])),
            // The 00:05 bar runs to 00:10, across the 00:07 boundary.
            (Timeframe::Min5, min7, vec![
                bar("2024-03-15T00:00:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
                bar("2024-03-15T00:05:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
            ], None),
            // The 02:00 bar is missing: the 00:00 bar is built from the three present.
            (Timeframe::Hour1, Timeframe::Hour4, vec![
                bar("2024-03-15T00:00:00Z", [20.0, 22.0, 19.0, 21.0, 100.0]),
                bar("2024-03-15T01:00:00Z", [21.0, 25.0, 20.0, 24.0, 200.0]),
                bar("2024-03-15T03:00:00Z", [24.0, 24.0, 18.0, 19.0, 300.0]),
                bar("2024-03-15T04:00:00Z", [19.0, 20.0, 17.0, 18.0, 400.0]),
            ], Some(vec![
                bar("2024-03-15T00:00:00Z", [20.0, 25.0, 18.0, 19.0, 600.0]),
                bar("2024-03-15T04:00:00Z", [19.0, 20.0, 17.0, 18.0, 400.0]),
            ])),
            // Saturday and Sunday belong to the week that opened on Monday the 11th.
            (Timeframe::Day1, Timeframe::Week1, vec![
                bar("2024-03-16T00:00:00Z", [30.0, 31.0, 29.0, 30.0, 1.0]),
                bar("2024-03-17T00:00:00Z", [30.0, 33.0, 30.0, 32.0, 2.0]),
                bar("2024-03-18T00:00:00Z", [32.0, 34.0, 31.0, 33.0, 3.0]),
                bar("2024-03-19T00:00:00Z", [33.0, 33.0, 28.0, 29.0, 4.0]),
            ], Some(vec![
                bar("2024-03-11T00:00:00Z", [30.0, 33.0, 29.0, 32.0, 3.0]),
                bar("2024-03-18T00:00:00Z", [32.0, 34.0, 28.0, 29.0, 7.0]),
            ])),
            // Two volumes that add up to infinity give no valid bar.
            (Timeframe::Min1, Timeframe::Min5, vec![
                bar("2024-03-15T00:00:00Z", [1.0, 1.0, 1.0, 1.0, f64::MAX]),
                bar("2024-03-15T00:01:00Z", [1.0, 1.0, 1.0, 1.0, f64::MAX]),
            ], None),
            // Some weeks end in the next month, so weekly bars never build monthly ones, even a week inside one month.
            (Timeframe::Week1, Timeframe::Month1, vec![
                bar("2024-03-04T00:00:00Z", [1.0, 1.0, 1.0, 1.0, 1.0]),
            ], None),
        ];
        for (from, to, bars, expected) in cases {
            assert_eq!(resample(&bars, from, to), expected, "{from} to {to}");
        }
    }
}
