//! Auto Key Levels.
//!
//! Horizontal lines at the key levels of the current and of the previous trading day, week and
//! month, and of one custom period, each shown by its own input. For each period the levels are
//! the point of control (POC) and the low and high of the value area (VAL, VAH) of its volume
//! profile with 24 rows and a 70% value-area target, and its open, high, low and close.
//! The study requests finer candles from the host through [`Indicator::data_requests`];
//! chart bars without finer candles use their high to low volume estimate, as
//! [`VolProfilePeriodic`](super::VolProfilePeriodic) does.
//! A period without volume has no POC, VAL or VAH.
//!
//! The current period is the forming one, so its levels follow each new bar; the previous period
//! is the last one before it that has bars. Periods are exchange trading days, as
//! [`VolumeWeightedAvgPrice`](super::VolumeWeightedAvgPrice) counts them, in the time zone and
//! session of the chart instrument ([`Indicator::set_instrument`]; UTC until then). The custom
//! period is the last Length days, weeks or months, the forming one included, or with the anchor
//! set to earnings, split or dividends the bars since the last such chart event
//! ([`Indicator::set_time_scale_events`]) at or before the last bar; without one it is not drawn.
//!
//! Each level runs from the first bar of its period, or with Levels start from set to the
//! current bar from the last bar, to the visible right edge. Without a viewport, period lines
//! stop at the last bar and current-bar lines project 20 bars right. A coloured tag at its
//! start carries its short name: `dPOC`, `wVAH` or `mO` for the current day, week or month,
//! `pdPOC`, `pwL` or `pmC` for the previous one and `Custom POC` for the custom period. Levels
//! take the colour, width and visibility of their period's plot.
//! Monthly levels are drawn over weekly ones and weekly over daily ones. A bar without a finite
//! open, high, low and close takes no part. The defaults show POC, VAL and VAH of the current
//! day and week from the start of each period. Overlay indicator without per-bar values;
//! alerts read each drawn level by its tag, such as `Auto Key Levels: dPOC`, at the level's
//! price on the bars it covers. Enabled periods without enough history or an event anchor
//! show a notice while the available periods keep drawing.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{AutoKeyLevels, Indicator};
//!
//! let time = Utc.with_ymd_and_hms(2026, 2, 2, 9, 0, 0).unwrap();
//! let mut levels = AutoKeyLevels::new();
//! levels.calculate(&[Bar::new(time, 10.0, 12.0, 8.0, 11.0, 100.0)]);
//! let tags: Vec<_> = levels.labels().iter().map(|tag| tag.text.as_str()).collect();
//! assert_eq!(tags, ["dPOC", "dVAL", "dVAH", "wPOC", "wVAL", "wVAH"]);
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use super::lower_interval;
use crate::data::{VolumeProfile, VolumeProfileConfig};
use crate::model::{Bar, BarClock, EventKind, Instrument, TimeScaleEvent, Timeframe};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, LabelPlacement, StudyCell, StudyData,
    StudyDataRequest, StudyLabel, StudyTable, StudyZone,
};
use chrono::{DateTime, NaiveDate, Utc};
use egui::Color32;
use std::collections::VecDeque;
use std::ops::Range;

/// The timeframes, from the lowest drawn to the highest, and the prefix of their labels.
const PERIODS: [(AnchorPeriod, &str); 3] = [
    (AnchorPeriod::Session, "d"),
    (AnchorPeriod::Week, "w"),
    (AnchorPeriod::Month, "m"),
];

/// The levels of a period, in the order [`levels_of`] returns them.
const LEVELS: [&str; 7] = ["POC", "VAL", "VAH", "O", "H", "L", "C"];

/// Plots style periods. Alerts read level `k` of plot `p` at
/// `PLOTS.len() + p * LEVELS.len() + k`, past the plots, which hold no values.
const PLOTS: [&str; 7] = [
    "Day",
    "Week",
    "Month",
    "Prev Day",
    "Prev Week",
    "Prev Month",
    "Custom",
];

/// Projection used by direct callers until a visible range is supplied.
const RIGHT_BARS: usize = 20;

/// The most custom periods that can be counted back.
const MAX_PERIODS: usize = 10_000;

/// Where each level starts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LevelsStart {
    /// The first bar of its period.
    Period,
    /// The last bar, running to the right.
    CurrentBar,
}

impl LevelsStart {
    fn label(self) -> &'static str {
        match self {
            Self::Period => "Start of period",
            Self::CurrentBar => "Current bar",
        }
    }
}

impl std::fmt::Debug for LevelsStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Where the custom period starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CustomAnchor {
    /// The last Length periods of the chosen unit.
    Custom,
    /// The last earnings event.
    Earnings,
    /// The last split or consolidation.
    Split,
    /// The last dividend.
    Dividends,
}

impl CustomAnchor {
    fn label(self) -> &'static str {
        match self {
            Self::Custom => "Custom",
            Self::Earnings => "Earnings",
            Self::Split => "Split",
            Self::Dividends => "Dividends",
        }
    }
}

/// The unit the custom period counts.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CustomUnit(AnchorPeriod);

impl CustomUnit {
    fn label(self) -> &'static str {
        match self.0 {
            AnchorPeriod::Session => "Days",
            AnchorPeriod::Week => "Weeks",
            _ => "Months",
        }
    }
}

impl std::fmt::Debug for CustomUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

const UNITS: [CustomUnit; 3] = [
    CustomUnit(AnchorPeriod::Session),
    CustomUnit(AnchorPeriod::Week),
    CustomUnit(AnchorPeriod::Month),
];

/// POC, VAL and VAH (absent without volume), then open, high, low and close of a period.
type Levels = [Option<f64>; 7];

