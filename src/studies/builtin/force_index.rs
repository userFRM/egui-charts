use super::over_finite_bars;
use crate::model::Bar;
/// Force Index
/// Measures the force behind price movements using price change and volume
/// Developed by Alexander Elder
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ForceIndex {
    /// EMA period for smoothing (typically 13)
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ForceIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate EMA
    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // Find first non-NaN value to start SMA
        let mut first_valid_idx = 0;
        let mut sum = 0.0;
        let mut count = 0;

        for (i, &value) in data.iter().enumerate() {
            if !value.is_nan() {
                if count < period {
                    sum += value;
                    count += 1;
                }
                if count == period {
                    first_valid_idx = i;
                    break;
                }
            }
        }

        for i in 0..data.len() {
            if i < first_valid_idx {
                ema.push(f64::NAN);
            } else if i == first_valid_idx {
                ema.push(sum / period as f64);
            } else {
                let prev = ema[i - 1];
                if prev.is_nan() || data[i].is_nan() {
                    ema.push(f64::NAN);
                } else {
                    ema.push((data[i] - prev) * multiplier + prev);
                }
            }
        }

        ema
    }

    /// The values over `data`, every bar of which has a finite close and volume.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();

        if data.len() < 2 {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate raw force index: (Close - PrevClose) * Volume
        let mut raw_force = Vec::with_capacity(data.len());
        raw_force.push(f64::NAN); // First bar has no previous

        for i in 1..data.len() {
            let force = (data[i].close - data[i - 1].close) * data[i].volume;
            raw_force.push(force);
        }

        // Apply EMA smoothing
        let smoothed = Self::calculate_ema(&raw_force, self.period);

        // Store results
        for value in smoothed {
            if value.is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push(IndicatorValue::Single(value));
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for ForceIndex {
    fn default() -> Self {
        Self::new(13)
    }
}

impl Indicator for ForceIndex {
    fn name(&self) -> &str {
        "Force Index"
    }

    fn desc(&self) -> &str {
        "Force Index - Measures force behind price movements"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.close.is_finite() && b.volume.is_finite(),
            |data| self.compute(data),
        );
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
        vec![format!("Force({})", self.period)]
    }
}
