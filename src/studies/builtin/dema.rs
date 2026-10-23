//! Double Exponential Moving Avg (DEMA) Indicator
//!
//! DEMA reduces the lag inherent in EMAs by applying EMA twice and
//! then subtracting a smoothed EMA.
//!
//! # Formula
//! DEMA = 2 * EMA(price, period) - EMA(EMA(price, period), period)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{DEMA, Indicator};
//!
//! let mut dema = DEMA::new(14);
//! dema.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Double Exponential Moving Avg indicator
#[derive(Clone)]
pub struct DEMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl DEMA {
    /// Create a new DEMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// EMA of the values that exist in `data` (NaN lead-in), seeded with the mean of the first
    /// `period` of them.
    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        let mut ema = vec![f64::NAN; data.len()];
        let Some(first) = data.iter().position(|v| !v.is_nan()) else {
            return ema;
        };
        if data.len() - first < period {
            return ema;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let seed = first + period - 1;
        ema[seed] = data[first..=seed].iter().sum::<f64>() / period as f64;
        for i in seed + 1..data.len() {
            ema[i] = (data[i] - ema[i - 1]) * multiplier + ema[i - 1];
        }

        ema
    }

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();

        if data.is_empty() {
            return values;
        }

        // Extract close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate first EMA
        let ema1 = Self::calculate_ema(&closes, self.period);

        // EMA of EMA, seeded from the first `period` values of EMA1
        let ema2 = Self::calculate_ema(&ema1, self.period);

        // Calculate DEMA = 2 * EMA1 - EMA2
        for i in 0..data.len() {
            if ema2[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                let dema = 2.0 * ema1[i] - ema2[i];
                values.push(IndicatorValue::Single(dema));
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for DEMA {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for DEMA {
    fn name(&self) -> &str {
        "DEMA"
    }

    fn desc(&self) -> &str {
        "Double Exponential Moving Avg - Reduces lag compared to EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        1
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
        vec![format!("DEMA({})", self.period)]
    }
}
