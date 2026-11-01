//! Refills identified by order events, or estimated from executions against aggregated depth.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};

use super::{
    DetectionKind, Detector,
    depth::{self, Depth, Price},
    stops::direction,
};
use crate::data::{OrderEvent, RestingOrder, meets_threshold};
use crate::model::{Signal, SignalId, SignalShape, Tick};
use crate::services::SignalEvent;

/// Order-level refills at one price and side. An exact refill follows depletion
/// within 100 ms, or replenishes an executed order within 100 ms without preserving priority.
/// Aggregate depth estimates refills from executed volume divided by the opening displayed size,
/// minus one.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct IcebergRule {
    /// Refills needed before reporting. Must be at least one.
    pub refills: u32,
}

impl Default for IcebergRule {
    fn default() -> Self {
        Self { refills: 2 }
    }
}

#[derive(Default)]
pub(super) struct Icebergs {
    levels: BTreeMap<Price, Episode>,
    orders: HashMap<u64, (f64, f64, DateTime<Utc>)>,
    next: u64,
}

struct Episode {
    shown: f64,
    executed: f64,
    refills: u64,
    depleted: VecDeque<(DateTime<Utc>, f64)>,
    active: HashSet<u64>,
    number: Option<u64>,
    reported: Option<u64>,
}

impl Icebergs {
    pub(super) fn clear(&mut self) {
        self.levels.clear();
        self.orders.clear();
    }

    pub(super) fn trade(&mut self, fill: &Tick, tick: f64, book: &Depth) {
        if book.exact {
            return;
        }
        let Some(price) = depth::resting(fill, tick) else {
            self.levels.clear();
            return;
        };
        // A run is consecutive at the inside on one passive side.
        self.levels.retain(|&at, _| at == price);
        if fill.size <= 0.0 || book.inside(price.0) != Some(price.1) || book.size(price) <= 0.0 {
            self.levels.clear();
            return;
        }
        let episode = self.levels.entry(price).or_insert(Episode {
            shown: book.size(price),
            executed: 0.0,
            refills: 0,
            depleted: VecDeque::new(),
            active: HashSet::new(),
            number: None,
            reported: None,
        });
        episode.executed = (episode.executed + fill.size).min(f64::MAX);
        let sizes = episode.executed / episode.shown;
        let whole = if meets_threshold(sizes, sizes.round()) {
            sizes.round()
        } else {
            sizes.floor()
        };
        episode.refills = (whole as u64).saturating_sub(1);
    }

    pub(super) fn update(
        &mut self,
        book: &Depth,
        time: DateTime<Utc>,
        tick: f64,
        settings: &Detector<IcebergRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        self.levels.retain(|&price, episode| {
            if book.exact {
                !episode.active.is_empty() || !episode.depleted.is_empty()
            } else {
                book.size(price) > 0.0
            }
        });
        if !book.exact {
            for (&price, episode) in &mut self.levels {
                Self::report(
                    &mut self.next,
                    price,
                    time,
                    tick,
                    episode,
                    settings,
                    true,
                    out,
                );
            }
        }
    }

