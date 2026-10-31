//! Pivot Points Standard.
//!
//! Support and resistance levels for each calendar period (day, week, month, quarter, year or
//! several years of exchange trading days) from the open, high, low and close of the previous
//! period in the data. With H, L, C and O the previous period's high, low, close and open,
//! R = H - L and P = (H + L + C) / 3:
//!
//! | Type        | Levels |
//! |-------------|--------|
//! | Traditional | R1 = 2P - L, S1 = 2P - H, R2/S2 = P ± R, R3 = 2P + (H - 2L), S3 = 2P - (2H - L), R4 = 3P + (H - 3L), S4 = 3P - (3H - L), R5 = 4P + (H - 4L), S5 = 4P - (4H - L) |
//! | Fibonacci   | R1/S1 = P ± 0.382R, R2/S2 = P ± 0.618R, R3/S3 = P ± R |
//! | Woodie      | P = (H + L + 2 * current period's open) / 4, R1 = 2P - L, S1 = 2P - H, R2/S2 = P ± R, R3 = H + 2(P - L), S3 = L - 2(H - P), R4 = R3 + R, S4 = S3 - R |
//! | Classic     | R1 = 2P - L, S1 = 2P - H, R2/S2 = P ± R, R3/S3 = P ± 2R, R4/S4 = P ± 3R |
//! | DM          | X = H + L + 2C if C = O, 2H + L + C if C > O, else H + 2L + C; P = X / 4, R1 = X / 2 - L, S1 = X / 2 - H |
//! | Camarilla   | R1..R4 / S1..S4 = C ± 1.1R / 12, 6, 4, 2; R5 = (H / L) * C, S5 = C - (R5 - C) |
//!
//! A level a type does not define is NaN. The levels after DeMark (DM) and Camarilla follow
//! Tom DeMark and Nick Stott; the others are the conventional floor-trader pivots.
//!
//! # Output
//!
//! Eleven lines, `[P, R1, S1, R2, S2, R3, S3, R4, S4, R5, S5]`, constant within a period and
//! drawn only for the last [`with_pivots_back`](PivotPoints::with_pivots_back) periods. A line
//! is not drawn across the change from one period to the next. Each level is labelled with its
//! name, and optionally its price, at its left or right end.
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..600).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::DateTime::from_timestamp(i * 3_600, 0).unwrap(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::model::Timeframe;
//! use egui_charts::studies::{Indicator, PivotMethod, PivotPoints, PivotTimeframe};
//!
//! // Weekly Fibonacci pivots of hourly bars, the last five weeks.
//! let mut pivots = PivotPoints::new()
//!     .with_method(PivotMethod::Fibonacci)
//!     .with_timeframe(PivotTimeframe::Weekly)
//!     .with_pivots_back(5);
//! pivots.set_timeframe(Timeframe::Hour1);
//! pivots.calculate(&bars);
//! assert_eq!(pivots.line_count(), 11);
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use crate::model::{Bar, Instrument, Timeframe};
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, LabelPlacement, StudyDataRequest, StudyLabel,
};
use chrono::{Datelike, NaiveDate};
use egui::Color32;

/// The level formulas used by [`PivotPoints`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PivotMethod {
    /// Floor-trader pivots with five levels each side.
    #[default]
    Traditional,
    /// Fibonacci ratios 0.382, 0.618 and 1 of the previous range around P.
    Fibonacci,
    /// P weighted by the current period's open, four levels each side.
    Woodie,
    /// Multiples of the previous range around P, four levels each side.
    Classic,
    /// Tom DeMark's pivot, conditional on the previous close against its open: P, R1 and S1.
    DM,
    /// Nick Stott's levels around the previous close, five each side.
    Camarilla,
}

/// The calendar period of each set of pivots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PivotTimeframe {
    /// By chart interval: daily up to 15 minutes, weekly below a day, monthly below a week,
    /// yearly from a week.
    #[default]
    Auto,
    /// Each trading day.
    Daily,
    /// Each ISO week.
    Weekly,
    /// Each calendar month.
    Monthly,
    /// Each calendar quarter.
    Quarterly,
    /// Each calendar year.
    Yearly,
    /// Each two calendar years, starting in an even year.
    TwoYears,
    /// Each three calendar years, starting in a year divisible by 3.
    ThreeYears,
    /// Each five calendar years, starting in a year divisible by 5.
    FiveYears,
    /// Each decade, 2020 to 2029 for example.
    TenYears,
}

/// Which end of each pivot level its label sits at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PivotLabelSide {
    /// At the start of the level, above its line.
    #[default]
    Left,
    /// At the end of the level, above its line.
    Right,
}

