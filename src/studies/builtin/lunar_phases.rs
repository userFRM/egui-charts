//! Lunar phases: the instants of new and full moon, and a study that marks them on the chart.
//!
//! The instants follow Jean Meeus, "Astronomical Algorithms" (2nd ed., 1998), chapter 49: a mean
//! phase from the lunation number `k`, corrected by the periodic terms of the Sun's and the
//! Moon's anomalies, the Moon's argument of latitude and the planetary arguments. The result is in
//! dynamical time and is converted to UTC with the ΔT polynomials of Espenak and Meeus.
//!
//! # Time basis, range and accuracy
//!
//! Times are UTC instants, like [`Bar::time`]. [`lunar_phases`] answers for 1900-01-01 up to but
//! excluding 2100-01-01; a request outside it is cut to that range. Meeus gives the periodic
//! terms to a few seconds, and ΔT in that range is known to a few seconds, so a phase is within
//! about one minute of the published almanac time. The calculation reads no data and does not
//! depend on how a request is split: the phases of two adjacent intervals are those of their union.
//!
//! # Study
//!
//! The study marks a phase on the bar whose interval contains it: from the bar's time up to the
//! next bar's time, and for the last bar up to its own length again. A phase before the first bar
//! or after the last bar's interval, and a phase of a bar without finite prices, is not marked; a
//! phase in a gap between bars, such as a weekend, belongs to the bar before it. A bar of more
//! than a lunar half-month holds only its first phase of each kind. Lines, in order: new moon,
//! at the low of its bar, and full moon, at the high of its bar. Because a value is a function of
//! the bar times alone, paging more history in, scrolling or restoring the study never repeats or
//! moves a phase.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::studies::{MoonPhase, lunar_phases};
//!
//! // The new moon of the total solar eclipse of 2024-04-08 was at 18:21 UTC.
//! let from = Utc.with_ymd_and_hms(2024, 4, 1, 0, 0, 0).unwrap();
//! let to = Utc.with_ymd_and_hms(2024, 4, 30, 0, 0, 0).unwrap();
//! let phases = lunar_phases(from, to);
//! assert_eq!(phases[0].phase, MoonPhase::New);
//! let almanac = Utc.with_ymd_and_hms(2024, 4, 8, 18, 21, 0).unwrap();
//! assert!((phases[0].time - almanac).num_seconds().abs() < 60);
//! assert_eq!(phases[1].phase, MoonPhase::Full);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use chrono::{DateTime, TimeZone, Utc};
use egui::Color32;

/// A phase of the Moon with a published instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoonPhase {
    /// The Moon passes between the Earth and the Sun.
    New,
    /// The Moon is opposite the Sun.
    Full,
}

/// One new or full moon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LunarPhase {
    /// Which phase.
    pub phase: MoonPhase,
    /// The instant, in UTC, rounded to the millisecond.
    pub time: DateTime<Utc>,
}

/// Julian day of the Unix epoch.
const UNIX_EPOCH_JD: f64 = 2_440_587.5;
/// Mean length of the Gregorian year in days, which locates a date among the lunations.
const GREGORIAN_YEAR_DAYS: f64 = 365.2425;

/// ΔT, dynamical time minus UT in seconds, at the decimal `year`: the polynomials of Espenak and
/// Meeus for 1900 to 2150.
fn delta_t(year: f64) -> f64 {
    let poly = |t: f64, c: &[f64]| c.iter().rev().fold(0.0, |acc, c| acc * t + c);
    match year {
        y if y < 1920.0 => poly(
            y - 1900.0,
            &[-2.79, 1.494119, -0.0598939, 0.0061966, -0.000197],
        ),
        y if y < 1941.0 => poly(y - 1920.0, &[21.20, 0.84493, -0.0761, 0.0020936]),
        y if y < 1961.0 => poly(y - 1950.0, &[29.07, 0.407, -1.0 / 233.0, 1.0 / 2547.0]),
        y if y < 1986.0 => poly(y - 1975.0, &[45.45, 1.067, -1.0 / 260.0, -1.0 / 718.0]),
        y if y < 2005.0 => poly(
            y - 2000.0,
            &[
                63.86,
                0.3345,
                -0.060374,
                0.0017275,
                0.000651814,
                0.00002373599,
            ],
        ),
        y if y < 2050.0 => poly(y - 2000.0, &[62.92, 0.32217, 0.005589]),
        y => -20.0 + 32.0 * ((y - 1820.0) / 100.0).powi(2) - 0.5628 * (2150.0 - y),
    }
}