/// Auto Key Levels: profile and price levels of the current and previous day, week and month.
#[derive(Clone, Debug)]
pub struct AutoKeyLevels {
    /// Current day, week and month.
    current: [bool; 3],
    /// POC, VAL, VAH and OHLC of the current periods.
    current_levels: [bool; 12],
    /// Previous day, week and month.
    previous: [bool; 3],
    /// POC, VAL, VAH and OHLC of the previous periods.
    previous_levels: [bool; 12],
    custom: bool,
    /// POC, VAL, VAH and OHLC of the custom period.
    custom_levels: [bool; 4],
    anchor: CustomAnchor,
    length: usize,
    unit: CustomUnit,
    start: LevelsStart,
    right_edge: Option<usize>,
    days: ExchangeDays,
    intraday: bool,
    clock: BarClock,
    finer: Vec<Bar>,
    tick_size: Option<f64>,
    notice: Option<StudyTable>,
    /// Earnings, split and dividend times of the chart.
    events: Vec<(EventKind, DateTime<Utc>)>,
    /// Levels of the bar ranges drawn by the last run.
    cache: Vec<(Range<usize>, Option<Levels>)>,
    values: IndicatorValues,
    stepper: Stepper<State>,
    zones: Vec<StudyZone>,
    /// The alert output and name of each zone, in zone order.
    outputs: Vec<(usize, String)>,
    labels: Vec<StudyLabel>,
    /// Current day, week, month; previous day, week, month; custom.
    colors: [Color32; 7],
    visible: bool,
}

/// The key and first bar of a period.
type Start = ((i32, u32), usize);

#[derive(Clone, Debug, Default)]
struct State {
    source: usize,
    previous_date: Option<NaiveDate>,
    intraday: bool,
    /// The current and the previous period of each timeframe of `PERIODS`.
    periods: [[Option<Start>; 2]; 3],
    /// The last `length` periods of the custom unit, oldest first.
    custom: VecDeque<(i64, usize)>,
}

