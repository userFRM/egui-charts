//! Levels and zones derived only from executed volume.

use crate::data::Footprint;
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
    /// Price with the largest absolute delta, ties resolved toward the lower price.
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
    /// Dominance multiple of an imbalance, normally 3 (300%).
    pub imbalance_ratio: f64,
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
    /// The delta POC needs at least this absolute net delta; below it the bar has none.
    pub min_delta: f64,
}

impl Default for FootprintStudyConfig {
    fn default() -> Self {
        Self {
            imbalance_ratio: 3.0,
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
/// prints and bars whose delta percentage or bid/ask ratio reaches a threshold.
///
/// Missing price rows break a stack. Stacks, POCs and unfinished levels end at the first
/// subsequent bar whose executed range revisits them, or expire as `config.extend_bars` says;
/// their `state` tells which. Call after trades change, not every paint: extending old zones
/// scans subsequent bars.
pub fn footprint_studies(flow: &Footprint, config: &FootprintStudyConfig) -> Vec<FootprintStudy> {
    let bars: Vec<_> = flow.bars().values().collect();
    let mut result = Vec::new();
    for (index, bar) in bars.iter().enumerate() {
        let mut add = |low: i64, high: i64, kind, extended| {
            let (end, state) = if extended {
                let later = bars[index + 1..]
                    .iter()
                    .take(config.extend_bars.unwrap_or(usize::MAX));
                let revisit = later.clone().find(|next| {
                    next.levels
                        .first_key_value()
                        .zip(next.levels.last_key_value())
                        .is_some_and(|((first, _), (last, _))| *first <= high && *last >= low)
                });
                match (revisit, config.extend_bars) {
                    (Some(next), _) => (Some(next.time), FootprintStudyState::Revisited),
                    (None, Some(n)) if later.count() == n => (
                        bars.get(index + n).map(|last| last.time),
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
        if let Some(poc) = bar.poc() {
            add(poc, poc, FootprintStudyKind::NakedPoc, true);
        }
        if let Some(row) = bar
            .levels
            .values()
            .max_by(|a, b| {
                a.delta()
                    .abs()
                    .total_cmp(&b.delta().abs())
                    .then_with(|| b.level.cmp(&a.level))
            })
            .filter(|row| row.delta().abs() >= config.min_delta)
        {
            add(row.level, row.level, FootprintStudyKind::DeltaPoc, false);
        }
        if last_row.bid > 0.0 {
            add(last, last, FootprintStudyKind::UnfinishedHigh, true);
        }
        if first_row.ask > 0.0 {
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
            && percent.abs() >= threshold
        {
            let kind = if percent > 0.0 {
                FootprintStudyKind::DeltaAsk
            } else {
                FootprintStudyKind::DeltaBid
            };
            add(first, last, kind, false);
        }
        if let Some(ratio) = config.ratio.filter(|r| *r >= 1.0) {
            let dominant = |a: f64, b: f64| a > b && a >= b * ratio;
            if dominant(bar.ask, bar.bid) {
                add(first, last, FootprintStudyKind::RatioAsk, false);
            }
            if dominant(bar.bid, bar.ask) {
                add(first, last, FootprintStudyKind::RatioBid, false);
            }
        }
        for (side, kind) in [
            (0, FootprintStudyKind::StackedBid),
            (1, FootprintStudyKind::StackedAsk),
        ] {
            let mut run: Option<(i64, i64, usize)> = None;
            for row in bar.levels.values() {
                let imbalance = bar.imbalance(row.level, config.imbalance_ratio, config.diagonal);
                let active = if side == 0 { imbalance.0 } else { imbalance.1 };
                if let Some((low, high, count)) = run
                    && (!active || high.checked_add(1) != Some(row.level))
                {
                    if count >= config.stack.max(1) {
                        add(low, high, kind, true);
                    }
                    run = None;
                }
                if active {
                    run = Some(match run {
                        Some((low, _, count)) => (low, row.level, count + 1),
                        None => (row.level, row.level, 1),
                    });
                }
            }
            if let Some((low, high, count)) = run
                && count >= config.stack.max(1)
            {
                add(low, high, kind, true);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use chrono::TimeDelta;

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
    }
}
