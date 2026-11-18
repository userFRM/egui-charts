//! The intervals a drawing or a study is drawn on.

use crate::model::{Timeframe, TimeframeUnit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::ops::RangeInclusive;

/// The intervals a drawing or a study is drawn on.
///
/// While [`show_on_all`](Self::show_on_all) is set, it is drawn on every interval. Otherwise it is
/// drawn on the intervals named in [`visible_timeframes`](Self::visible_timeframes) and on those
/// whose unit has a range in [`ranges`](Self::ranges) holding their value. An interval's unit and
/// value are those of its label, as [`Timeframe::as_str`](crate::model::Timeframe::as_str) writes
/// it: `90min` is 90 minutes, and a 24 hour interval, labelled `1D`, is 1 day.
///
/// The default names no interval and holds no range, so it is drawn on none; [`all`](Self::all)
/// draws on every interval.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TimeframeVisibility {
    /// Show on every interval.
    pub show_on_all: bool,
    /// Interval labels drawn on, matched exactly.
    #[serde(serialize_with = "serialize_sorted")]
    pub visible_timeframes: HashSet<String>,
    /// Values of each unit drawn on, both ends included, such as 1 to 4 hours for
    /// 1h, 2h, 3h and 4h. A range whose start is after its end holds no value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ranges: BTreeMap<TimeframeUnit, RangeInclusive<u64>>,
}

/// Writes the set sorted, so saving the same drawings gives the same file.
fn serialize_sorted<S: serde::Serializer>(
    set: &HashSet<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(set.iter().collect::<std::collections::BTreeSet<_>>())
}

/// The value and unit of an interval label written by `Timeframe::as_str`, such as `90min`.
fn interval_parts(label: &str) -> Option<(u64, TimeframeUnit)> {
    let split = label.find(|c: char| !c.is_ascii_digit())?;
    let unit = match &label[split..] {
        "t" => TimeframeUnit::Tick,
        "v" => TimeframeUnit::Volume,
        "r" => TimeframeUnit::Range,
        "delta" => TimeframeUnit::Delta,
        "ms" => TimeframeUnit::Millisecond,
        "s" => TimeframeUnit::Second,
        "min" => TimeframeUnit::Minute,
        "h" => TimeframeUnit::Hour,
        "D" => TimeframeUnit::Day,
        "W" => TimeframeUnit::Week,
        "M" => TimeframeUnit::Month,
        _ => return None,
    };
    Some((label[..split].parse().ok()?, unit))
}

impl TimeframeVisibility {
    /// Visibility on every interval, which drawings and studies start with.
    pub fn all() -> Self {
        Self {
            show_on_all: true,
            visible_timeframes: HashSet::new(),
            ranges: BTreeMap::new(),
        }
    }

    /// Create visibility for specific timeframes only
    pub fn specific(timeframes: &[&str]) -> Self {
        Self {
            show_on_all: false,
            visible_timeframes: timeframes.iter().map(|s| (*s).to_string()).collect(),
            ranges: BTreeMap::new(),
        }
    }

    /// Show only on `current`, matched by its canonical interval label.
    pub fn current_interval(current: Timeframe) -> Self {
        Self::specific(&[&current.as_str()])
    }

    /// Show on `current` and longer time intervals. Calendar months compare as 30 days,
    /// as in [`Timeframe::duration_ms`]. Trade intervals compare only within their own unit.
    pub fn higher_intervals(current: Timeframe) -> Self {
        Self::relative_intervals(current, true)
    }

    /// Show on `current` and shorter time intervals, using the same units and calendar
    /// approximation as [`Self::higher_intervals`].
    pub fn lower_intervals(current: Timeframe) -> Self {
        Self::relative_intervals(current, false)
    }

    fn relative_intervals(current: Timeframe, higher: bool) -> Self {
        use TimeframeUnit as U;
        let (value, unit) = interval_parts(&current.as_str()).expect("canonical interval label");
        let mut visibility = Self::default();
        if current.is_trade_based() {
            visibility.set_range(
                unit,
                Some(if higher { value..=u64::MAX } else { 1..=value }),
            );
            return visibility;
        }
        // Milliseconds in u128 keep the largest custom seconds and converted bounds exact.
        let units = [
            (U::Millisecond, 1_u128),
            (U::Second, 1_000),
            (U::Minute, 60_000),
            (U::Hour, 3_600_000),
            (U::Day, 86_400_000),
            (U::Week, 604_800_000),
            (U::Month, 2_592_000_000),
        ];
        let duration = u128::from(value) * units.iter().find(|(u, _)| *u == unit).unwrap().1;
        for (unit, scale) in units {
            let bound = if higher {
                duration.div_ceil(scale)
            } else {
                duration / scale
            };
            // Subsecond intervals are presets; custom time intervals use whole seconds.
            let max = if unit == U::Millisecond {
                999
            } else {
                u64::MAX
            };
            if higher && bound > u128::from(max) || !higher && bound == 0 {
                continue;
            }
            let bound = bound.min(u128::from(max)) as u64;
            visibility.set_range(unit, Some(if higher { bound..=max } else { 1..=bound }));
        }
        visibility
    }

