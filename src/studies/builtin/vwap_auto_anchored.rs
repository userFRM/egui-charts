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
use super::over_finite_bars;
use crate::model::{Bar, Instrument, PriceSource, Timeframe};
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValues};
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
    colors: Vec<Color32>,
    visible: bool,
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

    fn price(&self, bar: &Bar) -> f64 {
        self.source.compute(bar.open, bar.high, bar.low, bar.close)
    }

    /// The first bar of the last period in `data`.
    fn period_start(&self, data: &[Bar], period: AnchorPeriod) -> usize {
        let keys: Vec<_> = self
            .days
            .of(data)
            .into_iter()
            .map(|day| period.key(day))
            .collect();
        keys.iter()
            .rposition(|key| Some(key) != keys.last())
            .map_or(0, |i| i + 1)
    }

    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let window = data.len().saturating_sub(self.length);
        // `max_by` keeps the last of equal bars: the most recent.
        let extreme = |key: fn(&Bar) -> f64| {
            (window..data.len()).max_by(|&a, &b| key(&data[a]).total_cmp(&key(&data[b])))
        };
        let day = Timeframe::Day1.duration_ms();
        let start = match self.anchor {
            AutoAnchor::Auto => {
                let period = match self.interval.duration_ms() {
                    ms if ms < day => AnchorPeriod::Session,
                    ms if ms <= day => AnchorPeriod::Month,
                    ms if ms <= 10 * day => AnchorPeriod::Quarter,
                    ms if ms <= 60 * day => AnchorPeriod::Year,
                    _ => AnchorPeriod::Decade,
                };
                Some(self.period_start(data, period))
            }
            AutoAnchor::HighestHigh => extreme(|b| b.high),
            AutoAnchor::LowestLow => extreme(|b| -b.low),
            AutoAnchor::HighestVolume => extreme(|b| b.volume),
            AutoAnchor::Session => Some(self.period_start(data, AnchorPeriod::Session)),
            AutoAnchor::Week => Some(self.period_start(data, AnchorPeriod::Week)),
            AutoAnchor::Month => Some(self.period_start(data, AnchorPeriod::Month)),
            AutoAnchor::Quarter => Some(self.period_start(data, AnchorPeriod::Quarter)),
            AutoAnchor::Year => Some(self.period_start(data, AnchorPeriod::Year)),
            AutoAnchor::Decade => Some(self.period_start(data, AnchorPeriod::Decade)),
            AutoAnchor::Century => Some(self.period_start(data, AnchorPeriod::Century)),
        };
        let mut values = IndicatorValues::missing(data.len());
        let (mut volume, mut weighted) = (0.0, 0.0);
        for (i, bar) in data.iter().enumerate().skip(start.unwrap_or(data.len())) {
            let price = self.price(bar);
            volume += bar.volume;
            weighted += price * bar.volume;
            let vwap = if volume.abs() < 1e-10 {
                price
            } else {
                weighted / volume
            };
            values.set_value(0, i, vwap);
        }
        values
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
        self.values = over_finite_bars(
            data,
            |b| {
                self.price(b).is_finite()
                    && b.high.is_finite()
                    && b.low.is_finite()
                    && b.volume.is_finite()
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
