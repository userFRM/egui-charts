//! Missed Pivot Levels.
//!
//! A completed trading period has pivot P = (high + low + close) / 3. If the next period's
//! running high to low range never reaches P, within `tolerance` percent of |P|, that pivot
//! is missed. It is confirmed when another period starts and drawn from the first bar of
//! the period that missed it. A pending pivot is not drawn while its test period is forming.
//! Daily, weekly (ISO weeks) and monthly periods follow exchange trading days in the chart
//! instrument's time zone and session ([`Indicator::set_instrument`]; UTC until supplied).
//! Periods without usable bars are skipped. The first period may be partial. A period shorter
//! than the chart interval ([`Indicator::set_timeframe`]) gives no levels.
//!
//! The default draws the missed pivots of each trading day, week and month together, each
//! period as its own plot (blue, teal and red, 2 px). There a pivot that a later bar reaches
//! is hidden unless `show_touched` is on, and a level whose first bar is more than
//! `days_back` days (180 by default, 1 to 730) before the latest bar is dropped. A single
//! period, or swings, draws levels in the colour of their side (above first, below second)
//! and keeps every reached level up to the bar reaching it.
//!
//! With the period set to swings, levels are swing highs and lows (see
//! [`PivotPointsHighLow`](super::PivotPointsHighLow)):
//!
//! - Confirmed `right` bars after the pivot bar; bars in that window never revisit it.
//! - Missed while every later bar stays below a pivot high or above a pivot low.
//! - Revisited when a later high reaches a high level or a low reaches a low level, within
//!   `tolerance` percent of |P|. A gap beyond the level also counts as reaching it.
//!
//! In every mode a revisited level ends at the first bar reaching it, including the bar that
//! confirms a missed period pivot. The latest `max_levels` levels are kept. A bar without a
//! finite high and low takes no part; period pivots also require a finite close. A pivot whose
//! sum overflows is skipped. Overlay indicator without per-bar values. The defaults take no
//! tolerance, so only a bar's range reaches a level, and 500 levels; `new` takes swings of
//! `left` and `right` bars.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, MissedPivotPeriod, MissedPivots};
//!
//! // A peak at 10, then a fall that never returns to it.
//! let bars: Vec<Bar> = [5.0, 8.0, 10.0, 8.0, 5.0, 6.0]
//!     .into_iter()
//!     .enumerate()
//!     .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64 * 60, 0).unwrap(), p, p, p, p, 1.0))
//!     .collect();
//! let mut missed = MissedPivots::new(2, 2, 0.1, 20);
//! missed.calculate(&bars);
//! // The pivot high at bar 2 is confirmed at bar 4 and is still missed.
//! assert_eq!(missed.zones().len(), 1);
//! assert_eq!((missed.zones()[0].bars.clone(), missed.zones()[0].top), (2..6, 10.0));
//!
//! // The six bars fall on one day, so no daily pivot has completed yet.
//! let mut daily = MissedPivots::new(2, 2, 0.1, 20).with_period(MissedPivotPeriod::Daily);
//! daily.calculate(&bars);
//! assert!(daily.zones().is_empty());
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use super::clamp_param;
use super::pivot_points_high_low::Swings;
use crate::model::{Bar, Instrument, Timeframe};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorLineStyle, IndicatorValue, IndicatorValues, StudyZone};
use chrono::NaiveDate;
use egui::Color32;
use std::collections::VecDeque;

/// The most levels that can be kept.
const MAX_LEVELS: usize = 500;

/// Where [`MissedPivots`] takes its levels from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MissedPivotPeriod {
    /// Swing highs and lows found by the left and right bar counts.
    Swings,
    /// The pivot of each trading day.
    Daily,
    /// The pivot of each ISO week of trading days.
    Weekly,
    /// The pivot of each calendar month of trading days.
    Monthly,
    /// The pivots of each trading day, ISO week and calendar month, each period drawn as its
    /// own plot.
    DailyWeeklyMonthly,
}

impl MissedPivotPeriod {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Swings => "Swings",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::DailyWeeklyMonthly => "Daily, weekly and monthly",
        }
    }
}

/// Missed Pivot Levels: pivot levels that price has not returned to.
#[derive(Clone, Debug)]
pub struct MissedPivots {
    period: MissedPivotPeriod,
    days: ExchangeDays,
    intraday: bool,
    /// The chart interval, once supplied; periods shorter than it give no levels.
    interval: Option<Timeframe>,
    left: usize,
    right: usize,
    /// With the three periods, the most days a level's first bar may lie before the latest bar.
    days_back: usize,
    /// With the three periods, whether levels a later bar has reached stay drawn.
    show_touched: bool,
    tolerance: f64,
    max_levels: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    zones: Vec<StudyZone>,
    /// Missed pivot high, missed pivot low; daily, weekly and monthly pivots.
    colors: [Color32; 5],
    visible: bool,
}

