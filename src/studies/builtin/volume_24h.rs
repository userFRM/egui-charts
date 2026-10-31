//! 24-hour Volume
//!
//! The volume traded over the last 24 hours at each bar, whatever the chart interval: a
//! rolling sum over a fixed span of calendar time rather than a number of bars, so a
//! 1-minute and a 1-hour chart of the same market show the same turnover.
//!
//! # Calculation
//! On an interval that divides a day,
//!
//! Volume24h(t) = Sum(volume of the bars that open in (t - 24 h, t])
//!
//! where `t` is the open time of the current bar. A bar that opens exactly 24 hours before
//! the current one belongs to the previous day, so the window holds exactly one day of bars,
//! and on daily bars it holds the bar alone.
//!
//! Bars longer than a day, or of an interval that does not divide a day, cannot be split into
//! the last 24 hours, so on those intervals the study requests lower-interval candles through
//! [`Indicator::data_requests`]: one hour above a day, one minute below it and one second below
//! a minute, or one second on an interval that is not a whole number of those. Each bar then sums the candles that open in the 24 hours before the close of its
//! last candle: the bar's own close when trading runs to its end, and otherwise its last
//! trading hours, so a weekly bar of a market closed at the weekend sums its last trading day.
//! A bar without candles has no value. Until candles are supplied the study sums chart bars,
//! which has no value on bars longer than a day. The volume is summed in the units the feed
//! reports.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, Volume24h};
//!
//! // Hourly bars of 10 units: from the 25th bar on, the oldest bar leaves the window.
//! let bars: Vec<Bar> = (0..30)
//!     .map(|h| Bar::new(Utc.timestamp_opt(h * 3_600, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 10.0))
//!     .collect();
//! let mut study = Volume24h::new();
//! study.calculate(&bars);
//! assert_eq!(study.values().value(0, 0), Some(10.0));
//! assert_eq!(study.values().value(0, 23), Some(240.0));
//! assert_eq!(study.values().value(0, 29), Some(240.0));
//! ```

use super::lower_interval;
use crate::model::{Bar, BarClock, Timeframe};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyData, StudyDataRequest};
use chrono::{DateTime, TimeDelta, Utc};
use egui::Color32;
use std::collections::VecDeque;

/// Rolling volume over the last 24 hours of calendar time, in a separate pane.
///
/// A bar with a non-finite volume has no value and adds nothing. The chart supplies its
/// interval through [`Indicator::set_bar_clock`] and the candles through
/// [`Indicator::update_data`]; a direct caller on bars longer than a day passes a clock of that
/// interval, without which every bar is summed as an intraday bar.
#[derive(Clone)]
pub struct Volume24h {
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// The chart clock, from `set_bar_clock`.
    clock: BarClock,
    /// Whether `values` were summed from lower-interval candles rather than chart bars.
    from_candles: bool,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    /// Open time and volume of each bar in the window, oldest first.
    window: VecDeque<(DateTime<Utc>, f64)>,
    sum: f64,
}

