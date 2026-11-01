//! Same-side execution bursts and bars that sweep through diagonal imbalances.

use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};

use super::{DetectionKind, Detector};
use crate::data::Footprint;
use crate::model::{BarClock, Direction, OrderSide, Signal, SignalId, SignalShape, Tick};
use crate::services::SignalEvent;

/// A burst of at least five same-side fills, at most 20 ms apart, travelling at least three
/// ticks without reversing. If any fill identifies its aggressor, at least three distinct
/// aggressors are required. Unknown-side fills break a burst and never count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StopRunRule;

/// A closed bar with directional delta of at least 25%, three diagonal imbalances at 3:1,
/// a body at least 60% of its range, and volume at least 1.5 times the previous ten traded bars.
/// Unknown-side fills never count. The latest subscribed time interval supplies the bars.
/// The first retained bar is left out because its opening executions may be missing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StopSweepRule;

#[derive(Default)]
pub(super) struct StopRuns {
    burst: Option<Burst>,
    next: u64,
}

struct Burst {
    first: DateTime<Utc>,
    last: DateTime<Utc>,
    side: OrderSide,
    open: i64,
    price: i64,
    fills: u64,
    size: f64,
    // Only three distinct ids are needed, even for a session-long burst.
    ids: Vec<u64>,
    number: Option<u64>,
}

impl StopRuns {
    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        tick: f64,
        enabled: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        let Some(side) = fill.side.filter(|_| fill.size > 0.0) else {
            self.burst = None;
            return;
        };
        let price = (fill.price / tick).round() as i64;
        if self.burst.as_ref().is_none_or(|burst| {
            let gap = fill.ts - burst.last;
            side != burst.side
                || gap < TimeDelta::zero()
                || gap > TimeDelta::milliseconds(20)
                || match side {
                    OrderSide::Buy => price < burst.price,
                    OrderSide::Sell => price > burst.price,
                }
        }) {
            self.burst = Some(Burst {
                first: fill.ts,
                last: fill.ts,
                side,
                open: price,
                price,
                fills: 0,
                size: 0.0,
                ids: Vec::new(),
                number: None,
            });
        }
        let burst = self.burst.as_mut().unwrap();
        burst.last = fill.ts;
        burst.price = price;
        burst.fills = burst.fills.saturating_add(1);
        burst.size = (burst.size + fill.size).min(f64::MAX);
        if let Some(id) = fill.aggressor_order_id
            && burst.ids.len() < 3
            && !burst.ids.contains(&id)
        {
            burst.ids.push(id);
        }
        let ticks = burst.price.abs_diff(burst.open);
        if !enabled
            || burst.fills < 5
            || ticks < 3
            || (!burst.ids.is_empty() && burst.ids.len() < 3)
        {
            return;
        }
        let number = *burst.number.get_or_insert_with(|| {
            self.next += 1;
            self.next
        });
        out.push(SignalEvent::Upsert(Signal {
            id: SignalId(format!("stop-run-{number}")),
            shape: SignalShape::Marker {
                time: fill.ts,
                price: Some(fill.price),
                direction: direction(side),
            },
            category: DetectionKind::StopRun.category().into(),
            label: Some("Stop run".into()),
            tooltip: Some(format!(
                "{} fills, {ticks} ticks, size {}, {} ms",
                burst.fills,
                burst.size,
                (burst.last - burst.first).num_milliseconds()
            )),
        }));
    }
}

pub(super) fn direction(side: OrderSide) -> Direction {
    match side {
        OrderSide::Buy => Direction::Up,
        OrderSide::Sell => Direction::Down,
    }
}

pub(super) struct StopSweeps {
    tick: f64,
    trades: VecDeque<Tick>,
    clock: Option<BarClock>,
    footprint: Option<Footprint>,
    bars: BTreeMap<DateTime<Utc>, Prices>,
    closed: Option<DateTime<Utc>>,
    first: Option<DateTime<Utc>>,
    volumes: VecDeque<f64>,
}

