//! Reusable alert settings, stored without the values that belong to one alert: symbols, levels,
//! drawings and webhook addresses.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{
    Alert, AlertChannelComparison, AlertComparison, AlertCondition, AlertDelivery, AlertFrequency,
    AlertRule, AlertSource, NotificationSchedule, Timeframe,
};

/// How many presets the user can keep.
pub(super) const MAX_PRESETS: usize = 5;

/// The stored document's current version. Version 1 kept whole alerts.
const VERSION: u32 = 2;

/// Which groups of settings a preset stores and restores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct PresetFields {
    /// Each rule's source and operator.
    pub(super) condition: bool,
    /// Each rule's interval.
    pub(super) interval: bool,
    /// How often the alert fires.
    pub(super) frequency: bool,
    /// The time left until expiry, or no expiry.
    pub(super) expiry: bool,
    /// The alert's name and message.
    pub(super) text: bool,
    /// The delivery choices, without the webhook address.
    pub(super) delivery: bool,
    /// The notification schedule.
    pub(super) schedule: bool,
}

impl Default for PresetFields {
    fn default() -> Self {
        Self {
            condition: true,
            interval: true,
            frequency: true,
            expiry: true,
            text: true,
            delivery: true,
            schedule: true,
        }
    }
}

impl PresetFields {
    pub(super) fn any(self) -> bool {
        self.condition
            || self.interval
            || self.frequency
            || self.expiry
            || self.text
            || self.delivery
            || self.schedule
    }
}

/// A stored set of settings. Groups that `fields` leaves out keep their defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct AlertPreset {
    pub(super) fields: PresetFields,
    /// The first rule and the additional ones, with zero thresholds and no drawings.
    rules: Vec<AlertRule>,
    /// The saving chart's names for what `rules` test: studies are numbered per chart, so an id
    /// alone may name another study elsewhere. Presets saved without names have none.
    source_names: Vec<(AlertSource, String)>,
    timeframe: Option<Timeframe>,
    additional_timeframes: Vec<Option<Timeframe>>,
    frequency: AlertFrequency,
    /// Seconds from applying the preset to the expiry; `None` never expires.
    expires_after: Option<i64>,
    name: String,
    message: String,
    delivery: AlertDelivery,
    schedule: Option<Box<NotificationSchedule>>,
}

impl AlertPreset {
    /// The selected groups of `alert`, whose expiry counts from the start of `now`'s minute, as
    /// entered times do. `sources` names what its chart offers.
    pub(super) fn capture(
        alert: &Alert,
        sources: &[(AlertSource, String)],
        fields: PresetFields,
        now: DateTime<Utc>,
    ) -> Self {
        let mut preset = Self {
            fields,
            ..Self::default()
        };
        if fields.condition {
            preset.rules = std::iter::once((&alert.source, &alert.condition))
                .chain(
                    alert
                        .additional_conditions
                        .iter()
                        .map(|rule| (&rule.source, &rule.condition)),
                )
                .filter_map(|(source, condition)| {
                    Some(AlertRule {
                        source: match source {
                            AlertSource::Drawing(_) => AlertSource::Price,
                            source => source.clone(),
                        },
                        condition: without_thresholds(condition)?,
                    })
                })
                .collect();
            preset.source_names = sources
                .iter()
                .filter(|(source, _)| {
                    preset.rules.iter().any(|rule| {
                        rule.source == *source || rule.condition.partners().contains(&Some(source))
                    })
                })
                .cloned()
                .collect();
        }
        if fields.interval {
            preset.timeframe = alert.timeframe;
            preset
                .additional_timeframes
                .clone_from(&alert.additional_timeframes);
        }
        if fields.frequency {
            preset.frequency = alert.frequency;
        }
        if fields.expiry {
            preset.expires_after = alert
                .expires
                .map(|expiry| expiry.timestamp() - now.timestamp() + i64::from(now.second()));
        }
        if fields.text {
            preset.name.clone_from(&alert.name);
            preset.message.clone_from(&alert.message);
        }
        if fields.delivery {
            preset.delivery = AlertDelivery {
                webhook_url: String::new(),
                ..alert.delivery.clone()
            };
        }
        if fields.schedule {
            preset.schedule.clone_from(&alert.schedule);
        }
        preset
    }

