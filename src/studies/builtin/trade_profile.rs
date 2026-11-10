//! Exact bid, ask and delta profiles over host-selected session or composite ranges.

use crate::data::{FeedError, Footprint, FootprintBar, NodeSensitivity};
use crate::model::{Bar, ProfileRows};
use chrono::{DateTime, Utc};
use std::ops::Range;

/// The first of `bars` that trades in the price `row`: its low below the row's end and its high
/// at or above its start. A bar without volume, such as the one a heartbeat opens, traded
/// nowhere, and a bar with a NaN high or low trades in no row.
pub(crate) fn first_trade_in(bars: &[Bar], row: &Range<f64>) -> Option<usize> {
    bars.iter()
        .position(|bar| bar.volume > 0.0 && bar.low < row.end && row.start <= bar.high)
}

/// Half-open session ranges. Adjacent sessions can be merged and split again by the user.
#[derive(Clone, Debug)]
pub struct TradeProfileRanges(Vec<Range<DateTime<Utc>>>);

impl TradeProfileRanges {
    /// Creates ordered, non-overlapping ranges. The host supplies exchange session boundaries,
    /// including any extended-hours splits, rather than inferring sessions from missing trades.
    pub fn new(ranges: Vec<Range<DateTime<Utc>>>) -> Result<Self, FeedError> {
        if let Some(i) = ranges.iter().position(|r| r.start >= r.end) {
            return Err(FeedError::InvalidData(format!(
                "trade profile range {i} must end after it starts, got {} to {}",
                ranges[i].start, ranges[i].end
            )));
        }
        if let Some(i) = ranges.windows(2).position(|r| r[0].end > r[1].start) {
            return Err(FeedError::InvalidData(format!(
                "trade profile range {} starts at {} before range {i} ends at {}",
                i + 1,
                ranges[i + 1].start,
                ranges[i].end
            )));
        }
        Ok(Self(ranges))
    }

    /// The current ranges in time order.
    pub fn ranges(&self) -> &[Range<DateTime<Utc>>] {
        &self.0
    }

    /// Combines this profile with its next neighbour, including any intervening gap.
    pub fn merge_next(&mut self, index: usize) -> bool {
        if index >= self.0.len().saturating_sub(1) {
            return false;
        }
        self.0[index].end = self.0.remove(index + 1).end;
        true
    }

    /// Splits a profile at an interior bar boundary. An endpoint leaves it unchanged.
    pub fn split(&mut self, index: usize, time: DateTime<Utc>) -> bool {
        let Some(range) = self.0.get_mut(index) else {
            return false;
        };
        if time <= range.start || time >= range.end {
            return false;
        }
        let end = range.end;
        range.end = time;
        self.0.insert(index + 1, time..end);
        true
    }
}

/// Executed volume over one session or composite, with the POC after each contributing bar.
#[derive(Clone, Debug)]
pub struct TradeProfile {
    /// Half-open range of bar opening times.
    pub range: Range<DateTime<Utc>>,
    /// Per-price classified and unclassified volume. POC and value area use total volume.
    pub volume: FootprintBar,
    /// Developing POC as `(bar open, grouped price level)`, including lower-price ties.
    pub developing_poc: Vec<(DateTime<Utc>, i64)>,
    /// Whether the executions of every bar in `range` are held. False when `range` starts
    /// before [`Footprint::complete_from`], or no bar is complete: `volume` and
    /// `developing_poc` then leave out the bars before it, and the profile is partial.
    pub complete: bool,
    /// Rows of the footprint grouped into each row of `volume`, so a level's price is its key
    /// times `group` times [`Footprint::price_step`]. One unless [`trade_profiles_with_rows`]
    /// grouped them.
    pub group: i64,
}

impl TradeProfile {
    /// Inclusive lower and upper value area rows after each contributing bar of `flow`, the
    /// footprint this profile was calculated from, paired with its opening time. Uses
    /// [`FootprintBar::value_area`], including its share and tie rules.
    pub fn developing_value_area(
        &self,
        flow: &Footprint,
        share: f64,
    ) -> Vec<(DateTime<Utc>, (i64, i64))> {
        let mut volume = FootprintBar::empty(self.range.start);
        flow.complete_bars(self.range.clone())
            .filter_map(|bar| {
                volume.absorb_grouped(bar, self.group);
                volume.value_area(share).map(|area| (bar.time, area))
            })
            .collect()
    }