struct Prices {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl StopSweeps {
    pub(super) fn new(tick: f64) -> Self {
        Self {
            tick,
            trades: VecDeque::new(),
            clock: None,
            footprint: None,
            bars: BTreeMap::new(),
            closed: None,
            first: None,
            volumes: VecDeque::new(),
        }
    }

    pub(super) fn set_clock(
        &mut self,
        clock: Option<BarClock>,
        settings: &Detector<StopSweepRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        self.clock = clock;
        self.footprint = self.clock.clone().map(|clock| {
            let mut footprint = Footprint::new(clock, self.tick, 1).unwrap();
            footprint.set_budget(12, 100_000);
            footprint
        });
        self.bars.clear();
        self.closed = None;
        self.first = None;
        self.volumes.clear();
        let trades = std::mem::take(&mut self.trades);
        for trade in &trades {
            self.aggregate(trade, settings.enabled, out);
        }
        self.trades = trades;
    }

    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        settings: &Detector<StopSweepRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        if fill.side.is_none()
            || fill.size <= 0.0
            || self.trades.back().is_some_and(|last| fill.ts < last.ts)
        {
            return;
        }
        self.aggregate(fill, settings.enabled, out);
        self.trades.push_back(fill.clone());
        if self.trades.len() > 100_000 {
            self.trades.pop_front();
        }
    }

    fn aggregate(&mut self, fill: &Tick, enabled: bool, out: &mut Vec<SignalEvent>) {
        self.advance(fill.ts, enabled, out);
        let (Some(clock), Some(footprint)) = (&self.clock, &mut self.footprint) else {
            return;
        };
        if !matches!(footprint.push(fill), Ok(true)) {
            return;
        }
        let open = clock.bar_open(fill.ts);
        self.first.get_or_insert(open);
        let bar = self.bars.entry(open).or_insert(Prices {
            open: fill.price,
            high: fill.price,
            low: fill.price,
            close: fill.price,
        });
        bar.high = bar.high.max(fill.price);
        bar.low = bar.low.min(fill.price);
        bar.close = fill.price;
        while self.bars.len() > 12 {
            self.bars.pop_first();
        }
    }

    pub(super) fn advance(
        &mut self,
        time: DateTime<Utc>,
        enabled: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        let (Some(clock), Some(footprint)) = (&self.clock, &self.footprint) else {
            return;
        };
        let open = clock.bar_open(time);
        for (&at, prices) in self.bars.range(..open) {
            if self.closed.is_some_and(|closed| at <= closed) {
                continue;
            }
            self.closed = Some(at);
            if self.first == Some(at) {
                continue;
            }
            let Some(bar) = footprint.bar_at(at) else {
                continue;
            };
            let volume = bar.volume();
            let buy = prices.close > prices.open;
            let delta = if buy { bar.delta() } else { -bar.delta() };
            let count = bar
                .levels
                .keys()
                .filter(|&&level| {
                    let (bid, ask) = bar.imbalance(level, 3.0, true);
                    if buy { ask } else { bid }
                })
                .count();
            if enabled
                && prices.high > prices.low
                && delta >= volume * 0.25
                && (prices.close - prices.open).abs() >= (prices.high - prices.low) * 0.6
                && count >= 3
                && self.volumes.len() == 10
                && volume >= 1.5 * self.volumes.iter().sum::<f64>() / 10.0
            {
                out.push(SignalEvent::Upsert(Signal {
                    id: SignalId(format!("stop-sweep-{}", at.timestamp_millis())),
                    shape: SignalShape::Marker {
                        time: at,
                        price: Some(prices.close),
                        direction: if buy { Direction::Up } else { Direction::Down },
                    },
                    category: DetectionKind::StopSweep.category().into(),
                    label: Some("Stop sweep".into()),
                    tooltip: Some(format!(
                        "Volume {volume}, delta {}, {count} diagonal imbalances",
                        bar.delta()
                    )),
                }));
            }
            self.volumes.push_back(volume);
            if self.volumes.len() > 10 {
                self.volumes.pop_front();
            }
        }
    }
}