impl AutoKeyLevels {
    /// POC, VAL and VAH of the current day and week, from the start of each period.
    pub fn new() -> Self {
        Self {
            current: [true, true, false],
            current_levels: [
                true, true, true, false, true, true, true, false, false, false, false, false,
            ],
            previous: [false; 3],
            previous_levels: [false; 12],
            custom: false,
            custom_levels: [false; 4],
            anchor: CustomAnchor::Custom,
            length: 1,
            unit: UNITS[0],
            start: LevelsStart::Period,
            right_edge: None,
            days: ExchangeDays::default(),
            intraday: false,
            clock: BarClock::new(Timeframe::Min1),
            finer: Vec::new(),
            tick_size: None,
            notice: None,
            events: Vec::new(),
            cache: Vec::new(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            zones: Vec::new(),
            outputs: Vec::new(),
            labels: Vec::new(),
            colors: [
                palette::ORANGE,
                palette::BLUE,
                palette::PURPLE,
                palette::ORANGE.gamma_multiply_u8(153),
                palette::BLUE.gamma_multiply_u8(153),
                palette::PURPLE.gamma_multiply_u8(153),
                palette::YELLOW,
            ],
            visible: true,
        }
    }

    /// The profile source interval, selected from the chart interval.
    fn lower_timeframe(&self) -> Option<Timeframe> {
        let interval = self.clock.timeframe()?;
        let seconds = interval.duration_ms() / 1_000;
        let lower = match seconds {
            0 => return None,
            1..=10 => Timeframe::Sec1,
            11..=15 => Timeframe::Sec5,
            16..=59 => Timeframe::Custom(std::num::NonZeroU64::new(15).unwrap()),
            60..=959 => Timeframe::Min1,
            960..=1_859 => Timeframe::Min5,
            1_860..=3_659 => Timeframe::Custom(std::num::NonZeroU64::new(600).unwrap()),
            3_660..=7_259 => Timeframe::Min15,
            7_260..=14_459 => Timeframe::Min30,
            _ => Timeframe::Hour1,
        };
        (lower.duration_ms() < interval.duration_ms()).then_some(lower)
    }

    /// The first bar of the custom period of `data`, if it has one.
    fn custom_start(&self, data: &[Bar]) -> Option<usize> {
        let kind = match self.anchor {
            CustomAnchor::Custom => return self.stepper.state().custom.front().map(|s| s.1),
            CustomAnchor::Earnings => EventKind::Earnings,
            CustomAnchor::Split => EventKind::Split,
            CustomAnchor::Dividends => EventKind::Dividend,
        };
        let last = data.last()?.time;
        let event = self
            .events
            .iter()
            .filter(|(k, time)| *k == kind && *time <= last)
            .map(|(_, time)| *time)
            .max()?;
        Some(data.partition_point(|bar| bar.time < event))
    }
}

/// Construct with the conventional default parameters.
impl Default for AutoKeyLevels {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `bar` takes part in the levels.
fn usable(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close]
        .iter()
        .all(|v| v.is_finite())
}

/// The levels of `bars`, or `None` when none of them is usable.
fn levels_of(bars: &[Bar], profile_bars: Option<&[Bar]>, tick_size: Option<f64>) -> Option<Levels> {
    let mut usable = bars.iter().filter(|bar| usable(bar));
    let first = usable.next()?;
    let (high, low, close) = usable.fold((first.high, first.low, first.close), |acc, bar| {
        (acc.0.max(bar.high), acc.1.min(bar.low), bar.close)
    });
    let config = VolumeProfileConfig::default();
    // Whole ticks per row, choosing the count nearest 24 rows. Chart prices bound the grid;
    // finer candles replace their volume distribution without changing the period's OHLC.
    let row_size = tick_size
        .map(|tick| {
            let target = ((high - low) / tick / config.rows() as f64).max(1.0);
            let down = target.floor();
            let up = target.ceil();
            let rows = |ticks: f64| ((high - low) / (ticks * tick)).ceil();
            let chosen = if (rows(down) - config.rows() as f64).abs()
                < (rows(up) - config.rows() as f64).abs()
            {
                down
            } else {
                up
            };
            chosen * tick
        })
        .filter(|height| height.is_finite() && *height > 0.0);
    let profile = VolumeProfile::from_bars(
        profile_bars.unwrap_or(bars),
        config.with_row_size(row_size)?,
    )
    .and_then(|profile| {
        let (poc, _) = profile.poc_and_value_area(config.value_area())?;
        let volume = profile.volume();
        let budget = volume.iter().sum::<f64>() * config.value_area();
        let (mut va_low, mut va_high, mut included) = (poc, poc, volume[poc]);
        // Keep the POC even when it alone exceeds the target. Grow by one adjacent row,
        // taking the greater volume, then the nearer row, then the upper row on a tie.
        while included < budget {
            let below = va_low.checked_sub(1);
            let above = (va_high + 1 < volume.len()).then_some(va_high + 1);
            let next = match (below, above) {
                (None, None) => break,
                (Some(row), None) | (None, Some(row)) => row,
                (Some(low), Some(high)) => {
                    if volume[high] > volume[low]
                        || (volume[high] == volume[low] && high - poc <= poc - low)
                    {
                        high
                    } else {
                        low
                    }
                }
            };
            // Stop before the selected row would exceed the remaining volume budget.
            if volume[next] > budget - included {
                break;
            }
            included += volume[next];
            va_low = va_low.min(next);
            va_high = va_high.max(next);
        }
        let (bottom, height) = (profile.low(), profile.row_height());
        Some([
            bottom + (poc as f64 + 0.5) * height,
            bottom + va_low as f64 * height,
            bottom + (va_high + 1) as f64 * height,
        ])
    });
    let [poc, val, vah] = profile.map_or([None; 3], |levels| levels.map(Some));
    Some([
        poc,
        val,
        vah,
        Some(first.open),
        Some(high),
        Some(low),
        Some(close),
    ])
}

impl Indicator for AutoKeyLevels {
    crate::studies::inputs::study_inputs!(self;
        current[0]: toggle("Day"),
        current[1]: toggle("Week"),
        current[2]: toggle("Month"),
        current_levels[0]: toggle("Day POC") if current[0] in [1],
        current_levels[1]: toggle("Day VAL") if current[0] in [1],
        current_levels[2]: toggle("Day VAH") if current[0] in [1],
        current_levels[3]: toggle("Day OHLC") if current[0] in [1],
        current_levels[4]: toggle("Week POC") if current[1] in [1],
        current_levels[5]: toggle("Week VAL") if current[1] in [1],
        current_levels[6]: toggle("Week VAH") if current[1] in [1],
        current_levels[7]: toggle("Week OHLC") if current[1] in [1],
        current_levels[8]: toggle("Month POC") if current[2] in [1],
        current_levels[9]: toggle("Month VAL") if current[2] in [1],
        current_levels[10]: toggle("Month VAH") if current[2] in [1],
        current_levels[11]: toggle("Month OHLC") if current[2] in [1],
        previous[0]: toggle("Prev Day"),
        previous[1]: toggle("Prev Week"),
        previous[2]: toggle("Prev Month"),
        previous_levels[0]: toggle("Prev Day POC") if previous[0] in [1],
        previous_levels[1]: toggle("Prev Day VAL") if previous[0] in [1],
        previous_levels[2]: toggle("Prev Day VAH") if previous[0] in [1],
        previous_levels[3]: toggle("Prev Day OHLC") if previous[0] in [1],
        previous_levels[4]: toggle("Prev Week POC") if previous[1] in [1],
        previous_levels[5]: toggle("Prev Week VAL") if previous[1] in [1],
        previous_levels[6]: toggle("Prev Week VAH") if previous[1] in [1],
        previous_levels[7]: toggle("Prev Week OHLC") if previous[1] in [1],
        previous_levels[8]: toggle("Prev Month POC") if previous[2] in [1],
        previous_levels[9]: toggle("Prev Month VAL") if previous[2] in [1],
        previous_levels[10]: toggle("Prev Month VAH") if previous[2] in [1],
        previous_levels[11]: toggle("Prev Month OHLC") if previous[2] in [1],
        custom: toggle("Custom"),
        custom_levels[0]: toggle("Custom POC") if custom in [1],
        custom_levels[1]: toggle("Custom VAL") if custom in [1],
        custom_levels[2]: toggle("Custom VAH") if custom in [1],
        custom_levels[3]: toggle("Custom OHLC") if custom in [1],
        anchor: choice("Anchor", [
            CustomAnchor::Custom,
            CustomAnchor::Earnings,
            CustomAnchor::Split,
            CustomAnchor::Dividends,
        ]) if custom in [1],
        length: length("Length", 1, MAX_PERIODS) if anchor in [0],
        unit: choice("Unit", UNITS) if anchor in [0],
        start: choice("Levels start from", [LevelsStart::Period, LevelsStart::CurrentBar])
    ; { self.cache.clear(); });

    fn name(&self) -> &str {
        "Auto Key Levels"
    }