#[derive(Clone, Debug)]
struct Level {
    bar: usize,
    /// The trading day of `bar`.
    day: NaiveDate,
    price: f64,
    high: bool,
    plot: usize,
    revisited: Option<usize>,
}

/// The forming period and the preceding period's pivot, until its range reaches it.
#[derive(Clone, Debug)]
struct Span {
    key: (i32, u32),
    /// The trading day of the period's first bar.
    day: NaiveDate,
    high: f64,
    low: f64,
    close: f64,
    pivot: Option<(usize, f64)>,
}

#[derive(Clone, Debug, Default)]
struct State {
    source: usize,
    previous_date: Option<NaiveDate>,
    intraday: bool,
    /// The forming day, week or month of each period taken.
    spans: [Option<Span>; 3],
    swings: Swings,
    levels: VecDeque<Level>,
}

impl MissedPivots {
    /// Pivots with `left` and `right` bars on each side (at least 1), revisited within
    /// `tolerance` percent (0 to 100, NaN to 0) of the level, keeping the latest `max_levels`
    /// (1 to 500).
    pub fn new(left: usize, right: usize, tolerance: f64, max_levels: usize) -> Self {
        Self {
            period: MissedPivotPeriod::Swings,
            days: ExchangeDays::default(),
            intraday: false,
            interval: None,
            left: left.max(1),
            right: right.max(1),
            days_back: 180,
            show_touched: false,
            tolerance: clamp_param(tolerance, 0.0, 100.0),
            max_levels: max_levels.clamp(1, MAX_LEVELS),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            zones: Vec::new(),
            colors: [
                palette::RED,
                palette::GREEN,
                palette::BLUE,
                palette::DEEP_TEAL,
                palette::BRIGHT_RED,
            ],
            visible: true,
        }
    }

    /// Takes swing levels, the pivot of each completed day, week or month, or the pivots of
    /// all three.
    pub fn with_period(mut self, period: MissedPivotPeriod) -> Self {
        self.period = period;
        self
    }

    /// Whether each period is its own plot, rather than each side.
    fn combined(&self) -> bool {
        self.period == MissedPivotPeriod::DailyWeeklyMonthly
    }

    /// Where the colours of the drawn plots lie in `colors`.
    fn plots(&self) -> std::ops::Range<usize> {
        if self.combined() { 2..5 } else { 0..2 }
    }
}

/// Construct with the conventional default parameters: the daily, weekly and monthly pivots
/// together, reached only by a bar's range, and the most levels.
impl Default for MissedPivots {
    fn default() -> Self {
        Self::new(10, 10, 0.0, MAX_LEVELS).with_period(MissedPivotPeriod::DailyWeeklyMonthly)
    }
}

impl Indicator for MissedPivots {
    crate::studies::inputs::study_inputs!(self;
        period: choice("Pivots Timeframe", [MissedPivotPeriod::Swings, MissedPivotPeriod::Daily, MissedPivotPeriod::Weekly, MissedPivotPeriod::Monthly, MissedPivotPeriod::DailyWeeklyMonthly]),
        left: length("Pivot left bars", 1, usize::MAX) if period in [0],
        right: length("Pivot right bars", 1, usize::MAX) if period in [0],
        days_back: length("Days back", 1, 730) if period in [4],
        show_touched: toggle("Show touched pivots") if period in [4],
        tolerance: number("Revisit tolerance (%)", 0.0, 100.0),
        max_levels: length("Max number of levels", 1, MAX_LEVELS)
    ; {});

    fn name(&self) -> &str {
        "Missed Pivot Levels"
    }