    /// Set the selected groups on `alert`, whose subject, levels, drawings and webhook address
    /// stay. A stored study that `sources` does not offer under its stored name is not reused, and
    /// a strategy fill alert keeps its condition, interval and any frequency it cannot use.
    pub(super) fn apply(
        &self,
        alert: &mut Alert,
        sources: &[(AlertSource, String)],
        now: DateTime<Utc>,
    ) {
        let fills = matches!(alert.condition, AlertCondition::StrategyFills(_));
        if self.fields.condition && !fills && !self.rules.is_empty() {
            let first = AlertRule {
                source: alert.source.clone(),
                condition: alert.condition.clone(),
            };
            let mut rules = self.rules.iter().enumerate().map(|(index, rule)| {
                let current = match index {
                    0 => &first,
                    _ => alert.additional_conditions.get(index - 1).unwrap_or(&first),
                };
                AlertRule {
                    source: restored_source(
                        &rule.source,
                        &current.source,
                        &self.source_names,
                        sources,
                    ),
                    condition: restored_condition(
                        &rule.condition,
                        &current.condition,
                        &self.source_names,
                        sources,
                    ),
                }
            });
            let restored = rules.next();
            let additional = rules.collect();
            if let Some(rule) = restored {
                alert.source = rule.source;
                alert.condition = rule.condition;
            }
            alert.additional_conditions = additional;
        }
        if self.fields.interval && !fills {
            if alert.timeframe != self.timeframe
                && let Some(inputs) = &mut alert.inputs
            {
                // A captured span counts bars of the interval it was captured on.
                inputs.start = None;
                inputs.visible = None;
            }
            alert.timeframe = self.timeframe;
            alert
                .additional_timeframes
                .clone_from(&self.additional_timeframes);
        }
        alert
            .additional_timeframes
            .truncate(alert.additional_conditions.len());
        if self.fields.frequency
            && (!fills
                || matches!(
                    self.frequency,
                    AlertFrequency::Once | AlertFrequency::EveryTime
                ))
        {
            alert.frequency = self.frequency;
        }
        if self.fields.expiry {
            alert.expires = self
                .expires_after
                .and_then(TimeDelta::try_seconds)
                .and_then(|after| now.checked_add_signed(after));
        }
        if self.fields.text {
            alert.name.clone_from(&self.name);
            alert.message.clone_from(&self.message);
        }
        if self.fields.delivery {
            alert.delivery = AlertDelivery {
                webhook_url: std::mem::take(&mut alert.delivery.webhook_url),
                ..self.delivery.clone()
            };
        }
        if self.fields.schedule {
            alert.schedule.clone_from(&self.schedule);
        }
    }
}

/// `condition`'s operator with zero thresholds. A drawing's relationship becomes the same
/// relationship to a level. `None` when only one strategy run can use it.
fn without_thresholds(condition: &AlertCondition) -> Option<AlertCondition> {
    use AlertCondition as C;
    let channel = |comparison| channel_condition(comparison, 0.0, 0.0);
    Some(match condition {
        C::StrategyFills(_) => return None,
        C::Crossing(_) => C::Crossing(0.0),
        C::CrossingUp(_) => C::CrossingUp(0.0),
        C::CrossingDown(_) => C::CrossingDown(0.0),
        C::Above(_) => C::Above(0.0),
        C::Below(_) => C::Below(0.0),
        C::EnteringChannel { .. } => channel(AlertChannelComparison::Entering),
        C::ExitingChannel { .. } => channel(AlertChannelComparison::Exiting),
        C::InsideChannel { .. } => channel(AlertChannelComparison::Inside),
        C::OutsideChannel { .. } => channel(AlertChannelComparison::Outside),
        C::MovingBy { bars, .. } => C::MovingBy {
            value: 0.0,
            bars: *bars,
        },
        C::MovingByPercent { bars, .. } => C::MovingByPercent {
            percent: 0.0,
            bars: *bars,
        },
        C::Compare {
            other: AlertSource::Drawing(_),
            comparison,
        } => level_condition(*comparison, 0.0),
        C::Compare { other, comparison } => C::Compare {
            other: other.clone(),
            comparison: *comparison,
        },
        C::Channel { comparison, .. } => channel(*comparison),
        C::VolumeAboveAverage { bars, .. } => C::VolumeAboveAverage {
            multiplier: 0.0,
            bars: *bars,
        },
    })
}