    fn desc(&self) -> &str {
        "Auto Key Levels - Profile and price levels of the current and previous day, week and month"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.finer.clear();
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], mut from: usize) {
        if from < self.values.len().saturating_sub(1) || data.len() < self.values.len() {
            from = 0;
        }
        let (unit, length) = (self.unit.0, self.length);
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let index = state.source;
                    state.source += 1;
                    if !usable(bar) {
                        return IndicatorValue::None;
                    }
                    let date = bar.time.date_naive();
                    state.intraday |= state.previous_date == Some(date);
                    state.previous_date = Some(date);
                    let day = self.days.day(bar, !self.intraday);
                    for ((period, _), [current, previous]) in PERIODS.iter().zip(&mut state.periods)
                    {
                        let key = period.key(day);
                        if current.is_none_or(|(at, _)| at != key) {
                            *previous = current.replace((key, index));
                        }
                    }
                    let key = unit.index(day);
                    if state.custom.back().is_none_or(|(at, _)| *at != key) {
                        state.custom.push_back((key, index));
                        while state
                            .custom
                            .front()
                            .is_some_and(|(first, _)| key - first >= length as i64)
                        {
                            state.custom.pop_front();
                        }
                    }
                    IndicatorValue::None
                });
            if self.intraday == self.stepper.state().intraday {
                break;
            }
            // Switching the inferred interval changes the exchange dates of earlier bars too.
            self.intraday = self.stepper.state().intraday;
            from = 0;
        }

        self.notice = None;
        let mut missing = Vec::new();
        let source = self.stepper.state().source;
        // Each group: its plot, label prefix, bar range and levels shown.
        let mut groups = Vec::new();
        if self.custom {
            if let Some(start) = self.custom_start(data) {
                groups.push((6, "Custom ".to_owned(), start..source, self.custom_levels));
            } else {
                missing.push(format!("Custom ({})", self.anchor.label()));
            }
        }
        for (t, ((_, prefix), [current, previous], shown)) in PERIODS
            .iter()
            .zip(&self.stepper.state().periods)
            .zip(self.current.into_iter().zip(self.previous))
            .map(|((period, starts), shown)| (period, starts, shown))
            .enumerate()
        {
            let max_interval = [Timeframe::Day1, Timeframe::Week1, Timeframe::Month1][t];
            let too_coarse = self
                .clock
                .timeframe()
                .is_some_and(|tf| tf.duration_ms() > max_interval.duration_ms());
            for (plot, on, available) in [
                (t, shown.0, current.is_some()),
                (3 + t, shown.1, previous.is_some()),
            ] {
                if on && (too_coarse || !available) {
                    missing.push(PLOTS[plot].to_owned());
                }
            }
            if too_coarse {
                continue;
            }
            let Some((_, start)) = *current else { continue };
            if shown.0 {
                groups.push((
                    t,
                    (*prefix).to_owned(),
                    start..source,
                    self.current_levels[t * 4..t * 4 + 4].try_into().unwrap(),
                ));
            }
            if let Some((_, first)) = previous.filter(|_| shown.1) {
                groups.push((
                    3 + t,
                    format!("p{prefix}"),
                    first..start,
                    self.previous_levels[t * 4..t * 4 + 4].try_into().unwrap(),
                ));
            }
        }
        if !missing.is_empty() {
            self.notice = Some(StudyTable {
                title: "Insufficient period data".to_owned(),
                columns: vec!["Period".to_owned()],
                rows: missing
                    .into_iter()
                    .map(|period| {
                        vec![StudyCell::Text(format!(
                            "{period}: load more data, use shorter bars or turn off this period"
                        ))]
                    })
                    .collect(),
                ..StudyTable::default()
            });
        }

        // Ranges that end before `from` hold the same bars as in the last run.
        let mut cache = std::mem::take(&mut self.cache);
        cache.retain(|(bars, _)| bars.end <= from);
        self.zones.clear();
        self.outputs.clear();
        self.labels.clear();
        for (plot, prefix, bars, shown) in groups {
            let levels = match cache.iter().find(|(cached, _)| *cached == bars) {
                Some((_, levels)) => *levels,
                None => {
                    let mut profile_bars = Vec::new();
                    if let Some(lower) = self.lower_timeframe().filter(|_| !self.finer.is_empty()) {
                        let nested =
                            lower_interval::nest(&self.clock, lower, data, &self.finer, bars.start);
                        for (bar, range) in data[bars.clone()].iter().zip(nested) {
                            if range.is_empty() {
                                profile_bars.push(bar.clone());
                            } else {
                                profile_bars.extend_from_slice(&self.finer[range]);
                            }
                        }
                    }
                    levels_of(
                        &data[bars.clone()],
                        (!profile_bars.is_empty()).then_some(profile_bars.as_slice()),
                        self.tick_size,
                    )
                }
            };
            self.cache.push((bars.clone(), levels));
            let span = match self.start {
                LevelsStart::Period => bars.start..self.right_edge.unwrap_or(source).max(source),
                LevelsStart::CurrentBar => {
                    source - 1
                        ..self
                            .right_edge
                            .unwrap_or(source - 1 + RIGHT_BARS)
                            .max(source)
                }
            };
            for (k, price) in levels.into_iter().flatten().enumerate() {
                let Some(price) = price.filter(|_| shown[k.min(3)]) else {
                    continue;
                };
                let text = format!("{prefix}{}", LEVELS[k]);
                self.zones.push(StudyZone {
                    bars: span.clone(),
                    top: price,
                    bottom: price,
                    plot,
                });
                self.outputs
                    .push((PLOTS.len() + plot * LEVELS.len() + k, text.clone()));
                self.labels.push(StudyLabel {
                    bar: span.start,
                    price,
                    text,
                    placement: LabelPlacement::AboveLeft,
                    plot,
                });
            }
        }
        // The renderer clips by bar; stable ties retain the monthly-over-weekly draw order.
        self.labels.sort_by_key(|label| label.bar);
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let tick = instrument
            .and_then(|instrument| instrument.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0);
        let changed = tick != self.tick_size;
        self.tick_size = tick;
        let changed = self.days.set(instrument) || changed;
        if changed {
            self.cache.clear();
        }
        changed
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.set_bar_clock(&self.clock.for_timeframe(timeframe));
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        if changed {
            self.cache.clear();
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
        let finer = match self.data_requests().first() {
            Some(request) => match lower_interval::reply(request, from, sources) {
                Some(bars) => bars,
                None => return,
            },
            None => &[],
        };
        self.finer = finer.to_vec();
        self.update(data, from);
    }

    fn set_time_scale_events(&mut self, events: &[TimeScaleEvent]) -> bool {
        let pairs = || {
            events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        EventKind::Earnings | EventKind::Dividend | EventKind::Split
                    )
                })
                .map(|event| (event.kind, event.time))
        };
        // Called every frame: no allocation while the events are unchanged.
        if self.events.iter().copied().eq(pairs()) {
            return false;
        }
        self.events = pairs().collect();
        self.custom && self.anchor != CustomAnchor::Custom
    }

    fn set_visible_logical_range(&mut self, range: crate::model::LogicalRange) {
        let end = range.to_strict_range().1;
        if self.right_edge != Some(end) {
            self.right_edge = Some(end);
            // Only the right ends follow the viewport; the levels stay as calculated.
            let end = end.max(self.stepper.state().source);
            for zone in &mut self.zones {
                zone.bars.end = end;
            }
        }
    }

    fn legend_lines(&self) -> Vec<usize> {
        Vec::new()
    }

    fn plot_holds_values(&self, _line: usize) -> bool {
        false
    }

    fn table(&self) -> Option<&StudyTable> {
        self.notice.as_ref()
    }

    fn zones(&self) -> &[StudyZone] {
        &self.zones
    }

    fn labels_use_plot_background(&self) -> bool {
        true
    }

    fn labels(&self) -> &[StudyLabel] {
        &self.labels
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

    // Its labels sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        PLOTS.map(String::from).to_vec()
    }

    fn alert_outputs(&self) -> Vec<(usize, String)> {
        let mut outputs = self.outputs.clone();
        outputs.sort_by_key(|(output, _)| *output);
        outputs
    }

    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        let at = self
            .outputs
            .iter()
            .position(|(output, _)| *output == line)?;
        let zone = &self.zones[at];
        zone.bars.contains(&bar).then_some(zone.top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TimezoneId;
    use crate::studies::{InputValue, StudyTemplate};
    use chrono::TimeZone;

    /// A level: its label, bars, price and plot.
    type Level = (String, Range<usize>, f64, usize);
    /// Name, bars seen, whether the instrument is in Tokyo, inputs and expected levels.
    type Row = (
        &'static str,
        usize,
        bool,
        Vec<(&'static str, InputValue)>,
        Vec<Level>,
    );

    const ALL: [&str; 7] = LEVELS;
    const OHLC: [&str; 4] = ["O", "H", "L", "C"];

    fn group(
        prefix: &str,
        bars: Range<usize>,
        plot: usize,
        names: &[&str],
        prices: &[f64],
    ) -> Vec<Level> {
        names
            .iter()
            .zip(prices)
            .map(|(name, price)| (format!("{prefix}{name}"), bars.clone(), *price, plot))
            .collect()
    }

    /// Point bars, so that each puts its volume in the row of its price, and every period
    /// spans 24 price units: rows of 1. The POC is the middle of the heaviest row and the
    /// value area that row alone when it holds 70% of the volume. Feb 2 2026 opens a new
    /// day, ISO week and month.
    fn bars() -> Vec<Bar> {
        [
            ((1, 30, 12), 10.0, 3.0),
            ((1, 30, 18), 34.0, 1.0),
            ((2, 2, 0), 54.0, 3.0),
            ((2, 2, 6), 30.0, 1.0),
            ((2, 3, 0), 30.0, 3.0),
            ((2, 3, 6), 54.0, 1.0),
        ]
        .into_iter()
        .map(|((month, day, hour), price, volume)| {
            let time = Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap();
            Bar::new(time, price, price, price, price, volume)
        })
        .collect()
    }

    /// Hand-calculated price and 24-row profile levels across a day, week and month boundary.
    /// Jan 30 puts 3/4 of its volume at 10: POC 10.5, value area [10, 11]. Feb 2 puts 3/4
    /// at 54: POC 53.5, area [53, 54]. Feb 2 and 3 together tie the endpoint rows at 4:
    /// POC 30.5 (the lower tie), value area [30, 53]: the last row would exceed the budget.
    /// Tokyo splits the Jan 30 UTC bars.
    #[test]
    fn levels_follow_each_period_and_input() {
        use InputValue::{Choice, Length, Toggle};
        let data = bars();
        let at = |month, day| Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0).unwrap();
        let events = [
            TimeScaleEvent::new("d", at(2, 2), EventKind::Dividend, ""),
            TimeScaleEvent::new("s", at(1, 30), EventKind::Split, ""),
            TimeScaleEvent::new("e", at(2, 3), EventKind::Earnings, ""),
            TimeScaleEvent::new("later", at(3, 2), EventKind::Earnings, ""),
        ];
        let tokyo =
            Instrument::new("TOKYO").with_time_zone(TimezoneId::fixed_offset(9 * 3600).unwrap());
        let jan30 = [10.5, 10.0, 11.0, 10.0, 34.0, 10.0, 34.0];
        // Each row first disables all periods; the level selections keep their defaults.
        let rows: Vec<Row> = vec![
            (
                "empty",
                0,
                false,
                vec![("current [0]", Toggle(true))],
                vec![],
            ),
            (
                "current day profile",
                2,
                false,
                vec![("current [0]", Toggle(true))],
                group("d", 0..2, 0, &ALL[..3], &jan30[..3]),
            ),
            (
                "day moves on its first bar",
                3,
                false,
                vec![
                    ("current [0]", Toggle(true)),
                    ("current_levels [0]", Toggle(false)),
                    ("current_levels [1]", Toggle(false)),
                    ("current_levels [2]", Toggle(false)),
                    ("current_levels [3]", Toggle(true)),
                ],
                group("d", 2..3, 0, &OHLC, &[54.0; 4]),
            ),
            (
                "previous day",
                6,
                false,
                vec![
                    ("previous [0]", Toggle(true)),
                    ("previous_levels [3]", Toggle(true)),
                ],
                group("pd", 2..6, 3, &OHLC, &[54.0, 54.0, 30.0, 30.0]),
            ),
            (
                "current week",
                6,
                false,
                vec![
                    ("current [1]", Toggle(true)),
                    ("current_levels [4]", Toggle(false)),
                    ("current_levels [7]", Toggle(true)),
                ],
                group(
                    "w",
                    2..6,
                    1,
                    &ALL[1..],
                    &[30.0, 53.0, 54.0, 54.0, 30.0, 54.0],
                ),
            ),
            (
                "previous week",
                6,
                false,
                vec![
                    ("previous [1]", Toggle(true)),
                    ("previous_levels [4]", Toggle(true)),
                    ("previous_levels [5]", Toggle(true)),
                    ("previous_levels [6]", Toggle(true)),
                ],
                group("pw", 0..6, 4, &ALL[..3], &jan30[..3]),
            ),
            (
                "month before rollover",
                2,
                false,
                vec![
                    ("current [2]", Toggle(true)),
                    ("current_levels [11]", Toggle(true)),
                ],
                group("m", 0..2, 2, &OHLC, &jan30[3..]),
            ),
            (
                "month moves",
                3,
                false,
                vec![
                    ("current [2]", Toggle(true)),
                    ("current_levels [11]", Toggle(true)),
                    ("previous [2]", Toggle(true)),
                    ("previous_levels [11]", Toggle(true)),
                ],
                [
                    group("m", 2..3, 2, &OHLC, &[54.0; 4]),
                    group("pm", 0..3, 5, &OHLC, &jan30[3..]),
                ]
                .concat(),
            ),
            (
                "independent periods and draw order",
                6,
                false,
                vec![
                    ("current [0]", Toggle(true)),
                    ("current [1]", Toggle(true)),
                    ("current [2]", Toggle(true)),
                    ("current_levels [8]", Toggle(true)),
                ],
                [
                    group("d", 4..6, 0, &ALL[..3], &[30.5, 30.0, 31.0]),
                    group("w", 2..6, 1, &ALL[..3], &[30.5, 30.0, 53.0]),
                    group("m", 2..6, 2, &ALL[..1], &[30.5]),
                ]
                .concat(),
            ),
            (
                "current bar to the right",
                2,
                false,
                vec![("current [0]", Toggle(true)), ("start", Choice(1))],
                group("d", 1..21, 0, &ALL[..3], &jan30[..3]),
            ),
            (
                "custom last two days",
                6,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("custom_levels [0]", Toggle(true)),
                    ("custom_levels [1]", Toggle(true)),
                    ("custom_levels [2]", Toggle(true)),
                    ("length", Length(2)),
                ],
                group("Custom ", 2..6, 6, &ALL[..3], &[30.5, 30.0, 53.0]),
            ),
            (
                "custom week",
                6,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("unit", Choice(1)),
                    ("custom_levels [3]", Toggle(true)),
                ],
                group("Custom ", 2..6, 6, &OHLC, &[54.0, 54.0, 30.0, 54.0]),
            ),
            (
                "custom month",
                3,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("unit", Choice(2)),
                    ("custom_levels [3]", Toggle(true)),
                ],
                group("Custom ", 2..3, 6, &OHLC, &[54.0; 4]),
            ),
            (
                "last earnings",
                6,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("anchor", Choice(1)),
                    ("custom_levels [3]", Toggle(true)),
                ],
                group("Custom ", 4..6, 6, &OHLC, &[30.0, 54.0, 30.0, 54.0]),
            ),
            (
                "last split",
                6,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("anchor", Choice(2)),
                    ("custom_levels [3]", Toggle(true)),
                ],
                group("Custom ", 0..6, 6, &OHLC, &[10.0, 54.0, 10.0, 54.0]),
            ),
            (
                "last dividend",
                6,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("anchor", Choice(3)),
                    ("custom_levels [3]", Toggle(true)),
                ],
                group("Custom ", 2..6, 6, &OHLC, &[54.0, 54.0, 30.0, 54.0]),
            ),
            (
                "future earnings ignored",
                2,
                false,
                vec![
                    ("custom", Toggle(true)),
                    ("anchor", Choice(1)),
                    ("custom_levels [3]", Toggle(true)),
                ],
                vec![],
            ),
            (
                "exchange trading days",
                2,
                true,
                vec![
                    ("current [0]", Toggle(true)),
                    ("current_levels [0]", Toggle(false)),
                    ("current_levels [1]", Toggle(false)),
                    ("current_levels [2]", Toggle(false)),
                    ("current_levels [3]", Toggle(true)),
                    ("previous [0]", Toggle(true)),
                    ("previous_levels [3]", Toggle(true)),
                ],
                [
                    group("d", 1..2, 0, &OHLC, &[34.0; 4]),
                    group("pd", 0..2, 3, &OHLC, &[10.0; 4]),
                ]
                .concat(),
            ),
        ];
        let run = |study: &mut dyn Indicator, seen, in_tokyo: bool| {
            study.set_instrument(in_tokyo.then_some(&tokyo));
            study.set_time_scale_events(&events);
            study.calculate(&data[..seen]);
            format!("{:?}", (study.zones(), study.labels()))
        };
        for (name, seen, in_tokyo, inputs, want) in rows {
            let mut study = AutoKeyLevels::new();
            assert!(study.legend_lines().is_empty());
            assert!(study.labels_use_plot_background());
            for id in ["current [0]", "current [1]"] {
                study.set_input(id, Toggle(false)).unwrap();
            }
            for (id, value) in inputs {
                study.set_input(id, value).unwrap();
            }
            let inputs = study.inputs();
            for input in &inputs {
                if let Some((parent, _)) = &input.shown_when
                    && let Some(parent) = inputs.iter().find(|input| input.id == *parent)
                    && let Toggle(on) = parent.value
                {
                    assert_eq!(input.is_shown(&inputs), on, "{name} {}", input.id);
                }
            }
            for id in ["length", "unit"] {
                let input = inputs.iter().find(|input| input.id == id).unwrap();
                assert_eq!(
                    input.is_shown(&inputs),
                    study.custom && study.anchor == CustomAnchor::Custom,
                    "{name} {id}"
                );
            }
            let drawn = run(&mut study, seen, in_tokyo);
            assert_eq!(study.zones().len(), study.labels().len(), "{name}");
            let got: Vec<_> = study
                .zones()
                .iter()
                .map(|zone| {
                    assert_eq!(zone.top, zone.bottom, "{name}");
                    (zone.bars.clone(), zone.top, zone.plot)
                })
                .collect();
            let expected: Vec<_> = want
                .iter()
                .map(|(_, bars, price, plot)| (bars.clone(), *price, *plot))
                .collect();
            assert_eq!(got, expected, "{name}");
            let tags: Vec<_> = study
                .labels()
                .iter()
                .map(|label| {
                    assert_eq!(label.placement, LabelPlacement::AboveLeft, "{name}");
                    (label.text.clone(), label.bar, label.price, label.plot)
                })
                .collect();
            let mut expected: Vec<_> = want
                .iter()
                .map(|(text, bars, price, plot)| (text.clone(), bars.start, *price, *plot))
                .collect();
            expected.sort_by_key(|tag| tag.1);
            assert_eq!(tags, expected, "{name} tags");
            let json = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
            let mut restored = serde_json::from_str::<StudyTemplate>(&json)
                .unwrap()
                .create()
                .unwrap();
            assert_eq!(
                run(restored.as_mut(), seen, in_tokyo),
                drawn,
                "{name} restored"
            );
        }
        // A literal template saved on origin/main keeps its seven styling plots and inputs.
        let saved = r#"{"name":"Auto Key Levels","inputs":[["current [0]",{"Toggle":true}],["current [1]",{"Toggle":true}],["current [2]",{"Toggle":false}],["current_levels [0]",{"Toggle":true}],["current_levels [1]",{"Toggle":true}],["current_levels [2]",{"Toggle":true}],["current_levels [3]",{"Toggle":false}],["current_levels [4]",{"Toggle":true}],["current_levels [5]",{"Toggle":true}],["current_levels [6]",{"Toggle":true}],["current_levels [7]",{"Toggle":false}],["current_levels [8]",{"Toggle":false}],["current_levels [9]",{"Toggle":false}],["current_levels [10]",{"Toggle":false}],["current_levels [11]",{"Toggle":false}],["previous [0]",{"Toggle":false}],["previous [1]",{"Toggle":false}],["previous [2]",{"Toggle":false}],["previous_levels [0]",{"Toggle":false}],["previous_levels [1]",{"Toggle":false}],["previous_levels [2]",{"Toggle":false}],["previous_levels [3]",{"Toggle":false}],["previous_levels [4]",{"Toggle":false}],["previous_levels [5]",{"Toggle":false}],["previous_levels [6]",{"Toggle":false}],["previous_levels [7]",{"Toggle":false}],["previous_levels [8]",{"Toggle":false}],["previous_levels [9]",{"Toggle":false}],["previous_levels [10]",{"Toggle":false}],["previous_levels [11]",{"Toggle":false}],["custom",{"Toggle":false}],["custom_levels [0]",{"Toggle":false}],["custom_levels [1]",{"Toggle":false}],["custom_levels [2]",{"Toggle":false}],["custom_levels [3]",{"Toggle":false}],["anchor",{"Choice":0}],["length",{"Length":1}],["unit",{"Choice":0}],["start",{"Choice":0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AutoKeyLevels.html","plots":[{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[126,87,194,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[153,65,0,153],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[25,59,153,153],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,52,116,153],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[253,216,53,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[16,7,0,16],"untagged_plots":[0,1,2,3,4,5,6],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let template: StudyTemplate = serde_json::from_str(saved).unwrap();
        let mut restored = template.create().unwrap();
        assert_eq!(restored.settings(), template.settings);
        assert_eq!(restored.line_count(), template.settings.plots.len());
        assert_eq!(Some(restored.scale()), template.scale);
        assert_eq!(restored.is_visible(), template.visible);
        for (id, value) in &template.inputs {
            assert_eq!(
                restored
                    .inputs()
                    .iter()
                    .find(|input| input.id == *id)
                    .unwrap()
                    .value,
                *value,
                "{id}"
            );
        }
        assert_eq!(
            run(restored.as_mut(), 6, false),
            run(&mut AutoKeyLevels::new(), 6, false)
        );
        let mut study = AutoKeyLevels::new();
        for start in [0, 1] {
            study.set_input("start", Choice(start)).unwrap();
            study.calculate(&data);
            for end in [40, 80, 30] {
                let range = crate::model::LogicalRange::new(0.0, (end - 1) as f32);
                study.set_visible_logical_range(range);
                for zone in study.zones() {
                    let first = if start == 1 {
                        5
                    } else if zone.plot == 0 {
                        4
                    } else {
                        2
                    };
                    assert_eq!(zone.bars, first..end);
                }
            }
        }
    }

    /// Host intrabars replace the chart's uniform volume estimate. Three units at 34 and
    /// one at 10 put the POC in the last equal row [33, 34]; with whole one-unit tick rows
    /// the endpoint belongs to [34, 35]. Correcting the heavy intrabar moves it back to 10.
    #[test]
    fn host_profiles_use_finer_candles_and_whole_tick_rows() {
        let time = Utc.with_ymd_and_hms(2026, 2, 2, 0, 0, 0).unwrap();
        let data = [Bar::new(time, 10.0, 34.0, 10.0, 34.0, 4.0)];
        let endpoints = [(10.0, 1.0), (34.0, 3.0), (34.0, 0.0)];
        // At row 12 the empty neighbours tie: alternate by distance, upper first. Bounds reach
        // 3..=22: row 23 is then selected and exceeds the budget, though row 2 would fit.
        // At row 1 a lower row of volume 1 reaches the budget exactly and is included.
        for (tick, points, want) in [
            (None, endpoints, [33.5, 33.0, 34.0]),
            (Some(1.0), endpoints, [34.5, 34.0, 35.0]),
            (
                None,
                [(10.0, 2.0), (22.0, 6.0), (34.0, 2.0)],
                [22.5, 13.0, 33.0],
            ),
            (
                None,
                [(10.0, 1.0), (11.0, 6.0), (34.0, 3.0)],
                [11.5, 10.0, 12.0],
            ),
        ] {
            let mut study = AutoKeyLevels::new();
            study
                .set_input("current [1]", InputValue::Toggle(false))
                .unwrap();
            study.set_timeframe(Timeframe::Min5);
            let instrument = tick.map(|tick| Instrument::new("TICKS").with_tick_size(tick));
            study.set_instrument(instrument.as_ref());
            let request = study.data_requests().pop().unwrap();
            assert_eq!(request.timeframe, Some(Timeframe::Min1));
            let mut intrabars: Vec<_> = points
                .into_iter()
                .enumerate()
                .map(|(i, (price, volume))| {
                    Bar::new(
                        time + chrono::TimeDelta::minutes(i as i64),
                        price,
                        price,
                        price,
                        price,
                        volume,
                    )
                })
                .collect();
            for (revision, want) in [want, [10.5, 10.0, 11.0]].into_iter().enumerate() {
                if revision == 1 {
                    intrabars[1] = Bar::new(intrabars[1].time, 10.0, 10.0, 10.0, 10.0, 3.0);
                    // Retain the range with a zero-volume candle at 34.
                    intrabars[0] = Bar::new(time, 34.0, 34.0, 34.0, 34.0, 0.0);
                    intrabars[2].volume = 0.0;
                }
                study.update_data(
                    &data,
                    0,
                    &[StudyData {
                        request: request.clone(),
                        bars: &intrabars,
                        clock: BarClock::new(Timeframe::Min1),
                        instrument: None,
                        timeframe: Timeframe::Min1,
                    }],
                );
                assert_eq!(
                    study
                        .zones()
                        .iter()
                        .map(|zone| zone.top)
                        .collect::<Vec<_>>(),
                    want,
                    "tick {tick:?} revision {revision}"
                );
            }
        }
    }

    /// Enabled periods missing history or an anchor name what is unavailable, keep usable
    /// levels drawn, and clear their notice after data arrives or the period is turned off.
    #[test]
    fn unavailable_periods_report_and_clear_their_notices() {
        use InputValue::{Choice, Toggle};
        let data = bars();
        type Row = (
            Timeframe,
            usize,
            Vec<(&'static str, InputValue)>,
            Vec<TimeScaleEvent>,
            Vec<&'static str>,
        );
        let mut rows: Vec<Row> = vec![
            (
                Timeframe::Min1,
                4,
                vec![("previous [0]", Toggle(true))],
                vec![],
                vec!["Prev Day"],
            ),
            (
                Timeframe::Day1,
                2,
                vec![("previous [2]", Toggle(true))],
                vec![],
                vec!["Prev Month"],
            ),
            (
                Timeframe::Min1,
                4,
                vec![
                    ("previous [0]", Toggle(true)),
                    ("previous [1]", Toggle(true)),
                    ("previous [2]", Toggle(true)),
                ],
                vec![],
                vec!["Prev Day", "Prev Week", "Prev Month"],
            ),
            (Timeframe::Week1, 2, vec![], vec![], vec!["Day"]),
        ];
        for (anchor, kind, name) in [
            (1, EventKind::Earnings, "Custom (Earnings)"),
            (2, EventKind::Split, "Custom (Split)"),
            (3, EventKind::Dividend, "Custom (Dividends)"),
        ] {
            rows.push((
                Timeframe::Min1,
                2,
                vec![("custom", Toggle(true)), ("anchor", Choice(anchor))],
                vec![TimeScaleEvent::new("anchor", data[4].time, kind, "")],
                vec![name],
            ));
        }
        for (interval, first, edits, events, missing) in rows {
            let mut study = AutoKeyLevels::new();
            study.set_timeframe(interval);
            for (id, value) in &edits {
                study.set_input(id, value.clone()).unwrap();
            }
            // A future event cannot anchor the loaded history.
            let future: Vec<_> = events
                .iter()
                .cloned()
                .map(|mut event| {
                    event.time = data[5].time + chrono::TimeDelta::days(1);
                    event
                })
                .collect();
            study.set_time_scale_events(&future);
            study.calculate(&data[first..]);
            let notice = study.table().unwrap();
            assert_eq!(notice.title, "Insufficient period data");
            let rows: Vec<_> = notice
                .rows
                .iter()
                .map(|row| match &row[0] {
                    StudyCell::Text(text) => text.as_str(),
                    _ => panic!("expected a period notice"),
                })
                .collect();
            let expected: Vec<_> = missing
                .iter()
                .map(|period| {
                    format!("{period}: load more data, use shorter bars or turn off this period")
                })
                .collect();
            assert_eq!(rows, expected, "{interval:?} {edits:?}");
            #[cfg(feature = "widget")]
            {
                let mut chart =
                    crate::Chart::new(crate::model::BarData::from_bars(data[first..].to_vec()));
                chart.set_timeframe(interval);
                chart.add_indicator(study.clone_box());
                let shapes = crate::widget::test_frames::shapes(
                    &egui::Context::default(),
                    &mut chart,
                    0.0,
                    vec![],
                );
                for period in &missing {
                    assert!(shapes.iter().any(|shape| matches!(shape,
                        egui::Shape::Text(text) if text.galley.text().starts_with(&format!("{period}:"))
                            && text.galley.text().contains("shorter bars")
                    )), "painted notice for {period}");
                }
            }
            assert!(!study.zones().is_empty(), "usable periods keep drawing");
            if interval == Timeframe::Week1 {
                assert!(study.zones().iter().all(|zone| zone.plot != 0));
                study.set_timeframe(Timeframe::Min1);
            }
            study.set_time_scale_events(&events);
            study.calculate(&data);
            assert!(
                study.table().is_none(),
                "history or events clear {missing:?}"
            );
            study.set_timeframe(interval);
            study.set_time_scale_events(&[]);
            study.calculate(&data[first..]);
            for id in [
                "current [0]",
                "previous [0]",
                "previous [1]",
                "previous [2]",
                "custom",
            ] {
                study.set_input(id, Toggle(false)).unwrap();
            }
            study.calculate(&data[first..]);
            assert!(
                study.table().is_none(),
                "turning periods off clears {missing:?}"
            );
        }
    }
}