    /// Local volume maxima and minima, respectively, between the first and last traded rows.
    /// Untraded rows have zero volume, so a gap between traded rows is a valley; it is
    /// evaluated as one span, however wide. Rows of equal smoothed volume form one plateau,
    /// reported once at its lowest row, and the outer traded rows are never nodes. See
    /// [`NodeSensitivity`] for the smoothing and prominence applied.
    pub fn nodes(&self, sensitivity: NodeSensitivity) -> (Vec<i64>, Vec<i64>) {
        let mut high = Vec::new();
        let mut low = Vec::new();
        let levels = &self.volume.levels;
        let (Some(&first), Some(&last)) = (levels.keys().next(), levels.keys().next_back()) else {
            return (high, low);
        };
        let reach = i64::from(sensitivity.smoothing);
        let smoothed = |at: i64| {
            levels
                .range(at.saturating_sub(reach)..=at.saturating_add(reach))
                .map(|(_, row)| row.volume())
                .sum::<f64>()
                / (2 * reach + 1) as f64
        };
        // The smoothed volume changes only where a traded row enters or leaves the window.
        let mut starts = vec![first];
        for &level in levels.keys() {
            for start in [level.saturating_sub(reach), level.saturating_add(reach + 1)] {
                if first < start && start <= last {
                    starts.push(start);
                }
            }
        }
        starts.sort_unstable();
        starts.dedup();
        let mut plateaus: Vec<(i64, f64)> = Vec::new();
        for start in starts {
            let volume = smoothed(start);
            if plateaus.last().is_none_or(|&(_, v)| v != volume) {
                plateaus.push((start, volume));
            }
        }
        let largest = plateaus.iter().map(|p| p.1).fold(0.0, f64::max);
        let share = sensitivity.prominence;
        let margin = if share.is_nan() {
            0.0
        } else {
            share.clamp(0.0, 1.0)
        } * largest;
        for pair in plateaus.windows(3) {
            let [(_, before), (level, volume), (_, after)] = [pair[0], pair[1], pair[2]];
            let rise = volume - before.max(after);
            let drop = before.min(after) - volume;
            if rise > 0.0 && rise >= margin {
                high.push(level);
            }
            if drop > 0.0 && drop >= margin {
                low.push(level);
            }
        }
        (high, low)
    }
}

/// Builds exact profiles from footprint rows. No OHLC volume is distributed across prices.
/// Ranges select whole bars, so session and split boundaries must align with the footprint's
/// bar clock. Unknown aggressor volume remains separate and contributes only to total volume.
/// Bars before [`Footprint::complete_from`], whose executions are not all held, are left
/// out; [`TradeProfile::complete`] tells whether a range starts before it.
pub fn trade_profiles(flow: &Footprint, ranges: &TradeProfileRanges) -> Vec<TradeProfile> {
    trade_profiles_with_rows(flow, ranges, ProfileRows::Feed)
}

