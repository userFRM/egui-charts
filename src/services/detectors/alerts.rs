//! Feed-time counts and live notifications, independent of retained chart signals.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};

use super::{DetectionKind, DetectorSettings, OrderFlowDetectors, RETAINED, Tracked};
use crate::model::SignalId;
use crate::services::{Events, Notification, Notifications, SignalEvent};

/// Running detections, including loaded history and excluding updates of an existing signal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetectionCounts {
    /// Detections since the symbol was tracked or reset, including evicted chart signals.
    pub total: u64,
    /// Detections in `(now - 5 minutes, now]` by feed time.
    pub last_five_minutes: u64,
    /// Detections in `(now - 30 seconds, now]` by feed time.
    pub last_thirty_seconds: u64,
}

/// The feed-time window of a count alert.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CountWindow {
    /// Since tracking or reset.
    Total,
    /// The preceding five minutes, excluding the opening boundary.
    FiveMinutes,
    /// The preceding thirty seconds, excluding the opening boundary.
    #[default]
    ThirtySeconds,
}

impl CountWindow {
    /// The count of this window.
    pub fn count(self, counts: DetectionCounts) -> u64 {
        match self {
            Self::Total => counts.total,
            Self::FiveMinutes => counts.last_five_minutes,
            Self::ThirtySeconds => counts.last_thirty_seconds,
        }
    }

    /// The window label, also a translation key for user interfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Total => "Since tracking began",
            Self::FiveMinutes => "Last 5 minutes",
            Self::ThirtySeconds => "Last 30 seconds",
        }
    }
}

/// Fires once when its window reaches `minimum`, rearming only below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CountAlert {
    /// Which running count to compare.
    pub window: CountWindow,
    /// Positive minimum, validated by [`OrderFlowDetectors::set_settings`].
    pub minimum: u64,
    /// Whether the host should play a sound for this count alert.
    pub sound: bool,
}

impl Default for CountAlert {
    fn default() -> Self {
        Self {
            window: CountWindow::default(),
            minimum: 1,
            sound: false,
        }
    }
}

/// A live detection or count alert for a panel to show and the host to sound.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DetectionAlert {
    /// The detector category.
    pub kind: DetectionKind,
    /// The same entry delivered through [`Notifications::watch`].
    pub notification: Notification,
    /// The window and reached count for a count alert; absent for an individual detection.
    pub count: Option<(CountWindow, u64)>,
    /// Whether the host should play this kind's sound.
    pub sound: bool,
}

#[derive(Default)]
pub(super) struct Delivery {
    alerts: VecDeque<DetectionAlert>,
    log: VecDeque<Notification>,
    watchers: Vec<Events<Notification>>,
}

impl Delivery {
    fn push(&mut self, alert: DetectionAlert) {
        self.watchers.retain(Events::is_active);
        for events in &self.watchers {
            events.push(alert.notification.clone());
        }
        self.log.push_back(alert.notification.clone());
        self.alerts.push_back(alert);
        if self.log.len() > RETAINED {
            self.log.pop_front();
        }
        if self.alerts.len() > RETAINED {
            self.alerts.pop_front();
        }
    }
}

impl OrderFlowDetectors {
    /// Counts of `kind` on `symbol` at the current feed time, or zero for an untracked symbol.
    /// Windows retain five minutes relative to the latest supplied feed time; queries before
    /// that retained interval cannot restore expired detections. Wall-clock time is not used.
    pub fn counts(&self, symbol: &str, kind: DetectionKind, now: DateTime<Utc>) -> DetectionCounts {
        self.lock()
            .symbols
            .get(symbol)
            .map(|tracked| tracked.counting.counts(kind, now))
            .unwrap_or_default()
    }

    /// Drains pending live alerts once, oldest first. At most the latest 5,000 are retained
    /// between calls; draining does not remove entries from the notification log.
    pub fn take_alerts(&self) -> Vec<DetectionAlert> {
        self.lock().delivery.alerts.drain(..).collect()
    }
}

impl Notifications for OrderFlowDetectors {
    fn watch(&self, events: Events<Notification>) {
        let mut state = self.lock();
        for notification in &state.delivery.log {
            events.push(notification.clone());
        }
        state.delivery.watchers.retain(Events::is_active);
        state.delivery.watchers.push(events);
    }
}

#[derive(Default)]
pub(super) struct Counting {
    kinds: HashMap<DetectionKind, Counter>,
    now: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct Counter {
    total: u64,
    times: BTreeMap<DateTime<Utc>, u64>,
    last_episode: u64,
    rule: Option<(CountWindow, u64)>,
    above: bool,
}

impl Counting {
    fn counts(&self, kind: DetectionKind, now: DateTime<Utc>) -> DetectionCounts {
        let Some(counter) = self.kinds.get(&kind) else {
            return DetectionCounts::default();
        };
        let mut counts = DetectionCounts {
            total: counter.total,
            ..Default::default()
        };
        for (&time, &number) in &counter.times {
            let age = now.signed_duration_since(time);
            if age >= TimeDelta::zero() && age < TimeDelta::minutes(5) {
                counts.last_five_minutes = counts.last_five_minutes.saturating_add(number);
                if age < TimeDelta::seconds(30) {
                    counts.last_thirty_seconds = counts.last_thirty_seconds.saturating_add(number);
                }
            }
        }
        counts
    }

