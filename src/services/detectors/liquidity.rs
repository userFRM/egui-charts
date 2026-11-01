//! The lifetime of large resting levels and recently added individual orders.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, TimeDelta, Utc};

use super::{
    DetectionKind, Detector, Threshold,
    depth::{self, Depth, Price},
    stops::direction,
};
use crate::data::{OrderEvent, RestingOrder, meets_threshold};
use crate::model::{Signal, SignalId, SignalShape, Tick};
use crate::services::SignalEvent;

/// Follows levels within ten ticks of their side's inside, and size added within five ticks.
/// Automatic thresholds are four times the median positive displayed size within ten ticks
/// on both sides at the current book update. An even count uses the mean of the middle pair.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LiquidityRule {
    /// Size that opens a tracked level and identifies a large individual order.
    pub threshold: Threshold,
    /// Size added in one update that reports fresh liquidity.
    pub fresh_threshold: Threshold,
}

impl Default for LiquidityRule {
    fn default() -> Self {
        Self {
            threshold: Threshold::Auto,
            fresh_threshold: Threshold::Auto,
        }
    }
}

#[derive(Default)]
pub(super) struct Liquidity {
    levels: BTreeMap<Price, Level>,
    orders: HashMap<u64, Order>,
    next: u64,
}

struct Level {
    number: u64,
    from: DateTime<Utc>,
    peak: f64,
    size: f64,
    executed: f64,
    since_update: f64,
    withdrawn: f64,
    wall: bool,
    tested: bool,
    beyond: Option<DateTime<Utc>>,
    largest: Option<u64>,
}

struct Order {
    from: DateTime<Utc>,
    price: Price,
    size: f64,
    executed: f64,
}

impl Liquidity {
    pub(super) fn clear(&mut self, out: &mut Vec<SignalEvent>) {
        for level in self.levels.values() {
            out.push(SignalEvent::Remove(SignalId(format!(
                "liquidity-{}",
                level.number
            ))));
        }
        self.levels.clear();
        self.orders.clear();
    }

    pub(super) fn forget(&mut self, id: &SignalId) {
        if let Some(number) =
            id.0.strip_prefix("liquidity-")
                .and_then(|s| s.parse::<u64>().ok())
        {
            self.levels.retain(|_, level| level.number != number);
        }
    }