/// A stored operator with the thresholds, drawings or plots of `current`. A single level seeds a
/// channel as the condition menu does, and an operator that cannot use what `current` tests leaves
/// it unchanged.
fn restored_condition(
    stored: &AlertCondition,
    current: &AlertCondition,
    names: &[(AlertSource, String)],
    sources: &[(AlertSource, String)],
) -> AlertCondition {
    use AlertCondition as C;
    let levels = match current {
        C::EnteringChannel { lower, upper }
        | C::ExitingChannel { lower, upper }
        | C::InsideChannel { lower, upper }
        | C::OutsideChannel { lower, upper } => Some((*lower, *upper)),
        _ => current.level().map(|level| (level, level + 1.0)),
    };
    let to_level = |comparison| match (current, levels) {
        (C::Compare { other, .. }, _) => C::Compare {
            other: other.clone(),
            comparison,
        },
        (_, Some((level, _))) => level_condition(comparison, level),
        _ => current.clone(),
    };
    let to_channel = |comparison| match (current, levels) {
        (C::Channel { boundaries, .. }, _) => C::Channel {
            boundaries: boundaries.clone(),
            comparison,
        },
        (_, Some((lower, upper))) => channel_condition(comparison, lower, upper),
        _ => current.clone(),
    };
    let compares = current.partners()[0].is_some();
    let change = match current {
        C::MovingBy { value, .. } => *value,
        C::MovingByPercent { percent, .. } => *percent,
        _ => 1.0,
    };
    match stored {
        C::StrategyFills(_) | C::Channel { .. } => current.clone(),
        C::MovingBy { .. } | C::MovingByPercent { .. } | C::VolumeAboveAverage { .. }
            if compares =>
        {
            current.clone()
        }
        C::Crossing(_) => to_level(AlertComparison::Crossing),
        C::CrossingUp(_) => to_level(AlertComparison::CrossingUp),
        C::CrossingDown(_) => to_level(AlertComparison::CrossingDown),
        C::Above(_) => to_level(AlertComparison::Above),
        C::Below(_) => to_level(AlertComparison::Below),
        C::EnteringChannel { .. } => to_channel(AlertChannelComparison::Entering),
        C::ExitingChannel { .. } => to_channel(AlertChannelComparison::Exiting),
        C::InsideChannel { .. } => to_channel(AlertChannelComparison::Inside),
        C::OutsideChannel { .. } => to_channel(AlertChannelComparison::Outside),
        C::MovingBy { bars, .. } => C::MovingBy {
            value: change,
            bars: *bars,
        },
        C::MovingByPercent { bars, .. } => C::MovingByPercent {
            percent: change,
            bars: *bars,
        },
        C::Compare { other, comparison } if !compares && offered(other, names, sources) => {
            C::Compare {
                other: other.clone(),
                comparison: *comparison,
            }
        }
        C::Compare { comparison, .. } => to_level(*comparison),
        C::VolumeAboveAverage { bars, .. } => C::VolumeAboveAverage {
            multiplier: match current {
                C::VolumeAboveAverage { multiplier, .. } => *multiplier,
                _ => 2.0,
            },
            bars: *bars,
        },
    }
}

/// The stored source unless the alert watches a drawing or the chart no longer offers it.
fn restored_source(
    stored: &AlertSource,
    current: &AlertSource,
    names: &[(AlertSource, String)],
    sources: &[(AlertSource, String)],
) -> AlertSource {
    if matches!(current, AlertSource::Drawing(_)) || !offered(stored, names, sources) {
        current.clone()
    } else {
        stored.clone()
    }
}

/// Study identifiers belong to one chart, so a preset can outlive them or meet them on another
/// chart. `names` tells which study a stored identifier meant; a preset saved without its name
/// accepts the identifier only when it is unambiguous.
fn offered(
    source: &AlertSource,
    names: &[(AlertSource, String)],
    sources: &[(AlertSource, String)],
) -> bool {
    match source {
        AlertSource::Price | AlertSource::Volume => true,
        AlertSource::Study { .. } => {
            let mut matching = sources.iter().filter(|(offered, _)| offered == source);
            match names.iter().find(|(named, _)| named == source) {
                Some(stored) => matching.any(|offered| offered == stored),
                None => matching.count() == 1,
            }
        }
        AlertSource::Drawing(_) => false,
    }
}