/// The instant of the phase of lunation `k` (an integer for a new moon, a half-integer for a full
/// moon) as a Julian day in UT, after Meeus chapter 49.
fn phase_jd(k: f64) -> f64 {
    let t = k / 1236.85;
    let (t2, t3, t4) = (t * t, t * t * t, t * t * t * t);
    let full = k.fract() != 0.0;
    let jde = 2_451_550.097_66 + 29.530_588_861 * k + 0.000_154_37 * t2 - 0.000_000_150 * t3
        + 0.000_000_000_73 * t4;
    let e = 1.0 - 0.002_516 * t - 0.000_007_4 * t2;
    let rad = f64::to_radians;
    // The Sun's mean anomaly, the Moon's mean anomaly and argument of latitude, and the longitude
    // of the Moon's ascending node, in radians.
    let m = rad(2.5534 + 29.105_356_70 * k - 0.000_001_4 * t2 - 0.000_000_11 * t3);
    let mp = rad(
        201.5643 + 385.816_935_28 * k + 0.010_758_2 * t2 + 0.000_012_38 * t3 - 0.000_000_058 * t4,
    );
    let f = rad(
        160.7108 + 390.670_502_84 * k - 0.001_611_8 * t2 - 0.000_002_27 * t3 + 0.000_000_011 * t4,
    );
    let omega = rad(124.7746 - 1.563_755_88 * k + 0.002_067_2 * t2 + 0.000_002_15 * t3);
    // Coefficient, power of E, then the multiples of M', M and F in the argument. The first four
    // coefficients differ between the phases.
    let (c0, c1, c2, c3) = if full {
        (-0.40614, 0.17302, 0.01614, 0.01043)
    } else {
        (-0.40720, 0.17241, 0.01608, 0.01039)
    };
    let (c4, c5, c6) = if full {
        (0.00734, -0.00515, 0.00209)
    } else {
        (0.00739, -0.00514, 0.00208)
    };
    #[rustfmt::skip]
    let terms: [(f64, i32, f64, f64, f64); 24] = [
        (c0, 0, 1.0, 0.0, 0.0),
        (c1, 1, 0.0, 1.0, 0.0),
        (c2, 0, 2.0, 0.0, 0.0),
        (c3, 0, 0.0, 0.0, 2.0),
        (c4, 1, 1.0, -1.0, 0.0),
        (c5, 1, 1.0, 1.0, 0.0),
        (c6, 2, 0.0, 2.0, 0.0),
        (-0.00111, 0, 1.0, 0.0, -2.0),
        (-0.00057, 0, 1.0, 0.0, 2.0),
        (0.00056, 1, 2.0, 1.0, 0.0),
        (-0.00042, 0, 3.0, 0.0, 0.0),
        (0.00042, 1, 0.0, 1.0, 2.0),
        (0.00038, 1, 0.0, 1.0, -2.0),
        (-0.00024, 1, 2.0, -1.0, 0.0),
        (-0.00007, 0, 1.0, 2.0, 0.0),
        (0.00004, 0, 2.0, 0.0, -2.0),
        (0.00004, 0, 0.0, 3.0, 0.0),
        (0.00003, 0, 1.0, 1.0, -2.0),
        (0.00003, 0, 2.0, 0.0, 2.0),
        (-0.00003, 0, 1.0, 1.0, 2.0),
        (0.00003, 0, 1.0, -1.0, 2.0),
        (-0.00002, 0, 1.0, -1.0, -2.0),
        (-0.00002, 0, 3.0, 1.0, 0.0),
        (0.00002, 0, 4.0, 0.0, 0.0),
    ];
    let mut correction = -0.00017 * omega.sin();
    for (c, e_power, a, b, d) in terms {
        correction += c * e.powi(e_power) * (a * mp + b * m + d * f).sin();
    }
    // Planetary arguments in degrees: constant, per lunation, and per T squared.
    #[rustfmt::skip]
    let planetary: [(f64, f64, f64, f64); 14] = [
        (299.77, 0.107408, -0.009173, 0.000325),
        (251.88, 0.016321, 0.0, 0.000165),
        (251.83, 26.651886, 0.0, 0.000164),
        (349.42, 36.412478, 0.0, 0.000126),
        (84.66, 18.206239, 0.0, 0.000110),
        (141.74, 53.303771, 0.0, 0.000062),
        (207.14, 2.453732, 0.0, 0.000060),
        (154.84, 7.306860, 0.0, 0.000056),
        (34.52, 27.261239, 0.0, 0.000047),
        (207.19, 0.121824, 0.0, 0.000042),
        (291.34, 1.844379, 0.0, 0.000040),
        (161.72, 24.198154, 0.0, 0.000037),
        (239.56, 25.513099, 0.0, 0.000035),
        (331.55, 3.592518, 0.0, 0.000023),
    ];
    for (base, per_lunation, per_t2, amplitude) in planetary {
        correction += amplitude * rad(base + per_lunation * k + per_t2 * t2).sin();
    }
    let jde = jde + correction;
    let year = 2000.0 + (jde - 2_451_545.0) / GREGORIAN_YEAR_DAYS;
    jde - delta_t(year) / 86_400.0
}

