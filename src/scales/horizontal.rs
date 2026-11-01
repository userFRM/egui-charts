//! Horizontal scales: what the horizontal position of a bar stands for and how it is labelled.
//!
//! Bars are placed left to right in the order of their [`time`](crate::model::Bar::time), one
//! slot each. On the time scale that order is chronological. A [`HorizontalScale`] gives it
//! another meaning, such as option strikes or bond maturities: the host encodes each key as an
//! instant that sorts as the key does, and the scale labels the axis and the crosshair from it.
//! [`key_time`] and [`time_key`] encode numeric keys, which [`PriceHorizontalScale`] and
//! [`MaturityScale`] use.

use super::time_formatter::TimeFormatter;
use super::timescale_marks::{
    TickMark, TickMarkGenerator, TickMarkGeneratorConfig, TickMarkType, TickMarkWeight, keep_spaced,
};
use crate::model::Bar;
use chrono::{DateTime, Duration, Utc};

/// Where the tick marks of a horizontal axis are laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickMarkLayout {
    /// Plot width in points.
    pub width: f32,
    /// Distance between neighbouring bars in points.
    pub bar_spacing: f32,
    /// Minimum distance between the centres of two labels in points.
    pub min_spacing: f32,
    /// Whether marks of one weight are shown all or none, so labels of one kind stay evenly
    /// spaced.
    pub uniform_distribution: bool,
}

/// What the keys of the horizontal axis mean: which of them are labelled, how, and how the
/// crosshair label reads.
///
/// A key is the [`time`](crate::model::Bar::time) of a bar. The time-based chart uses its
/// [`TimeFormatter`]; `Chart::set_horizontal_scale` (requires `widget`)
/// replaces it.
pub trait HorizontalScale: Send + Sync {
    /// The marks to label among `points`, the key and logical index of each bar in view in
    /// order, extended into whitespace on both sides using the chart's full-history time
    /// mapping. When the keys reach before the first bar, every index is shifted by the same
    /// number of slots, so only the order of the indices and their differences carry meaning.
    /// The renderer places each mark at its key and skips labels that would overlap.
    fn tick_marks(
        &self,
        points: &[(DateTime<Utc>, usize)],
        layout: &TickMarkLayout,
    ) -> Vec<TickMark>;

    /// The crosshair label of `key`. `bar_seconds` is the duration of the bars in view, which
    /// the time scale uses for the precision of the clock.
    fn format_crosshair(&self, key: DateTime<Utc>, bar_seconds: i64) -> String;
}

/// The time scale: calendar and clock boundaries in the formatter's zone.
impl HorizontalScale for Box<dyn TimeFormatter> {
    fn tick_marks(
        &self,
        points: &[(DateTime<Utc>, usize)],
        layout: &TickMarkLayout,
    ) -> Vec<TickMark> {
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return Vec::new();
        };
        // The median controls clock precision; point timestamps already carry the chart mapping.
        let bar_duration = median_bar_duration(points.iter().map(|point| point.0));
        let zoom_factor = (layout.bar_spacing / 8.0).clamp(0.5, 2.5);
        let config = TickMarkGeneratorConfig {
            min_spacing: layout.min_spacing,
            max_marks: ((layout.width / layout.min_spacing).ceil() as usize + 2).clamp(6, 24),
            show_subseconds: bar_duration < Duration::seconds(1),
            use_24_hour: true,
            target_density: (1.0 * zoom_factor).clamp(0.6, 2.5),
        };
        TickMarkGenerator::with_formatter(config, (**self).clone_box())
            .with_uniform_distribution(layout.uniform_distribution)
            .generate_marks(first.0, last.0, layout.width, points)
    }

    fn format_crosshair(&self, key: DateTime<Utc>, bar_seconds: i64) -> String {
        (**self).format_crosshair(key, bar_seconds)
    }
}

/// The median positive gap between successive times, ignoring repeated keys and defaulting
/// to a minute when there is no gap, as the time axis does for a single key.
pub(crate) fn median_bar_duration(times: impl Iterator<Item = DateTime<Utc>>) -> Duration {
    let mut previous = None;
    let mut deltas_ms: Vec<_> = times
        .filter_map(|time| {
            let delta = (time - previous.replace(time)?).num_milliseconds().abs();
            (delta > 0).then_some(delta)
        })
        .collect();
    deltas_ms.sort_unstable();
    Duration::milliseconds(
        deltas_ms
            .get(deltas_ms.len() / 2)
            .copied()
            .unwrap_or(60_000),
    )
}

/// The bar time standing for a numeric key on a horizontal scale other than time: the key as
/// seconds from the Unix epoch, to the nanosecond, so times sort as their keys do.
///
/// `None` for a key that is not finite or lies beyond about ±9.2e9.
pub fn key_time(key: f64) -> Option<DateTime<Utc>> {
    let nanos = (key * 1e9).round();
    (nanos.abs() < i64::MAX as f64).then(|| DateTime::from_timestamp_nanos(nanos as i64))
}

