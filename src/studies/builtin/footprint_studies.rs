//! Levels and zones derived only from executed volume.

use crate::data::Footprint;
use chrono::{DateTime, Utc};

/// Meaning of a footprint annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootprintStudyKind {
    /// Consecutive sell imbalances.
    StackedBid,
    /// Consecutive buy imbalances.
    StackedAsk,
    /// Both sides traded at the bar's lowest or highest price.
    UnfinishedAuction,
    /// Highest-volume level, extended until a later bar trades through it.
    NakedPoc,
    /// Price with the largest absolute delta, ties resolved toward the lower price.
    DeltaPoc,
    /// Row volume exceeds the configured share of this bar's volume.
    VolumeCluster,
    /// Small positive executed volume at a bar extreme, below the host's size threshold.
    SinglePrint,
}

/// A level or zone beginning at one bar and optionally ending at its first revisit.
#[derive(Clone, Debug, PartialEq)]
pub struct FootprintStudy {
    /// Bar that created the level or zone.
    pub start: DateTime<Utc>,
    /// First later bar whose traded range overlaps the zone; `None` while untouched.
    /// Local markers use the originating bar as their end.
    pub end: Option<DateTime<Utc>>,
    /// Inclusive lower grouped price key.
    pub low: i64,
    /// Inclusive upper grouped price key.
    pub high: i64,
    /// Interpretation of the annotation.
    pub kind: FootprintStudyKind,
}

/// Finds diagonal stacked imbalances, unfinished extremes, naked POCs, delta POCs and clusters.
///
/// At least `stack.max(1)` adjacent imbalances make a zone. Missing price rows break a stack.
/// POCs and stacks end at the first subsequent bar whose executed range revisits them.
/// `single_print_max` marks extreme rows whose positive total volume is no larger than this
/// threshold in instrument volume units; zero disables it. This is an execution-size marker,
/// independent of single-period occupancy in time profiles.
/// `cluster_share` must be finite and in 0..=1; an invalid share disables clusters. Call after
/// trades change, not every paint: extending old zones scans subsequent bars.
pub fn footprint_studies(
    flow: &Footprint,
    ratio: f64,
    stack: usize,
    cluster_share: f64,
    single_print_max: f64,
) -> Vec<FootprintStudy> {
    let bars: Vec<_> = flow.bars().values().collect();
    let mut result = Vec::new();
    for (index, bar) in bars.iter().enumerate() {
        let mut add = |low: i64, high: i64, kind, extended| {
            let end = if extended {
                bars[index + 1..]
                    .iter()
                    .find(|next| {
                        next.levels
                            .first_key_value()
                            .zip(next.levels.last_key_value())
                            .is_some_and(|((first, _), (last, _))| *first <= high && *last >= low)
                    })
                    .map(|next| next.time)
            } else {
                Some(bar.time)
            };
            result.push(FootprintStudy {
                start: bar.time,
                end,
                low,
                high,
                kind,
            });
        };
        if let Some(poc) = bar.poc() {
            add(poc, poc, FootprintStudyKind::NakedPoc, true);
        }
        if let Some(row) = bar.levels.values().max_by(|a, b| {
            a.delta()
                .abs()
                .total_cmp(&b.delta().abs())
                .then_with(|| b.level.cmp(&a.level))
        }) {
            add(row.level, row.level, FootprintStudyKind::DeltaPoc, false);
        }
        let extremes = [bar.levels.first_key_value(), bar.levels.last_key_value()];
        for (i, extreme) in extremes.into_iter().enumerate() {
            if let Some((&key, row)) = extreme
                && (i == 0 || bar.levels.len() > 1)
            {
                if row.bid > 0.0 && row.ask > 0.0 {
                    add(key, key, FootprintStudyKind::UnfinishedAuction, false);
                }
                if single_print_max.is_finite()
                    && row.volume() > 0.0
                    && row.volume() <= single_print_max
                {
                    add(key, key, FootprintStudyKind::SinglePrint, false);
                }
            }
        }
        for row in bar.levels.values() {
            if cluster_share.is_finite()
                && (0.0..=1.0).contains(&cluster_share)
                && bar.volume() > 0.0
                && row.volume() / bar.volume() > cluster_share
            {
                add(
                    row.level,
                    row.level,
                    FootprintStudyKind::VolumeCluster,
                    false,
                );
            }
        }
        for (side, kind) in [
            (0, FootprintStudyKind::StackedBid),
            (1, FootprintStudyKind::StackedAsk),
        ] {
            let mut run: Option<(i64, i64, usize)> = None;
            for row in bar.levels.values() {
                let imbalance = bar.imbalance(row.level, ratio, true);
                let active = if side == 0 { imbalance.0 } else { imbalance.1 };
                if let Some((low, high, count)) = run
                    && (!active || high.checked_add(1) != Some(row.level))
                {
                    if count >= stack.max(1) {
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
                && count >= stack.max(1)
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
    use crate::model::{BarClock, Tick, Timeframe, TradeSide};
    use chrono::TimeDelta;

    #[test]
    fn adjacent_diagonals_extend_only_until_a_later_revisit() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (second, price, size, side) in [
            (0, 100.0, 1.0, TradeSide::Sell),
            (1, 101.0, 1.0, TradeSide::Sell),
            (2, 101.0, 4.0, TradeSide::Buy),
            (3, 102.0, 1.0, TradeSide::Sell),
            (4, 102.0, 4.0, TradeSide::Buy),
            (5, 103.0, 4.0, TradeSide::Buy),
            (60, 110.0, 1.0, TradeSide::Buy),
            (120, 102.0, 1.0, TradeSide::Sell),
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
        assert!(
            !footprint_studies(&flow, 3.0, 3, 1.0 / 3.0, 1.0)
                .iter()
                .any(|s| s.start == start && s.kind == FootprintStudyKind::VolumeCluster)
        );
        let studies = footprint_studies(&flow, 3.0, 3, 0.3, 1.0);
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
        assert!(
            !first
                .iter()
                .any(|s| s.kind == FootprintStudyKind::UnfinishedAuction)
        );
        assert_eq!(
            first
                .iter()
                .filter(|s| s.kind == FootprintStudyKind::VolumeCluster)
                .count(),
            2
        ); // 5/15 at 101 and 102.
        // Unknown volume does not make a two-sided auction; one price is only marked once.
        for (sides, unfinished) in [
            ([Some(TradeSide::Buy), Some(TradeSide::Sell)], 1),
            ([Some(TradeSide::Buy), Some(TradeSide::Buy)], 0),
            ([Some(TradeSide::Sell), None], 0),
        ] {
            let mut extreme = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
            for side in sides {
                extreme.push(&Tick::new(start, 100.0, 1.0, side)).unwrap();
            }
            assert_eq!(
                footprint_studies(&extreme, 3.0, 3, 0.3, 0.0)
                    .iter()
                    .filter(|s| s.kind == FootprintStudyKind::UnfinishedAuction)
                    .count(),
                unfinished
            );
        }
    }
}