    fn desc(&self) -> &str {
        "Missed Pivot Levels - Pivot levels that price has not returned to"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], mut from: usize) {
        let (left, right, max_levels) = (self.left, self.right, self.max_levels);
        let tolerance = self.tolerance / 100.0;
        let anchors: &[AnchorPeriod] = match self.period {
            MissedPivotPeriod::Swings => &[],
            MissedPivotPeriod::Daily => &[AnchorPeriod::Session],
            MissedPivotPeriod::Weekly => &[AnchorPeriod::Week],
            MissedPivotPeriod::Monthly => &[AnchorPeriod::Month],
            MissedPivotPeriod::DailyWeeklyMonthly => &[
                AnchorPeriod::Session,
                AnchorPeriod::Week,
                AnchorPeriod::Month,
            ],
        };
        let combined = self.combined();
        let (days_back, show_touched) = (self.days_back as i64, self.show_touched);
        // A period shorter than the chart interval gives no levels; without a time interval
        // every period does.
        let interval = self.interval.map_or(0, |interval| interval.duration_ms());
        let active: [bool; 3] = std::array::from_fn(|slot| {
            anchors.get(slot).is_some_and(|anchor| {
                let period = match anchor {
                    AnchorPeriod::Session => Timeframe::Day1,
                    AnchorPeriod::Week => Timeframe::Week1,
                    _ => Timeframe::Month1,
                };
                interval <= period.duration_ms()
            })
        });
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let index = state.source;
                    state.source += 1;
                    // A bar without a finite high and low takes no part, as if it were absent.
                    if !(bar.high.is_finite() && bar.low.is_finite())
                        || (!anchors.is_empty() && !bar.close.is_finite())
                    {
                        return IndicatorValue::None;
                    }
                    // Swing levels need no trading day.
                    let day = if anchors.is_empty() {
                        NaiveDate::MIN
                    } else {
                        let date = bar.time.date_naive();
                        state.intraday |= state.previous_date == Some(date);
                        state.previous_date = Some(date);
                        self.days.day(bar, !self.intraday)
                    };
                    for (slot, anchor) in anchors.iter().enumerate() {
                        if !active[slot] {
                            continue;
                        }
                        let key = anchor.key(day);
                        match &mut state.spans[slot] {
                            Some(span) if span.key == key => {
                                span.high = span.high.max(bar.high);
                                span.low = span.low.min(bar.low);
                                span.close = bar.close;
                            }
                            _ => {
                                let done = state.spans[slot].take();
                                if let Some(span) = &done
                                    && let Some((bar, price)) = span.pivot
                                {
                                    let high = price > span.high;
                                    state.levels.push_back(Level {
                                        bar,
                                        day: span.day,
                                        price,
                                        high,
                                        plot: if combined { slot } else { usize::from(!high) },
                                        revisited: None,
                                    });
                                }
                                let pivot = done
                                    .map(|span| (span.high + span.low + span.close) / 3.0)
                                    .filter(|p| p.is_finite())
                                    .map(|p| (index, p));
                                state.spans[slot] = Some(Span {
                                    key,
                                    day,
                                    high: bar.high,
                                    low: bar.low,
                                    close: bar.close,
                                    pivot,
                                });
                            }
                        }
                    }
                    for level in state.levels.iter_mut().filter(|l| l.revisited.is_none()) {
                        let reach = level.price.abs() * tolerance;
                        let reached = if level.high {
                            bar.high >= level.price - reach
                        } else {
                            bar.low <= level.price + reach
                        };
                        if reached {
                            level.revisited = Some(index);
                        }
                    }
                    if anchors.is_empty() {
                        let confirmed = state.swings.push(index, bar, left, right);
                        for (high, pivot) in [(true, confirmed[0]), (false, confirmed[1])] {
                            if let Some((bar, price)) = pivot {
                                state.levels.push_back(Level {
                                    bar,
                                    day,
                                    price,
                                    high,
                                    plot: usize::from(!high),
                                    revisited: None,
                                });
                            }
                        }
                    }
                    for span in state.spans.iter_mut().flatten() {
                        if let Some((_, price)) = span.pivot {
                            let reach = price.abs() * tolerance;
                            if span.low <= price + reach && span.high >= price - reach {
                                span.pivot = None;
                            }
                        }
                    }
                    if combined {
                        state.levels.retain(|level| {
                            (day - level.day).num_days() <= days_back
                                && (show_touched || level.revisited.is_none())
                        });
                    }
                    while state.levels.len() > max_levels {
                        state.levels.pop_front();
                    }
                    IndicatorValue::None
                });
            if self.intraday == self.stepper.state().intraday {
                break;
            }
            // Switching the inferred interval changes earlier exchange dates too.
            self.intraday = self.stepper.state().intraday;
            from = 0;
        }
        let state = self.stepper.state();
        self.zones = state
            .levels
            .iter()
            .map(|level| StudyZone {
                bars: level.bar..level.revisited.map_or(state.source, |at| at + 1),
                top: level.price,
                bottom: level.price,
                plot: level.plot,
            })
            .collect();
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        if self.interval != Some(timeframe) {
            self.interval = Some(timeframe);
            // Levels found so far may belong to periods the new interval leaves out.
            self.stepper = Stepper::default();
        }
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.days.set(instrument)
    }

    fn zones(&self) -> &[StudyZone] {
        &self.zones
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors[self.plots()].to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        let plots = self.plots();
        for (line, color) in self.colors[plots].iter_mut().zip(colors) {
            *line = color;
        }
    }

    fn line_style(&self, _line: usize) -> IndicatorLineStyle {
        IndicatorLineStyle {
            width: if self.combined() { 2.0 } else { 1.0 },
            ..IndicatorLineStyle::default()
        }
    }

    fn is_overlay(&self) -> bool {
        true
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
        let names: &[&str] = if self.combined() {
            &["Daily pivots", "Weekly pivots", "Monthly pivots"]
        } else {
            &["Missed high", "Missed low"]
        };
        names.iter().map(|name| name.to_string()).collect()
    }

    // The lines only style zones and hold no values.
    fn legend_lines(&self) -> Vec<usize> {
        Vec::new()
    }

    fn alert_outputs(&self) -> Vec<(usize, String)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bars(highs_lows: &[(f64, f64, f64)]) -> Vec<Bar> {
        highs_lows
            .iter()
            .enumerate()
            .map(|(i, &(high, low, close))| {
                let time = Utc.timestamp_opt(i as i64 * 86_400, 0).unwrap();
                Bar::new(time, low, high, low, close, 1.0)
            })
            .collect()
    }

    /// A hand-labelled history with pivots of one bar each side, given to the study a bar at a
    /// time. The pivot high 10 at bar 1 is confirmed at bar 2 and revisited at bar 5, whose high
    /// 9.96 is within 0.5% of 10 (10 - 0.05 = 9.95); the pivot low 4 at bar 3 is confirmed at
    /// bar 4 and still missed, bar 5 lows staying above 4 * 1.005 = 4.02; the pivot high 9.96 at
    /// bar 5 is confirmed at bar 6 and missed.
    ///
    /// The default study draws the missed pivots of each day, week and month of nine daily bars
    /// together, one plot per period. The day 3 pivot 14.5 that day 4 misses by 0.01 is drawn,
    /// unless a 0.1% tolerance counts it as reached; the day 4 pivot 15.42 is touched by the bar
    /// that confirms it and is drawn only while touched pivots are shown.
    #[test]
    fn levels_are_confirmed_missed_and_revisited() {
        use crate::studies::InputValue::{Length, Number, Toggle};
        let swings = bars(&[
            (6.0, 5.0, 5.0),
            (10.0, 8.0, 8.0),
            (9.0, 7.0, 7.0),
            (6.0, 4.0, 4.0),
            (7.0, 5.0, 5.0),
            (9.96, 8.0, 8.0),
            (8.0, 6.0, 6.0),
        ]);
        // Pivots 10, 13, 14, 12, 10.5. The next day touches 13 at its low;
        // 10 is missed below day 1 then reached on day 4; 14 is missed above day 3
        // then reached on day 5. Day 5 confirms and revisits 12 in the same step.
        let periods = bars(&[
            (12.0, 8.0, 10.0),
            (15.0, 11.0, 13.0),
            (14.5, 13.0, 14.5),
            (13.5, 10.5, 12.0),
            (11.5, 9.5, 10.5),
            (14.5, 12.0, 14.0),
        ]);
        // Thursday 29 January 2026 to Wednesday 1 April: ISO weeks 5, 5, 6, 6, 7, 10, 10, 11, 14.
        let mut together = bars(&[
            (11.0, 9.0, 10.0),
            (13.0, 11.0, 12.0),
            (14.0, 12.0, 13.0),
            (15.5, 13.0, 15.0),
            (16.0, 14.51, 15.75),
            (18.0, 16.0, 17.0),
            (17.5, 15.0, 16.0),
            (19.0, 16.5, 18.0),
            (21.0, 19.5, 20.0),
        ]);
        let days = [
            (1, 29),
            (1, 30),
            (2, 2),
            (2, 3),
            (2, 9),
            (3, 2),
            (3, 3),
            (3, 9),
            (4, 1),
        ];
        for (bar, (month, day)) in together.iter_mut().zip(days) {
            bar.time = Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0).unwrap();
        }
        // Bars seen, then the zones (bars, price, plot) at that point.
        type Zone = (std::ops::Range<usize>, f64, usize);
        let zones = |missed: &MissedPivots| -> Vec<Zone> {
            missed
                .zones()
                .iter()
                .map(|z| (z.bars.clone(), z.top, z.plot))
                .collect()
        };
        // Day, week and month pivots (plots 0, 1, 2) in the order bars 2, 4, 5 and 8 confirm
        // them. Day 4's 15.42 is touched at bar 6, week 7's 15.42 is reached in week 10, and the
        // pivots of day 7, week 11 and March are still being tested.
        let drawn: Vec<Zone> = vec![
            (1..9, 10.0, 0),
            (2..9, 34.0 / 3.0, 1),
            (4..9, 43.5 / 3.0, 0),
            (4..9, 42.5 / 3.0, 1),
            (2..9, 34.0 / 3.0, 2),
            (7..9, 48.5 / 3.0, 0),
            (7..9, 49.0 / 3.0, 1),
            (5..9, 43.75 / 3.0, 2),
        ];
        let mut touched = drawn.clone();
        touched.insert(5, (5..7, (16.0 + 14.51 + 15.75) / 3.0, 0));
        let mut tolerant = drawn.clone();
        tolerant.remove(2);
        let weekly: Vec<Zone> = drawn.iter().filter(|zone| zone.2 > 0).cloned().collect();
        let together_rows = [
            (vec![], None, drawn.clone()),
            (vec![("show_touched", Toggle(true))], None, touched),
            // 2 March, the day of bar 5, is 30 days before 1 April.
            (vec![("days_back", Length(30))], None, drawn[5..].to_vec()),
            (vec![("days_back", Length(29))], None, drawn[5..7].to_vec()),
            (vec![("tolerance", Number(0.1))], None, tolerant),
            (vec![], Some(crate::model::Timeframe::Week1), weekly),
        ]
        .into_iter()
        .map(|(inputs, interval, want)| {
            let mut missed = MissedPivots::default();
            for (id, value) in inputs {
                missed.set_input(id, value).unwrap();
            }
            if let Some(interval) = interval {
                missed.set_timeframe(interval);
            }
            (missed, together.clone(), vec![(9, want)])
        });
        let swing_want: [(usize, Vec<Zone>); 4] = [
            (2, vec![]),
            (3, vec![(1..3, 10.0, 0)]),
            (5, vec![(1..5, 10.0, 0), (3..5, 4.0, 1)]),
            (7, vec![(1..6, 10.0, 0), (3..7, 4.0, 1), (5..7, 9.96, 0)]),
        ];
        let period_want = vec![
            (2, vec![]),
            (3, vec![(1..3, 10.0, 1)]),
            (4, vec![(1..4, 10.0, 1)]),
            (5, vec![(1..5, 10.0, 1), (3..5, 14.0, 0)]),
            (6, vec![(1..5, 10.0, 1), (3..6, 14.0, 0), (4..6, 12.0, 0)]),
        ];
        let mut rows = vec![(
            MissedPivots::new(1, 1, 0.5, 20),
            swings,
            swing_want.to_vec(),
        )];
        for period in [
            MissedPivotPeriod::Daily,
            MissedPivotPeriod::Weekly,
            MissedPivotPeriod::Monthly,
        ] {
            let mut data = periods.clone();
            for (i, bar) in data.iter_mut().enumerate() {
                bar.time = match period {
                    MissedPivotPeriod::Weekly => {
                        Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap()
                            + chrono::TimeDelta::weeks(i as i64)
                    }
                    MissedPivotPeriod::Monthly => Utc
                        .with_ymd_and_hms(2026, i as u32 + 1, 1, 0, 0, 0)
                        .unwrap(),
                    _ => bar.time,
                };
            }
            rows.push((
                MissedPivots::new(1, 1, 0.0, 20).with_period(period),
                data,
                period_want.clone(),
            ));
        }
        for (mut missed, data, want) in together_rows.chain(rows) {
            for (seen, want) in want {
                missed.calculate(&data[..seen]);
                assert_eq!(
                    zones(&missed),
                    want,
                    "{:?} after {seen} bars",
                    missed.period
                );
            }
        }
        let missed = MissedPivots::default();
        assert_eq!(
            missed.line_names(),
            ["Daily pivots", "Weekly pivots", "Monthly pivots"]
        );
        let solid = |color| (color, 2.0, crate::model::LineStyle::Solid, true);
        assert_eq!(
            missed
                .settings()
                .plots
                .iter()
                .map(|p| (p.color, p.width, p.pattern, p.visible))
                .collect::<Vec<_>>(),
            [palette::BLUE, palette::DEEP_TEAL, palette::BRIGHT_RED].map(solid)
        );
    }
}
