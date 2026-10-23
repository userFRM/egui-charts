//! Volume Weighted Average Price (VWAP) Indicator
//!
//! VWAP is the average price weighted by volume over an anchor period: a trading
//! session, a week or a month. It restarts at the first bar of each period and is
//! commonly used as an execution benchmark.
//!
//! # Interpretation
//! - Price above VWAP: Bullish bias
//! - Price below VWAP: Bearish bias
//! - VWAP can act as support/resistance
//! - Institutional traders often use VWAP as execution benchmark
//!
//! # Calculation
//! VWAP = Sum(Typical Price * Volume) / Sum(Volume) since the start of the period
//! Typical Price = (High + Low + Close) / 3
//! Band = VWAP +/- k * sigma, where sigma^2 = Sum(Volume * Typical Price^2) / Sum(Volume) - VWAP^2
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use chrono::{NaiveTime, Weekday};
//! use egui_charts::model::{Session, SessionWindow, TimezoneId};
//! use egui_charts::studies::{Indicator, VolumeWeightedAvgPrice};
//!
//! // Sessions from 17:00 to 16:00 Chicago time, each named by the day it closes.
//! let weekdays = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri];
//! let hour = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
//! let mut vwap = VolumeWeightedAvgPrice::new()
//!     .with_time_zone(TimezoneId::named("America/Chicago").unwrap())
//!     .with_session(Session::new([SessionWindow::new(weekdays, hour(17), hour(16))]))
//!     .with_std_dev_bands([1.0, 2.0]);
//! vwap.calculate(&bars);
//! assert_eq!(vwap.line_cnt(), 5);
//! ```

use super::clamp_param;
use crate::model::{Bar, Session, SessionWindow, TimezoneId};
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use egui::Color32;

/// The period over which [`VolumeWeightedAvgPrice`] accumulates before it restarts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum VwapAnchor {
    /// Each trading day.
    #[default]
    Session,
    /// Each ISO week, Monday to Sunday, of trading days.
    Week,
    /// Each calendar month of trading days.
    Month,
}

impl VwapAnchor {
    /// Identifies the period that holds trading day `day`.
    fn period(self, day: NaiveDate) -> (i32, u32) {
        match self {
            Self::Session => (day.year(), day.ordinal()),
            Self::Week => (day.iso_week().year(), day.iso_week().week()),
            Self::Month => (day.year(), day.month()),
        }
    }
}

/// The trading day of `time`: its date in `zone`, or the next date from the open of a
/// window that spans midnight, since a window is named by the day on which it closes.
fn trading_day(time: DateTime<Utc>, zone: Tz, windows: &[SessionWindow]) -> NaiveDate {
    let utc = time.naive_utc();
    // Near the ends of the representable range the local time may not exist; keep UTC there.
    let local = utc
        .checked_add_offset(zone.offset_from_utc_datetime(&utc).fix())
        .unwrap_or(utc);
    let date = local.date();
    // A window from 00:00 to 00:00 lies within its opening day.
    if windows
        .iter()
        .any(|w| w.close <= w.open && w.open > NaiveTime::MIN && local.time() >= w.open)
    {
        date.succ_opt().unwrap_or(date)
    } else {
        date
    }
}

/// Volume Weighted Average Price, restarted at each anchor period, with optional
/// standard deviation bands.
///
/// [`new`](Self::new) restarts at each UTC day until a time zone is set with
/// [`with_time_zone`](Self::with_time_zone). Pass the instrument's
/// [`time_zone`](crate::model::Instrument::time_zone) and
/// [`session`](crate::model::Instrument::session) so that sessions which cross
/// midnight in the exchange zone, or UTC midnight, stay one period.
#[derive(Clone)]
pub struct VolumeWeightedAvgPrice {
    values: Vec<IndicatorValue>,
    /// One per line: the VWAP, then the upper and lower band of each multiplier.
    colors: Vec<Color32>,
    visible: bool,
    anchor: VwapAnchor,
    time_zone: TimezoneId,
    windows: Vec<SessionWindow>,
    bands: Vec<f64>,
}

