//! VWAP Auto Anchored.
//!
//! A volume weighted average price drawn only from one anchor bar to the latest bar. The anchor
//! is the first bar of the last anchor period in the data (a session, week, month, quarter,
//! year, decade or century of trading days), or the bar with the highest high, the lowest low
//! or the highest volume among the last `length` bars, the most recent of equal bars. Auto
//! takes the period from the chart interval: the session below a day, the month at a day,
//! the quarter up to 10 days, the year up to 60 days and the decade above. The anchor moves
//! as bars arrive, and the line is redrawn from the new anchor.
//!
//! # Calculation
//! VWAP = Sum(Source * Volume) / Sum(Volume) from the anchor bar, Source = (High + Low + Close) / 3
//! by default; the source itself while the volume sum is zero.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AutoAnchor, Indicator, VwapAutoAnchored};
//!
//! // From the highest high of the last 50 bars.
//! let mut vwap = VwapAutoAnchored::new()
//!     .with_anchor(AutoAnchor::HighestHigh)
//!     .with_length(50);
//! vwap.calculate(&bars);
//! assert!(vwap.values().value(0, bars.len() - 1).is_some());
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use crate::model::{Bar, Instrument, PriceSource, Timeframe};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// How [`VwapAutoAnchored`] finds its anchor bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutoAnchor {
    /// The period by chart interval: session below a day, month at a day, quarter up to 10
    /// days, year up to 60 days, decade above.
    #[default]
    Auto,
    /// The highest high among the last `length` bars.
    HighestHigh,
    /// The lowest low among the last `length` bars.
    LowestLow,
    /// The highest volume among the last `length` bars.
    HighestVolume,
    /// The start of the last trading session.
    Session,
    /// The start of the last ISO week.
    Week,
    /// The start of the last calendar month.
    Month,
    /// The start of the last calendar quarter.
    Quarter,
    /// The start of the last calendar year.
    Year,
    /// The start of the last decade.
    Decade,
    /// The start of the last century.
    Century,
}

/// VWAP from an automatically found anchor bar to the latest bar.
///
/// Periods are counted in exchange trading days, as
/// [`VolumeWeightedAvgPrice`](super::VolumeWeightedAvgPrice) counts them. The chart supplies
/// its interval for [`AutoAnchor::Auto`] and the instrument's time zone and session; direct
/// callers set them with [`Indicator::set_timeframe`] (one minute until then) and
/// [`Indicator::set_instrument`] (UTC until then). Overlay indicator.
#[derive(Clone)]
pub struct VwapAutoAnchored {
    anchor: AutoAnchor,
    length: usize,
    source: PriceSource,
    interval: Timeframe,
    days: ExchangeDays,
    values: IndicatorValues,
    stepper: Stepper<State>,
    intraday: bool,
    shown_start: usize,
    shown_indices: Vec<usize>,
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Copy)]
struct Point {
    source: usize,
    high: f64,
    low: f64,
    volume: f64,
    price: f64,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    previous_date: Option<chrono::NaiveDate>,
    intraday: bool,
    period: Option<(i32, u32)>,
    start: usize,
    volume: f64,
    weighted: f64,
    candidates: std::collections::VecDeque<Point>,
}

impl VwapAutoAnchored {
    /// Anchor Auto, length 20, source hlc3.
    pub fn new() -> Self {
        Self {
            anchor: AutoAnchor::Auto,
            length: 20,
            source: PriceSource::HLC3,
            interval: Timeframe::Min1,
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            intraday: false,
            shown_start: 0,
            shown_indices: Vec::new(),
            colors: vec![palette::YELLOW],
            visible: true,
        }
    }

    /// How the anchor bar is found.
    pub fn with_anchor(mut self, anchor: AutoAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// The bars searched by the highest high, lowest low and highest volume anchors,
    /// raised to 1 or more.
    pub fn with_length(mut self, length: usize) -> Self {
        self.length = length.max(1);
        self
    }

    /// The price averaged. The anchor search reads the high, low or volume whatever the source.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }

    fn period(&self) -> Option<AnchorPeriod> {
        let day = Timeframe::Day1.duration_ms();
        Some(match self.anchor {
            AutoAnchor::Auto => match self.interval.duration_ms() {
                ms if ms < day => AnchorPeriod::Session,
                ms if ms <= day => AnchorPeriod::Month,
                ms if ms <= 10 * day => AnchorPeriod::Quarter,
                ms if ms <= 60 * day => AnchorPeriod::Year,
                _ => AnchorPeriod::Decade,
            },
            AutoAnchor::Session => AnchorPeriod::Session,
            AutoAnchor::Week => AnchorPeriod::Week,
            AutoAnchor::Month => AnchorPeriod::Month,
            AutoAnchor::Quarter => AnchorPeriod::Quarter,
            AutoAnchor::Year => AnchorPeriod::Year,
            AutoAnchor::Decade => AnchorPeriod::Decade,
            AutoAnchor::Century => AnchorPeriod::Century,
            _ => return None,
        })
    }
}

impl Default for VwapAutoAnchored {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VwapAutoAnchored {
    crate::studies::inputs::study_inputs!(self;
        anchor: choice("Anchor Period", [AutoAnchor::Auto, AutoAnchor::HighestHigh, AutoAnchor::LowestLow, AutoAnchor::HighestVolume, AutoAnchor::Session, AutoAnchor::Week, AutoAnchor::Month, AutoAnchor::Quarter, AutoAnchor::Year, AutoAnchor::Decade, AutoAnchor::Century]),
        length: length("Length", 1, usize::MAX),
        source: choice("Source", PriceSource::all())
    ; {});