    pub(super) fn advance(
        &mut self,
        time: DateTime<Utc>,
        settings: &DetectorSettings,
        live: bool,
        symbol: &str,
        delivery: &mut Delivery,
    ) {
        let now = self.now.map_or(time, |last| last.max(time));
        self.now = Some(now);
        for counter in self.kinds.values_mut() {
            while counter
                .times
                .first_key_value()
                .is_some_and(|(&time, _)| now.signed_duration_since(time) >= TimeDelta::minutes(5))
            {
                counter.times.pop_first();
            }
        }
        for kind in [DetectionKind::Iceberg, DetectionKind::StopRun] {
            self.check(kind, now, settings, live, symbol, delivery);
        }
    }

    fn check(
        &mut self,
        kind: DetectionKind,
        now: DateTime<Utc>,
        settings: &DetectorSettings,
        live: bool,
        symbol: &str,
        delivery: &mut Delivery,
    ) {
        let rule = match kind {
            DetectionKind::Iceberg => settings.iceberg_count_alert,
            DetectionKind::StopRun => settings.stop_run_count_alert,
            _ => return,
        };
        let count = rule.map(|rule| rule.window.count(self.counts(kind, now)));
        let counter = self.kinds.entry(kind).or_default();
        let edge = rule.map(|rule| (rule.window, rule.minimum));
        if counter.rule != edge {
            counter.above = false;
            counter.rule = edge;
        }
        let above = rule
            .zip(count)
            .is_some_and(|(rule, count)| count >= rule.minimum);
        if above && !counter.above && live {
            let rule = rule.unwrap();
            let count = count.unwrap();
            let mut notification = Notification::new(now, kind.category());
            notification.body = format!("{}: {count}", rule.window.label());
            notification.symbol = Some(symbol.into());
            delivery.push(DetectionAlert {
                kind,
                notification,
                count: Some((rule.window, count)),
                sound: rule.sound,
            });
        }
        counter.above = above;
    }
}

impl Tracked {
    pub(super) fn record(
        &mut self,
        events: &[SignalEvent],
        time: DateTime<Utc>,
        live: bool,
        settings: &DetectorSettings,
        delivery: &mut Delivery,
        added: &mut HashSet<SignalId>,
    ) {
        for event in events {
            let SignalEvent::Upsert(signal) = event else {
                continue;
            };
            if self.detections.iter().any(|kept| kept.id == signal.id)
                || !added.insert(signal.id.clone())
            {
                continue;
            }
            let Some(kind) = DetectionKind::ALL
                .into_iter()
                .find(|kind| kind.category() == signal.category)
            else {
                continue;
            };
            let counter = self.counting.kinds.entry(kind).or_default();
            // These episodes can outlive chart retention; their numbers only increase.
            if matches!(
                kind,
                DetectionKind::BigTrade
                    | DetectionKind::BigTradeZone
                    | DetectionKind::StopRun
                    | DetectionKind::Iceberg
            ) {
                let number = signal
                    .id
                    .0
                    .rsplit('-')
                    .next()
                    .unwrap()
                    .parse::<u64>()
                    .expect("numbered detector episode");
                if number <= counter.last_episode {
                    continue;
                }
                counter.last_episode = number;
            }
            counter.total = counter.total.saturating_add(1);
            let number = counter.times.entry(time).or_default();
            *number = number.saturating_add(1);
            let (alert, sound) = match kind {
                DetectionKind::BigTrade => (settings.big_trades.alert, settings.big_trades.sound),
                // A zone is the price range of a trade that has already alerted.
                DetectionKind::BigTradeZone => (false, false),
                DetectionKind::StopRun => (settings.stop_runs.alert, settings.stop_runs.sound),
                DetectionKind::StopSweep => {
                    (settings.stop_sweeps.alert, settings.stop_sweeps.sound)
                }
                DetectionKind::Iceberg => (settings.icebergs.alert, settings.icebergs.sound),
                DetectionKind::Absorption => (settings.absorption.alert, settings.absorption.sound),
                _ => (settings.liquidity.alert, settings.liquidity.sound),
            };
            if live && alert {
                let mut notification = Notification::new(time, kind.category());
                notification.body = signal
                    .tooltip
                    .as_ref()
                    .or(signal.label.as_ref())
                    .cloned()
                    .unwrap_or_else(|| kind.category().into());
                notification.symbol = Some(self.instrument.symbol.clone());
                delivery.push(DetectionAlert {
                    kind,
                    notification,
                    count: None,
                    sound,
                });
            }
            self.counting.check(
                kind,
                self.counting.now.unwrap_or(time),
                settings,
                live,
                &self.instrument.symbol,
                delivery,
            );
        }
    }
}