/// The new and full moons in `from..to`, in time order, over 1900-01-01 to 2100-01-01 UTC; see the
/// module documentation for the time basis and accuracy.
pub fn lunar_phases(from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<LunarPhase> {
    let from = from.max(Utc.with_ymd_and_hms(1900, 1, 1, 0, 0, 0).unwrap());
    let to = to.min(Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap());
    // A lunation near a `from` beyond the range could fall outside the representable times.
    if from >= to {
        return Vec::new();
    }
    let at = |ms: f64| Utc.timestamp_millis_opt(ms.round() as i64).unwrap();
    // Lunations are counted in halves, the new moons at the even ones; start a lunation early.
    let year = 1970.0 + from.timestamp() as f64 / 86_400.0 / GREGORIAN_YEAR_DAYS;
    let mut half = ((year - 2000.0) * 12.3685 * 2.0).floor() as i64 - 2;
    let mut phases = Vec::new();
    loop {
        let jd = phase_jd(half as f64 / 2.0);
        let time = at((jd - UNIX_EPOCH_JD) * 86_400_000.0);
        if time >= to {
            return phases;
        }
        if time >= from {
            let phase = if half % 2 == 0 {
                MoonPhase::New
            } else {
                MoonPhase::Full
            };
            phases.push(LunarPhase { phase, time });
        }
        half += 1;
    }
}

/// Lunar phases: a marker at the low of the bar holding each new moon and at the high of the bar
/// holding each full moon.
#[derive(Clone)]
pub struct LunarPhases {
    values: IndicatorValues,
    new_color: Color32,
    full_color: Color32,
    visible: bool,
}

impl LunarPhases {
    /// Create the study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            new_color: palette::BLUE.gamma_multiply_u8(128),
            full_color: palette::WHITE.gamma_multiply_u8(128),
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::missing(data.len());
        if !data.is_empty() {
            // Both columns exist, so a bar with one phase still has a value per line.
            values.set_value(1, 0, f64::NAN);
        }
        let [.., before, last] = data else {
            return values;
        };
        let end = last.time + (last.time - before.time);
        for LunarPhase { phase, time } in lunar_phases(data[0].time, end) {
            let bar = data.partition_point(|b| b.time <= time) - 1;
            let (line, price) = match phase {
                MoonPhase::New => (0, data[bar].low),
                MoonPhase::Full => (1, data[bar].high),
            };
            if values.value(line, bar).is_none() {
                values.set_value(line, bar, price);
            }
        }
        values
    }
}

impl Default for LunarPhases {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for LunarPhases {
    fn name(&self) -> &str {
        "Lunar Phases"
    }

