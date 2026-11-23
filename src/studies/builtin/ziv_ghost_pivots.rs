//! Ziv Ghost Pivots.
//!
//! The pivot of each 4-hour, day, week and month period, drawn on its bars: the typical price
//! (high + low + close) / 3 of the period, final for a completed period and so far for the current
//! one, so that the current period shows where its pivot would lie if it ended now.
//!
//! Days, weeks and months are exchange trading periods, counted as
//! [`PivotPoints`](super::PivotPoints) counts them: the chart supplies the instrument's time zone
//! and session through [`Indicator::set_instrument`] (UTC days until then), weeks run Monday to
//! Sunday and months follow the calendar. 4-hour periods follow the session as
//! [`HigherIntervalCandles`](super::HigherIntervalCandles) do, from the chart's
//! [`BarClock`] ([`Indicator::set_bar_clock`] or [`Indicator::set_timeframe`]; one minute in UTC
//! until then). On a 4-hour chart without session hours each bar is its own 4-hour period,
//! wherever it opens. A period shorter than the chart interval has no value, nor has the 4-hour
//! period on a chart without a time interval. Bars that do not form a valid candle, such as bars
//! with a non-finite price or volume, have no value and do not enter a period.
//!
//! # Output
//!
//! Five lines: the 4-hour, day, week and month pivots, drawn as crosses and each labelled with its
//! period at its latest bar, and the close. A period that is turned off has no values, label or
//! legend entry. Overlay indicator.

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use crate::model::{Bar, BarClock, Instrument, Timeframe};
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, LabelPlacement, StudyLabel, palette,
};
use chrono::NaiveDate;
use egui::Color32;
use std::ops::Range;

/// The period of each pivot plot.
const PERIODS: [Timeframe; 4] = [
    Timeframe::Hour4,
    Timeframe::Day1,
    Timeframe::Week1,
    Timeframe::Month1,
];

/// The label of each pivot plot at its latest bar.
const LABELS: [&str; 4] = ["4H", "D", "W", "M"];

/// Ziv Ghost Pivots: the typical price of the 4-hour, day, week and month periods so far.
#[derive(Clone, Debug)]
pub struct ZivGhostPivots {
    show_four_hour: bool,
    show_day: bool,
    show_week: bool,
    show_month: bool,
    clock: BarClock,
    instrument: Instrument,
    days: ExchangeDays,
    /// The trading day of each calculated bar.
    dates: Vec<NaiveDate>,
    /// The first bar sharing its UTC date with the bar before it, which makes trading days
    /// exchange dates rather than UTC dates.
    shared: Option<usize>,
    /// The first bar of each period of each pivot plot, where an update resumes.
    starts: [Vec<usize>; 4],
    values: IndicatorValues,
    labels: Vec<StudyLabel>,
    colors: [Color32; 5],
    visible: bool,
}

impl ZivGhostPivots {
    /// All four periods shown.
    pub fn new() -> Self {
        let cyan = Color32::from_rgb(0x00, 0xBC, 0xD4);
        Self {
            show_four_hour: true,
            show_day: true,
            show_week: true,
            show_month: true,
            clock: BarClock::new(Timeframe::Min1),
            instrument: Instrument::new(""),
            days: ExchangeDays::default(),
            dates: Vec::new(),
            shared: None,
            starts: Default::default(),
            values: IndicatorValues::new(),
            labels: Vec::new(),
            colors: [
                cyan,
                palette::BLUE,
                palette::DEEP_PURPLE,
                palette::PINK,
                cyan,
            ],
            visible: true,
        }
    }

    /// Whether pivot plot `plot` has values: turned on and no shorter than the chart interval.
    fn active(&self, plot: usize) -> bool {
        let shown = [
            self.show_four_hour,
            self.show_day,
            self.show_week,
            self.show_month,
        ];
        let source = self.clock.timeframe().map_or(0, |tf| tf.duration_ms());
        shown[plot] && source <= PERIODS[plot].duration_ms() && (plot > 0 || source > 0)
    }

