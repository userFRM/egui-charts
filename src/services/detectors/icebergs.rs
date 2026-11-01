//! Refills identified by order events, or estimated from executions against aggregated depth.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, TimeDelta, Utc};

use super::{
    DetectionKind, Detector,
    depth::{self, Depth, Price},
    stops::direction,
};
use crate::data::{OrderEvent, RestingOrder, meets_threshold};
use crate::model::{Signal, SignalId, SignalShape, Tick};
use crate::services::SignalEvent;

/// Refills of one displayed size at one price and side. An exact refill follows depletion
/// within 100 ms, or restores an executed order without preserving priority. Aggregate depth
/// estimates refills from executed volume divided by the opening displayed size, minus one.
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
    orders: HashMap<u64, (f64, f64)>,
    next: u64,
}

struct Episode {
    shown: f64,
    executed: f64,
    refills: u64,
    depleted: Option<DateTime<Utc>>,
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
            depleted: None,
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
            book.size(price) > 0.0
                || (book.exact
                    && episode
                        .depleted
                        .is_some_and(|at| time >= at && time - at <= TimeDelta::milliseconds(100)))
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
                .is_none_or(|at| time >= at && time - at <= TimeDelta::milliseconds(100))
        });
        match event {
            OrderEvent::Execute { id, size, .. } => {
                let Some(order) = before else {
                    return;
                };
                let (shown, executed) = self.orders.entry(*id).or_insert((order.size, 0.0));
                *executed = (*executed + size).min(f64::MAX);
                let price = depth::price(order.side, order.price, tick);
                let episode = self.levels.entry(price).or_insert(Episode {
                    shown: *shown,
                    executed: 0.0,
                    refills: 0,
                    depleted: None,
                    number: None,
                    reported: None,
                });
                episode.executed = (episode.executed + size).min(f64::MAX);
                if *size == order.size {
                    episode.depleted = Some(time);
                    self.orders.remove(id);
                }
            }
            OrderEvent::Add(order) => {
                let price = depth::price(order.side, order.price, tick);
                if let Some(episode) = self.levels.get_mut(&price) {
                    if episode.depleted.is_some() && episode.shown == order.size {
                        episode.depleted = None;
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
                    } else if episode.depleted.is_some() {
                        self.levels.remove(&price);
                    }
                }
                self.orders.insert(order.id, (order.size, 0.0));
            }
            OrderEvent::Modify {
                id,
                price,
                size: Some(size),
                keep_priority,
            } => {
                let Some(order) = before else {
                    return;
                };
                let at = depth::price(order.side, order.price, tick);
                let same_price = price.is_none_or(|p| depth::price(order.side, p, tick) == at);
                let (shown, executed) = self.orders.entry(*id).or_insert((order.size, 0.0));
                if same_price && !keep_priority && *size > order.size && *executed > 0.0 {
                    if let Some(episode) = self.levels.get_mut(&at) {
                        episode.shown = *size;
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
                    *shown = *size;
                    *executed = 0.0;
                } else if !same_price || *size < order.size {
                    self.levels.remove(&at);
                    self.orders.remove(id);
                }
            }
            OrderEvent::Cancel { id } => {
                if let Some(order) = before {
                    self.levels
                        .remove(&depth::price(order.side, order.price, tick));
                }
                self.orders.remove(id);
            }
            _ => {}
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