impl Volume24h {
    /// A 24-hour volume study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            clock: BarClock::new(Timeframe::Min1),
            from_candles: false,
            color: palette::BLUE,
            visible: true,
        }
    }

    /// The lower interval read where chart bars cannot hold exactly the last 24 hours: on
    /// intervals longer than a day or that do not divide it.
    fn lower_timeframe(&self) -> Option<Timeframe> {
        let span = self.clock.timeframe()?.duration_ms();
        let day = Timeframe::Day1.duration_ms();
        if span > 0 && span <= day && day % span == 0 {
            return None;
        }
        lower_interval::lower_timeframe(&self.clock, None)
    }

    /// Sums, for each bar of `data[from..]`, the candles that open in the 24 hours before the
    /// close of its last candle.
    fn sum_candles(&mut self, data: &[Bar], from: usize, lower: Timeframe, candles: &[Bar]) {
        let from = if self.from_candles { from } else { 0 };
        self.from_candles = true;
        if from == 0 {
            self.values = IndicatorValues::missing(data.len());
        } else {
            self.values.truncate(from);
            self.values.resize(data.len(), IndicatorValue::None);
        }
        let ranges = lower_interval::nest(&self.clock, lower, data, candles, from);
        for (i, range) in (from..).zip(ranges) {
            if range.is_empty() || !data[i].close.is_finite() {
                continue;
            }
            let last = range.end - 1;
            let end = candles[last]
                .time
                .checked_add_signed(lower.duration())
                .unwrap_or(DateTime::<Utc>::MAX_UTC);
            let start = end
                .checked_sub_signed(TimeDelta::days(1))
                .unwrap_or(DateTime::<Utc>::MIN_UTC);
            let first = candles[..last].partition_point(|candle| candle.time < start);
            // shortcut: sums each window afresh, up to 86,400 candles a bar on one-second
            // candles (intervals of a few seconds that do not divide the day); slide the window
            // if such charts become slow.
            let sum = candles[first..range.end]
                .iter()
                .map(|candle| candle.volume)
                .sum();
            self.values.set_value(0, i, sum);
        }
    }
}

impl Default for Volume24h {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Volume24h {
    fn name(&self) -> &str {
        "24-hour Volume"
    }