fn level_condition(comparison: AlertComparison, level: f64) -> AlertCondition {
    match comparison {
        AlertComparison::Crossing => AlertCondition::Crossing(level),
        AlertComparison::CrossingUp => AlertCondition::CrossingUp(level),
        AlertComparison::CrossingDown => AlertCondition::CrossingDown(level),
        AlertComparison::Above => AlertCondition::Above(level),
        AlertComparison::Below => AlertCondition::Below(level),
    }
}

fn channel_condition(comparison: AlertChannelComparison, lower: f64, upper: f64) -> AlertCondition {
    match comparison {
        AlertChannelComparison::Entering => AlertCondition::EnteringChannel { lower, upper },
        AlertChannelComparison::Exiting => AlertCondition::ExitingChannel { lower, upper },
        AlertChannelComparison::Inside => AlertCondition::InsideChannel { lower, upper },
        AlertChannelComparison::Outside => AlertCondition::OutsideChannel { lower, upper },
    }
}

#[derive(Serialize, Deserialize)]
struct PresetDocument<T> {
    version: u32,
    presets: T,
    /// The schedule of the latest saved alert that had one.
    #[serde(default)]
    last_schedule: Option<NotificationSchedule>,
}

/// The stored document for `presets` and the remembered `last_schedule`.
pub(super) fn to_document(
    presets: &BTreeMap<String, AlertPreset>,
    last_schedule: Option<&NotificationSchedule>,
) -> serde_json::Result<String> {
    serde_json::to_string(&PresetDocument {
        version: VERSION,
        presets,
        last_schedule: last_schedule.cloned(),
    })
}