const LEVEL_NAMES: [&str; 11] = [
    "P", "R1", "S1", "R2", "S2", "R3", "S3", "R4", "S4", "R5", "S5",
];

/// What the level labels of the pivot studies show.
#[derive(Clone, Copy)]
pub(super) struct LevelLabels {
    pub names: bool,
    pub prices: bool,
    pub side: PivotLabelSide,
    pub precision: Option<usize>,
}

impl LevelLabels {
    fn text(self, line: usize, price: f64) -> String {
        let name = LEVEL_NAMES[line];
        let price = || crate::scales::StudyFormat::Price.format(price, self.precision);
        match (self.names, self.prices) {
            (true, true) => format!("{name} ({})", price()),
            (true, false) => name.to_owned(),
            _ => price(),
        }
    }
}

/// Rewrites `labels` from the bar before `from` on, given that `values[..from]` is unchanged. A
/// label depends only on its own bar and the bars either side, so the earlier ones still hold.
pub(super) fn refresh_level_labels(
    labels: &mut Vec<StudyLabel>,
    values: &IndicatorValues,
    from: usize,
    options: LevelLabels,
) {
    if !options.names && !options.prices {
        labels.clear();
        return;
    }
    let len = values.len();
    let first = from.min(len).saturating_sub(1);
    labels.truncate(labels.partition_point(|label| label.bar < first));
    for bar in first..len {
        for (line, _) in LEVEL_NAMES.iter().enumerate() {
            let Some(price) = values.value(line, bar) else {
                continue;
            };
            let (neighbour, placement) = match options.side {
                PivotLabelSide::Left => (bar.checked_sub(1), LabelPlacement::AboveRight),
                PivotLabelSide::Right => (
                    Some(bar + 1).filter(|&next| next < len),
                    LabelPlacement::AboveLeft,
                ),
            };
            if neighbour.and_then(|at| values.value(line, at)) != Some(price) {
                labels.push(StudyLabel {
                    bar,
                    price,
                    text: options.text(line, price),
                    placement,
                    plot: line,
                });
            }
        }
    }
}

/// Pivot Points Standard: support and resistance levels for each calendar period from the
/// previous period.
///
/// Periods are counted in exchange trading days, as
/// [`VolumeWeightedAvgPrice`](super::VolumeWeightedAvgPrice) counts them. The chart supplies
/// its interval for [`PivotTimeframe::Auto`] and the instrument's time zone and session;
/// direct callers set them with [`Indicator::set_timeframe`] (one minute until then) and
/// [`Indicator::set_instrument`] (UTC until then). The levels come from the chart's own bars, or
/// with [`with_daily_bars`](Self::with_daily_bars) from the feed's daily bars. Overlay indicator.
#[derive(Clone)]
pub struct PivotPoints {
    method: PivotMethod,
    timeframe: PivotTimeframe,
    pivots_back: usize,
    interval: Timeframe,
    days: ExchangeDays,
    values: IndicatorValues,
    stepper: Stepper<State>,
    intraday: bool,
    shown_start: usize,
    /// One per line: P, then R1, S1 through R5, S5.
    colors: Vec<Color32>,
    visible: bool,
    show_labels: bool,
    show_prices: bool,
    label_side: PivotLabelSide,
    daily_bars: bool,
    /// The instrument's decimals and those of the study's presentation settings.
    precision: Option<usize>,
    label_precision: Option<usize>,
    /// The feed's daily bars by period, once the host has supplied them.
    daily: Option<Vec<((i32, u32), Bar)>>,
    labels: Vec<StudyLabel>,
}

#[derive(Clone)]
struct Period {
    key: (i32, u32),
    start: usize,
    ohlc: Bar,
    levels: [f64; 11],
}

#[derive(Clone, Default)]
struct State {
    index: usize,
    previous_date: Option<chrono::NaiveDate>,
    intraday: bool,
    periods: std::collections::VecDeque<Period>,
}

impl PivotPoints {
    /// Traditional pivots, timeframe Auto, for the last 15 periods.
    pub fn new() -> Self {
        // The pivot and every resistance and support level share one colour.
        let colors = vec![crate::studies::palette::DARK_ORANGE; 11];
        Self {
            method: PivotMethod::Traditional,
            timeframe: PivotTimeframe::Auto,
            pivots_back: 15,
            interval: Timeframe::Min1,
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            intraday: false,
            shown_start: 0,
            colors,
            visible: true,
            show_labels: true,
            show_prices: false,
            label_side: PivotLabelSide::Left,
            daily_bars: false,
            precision: None,
            label_precision: None,
            daily: None,
            labels: Vec::new(),
        }
    }

