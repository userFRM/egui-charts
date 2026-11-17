//! Levels and zones derived only from executed volume.

use crate::data::{Footprint, FootprintBarView, FootprintView, meets_threshold};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Meaning of a footprint annotation. Sells trade on the bid and buys on the ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FootprintStudyKind {
    /// Consecutive sell imbalances.
    StackedBid,
    /// Consecutive buy imbalances.
    StackedAsk,
    /// Sell volume traded at the bar's highest price, so the auction there did not finish.
    UnfinishedHigh,
    /// Buy volume traded at the bar's lowest price, so the auction there did not finish.
    UnfinishedLow,
    /// Highest-volume level, extended until a later bar trades through it.
    NakedPoc,
    /// Price with the largest absolute delta, ties resolved toward the lower price. A bar without
    /// a nonzero row delta has none.
    DeltaPoc,
    /// Row volume exceeds the configured share of this bar's volume.
    VolumeCluster,
    /// Small positive executed volume at a bar extreme, below the host's size threshold.
    SinglePrint,
    /// The bar's positive delta reaches the configured share of its volume.
    DeltaAsk,
    /// The bar's negative delta reaches the configured share of its volume.
    DeltaBid,
    /// The bar's buy volume reaches the configured multiple of its sell volume.
    RatioAsk,
    /// The bar's sell volume reaches the configured multiple of its buy volume.
    RatioBid,
}

/// Where a level or zone stands against later trading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FootprintStudyState {
    /// A marker that stays inside its own bar.
    Local,
    /// No later bar has traded in the zone, and it has not expired.
    Unvisited,
    /// A later bar traded in the zone, ending it.
    Revisited,
    /// No later bar traded in the zone within the configured number of bars, ending it.
    Expired,
}

/// A level or zone beginning at one bar and optionally ending at its first revisit.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FootprintStudy {
    /// Bar that created the level or zone.
    pub start: DateTime<Utc>,
    /// Last bar of the level or zone: the first later bar whose traded range overlaps it, or the
    /// last of the configured number of bars when none did; `None` while untouched and
    /// unexpired. Local markers use the originating bar as their end.
    pub end: Option<DateTime<Utc>>,
    /// Inclusive lower grouped price key.
    pub low: i64,
    /// Inclusive upper grouped price key.
    pub high: i64,
    /// Interpretation of the annotation.
    pub kind: FootprintStudyKind,
    /// Whether later trading ended it, and how.
    pub state: FootprintStudyState,
}

/// What [`footprint_studies`] marks, and how. The default marks stacks of three diagonal
/// imbalances at 300%, unfinished highs and lows, naked and delta POCs, and leaves the rest off.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct FootprintStudyConfig {
    /// Mark consecutive imbalance zones.
    pub stacked_imbalances: bool,
    /// Extend the highest-volume row to its first revisit.
    pub naked_poc: bool,
    /// Mark the row with the largest absolute delta.
    pub delta_poc: bool,
    /// Extend unfinished highs and lows to their first revisit.
    pub unfinished_auctions: bool,
    /// Dominance multiple of an imbalance, normally 3 (300%).
    pub imbalance_ratio: f64,
    /// Inclusive minimum dominant-side volume of each imbalance in a stack, in instrument
    /// volume units after row grouping. Zero preserves all ratio signals; negative or
    /// non-finite, none. The chart uses its footprint config's minimum for displayed zones.
    pub imbalance_min_volume: f64,
    /// At least this many (and one) adjacent imbalances make a zone.
    pub stack: usize,
    /// Build imbalances from a side against the opposite side one price away, as the footprint
    /// shows them, rather than at the same price.
    pub diagonal: bool,
    /// Bars a level or zone extends past its own: it ends at its first revisit, or expires
    /// after this many later bars. `None` extends until a revisit or the last bar.
    pub extend_bars: Option<usize>,
    /// Row volume above this share of the bar volume, in 0..=1, marks a cluster. Out of range,
    /// not a number, or `None`, none.
    pub cluster_share: Option<f64>,
    /// Extreme rows whose positive total volume is no larger than this, in instrument volume
    /// units, are single prints. This is an execution-size marker, independent of
    /// single-period occupancy in time profiles. `None`, none.
    pub single_print_max: Option<f64>,
    /// Marks a bar whose absolute delta, as a percentage of its volume with unknown side
    /// included, reaches this value, at equality too. A bar without volume or with zero delta
    /// has none. Not a number or negative, or `None`, none.
    pub delta_percent: Option<f64>,
    /// Marks a bar whose buy volume reaches this multiple of its sell volume, or the reverse,
    /// at equality too. A side with volume against none meets any multiple, two empty sides
    /// none, and unknown side counts in neither. Below 1 or not a number, or `None`, none.
    pub ratio: Option<f64>,
    /// The delta POC needs at least this absolute net delta, and more than zero; below it the bar
    /// has none.
    pub min_delta: f64,
}

