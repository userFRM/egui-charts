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
use super::over_finite_bars;
use crate::model::{Bar, Instrument, PriceSource};
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
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
    colors: Vec<Color32>,
    visible: bool,
}

impl TimeWeightedAvgPrice {
    /// The mean of ohlc4 over each session.
    pub fn new() -> Self {
        Self {
            anchor: AnchorPeriod::Session,
            source: PriceSource::OHLC4,
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.info],
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

    /// The values over `data`, every bar of which has a finite source.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let (mut period, mut sum, mut count) = (None, 0.0, 0.0);
        for (bar, day) in data.iter().zip(self.days.of(data)) {
            let key = Some(self.anchor.key(day));
            if key != period {
                (period, sum, count) = (key, 0.0, 0.0);
            }
            sum += self.price(bar);
            count += 1.0;
            values.push_lines(&[sum / count]);
        }
        values
    }

    fn price(&self, bar: &Bar) -> f64 {
        self.source.compute(bar.open, bar.high, bar.low, bar.close)
    }
}

impl Default for TimeWeightedAvgPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for TimeWeightedAvgPrice {
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
        self.values = over_finite_bars(
            data,
            |b| self.price(b).is_finite(),
            |data| self.compute(data),
        );
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
