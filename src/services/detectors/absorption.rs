//! Aggressive volume executed into a resting inside level that holds.

use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};

use super::{
    DetectionKind, Detector, Threshold,
    depth::{self, Depth, Price},
    stops::direction,
};
use crate::data::meets_threshold;
use crate::model::{Signal, SignalId, SignalShape, Tick};
use crate::services::SignalEvent;

/// An inside-price episode closes after one second without an execution there. It qualifies
/// when aggressive volume reaches both the threshold and opening displayed size, no trade
/// printed beyond it, and half the displayed size remains or order-level depth received adds.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AbsorptionRule {
    /// Fixed size, or nearest-rank 99th percentile of the last 1,000 closed episode volumes,
    /// inactive until 100 episodes have closed. Gaps discard episodes without calibrating.
    pub threshold: Threshold,
}

impl Default for AbsorptionRule {
    fn default() -> Self {
        Self {
            threshold: Threshold::Auto,
        }
    }
}

#[derive(Default)]
pub(super) struct Absorption {
    episodes: BTreeMap<Price, Episode>,
    volumes: VecDeque<f64>,
    next: u64,
}

struct Episode {
    first: DateTime<Utc>,
    last: DateTime<Utc>,
    shown: f64,
    volume: f64,
    added: bool,
}

impl Absorption {
    pub(super) fn clear(&mut self) {
        self.episodes.clear();
    }

    pub(super) fn trade(
        &mut self,
        fill: &Tick,
        tick: f64,
        book: &Depth,
        settings: &Detector<AbsorptionRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        let traded = (fill.price / tick).round() as i64;
        let broken: Vec<_> = self
            .episodes
            .iter()
            .filter(|(price, episode)| fill.ts >= episode.first && depth::beyond(**price, traded))
            .map(|(&price, _)| price)
            .collect();
        for price in broken {
            if let Some(episode) = self.episodes.remove(&price) {
                self.calibrate(episode.volume);
            }
        }
        self.advance(fill.ts, tick, book, settings, out);
        let Some(price) = depth::resting(fill, tick).filter(|price| {
            book.inside(price.0) == Some(price.1) && book.size(*price) > 0.0 && fill.size > 0.0
        }) else {
            return;
        };
        let episode = self.episodes.entry(price).or_insert(Episode {
            first: fill.ts,
            last: fill.ts,
            shown: book.size(price),
            volume: 0.0,
            added: false,
        });
        if fill.ts < episode.last {
            return;
        }
        episode.last = fill.ts;
        episode.volume = (episode.volume + fill.size).min(f64::MAX);
    }

    pub(super) fn update(&mut self, before: &Depth, after: &Depth) {
        if after.exact {
            for (&price, episode) in &mut self.episodes {
                episode.added |= after.size(price) > before.size(price);
            }
        }
    }

    pub(super) fn advance(
        &mut self,
        time: DateTime<Utc>,
        tick: f64,
        book: &Depth,
        settings: &Detector<AbsorptionRule>,
        out: &mut Vec<SignalEvent>,
    ) {
        let closed: Vec<_> = self
            .episodes
            .iter()
            .filter(|(_, episode)| time - episode.last >= TimeDelta::seconds(1))
            .map(|(&price, _)| price)
            .collect();
        for price in closed {
            let episode = self.episodes.remove(&price).unwrap();
            let threshold = match settings.rule.threshold {
                Threshold::Fixed(size) => Some(size),
                Threshold::Auto if self.volumes.len() >= 100 => {
                    let mut sorted: Vec<_> = self.volumes.iter().copied().collect();
                    sorted.sort_unstable_by(f64::total_cmp);
                    Some(sorted[(sorted.len() * 99).div_ceil(100) - 1])
                }
                Threshold::Auto => None,
            };
            if settings.enabled
                && threshold.is_some_and(|threshold| meets_threshold(episode.volume, threshold))
                && meets_threshold(episode.volume, episode.shown)
                && (meets_threshold(book.size(price), episode.shown * 0.5) || episode.added)
            {
                self.next += 1;
                out.push(SignalEvent::Upsert(Signal {
                    id: SignalId(format!("absorption-{}", self.next)),
                    shape: SignalShape::PriceZone {
                        from: episode.first,
                        to: Some(episode.last),
                        price: price.1 as f64 * tick,
                        direction: direction(depth::side(price)),
                    },
                    category: DetectionKind::Absorption.category().into(),
                    label: Some("Absorption".into()),
                    tooltip: Some(format!(
                        "Absorbed {}, opening shown {}",
                        episode.volume, episode.shown
                    )),
                }));
            }
            self.calibrate(episode.volume);
        }
    }

    fn calibrate(&mut self, volume: f64) {
        self.volumes.push_back(volume);
        if self.volumes.len() > 1_000 {
            self.volumes.pop_front();
        }
    }
}