/// The numeric key a bar time stands for, the inverse of [`key_time`].
pub fn time_key(time: DateTime<Utc>) -> f64 {
    time.timestamp() as f64 + f64::from(time.timestamp_subsec_nanos()) / 1e9
}

/// A horizontal scale of prices, such as the strikes of an option chain, encoded with
/// [`key_time`].
///
/// The axis labels the roundest prices in view first: multiples of 100, then of 50, 25, 10, 5
/// and 1, then half prices, and draws the roundest kind shown in bold. Labels and the crosshair
/// show `precision` decimals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceHorizontalScale {
    /// Decimals of the labels.
    pub precision: usize,
}

impl Default for PriceHorizontalScale {
    fn default() -> Self {
        Self { precision: 2 }
    }
}

impl HorizontalScale for PriceHorizontalScale {
    fn tick_marks(
        &self,
        points: &[(DateTime<Utc>, usize)],
        layout: &TickMarkLayout,
    ) -> Vec<TickMark> {
        let weight = |price: f64| {
            [
                (100.0, 8),
                (50.0, 7),
                (25.0, 6),
                (10.0, 5),
                (5.0, 4),
                (1.0, 3),
                (0.5, 1),
            ]
            .into_iter()
            .find(|(step, _)| price % step == 0.0)
            .map_or(0, |(_, weight)| weight)
        };
        weighted_marks(points, layout, weight, |price| {
            format!("{price:.*}", self.precision)
        })
    }

    fn format_crosshair(&self, key: DateTime<Utc>, _bar_seconds: i64) -> String {
        format!("{:.*}", self.precision, time_key(key))
    }
}

/// A horizontal scale of maturities in months, for yield curves, encoded with [`key_time`].
///
/// Maturities read `3M` below a year and `2Y` or `2Y6M` from a year on. The axis labels
/// multiples of 10 years first, then of 5 and 3 years, whole years, half years, quarters and
/// months, and draws the longest kind shown in bold.
///
/// The bars of a chart take one slot each whatever their keys, so a curve spaced by maturity
/// needs a slot every month: give the main series the [`whitespace`](Self::whitespace) bars and
/// draw each curve as a series of its own, placed on them by key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaturityScale {
    /// Months between neighbouring slots.
    pub base_resolution: u32,
    /// Months the whitespace spans at least, from `start`.
    pub minimum_range: u32,
    /// Maturity of the first slot in months.
    pub start: u32,
}

impl Default for MaturityScale {
    /// Monthly slots over the first ten years.
    fn default() -> Self {
        Self {
            base_resolution: 1,
            minimum_range: 120,
            start: 0,
        }
    }
}

impl MaturityScale {
    /// Whitespace bars every `base_resolution` months from `start` up to `last`, the longest
    /// maturity on the chart, or up to `minimum_range` when that is later.
    /// Non-finite `last` values fall back to `minimum_range`; finite values are capped at
    /// 12,000 months (1,000 years) before comparison, without limiting `minimum_range`.
    pub fn whitespace(&self, last: f64) -> Vec<Bar> {
        let step = f64::from(self.base_resolution.max(1));
        let minimum = f64::from(self.minimum_range);
        let end = if last.is_finite() {
            minimum.max(last.min(12_000.0))
        } else {
            minimum
        };
        (0..)
            .map(|slot| f64::from(self.start) + f64::from(slot) * step)
            .take_while(|months| *months <= end)
            .filter_map(key_time)
            .map(Bar::whitespace)
            .collect()
    }

    fn format(months: f64) -> String {
        let (years, rest) = ((months / 12.0).floor(), months % 12.0);
        if months < 12.0 {
            format!("{months}M")
        } else if rest == 0.0 {
            format!("{years}Y")
        } else {
            format!("{years}Y{rest}M")
        }
    }
}

impl HorizontalScale for MaturityScale {
    fn tick_marks(
        &self,
        points: &[(DateTime<Utc>, usize)],
        layout: &TickMarkLayout,
    ) -> Vec<TickMark> {
        let weight = |months: f64| {
            [
                (120.0, 10),
                (60.0, 9),
                (36.0, 8),
                (12.0, 7),
                (6.0, 6),
                (3.0, 5),
                (1.0, 4),
            ]
            .into_iter()
            .find(|(step, _)| months % step == 0.0)
            .map_or(0, |(_, weight)| weight)
        };
        weighted_marks(points, layout, weight, Self::format)
    }

    fn format_crosshair(&self, key: DateTime<Utc>, _bar_seconds: i64) -> String {
        Self::format(time_key(key))
    }
}

