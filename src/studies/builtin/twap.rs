//! Time Weighted Average Price (TWAP).
//!
//! The arithmetic mean of a price source, (open + high + low + close) / 4 by default, over the
//! bars since the start of the anchor period: a trading session by default, or a week, month,
//! quarter, year, decade or century of trading days. Each bar weighs the same, so on bars of
//! equal duration the mean weighs every interval of time equally.
//!
//! # Calculation
//! TWAP = Sum(Source) / Count since the start of the period
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AnchorPeriod, Indicator, TimeWeightedAvgPrice};
//!
//! let mut twap = TimeWeightedAvgPrice::new().with_anchor(AnchorPeriod::Week);
//! twap.calculate(&bars);
//! assert_eq!(twap.values().len(), bars.len());
//! ```

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use crate::model::{Bar, Instrument, PriceSource};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Time Weighted Average Price, restarted at each anchor period.
///
/// Periods are counted in exchange trading days, as
/// [`VolumeWeightedAvgPrice`](super::VolumeWeightedAvgPrice) counts them, in the time zone and
/// session of the chart instrument ([`Indicator::set_instrument`]; UTC until then).
/// Overlay indicator.
#[derive(Clone)]
pub struct TimeWeightedAvgPrice {
    anchor: AnchorPeriod,
    source: PriceSource,
    days: ExchangeDays,
    values: IndicatorValues,
    stepper: Stepper<State>,
    intraday: bool,
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous_date: Option<chrono::NaiveDate>,
    intraday: bool,
    period: Option<(i32, u32)>,
    sum: f64,
    count: f64,
}

impl TimeWeightedAvgPrice {
    /// The mean of ohlc4 over each session.
    pub fn new() -> Self {
        Self {
            anchor: AnchorPeriod::Session,
            source: PriceSource::OHLC4,
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            intraday: false,
            colors: vec![palette::BLUE],
            visible: true,
        }
    }

    /// Restarts at each selected calendar period.
    pub fn with_anchor(mut self, anchor: AnchorPeriod) -> Self {
        self.anchor = anchor;
        self
    }

    /// The price averaged.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }
}

impl Default for TimeWeightedAvgPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for TimeWeightedAvgPrice {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(self.source))
    }

    crate::studies::inputs::study_inputs!(self;
        anchor: choice("Anchor Period", [AnchorPeriod::Session, AnchorPeriod::Week, AnchorPeriod::Month, AnchorPeriod::Quarter, AnchorPeriod::Year, AnchorPeriod::Decade, AnchorPeriod::Century]),
        source: choice("Source", PriceSource::all())
    ; {});

    fn name(&self) -> &str {
        "Time Weighted Average Price"
    }

    fn desc(&self) -> &str {
        "Time Weighted Average Price - Mean price since the start of each anchor period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], mut from: usize) {
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let price = self.source.compute(bar.open, bar.high, bar.low, bar.close);
                    if !price.is_finite() {
                        return IndicatorValue::None;
                    }
                    let date = bar.time.date_naive();
                    state.intraday |= state.previous_date == Some(date);
                    state.previous_date = Some(date);
                    let key = Some(self.anchor.key(self.days.day(bar, !self.intraday)));
                    if key != state.period {
                        (state.period, state.sum, state.count) = (key, 0.0, 0.0);
                    }
                    state.sum += price;
                    state.count += 1.0;
                    IndicatorValue::Single(state.sum / state.count)
                });
            if self.intraday == self.stepper.state().intraday {
                break;
            }
            // Switching the inferred interval changes the exchange dates of earlier bars too.
            self.intraday = self.stepper.state().intraday;
            from = 0;
        }
    }

    fn is_incremental(&self) -> bool {
        true
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
        vec!["TWAP".to_string()]
    }
}
