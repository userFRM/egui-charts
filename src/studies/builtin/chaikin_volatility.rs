use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Chaikin Volatility
/// Measures the rate of change of the trading range
/// High values indicate increased volatility
#[derive(Clone)]
pub struct ChaikinVolatility {
    /// EMA period (typically 10)
    ema_period: usize,
    /// ROC period (typically 10)
    roc_period: usize,
    values: IndicatorValues,
    stepper: Stepper<(usize, Smoothing, std::collections::VecDeque<f64>)>,
    color: Color32,
    visible: bool,
}

impl ChaikinVolatility {
    /// Create with `ema_period`, `roc_period` in bars, each clamped to at least one.
    pub fn new(ema_period: usize, roc_period: usize) -> Self {
        Self {
            ema_period: ema_period.max(1),
            roc_period: roc_period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: Color32::from_rgb(171, 71, 188),
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
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

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "CV"
    }

    fn desc(&self) -> &str {
        "Chaikin Volatility - Rate of change of trading range"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() {
                    return IndicatorValue::None;
                }
                state.0 += 1;
                let ema = state
                    .1
                    .next(bar.high - bar.low, 1, 2.0 / (self.ema_period as f64 + 1.0));
                state.2.push_back(ema);
                let past = if state.2.len() > self.roc_period {
                    state.2.pop_front().unwrap()
                } else {
                    f64::NAN
                };
                if state.0 < self.ema_period.saturating_add(self.roc_period) {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(if past != 0.0 {
                        (ema - past) / past * 100.0
                    } else {
                        0.0
                    })
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
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