    fn desc(&self) -> &str {
        "24-hour Volume - Volume traded over the last 24 hours, on any interval"
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Volume
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.color,
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if std::mem::take(&mut self.from_candles) {
            // Chart bars replace candle sums from the first bar.
            self.values.clear();
            self.stepper = Stepper::default();
        }
        let long_bars = self
            .clock
            .timeframe()
            .is_some_and(|timeframe| timeframe.duration_ms() > Timeframe::Day1.duration_ms());
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if long_bars || !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                let start = bar
                    .time
                    .checked_sub_signed(TimeDelta::days(1))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                let mut largest = 0.0_f64;
                while let Some(&(time, volume)) = state.window.front()
                    && time <= start
                {
                    state.window.pop_front();
                    state.sum -= volume;
                    largest = largest.max(volume.abs());
                }
                if largest > state.sum.abs() {
                    // A removed volume that dominated the sum leaves its rounding behind:
                    // add the window again so the error does not linger.
                    state.sum = state.window.iter().map(|&(_, volume)| volume).sum();
                }
                state.window.push_back((bar.time, bar.volume));
                state.sum += bar.volume;
                // Overflow of a finite sum must not poison later ordinary windows.
                if !state.sum.is_finite() {
                    state.sum = state.window.iter().map(|&(_, volume)| volume).sum();
                }
                IndicatorValue::Single(state.sum)
            });
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        if changed {
            self.clock = clock.clone();
            self.values.clear();
            self.stepper = Stepper::default();
            self.from_candles = false;
        }
        changed
    }

    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.lower_timeframe()
            .map(lower_interval::request)
            .into_iter()
            .collect()
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let Some(lower) = self.lower_timeframe() else {
            return self.update(data, from);
        };
        let from = from.min(self.values.len()).min(data.len());
        match lower_interval::reply(&lower_interval::request(lower), from, sources) {
            None => {}
            Some([]) => self.update(data, from),
            Some(candles) => self.sum_candles(data, from, lower, candles),
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
        }
    }

    fn is_overlay(&self) -> bool {
        false
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Each value is the sum of its own window, even after a volume too large for the others to
    /// register in a running sum has left it.
    #[test]
    fn a_dominant_volume_leaves_no_residue() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let bars: Vec<Bar> = (0..60)
            .map(|h| {
                let volume = if h == 0 { 1e17 } else { 1.0 };
                Bar::new(start + TimeDelta::hours(h), 1.0, 1.0, 1.0, 1.0, volume)
            })
            .collect();
        let mut study = Volume24h::new();
        study.calculate(&bars);
        for bar in 24..60 {
            assert_eq!(study.values().value(0, bar), Some(24.0), "bar {bar}");
        }
    }

    /// On intervals longer than a day or that do not divide it, each bar sums the lower-interval
    /// candles of the 24 hours before the close of its last candle; without candles it sums chart
    /// bars as before. Intervals that divide the day request nothing.
    #[test]
    fn long_and_uneven_intervals_sum_lower_interval_candles() {
        use crate::studies::{StudyData, StudyDataRequest};
        let (hour, day) = (3_600, 86_400);
        let bar =
            |s: i64, volume| Bar::new(Utc.timestamp_opt(s, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, volume);
        let five_hours = Timeframe::Custom(std::num::NonZeroU64::new(18_000).unwrap());
        let mut study = Volume24h::new();
        for (chart, lower) in [
            (Timeframe::Hour1, None),
            (Timeframe::Day1, None),
            (five_hours, Some(Timeframe::Min1)),
            (Timeframe::Week1, Some(Timeframe::Hour1)),
        ] {
            study.set_bar_clock(&BarClock::new(chart));
            let requested = study.data_requests().first().and_then(|r| r.timeframe);
            assert_eq!(requested, lower, "{chart:?}");
        }
        let sums = |study: &mut Volume24h, data: &[Bar], lower: Timeframe, candles: &[Bar]| {
            let source = StudyData {
                request: StudyDataRequest {
                    metric: None,
                    symbol: String::new(),
                    timeframe: Some(lower),
                },
                bars: candles,
                clock: BarClock::new(lower),
                instrument: None,
                timeframe: lower,
            };
            study.update_data(data, 0, &[source]);
            (0..data.len())
                .map(|i| study.values().value(0, i))
                .collect::<Vec<_>>()
        };
        // Weekly bars from Monday 5 January 1970 (day 4). The first week's last candle closes on
        // Friday at 16:00, so its window starts on Thursday at 16:00; the second week's closes
        // with the bar, on Monday 19 January at 00:00.
        let weeks = [4, 11, 18].map(|d| bar(d * day, 1.0));
        let hourly = [
            bar(7 * day + 15 * hour, 1.0),   // Thursday 15:00, before the window
            bar(7 * day + 16 * hour, 2.0),   // Thursday 16:00, its first hour
            bar(8 * day + 15 * hour, 4.0),   // Friday 15:00, the week's last candle
            bar(16 * day + 23 * hour, 8.0),  // Saturday 23:00, before the window
            bar(17 * day, 16.0),             // Sunday 00:00, its first hour
            bar(17 * day + 23 * hour, 32.0), // Sunday 23:00
        ];
        assert_eq!(
            sums(&mut study, &weeks, Timeframe::Hour1, &hourly),
            [Some(6.0), Some(48.0), None]
        );
        // Five-hour bars from midnight on two days; minute candles at 04:59, 05:00 and 09:59
        // on the first day and at 04:59 and 05:00 on the second.
        study.set_bar_clock(&BarClock::new(five_hours));
        let bars = [0, 5, 10, 15, 20, 24, 29].map(|h| bar(h * hour, 1.0));
        let minutes = [
            (17_940, 1.0),
            (18_000, 2.0),
            (35_940, 4.0),
            (104_340, 8.0),
            (104_400, 16.0),
        ]
        .map(|(s, v)| bar(s, v));
        assert_eq!(
            sums(&mut study, &bars, Timeframe::Min1, &minutes),
            [
                Some(1.0),
                Some(7.0),
                None,
                None,
                None,
                Some(14.0),
                Some(28.0)
            ]
        );
        // Without candles the chart bars opened in the last 24 hours are summed again.
        assert_eq!(
            sums(&mut study, &bars, Timeframe::Min1, &[]),
            [1.0, 2.0, 3.0, 4.0, 5.0, 5.0, 5.0].map(Some)
        );
    }
}