/// [`trade_profiles`] with the footprint's rows grouped as `rows` says, in whole footprint
/// rows: a row is never finer than the footprint's. [`ProfileRows::Count`] sizes each profile
/// from its own traded span, so profiles of one chart can differ in row height; see
/// [`TradeProfile::group`].
pub fn trade_profiles_with_rows(
    flow: &Footprint,
    ranges: &TradeProfileRanges,
    rows: ProfileRows,
) -> Vec<TradeProfile> {
    let complete_from = flow.complete_from();
    ranges
        .ranges()
        .iter()
        .map(|range| {
            let group = match rows {
                ProfileRows::Feed => 1,
                ProfileRows::Ticks(ticks) => {
                    let feed_ticks = (flow.price_step() / flow.tick_size()).round().max(1.0);
                    (f64::from(ticks) / feed_ticks).ceil().max(1.0) as i64
                }
                ProfileRows::Count(count) => {
                    let (low, high) = flow
                        .complete_bars(range.clone())
                        .flat_map(|bar| bar.levels.keys().copied())
                        .fold((i64::MAX, i64::MIN), |(low, high), l| {
                            (low.min(l), high.max(l))
                        });
                    let span = (i128::from(high) - i128::from(low) + 1).max(1);
                    let count = i128::from(count.max(1));
                    i64::try_from((span + count - 1) / count).unwrap_or(i64::MAX)
                }
            };
            let mut volume = FootprintBar::empty(range.start);
            let mut developing_poc = Vec::new();
            for bar in flow.complete_bars(range.clone()) {
                volume.absorb_grouped(bar, group);
                if let Some(poc) = volume.poc() {
                    developing_poc.push((bar.time, poc));
                }
            }
            TradeProfile {
                range: range.clone(),
                volume,
                developing_poc,
                complete: complete_from.is_some_and(|from| from <= range.start),
                group,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::FootprintLevel;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use chrono::TimeDelta;

    #[test]
    fn sessions_merge_and_split_without_estimating_side_or_developing_poc() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (second, price, size, side) in [
            (0, 99.0, 2.0, Some(OrderSide::Sell)),
            (1, 100.0, 8.0, Some(OrderSide::Buy)),
            (2, 101.0, 1.0, None),
            (3, 102.0, 5.0, Some(OrderSide::Sell)),
            (60, 101.0, 10.0, Some(OrderSide::Buy)),
        ] {
            flow.push(&Tick::new(
                start + TimeDelta::seconds(second),
                price,
                size,
                side,
            ))
            .unwrap();
        }
        let middle = start + TimeDelta::minutes(1);
        let end = start + TimeDelta::minutes(2);
        let mut ranges = TradeProfileRanges::new(vec![start..middle, middle..end]).unwrap();
        let split = trade_profiles(&flow, &ranges);
        assert_eq!(split[0].volume.poc(), Some(100));
        assert_eq!(
            split[0].nodes(NodeSensitivity::default()),
            (vec![100], vec![101])
        );
        assert!(ranges.merge_next(0));
        let merged = trade_profiles(&flow, &ranges);
        assert_eq!(merged[0].developing_poc, [(start, 100), (middle, 101)]);
        assert_eq!(
            (
                merged[0].volume.bid,
                merged[0].volume.ask,
                merged[0].volume.unknown,
                merged[0].volume.delta()
            ),
            (7.0, 18.0, 1.0, 11.0)
        );
        assert!(ranges.split(0, middle));
        let restored = trade_profiles(&flow, &ranges);
        assert_eq!(restored[0].volume, split[0].volume);
        assert_eq!(restored[1].volume, split[1].volume);
        assert!(!ranges.split(0, start));
        for (ranges, expected) in [
            (
                vec![start..start],
                "trade profile range 0 must end after it starts, got 2026-01-05 10:00:00 UTC to 2026-01-05 10:00:00 UTC",
            ),
            (
                vec![start..end, middle..end],
                "trade profile range 1 starts at 2026-01-05 10:01:00 UTC before range 0 ends at 2026-01-05 10:02:00 UTC",
            ),
        ] {
            assert_eq!(
                TradeProfileRanges::new(ranges).unwrap_err().to_string(),
                format!("invalid data: {expected}")
            );
        }
    }

    /// Expected nodes were worked out by hand from the volumes by level, with untraded levels
    /// at zero: a smoothed volume is the mean over `2 * smoothing + 1` levels.
    #[test]
    fn nodes_follow_the_sensitivity_across_steps_plateaus_and_gaps() {
        let nodes = |volumes: &[(i64, f64)], smoothing, prominence| {
            let levels = volumes
                .iter()
                .map(|&(level, ask)| {
                    (
                        level,
                        FootprintLevel {
                            level,
                            ask,
                            ..Default::default()
                        },
                    )
                })
                .collect();
            let time = "2026-01-05T10:00:00Z".parse().unwrap();
            TradeProfile {
                range: time..time,
                volume: FootprintBar {
                    time,
                    levels,
                    bid: 0.0,
                    ask: 0.0,
                    unknown: 0.0,
                    min_delta: 0.0,
                    max_delta: 0.0,
                },
                developing_poc: Vec::new(),
                complete: true,
                group: 1,
            }
            .nodes(NodeSensitivity {
                smoothing,
                prominence,
            })
        };
        let steps = [(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0)];
        let flat = [(1, 5.0), (2, 5.0), (3, 5.0)];
        let plateau = [(1, 1.0), (2, 5.0), (3, 5.0), (4, 1.0)];
        let gaps = [(1, 4.0), (5, 6.0), (9, 4.0)];
        let wide_gap = [(-1_000_000_000_000, 2.0), (1_000_000_000_000, 2.0)];
        let noisy = [(1, 3.0), (2, 1.0), (3, 3.0), (4, 1.0), (5, 3.0)];
        let ragged = [(1, 10.0), (2, 4.0), (3, 6.0), (4, 2.0), (5, 10.0)];
        for (volumes, smoothing, prominence, high, low) in [
            (&steps[..], 0, 0.0, vec![], vec![]),
            (&flat[..], 0, 0.0, vec![], vec![]),
            (&plateau[..], 0, 0.0, vec![2], vec![]),
            (&gaps[..], 0, 0.0, vec![5], vec![2, 6]),
            (&wide_gap[..], 0, 0.0, vec![], vec![-999_999_999_999]),
            (&noisy[..], 0, 0.0, vec![3], vec![2, 4]),
            (&noisy[..], 2, 0.0, vec![3], vec![]),
            (&ragged[..], 0, 0.0, vec![3], vec![2, 4]),
            (&ragged[..], 0, 0.3, vec![], vec![4]),
            (&ragged[..], 0, f64::NAN, vec![3], vec![2, 4]),
        ] {
            assert_eq!(
                nodes(volumes, smoothing, prominence),
                (high, low),
                "{volumes:?}, smoothing {smoothing}, prominence {prominence}"
            );
        }
    }

    #[test]
    fn row_rules_group_footprint_rows_in_whole_ticks() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let end = start + TimeDelta::minutes(1);
        // Ten ticks, 100.0 to 109.0, with the most volume at 107.0.
        let build = |ticks_per_row| {
            let mut flow =
                Footprint::new(BarClock::new(Timeframe::Min1), 1.0, ticks_per_row).unwrap();
            for price in 100..110 {
                let size = if price == 107 { 5.0 } else { 1.0 };
                flow.push(&Tick::new(start, f64::from(price), size, None))
                    .unwrap();
            }
            flow
        };
        let ranges = TradeProfileRanges::new(vec![start..end]).unwrap();
        // (ticks per feed row, rule, rows grouped, POC key, rows held)
        for (feed, rows, group, poc, held) in [
            (1, ProfileRows::Feed, 1, 107, 10),
            (1, ProfileRows::Ticks(5), 5, 21, 2),
            // A row is never finer than the feed's, and holds at least the ticks asked for.
            (2, ProfileRows::Ticks(1), 1, 53, 5),
            (2, ProfileRows::Ticks(3), 2, 26, 3),
            // Ten ticks over four rows need three ticks each, so over three rows.
            (1, ProfileRows::Count(4), 3, 35, 4),
            (1, ProfileRows::Count(100), 1, 107, 10),
        ] {
            let profiles = trade_profiles_with_rows(&build(feed), &ranges, rows);
            let profile = &profiles[0];
            assert_eq!(profile.group, group, "{rows:?} over {feed}-tick rows");
            assert_eq!(
                profile.volume.poc(),
                Some(poc),
                "{rows:?} over {feed}-tick rows"
            );
            assert_eq!(
                profile.volume.levels.len(),
                held,
                "{rows:?} over {feed}-tick rows"
            );
            assert_eq!(profile.volume.volume(), 14.0);
            assert_eq!(profile.developing_poc, [(start, poc)]);
        }
    }
}
