//! Triple Exponential Moving Avg (TEMA) Indicator
//!
//! TEMA further reduces lag compared to DEMA by applying EMA three times
//! with a specific formula.
//!
//! # Formula
//! TEMA = 3 * EMA1 - 3 * EMA2 + EMA3
//! Where:
//! - EMA1 = EMA(price, period)
//! - EMA2 = EMA(EMA1, period)
//! - EMA3 = EMA(EMA2, period)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{TEMA, Indicator};
//!
//! let mut tema = TEMA::new(14);
//! tema.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Triple Exponential Moving Avg indicator
#[derive(Clone)]
pub struct TEMA {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl TEMA {
    /// Create a new TEMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.pink, // Pink
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
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.is_empty() {
            return values;
        }

        // Extract close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate first EMA
        let ema1 = Self::calculate_ema(&closes, self.period);

        // Calculate second EMA (EMA of EMA)
        let ema2 = Self::calculate_ema(&ema1, self.period);

        // Calculate third EMA (EMA of EMA of EMA)
        let ema3 = Self::calculate_ema(&ema2, self.period);

        // Calculate TEMA = 3 * EMA1 - 3 * EMA2 + EMA3
        for i in 0..data.len() {
            if ema3[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                let tema = 3.0 * ema1[i] - 3.0 * ema2[i] + ema3[i];
                values.push(IndicatorValue::Single(tema));
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for TEMA {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for TEMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "TEMA"
    }

    fn desc(&self) -> &str {
        "Triple Exponential Moving Avg - Minimal lag moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
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
        vec![format!("TEMA({})", self.period)]
    }
}