    /// The level formulas.
    pub fn with_method(mut self, method: PivotMethod) -> Self {
        self.method = method;
        self
    }

    /// The calendar period of each set of pivots.
    pub fn with_timeframe(mut self, timeframe: PivotTimeframe) -> Self {
        self.timeframe = timeframe;
        self
    }

    /// The number of most recent periods that show pivots, raised to 1 or more.
    pub fn with_pivots_back(mut self, pivots_back: usize) -> Self {
        self.pivots_back = pivots_back.max(1);
        self
    }

    /// Sets the colors line by line, as [`Indicator::set_colors`] does.
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.set_colors(colors);
        self
    }

    /// Whether each level is labelled with its name (P, R1, S1 and so on) and with its price.
    /// Names only by default.
    pub fn with_labels(mut self, names: bool, prices: bool) -> Self {
        self.show_labels = names;
        self.show_prices = prices;
        self
    }

    /// The end of each level its label sits at; the left end by default.
    pub fn with_label_side(mut self, side: PivotLabelSide) -> Self {
        self.label_side = side;
        self
    }

    /// On intervals shorter than a day, takes each previous period's high, low and close (and,
    /// for the methods that use it, the period's open) from the feed's daily bars, requested
    /// through [`Indicator::data_requests`] and passed to [`Indicator::update_data`]. Without
    /// daily bars from the host there are no levels. `calculate` and `update` use the chart's
    /// own bars.
    pub fn with_daily_bars(mut self, daily: bool) -> Self {
        self.daily_bars = daily;
        self
    }

    /// `None` for the periods of several years that [`AnchorPeriod`] does not hold.
    fn period(&self) -> Option<AnchorPeriod> {
        Some(match self.timeframe {
            PivotTimeframe::Auto => {
                let ms = self.interval.duration_ms();
                if ms <= Timeframe::Min15.duration_ms() {
                    AnchorPeriod::Session
                } else if ms < Timeframe::Day1.duration_ms() {
                    AnchorPeriod::Week
                } else if ms < Timeframe::Week1.duration_ms() {
                    AnchorPeriod::Month
                } else {
                    AnchorPeriod::Year
                }
            }
            PivotTimeframe::Daily => AnchorPeriod::Session,
            PivotTimeframe::Weekly => AnchorPeriod::Week,
            PivotTimeframe::Monthly => AnchorPeriod::Month,
            PivotTimeframe::Quarterly => AnchorPeriod::Quarter,
            PivotTimeframe::Yearly => AnchorPeriod::Year,
            PivotTimeframe::TenYears => AnchorPeriod::Decade,
            PivotTimeframe::TwoYears | PivotTimeframe::ThreeYears | PivotTimeframe::FiveYears => {
                return None;
            }
        })
    }

    /// Identifies the period that holds trading day `day`. Periods of several years start in a
    /// year divisible by their length, so they do not move when older history loads.
    fn key(period: Option<AnchorPeriod>, timeframe: PivotTimeframe, day: NaiveDate) -> (i32, u32) {
        match (period, timeframe) {
            (Some(period), _) => period.key(day),
            (None, PivotTimeframe::TwoYears) => (day.year().div_euclid(2), 0),
            (None, PivotTimeframe::ThreeYears) => (day.year().div_euclid(3), 0),
            (None, _) => (day.year().div_euclid(5), 0),
        }
    }

    fn usable(method: PivotMethod, bar: &Bar) -> bool {
        bar.high.is_finite()
            && bar.low.is_finite()
            && bar.close.is_finite()
            && (!matches!(method, PivotMethod::Woodie | PivotMethod::DM) || bar.open.is_finite())
    }

    fn label_options(&self) -> LevelLabels {
        LevelLabels {
            names: self.show_labels,
            prices: self.show_prices,
            side: self.label_side,
            precision: self.label_precision.or(self.precision),
        }
    }

    /// Labels the projected `levels`, one per finite level, at the chosen end of `bars`.
    pub(super) fn label_projection(&mut self, bars: std::ops::Range<usize>, levels: &[f64; 11]) {
        let options = self.label_options();
        if !options.names && !options.prices {
            return;
        }
        let (bar, placement) = match options.side {
            PivotLabelSide::Left => (bars.start, LabelPlacement::AboveRight),
            PivotLabelSide::Right => (bars.end, LabelPlacement::AboveLeft),
        };
        self.labels.extend(
            levels
                .iter()
                .enumerate()
                .filter(|(_, price)| price.is_finite())
                .map(|(plot, &price)| StudyLabel {
                    bar,
                    price,
                    text: options.text(plot, price),
                    placement,
                    plot,
                }),
        );
    }

    /// The daily bars grouped by period: the first bar of each with the running high, low and
    /// close. Daily bars keep their UTC date, as a chart of daily bars does.
    fn daily_periods(&self, bars: &[Bar]) -> Vec<((i32, u32), Bar)> {
        let (period, timeframe) = (self.period(), self.timeframe);
        let mut periods: Vec<((i32, u32), Bar)> = Vec::new();
        for bar in bars.iter().filter(|bar| Self::usable(self.method, bar)) {
            let key = Self::key(period, timeframe, bar.time.date_naive());
            match periods.last_mut() {
                Some((last, current)) if *last == key => {
                    current.high = current.high.max(bar.high);
                    current.low = current.low.min(bar.low);
                    current.close = bar.close;
                }
                _ => periods.push((key, bar.clone())),
            }
        }
        periods
    }

    /// Whether `new` differs from the daily periods that computed the levels so far: the open of
    /// each period up to the latest and the rest of the earlier ones. A live tick on today's
    /// daily bar changes neither, so it does not force a full recalculation.
    fn daily_levels_changed(&self, new: &[((i32, u32), Bar)]) -> bool {
        let Some(old) = &self.daily else {
            return true;
        };
        let Some(latest) = self.stepper.state().periods.back().map(|period| period.key) else {
            return false;
        };
        let used = |periods: &[((i32, u32), Bar)]| periods.partition_point(|(k, _)| *k <= latest);
        let same = |a: f64, b: f64| a.to_bits() == b.to_bits();
        used(old) != used(new)
            || old[..used(old)]
                .iter()
                .zip(new)
                .any(|((key, a), (new_key, b))| {
                    key != new_key
                        || !same(a.open, b.open)
                        || (*key < latest
                            && !(same(a.high, b.high)
                                && same(a.low, b.low)
                                && same(a.close, b.close)))
                })
    }

    fn run(&mut self, data: &[Bar], mut from: usize) {
        let (period, timeframe) = (self.period(), self.timeframe);
        let method = self.method;
        let entry_shown = self.shown_start;
        let len = self.values.len();
        if len == 0 || from < len - 1 || data.len() < len {
            self.shown_start = 0;
        }
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let index = state.index;
                    state.index += 1;
                    if !Self::usable(method, bar) {
                        return IndicatorValue::None;
                    }
                    let date = bar.time.date_naive();
                    state.intraday |= state.previous_date == Some(date);
                    state.previous_date = Some(date);
                    let key = Self::key(period, timeframe, self.days.day(bar, !self.intraday));
                    if state.periods.back().is_none_or(|last| last.key != key) {
                        let levels = match &self.daily {
                            Some(daily) => {
                                let at = daily.partition_point(|(k, _)| *k < key);
                                let open = daily
                                    .get(at)
                                    .filter(|(k, _)| *k == key)
                                    .map_or(bar.open, |(_, period)| period.open);
                                at.checked_sub(1).map_or([f64::NAN; 11], |previous| {
                                    Self::levels(method, &daily[previous].1, open)
                                })
                            }
                            None => state.periods.back().map_or([f64::NAN; 11], |last| {
                                Self::levels(method, &last.ohlc, bar.open)
                            }),
                        };
                        state.periods.push_back(Period {
                            key,
                            start: index,
                            ohlc: bar.clone(),
                            levels,
                        });
                        if state.periods.len() > self.pivots_back {
                            state.periods.pop_front();
                        }
                    } else {
                        let current = state.periods.back_mut().unwrap();
                        current.ohlc.high = current.ohlc.high.max(bar.high);
                        current.ohlc.low = current.ohlc.low.min(bar.low);
                        current.ohlc.close = bar.close;
                    }
                    IndicatorValue::Multiple(state.periods.back().unwrap().levels.to_vec())
                });
            if self.intraday == self.stepper.state().intraday {
                break;
            }
            self.intraday = self.stepper.state().intraday;
            self.shown_start = 0;
            from = 0;
        }
        let periods = &self.stepper.state().periods;
        let start = periods.front().map_or(data.len(), |period| period.start);
        if start > self.shown_start {
            for bar in self.shown_start..start {
                self.values.set(bar, IndicatorValue::None);
            }
        } else if start < self.shown_start {
            // Withdrawing a new period reveals the one that its display limit had expired.
            for (at, period) in periods.iter().enumerate() {
                let end = periods
                    .get(at + 1)
                    .map_or(data.len(), |next| next.start)
                    .min(self.shown_start);
                for (bar, source) in data.iter().enumerate().take(end).skip(period.start) {
                    if Self::usable(method, source) {
                        self.values
                            .set(bar, IndicatorValue::Multiple(period.levels.to_vec()));
                    }
                }
            }
        }
        // The expiry or the return of a period changes values before `from`.
        let unchanged = if start != entry_shown { 0 } else { from };
        self.shown_start = start;
        let options = self.label_options();
        refresh_level_labels(&mut self.labels, &self.values, unchanged, options);
    }

    /// The method and the latest period so far: its open, running high and low, latest close,
    /// and first bar. `None` before a usable bar.
    pub(super) fn latest_period(&self) -> Option<(PivotMethod, &Bar, usize)> {
        let period = self.stepper.state().periods.back()?;
        Some((self.method, &period.ohlc, period.start))
    }

    /// `[P, R1, S1, ..., R5, S5]` from the previous period and the current period's open.
    pub(super) fn levels(method: PivotMethod, prev: &Bar, open: f64) -> [f64; 11] {
        let (h, l, c) = (prev.high, prev.low, prev.close);
        let r = h - l;
        let p = (h + l + c) / 3.0;
        let nan = f64::NAN;
        match method {
            PivotMethod::Traditional => [
                p,
                2.0 * p - l,
                2.0 * p - h,
                p + r,
                p - r,
                2.0 * p + (h - 2.0 * l),
                2.0 * p - (2.0 * h - l),
                3.0 * p + (h - 3.0 * l),
                3.0 * p - (3.0 * h - l),
                4.0 * p + (h - 4.0 * l),
                4.0 * p - (4.0 * h - l),
            ],
            PivotMethod::Fibonacci => [
                p,
                p + 0.382 * r,
                p - 0.382 * r,
                p + 0.618 * r,
                p - 0.618 * r,
                p + r,
                p - r,
                nan,
                nan,
                nan,
                nan,
            ],
            PivotMethod::Woodie => {
                let p = (h + l + 2.0 * open) / 4.0;
                let (r3, s3) = (h + 2.0 * (p - l), l - 2.0 * (h - p));
                [
                    p,
                    2.0 * p - l,
                    2.0 * p - h,
                    p + r,
                    p - r,
                    r3,
                    s3,
                    r3 + r,
                    s3 - r,
                    nan,
                    nan,
                ]
            }
            PivotMethod::Classic => [
                p,
                2.0 * p - l,
                2.0 * p - h,
                p + r,
                p - r,
                p + 2.0 * r,
                p - 2.0 * r,
                p + 3.0 * r,
                p - 3.0 * r,
                nan,
                nan,
            ],
            PivotMethod::DM => {
                let x = if c == prev.open {
                    h + l + 2.0 * c
                } else if c > prev.open {
                    2.0 * h + l + c
                } else {
                    h + 2.0 * l + c
                };
                [
                    x / 4.0,
                    x / 2.0 - l,
                    x / 2.0 - h,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                    nan,
                ]
            }
            PivotMethod::Camarilla => {
                let k = 1.1 * r;
                let r5 = h / l * c;
                [
                    p,
                    c + k / 12.0,
                    c - k / 12.0,
                    c + k / 6.0,
                    c - k / 6.0,
                    c + k / 4.0,
                    c - k / 4.0,
                    c + k / 2.0,
                    c - k / 2.0,
                    r5,
                    c - (r5 - c),
                ]
            }
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for PivotPoints {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for PivotPoints {
    crate::studies::inputs::study_inputs!(self;
        method: choice("Type", [PivotMethod::Traditional, PivotMethod::Fibonacci, PivotMethod::Woodie, PivotMethod::Classic, PivotMethod::DM, PivotMethod::Camarilla]),
        timeframe: choice("Pivots Timeframe", [PivotTimeframe::Auto, PivotTimeframe::Daily, PivotTimeframe::Weekly, PivotTimeframe::Monthly, PivotTimeframe::Quarterly, PivotTimeframe::Yearly, PivotTimeframe::TwoYears, PivotTimeframe::ThreeYears, PivotTimeframe::FiveYears, PivotTimeframe::TenYears]),
        pivots_back: length("Number of Pivots Back", 1, usize::MAX),
        show_labels: toggle("Show Labels"),
        show_prices: toggle("Show Prices"),
        label_side: choice("Labels Position", [PivotLabelSide::Left, PivotLabelSide::Right]),
        daily_bars: toggle("Use Daily Bars")
    ; {});

    fn name(&self) -> &str {
        "Pivot Points Standard"
    }

    fn desc(&self) -> &str {
        "Pivot Points Standard - Support and resistance levels from the previous calendar period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = if self.daily.take().is_some() { 0 } else { from };
        self.run(data, from);
    }

    fn data_requests(&self) -> Vec<StudyDataRequest> {
        if self.daily_bars && self.interval.duration_ms() < Timeframe::Day1.duration_ms() {
            vec![StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(Timeframe::Day1),
                metric: None,
            }]
        } else {
            Vec::new()
        }
    }

    fn update_data(
        &mut self,
        data: &[Bar],
        from: usize,
        sources: &[crate::studies::StudyData<'_>],
    ) {
        let requests = self.data_requests();
        let Some(request) = requests.first() else {
            return self.update(data, from);
        };
        let bars = sources
            .iter()
            .find(|source| source.request == *request)
            .map_or(&[][..], |source| source.bars);
        let daily = self.daily_periods(bars);
        let from = if self.daily_levels_changed(&daily) {
            0
        } else {
            from
        };
        self.daily = Some(daily);
        self.run(data, from);
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.interval = timeframe;
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let precision = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map(usize::from);
        let changed = precision != self.precision;
        self.precision = precision;
        self.days.set(instrument) | changed
    }

    fn labels(&self) -> &[StudyLabel] {
        &self.labels
    }

    fn set_label_precision(&mut self, precision: Option<usize>) {
        self.label_precision = precision;
    }

    /// Transparent on the first bar of a period, so the change of level is not drawn.
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        let previous = self.values.value(line, bar.checked_sub(1)?)?;
        (Some(previous) != self.values.value(line, bar)).then_some(Color32::TRANSPARENT)
    }

    fn values(&self) -> &IndicatorValues {
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
        true // Drawn on price chart
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
        LEVEL_NAMES.map(String::from).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Auto takes the session up to 15 minutes, the week below a day, the month below a week
    /// and the year from a week up.
    #[test]
    fn auto_follows_the_chart_interval() {
        let mut pivots = PivotPoints::new();
        for (interval, period) in [
            (Timeframe::Min15, AnchorPeriod::Session),
            (Timeframe::Min30, AnchorPeriod::Week),
            (Timeframe::Hour4, AnchorPeriod::Week),
            (Timeframe::Day1, AnchorPeriod::Month),
            (Timeframe::Week1, AnchorPeriod::Year),
        ] {
            pivots.set_timeframe(interval);
            assert_eq!(pivots.period(), Some(period), "{interval:?}");
        }
    }

    /// Hourly bars of three days, as (high, low, close). Day 2 takes H 12, L 8, C 11 from day 1:
    /// P 31/3, R1 38/3, S1 26/3, R2 43/3, S2 19/3, R3 50/3, S3 14/3, R4 19, S4 3, R5 64/3, S5 4/3.
    /// Day 3 takes H 14, L 9, C 12 from day 2: P 35/3.
    fn three_days() -> Vec<Bar> {
        let hour = |day, h| Utc.with_ymd_and_hms(2026, 3, day, h, 0, 0).unwrap();
        [
            (hour(2, 9), 10.0, 8.0, 9.0),
            (hour(2, 10), 12.0, 9.0, 11.0),
            (hour(3, 9), 11.0, 9.0, 10.0),
            (hour(3, 10), 14.0, 10.0, 12.0),
            (hour(4, 9), 12.0, 10.0, 11.0),
        ]
        .into_iter()
        .map(|(time, high, low, close)| Bar::new(time, close, high, low, close, 1.0))
        .collect()
    }

    #[test]
    fn every_defined_level_is_labelled_at_the_chosen_end_of_its_period() {
        let daily = || PivotPoints::new().with_timeframe(PivotTimeframe::Daily);
        let names = [
            "P", "R1", "S1", "R2", "S2", "R3", "S3", "R4", "S4", "R5", "S5",
        ];
        let priced = [
            "P (10.33)",
            "R1 (12.67)",
            "S1 (8.67)",
            "R2 (14.33)",
            "S2 (6.33)",
            "R3 (16.67)",
            "S3 (4.67)",
            "R4 (19.00)",
            "S4 (3.00)",
            "R5 (21.33)",
            "S5 (1.33)",
        ];
        let mut tick = daily().with_labels(true, true);
        tick.set_instrument(Some(
            &crate::model::Instrument::new("X").with_tick_size(0.001),
        ));
        // (name, study, bars of the labels, placement, texts of the first period's labels)
        type Case<'a> = (
            &'a str,
            Box<dyn Indicator>,
            &'a [usize],
            LabelPlacement,
            Vec<&'a str>,
        );
        let mut styled =
            crate::studies::ConfiguredStudy::new(Box::new(daily().with_labels(false, true)));
        styled.set_instrument(Some(
            &crate::model::Instrument::new("X").with_tick_size(0.001),
        ));
        let mut settings = styled.settings();
        settings.precision = Some(4);
        styled.apply_settings(&[], settings).unwrap();
        let cases: Vec<Case> = vec![
            (
                "defaults",
                Box::new(daily()),
                &[2, 4],
                LabelPlacement::AboveRight,
                names.to_vec(),
            ),
            (
                "prices at the right end",
                Box::new(
                    daily()
                        .with_labels(true, true)
                        .with_label_side(PivotLabelSide::Right),
                ),
                &[3, 4],
                LabelPlacement::AboveLeft,
                priced.to_vec(),
            ),
            (
                "prices only",
                Box::new(daily().with_labels(false, true)),
                &[2, 4],
                LabelPlacement::AboveRight,
                vec![
                    "10.33", "12.67", "8.67", "14.33", "6.33", "16.67", "4.67", "19.00", "3.00",
                    "21.33", "1.33",
                ],
            ),
            (
                "seven Fibonacci levels",
                Box::new(daily().with_method(PivotMethod::Fibonacci)),
                &[2, 4],
                LabelPlacement::AboveRight,
                names[..7].to_vec(),
            ),
            (
                "expired period",
                Box::new(daily().with_pivots_back(1)),
                &[4],
                LabelPlacement::AboveRight,
                names.to_vec(),
            ),
            (
                "instrument decimals",
                Box::new(tick),
                &[2, 4],
                LabelPlacement::AboveRight,
                vec![
                    "P (10.333)",
                    "R1 (12.667)",
                    "S1 (8.667)",
                    "R2 (14.333)",
                    "S2 (6.333)",
                    "R3 (16.667)",
                    "S3 (4.667)",
                    "R4 (19.000)",
                    "S4 (3.000)",
                    "R5 (21.333)",
                    "S5 (1.333)",
                ],
            ),
            (
                "style precision over instrument decimals",
                Box::new(styled),
                &[2, 4],
                LabelPlacement::AboveRight,
                vec![
                    "10.3333", "12.6667", "8.6667", "14.3333", "6.3333", "16.6667", "4.6667",
                    "19.0000", "3.0000", "21.3333", "1.3333",
                ],
            ),
        ];
        for (name, mut study, bars, placement, texts) in cases {
            study.calculate(&three_days());
            let labels = study.labels();
            let per_period = texts.len();
            assert_eq!(labels.len(), per_period * bars.len(), "{name}");
            for (at, label) in labels.iter().enumerate() {
                let bar = bars[at / per_period];
                let plot = at % per_period;
                assert_eq!(
                    (label.bar, label.plot, label.placement),
                    (bar, plot, placement),
                    "{name}"
                );
                assert_eq!(
                    label.price,
                    study.values().value(plot, bar).unwrap(),
                    "{name}"
                );
            }
            let first: Vec<_> = labels[..per_period]
                .iter()
                .map(|l| l.text.as_str())
                .collect();
            assert_eq!(first, texts, "{name}");
        }

        let mut off = daily().with_labels(false, false);
        off.calculate(&three_days());
        assert!(off.labels().is_empty());
        let mut right = daily()
            .with_labels(true, true)
            .with_label_side(PivotLabelSide::Right);
        right.calculate(&three_days());
        assert_eq!(right.labels()[11].text, "P (11.67)");
    }

    /// One bar on 2 January of each year 2014 to 2025, with `k = year - 2014`: high 100 + k, low
    /// 90 + k, close 95 + k. Each row gives the block before the one holding 2020, its H, L, C and
    /// from them P = (H + L + C) / 3 and R1 = 2P - L.
    #[test]
    fn periods_of_several_years_start_in_years_divisible_by_their_length() {
        let bars: Vec<Bar> = (2014..=2025)
            .map(|year| {
                let k = f64::from(year - 2014);
                let time = Utc.with_ymd_and_hms(year, 1, 2, 0, 0, 0).unwrap();
                Bar::new(time, 95.0 + k, 100.0 + k, 90.0 + k, 95.0 + k, 1.0)
            })
            .collect();
        for (timeframe, p, r1) in [
            // 2018-2019: 105, 94, 100.
            (PivotTimeframe::TwoYears, 299.0 / 3.0, 316.0 / 3.0),
            // 2016-2018: 104, 92, 99.
            (PivotTimeframe::ThreeYears, 295.0 / 3.0, 314.0 / 3.0),
            // 2015-2019: 105, 91, 100.
            (PivotTimeframe::FiveYears, 296.0 / 3.0, 319.0 / 3.0),
            // 2014-2019: 105, 90, 100.
            (PivotTimeframe::TenYears, 295.0 / 3.0, 320.0 / 3.0),
        ] {
            let mut pivots = PivotPoints::new().with_timeframe(timeframe);
            pivots.calculate(&bars);
            let level = |line| pivots.values().value(line, 6).unwrap();
            assert!((level(0) - p).abs() < 1e-9, "{timeframe:?} P {}", level(0));
            assert!(
                (level(1) - r1).abs() < 1e-9,
                "{timeframe:?} R1 {}",
                level(1)
            );
        }
    }

    /// The daily bars of 2 to 6 March have high 13, low 7 and close 11, so the week of 9 March has
    /// P = 31/3, R1 = 2P - L = 41/3 and S1 = 2P - H = 23/3, whatever its hourly bars and whether
    /// the chart holds that earlier week at all.
    #[test]
    fn daily_bars_supply_the_previous_period_of_an_intraday_chart() {
        let day = |d| Utc.with_ymd_and_hms(2026, 3, d, 0, 0, 0).unwrap();
        let daily: Vec<Bar> = [
            (2, 10.0, 8.0, 9.0),
            (3, 12.0, 9.0, 11.0),
            (4, 11.0, 7.0, 8.0),
            (5, 13.0, 9.0, 12.0),
            (6, 12.0, 10.0, 11.0),
            (9, 60.0, 30.0, 45.0),
            (10, 60.0, 30.0, 45.0),
        ]
        .into_iter()
        .map(|(d, high, low, close)| Bar::new(day(d), close, high, low, close, 1.0))
        .collect();
        let hourly: Vec<Bar> = (10..13)
            .map(|h| {
                let time = Utc.with_ymd_and_hms(2026, 3, 9, h, 0, 0).unwrap();
                Bar::new(time, 45.0, 50.0, 40.0, 45.0, 1.0)
            })
            .collect();
        let study = || {
            let mut pivots = PivotPoints::new().with_timeframe(PivotTimeframe::Weekly);
            pivots.set_timeframe(Timeframe::Hour1);
            pivots
        };
        let mut pivots = study().with_daily_bars(true);
        let requests = pivots.data_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].timeframe, Some(Timeframe::Day1));
        pivots.update_data(
            &hourly,
            0,
            &[crate::studies::StudyData {
                request: requests[0].clone(),
                bars: &daily,
                clock: crate::model::BarClock::new(Timeframe::Day1),
                instrument: None,
                timeframe: Timeframe::Day1,
            }],
        );
        let mut on_daily = PivotPoints::new().with_timeframe(PivotTimeframe::Weekly);
        on_daily.set_timeframe(Timeframe::Day1);
        on_daily.calculate(&daily);
        for bar in 0..3 {
            for (line, want) in [(0, 31.0 / 3.0), (1, 41.0 / 3.0), (2, 23.0 / 3.0)] {
                let got = pivots.values().value(line, bar).unwrap();
                assert!((got - want).abs() < 1e-9, "line {line} bar {bar}: {got}");
            }
            for line in 0..11 {
                assert_eq!(
                    pivots.values().value(line, bar),
                    on_daily.values().value(line, 5),
                    "line {line} bar {bar}"
                );
            }
        }
        // A tick on the newest daily bar keeps the levels; a corrected earlier week moves them.
        let mut ticked = daily.clone();
        ticked[6].close = 44.0;
        let mut corrected = daily.clone();
        corrected[0].low = 1.0;
        for (series, p) in [(&ticked, 31.0 / 3.0), (&corrected, 25.0 / 3.0)] {
            pivots.update_data(
                &hourly,
                3,
                &[crate::studies::StudyData {
                    request: requests[0].clone(),
                    bars: series,
                    clock: crate::model::BarClock::new(Timeframe::Day1),
                    instrument: None,
                    timeframe: Timeframe::Day1,
                }],
            );
            assert!((pivots.values().value(0, 2).unwrap() - p).abs() < 1e-9);
        }

        // Without the option the first period has no levels; without daily bars from the host
        // chart prices do not stand in.
        let mut own = study();
        assert!(own.data_requests().is_empty());
        own.calculate(&hourly);
        let mut missing = study().with_daily_bars(true);
        missing.update_data(&hourly, 0, &[]);
        for study in [&own, &missing] {
            assert!((0..3).all(|bar| study.values().value(0, bar).is_none()));
        }
    }
}
