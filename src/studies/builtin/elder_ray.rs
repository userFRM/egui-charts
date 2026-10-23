use super::over_finite_bars;
use crate::model::Bar;
/// Elder Ray Index (Bull Power / Bear Power)
/// Measures buying and selling pressure
/// Developed by Alexander Elder
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ElderRay {
    /// EMA period (typically 13)
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Bull Power (green), Bear Power (red)]
    colors: Vec<Color32>,
    visible: bool,
}

impl ElderRay {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Bull Power
                DESIGN_TOKENS.semantic.extended.error,   // Red - Bear Power
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// Calculate EMA of close prices
    fn calculate_ema(data: &[Bar], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // First value is SMA
        let first_sma: f64 = data.iter().take(period).map(|b| b.close).sum::<f64>() / period as f64;

        for i in 0..data.len() {
            if i < period - 1 {
                ema.push(f64::NAN);
            } else if i == period - 1 {
                ema.push(first_sma);
            } else {
                let prev = ema[i - 1];
                ema.push((data[i].close - prev) * multiplier + prev);
            }
        }

        ema
    }

    /// The values over `data`, every bar of which has a finite high, low and close.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate EMA
        let ema = Self::calculate_ema(data, self.period);

        // Calculate Bull Power and Bear Power
        for i in 0..data.len() {
            if ema[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                // Bull Power = High - EMA
                let bull_power = data[i].high - ema[i];
                // Bear Power = Low - EMA
                let bear_power = data[i].low - ema[i];

                values.push(IndicatorValue::Multiple(vec![bull_power, bear_power]));
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for ElderRay {
    fn default() -> Self {
        Self::new(13)
    }
}

impl Indicator for ElderRay {
    fn name(&self) -> &str {
        "Elder Ray"
    }

    fn desc(&self) -> &str {
        "Elder Ray - Bull and Bear Power indicators"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 2 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        false // Separate pane
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
        vec![
            format!("Bull({})", self.period),
            format!("Bear({})", self.period),
        ]
    }
}
