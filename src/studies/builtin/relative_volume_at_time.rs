//! Relative Volume at Time
//!
//! Compares the volume traded so far in the current period, by default the trading day, with
//! the average volume traded by the same point of the previous periods. A reading of 2 at
//! 10:30 means twice the usual volume by 10:30, so unusual activity shows early in the day
//! instead of once the day is complete.
//!
//! # Calculation
//! For a bar at offset `d` into its period, with `n` the length:
//!
//! - current = the volume from the start of the period through the bar (Cumulative mode,
//!   the default), or the bar's own volume (Regular mode)
//! - past_k = the same quantity at the last bar of the k-th previous period whose offset is
//!   at most `d`, or 0 when that period has no bar by then
//! - RVOL = current / ((past_1 + ... + past_n) / n)
//!
//! The offset of a bar is the time elapsed since the open of its trading day, plus one day for
//! each calendar day from the first day of the period (the Monday of a week, the 1st of a
//! month) to that trading day, so a week's Tuesday 10:30 lines up with every other Tuesday
//! 10:30. Trading days follow the instrument's time zone and session, as for the VWAP. Bars of
//! a day or longer are dated by their UTC date and have no intraday part, so with the day
//! anchor every daily bar is a period of its own and the study compares each bar's volume with
//! the average of the previous `n` bars. There is no value before `n` previous periods, or when
//! their average is zero.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, RelativeVolumeAtTime};
//!
//! // Two hourly bars a day; the third day trades twice the volume of the first two.
//! let bars: Vec<Bar> = [(0, 100.0), (1, 300.0), (24, 100.0), (25, 300.0), (48, 200.0), (49, 600.0)]
//!     .into_iter()
//!     .map(|(h, v)| Bar::new(Utc.timestamp_opt(h * 3_600, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, v))
//!     .collect();
//! let mut study = RelativeVolumeAtTime::new(2);
//! study.calculate(&bars);
//! assert_eq!(study.values().value(0, 3), None);
//! assert_eq!(study.values().value(0, 4), Some(2.0));
//! assert_eq!(study.values().value(0, 5), Some(2.0));
//! ```

use crate::model::{Bar, BarClock, ExchangeSessionProvider, Instrument, Session, TimezoneId};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use chrono::{Datelike, NaiveDate};
use egui::Color32;

const DAY_MS: i64 = 86_400_000;

/// The period at whose start [`RelativeVolumeAtTime`] restarts its comparison.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RelativeVolumeAnchor {
    /// Each trading day.
    #[default]
    Day,
    /// Each ISO week, Monday to Sunday, of trading days.
    Week,
    /// Each calendar month of trading days.
    Month,
}

impl RelativeVolumeAnchor {
    /// The period that holds trading day `day`, and the days from the period's first day.
    fn locate(self, day: NaiveDate) -> ((i32, u32), u32) {
        match self {
            Self::Day => ((day.year(), day.ordinal()), 0),
            Self::Week => (
                (day.iso_week().year(), day.iso_week().week()),
                day.weekday().num_days_from_monday(),
            ),
            Self::Month => ((day.year(), day.month()), day.day0()),
        }
    }
}

/// The volume compared by [`RelativeVolumeAtTime`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RelativeVolumeMode {
    /// Volume accumulated since the start of the period.
    #[default]
    Cumulative,
    /// The volume of the bar alone.
    Regular,
}

/// Volume relative to its average at the same point of the previous periods, as columns
/// around a level at 1.
///
/// A bar with a non-finite volume has no value and adds nothing. The chart supplies the
/// instrument's time zone and session through [`Indicator::set_instrument`] and its interval
/// through [`Indicator::set_bar_clock`]. Without them trading days are UTC days, and direct
/// callers on daily bars stamped away from UTC midnight pass a daily clock.
#[derive(Clone)]
pub struct RelativeVolumeAtTime {
    length: usize,
    anchor: RelativeVolumeAnchor,
    mode: RelativeVolumeMode,
    instrument: Option<(TimezoneId, Option<Session>)>,
    /// Whether the chart interval is a day or longer.
    daily: bool,
    values: IndicatorValues,
    stepper: Stepper<State>,
    periods: std::collections::VecDeque<Vec<(i64, f64)>>,
    undo: Undo,
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    key: Option<(i32, u32)>,
    cumulative: f64,
    calendar_date: Option<NaiveDate>,
    opens: Vec<(chrono::DateTime<chrono::Utc>, NaiveDate)>,
}

#[derive(Clone, Default)]
struct Undo {
    appended: bool,
    new_period: bool,
    expired: Option<Vec<(i64, f64)>>,
}

