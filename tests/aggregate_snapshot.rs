use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::data::{Footprint, FootprintBar, FootprintLevel};
use egui_charts::model::{BarClock, Tick, Timeframe};
use std::collections::BTreeMap;

fn start() -> DateTime<Utc> {
    "2026-01-05T10:00:00Z".parse().unwrap()
}

fn bar(time: DateTime<Utc>) -> FootprintBar {
    FootprintBar {
        time,
        levels: BTreeMap::from([(
            200,
            FootprintLevel {
                level: 200,
                bid: 3.0,
                ask: 8.0,
                unknown: 2.0,
                bid_trades: 2,
                ask_trades: 3,
                unknown_trades: 1,
            },
        )]),
        bid: 3.0,
        ask: 8.0,
        unknown: 2.0,
        min_delta: -2.0,
        max_delta: 6.0,
    }
}

fn snapshot(
    bars: impl IntoIterator<Item = FootprintBar>,
) -> Result<Footprint, egui_charts::data::FeedError> {
    Footprint::from_bars(BarClock::new(Timeframe::Min1), 0.25, 2, bars)
}

#[test]
fn aggregate_snapshot_preserves_volume_counts_and_delta_path() {
    let first = bar(start());
    let mut second = bar(start() + TimeDelta::minutes(1));
    (second.bid, second.ask, second.unknown) = (4.0, 1.0, 1.0);
    (second.min_delta, second.max_delta) = (-4.0, 0.0);
    let row = second.levels.get_mut(&200).unwrap();
    (row.bid, row.ask, row.unknown) = (4.0, 1.0, 1.0);
    (row.bid_trades, row.ask_trades, row.unknown_trades) = (2, 1, 1);
    let flow = snapshot([second.clone(), first.clone()]).unwrap();
    assert_eq!(
        flow.bars().values().cloned().collect::<Vec<_>>(),
        [first.clone(), second]
    );
    assert_eq!(flow.bar_at(start() + TimeDelta::seconds(59)), Some(&first));
    assert_eq!(
        (flow.tick_size(), flow.price_step(), flow.level_of(100.0)),
        (0.25, 0.5, Some(200))
    );
    let profile = flow.profile(start(), start() + TimeDelta::minutes(1));
    assert_eq!(
        (
            profile.bid,
            profile.ask,
            profile.unknown,
            profile.volume(),
            profile.trades()
        ),
        (7.0, 9.0, 3.0, 19.0, 10)
    );
    assert_eq!(
        (profile.delta(), profile.min_delta, profile.max_delta),
        (2.0, -2.0, 6.0)
    );
    assert_eq!(profile.average_size(), Some(1.9));
}

#[test]
fn aggregate_snapshot_replacement_removes_old_and_corrected_values() {
    let mut flow = snapshot([bar(start()), bar(start() + TimeDelta::minutes(1))]).unwrap();
    assert_eq!(flow.bars().len(), 2);
    let mut correction = bar(start() + TimeDelta::minutes(1));
    correction.unknown = 0.0;
    let row = correction.levels.get_mut(&200).unwrap();
    (row.unknown, row.unknown_trades) = (0.0, 0);
    let replacement = snapshot([correction.clone()]).unwrap();
    flow = replacement;
    assert!(flow.bar_at(start()).is_none());
    assert_eq!(flow.bars().len(), 1);
    assert_eq!(flow.bar_at(correction.time), Some(&correction));
    assert_eq!(flow.profile(start(), correction.time).trades(), 5);
}

#[test]
fn aggregate_snapshot_keeps_missing_bars_absent_and_known_empty_bars_present() {
    let missing = start() + TimeDelta::minutes(1);
    let flow = snapshot([bar(start()), bar(start() + TimeDelta::minutes(2))]).unwrap();
    assert!(flow.bar_at(missing).is_none());
    // This lower bound says nothing about the missing minute's coverage.
    assert_eq!(flow.complete_from(), Some(start()));
    let empty = FootprintBar {
        time: missing,
        levels: BTreeMap::new(),
        bid: 0.0,
        ask: 0.0,
        unknown: 0.0,
        min_delta: 0.0,
        max_delta: 0.0,
    };
    let known = snapshot([bar(start()), empty.clone()]).unwrap();
    assert_eq!(known.bar_at(missing), Some(&empty));
    assert_eq!(known.bar_at(missing).unwrap().trades(), 0);
    let empty = snapshot([]).unwrap();
    assert!(empty.bars().is_empty());
    assert_eq!(empty.complete_from(), None);
}

#[test]
fn aggregate_snapshot_rejects_trade_clocks_and_execution_operations() {
    for interval in ["3t", "5v", "2r", "5delta"] {
        assert!(
            Footprint::from_bars(BarClock::new(interval.parse().unwrap()), 0.25, 2, []).is_err(),
            "{interval}"
        );
    }
    let mut flow = snapshot([bar(start())]).unwrap();
    let before = flow.bars().clone();
    assert!(flow.set_size_filter(0.0, f64::MAX).is_err());
    assert!(
        flow.push(&Tick::new(
            start() + TimeDelta::minutes(1),
            100.0,
            1.0,
            None
        ))
        .is_err()
    );
    assert_eq!(flow.bars(), &before);
    let mut empty = snapshot([]).unwrap();
    assert!(empty.set_size_filter(0.0, f64::MAX).is_err());
    assert!(empty.push(&Tick::new(start(), 100.0, 1.0, None)).is_err());
}