/// Read a stored document. Whole alerts from version 1 become presets of every group except an
/// expiry, whose duration they did not record; a later save rewrites them without their excluded
/// values. Any other version is an error, so that a save cannot overwrite it. The remembered
/// schedule is read from either version.
pub(super) fn from_document(
    document: &str,
    now: DateTime<Utc>,
) -> Result<(BTreeMap<String, AlertPreset>, Option<NotificationSchedule>), String> {
    let document: PresetDocument<serde_json::Value> =
        serde_json::from_str(document).map_err(|error| error.to_string())?;
    let presets = match document.version {
        1 => serde_json::from_value::<BTreeMap<String, Alert>>(document.presets)
            .map(|alerts| {
                alerts
                    .into_iter()
                    .map(|(name, alert)| {
                        let fields = PresetFields {
                            expiry: alert.expires.is_none(),
                            ..PresetFields::default()
                        };
                        (name, AlertPreset::capture(&alert, &[], fields, now))
                    })
                    .collect()
            })
            .map_err(|error| error.to_string()),
        VERSION => serde_json::from_value(document.presets).map_err(|error| error.to_string()),
        _ => Err("Unsupported alert preset version".into()),
    };
    presets.map(|presets| (presets, document.last_schedule))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AlertInputs, NotificationWindow, TimezoneId};
    use chrono::{NaiveTime, TimeZone, Weekday};

    fn study() -> AlertSource {
        AlertSource::Study {
            id: "rsi".into(),
            output: 1,
        }
    }

    #[test]
    fn stored_presets_hold_no_subject_threshold_drawing_or_webhook_address() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let mut alert = Alert::new("SECRETSYMBOL", AlertCondition::CrossingUp(123.456));
        alert.id = Some(crate::model::AlertId("SECRETID".into()));
        alert.watchlist = Some("SECRETLIST".into());
        alert.source = AlertSource::Drawing("SECRETDRAWING".into());
        alert.delivery.webhook = true;
        alert.delivery.webhook_url = "https://SECRETHOOK".into();
        alert.additional_conditions = vec![
            AlertRule {
                source: AlertSource::Price,
                condition: AlertCondition::Compare {
                    other: AlertSource::Drawing("SECRETDRAWING".into()),
                    comparison: AlertComparison::Below,
                },
            },
            AlertRule {
                source: study(),
                condition: AlertCondition::Channel {
                    boundaries: Box::new([
                        AlertSource::Drawing("SECRETDRAWING#1".into()),
                        AlertSource::Drawing("SECRETDRAWING#2".into()),
                    ]),
                    comparison: AlertChannelComparison::Outside,
                },
            },
            AlertRule {
                source: AlertSource::Volume,
                condition: AlertCondition::MovingByPercent {
                    percent: 123.456,
                    bars: 7,
                },
            },
        ];
        let version_1 = serde_json::json!({"version": 1, "presets": {"Old": alert}}).to_string();
        for preset in [
            AlertPreset::capture(&alert, &[], PresetFields::default(), now),
            from_document(&version_1, now)
                .unwrap()
                .0
                .remove("Old")
                .unwrap(),
        ] {
            let stored = serde_json::to_string(&preset).unwrap();
            assert!(!stored.contains("SECRET"), "{stored}");
            assert!(!stored.contains("123.456"), "{stored}");
            // Operators, lookbacks and the webhook choice stay.
            let rules: Vec<_> = preset.rules.iter().map(|r| r.condition.clone()).collect();
            assert_eq!(
                rules,
                [
                    AlertCondition::CrossingUp(0.0),
                    AlertCondition::Below(0.0),
                    AlertCondition::OutsideChannel {
                        lower: 0.0,
                        upper: 0.0
                    },
                    AlertCondition::MovingByPercent {
                        percent: 0.0,
                        bars: 7
                    },
                ]
            );
            assert!(preset.delivery.webhook);
        }
    }

    /// Each row applies a preset saved from `stored` on `stored_source` to an alert on
    /// `current_source` with `current`, and expects the operator with the current values.
    #[test]
    fn applying_keeps_the_current_values_and_takes_the_stored_operator() {
        use AlertCondition as C;
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let drawing = |id: &str| AlertSource::Drawing(id.into());
        let compare = |other, comparison| C::Compare { other, comparison };
        let boundaries = Box::new([drawing("a#1"), drawing("a#2")]);
        let channel = C::Channel {
            boundaries: boundaries.clone(),
            comparison: AlertChannelComparison::Entering,
        };
        let moving = C::MovingBy {
            value: 3.0,
            bars: 1,
        };
        let volume = C::VolumeAboveAverage {
            multiplier: 2.0,
            bars: 20,
        };
        let gone = AlertSource::Study {
            id: "gone".into(),
            output: 0,
        };
        let price = AlertSource::Price;
        let cases = [
            // A level from the opened alert, and a channel seeded the way the menu seeds it.
            (
                price.clone(),
                C::Above(9.0),
                price.clone(),
                C::Crossing(100.0),
                price.clone(),
                C::Above(100.0),
            ),
            (
                price.clone(),
                C::InsideChannel {
                    lower: 1.0,
                    upper: 2.0,
                },
                price.clone(),
                C::Crossing(100.0),
                price.clone(),
                C::InsideChannel {
                    lower: 100.0,
                    upper: 101.0,
                },
            ),
            (
                price.clone(),
                C::Below(9.0),
                price.clone(),
                C::EnteringChannel {
                    lower: 5.0,
                    upper: 6.0,
                },
                price.clone(),
                C::Below(5.0),
            ),
            // Lookbacks are stored, changes and multipliers are not.
            (
                price.clone(),
                C::MovingBy {
                    value: 9.0,
                    bars: 7,
                },
                price.clone(),
                C::MovingByPercent {
                    percent: 3.0,
                    bars: 1,
                },
                price.clone(),
                C::MovingBy {
                    value: 3.0,
                    bars: 7,
                },
            ),
            (
                AlertSource::Volume,
                C::VolumeAboveAverage {
                    multiplier: 9.0,
                    bars: 7,
                },
                price.clone(),
                C::Crossing(100.0),
                AlertSource::Volume,
                C::VolumeAboveAverage {
                    multiplier: 2.0,
                    bars: 7,
                },
            ),
            // An available plot is reused, one the chart no longer has is not.
            (
                study(),
                C::Above(9.0),
                price.clone(),
                C::Crossing(100.0),
                study(),
                C::Above(100.0),
            ),
            (
                gone.clone(),
                C::Above(9.0),
                price.clone(),
                C::Crossing(100.0),
                price.clone(),
                C::Above(100.0),
            ),
            (
                price.clone(),
                compare(study(), AlertComparison::CrossingDown),
                price.clone(),
                C::Crossing(100.0),
                price.clone(),
                compare(study(), AlertComparison::CrossingDown),
            ),
            (
                price.clone(),
                compare(gone.clone(), AlertComparison::Below),
                price.clone(),
                C::Crossing(100.0),
                price.clone(),
                C::Below(100.0),
            ),
            // A stored drawing leaves only its relationship; a drawing alert keeps its drawing.
            (
                price.clone(),
                compare(drawing("old"), AlertComparison::Above),
                price.clone(),
                C::Crossing(100.0),
                price.clone(),
                C::Above(100.0),
            ),
            (
                price.clone(),
                C::CrossingUp(9.0),
                price.clone(),
                compare(drawing("new"), AlertComparison::Crossing),
                price.clone(),
                compare(drawing("new"), AlertComparison::CrossingUp),
            ),
            (
                price.clone(),
                C::Channel {
                    boundaries: Box::new([drawing("old#1"), drawing("old#2")]),
                    comparison: AlertChannelComparison::Exiting,
                },
                price.clone(),
                channel.clone(),
                price.clone(),
                C::Channel {
                    boundaries,
                    comparison: AlertChannelComparison::Exiting,
                },
            ),
            // A comparison keeps what it compares with and takes the stored relationship.
            (
                price.clone(),
                C::CrossingUp(9.0),
                price.clone(),
                compare(study(), AlertComparison::Crossing),
                price.clone(),
                compare(study(), AlertComparison::CrossingUp),
            ),
            (
                price.clone(),
                compare(study(), AlertComparison::Below),
                price.clone(),
                compare(drawing("new"), AlertComparison::Crossing),
                price.clone(),
                compare(drawing("new"), AlertComparison::Below),
            ),
            // An operator that cannot use the drawings or plots leaves the condition as it is.
            (
                price.clone(),
                C::Above(9.0),
                price.clone(),
                channel.clone(),
                price.clone(),
                channel.clone(),
            ),
            (
                price.clone(),
                compare(study(), AlertComparison::Below),
                price.clone(),
                channel.clone(),
                price.clone(),
                channel.clone(),
            ),
            (
                price.clone(),
                C::VolumeAboveAverage {
                    multiplier: 9.0,
                    bars: 7,
                },
                price.clone(),
                channel.clone(),
                price.clone(),
                channel,
            ),
            (
                price.clone(),
                C::InsideChannel {
                    lower: 1.0,
                    upper: 2.0,
                },
                price.clone(),
                compare(drawing("new"), AlertComparison::Crossing),
                price.clone(),
                compare(drawing("new"), AlertComparison::Crossing),
            ),
            (
                price.clone(),
                C::MovingBy {
                    value: 9.0,
                    bars: 7,
                },
                price.clone(),
                compare(study(), AlertComparison::Crossing),
                price.clone(),
                compare(study(), AlertComparison::Crossing),
            ),
            // So does one that needs a level the condition does not have.
            (
                price.clone(),
                C::Above(9.0),
                price.clone(),
                moving.clone(),
                price.clone(),
                moving.clone(),
            ),
            (
                price.clone(),
                compare(gone.clone(), AlertComparison::Below),
                price.clone(),
                moving.clone(),
                price.clone(),
                moving,
            ),
            (
                AlertSource::Volume,
                C::InsideChannel {
                    lower: 1.0,
                    upper: 2.0,
                },
                AlertSource::Volume,
                volume.clone(),
                AlertSource::Volume,
                volume,
            ),
            (
                study(),
                C::Below(9.0),
                drawing("new"),
                C::Crossing(100.0),
                drawing("new"),
                C::Below(100.0),
            ),
        ];
        let sources = [(study(), "RSI signal".to_owned())];
        for (stored_source, stored, current_source, current, source, condition) in cases {
            let mut saved = Alert::new("OLD", stored.clone());
            saved.source = stored_source;
            let preset = AlertPreset::capture(&saved, &sources, PresetFields::default(), now);
            let mut alert = Alert::new("NEW", current.clone());
            alert.source = current_source;
            preset.apply(&mut alert, &sources, now);
            assert_eq!(
                (&alert.source, &alert.condition),
                (&source, &condition),
                "{stored:?} on {current:?}"
            );
            assert!(alert.is_valid(), "{alert:?}");
            assert_eq!(alert.symbol, "NEW");
        }
    }

    #[test]
    fn only_the_selected_groups_survive_the_document_and_reach_the_alert() {
        let saved_at =
            Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 48).unwrap() + TimeDelta::milliseconds(500);
        let now = Utc.with_ymd_and_hms(2026, 3, 1, 8, 0, 15).unwrap();
        let schedule = Box::new(NotificationSchedule {
            timezone: TimezoneId::utc(),
            days: vec![Weekday::Mon],
            windows: vec![NotificationWindow {
                start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            }],
        });
        let mut saved = Alert::new("OLD", AlertCondition::Below(9.0));
        saved.name = "Name".into();
        saved.message = "Message".into();
        saved.timeframe = Some(Timeframe::Hour1);
        saved.frequency = AlertFrequency::OncePerBar;
        // An entered 11:00 at 10:00:48.5 means an hour.
        saved.expires = Some(Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap());
        saved.schedule = Some(schedule.clone());
        saved.delivery.email = true;
        saved.delivery.popup = false;
        saved.additional_conditions.push(AlertRule {
            source: AlertSource::Volume,
            condition: AlertCondition::Above(1.0),
        });
        saved.additional_timeframes.push(Some(Timeframe::Min5));

        let mut blank = PresetFields {
            condition: false,
            interval: false,
            frequency: false,
            expiry: false,
            text: false,
            delivery: false,
            schedule: false,
        };
        assert!(!blank.any());
        let span = chrono::DateTime::from_timestamp(0, 0);
        let opened = || {
            let mut alert = Alert::new("NEW", AlertCondition::Crossing(100.0));
            alert.message = "Own".into();
            alert.delivery.webhook_url = "https://own".into();
            alert.expires = Some(now + TimeDelta::days(9));
            alert.inputs = Some(Box::new(AlertInputs {
                start: span,
                ..Default::default()
            }));
            alert
        };
        let apply = |saved: &Alert, fields: PresetFields| {
            let preset = AlertPreset::capture(saved, &[], fields, saved_at);
            let presets = BTreeMap::from([("P".to_owned(), preset)]);
            let document = to_document(&presets, None).unwrap();
            let mut alert = opened();
            from_document(&document, saved_at).unwrap().0["P"].apply(&mut alert, &[], now);
            alert
        };

        assert_eq!(
            apply(&saved, blank),
            opened(),
            "nothing selected changes nothing"
        );
        blank.condition = true;
        let alert = apply(&saved, blank);
        assert_eq!(alert.condition, AlertCondition::Below(100.0));
        assert_eq!(alert.additional_conditions.len(), 1);
        assert!(alert.additional_timeframes.is_empty());
        assert_eq!((alert.timeframe, alert.message.as_str()), (None, "Own"));
        blank.condition = false;
        blank.interval = true;
        let alert = apply(&saved, blank);
        assert_eq!(alert.timeframe, Some(Timeframe::Hour1));
        assert!(alert.additional_timeframes.is_empty(), "no rule takes it");
        assert_eq!(alert.inputs.as_ref().unwrap().start, None);
        assert_eq!(alert.condition, AlertCondition::Crossing(100.0));
        blank.interval = false;
        blank.frequency = true;
        assert_eq!(apply(&saved, blank).frequency, AlertFrequency::OncePerBar);
        blank.frequency = false;
        blank.expiry = true;
        assert_eq!(
            apply(&saved, blank).expires,
            Some(now + TimeDelta::hours(1))
        );
        saved.expires = None;
        assert_eq!(
            apply(&saved, blank).expires,
            None,
            "open-ended stays open-ended"
        );
        blank.expiry = false;
        blank.text = true;
        let alert = apply(&saved, blank);
        assert_eq!(
            (alert.name.as_str(), alert.message.as_str()),
            ("Name", "Message")
        );
        blank.text = false;
        blank.delivery = true;
        let alert = apply(&saved, blank);
        assert!(alert.delivery.email && !alert.delivery.popup);
        assert_eq!(alert.delivery.webhook_url, "https://own");
        blank.delivery = false;
        blank.schedule = true;
        assert_eq!(apply(&saved, blank).schedule, Some(schedule));
        let all = apply(&saved, PresetFields::default());
        assert_eq!(all.additional_timeframes, [Some(Timeframe::Min5)]);
    }

    #[test]
    fn a_strategy_fill_alert_keeps_what_it_needs_to_stay_valid() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let mut saved = Alert::new("OLD", AlertCondition::Below(9.0));
        saved.frequency = AlertFrequency::OncePerBarClose;
        saved.timeframe = Some(Timeframe::Hour1);
        let preset = AlertPreset::capture(&saved, &[], PresetFields::default(), now);
        let run = AlertCondition::StrategyFills("run".into());
        let mut alert = Alert::new("NEW", run.clone());
        preset.apply(&mut alert, &[], now);
        assert_eq!(alert.condition, run);
        assert_eq!(alert.frequency, AlertFrequency::EveryTime);
        assert_eq!(alert.timeframe, None);
        assert!(alert.is_valid());
        // The run itself is not reusable.
        let fills =
            AlertPreset::capture(&Alert::new("OLD", run), &[], PresetFields::default(), now);
        let mut alert = Alert::new("NEW", AlertCondition::Above(1.0));
        fills.apply(&mut alert, &[], now);
        assert_eq!(alert.condition, AlertCondition::Above(1.0));
    }

    /// Charts number their studies from 2, so the first study of any two charts shares an id.
    #[test]
    fn a_study_id_of_another_chart_is_not_taken_for_the_stored_plot() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let plot = AlertSource::Study {
            id: "2".into(),
            output: 0,
        };
        let named = |name: &str| (plot.clone(), name.to_owned());
        let crossing_up_plot = AlertCondition::Compare {
            other: plot.clone(),
            comparison: AlertComparison::CrossingUp,
        };
        let mut on_plot = Alert::new("OLD", AlertCondition::Above(70.0));
        on_plot.source = plot.clone();
        let crossing_plot = Alert::new("OLD", crossing_up_plot.clone());
        // What this file stored before presets kept the names of their plots.
        let unnamed = |condition: serde_json::Value, source: serde_json::Value| {
            let rule = serde_json::json!({"source": source, "condition": condition});
            let document = serde_json::json!({"version": 2, "presets": {"P": {"rules": [rule]}}});
            from_document(&document.to_string(), now)
                .unwrap()
                .0
                .remove("P")
                .unwrap()
        };
        let study = serde_json::json!({"Study": {"id": "2", "output": 0}});
        let unnamed_on_plot = unnamed(serde_json::json!({"Above": 0.0}), study.clone());
        let unnamed_crossing = unnamed(
            serde_json::json!({"Compare": {"other": study, "comparison": "CrossingUp"}}),
            serde_json::json!("Price"),
        );

        let rsi = [named("RSI: RSI")];
        let sma = [named("SMA: SMA")];
        let on_plot = AlertPreset::capture(&on_plot, &rsi, PresetFields::default(), now);
        let crossing = AlertPreset::capture(&crossing_plot, &sma, PresetFields::default(), now);
        let macd = [named("MACD: MACD")];
        let twice = [named("RSI: RSI"), named("MACD: MACD")];
        let kept_on_level = (AlertSource::Price, AlertCondition::Above(100.0));
        let crossing_level = (AlertSource::Price, AlertCondition::CrossingUp(100.0));
        let reused_source = (plot.clone(), AlertCondition::Above(100.0));
        let reused_other = (AlertSource::Price, crossing_up_plot);
        let rows: [(&AlertPreset, &[(AlertSource, String)], _); 8] = [
            // Another chart's study 2 is not the stored one.
            (&on_plot, &macd, &kept_on_level),
            (&crossing, &macd, &crossing_level),
            // The chart that saved the preset still has it.
            (&on_plot, &rsi, &reused_source),
            (&crossing, &sma, &reused_other),
            // Without a stored name only a single candidate can be meant.
            (&unnamed_on_plot, &macd, &reused_source),
            (&unnamed_crossing, &macd, &reused_other),
            (&unnamed_on_plot, &twice, &kept_on_level),
            (&unnamed_crossing, &twice, &crossing_level),
        ];
        for (index, (preset, sources, expected)) in rows.into_iter().enumerate() {
            let mut alert = Alert::new("NEW", AlertCondition::Crossing(100.0));
            preset.apply(&mut alert, sources, now);
            assert_eq!(
                (&alert.source, &alert.condition),
                (&expected.0, &expected.1),
                "row {index}"
            );
        }
    }
}
