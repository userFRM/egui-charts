use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Mass Index
/// Identifies trend reversals based on range expansion
/// Developed by Donald Dorsey
#[derive(Clone)]
pub struct MassIndex {
    /// EMA period for range smoothing (typically 9)
    ema_period: usize,
    /// Summation period (typically 25)
    sum_period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl MassIndex {
    /// Create with EMA period 9 and rolling-sum period 25.
    pub fn new() -> Self {
        Self {
            ema_period: 9,
            sum_period: 25,
            values: IndicatorValues::new(),
            color: palette::DEEP_ORANGE,
            visible: true,
        }
    }

    /// Set the EMA and rolling-sum periods in bars, each at least one.
    pub fn with_periods(mut self, ema_period: usize, sum_period: usize) -> Self {
        self.ema_period = ema_period.max(1);
        self.sum_period = sum_period.max(1);
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        let required = self
            .ema_period
            .saturating_mul(2)
            .saturating_add(self.sum_period);
        if data.len() < required {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate range (High - Low)
        let ranges: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();

        // Calculate single EMA of range
        let ema1 = helpers::ema_sma(&ranges, self.ema_period);

        // Calculate double EMA of range
        let ema2 = helpers::ema_sma(&ema1, self.ema_period);

        // Calculate EMA ratio (single/double)
        let ratio: Vec<f64> = ema1
            .iter()
            .zip(ema2.iter())
            .map(|(e1, e2)| {
                if e1.is_nan() || e2.is_nan() || *e2 == 0.0 {
                    f64::NAN
                } else {
                    e1 / e2
                }
            })
            .collect();

        // Sum the ratio over sum_period
        for i in 0..data.len() {
            if i < self.sum_period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let window = &ratio[i + 1 - self.sum_period..=i];
                let mut sum = 0.0;
                let mut has_nan = false;

                for &v in window {
                    if v.is_nan() {
                        has_nan = true;
                        break;
                    }
                    sum += v;
                }

                if has_nan {
                    values.push(IndicatorValue::None);
                } else {
                    values.push(IndicatorValue::Single(sum));
                }
            }
        }

        values
    }
}

impl Default for MassIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for MassIndex {
    crate::studies::inputs::study_inputs!(self;
        ema_period: length("EMA period", 1, usize::MAX),
        sum_period: length("Sum period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Mass Index"
    }

    fn desc(&self) -> &str {
        "Mass Index - Identifies reversals based on range expansion"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
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
        vec![format!("Mass({},{})", self.ema_period, self.sum_period)]
    }
}
