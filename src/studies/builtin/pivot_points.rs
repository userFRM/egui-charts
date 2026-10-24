//! Pivot Points Standard.
//!
//! Support and resistance levels for each calendar period (day, week, month, quarter or year
//! of exchange trading days) from the open, high, low and close of the previous period in the
//! data. With H, L, C and O the previous period's high, low, close and open, R = H - L and
//! P = (H + L + C) / 3:
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
//! is not drawn across the change from one period to the next.
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
//! assert_eq!(pivots.line_cnt(), 11);
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use super::over_finite_bars;
use crate::model::{Bar, Instrument, Timeframe};
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
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
}

/// Pivot Points Standard: support and resistance levels for each calendar period from the
/// previous period.
///
/// Periods are counted in exchange trading days, as
/// [`VolumeWeightedAvgPrice`](super::VolumeWeightedAvgPrice) counts them. The chart supplies
/// its interval for [`PivotTimeframe::Auto`] and the instrument's time zone and session;
/// direct callers set them with [`Indicator::set_timeframe`] (one minute until then) and
/// [`Indicator::set_instrument`] (UTC until then). The levels come from the chart's own bars.
/// Overlay indicator.
#[derive(Clone)]
pub struct PivotPoints {
    method: PivotMethod,
    timeframe: PivotTimeframe,
    pivots_back: usize,
    interval: Timeframe,
    days: ExchangeDays,
    values: IndicatorValues,
    /// One per line: P, then R1, S1 through R5, S5.
    colors: Vec<Color32>,
    visible: bool,
}

impl PivotPoints {
    /// Traditional pivots, timeframe Auto, for the last 15 periods.
    pub fn new() -> Self {
        let t = &DESIGN_TOKENS.semantic.extended;
        // Pivot amber, resistances red and supports green, darker further out.
        let colors = vec![
            t.favorite_gold,
            t.error,
            t.success,
            t.error_dark,
            t.success_dark,
            t.error_darker,
            t.success_darker,
            t.error_darker,
            t.success_darker,
            t.error_darker,
            t.success_darker,
        ];
        Self {
            method: PivotMethod::Traditional,
            timeframe: PivotTimeframe::Auto,
            pivots_back: 15,
            interval: Timeframe::Min1,
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            colors,
            visible: true,
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

    fn period(&self) -> AnchorPeriod {
        match self.timeframe {
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
        }
    }

    /// `[P, R1, S1, ..., R5, S5]` from the previous period and the current period's open.
    fn levels(&self, prev: &Bar, open: f64) -> [f64; 11] {
        let (h, l, c) = (prev.high, prev.low, prev.close);
        let r = h - l;
        let p = (h + l + c) / 3.0;
        let nan = f64::NAN;
        match self.method {
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

    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let period = self.period();
        // Each period's key, first bar and open, high, low and close.
        let mut periods: Vec<((i32, u32), usize, Bar)> = Vec::new();
        for (i, (bar, day)) in data.iter().zip(self.days.of(data)).enumerate() {
            let key = period.key(day);
            match periods.last_mut() {
                Some((last, _, ohlc)) if *last == key => {
                    ohlc.high = ohlc.high.max(bar.high);
                    ohlc.low = ohlc.low.min(bar.low);
                    ohlc.close = bar.close;
                }
                _ => periods.push((key, i, bar.clone())),
            }
        }
        let mut values = IndicatorValues::missing(data.len());
        let first = periods.len().saturating_sub(self.pivots_back).max(1);
        for k in first..periods.len() {
            let start = periods[k].1;
            let end = periods.get(k + 1).map_or(data.len(), |next| next.1);
            let levels = self.levels(&periods[k - 1].2, data[start].open);
            for bar in start..end {
                for (line, level) in levels.iter().enumerate() {
                    values.set_value(line, bar, *level);
                }
            }
        }
        values
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
        timeframe: choice("Pivots Timeframe", [PivotTimeframe::Auto, PivotTimeframe::Daily, PivotTimeframe::Weekly, PivotTimeframe::Monthly, PivotTimeframe::Quarterly, PivotTimeframe::Yearly]),
        pivots_back: length("Number of Pivots Back", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Pivot Points Standard"
    }

    fn desc(&self) -> &str {
        "Pivot Points Standard - Support and resistance levels from the previous calendar period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        let open = matches!(self.method, PivotMethod::Woodie | PivotMethod::DM);
        self.values = over_finite_bars(
            data,
            |b| {
                b.high.is_finite()
                    && b.low.is_finite()
                    && b.close.is_finite()
                    && (!open || b.open.is_finite())
            },
            |data| self.compute(data),
        );
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.interval = timeframe;
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.days.set(instrument)
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
        [
            "P", "R1", "S1", "R2", "S2", "R3", "S3", "R4", "S4", "R5", "S5",
        ]
        .map(String::from)
        .to_vec()
    }
}