#[test]
fn aggregate_snapshot_rejects_invalid_aggregates() {
    let mut cases: Vec<(&str, FootprintBar)> = Vec::new();
    let mut add = |name, change: fn(&mut FootprintBar)| {
        let mut invalid = bar(start());
        change(&mut invalid);
        cases.push((name, invalid));
    };
    add("unaligned open", |b| b.time += TimeDelta::seconds(1));
    add("key mismatch", |b| {
        b.levels.get_mut(&200).unwrap().level = 201
    });
    add("negative row", |b| {
        b.levels.get_mut(&200).unwrap().bid = -1.0
    });
    add("nonfinite row", |b| {
        b.levels.get_mut(&200).unwrap().ask = f64::NAN
    });
    add("missing count", |b| {
        b.levels.get_mut(&200).unwrap().unknown_trades = 0
    });
    add("count without volume", |b| {
        b.unknown = 0.0;
        b.levels.get_mut(&200).unwrap().unknown = 0.0;
    });
    add("total mismatch", |b| b.bid = 4.0);
    add("nonfinite total", |b| b.ask = f64::INFINITY);
    add("nonfinite extrema", |b| b.min_delta = f64::NAN);
    add("missing initial zero", |b| b.min_delta = 1.0);
    add("missing final delta", |b| b.max_delta = 4.0);
    add("extremum beyond side total", |b| b.min_delta = -4.0);
    add("impossible extrema span", |b| {
        b.min_delta = -3.0;
        b.max_delta = 8.0;
    });
    add("count overflow", |b| {
        b.levels.get_mut(&200).unwrap().bid_trades = u64::MAX
    });
    for (name, invalid) in cases {
        assert!(snapshot([invalid]).is_err(), "accepted {name}");
    }
    assert!(
        snapshot([bar(start()), bar(start())]).is_err(),
        "duplicate opens"
    );
    assert!(
        Footprint::from_bars(BarClock::new(Timeframe::Min1), f64::MAX, 1, [bar(start())]).is_err(),
        "unrepresentable row price"
    );
    assert!(Footprint::from_bars(BarClock::new(Timeframe::Min1), f64::NAN, 1, []).is_err());
    assert!(Footprint::from_bars(BarClock::new(Timeframe::Min1), 0.25, 0, []).is_err());
}

#[test]
fn aggregate_snapshot_tolerates_reconciliation_roundoff_but_not_disagreement() {
    let mut decimal = bar(start());
    (decimal.bid, decimal.ask, decimal.unknown) = (0.0, 0.3, 0.0);
    (decimal.min_delta, decimal.max_delta) = (0.0, 0.3);
    decimal.levels = [(200, 0.1), (201, 0.2)]
        .into_iter()
        .map(|(level, ask)| {
            (
                level,
                FootprintLevel {
                    level,
                    ask,
                    ask_trades: 1,
                    ..Default::default()
                },
            )
        })
        .collect();
    let flow = snapshot([decimal.clone()]).unwrap();
    assert_eq!(flow.bar_at(start()).unwrap().ask, 0.3);
    assert_eq!(flow.bar_at(start()).unwrap().trades(), 2);
    decimal.ask += 1e-6;
    assert!(snapshot([decimal]).is_err());
}

#[test]
fn aggregate_snapshot_rejects_combined_volume_or_count_overflow() {
    let large = |time, ask, ask_trades| FootprintBar {
        time,
        levels: BTreeMap::from([(
            200,
            FootprintLevel {
                level: 200,
                ask,
                ask_trades,
                ..Default::default()
            },
        )]),
        bid: 0.0,
        ask,
        unknown: 0.0,
        min_delta: 0.0,
        max_delta: ask,
    };
    assert!(
        snapshot([
            large(start(), f64::MAX * 0.75, 1),
            large(start() + TimeDelta::minutes(1), f64::MAX * 0.75, 1)
        ])
        .is_err()
    );
    assert!(
        snapshot([
            large(start(), 1.0, u64::MAX),
            large(start() + TimeDelta::minutes(1), 1.0, 1)
        ])
        .is_err()
    );
}

#[test]
fn aggregate_snapshot_rejects_an_untraded_price_row() {
    let mut empty = bar(start());
    (
        empty.bid,
        empty.ask,
        empty.unknown,
        empty.min_delta,
        empty.max_delta,
    ) = (0.0, 0.0, 0.0, 0.0, 0.0);
    empty.levels.insert(
        200,
        FootprintLevel {
            level: 200,
            ..Default::default()
        },
    );
    assert!(snapshot([empty]).is_err());
}

#[test]
fn aggregate_snapshot_rejects_overflow_inside_a_price_row() {
    let mut large = bar(start());
    (large.bid, large.ask, large.unknown) = (f64::MAX * 0.6, f64::MAX * 0.4, 0.0);
    (large.min_delta, large.max_delta) = (-large.bid, 0.0);
    let row = large.levels.get_mut(&200).unwrap();
    (row.bid, row.ask, row.unknown) = (large.bid * (1.0 + 5e-10), large.ask * (1.0 + 5e-10), 0.0);
    (row.bid_trades, row.ask_trades, row.unknown_trades) = (1, 1, 0);
    assert!(large.volume().is_finite());
    assert!(!large.levels[&200].volume().is_finite());
    assert!(snapshot([large]).is_err());
}