    pub(super) fn order(
        &mut self,
        event: &OrderEvent,
        before: Option<&RestingOrder>,
        time: DateTime<Utc>,
        tick: f64,
        settings: &Detector<IcebergRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        self.levels.retain(|_, episode| {
            episode
                .depleted
                .retain(|(at, _)| time >= *at && time - *at <= TimeDelta::milliseconds(100));
            !episode.active.is_empty() || !episode.depleted.is_empty()
        });
        match event {
            OrderEvent::Execute { id, size, .. } => {
                let Some(order) = before else {
                    return;
                };
                let (shown, executed, executed_at) =
                    self.orders.entry(*id).or_insert((order.size, 0.0, time));
                *executed = (*executed + size).min(f64::MAX);
                *executed_at = time;
                let price = depth::price(order.side, order.price, tick);
                let episode = self.levels.entry(price).or_insert(Episode {
                    shown: *shown,
                    executed: 0.0,
                    refills: 0,
                    depleted: VecDeque::new(),
                    active: HashSet::new(),
                    number: None,
                    reported: None,
                });
                episode.executed = (episode.executed + size).min(f64::MAX);
                episode.active.insert(*id);
                if meets_threshold(*size, order.size) {
                    episode.depleted.push_back((time, *shown));
                    episode.active.remove(id);
                    self.orders.remove(id);
                }
            }
            OrderEvent::Add(order) => {
                let price = depth::price(order.side, order.price, tick);
                if let Some(episode) = self.levels.get_mut(&price)
                    && let Some(at) = episode.depleted.iter().position(|(_, shown)| {
                        meets_threshold(*shown, order.size) && meets_threshold(order.size, *shown)
                    })
                {
                    episode.depleted.remove(at);
                    episode.active.insert(order.id);
                    episode.shown = order.size;
                    episode.refills += 1;
                    Self::report(
                        &mut self.next,
                        price,
                        time,
                        tick,
                        episode,
                        settings,
                        false,
                        out,
                    );
                }
                self.orders.insert(order.id, (order.size, 0.0, time));
            }
            OrderEvent::Modify {
                id,
                price,
                size,
                keep_priority,
            } => {
                let Some(order) = before else {
                    return;
                };
                let at = depth::price(order.side, order.price, tick);
                let same_price = price.is_none_or(|p| depth::price(order.side, p, tick) == at);
                let size = size.unwrap_or(order.size);
                if !same_price || size == 0.0 {
                    if let Some(episode) = self.levels.get_mut(&at) {
                        episode.active.remove(id);
                    }
                    self.orders.remove(id);
                    return;
                }
                let (shown, executed, executed_at) =
                    self.orders.entry(*id).or_insert((order.size, 0.0, time));
                if !keep_priority
                    && size > order.size
                    && *executed > 0.0
                    && time >= *executed_at
                    && time - *executed_at <= TimeDelta::milliseconds(100)
                {
                    if let Some(episode) = self.levels.get_mut(&at) {
                        episode.shown = size;
                        episode.refills += 1;
                        Self::report(
                            &mut self.next,
                            at,
                            time,
                            tick,
                            episode,
                            settings,
                            false,
                            out,
                        );
                    }
                    *executed = 0.0;
                }
                if size != order.size {
                    *shown = size;
                }
            }
            OrderEvent::Cancel { id } => {
                if let Some(order) = before
                    && let Some(episode) =
                        self.levels
                            .get_mut(&depth::price(order.side, order.price, tick))
                {
                    episode.active.remove(id);
                }
                self.orders.remove(id);
            }
        }
    }

    fn report(
        next: &mut u64,
        price: Price,
        time: DateTime<Utc>,
        tick: f64,
        episode: &mut Episode,
        settings: &Detector<IcebergRule>,
        estimate: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        if !settings.enabled
            || episode.refills < u64::from(settings.rule.refills)
            || episode.reported == Some(episode.refills)
        {
            return;
        }
        episode.reported = Some(episode.refills);
        let number = *episode.number.get_or_insert_with(|| {
            *next += 1;
            *next
        });
        out.push(SignalEvent::Upsert(Signal {
            id: SignalId(format!("iceberg-{number}")),
            shape: SignalShape::Marker {
                time,
                price: Some(price.1 as f64 * tick),
                direction: direction(depth::side(price)),
            },
            category: DetectionKind::Iceberg.category().into(),
            label: None,
            tooltip: Some(format!(
                "{} refills, executed {}, shown {}{}",
                episode.refills,
                episode.executed,
                episode.shown,
                if estimate {
                    "; estimate from aggregated depth"
                } else {
                    "; order-level refills"
                }
            )),
        }));
    }
}