impl Default for FootprintStudyConfig {
    fn default() -> Self {
        Self {
            stacked_imbalances: true,
            naked_poc: true,
            delta_poc: true,
            unfinished_auctions: true,
            imbalance_ratio: 3.0,
            imbalance_min_volume: 0.0,
            stack: 3,
            diagonal: true,
            extend_bars: None,
            cluster_share: None,
            single_print_max: None,
            delta_percent: None,
            ratio: None,
            min_delta: 0.0,
        }
    }
}

/// Finds stacked imbalances, unfinished highs and lows, naked POCs, delta POCs, clusters, single
/// prints and bars whose delta percentage or bid/ask ratio reaches a threshold, from complete
/// execution bars. Outcomes that depend on a missing history join are withheld.
///
/// Missing price rows break a stack. Stacks, POCs and unfinished levels end at the first
/// subsequent bar whose executed range revisits them, or expire as `config.extend_bars` says;
/// their `state` tells which. Call after trades change, not every paint: extending old zones
/// scans subsequent bars.
pub fn footprint_studies(flow: &Footprint, config: &FootprintStudyConfig) -> Vec<FootprintStudy> {
    let flow = FootprintView::from(flow);
    let bars: Vec<_> = flow.complete_bars(..).map(|bar| (bar, 1)).collect();
    footprint_studies_on_rows(&bars, config, flow)
}