    /// Recomputes pivot plot `plot` from the period holding the last usable bar before `from`.
    fn plot(&mut self, data: &[Bar], plot: usize, from: usize) {
        let starts = &mut self.starts[plot];
        let kept = starts
            .partition_point(|&start| start < from)
            .saturating_sub(1);
        let start = starts.get(kept).copied().unwrap_or(0);
        starts.truncate(kept);
        for at in start..data.len() {
            self.values.set_value(plot, at, f64::NAN);
        }
        let indices: Vec<usize> = (start..data.len())
            .filter(|&at| crate::data::merge::valid(&data[at]) && !data[at].is_whitespace())
            .collect();
        let periods: Vec<(Range<usize>, [f64; 3])> = if plot == 0 {
            let bars: Vec<Bar> = indices.iter().map(|&at| data[at].clone()).collect();
            // A chart's clock keeps only the hours the chart shows.
            let instrument = match self.clock.session() {
                Some(session) => self.instrument.clone().with_session(session.clone()),
                None => self.instrument.clone(),
            };
            let target = self.clock.for_timeframe(PERIODS[0]);
            match crate::data::resample_periods(
                &bars,
                self.clock.clone(),
                target,
                &instrument,
                true,
                true,
            ) {
                Some(periods) => periods
                    .into_iter()
                    .map(|(range, bar)| (range, [bar.high, bar.low, bar.close]))
                    .collect(),
                None if start > 0 => return self.plot(data, plot, 0),
                None => Vec::new(),
            }
        } else {
            let anchor = [
                AnchorPeriod::Session,
                AnchorPeriod::Week,
                AnchorPeriod::Month,
            ][plot - 1];
            let mut periods: Vec<(Range<usize>, [f64; 3])> = Vec::new();
            let mut key = None;
            for (n, &at) in indices.iter().enumerate() {
                let bar = &data[at];
                let next = Some(anchor.key(self.dates[at]));
                match periods.last_mut() {
                    Some((range, [high, low, close])) if key == next => {
                        range.end = n + 1;
                        *high = high.max(bar.high);
                        *low = low.min(bar.low);
                        *close = bar.close;
                    }
                    _ => {
                        periods.push((n..n + 1, [bar.high, bar.low, bar.close]));
                        key = next;
                    }
                }
            }
            periods
        };
        let mut last = None;
        for (range, [high, low, close]) in periods {
            let pivot = (high + low + close) / 3.0;
            self.starts[plot].push(indices[range.start]);
            for &at in &indices[range.clone()] {
                self.values.set_value(plot, at, pivot);
            }
            last = Some((indices[range.end - 1], pivot));
        }
        if let Some((bar, price)) = last.filter(|(_, price)| price.is_finite()) {
            self.labels.push(StudyLabel {
                bar,
                price,
                text: LABELS[plot].to_string(),
                placement: LabelPlacement::AboveRight,
                plot,
            });
        }
    }
}

impl Default for ZivGhostPivots {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ZivGhostPivots {
    crate::studies::inputs::study_inputs!(self;
        show_four_hour: toggle("Show 4-hour pivot"),
        show_day: toggle("Show day pivot"),
        show_week: toggle("Show week pivot"),
        show_month: toggle("Show month pivot")
    ; {});

    fn name(&self) -> &str {
        "Ziv Ghost Pivots"
    }

