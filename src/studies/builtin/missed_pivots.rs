//! Missed Pivot Levels.
//!
//! A completed trading period has pivot P = (high + low + close) / 3. If the next period's
//! running high to low range never reaches P, within `tolerance` percent of |P|, that pivot
//! is missed. It is confirmed when another period starts and drawn from the first bar of
//! the period that missed it. A pending pivot is not drawn while its test period is forming.
//! Daily, weekly (ISO weeks) and monthly periods follow exchange trading days in the chart
//! instrument's time zone and session ([`Indicator::set_instrument`]; UTC until supplied).
//! Periods without usable bars are skipped. The first period may be partial.
//!
//! With the period set to swings, levels are swing highs and lows (see
//! [`PivotPointsHighLow`](super::PivotPointsHighLow)):
//!
//! - Confirmed `right` bars after the pivot bar; bars in that window never revisit it.
//! - Missed while every later bar stays below a pivot high or above a pivot low.
//! - Revisited when a later high reaches a high level or a low reaches a low level, within
//!   `tolerance` percent of |P|. A gap beyond the level also counts as reaching it.
//!
//! In both modes a revisited level ends at the first bar reaching it, including the bar that
//! confirms a missed period pivot. The latest `max_levels` levels, missed or revisited, are
//! kept, in the colour of their side (above first, below second). A bar without a finite high
//! and low takes no part; period pivots also require a finite close. A pivot whose sum
//! overflows is skipped. Overlay indicator without per-bar values. The defaults take daily
//! pivots, a tolerance of 0.1% and 20 levels; `new` takes swings of `left` and `right` bars.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, MissedPivots};
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
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use super::clamp_param;
use super::pivot_points_high_low::Swings;
use crate::model::{Bar, Instrument};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyZone};
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
}

/// Missed Pivot Levels: pivot levels that price has not returned to.
#[derive(Clone, Debug)]
pub struct MissedPivots {
    period: MissedPivotPeriod,
    days: ExchangeDays,
    intraday: bool,
    left: usize,
    right: usize,
    tolerance: f64,
    max_levels: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    zones: Vec<StudyZone>,
    /// Missed pivot high, missed pivot low.
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Debug)]
struct Level {
    bar: usize,
    price: f64,
    high: bool,
    revisited: Option<usize>,
}

/// The forming period and the preceding period's pivot, until its range reaches it.
#[derive(Clone, Debug)]
struct Span {
    key: (i32, u32),
    high: f64,
    low: f64,
    close: f64,
    pivot: Option<(usize, f64)>,
}

#[derive(Clone, Debug, Default)]
struct State {
    source: usize,
    previous_date: Option<chrono::NaiveDate>,
    intraday: bool,
    span: Option<Span>,
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
            left: left.max(1),
            right: right.max(1),
            tolerance: clamp_param(tolerance, 0.0, 100.0),
            max_levels: max_levels.clamp(1, MAX_LEVELS),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            zones: Vec::new(),
            colors: [palette::RED, palette::GREEN],
            visible: true,
        }
    }

    /// Takes swing levels or the pivot of each completed day, week or month.
    pub fn with_period(mut self, period: MissedPivotPeriod) -> Self {
        self.period = period;
        self.calculate(&[]);
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for MissedPivots {
    fn default() -> Self {
        Self::new(10, 10, 0.1, 20).with_period(MissedPivotPeriod::Daily)
    }
}

impl Indicator for MissedPivots {
    crate::studies::inputs::study_inputs!(self;
        period: choice("Pivots Timeframe", [MissedPivotPeriod::Swings, MissedPivotPeriod::Daily, MissedPivotPeriod::Weekly, MissedPivotPeriod::Monthly]),
        left: length("Pivot left bars", 1, usize::MAX) if period in [0],
        right: length("Pivot right bars", 1, usize::MAX) if period in [0],
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
        let anchor = match self.period {
            MissedPivotPeriod::Swings => None,
            MissedPivotPeriod::Daily => Some(AnchorPeriod::Session),
            MissedPivotPeriod::Weekly => Some(AnchorPeriod::Week),
            MissedPivotPeriod::Monthly => Some(AnchorPeriod::Month),
        };
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let index = state.source;
                    state.source += 1;
                    // A bar without a finite high and low takes no part, as if it were absent.
                    if !(bar.high.is_finite() && bar.low.is_finite())
                        || (anchor.is_some() && !bar.close.is_finite())
                    {
                        return IndicatorValue::None;
                    }
                    if let Some(anchor) = anchor {
                        let date = bar.time.date_naive();
                        state.intraday |= state.previous_date == Some(date);
                        state.previous_date = Some(date);
                        let key = anchor.key(self.days.day(bar, !self.intraday));
                        match &mut state.span {
                            Some(span) if span.key == key => {
                                span.high = span.high.max(bar.high);
                                span.low = span.low.min(bar.low);
                                span.close = bar.close;
                            }
                            _ => {
                                let done = state.span.take();
                                if let Some(span) = &done
                                    && let Some((bar, price)) = span.pivot
                                {
                                    state.levels.push_back(Level {
                                        bar,
                                        price,
                                        high: price > span.high,
                                        revisited: None,
                                    });
                                }
                                let pivot = done
                                    .map(|span| (span.high + span.low + span.close) / 3.0)
                                    .filter(|p| p.is_finite())
                                    .map(|p| (index, p));
                                state.span = Some(Span {
                                    key,
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
                    if anchor.is_none() {
                        let confirmed = state.swings.push(index, bar, left, right);
                        for (high, pivot) in [(true, confirmed[0]), (false, confirmed[1])] {
                            if let Some((bar, price)) = pivot {
                                state.levels.push_back(Level {
                                    bar,
                                    price,
                                    high,
                                    revisited: None,
                                });
                            }
                        }
                    } else if let Some(span) = &mut state.span
                        && let Some((_, price)) = span.pivot
                    {
                        let reach = price.abs() * tolerance;
                        if span.low <= price + reach && span.high >= price - reach {
                            span.pivot = None;
                        }
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
                plot: usize::from(!level.high),
            })
            .collect();
    }

    fn is_incremental(&self) -> bool {
        true
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
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
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
        vec!["Missed high".to_string(), "Missed low".to_string()]
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
    #[test]
    fn levels_are_confirmed_missed_and_revisited() {
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
        // Bars seen, then the zones (bars, price, plot) at that point.
        type Zone = (std::ops::Range<usize>, f64, usize);
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
        for (mut missed, data, want) in rows {
            for (seen, zones) in want {
                missed.calculate(&data[..seen]);
                let got: Vec<_> = missed
                    .zones()
                    .iter()
                    .map(|z| (z.bars.clone(), z.top, z.plot))
                    .collect();
                assert_eq!(got, zones, "{:?} after {seen} bars", missed.period);
            }
        }
    }
}