/// Each bar's row keys are in multiples of its display grouping; output keys use input rows.
pub(crate) fn footprint_studies_on_rows(
    bars: &[(FootprintBarView<'_>, i64)],
    config: &FootprintStudyConfig,
    flow: FootprintView<'_>,
) -> Vec<FootprintStudy> {
    let mut result = Vec::new();
    let mut asks = Vec::new();
    for (index, &(bar, group)) in bars.iter().enumerate() {
        let mut add = |low: i64, high: i64, kind, extended| {
            let low = low.saturating_mul(group);
            let high = high.saturating_mul(group).saturating_add(group - 1);
            let (end, state) = if extended {
                let later = bars[index + 1..]
                    .iter()
                    .take(config.extend_bars.unwrap_or(usize::MAX));
                let revisit = later.clone().find(|(next, next_group)| {
                    next.levels
                        .first_key_value()
                        .zip(next.levels.last_key_value())
                        .is_some_and(|((first, _), (last, _))| {
                            first.saturating_mul(*next_group) <= high
                                && last
                                    .saturating_mul(*next_group)
                                    .saturating_add(*next_group - 1)
                                    >= low
                        })
                });
                match (revisit, config.extend_bars) {
                    (Some((next, _)), _) => (Some(next.time), FootprintStudyState::Revisited),
                    (None, Some(n)) if later.count() == n => (
                        bars.get(index + n).map(|(last, _)| last.time),
                        FootprintStudyState::Expired,
                    ),
                    _ => (None, FootprintStudyState::Unvisited),
                }
            } else {
                (Some(bar.time), FootprintStudyState::Local)
            };
            result.push(FootprintStudy {
                start: bar.time,
                end,
                low,
                high,
                kind,
                state,
            });
        };
        let (Some((&first, first_row)), Some((&last, last_row))) =
            (bar.levels.first_key_value(), bar.levels.last_key_value())
        else {
            continue;
        };
        if config.naked_poc
            && let Some(poc) = bar.poc()
        {
            add(poc, poc, FootprintStudyKind::NakedPoc, true);
        }
        if config.delta_poc
            && let Some(row) = bar
                .levels
                .values()
                .max_by(|a, b| {
                    a.delta()
                        .abs()
                        .total_cmp(&b.delta().abs())
                        .then_with(|| b.level.cmp(&a.level))
                })
                .filter(|row| row.delta() != 0.0 && row.delta().abs() >= config.min_delta)
        {
            add(row.level, row.level, FootprintStudyKind::DeltaPoc, false);
        }
        if last_row.bid > 0.0 && config.unfinished_auctions {
            add(last, last, FootprintStudyKind::UnfinishedHigh, true);
        }
        if first_row.ask > 0.0 && config.unfinished_auctions {
            add(first, first, FootprintStudyKind::UnfinishedLow, true);
        }
        if let Some(max) = config.single_print_max.filter(|max| max.is_finite()) {
            for (i, (key, row)) in [(first, first_row), (last, last_row)]
                .into_iter()
                .enumerate()
            {
                if (i == 0 || first != last) && row.volume() > 0.0 && row.volume() <= max {
                    add(key, key, FootprintStudyKind::SinglePrint, false);
                }
            }
        }
        if let Some(share) = config
            .cluster_share
            .filter(|share| (0.0..=1.0).contains(share))
            && bar.volume() > 0.0
        {
            for row in bar.levels.values() {
                if row.volume() / bar.volume() > share {
                    add(
                        row.level,
                        row.level,
                        FootprintStudyKind::VolumeCluster,
                        false,
                    );
                }
            }
        }
        if let Some((percent, threshold)) = bar
            .delta_percent()
            .zip(config.delta_percent.filter(|t| *t >= 0.0))
            && percent != 0.0
            && meets_threshold(percent.abs(), threshold)
        {
            let kind = if percent > 0.0 {
                FootprintStudyKind::DeltaAsk
            } else {
                FootprintStudyKind::DeltaBid
            };
            add(first, last, kind, false);
        }
        if let Some(ratio) = config.ratio.filter(|r| *r >= 1.0) {
            let dominant = |a: f64, b: f64| !meets_threshold(b, a) && meets_threshold(a, b * ratio);
            if dominant(bar.ask, bar.bid) {
                add(first, last, FootprintStudyKind::RatioAsk, false);
            }
            if dominant(bar.bid, bar.ask) {
                add(first, last, FootprintStudyKind::RatioBid, false);
            }
        }
        if !config.stacked_imbalances {
            continue;
        }
        let mut runs: [Option<(i64, i64, usize)>; 2] = [None; 2];
        let mut finish = |side, (low, high, count): (i64, i64, usize)| {
            if count >= config.stack.max(1) {
                if side == 0 {
                    add(low, high, FootprintStudyKind::StackedBid, true);
                } else {
                    asks.push((low, high));
                }
            }
        };
        for row in bar.levels.values() {
            let (bid, ask) = bar.imbalance(
                row.level,
                config.imbalance_ratio,
                config.diagonal,
                config.imbalance_min_volume,
            );
            for (side, active) in [bid, ask].into_iter().enumerate() {
                if let Some((low, high, count)) = runs[side]
                    && (!active || high.checked_add(1) != Some(row.level))
                {
                    finish(side, (low, high, count));
                    runs[side] = None;
                }
                if active {
                    runs[side] = Some(match runs[side] {
                        Some((low, _, count)) => (low, row.level, count + 1),
                        None => (row.level, row.level, 1),
                    });
                }
            }
        }
        for (side, run) in runs.into_iter().enumerate() {
            if let Some(run) = run {
                finish(side, run);
            }
        }
        // Ask zones follow bid zones, as the two loops emitted them;
        // only the buffer's allocation outlives the bar.
        for (low, high) in asks.drain(..) {
            add(low, high, FootprintStudyKind::StackedAsk, true);
        }
    }
    result.retain(|study| {
        study.state == FootprintStudyState::Local || !flow.span_has_gap(study.start, study.end)
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use chrono::TimeDelta;

    #[cfg(feature = "widget")]
    #[test]
    fn missing_join_withholds_only_unproven_annotation_outcomes() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        for (price, expiry) in [(110.0, None), (100.0, None), (110.0, Some(1))] {
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
            for (minute, price) in [(0, 100.0), (1, 100.0), (4, price)] {
                flow.push(&Tick::new(
                    start + TimeDelta::minutes(minute),
                    price,
                    5.0,
                    Some(OrderSide::Buy),
                ))
                .unwrap();
            }
            let rows: Vec<_> = flow.complete_bars(..).map(|bar| (bar.into(), 1)).collect();
            let config = FootprintStudyConfig {
                extend_bars: expiry,
                ..Default::default()
            };
            let all = footprint_studies_on_rows(&rows, &config, (&flow).into());
            flow.add_missing(start + TimeDelta::minutes(2), start + TimeDelta::minutes(4));
            let certain = footprint_studies(&flow, &config);
            assert!(all.iter().any(|s| s.start == start + TimeDelta::minutes(1)
                && s.kind == FootprintStudyKind::NakedPoc));
            assert!(
                !certain
                    .iter()
                    .any(|s| s.start == start + TimeDelta::minutes(1)
                        && s.kind == FootprintStudyKind::NakedPoc)
            );
            assert!(
                certain
                    .iter()
                    .any(|s| s.start == start && s.state == FootprintStudyState::Revisited)
            );
            assert!(
                certain
                    .iter()
                    .any(|s| s.start == start + TimeDelta::minutes(1)
                        && s.state == FootprintStudyState::Local)
            );
            assert!(
                certain
                    .iter()
                    .any(|s| s.start == start + TimeDelta::minutes(4)
                        && s.kind == FootprintStudyKind::NakedPoc)
            );
        }
    }

    #[test]
    fn adjacent_diagonals_extend_only_until_a_later_revisit() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (second, price, size, side) in [
            (0, 100.0, 1.0, OrderSide::Sell),
            (1, 101.0, 1.0, OrderSide::Sell),
            (2, 101.0, 4.0, OrderSide::Buy),
            (3, 102.0, 1.0, OrderSide::Sell),
            (4, 102.0, 4.0, OrderSide::Buy),
            (5, 103.0, 4.0, OrderSide::Buy),
            (60, 110.0, 1.0, OrderSide::Buy),
            (120, 102.0, 1.0, OrderSide::Sell),
        ] {
            flow.push(&Tick::new(
                start + TimeDelta::seconds(second),
                price,
                size,
                Some(side),
            ))
            .unwrap();
        }
        // Exactly one third is not above the requested share.
        let config = |cluster_share| FootprintStudyConfig {
            cluster_share: Some(cluster_share),
            single_print_max: Some(1.0),
            ..Default::default()
        };
        assert!(
            !footprint_studies(&flow, &config(1.0 / 3.0))
                .iter()
                .any(|s| s.start == start && s.kind == FootprintStudyKind::VolumeCluster)
        );
        let studies = footprint_studies(&flow, &config(0.3));
        let first: Vec<_> = studies.iter().filter(|s| s.start == start).collect();
        let single = first
            .iter()
            .find(|s| s.kind == FootprintStudyKind::SinglePrint)
            .unwrap();
        assert_eq!(
            (single.low, single.high, single.end),
            (100, 100, Some(start))
        );
        let stack = first
            .iter()
            .find(|s| s.kind == FootprintStudyKind::StackedAsk)
            .unwrap();
        assert_eq!(
            (stack.low, stack.high, stack.end),
            (101, 103, Some(start + TimeDelta::minutes(2)))
        );
        let poc = first
            .iter()
            .find(|s| s.kind == FootprintStudyKind::NakedPoc)
            .unwrap();
        assert_eq!((poc.low, poc.end), (101, None)); // Tie at 101/102 takes 101; 102 does not touch it.
        assert_eq!(poc.state, FootprintStudyState::Unvisited);
        assert_eq!(stack.state, FootprintStudyState::Revisited);
        assert!(!first.iter().any(|s| matches!(
            s.kind,
            FootprintStudyKind::UnfinishedHigh | FootprintStudyKind::UnfinishedLow
        )));
        assert_eq!(
            first
                .iter()
                .filter(|s| s.kind == FootprintStudyKind::VolumeCluster)
                .count(),
            2
        ); // 5/15 at 101 and 102.
    }

    /// A footprint of the given executions, `(second, price, size, side)`, in 1-minute bars of
    /// 1.0 ticks, the first bar opening at 10:00. A bar is told apart by its second.
    fn flow(trades: &[(i64, f64, f64, Option<OrderSide>)]) -> Footprint {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for &(second, price, size, side) in trades {
            flow.push(&Tick::new(
                start + TimeDelta::seconds(second),
                price,
                size,
                side,
            ))
            .unwrap();
        }
        flow
    }

    /// The studies of one kind as `(low, high, minutes to the end bar, state)`.
    fn found(
        flow: &Footprint,
        config: &FootprintStudyConfig,
        kind: FootprintStudyKind,
    ) -> Vec<(i64, i64, Option<i64>, FootprintStudyState)> {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        footprint_studies(flow, config)
            .iter()
            .filter(|s| s.kind == kind && s.start == start)
            .map(|s| {
                let end = s.end.map(|end| (end - start).num_minutes());
                (s.low, s.high, end, s.state)
            })
            .collect()
    }

    #[test]
    fn stacks_keep_side_order_across_price_gaps() {
        use FootprintStudyKind::*;
        use FootprintStudyState::*;
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let mut trades = Vec::new();
        for level in (100..112).chain([113, 115]) {
            let ask = level < 103 || (106..109).contains(&level) || level > 111;
            trades.push((0, level as f64, if ask { 1.0 } else { 4.0 }, sell));
            trades.push((0, level as f64, if ask { 4.0 } else { 1.0 }, buy));
        }
        trades.extend([(0, 106.0, 7.0, None), (60, 102.0, 1.0, buy)]);
        let flow = flow(&trades);
        let owned: Vec<_> = flow.complete_bars(..).map(FootprintBarView::from).collect();
        let slices: Vec<Vec<_>> = owned
            .iter()
            .map(|bar| bar.levels.values().cloned().collect())
            .collect();
        for (diagonal, expected) in [
            (
                false,
                vec![
                    (StackedBid, 103, 105, None, Unvisited),
                    (StackedBid, 109, 111, None, Unvisited),
                    (StackedAsk, 100, 102, Some(1), Revisited),
                    (StackedAsk, 106, 108, None, Unvisited),
                ],
            ),
            (
                true,
                vec![
                    (StackedBid, 103, 104, None, Unvisited),
                    (StackedBid, 109, 110, None, Unvisited),
                    (StackedAsk, 101, 102, Some(1), Revisited),
                    (StackedAsk, 107, 108, None, Unvisited),
                ],
            ),
        ] {
            for shared in [false, true] {
                let bars: Vec<_> = owned
                    .iter()
                    .zip(&slices)
                    .map(|(&bar, rows)| {
                        let bar = if shared {
                            FootprintBarView {
                                levels: crate::data::FootprintRows::Slice(rows),
                                ..bar
                            }
                        } else {
                            bar
                        };
                        (bar, 1)
                    })
                    .collect();
                // Four against one must also pass exactly at the requested ratio and minimum.
                for ratio in [3.0, 4.0] {
                    let config = FootprintStudyConfig {
                        naked_poc: false,
                        delta_poc: false,
                        unfinished_auctions: false,
                        stack: 2,
                        diagonal,
                        imbalance_ratio: ratio,
                        imbalance_min_volume: 4.0,
                        ..Default::default()
                    };
                    let actual: Vec<_> = footprint_studies_on_rows(&bars, &config, (&flow).into())
                        .into_iter()
                        .map(|study| {
                            assert_eq!(study.start, owned[0].time);
                            let end = study.end.map(|end| (end - study.start).num_minutes());
                            (study.kind, study.low, study.high, end, study.state)
                        })
                        .collect();
                    assert_eq!(actual, expected, "diagonal={diagonal}, shared={shared}");
                }
            }
        }
    }

    #[test]
    fn annotation_switches_preserve_other_kinds_and_order() {
        use FootprintStudyKind::*;
        use FootprintStudyState::*;
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let flow = flow(&[
            (0, 100.0, 1.0, sell),
            (1, 100.0, 1.0, buy),
            (2, 101.0, 1.0, sell),
            (3, 101.0, 4.0, buy),
            (4, 102.0, 1.0, sell),
            (5, 102.0, 4.0, buy),
            (6, 103.0, 1.0, sell),
            (7, 103.0, 4.0, buy),
            (8, 103.0, 4.0, None),
            (60, 110.0, 1.0, buy),
            (120, 102.0, 1.0, sell),
        ]);
        let start = flow.bars().first_key_value().unwrap().0;
        for mask in 0..16 {
            let config = FootprintStudyConfig {
                naked_poc: mask & 1 != 0,
                delta_poc: mask & 2 != 0,
                unfinished_auctions: mask & 4 != 0,
                stacked_imbalances: mask & 8 != 0,
                single_print_max: Some(2.0),
                cluster_share: Some(0.3),
                delta_percent: Some(40.0),
                ratio: Some(3.0),
                ..Default::default()
            };
            let actual: Vec<_> = footprint_studies(&flow, &config)
                .into_iter()
                .filter(|s| s.start == *start)
                .map(|s| {
                    (
                        s.kind,
                        s.low,
                        s.high,
                        s.end.map(|end| (end - *start).num_minutes()),
                        s.state,
                    )
                })
                .collect();
            // Unknown volume makes 103 the volume POC; equal deltas at 101..103 pick 101.
            let expected: Vec<_> = [
                (mask & 1 != 0, NakedPoc, 103, 103, None, Unvisited),
                (mask & 2 != 0, DeltaPoc, 101, 101, Some(0), Local),
                (mask & 4 != 0, UnfinishedHigh, 103, 103, None, Unvisited),
                (mask & 4 != 0, UnfinishedLow, 100, 100, None, Unvisited),
                (true, SinglePrint, 100, 100, Some(0), Local),
                (true, VolumeCluster, 103, 103, Some(0), Local),
                (true, DeltaAsk, 100, 103, Some(0), Local),
                (true, RatioAsk, 100, 103, Some(0), Local),
                (mask & 8 != 0, StackedAsk, 101, 103, Some(2), Revisited),
            ]
            .into_iter()
            .filter(|row| row.0)
            .map(|(_, kind, low, high, end, state)| (kind, low, high, end, state))
            .collect();
            assert_eq!(actual, expected, "enabled mask {mask}");
        }
    }

    /// Guards the level rules against hand-worked bars: the thresholds include equality and count
    /// unknown volume only in the delta percentage's denominator, a side against an empty one
    /// meets any ratio, the delta POC needs its minimum and breaks ties toward the lower price,
    /// an unfinished high needs sell volume at the high and a low buy volume at the low even with
    /// the other side empty, unfinished levels end at their revisit or expiry with the state
    /// saying which, and horizontal and diagonal imbalances build different zones.
    #[test]
    fn thresholds_unfinished_levels_and_zones_follow_their_rules() {
        use FootprintStudyKind::*;
        use FootprintStudyState::*;
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let marked = |trades: &[_], config: &FootprintStudyConfig, kind| {
            !found(&flow(trades), config, kind).is_empty()
        };
        // Delta percentage: 3 buys and 1 sell make 50%; 4 unknown more make it 25%.
        let half = [(0, 100.0, 3.0, buy), (1, 100.0, 1.0, sell)];
        let diluted = [
            (0, 100.0, 3.0, buy),
            (1, 100.0, 1.0, sell),
            (2, 100.0, 4.0, None),
        ];
        let flat = [(0, 100.0, 2.0, buy), (1, 100.0, 2.0, sell)];
        let delta = |t: f64| FootprintStudyConfig {
            delta_percent: Some(t),
            ..Default::default()
        };
        assert!(marked(&half, &delta(50.0), DeltaAsk));
        assert!(!marked(&half, &delta(50.1), DeltaAsk));
        assert!(marked(&diluted, &delta(25.0), DeltaAsk));
        assert!(!marked(&diluted, &delta(26.0), DeltaAsk));
        assert!(marked(
            &[(0, 100.0, 3.0, sell), (1, 100.0, 1.0, buy)],
            &delta(50.0),
            DeltaBid
        ));
        assert!(!marked(&flat, &delta(0.0), DeltaAsk) && !marked(&flat, &delta(0.0), DeltaBid));
        // Ratio: 6 buys against 2 sells is exactly 3.
        let ratio = |r: f64| FootprintStudyConfig {
            ratio: Some(r),
            ..Default::default()
        };
        let six = [(0, 100.0, 6.0, buy), (1, 100.0, 2.0, sell)];
        assert!(marked(&six, &ratio(3.0), RatioAsk));
        assert!(!marked(&six, &ratio(3.5), RatioAsk));
        assert!(marked(&[(0, 100.0, 5.0, buy)], &ratio(1000.0), RatioAsk));
        assert!(marked(
            &[(0, 100.0, 2.0, buy), (1, 100.0, 6.0, sell)],
            &ratio(3.0),
            RatioBid
        ));
        assert!(!marked(&[(0, 100.0, 5.0, None)], &ratio(1.0), RatioAsk));
        assert!(!marked(&flat, &ratio(1.0), RatioAsk) && !marked(&flat, &ratio(1.0), RatioBid));
        // Sells of 0.1 and 0.2 sum to 0.30000000000000004 and balance a buy of 0.3.
        let rounded = [
            (0, 100.0, 0.1, sell),
            (1, 100.0, 0.2, sell),
            (2, 100.0, 0.3, buy),
        ];
        for kind in [DeltaAsk, DeltaBid] {
            assert!(!marked(&rounded, &delta(0.0), kind), "{kind:?}");
        }
        for kind in [RatioAsk, RatioBid] {
            assert!(!marked(&rounded, &ratio(1.0), kind), "{kind:?}");
        }
        // 29 of 100 is 29%, 33 against 30 and 55 against 50 are 1.1, also with fractional sizes.
        for scale in [1.0, 0.1] {
            for (side, other, delta_kind, ratio_kind) in [
                (buy, sell, DeltaAsk, RatioAsk),
                (sell, buy, DeltaBid, RatioBid),
            ] {
                let pair =
                    |a: f64, b: f64| [(0, 100.0, a * scale, side), (1, 100.0, b * scale, other)];
                let imbalance = |a, b| {
                    let level = flow(&pair(a, b));
                    level
                        .bars()
                        .values()
                        .next()
                        .unwrap()
                        .imbalance(100, 1.1, false, 0.0)
                };
                let share = [
                    (0, 100.0, 29.0 * scale, side),
                    (1, 100.0, 71.0 * scale, None),
                ];
                assert!(marked(&share, &delta(29.0), delta_kind), "scale {scale}");
                assert_eq!(imbalance(33.0, 30.0), (side == sell, side == buy));
                assert_eq!(imbalance(32.0, 30.0), (false, false));
                assert!(marked(&pair(55.0, 50.0), &ratio(1.1), ratio_kind));
                assert!(!marked(&pair(54.0, 50.0), &ratio(1.1), ratio_kind));
            }
        }
        // Sizes summed trade by trade: 209 buys against 190 sells of 0.01 are 1.1, and 129 buys
        // against 71 sells of 0.1 are a delta of 29%.
        let lots = |buys: usize, sells: usize, size: f64| {
            let mut trades = vec![(0, 100.0, size, buy); buys];
            trades.extend(vec![(0, 100.0, size, sell); sells]);
            trades
        };
        let accumulated = lots(209, 190, 0.01);
        assert_eq!(
            flow(&accumulated)
                .bars()
                .values()
                .next()
                .unwrap()
                .imbalance(100, 1.1, false, 0.0),
            (false, true)
        );
        assert!(marked(&accumulated, &ratio(1.1), RatioAsk));
        for (minimum, expected) in [(2.09, true), (2.1, false), (-1.0, false)] {
            let config = FootprintStudyConfig {
                diagonal: false,
                stack: 1,
                imbalance_ratio: 1.1,
                imbalance_min_volume: minimum,
                ..Default::default()
            };
            assert_eq!(
                marked(&accumulated, &config, StackedAsk),
                expected,
                "minimum {minimum}"
            );
        }
        assert!(!marked(&lots(208, 190, 0.01), &ratio(1.1), RatioAsk));
        assert!(marked(&lots(129, 71, 0.1), &delta(29.0), DeltaAsk));
        // Delta POC: 100 nets +2 and 101 nets -3; at a tie of 3 the lower price wins.
        let rows = [(0, 100.0, 2.0, buy), (1, 101.0, 3.0, sell)];
        let tied = [(0, 100.0, 3.0, buy), (1, 101.0, 3.0, sell)];
        let poc = |min_delta: f64| FootprintStudyConfig {
            min_delta,
            ..Default::default()
        };
        assert_eq!(
            found(&flow(&rows), &poc(3.0), DeltaPoc),
            [(101, 101, Some(0), Local)]
        );
        assert!(found(&flow(&rows), &poc(3.5), DeltaPoc).is_empty());
        assert_eq!(
            found(&flow(&tied), &poc(0.0), DeltaPoc),
            [(100, 100, Some(0), Local)]
        );
        // Unfinished extremes: a one-sided extreme counts, an unknown-only one does not.
        for (trades, high, low) in [
            (vec![(0, 100.0, 1.0, buy), (1, 100.0, 1.0, sell)], 1, 1),
            (vec![(0, 100.0, 1.0, buy)], 0, 1),
            (vec![(0, 100.0, 1.0, sell), (1, 100.0, 1.0, None)], 1, 0),
            (vec![(0, 100.0, 1.0, None)], 0, 0),
            (vec![(0, 100.0, 1.0, sell), (1, 101.0, 1.0, buy)], 0, 0),
        ] {
            let flow = flow(&trades);
            let config = FootprintStudyConfig::default();
            assert_eq!(
                found(&flow, &config, UnfinishedHigh).len(),
                high,
                "{trades:?}"
            );
            assert_eq!(
                found(&flow, &config, UnfinishedLow).len(),
                low,
                "{trades:?}"
            );
        }
        // Bar 0 trades 100 (buys) and 101 (both): an unfinished low at 100 and high at 101.
        // Bars 1 and 2 stay at 103; bar 3 revisits 101 only.
        let tape = [
            (0, 100.0, 1.0, buy),
            (1, 101.0, 1.0, sell),
            (2, 101.0, 1.0, buy),
            (60, 103.0, 1.0, buy),
            (120, 103.0, 1.0, buy),
            (180, 101.0, 1.0, buy),
        ];
        for (extend_bars, high, low) in [
            (None, (Some(3), Revisited), (None, Unvisited)),
            (Some(0), (Some(0), Expired), (Some(0), Expired)),
            (Some(2), (Some(2), Expired), (Some(2), Expired)),
            (Some(3), (Some(3), Revisited), (Some(3), Expired)),
            (Some(5), (Some(3), Revisited), (None, Unvisited)),
        ] {
            let flow = flow(&tape);
            let config = FootprintStudyConfig {
                extend_bars,
                ..Default::default()
            };
            assert_eq!(
                found(&flow, &config, UnfinishedHigh),
                [(101, 101, high.0, high.1)],
                "high, {extend_bars:?}"
            );
            assert_eq!(
                found(&flow, &config, UnfinishedLow),
                [(100, 100, low.0, low.1)],
                "low, {extend_bars:?}"
            );
        }
        // Three rows each with 6 sells against 1 buy: a stack when compared at one price, but
        // diagonally the top row has no row above it, so only two imbalances run.
        let wall: Vec<_> = (0..3)
            .flat_map(|i| {
                let price = 100.0 + i as f64;
                [(2 * i, price, 6.0, sell), (2 * i + 1, price, 1.0, buy)]
            })
            .collect();
        let diagonal = |diagonal| FootprintStudyConfig {
            diagonal,
            ..Default::default()
        };
        assert_eq!(
            found(&flow(&wall), &diagonal(false), StackedBid),
            [(100, 102, None, Unvisited)]
        );
        assert!(found(&flow(&wall), &diagonal(true), StackedBid).is_empty());
        // Dominant-side minimum: 9/1 passes the ratio but not a floor of 10; 12/4 passes both.
        // Unknown volume and the opposite side cannot make up the minimum.
        for (side, other, kind) in [(buy, sell, StackedAsk), (sell, buy, StackedBid)] {
            for diagonal in [false, true] {
                let opposite_price = if !diagonal {
                    101.0
                } else if side == buy {
                    100.0
                } else {
                    102.0
                };
                for (dominant, opposite, minimum, expected) in [
                    (9.0, 1.0, 0.0, true),
                    (12.0, 4.0, 0.0, true),
                    (9.0, 1.0, 10.0, false),
                    (12.0, 4.0, 10.0, true),
                    (10.0, 1.0, 10.0, true),
                    (10.0, 4.0, 10.0, false),
                ] {
                    let trades = [
                        (0, opposite_price, opposite, other),
                        (0, opposite_price, 100.0, None),
                        (1, 101.0, dominant, side),
                        (2, 101.0, 100.0, None),
                    ];
                    let config = FootprintStudyConfig {
                        diagonal,
                        stack: 1,
                        imbalance_min_volume: minimum,
                        ..Default::default()
                    };
                    assert_eq!(
                        marked(&trades, &config, kind),
                        expected,
                        "{side:?}, diagonal {diagonal}, {dominant}/{opposite}, minimum {minimum}"
                    );
                }
            }
        }
        // Asks 12, 9, 15 against bids of 1 below form a stack at 101..103 until the middle
        // row falls below the minimum.
        let thin = [
            (0, 100.0, 1.0, sell),
            (1, 101.0, 1.0, sell),
            (2, 102.0, 1.0, sell),
            (3, 101.0, 12.0, buy),
            (4, 102.0, 9.0, buy),
            (5, 103.0, 15.0, buy),
        ];
        for (minimum, expected) in [(0.0, true), (10.0, false)] {
            let config = FootprintStudyConfig {
                imbalance_min_volume: minimum,
                ..Default::default()
            };
            let zones = found(&flow(&thin), &config, StackedAsk);
            assert_eq!(zones.len(), usize::from(expected), "minimum {minimum}");
            if expected {
                assert_eq!(zones, [(101, 103, None, Unvisited)]);
            }
        }
    }

    /// A bar whose rows carry no net delta, because their side is unknown, their sides balance or
    /// they differ only by the rounding of their sums, has no delta POC with the default minimum.
    #[test]
    fn delta_poc_needs_a_row_with_net_delta() {
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let unknown = [(0, 100.0, 2.0, None), (1, 101.0, 3.0, None)];
        let balanced = [
            (0, 100.0, 2.0, buy),
            (1, 100.0, 2.0, sell),
            (2, 101.0, 1.0, buy),
            (3, 101.0, 1.0, sell),
        ];
        // Sells of 0.1 and 0.2 sum to 0.30000000000000004 against a buy of 0.3.
        let rounded = [
            (0, 100.0, 0.1, sell),
            (1, 100.0, 0.2, sell),
            (2, 100.0, 0.3, buy),
        ];
        for (case, trades) in [
            ("unknown side", &unknown[..]),
            ("balanced", &balanced[..]),
            ("rounding", &rounded[..]),
        ] {
            assert!(
                found(
                    &flow(trades),
                    &FootprintStudyConfig::default(),
                    FootprintStudyKind::DeltaPoc
                )
                .is_empty(),
                "{case}"
            );
        }
    }
}