/// Marks every point with the `weight` of its numeric key and keeps the heaviest marks that lie
/// the minimum spacing apart, as the time scale does. The heaviest kept marks are bold.
fn weighted_marks(
    points: &[(DateTime<Utc>, usize)],
    layout: &TickMarkLayout,
    weight: impl Fn(f64) -> u8,
    label: impl Fn(f64) -> String,
) -> Vec<TickMark> {
    let marks = points
        .iter()
        .map(|&(time, index)| TickMark {
            time,
            mark_type: TickMarkType::Time,
            weight: TickMarkWeight::new(weight(time_key(time))),
            label: String::new(),
            index,
            bold: false,
        })
        .collect();
    let mut kept = keep_spaced(
        marks,
        layout.min_spacing / layout.bar_spacing,
        layout.uniform_distribution,
    );
    let heaviest = kept.iter().map(|mark| mark.weight).max();
    for mark in &mut kept {
        mark.bold = Some(mark.weight) == heaviest;
        mark.label = label(time_key(mark.time));
    }
    kept.sort_by_key(|mark| mark.index);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected marks follow from the published weights by hand: a mark needs the minimum
    /// spacing over the bar spacing, rounded up, in bars to its kept neighbours.
    #[test]
    fn numeric_scales_encode_keys_and_label_the_roundest_in_reach() {
        for key in [4512.5, 101.1, -0.5, 0.0, 360.0] {
            assert_eq!(
                key_time(key).map(time_key),
                Some(key),
                "round trip of {key}"
            );
        }
        for key in [f64::NAN, f64::INFINITY, 1e10] {
            assert_eq!(key_time(key), None, "{key}");
        }

        let price = PriceHorizontalScale { precision: 0 };
        let strikes = [4400.0, 4450.0, 4500.0, 4525.0, 4550.0, 4575.0, 4600.0];
        let months: Vec<f64> = (0..=36).map(f64::from).collect();
        // Scale, keys, bar spacing, label spacing, uniform distribution, labels and bold labels.
        type Case<'a> = (
            &'a dyn HorizontalScale,
            &'a [f64],
            f32,
            f32,
            bool,
            &'a [&'a str],
            &'a [&'a str],
        );
        let cases: [Case; 3] = [
            // Two bars apart: the multiples of 100, then 4550 between 4500 and 4600, while
            // 4450 sits beside 4400.
            (
                &price,
                &strikes,
                40.0,
                80.0,
                false,
                &["4400", "4500", "4550", "4600"],
                &["4400", "4500", "4600"],
            ),
            // 4450 not fitting leaves out every multiple of 50.
            (
                &price,
                &strikes,
                40.0,
                80.0,
                true,
                &["4400", "4500", "4600"],
                &["4400", "4500", "4600"],
            ),
            // Six bars apart: 0 (10 years), 36 (3 years), whole years, then half years.
            (
                &MaturityScale::default(),
                &months,
                10.0,
                60.0,
                false,
                &["0M", "6M", "1Y", "1Y6M", "2Y", "2Y6M", "3Y"],
                &["0M"],
            ),
        ];
        for (scale, keys, bar_spacing, min_spacing, uniform_distribution, labels, bold) in cases {
            let points: Vec<_> = keys
                .iter()
                .enumerate()
                .map(|(index, &key)| (key_time(key).unwrap(), index))
                .collect();
            let layout = TickMarkLayout {
                width: 800.0,
                bar_spacing,
                min_spacing,
                uniform_distribution,
            };
            let marks = scale.tick_marks(&points, &layout);
            let shown: Vec<_> = marks.iter().map(|mark| mark.label.as_str()).collect();
            assert_eq!(shown, labels);
            let heavy: Vec<_> = marks
                .iter()
                .filter(|mark| mark.bold)
                .map(|mark| mark.label.as_str())
                .collect();
            assert_eq!(heavy, bold);
        }

        let at = |key| key_time(key).unwrap();
        assert_eq!(
            PriceHorizontalScale::default().format_crosshair(at(4512.5), 0),
            "4512.50"
        );
        let maturity = MaturityScale::default();
        for (months, label) in [(3.0, "3M"), (30.0, "2Y6M"), (360.0, "30Y")] {
            assert_eq!(maturity.format_crosshair(at(months), 0), label);
        }

        let mut quarterly = MaturityScale {
            base_resolution: 3,
            minimum_range: 12,
            start: 0,
        };
        // Inclusive quarterly slots: 0 through 12,000 has 4,001 bars; 24,000 has 8,001.
        for (minimum, last, slots) in [
            (12, 7.0, 12.0),
            (12, 20.0, 18.0),
            (12, 12_003.0, 12_000.0),
            (12, 1e12, 12_000.0),
            (12, f64::INFINITY, 12.0),
            (12, f64::NAN, 12.0),
            (24_000, 1e12, 24_000.0),
        ] {
            quarterly.minimum_range = minimum;
            let expected: Vec<_> = (0..=(slots as u32 / 3))
                .map(|slot| at(f64::from(slot * 3)))
                .collect();
            let times: Vec<_> = quarterly
                .whitespace(last)
                .iter()
                .map(|bar| bar.time)
                .collect();
            assert_eq!(times.len(), expected.len(), "whitespace to {last}");
            assert_eq!(times, expected, "whitespace to {last}");
        }
    }
}