    fn desc(&self) -> &str {
        "Lunar Phases - New and full moons marked on the bars that hold them"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        let finite = |b: &Bar| b.high.is_finite() && b.low.is_finite();
        // The last usable bar before the change owns the phases up to the first changed bar, and
        // the bar before it gives the length of the last bar.
        let from = usable_behind(1, finite)(data, from.min(values.len()).min(data.len()));
        update_window(&mut values, data, from, usable_behind(1, finite), |data| {
            over_finite_bars(data, finite, Self::compute)
        });
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: if line == 0 {
                self.new_color
            } else {
                self.full_color
            },
            width: 2.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Circles,
            visible: true,
        }
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        self.values
            .value(line, bar)
            .map(|_| if line == 0 { "New" } else { "Full" }.to_owned())
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.new_color, self.full_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.new_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.full_color = color;
        }
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec!["New Moon".to_string(), "Full Moon".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    /// The phases agree with the minute of published almanac times, and with the example of Meeus
    /// chapter 49, the new moon of 1977-02-18 at 03:37:42 in dynamical time, at the edges of the
    /// supported range too.
    #[test]
    fn phases_match_published_times() {
        use MoonPhase::{Full, New};
        let published = [
            (New, utc(1977, 2, 18, 3, 36)),
            (New, utc(2000, 1, 6, 18, 14)),
            (Full, utc(2000, 1, 21, 4, 40)),
            (New, utc(2024, 1, 11, 11, 57)),
            (Full, utc(2024, 1, 25, 17, 54)),
            (Full, utc(2024, 3, 25, 7, 0)),
            (New, utc(2024, 4, 8, 18, 21)),
            (Full, utc(2024, 4, 23, 23, 49)),
        ];
        for (phase, expected) in published {
            let found = lunar_phases(
                expected - chrono::Duration::days(1),
                expected + chrono::Duration::days(1),
            );
            assert_eq!(found.len(), 1, "{expected}");
            assert_eq!(found[0].phase, phase, "{expected}");
            // The almanac gives the minute, which the phase is within.
            assert!(
                (found[0].time - expected).num_seconds().abs() <= 60,
                "{expected}"
            );
        }
        // Adjacent intervals give the phases of their union.
        let cut = utc(2024, 2, 10, 0, 0);
        let mut split = lunar_phases(utc(2024, 1, 1, 0, 0), cut);
        split.extend(lunar_phases(cut, utc(2024, 4, 1, 0, 0)));
        assert_eq!(
            split,
            lunar_phases(utc(2024, 1, 1, 0, 0), utc(2024, 4, 1, 0, 0))
        );
        assert!(lunar_phases(utc(1899, 1, 1, 0, 0), utc(1900, 1, 1, 0, 0)).is_empty());
        assert!(lunar_phases(utc(2100, 1, 1, 0, 0), utc(2101, 1, 1, 0, 0)).is_empty());
        assert!(lunar_phases(DateTime::<Utc>::MAX_UTC, DateTime::<Utc>::MAX_UTC).is_empty());
    }

    /// The study marks a phase on the bar holding it, new moons at the low and full moons at the
    /// high, and a series with earlier bars loaded in front or later bars cut off marks the same
    /// bars, once each.
    #[test]
    fn study_marks_the_bar_holding_each_phase_once() {
        // Weekday bars from 2024-01-08; the new moon of Thursday 11 January at 11:57 and the full
        // moon of Thursday 25 January at 17:54 fall on their own bars.
        let bars: Vec<Bar> = (0..24)
            .map(|i| utc(2024, 1, 8, 0, 0) + chrono::Duration::days(i))
            .filter(|t| t.weekday().number_from_monday() <= 5)
            .map(|t| Bar::new(t, 10.0, 12.0, 8.0, 11.0, 1.0))
            .collect();
        let mut study = LunarPhases::new();
        study.calculate(&bars);
        let marked =
            |study: &LunarPhases, bars: &[Bar], line: usize| -> Vec<(DateTime<Utc>, f64)> {
                bars.iter()
                    .enumerate()
                    .filter_map(|(i, b)| study.values().value(line, i).map(|v| (b.time, v)))
                    .collect()
            };
        assert_eq!(marked(&study, &bars, 0), [(utc(2024, 1, 11, 0, 0), 8.0)]);
        assert_eq!(marked(&study, &bars, 1), [(utc(2024, 1, 25, 0, 0), 12.0)]);
        assert_eq!(study.plot_text(1, 13).as_deref(), Some("Full"));
        let mut tail = LunarPhases::new();
        tail.calculate(&bars[2..]);
        for line in 0..2 {
            assert_eq!(marked(&tail, &bars[2..], line), marked(&study, &bars, line));
        }
        // A later bar moves the end of the last bar's interval, not the marks.
        let mut longer = bars.clone();
        longer.push(Bar::new(utc(2024, 2, 1, 0, 0), 10.0, 12.0, 8.0, 11.0, 1.0));
        study.update(&longer, bars.len());
        let mut fresh = LunarPhases::new();
        fresh.calculate(&longer);
        for line in 0..2 {
            assert_eq!(marked(&study, &longer, line), marked(&fresh, &longer, line));
        }
    }
}