impl RelativeVolumeAtTime {
    /// Cumulative daily relative volume over `length` previous days (zero becomes one).
    pub fn new(length: usize) -> Self {
        Self {
            length: length.max(1),
            anchor: RelativeVolumeAnchor::Day,
            mode: RelativeVolumeMode::Cumulative,
            instrument: None,
            daily: false,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            periods: std::collections::VecDeque::new(),
            undo: Undo::default(),
            colors: [palette::UP, palette::DOWN],
            visible: true,
        }
    }

    /// Restarts at each selected period.
    pub fn with_anchor(mut self, anchor: RelativeVolumeAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Compares cumulative or single-bar volume.
    pub fn with_mode(mut self, mode: RelativeVolumeMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Default for RelativeVolumeAtTime {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for RelativeVolumeAtTime {
    crate::studies::inputs::study_inputs!(self;
        anchor: choice("Anchor", [RelativeVolumeAnchor::Day, RelativeVolumeAnchor::Week, RelativeVolumeAnchor::Month]),
        length: length("Length", 1, usize::MAX),
        mode: choice("Calculation mode", [RelativeVolumeMode::Cumulative, RelativeVolumeMode::Regular])
    ; {});

    fn name(&self) -> &str {
        "Relative Volume at Time"
    }

    fn desc(&self) -> &str {
        "Relative Volume at Time - Volume against its average at the same time of past periods"
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[0],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn plot_color(&self, _line: usize, bar: usize) -> Option<Color32> {
        let value = self.values.value(0, bar)?;
        Some(self.colors[usize::from(value < 1.0)])
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![(1.0, Color32::from_gray(128), "Equality")],
            ..Default::default()
        }
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        if len > 0 && from == len && data.len() == len {
            return;
        }
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        if start == 0 {
            self.periods.clear();
        } else if start < len {
            if self.undo.appended {
                self.periods.back_mut().unwrap().pop();
            }
            if self.undo.new_period {
                self.periods.pop_back();
            }
            if let Some(expired) = self.undo.expired.take() {
                self.periods.push_front(expired);
            }
        }
        self.undo = Undo::default();
        let provider = (!self.daily).then(|| {
            let (zone, session) = self
                .instrument
                .clone()
                .unwrap_or_else(|| (TimezoneId::utc(), None));
            ExchangeSessionProvider::new(zone, session)
        });
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let source = state.source;
                state.source += 1;
                if !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                if !self.daily && state.calendar_date != Some(bar.time.date_naive()) {
                    state.opens = provider.as_ref().unwrap().opens([bar.time]);
                    state.calendar_date = Some(bar.time.date_naive());
                }
                let (open, day) = if self.daily {
                    (bar.time, bar.time.date_naive())
                } else {
                    match state.opens.partition_point(|(open, _)| *open <= bar.time) {
                        0 => (bar.time, bar.time.date_naive()),
                        n => state.opens[n - 1],
                    }
                };
                let (period, days) = self.anchor.locate(day);
                let offset = i64::from(days) * DAY_MS + (bar.time - open).num_milliseconds();
                let last = source + 1 == data.len();
                if state.key != Some(period) {
                    state.key = Some(period);
                    state.cumulative = 0.0;
                    self.periods.push_back(Vec::new());
                    let expired = if self.periods.len() > self.length.saturating_add(1) {
                        self.periods.pop_front()
                    } else {
                        None
                    };
                    if last {
                        self.undo.new_period = true;
                        self.undo.expired = expired;
                    }
                }
                state.cumulative += bar.volume;
                let current = match self.mode {
                    RelativeVolumeMode::Cumulative => state.cumulative,
                    RelativeVolumeMode::Regular => bar.volume,
                };
                self.periods.back_mut().unwrap().push((offset, current));
                if last {
                    self.undo.appended = true;
                }
                if self.periods.len() <= self.length {
                    return IndicatorValue::None;
                }
                let total: f64 = self
                    .periods
                    .iter()
                    .take(self.length)
                    .map(|past| match past.partition_point(|&(o, _)| o <= offset) {
                        0 => 0.0,
                        n => past[n - 1].1,
                    })
                    .sum();
                let average = total / self.length as f64;
                if average != 0.0 {
                    IndicatorValue::Single(current / average)
                } else {
                    IndicatorValue::None
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let next = instrument.map(|instrument| {
            (
                instrument.time_zone.clone().unwrap_or_else(TimezoneId::utc),
                instrument.session.clone(),
            )
        });
        let changed = next != self.instrument;
        self.instrument = next;
        changed
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let daily = clock.timeframe().is_some_and(|timeframe| {
            timeframe.duration_ms() >= crate::model::Timeframe::Day1.duration_ms()
        });
        let changed = daily != self.daily;
        self.daily = daily;
        changed
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("RVOL({})", self.length)]
    }
}
