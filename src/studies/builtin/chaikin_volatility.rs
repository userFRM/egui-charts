use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
/// Chaikin Volatility
/// Measures the rate of change of the trading range
/// High values indicate increased volatility
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ChaikinVolatility {
    /// EMA period (typically 10)
    ema_period: usize,
    /// ROC period (typically 10)
    roc_period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl ChaikinVolatility {
    pub fn new(ema_period: usize, roc_period: usize) -> Self {
        Self {
            ema_period: ema_period.max(1),
            roc_period: roc_period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_orange,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        let min_period = self.ema_period.saturating_add(self.roc_period);
        if data.len() < min_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate H-L range
        let hl_range: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();

        // Calculate EMA of H-L range
        let hl_ema = helpers::ema(&hl_range, self.ema_period);

        // Calculate Rate of Change of EMA
        for i in 0..data.len() {
            if i < min_period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let current = hl_ema[i];
                let past = hl_ema[i - self.roc_period];

                let cv = if past != 0.0 {
                    (current - past) / past * 100.0
                } else {
                    0.0
                };

                values.push(IndicatorValue::Single(cv));
            }
        }

        values
    }
}

impl Default for ChaikinVolatility {
    fn default() -> Self {
        Self::new(10, 10)
    }
}

impl Indicator for ChaikinVolatility {
    crate::studies::inputs::study_inputs!(self;
        ema_period: length("EMA period", 1, usize::MAX),
        roc_period: length("Roc period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "CV"
    }

    fn desc(&self) -> &str {
        "Chaikin Volatility - Rate of change of trading range"
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
        false
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
        vec![format!("CV({},{})", self.ema_period, self.roc_period)]
    }
}