    /// Check if visible on the interval labelled `timeframe`
    #[inline]
    pub fn is_visible_on(&self, timeframe: &str) -> bool {
        self.show_on_all
            || self.visible_timeframes.contains(timeframe)
            || (!self.ranges.is_empty()
                && interval_parts(timeframe).is_some_and(|(value, unit)| {
                    self.ranges
                        .get(&unit)
                        .is_some_and(|range| range.contains(&value))
                }))
    }

    /// The values of `unit` the drawing shows on: its range, widened to cover the labels of that
    /// unit in [`visible_timeframes`](Self::visible_timeframes). `None` when neither names the unit.
    pub fn range(&self, unit: TimeframeUnit) -> Option<RangeInclusive<u64>> {
        self.visible_timeframes
            .iter()
            .filter_map(|label| interval_parts(label))
            .filter(|&(_, of)| of == unit)
            .map(|(value, _)| value..=value)
            .chain(self.ranges.get(&unit).cloned())
            .reduce(|a, b| *a.start().min(b.start())..=*a.end().max(b.end()))
    }

    /// Show the drawing on `range` of `unit`, or on no interval of that unit for `None`, in place
    /// of that unit's labels in [`visible_timeframes`](Self::visible_timeframes). A range turns
    /// [`show_on_all`](Self::show_on_all) off.
    pub fn set_range(&mut self, unit: TimeframeUnit, range: Option<RangeInclusive<u64>>) {
        self.visible_timeframes
            .retain(|label| interval_parts(label).is_none_or(|(_, of)| of != unit));
        if let Some(range) = range {
            self.show_on_all = false;
            self.ranges.insert(unit, range);
        } else {
            self.ranges.remove(&unit);
        }
    }

    /// Add a timeframe to visibility
    pub fn add_timeframe(&mut self, timeframe: &str) {
        self.show_on_all = false;
        self.visible_timeframes.insert(timeframe.to_string());
    }