impl VolumeWeightedAvgPrice {
    /// A VWAP restarted at each session, in UTC with no session windows, without bands.
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.favorite_gold], // Amber
            visible: true,
            anchor: VwapAnchor::Session,
            time_zone: TimezoneId::utc(),
            windows: Vec::new(),
            bands: Vec::new(),
        }
    }

    /// Restarts at each session, week or month.
    pub fn with_anchor(mut self, anchor: VwapAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// The exchange time zone, in which trading days are counted. A bar counts on the
    /// exchange date of its timestamp, so a daily bar stamped at UTC midnight falls on
    /// the previous date west of UTC.
    pub fn with_time_zone(mut self, time_zone: TimezoneId) -> Self {
        self.time_zone = time_zone;
        self
    }

    /// The trading hours, in exchange time. A window that spans midnight starts the
    /// next trading day at its open.
    pub fn with_session(mut self, session: Session) -> Self {
        self.windows = session.windows;
        self
    }

    /// Adds an upper and a lower band at each multiple of the volume-weighted
    /// standard deviation of the typical price over the period. Multipliers clamp
    /// to 0 or more, NaN to 0. The bands are drawn in the VWAP colour at half intensity.
    pub fn with_std_dev_bands(mut self, multipliers: impl IntoIterator<Item = f64>) -> Self {
        self.bands = multipliers
            .into_iter()
            .map(|k| clamp_param(k, 0.0, f64::MAX))
            .collect();
        let band_color = self.colors[0].gamma_multiply(0.5);
        self.colors.truncate(1);
        self.colors.resize(self.line_cnt(), band_color);
        self
    }

    /// Set the VWAP line color; the bands take it at half intensity.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.colors.fill(color.gamma_multiply(0.5));
        self.colors[0] = color;
        self
    }
}

impl Default for VolumeWeightedAvgPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VolumeWeightedAvgPrice {
    fn name(&self) -> &str {
        "VWAP"
    }

    fn desc(&self) -> &str {
        "Volume Weighted Avg Price - Trading benchmark indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        let zone = self.time_zone.tz();
        let mut period = None;
        let (mut volume, mut tp_volume, mut tp2_volume) = (0.0, 0.0, 0.0);

        for bar in data {
            // A bar with a non-finite price or volume has no value and adds nothing.
            if ![bar.high, bar.low, bar.close, bar.volume]
                .iter()
                .all(|x| x.is_finite())
            {
                self.values.push(IndicatorValue::None);
                continue;
            }
            let current = Some(
                self.anchor
                    .period(trading_day(bar.time, zone, &self.windows)),
            );
            if current != period {
                period = current;
                (volume, tp_volume, tp2_volume) = (0.0, 0.0, 0.0);
            }

            let tp = bar.typical_price();
            volume += bar.volume;
            tp_volume += tp * bar.volume;
            tp2_volume += tp * tp * bar.volume;

            let (vwap, sigma) = if volume.abs() < 1e-10 {
                (tp, 0.0)
            } else {
                let vwap = tp_volume / volume;
                // Clamped: cancellation can leave a tiny negative variance.
                (vwap, (tp2_volume / volume - vwap * vwap).max(0.0).sqrt())
            };
            self.values.push(if self.bands.is_empty() {
                IndicatorValue::Single(vwap)
            } else {
                IndicatorValue::Multiple(
                    std::iter::once(vwap)
                        .chain(
                            self.bands
                                .iter()
                                .flat_map(|k| [vwap + k * sigma, vwap - k * sigma]),
                        )
                        .collect(),
                )
            });
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    /// Sets the colors line by line; lines without a color keep theirs.
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
        }
    }