    fn name(&self) -> &str {
        "VWAP Auto Anchored"
    }

    fn desc(&self) -> &str {
        "VWAP Auto Anchored - Volume weighted average price from an automatically found anchor"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], mut from: usize) {
        let period = self.period();
        let price_source = self.source;
        let usable = |bar: &Bar| {
            price_source
                .compute(bar.open, bar.high, bar.low, bar.close)
                .is_finite()
                && bar.high.is_finite()
                && bar.low.is_finite()
                && bar.volume.is_finite()
        };
        let len = self.values.len();
        if len == 0 || from < len - 1 || data.len() < len {
            self.shown_start = 0;
        }
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let source = state.source;
                    state.source += 1;
                    if !usable(bar) {
                        return IndicatorValue::None;
                    }
                    let price = price_source.compute(bar.open, bar.high, bar.low, bar.close);
                    let Some(period) = period else {
                        state.candidates.push_back(Point {
                            source,
                            high: bar.high,
                            low: bar.low,
                            volume: bar.volume,
                            price,
                        });
                        if state.candidates.len() > self.length {
                            state.candidates.pop_front();
                        }
                        return IndicatorValue::None;
                    };
                    let date = bar.time.date_naive();
                    state.intraday |= state.previous_date == Some(date);
                    state.previous_date = Some(date);
                    let key = Some(period.key(self.days.day(bar, !self.intraday)));
                    if key != state.period {
                        state.period = key;
                        state.start = source;
                        state.volume = 0.0;
                        state.weighted = 0.0;
                    }
                    state.volume += bar.volume;
                    state.weighted += price * bar.volume;
                    IndicatorValue::Single(if state.volume == 0.0 {
                        price
                    } else {
                        state.weighted / state.volume
                    })
                });
            if period.is_none() || self.intraday == self.stepper.state().intraday {
                break;
            }
            self.intraday = self.stepper.state().intraday;
            self.shown_start = 0;
            from = 0;
        }
        if period.is_some() {
            let start = self.stepper.state().start;
            if start > self.shown_start {
                for bar in self.shown_start..start {
                    self.values.set(bar, IndicatorValue::None);
                }
            } else if start < self.shown_start {
                // Withdrawing a new anchor period restores the previously visible line.
                let (mut volume, mut weighted) = (0.0, 0.0);
                for (i, bar) in data.iter().enumerate().take(self.shown_start).skip(start) {
                    if !usable(bar) {
                        continue;
                    }
                    let price = price_source.compute(bar.open, bar.high, bar.low, bar.close);
                    volume += bar.volume;
                    weighted += price * bar.volume;
                    self.values.set_value(
                        0,
                        i,
                        if volume == 0.0 {
                            price
                        } else {
                            weighted / volume
                        },
                    );
                }
            }
            self.shown_start = start;
        } else {
            for &bar in &self.shown_indices {
                if bar < self.values.len() {
                    self.values.set(bar, IndicatorValue::None);
                }
            }
            self.shown_indices.clear();
            let candidates = &self.stepper.state().candidates;
            let key = |point: &Point| match self.anchor {
                AutoAnchor::HighestHigh => point.high,
                AutoAnchor::LowestLow => -point.low,
                _ => point.volume,
            };
            let start = (0..candidates.len())
                .max_by(|&a, &b| key(&candidates[a]).total_cmp(&key(&candidates[b])))
                .unwrap_or(candidates.len());
            let (mut volume, mut weighted) = (0.0, 0.0);
            for point in candidates.iter().skip(start) {
                volume += point.volume;
                weighted += point.price * point.volume;
                self.values.set_value(
                    0,
                    point.source,
                    if volume == 0.0 {
                        point.price
                    } else {
                        weighted / volume
                    },
                );
                self.shown_indices.push(point.source);
            }
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.interval = timeframe;
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.days.set(instrument)
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(color) = colors.first() {
            self.colors = vec![*color];
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
        vec!["VWAP".to_string()]
    }
}