    /// Remove a timeframe from visibility
    pub fn remove_timeframe(&mut self, timeframe: &str) {
        self.visible_timeframes.remove(timeframe);
        if self.visible_timeframes.is_empty() && self.ranges.is_empty() {
            self.show_on_all = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drawing shows on intervals whose unit and value, read from the label, fall in a range,
    /// and on labels saved before ranges, matched exactly until their unit's range is set.
    #[test]
    fn test_timeframe_visibility() {
        let mut vis = TimeframeVisibility::all();
        assert!(vis.is_visible_on("1h"));
        assert!(vis.is_visible_on("1D"));

        vis.add_timeframe("1h");
        assert!(vis.is_visible_on("1h"));
        assert!(!vis.is_visible_on("1D"));

        let n = |v| std::num::NonZeroU64::new(v).unwrap();
        let mut vis = TimeframeVisibility::all();
        vis.set_range(TimeframeUnit::Hour, Some(1..=4));
        vis.set_range(TimeframeUnit::Minute, Some(1..=59));
        vis.set_range(TimeframeUnit::Second, Some(1..=600));
        vis.set_range(TimeframeUnit::Tick, Some(100..=1000));
        for (timeframe, shown) in [
            (Timeframe::Hour1, true),                // "1h", the start
            (Timeframe::Custom(n(2 * 3600)), true),  // "2h", not one of the nine former names
            (Timeframe::Hour4, true),                // "4h", the end
            (Timeframe::Custom(n(5 * 3600)), false), // "5h"
            (Timeframe::Day1, false),                // "1D"
            (Timeframe::Min30, true),                // "30min"
            (Timeframe::Custom(n(90 * 60)), false),  // "90min", 90 > 59
            (Timeframe::Month1, false),              // "1M" is a month, not a minute
            (Timeframe::Sec30, true),                // "30s"
            (Timeframe::Ms500, false),               // "500ms" is not 500 seconds
            (Timeframe::Ticks(n(99)), false),
            (Timeframe::Ticks(n(100)), true),
            (Timeframe::Ticks(n(1000)), true),
            (Timeframe::Ticks(n(1001)), false),
            (Timeframe::Range(n(100)), false), // "100r", unit not set
            (Timeframe::Delta(n(100)), false), // "100delta"
        ] {
            assert_eq!(vis.is_visible_on(&timeframe.as_str()), shown, "{timeframe}");
        }
        vis.show_on_all = true;
        assert!(vis.is_visible_on("1D"));

        let mut saved = TimeframeVisibility::specific(&["1h", "4h", "1D"]);
        assert_eq!(saved.range(TimeframeUnit::Hour), Some(1..=4));
        assert!(!saved.is_visible_on("2h"));
        saved.set_range(TimeframeUnit::Hour, Some(2..=3));
        assert_eq!(saved.range(TimeframeUnit::Hour), Some(2..=3));
        assert!(
            !saved.is_visible_on("1h") && saved.is_visible_on("2h") && saved.is_visible_on("1D")
        );
        saved.remove_timeframe("1D");
        assert!(!saved.is_visible_on("1D") && saved.is_visible_on("3h"));
        saved.set_range(TimeframeUnit::Hour, None);
        assert_eq!(saved.range(TimeframeUnit::Hour), None);

        // One hour = 3,600 seconds; 90 minutes = 5,400 seconds. Both bounds include equality.
        for (current, probes) in [
            (
                Timeframe::Hour1,
                vec![
                    ("30s", false, true),
                    ("59min", false, true),
                    ("1h", true, true),
                    ("90min", true, false),
                    ("1D", true, false),
                    ("1W", true, false),
                    ("1M", true, false),
                    ("100t", false, false),
                ],
            ),
            (
                Timeframe::Custom(n(5400)),
                vec![
                    ("1h", false, true),
                    ("89min", false, true),
                    ("90min", true, true),
                    ("91min", true, false),
                    ("2h", true, false),
                ],
            ),
            (
                Timeframe::Ms250,
                vec![
                    ("100ms", false, true),
                    ("250ms", true, true),
                    ("500ms", true, false),
                    ("1s", true, false),
                ],
            ),
            (
                Timeframe::Week1,
                vec![
                    ("6D", false, true),
                    ("7D", true, true),
                    ("1W", true, true),
                    ("1M", true, false),
                ],
            ),
            (
                Timeframe::Month1,
                vec![
                    ("4W", false, true),
                    ("30D", true, true),
                    ("1M", true, true),
                    ("5W", true, false),
                ],
            ),
            (
                Timeframe::Ticks(n(100)),
                vec![
                    ("99t", false, true),
                    ("100t", true, true),
                    ("101t", true, false),
                    ("100v", false, false),
                    ("1D", false, false),
                ],
            ),
            (
                Timeframe::Volume(n(100)),
                vec![
                    ("99v", false, true),
                    ("100v", true, true),
                    ("101v", true, false),
                    ("100t", false, false),
                ],
            ),
            (
                Timeframe::Range(n(100)),
                vec![
                    ("99r", false, true),
                    ("100r", true, true),
                    ("101r", true, false),
                    ("100delta", false, false),
                ],
            ),
            (
                Timeframe::Delta(n(100)),
                vec![
                    ("99delta", false, true),
                    ("100delta", true, true),
                    ("101delta", true, false),
                    ("100r", false, false),
                ],
            ),
        ] {
            let above = TimeframeVisibility::higher_intervals(current);
            let below = TimeframeVisibility::lower_intervals(current);
            let only = TimeframeVisibility::current_interval(current);
            for (label, higher, lower) in probes {
                assert_eq!(
                    above.is_visible_on(label),
                    higher,
                    "above {current}: {label}"
                );
                assert_eq!(
                    below.is_visible_on(label),
                    lower,
                    "below {current}: {label}"
                );
                assert_eq!(
                    only.is_visible_on(label),
                    label == current.as_str(),
                    "only {current}: {label}"
                );
            }
        }
        assert_eq!(
            TimeframeVisibility::higher_intervals(Timeframe::Hour1)
                .range(TimeframeUnit::Millisecond),
            None
        );
        // Values above i64::MAX milliseconds must not collapse to equal saturated durations.
        let largest = TimeframeVisibility::higher_intervals(Timeframe::Custom(n(u64::MAX)));
        assert!(largest.is_visible_on(&format!("{}s", u64::MAX)));
        assert!(!largest.is_visible_on(&format!("{}s", u64::MAX - 1)));
        assert!(
            TimeframeVisibility::lower_intervals(Timeframe::Custom(n(u64::MAX)))
                .is_visible_on(&format!("{}s", u64::MAX - 1))
        );

        let legacy = r#"{"show_on_all":false,"visible_timeframes":["4h"]}"#;
        let loaded: TimeframeVisibility = serde_json::from_str(legacy).unwrap();
        assert_eq!(loaded, TimeframeVisibility::specific(&["4h"]));
        assert_eq!(serde_json::to_string(&loaded).unwrap(), legacy);
        let mut hours = TimeframeVisibility::default();
        hours.set_range(TimeframeUnit::Hour, Some(1..=4));
        let json = serde_json::to_string(&hours).unwrap();
        assert_eq!(
            json,
            r#"{"show_on_all":false,"visible_timeframes":[],"ranges":{"Hour":{"start":1,"end":4}}}"#
        );
        assert_eq!(
            serde_json::from_str::<TimeframeVisibility>(&json).unwrap(),
            hours
        );
    }
}