    fn is_overlay(&self) -> bool {
        true // VWAP is drawn on the main chart
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

    fn line_cnt(&self) -> usize {
        1 + 2 * self.bands.len()
    }

    fn line_names(&self) -> Vec<String> {
        std::iter::once("VWAP".to_string())
            .chain(
                self.bands
                    .iter()
                    .flat_map(|k| [format!("Upper({k})"), format!("Lower({k})")]),
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDateTime, NaiveTime, TimeZone, Weekday};

    struct Case {
        name: &'static str,
        /// `None` keeps the UTC default of `new()`.
        zone: Option<&'static str>,
        /// Open and close hour of one window, Monday to Friday.
        window: Option<(u32, u32)>,
        anchor: VwapAnchor,
        bands: &'static [f64],
        /// Exchange-local time, then high, low, close and volume.
        bars: &'static [(&'static str, [f64; 4])],
        /// VWAP, then the upper and lower band of each multiplier.
        want: &'static [&'static [f64]],
    }

    /// Expected values come from the anchored VWAP definition, sum(tp * v) / sum(v) with
    /// tp = (h + l + c) / 3 restarted at each period (Berkowitz, Logue and Noser, "The Total
    /// Cost of Transactions on the NYSE", Journal of Finance 43(1), 1988), with bands at k
    /// volume-weighted population standard deviations of tp, and trading days named by the
    /// day on which their session window closes in the exchange zone. They are computed
    /// independently with exact fractions.
    #[test]
    fn calculate_matches_reference() {
        let cases = [
            Case {
                name: "new() restarts at UTC midnight",
                zone: None,
                window: None,
                anchor: VwapAnchor::Session,
                bands: &[],
                bars: &[
                    ("2026-03-02 22:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-02 23:00", [103.0, 100.0, 102.0, 2000.0]),
                    ("2026-03-03 00:00", [104.0, 101.0, 103.0, 1500.0]),
                    ("2026-03-03 01:00", [102.0, 100.0, 101.0, 500.0]),
                ],
                want: &[
                    &[100.0],
                    &[101.11111111111111],
                    &[102.66666666666667],
                    &[102.25],
                ],
            },
            Case {
                // The 10:00 open is 23:00 UTC on the previous day.
                name: "Sydney session across UTC midnight",
                zone: Some("Australia/Sydney"),
                window: Some((10, 16)),
                anchor: VwapAnchor::Session,
                bands: &[],
                bars: &[
                    ("2026-03-02 15:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-03 10:00", [103.0, 101.0, 102.0, 2000.0]),
                    ("2026-03-03 11:00", [104.0, 102.0, 103.0, 1500.0]),
                    ("2026-03-03 12:00", [102.0, 100.0, 101.0, 800.0]),
                ],
                want: &[
                    &[100.0],
                    &[102.0],
                    &[102.42857142857143],
                    &[102.16279069767442],
                ],
            },
            Case {
                // The 17:00 open starts the next trading day; UTC midnight is 18:00.
                name: "Chicago session across local midnight",
                zone: Some("America/Chicago"),
                window: Some((17, 16)),
                anchor: VwapAnchor::Session,
                bands: &[],
                bars: &[
                    ("2026-03-02 14:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-02 15:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-03-02 17:00", [103.0, 101.0, 102.0, 900.0]),
                    ("2026-03-02 18:00", [104.0, 102.0, 103.0, 1100.0]),
                    ("2026-03-03 09:00", [105.0, 103.0, 104.0, 700.0]),
                ],
                want: &[
                    &[100.0],
                    &[100.54545454545455],
                    &[102.0],
                    &[102.55],
                    &[102.92592592592592],
                ],
            },
            Case {
                // Sunday 17:00, the day daylight saving starts, opens Monday's session.
                name: "Chicago week",
                zone: Some("America/Chicago"),
                window: Some((17, 16)),
                anchor: VwapAnchor::Week,
                bands: &[],
                bars: &[
                    ("2026-03-05 15:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-06 15:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-03-08 17:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2026-03-09 17:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[&[100.0], &[100.54545454545455], &[98.0], &[98.55]],
            },
            Case {
                name: "UTC month",
                zone: None,
                window: None,
                anchor: VwapAnchor::Month,
                bands: &[],
                bars: &[
                    ("2026-01-30 12:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-01-31 12:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-02-01 12:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2026-02-02 12:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[&[100.0], &[100.54545454545455], &[98.0], &[98.55]],
            },
            Case {
                // A 24-hour session from 00:00 to 00:00 keeps each bar on its own date.
                name: "UTC month with a 24-hour session",
                zone: None,
                window: Some((0, 0)),
                anchor: VwapAnchor::Month,
                bands: &[],
                bars: &[
                    ("2026-01-30 12:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-01-31 12:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-02-01 12:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2026-02-02 12:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[&[100.0], &[100.54545454545455], &[98.0], &[98.55]],
            },
            Case {
                // The second day opens on a bar without volume: its typical price.
                name: "bands",
                zone: None,
                window: None,
                anchor: VwapAnchor::Session,
                bands: &[1.0, 2.5],
                bars: &[
                    ("2026-03-02 10:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-02 11:00", [104.0, 100.0, 102.0, 2000.0]),
                    ("2026-03-02 12:00", [103.0, 97.0, 99.0, 1500.0]),
                    ("2026-03-03 10:00", [102.0, 100.0, 101.0, 0.0]),
                    ("2026-03-03 11:00", [104.0, 102.0, 103.0, 500.0]),
                ],
                want: &[
                    &[100.0, 100.0, 100.0, 100.0, 100.0],
                    &[
                        101.33333333333333,
                        102.2761423749154,
                        100.39052429175128,
                        103.69035593728849,
                        98.97631072937817,
                    ],
                    &[
                        100.77777777777777,
                        101.87772165962352,
                        99.67783389593204,
                        103.52763748239212,
                        98.02791807316342,
                    ],
                    &[101.0, 101.0, 101.0, 101.0, 101.0],
                    &[103.0, 103.0, 103.0, 103.0, 103.0],
                ],
            },
        ];

        let weekdays = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];
        let hour = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
        let mut failures = Vec::new();
        for case in cases {
            let zone: Tz = case.zone.unwrap_or("UTC").parse().unwrap();
            let bars: Vec<Bar> = case
                .bars
                .iter()
                .map(|(local, [h, l, c, v])| {
                    let local = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M").unwrap();
                    let time = zone.from_local_datetime(&local).single().unwrap();
                    Bar::new(time.with_timezone(&Utc), *c, *h, *l, *c, *v)
                })
                .collect();
            let mut study = VolumeWeightedAvgPrice::new();
            if let Some(name) = case.zone {
                study = study.with_time_zone(TimezoneId::named(name).unwrap());
            }
            if let Some((open, close)) = case.window {
                let window = SessionWindow::new(weekdays, hour(open), hour(close));
                study = study.with_session(Session::new([window]));
            }
            if case.anchor != VwapAnchor::default() {
                study = study.with_anchor(case.anchor);
            }
            study = study.with_std_dev_bands(case.bands.iter().copied());
            study.calculate(&bars);

            let lines = 1 + 2 * case.bands.len();
            assert_eq!(study.line_cnt(), lines, "{}", case.name);
            assert_eq!(study.line_names().len(), lines, "{}", case.name);
            assert_eq!(study.colors().len(), lines, "{}", case.name);
            assert_eq!(study.values().len(), case.want.len(), "{}", case.name);
            for (i, (got, want)) in study.values().iter().zip(case.want).enumerate() {
                let got = match got {
                    IndicatorValue::Single(v) if case.bands.is_empty() => vec![*v],
                    IndicatorValue::Multiple(vs) if !case.bands.is_empty() => vs.clone(),
                    other => panic!("{} bar {i}: {other:?}", case.name),
                };
                let close = got.len() == want.len()
                    && got
                        .iter()
                        .zip(*want)
                        .all(|(g, w)| (g - w).abs() <= 1e-12 * w.abs());
                if !close {
                    failures.push(format!("{} bar {i}: got {got:?}, want {want:?}", case.name));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// The local time of a bar at either end of the representable range is out of range
    /// in a zone east or west of UTC; `calculate` must still not panic.
    #[test]
    fn extreme_bar_times_do_not_panic_in_a_zone() {
        for (zone, time) in [
            ("Australia/Sydney", DateTime::<Utc>::MAX_UTC),
            ("America/Chicago", DateTime::<Utc>::MIN_UTC),
        ] {
            let mut study =
                VolumeWeightedAvgPrice::new().with_time_zone(TimezoneId::named(zone).unwrap());
            study.calculate(&[Bar::new(time, 1.0, 1.0, 1.0, 1.0, 1.0)]);
            assert_eq!(study.values().len(), 1, "{zone}");
        }
    }
}
