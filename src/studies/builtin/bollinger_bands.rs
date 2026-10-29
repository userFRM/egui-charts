//! Bollinger Bands indicator.
//!
//! Bollinger Bands are a volatility envelope placed above and below a
//! moving average, a simple one by default. The width of the bands is
//! determined by the standard deviation of closing prices, so they
//! automatically widen during volatile markets and contract during quiet
//! markets.
//!
//! # Components
//!
//! 1. **Upper band** = basis + (K * standard deviation)
//! 2. **Middle band** = basis = MA(close, period)
//! 3. **Lower band** = basis - (K * standard deviation)
//!
//! where *K* is the `std_dev` multiplier (typically 2.0) and the standard
//! deviation is the population deviation of the last `period` closes about
//! their simple mean, whatever the basis.
//!
//! The basis is one of the [`AverageType`] averages, simple by default:
//! exponential, weighted, smoothed or volume weighted are the others. It
//! keeps the seed and warm-up of the standalone average, but all three bands
//! start once a full window of `period` bars exists. A window with a missing
//! close leaves all three bands missing at that bar.
//!
//! # Default parameters
//!
//! `BollingerBands::new(20, 2.0)` -- 20-period SMA with 2 standard deviations.
//!
//! # Interpretation
//!
//! - Price touching the upper band: potentially overbought.
//! - Price touching the lower band: potentially oversold.
//! - Band squeeze (narrow bands): low volatility, often precedes a breakout.
//! - Walking the band: strong trend when price hugs one band.
//!
//! # Default colours
//!
//! Upper and lower bands use [`palette::ACCENT`](crate::studies::palette::ACCENT);
//! the middle band uses [`palette::BB_MIDDLE`](crate::studies::palette::BB_MIDDLE).
//!
//! # Example
//!
//! ```rust
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{BollingerBands, Indicator, IndicatorValue};
//!
//! let mut bb = BollingerBands::new(20, 2.0);
//! bb.calculate(&bars);
//!
//! if let IndicatorValue::Multiple(vals) = &bb.values().to_rows()[30] {
//!     let (upper, middle, lower) = (vals[0], vals[1], vals[2]);
//! }
//! ```

use super::{AverageType, clamp_param};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Bollinger Bands indicator.
///
/// A three-line volatility overlay: upper band, middle SMA, and lower band.
/// The bands expand and contract with market volatility.
#[derive(Clone)]
pub struct BollingerBands {
    period: usize,
    basis: AverageType,
    std_dev: f64,
    /// The basis average, built when a calculation starts; `None` for the simple mean.
    average: Option<Box<dyn Indicator>>,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl BollingerBands {
    /// `std_dev` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, std_dev: f64) -> Self {
        Self {
            period: period.max(1),
            basis: AverageType::Simple,
            std_dev: clamp_param(std_dev, 0.0, f64::MAX),
            average: None,
            values: IndicatorValues::new(),
            colors: vec![
                palette::ACCENT,    // Upper
                palette::BB_MIDDLE, // Middle
                palette::ACCENT,    // Lower
            ],
            visible: true,
        }
    }

    /// Use `basis` for the middle band. [`AverageType::Hull`] is not a Bollinger basis and
    /// leaves the basis unchanged.
    pub fn with_basis(mut self, basis: AverageType) -> Self {
        if basis != AverageType::Hull {
            self.basis = basis;
        }
        self
    }

    /// The bands at bar `i` of `data`.
    fn row(&self, data: &[Bar], i: usize) -> IndicatorValue {
        if i + 1 < self.period {
            return IndicatorValue::None;
        }
        let prices: Vec<f64> = data[i + 1 - self.period..=i]
            .iter()
            .map(|bar| bar.close)
            .collect();

        let sma = prices.iter().sum::<f64>() / self.period as f64;
        let variance = prices.iter().map(|p| (p - sma).powi(2)).sum::<f64>() / self.period as f64;
        let std = variance.sqrt();
        let middle = self.average.as_ref().map_or(sma, |average| {
            average.values().value(0, i).unwrap_or(f64::NAN)
        });
        if !std.is_finite() {
            return IndicatorValue::None;
        }

        IndicatorValue::Multiple(vec![
            middle + (std * self.std_dev),
            middle,
            middle - (std * self.std_dev),
        ])
    }
}

/// Construct with the conventional default parameters.
impl Default for BollingerBands {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for BollingerBands {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        basis: choice("Basis MA type", [AverageType::Simple, AverageType::Exponential, AverageType::Weighted, AverageType::Smoothed, AverageType::VolumeWeighted]),
        std_dev: number("Std dev", 0.0, f64::MAX)
    ; {});

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        let color = self.colors[0];
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Plot(2),
            color: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 16),
            inverted_color: None,
        }]
    }

    fn name(&self) -> &str {
        "BB"
    }

    fn desc(&self) -> &str {
        "Bollinger Bands - Volatility indicator with upper and lower bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        if from == 0 {
            self.average =
                (self.basis != AverageType::Simple).then(|| self.basis.build(self.period));
        }
        if let Some(average) = &mut self.average {
            average.update(data, from);
        }
        self.values.truncate(from);
        for i in from..data.len() {
            let row = self.row(data, i);
            self.values.push(row);
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            // If less than 3 colors provided, use the first color for all bands
            self.colors = vec![colors[0], colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_count(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("BB Upper({}, {})", self.period, self.std_dev),
            format!("BB Middle({}, {})", self.period, self.std_dev),
            format!("BB Lower({}, {})", self.period, self.std_dev),
        ]
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
}
