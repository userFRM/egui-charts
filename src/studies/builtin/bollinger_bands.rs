//! Bollinger Bands indicator.
//!
//! Bollinger Bands are a volatility envelope placed above and below a
//! simple moving average. The width of the bands is determined by the
//! standard deviation of closing prices, so they automatically widen
//! during volatile markets and contract during quiet markets.
//!
//! # Components
//!
//! 1. **Upper band** = SMA + (K * standard deviation)
//! 2. **Middle band** = SMA(close, period)
//! 3. **Lower band** = SMA - (K * standard deviation)
//!
//! where *K* is the `std_dev` multiplier (typically 2.0).
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
//! Upper/middle/lower sourced from `indicators.bb_upper`, `bb_middle`, `bb_lower`.
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

use super::clamp_param;
use crate::model::Bar;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Bollinger Bands indicator.
///
/// A three-line volatility overlay: upper band, middle SMA, and lower band.
/// The bands expand and contract with market volatility.
#[derive(Clone)]
pub struct BollingerBands {
    period: usize,
    std_dev: f64,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl BollingerBands {
    /// `std_dev` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, std_dev: f64) -> Self {
        Self {
            period: period.max(1),
            std_dev: clamp_param(std_dev, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.bb_upper,  // Upper
                DESIGN_TOKENS.semantic.indicators.bb_middle, // Middle
                DESIGN_TOKENS.semantic.indicators.bb_lower,  // Lower
            ],
            visible: true,
        }
    }

    /// The values over `data`.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let prices: Vec<f64> = data[i + 1 - self.period..=i]
                    .iter()
                    .map(|bar| bar.close)
                    .collect();

                let sma = prices.iter().sum::<f64>() / self.period as f64;
                let variance =
                    prices.iter().map(|p| (p - sma).powi(2)).sum::<f64>() / self.period as f64;
                let std = variance.sqrt();

                let upper = sma + (std * self.std_dev);
                let lower = sma - (std * self.std_dev);

                values.push_lines(&[upper, sma, lower]);
            }
        }
        values
    }
}

/// Construct with the conventional default parameters.
impl Default for BollingerBands {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for BollingerBands {
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
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period - 1, |_| true),
            |data| self.compute(data),
        );
        self.values = values;
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

    fn line_cnt(&self) -> usize {
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
