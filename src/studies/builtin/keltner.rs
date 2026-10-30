//! Keltner Channels Indicator
//!
//! Keltner Channels are volatility-based bands set above and below a moving average.
//!
//! # Components
//! - Middle Line: an exponential average of the source, close by default, seeded with the mean
//!   of its first `length` values, or a simple average of the source.
//! - Upper and Lower Bands: the middle line plus and minus the multiplier times the band
//!   distance, which is Wilder's average true range over the ATR length (the default), the
//!   bar's own true range, or Wilder's average of the bar's high minus low over the middle
//!   line's length.
//!
//! A bar with a non-finite high, low, close or source price is skipped and changes nothing.
//!
//! # Interpretation
//! - Price above upper band: Strong uptrend or overbought
//! - Price below lower band: Strong downtrend or oversold
//! - Channel squeeze (narrow bands): Low volatility, potential breakout
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{KeltnerChannels, Indicator};
//!
//! let mut keltner = KeltnerChannels::new(20, 10, 2.0);
//! keltner.calculate(&bars);
//! ```

use super::clamp_param;
use super::price_oscillator::{Mean, OscillatorAverage};
use super::smoothing::Smoothing;
use crate::model::{Bar, PriceSource};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// How far the bands of [`KeltnerChannels`] lie from the middle line, before the multiplier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeltnerBand {
    /// Wilder's average of the true range over the ATR length.
    AverageTrueRange,
    /// The bar's own true range; high minus low on the first bar.
    TrueRange,
    /// Wilder's average of the bar's high minus low over the middle line's length.
    Range,
}

/// Keltner Channels indicator
#[derive(Clone)]
pub struct KeltnerChannels {
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
    source: PriceSource,
    average: OscillatorAverage,
    band: KeltnerBand,
    values: IndicatorValues,
    stepper: Stepper<State>,
    upper_color: Color32,
    middle_color: Color32,
    lower_color: Color32,
    fill_color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<f64>,
    middle: Mean,
    band: Smoothing,
}

impl KeltnerChannels {
    /// Create a new Keltner Channels indicator
    ///
    /// # Arguments
    /// * `ema_period` - EMA period for middle line (typically 20)
    /// * `atr_period` - ATR period (typically 10)
    /// * `multiplier` - ATR multiplier (typically 2.0), clamped into 0 to `f64::MAX`, NaN to 0
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period: ema_period.max(1),
            atr_period: atr_period.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            source: PriceSource::Close,
            average: OscillatorAverage::Exponential,
            band: KeltnerBand::AverageTrueRange,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            upper_color: palette::BLUE,
            middle_color: palette::BLUE,
            lower_color: palette::BLUE,
            fill_color: palette::LIGHT_BLUE.gamma_multiply_u8(13),
            visible: true,
        }
    }

    /// Create with default params (20, 10, 2.0)
    pub fn default_params() -> Self {
        Self::new(20, 10, 2.0)
    }

    /// Set colors for upper, middle, and lower bands
    pub fn with_colors(mut self, upper: Color32, middle: Color32, lower: Color32) -> Self {
        self.upper_color = upper;
        self.middle_color = middle;
        self.lower_color = lower;
        self.fill_color = Color32::from_rgba_unmultiplied(upper.r(), upper.g(), upper.b(), 16);
        self
    }

    /// Average `source` for the middle line instead of the close. The bands keep using the bars.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }

    /// Use an exponential (the default) or simple average for the middle line.
    pub fn with_average(mut self, average: OscillatorAverage) -> Self {
        self.average = average;
        self
    }

    /// Measure the band distance with `band` instead of the average true range.
    pub fn with_band(mut self, band: KeltnerBand) -> Self {
        self.band = band;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for KeltnerChannels {
    fn default() -> Self {
        Self::new(20, 10, 2.0)
    }
}

impl Indicator for KeltnerChannels {
    crate::studies::inputs::study_inputs!(self;
        ema_period: length("Length", 1, usize::MAX),
        multiplier: number("Multiplier", 0.0, f64::MAX),
        source: choice("Source", PriceSource::all()),
        average: choice("Average", [OscillatorAverage::Exponential, OscillatorAverage::Simple]),
        band: choice("Band style", [KeltnerBand::AverageTrueRange, KeltnerBand::TrueRange, KeltnerBand::Range]),
        atr_period: length("ATR length", 1, usize::MAX)
    ; {});

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(1),
            second: crate::studies::FillBoundary::Plot(2),
            color: self.fill_color,
            inverted_color: None,
        }]
    }

    fn name(&self) -> &str {
        "Keltner"
    }

    fn desc(&self) -> &str {
        "Keltner Channels - Volatility-based envelope"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let price = self.source.compute(bar.open, bar.high, bar.low, bar.close);
                if ![bar.high, bar.low, bar.close, price]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    return IndicatorValue::None;
                }
                let tr = state
                    .previous
                    .map_or(bar.high - bar.low, |close| ATR::true_range(bar, close));
                state.previous = Some(bar.close);
                let middle = state.middle.next(self.average, price, self.ema_period);
                let band = match self.band {
                    KeltnerBand::AverageTrueRange => {
                        state
                            .band
                            .next(tr, self.atr_period, 1.0 / self.atr_period as f64)
                    }
                    KeltnerBand::TrueRange => tr,
                    KeltnerBand::Range => state.band.next(
                        bar.high - bar.low,
                        self.ema_period,
                        1.0 / self.ema_period as f64,
                    ),
                };
                // Only NaN is missing here: infinite bands from a huge multiplier keep the middle line.
                if middle.is_nan() || band.is_nan() {
                    return IndicatorValue::None;
                }
                let width = self.multiplier * band;
                IndicatorValue::Multiple(vec![middle, middle + width, middle - width])
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![
            self.middle_color,
            self.upper_color,
            self.lower_color,
            self.fill_color,
        ]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.middle_color = colors[0];
        }
        if colors.len() > 1 {
            self.upper_color = colors[1];
        }
        if colors.len() > 2 {
            self.lower_color = colors[2];
        }
        if let Some(&color) = colors.get(3) {
            self.fill_color = color;
        } else if colors.len() > 1 {
            let color = self.upper_color;
            self.fill_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 16);
        }
    }

    fn is_overlay(&self) -> bool {
        true // Keltner is drawn on main chart
    }

    fn line_count(&self) -> usize {
        3 // Middle, Upper, Lower
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
        let average = match self.average {
            OscillatorAverage::Exponential => "EMA",
            OscillatorAverage::Simple => "SMA",
        };
        let band = match self.band {
            KeltnerBand::AverageTrueRange => "ATR",
            KeltnerBand::TrueRange => "TR",
            KeltnerBand::Range => "Range",
        };
        vec![
            format!("{average}({})", self.ema_period),
            format!("Upper({}x {band})", self.multiplier),
            format!("Lower({}x {band})", self.multiplier),
        ]
    }
}