    fn desc(&self) -> &str {
        "Ziv Ghost Pivots - Typical price of the current 4-hour, day, week and month periods"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        let was_daily = self.shared.is_none();
        self.shared = self.shared.filter(|&at| at < from).or_else(|| {
            (from.max(1)..data.len())
                .find(|&at| data[at - 1].time.date_naive() == data[at].time.date_naive())
        });
        let daily = self.shared.is_none();
        let from = if daily == was_daily { from } else { 0 };
        if from == 0 {
            self.values.clear();
            self.starts = Default::default();
        }
        self.values.resize(data.len(), IndicatorValue::None);
        if !data.is_empty() && self.values.columns().len() < 5 {
            self.values.set_value(4, 0, f64::NAN);
        }
        self.dates.truncate(from);
        let days = &self.days;
        self.dates
            .extend(data[from..].iter().map(|bar| days.day(bar, daily)));
        for (at, bar) in data.iter().enumerate().skip(from) {
            self.values.set_value(4, at, bar.close);
        }
        self.labels.clear();
        for plot in 0..4 {
            if self.active(plot) {
                self.plot(data, plot, from);
            }
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.clock = self.clock.for_timeframe(timeframe);
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        if self.clock == *clock {
            return false;
        }
        self.clock = clock.clone();
        true
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let next = instrument.cloned().unwrap_or_else(|| Instrument::new(""));
        let changed =
            self.instrument.time_zone != next.time_zone || self.instrument.session != next.session;
        self.instrument = next;
        self.days.set(instrument) | changed
    }

    fn labels(&self) -> &[StudyLabel] {
        &self.labels
    }

    fn legend_lines(&self) -> Vec<usize> {
        let shown = [
            self.show_four_hour,
            self.show_day,
            self.show_week,
            self.show_month,
            true,
        ];
        (0..5).filter(|&line| shown[line]).collect()
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line < 4 {
                crate::studies::PlotType::Crosses
            } else {
                crate::studies::PlotType::Line
            },
            visible: true,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    // Its labels sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        ["4-hour", "Day", "Week", "Month", "Close"]
            .map(String::from)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bars around the New York change to daylight saving time on Sunday 8 March 2026, with
    /// (high, low, close) and the New York time: Thursday 26 February 10:00 (10, 8, 9) and 15:00
    /// (12, 9, 11); Saturday 28 February 22:00 EST, 1 March in UTC (11, 7, 8); Sunday 1 March
    /// 23:00 EST, Monday in UTC (13, 9, 12); Monday 2 March 10:00 (9, 8, 8.5); Monday 9 March
    /// 00:00 EDT, still Sunday at the winter offset (10, 9, 9.5); and 11:00 (11, 9.5, 10.5).
    /// Pivots by hand: week of 23 February (13 + 7 + 12) / 3, week of 2 March 25.5 / 3, week of
    /// 9 March so far (11 + 9 + 10.5) / 3 and after its first bar 28.5 / 3; February
    /// (12 + 7 + 8) / 3, March (13 + 8 + 10.5) / 3 and before the last bar (13 + 8 + 9.5) / 3;
    /// days (12 + 8 + 11) / 3, 26 / 3, 34 / 3, 25.5 / 3, then 30.5 / 3 or 28.5 / 3.
    #[cfg(feature = "timezones")]
    #[test]
    fn pivots_hold_the_typical_price_of_each_exchange_period_so_far() {
        use crate::model::TimezoneId;
        let bars: Vec<Bar> = [
            ("2026-02-26T15:00:00Z", 10.0, 8.0, 9.0),
            ("2026-02-26T20:00:00Z", 12.0, 9.0, 11.0),
            ("2026-03-01T03:00:00Z", 11.0, 7.0, 8.0),
            ("2026-03-02T04:00:00Z", 13.0, 9.0, 12.0),
            ("2026-03-02T15:00:00Z", 9.0, 8.0, 8.5),
            ("2026-03-09T04:00:00Z", 10.0, 9.0, 9.5),
            ("2026-03-09T15:00:00Z", 11.0, 9.5, 10.5),
        ]
        .into_iter()
        .map(|(time, high, low, close)| {
            Bar::new(time.parse().unwrap(), close, high, low, close, 1.0)
        })
        .collect();
        let new_york =
            Instrument::new("NY").with_time_zone(TimezoneId::named("America/New_York").unwrap());
        let mut study = ZivGhostPivots::new();
        study.set_timeframe(Timeframe::Hour1);
        assert!(study.set_instrument(Some(&new_york)));
        let (day, week, month) = (31.0 / 3.0, 32.0 / 3.0, 9.0);
        // (bars, plot, value of each bar)
        let rows: [(usize, usize, &[f64]); 6] = [
            (
                7,
                1,
                &[
                    day,
                    day,
                    26.0 / 3.0,
                    34.0 / 3.0,
                    8.5,
                    30.5 / 3.0,
                    30.5 / 3.0,
                ],
            ),
            (7, 2, &[week, week, week, week, 8.5, 30.5 / 3.0, 30.5 / 3.0]),
            (
                7,
                3,
                &[
                    month,
                    month,
                    month,
                    31.5 / 3.0,
                    31.5 / 3.0,
                    31.5 / 3.0,
                    31.5 / 3.0,
                ],
            ),
            (6, 1, &[day, day, 26.0 / 3.0, 34.0 / 3.0, 8.5, 9.5]),
            (6, 2, &[week, week, week, week, 8.5, 9.5]),
            (
                6,
                3,
                &[month, month, month, 30.5 / 3.0, 30.5 / 3.0, 30.5 / 3.0],
            ),
        ];
        for (len, plot, want) in rows {
            study.calculate(&bars[..len]);
            let got: Vec<_> = (0..len).map(|at| study.values().value(plot, at)).collect();
            let want: Vec<_> = want.iter().map(|&v| Some(v)).collect();
            assert_eq!(got, want, "{len} bars, plot {plot}");
        }
        study.calculate(&bars);
        let label = |plot: usize, price: f64| StudyLabel {
            bar: 6,
            price,
            text: LABELS[plot].to_string(),
            placement: LabelPlacement::AboveRight,
            plot,
        };
        assert_eq!(
            study.labels(),
            [
                label(0, 31.0 / 3.0),
                label(1, 30.5 / 3.0),
                label(2, 30.5 / 3.0),
                label(3, 31.5 / 3.0),
            ]
        );
    }

    /// On a 4-hour chart each bar is its own 4-hour period, also when it opens off the 4-hour
    /// grid: bars opening at 01:00, 05:00 and 09:00 UTC with (high, low, close) (10, 8, 9),
    /// (12, 9, 11) and (11, 7, 8) have the pivots 27 / 3, 32 / 3 and 26 / 3, and their day
    /// (12 + 7 + 8) / 3.
    #[test]
    fn four_hour_bars_off_the_grid_keep_their_own_pivot() {
        let mut bars =
            super::super::hlc_bars(&[(10.0, 8.0, 9.0), (12.0, 9.0, 11.0), (11.0, 7.0, 8.0)]);
        for (bar, time) in bars.iter_mut().zip([
            "2026-03-02T01:00:00Z",
            "2026-03-02T05:00:00Z",
            "2026-03-02T09:00:00Z",
        ]) {
            bar.time = time.parse().unwrap();
        }
        let mut study = ZivGhostPivots::new();
        study.set_timeframe(Timeframe::Hour4);
        study.calculate(&bars);
        let pivots: Vec<_> = (0..3).map(|at| study.values().value(0, at)).collect();
        assert_eq!(pivots, [Some(9.0), Some(32.0 / 3.0), Some(26.0 / 3.0)]);
        let day: Vec<_> = (0..3).map(|at| study.values().value(1, at)).collect();
        assert_eq!(day, [Some(9.0); 3]);
        let label = &study.labels()[0];
        assert_eq!((label.plot, label.bar, label.price), (0, 2, 26.0 / 3.0));
    }

    /// On a daily chart the 4-hour pivot has no value, and a period turned off loses its values,
    /// label and legend entry while the others keep theirs.
    #[test]
    fn short_or_hidden_periods_have_no_plot() {
        let bars = super::super::hlc_bars(&[(10.0, 8.0, 9.0), (12.0, 9.0, 11.0)]);
        let mut study = ZivGhostPivots::new();
        study.set_timeframe(Timeframe::Day1);
        study
            .set_input("show_week", crate::studies::InputValue::Toggle(false))
            .unwrap();
        study.calculate(&bars);
        let line = |plot: usize| -> Vec<Option<f64>> {
            (0..2).map(|at| study.values().value(plot, at)).collect()
        };
        assert_eq!(line(0), [None, None]);
        assert_eq!(line(1), [Some(9.0), Some(32.0 / 3.0)]);
        assert_eq!(line(2), [None, None]);
        assert_eq!(line(4), [Some(9.0), Some(11.0)]);
        let plots: Vec<_> = study.labels().iter().map(|label| label.plot).collect();
        assert_eq!(plots, [1, 3]);
        assert_eq!(study.legend_lines(), [0, 1, 3, 4]);
    }
}