    fn threshold(rule: Threshold, book: &Depth) -> Option<f64> {
        match rule {
            Threshold::Fixed(size) => Some(size),
            Threshold::Auto => {
                let mut sizes: Vec<_> = book
                    .levels
                    .iter()
                    .filter(|(price, _)| book.near(**price, 10))
                    .map(|(_, level)| level.size)
                    .filter(|size| *size > 0.0)
                    .collect();
                if sizes.is_empty() {
                    return None;
                }
                sizes.sort_unstable_by(f64::total_cmp);
                let middle = sizes.len() / 2;
                let median = if sizes.len().is_multiple_of(2) {
                    sizes[middle - 1] / 2.0 + sizes[middle] / 2.0
                } else {
                    sizes[middle]
                };
                Some((median * 4.0).min(f64::MAX))
            }
        }
    }

    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        tick: f64,
        enabled: bool,
        book: &Depth,
        out: &mut Vec<SignalEvent>,
    ) {
        let passive = depth::resting(fill, tick);
        let traded = (fill.price / tick).round() as i64;
        let mut swept = Vec::new();
        for (&price, level) in &mut self.levels {
            if fill.ts < level.from {
                continue;
            }
            if !book.exact && passive == Some(price) {
                level.executed = (level.executed + fill.size).min(f64::MAX);
                level.since_update = (level.since_update + fill.size).min(f64::MAX);
                level.tested |= meets_threshold(level.executed, level.peak * 0.1)
                    && book.inside(price.0) == Some(price.1);
            }
            if depth::beyond(price, traded) {
                level.beyond = Some(fill.ts);
            }
            if enabled
                && !meets_threshold(level.size, level.peak * 0.2)
                && meets_threshold(level.executed, (level.peak - level.size) * 0.5)
                && depth::beyond(price, traded)
            {
                swept.push(price);
            }
        }
        for price in swept {
            let level = self.levels.remove(&price).unwrap();
            out.push(Self::zone(price, tick, &level, Some(fill.ts)));
            out.push(Self::marker(
                DetectionKind::LiquiditySweep,
                price,
                fill.ts,
                tick,
                &level,
            ));
        }
    }

    pub(super) fn update(
        &mut self,
        before: &Depth,
        book: &Depth,
        time: DateTime<Utc>,
        tick: f64,
        snapshot: bool,
        settings: &Detector<LiquidityRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        self.orders
            .retain(|_, order| time >= order.from && time - order.from <= TimeDelta::seconds(2));
        let threshold = Self::threshold(settings.rule.threshold, book);
        let fresh = Self::threshold(settings.rule.fresh_threshold, book);
        if !settings.enabled {
            self.clear(out);
            return;
        }
        let mut pulled = Vec::new();
        let mut swept = Vec::new();
        for (&price, level) in &mut self.levels {
            if time < level.from {
                continue;
            }
            let size = book.size(price);
            if !book.exact {
                level.withdrawn += (level.size - size - level.since_update).max(0.0);
            }
            level.since_update = 0.0;
            level.size = size;
            level.peak = level.peak.max(size);
            level.tested &= meets_threshold(level.executed, level.peak * 0.1);
            if let Some((id, _)) = book.levels.get(&price).and_then(|level| level.largest) {
                level.largest = Some(id);
            }
            if !level.wall
                && level.tested
                && meets_threshold(level.executed, level.peak * 0.1)
                && meets_threshold(size, level.peak * 0.5)
                && book
                    .inside(price.0)
                    .is_some_and(|inside| inside.abs_diff(price.1) >= 3)
            {
                level.wall = true;
                out.push(Self::marker(DetectionKind::Wall, price, time, tick, level));
            }
            if !meets_threshold(size, level.peak * 0.2) {
                if level.beyond.is_some()
                    && meets_threshold(level.executed, (level.peak - size) * 0.5)
                {
                    swept.push(price);
                } else if meets_threshold(level.withdrawn, level.peak * 0.8) {
                    pulled.push(price);
                }
            }
        }
        for price in swept {
            let level = self.levels.remove(&price).unwrap();
            out.push(Self::zone(price, tick, &level, Some(time)));
            out.push(Self::marker(
                DetectionKind::LiquiditySweep,
                price,
                time,
                tick,
                &level,
            ));
        }
        for price in pulled {
            let level = self.levels.remove(&price).unwrap();
            out.push(Self::zone(price, tick, &level, Some(time)));
            let mut marker =
                Self::marker(DetectionKind::PulledLiquidity, price, time, tick, &level);
            if let SignalEvent::Upsert(signal) = &mut marker
                && out.iter().any(|event| matches!(event, SignalEvent::Upsert(other)
                    if other.category == DetectionKind::Spoofing.category() && other.shape == signal.shape))
            {
                signal.label = None;
            }
            out.push(marker);
        }
        for (&price, shown) in &book.levels {
            let added = (shown.size - before.size(price)).max(0.0);
            if !snapshot
                && book.near(price, 5)
                && fresh.is_some_and(|threshold| meets_threshold(added, threshold))
            {
                self.next += 1;
                let level = Level {
                    number: self.next,
                    from: time,
                    peak: added,
                    size: shown.size,
                    executed: 0.0,
                    since_update: 0.0,
                    withdrawn: 0.0,
                    wall: false,
                    tested: false,
                    beyond: None,
                    largest: shown.largest.map(|(id, _)| id),
                };
                out.push(Self::marker(
                    DetectionKind::FreshLiquidity,
                    price,
                    time,
                    tick,
                    &level,
                ));
            }
            if book.near(price, 10)
                && threshold.is_some_and(|threshold| meets_threshold(shown.size, threshold))
                && !self.levels.contains_key(&price)
            {
                self.next += 1;
                let level = Level {
                    number: self.next,
                    from: time,
                    peak: shown.size,
                    size: shown.size,
                    executed: 0.0,
                    since_update: 0.0,
                    withdrawn: 0.0,
                    wall: false,
                    tested: false,
                    beyond: None,
                    largest: shown.largest.map(|(id, _)| id),
                };
                out.push(Self::zone(price, tick, &level, None));
                self.levels.insert(price, level);
            }
        }
    }

    pub(super) fn order(
        &mut self,
        event: &OrderEvent,
        before: Option<&RestingOrder>,
        book: &Depth,
        time: DateTime<Utc>,
        tick: f64,
        settings: &Detector<LiquidityRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        if !settings.enabled {
            return;
        }
        match event {
            OrderEvent::Add(order) => {
                let price = depth::price(order.side, order.price, tick);
                if book.near(price, 5)
                    && Self::threshold(settings.rule.threshold, book)
                        .is_some_and(|threshold| meets_threshold(order.size, threshold))
                {
                    self.orders.insert(
                        order.id,
                        Order {
                            from: time,
                            price,
                            size: order.size,
                            executed: 0.0,
                        },
                    );
                }
            }
            OrderEvent::Execute { id, size, .. } => {
                // Sequenced executions count even when the tape does not identify its aggressor.
                if let Some(order) = before {
                    let price = depth::price(order.side, order.price, tick);
                    if let Some(level) = self.levels.get_mut(&price) {
                        level.executed += size;
                        level.tested |= meets_threshold(level.executed, level.peak * 0.1)
                            && book.inside(price.0) == Some(price.1);
                    }
                }
                if let Some(order) = self.orders.get_mut(id) {
                    order.executed += size;
                }
            }
            OrderEvent::Cancel { id } | OrderEvent::Modify { id, .. } => {
                let Some(old) = before else {
                    return;
                };
                let price = depth::price(old.side, old.price, tick);
                let remaining = match event {
                    OrderEvent::Modify {
                        price: to, size, ..
                    } if to.is_none_or(|to| depth::price(old.side, to, tick) == price) => {
                        size.unwrap_or(old.size)
                    }
                    _ => 0.0,
                };
                let withdrawn = (old.size - remaining).max(0.0);
                if let Some(level) = self.levels.get_mut(&price) {
                    level.withdrawn += withdrawn;
                }
                if let Some(order) = self.orders.get(id)
                    && !meets_threshold(remaining, order.size * 0.1)
                {
                    if time >= order.from
                        && time - order.from <= TimeDelta::seconds(2)
                        && !meets_threshold(order.executed, order.size * 0.1)
                    {
                        self.next += 1;
                        let level = Level {
                            number: self.next,
                            from: order.from,
                            peak: order.size,
                            size: remaining,
                            executed: order.executed,
                            since_update: 0.0,
                            withdrawn,
                            wall: false,
                            tested: false,
                            beyond: None,
                            largest: Some(*id),
                        };
                        out.push(Self::marker(
                            DetectionKind::Spoofing,
                            order.price,
                            time,
                            tick,
                            &level,
                        ));
                    }
                    self.orders.remove(id);
                }
            }
        }
    }

    fn zone(price: Price, tick: f64, level: &Level, to: Option<DateTime<Utc>>) -> SignalEvent {
        SignalEvent::Upsert(Signal {
            id: SignalId(format!("liquidity-{}", level.number)),
            shape: SignalShape::PriceZone {
                from: level.from,
                to,
                price: price.1 as f64 * tick,
                direction: direction(depth::side(price)),
            },
            category: DetectionKind::LargeLiquidity.category().into(),
            label: None,
            tooltip: Some(Self::tooltip(level)),
        })
    }

    fn marker(
        kind: DetectionKind,
        price: Price,
        time: DateTime<Utc>,
        tick: f64,
        level: &Level,
    ) -> SignalEvent {
        SignalEvent::Upsert(Signal {
            id: SignalId(format!("{}-{}", kind.category(), level.number)),
            shape: SignalShape::Marker {
                time,
                price: Some(price.1 as f64 * tick),
                direction: direction(depth::side(price)),
            },
            category: kind.category().into(),
            label: (kind != DetectionKind::FreshLiquidity).then(|| kind.category().into()),
            tooltip: Some(Self::tooltip(level)),
        })
    }

    fn tooltip(level: &Level) -> String {
        format!(
            "Peak {}, executed {}, withdrawn {}{}",
            level.peak,
            level.executed,
            level.withdrawn,
            level
                .largest
                .map_or_else(String::new, |id| format!(", largest resting order {id}"))
        )
    }
}
