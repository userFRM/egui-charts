use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
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
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    index: usize,
    count: usize,
    ema: [Smoothing; 2],
    ratios: std::collections::VecDeque<f64>,
    pending: Vec<(usize, f64)>,
}

impl MassIndex {
    /// Create with EMA period 9 and rolling-sum period 25.
    pub fn new() -> Self {
        Self {
            ema_period: 9,
            sum_period: 25,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::LIGHT_BLUE,
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.index;
                state.index += 1;
                if !bar.high.is_finite() || !bar.low.is_finite() {
                    return IndicatorValue::None;
                }
                state.count += 1;
                let alpha = 2.0 / (self.ema_period as f64 + 1.0);
                let first = state.ema[0].next(bar.high - bar.low, self.ema_period, alpha);
                let second = state.ema[1].next(first, self.ema_period, alpha);
                let ratio = if first.is_nan() || second.is_nan() || second == 0.0 {
                    f64::NAN
                } else {
                    first / second
                };
                state.ratios.push_back(ratio);
                if state.ratios.len() > self.sum_period {
                    state.ratios.pop_front();
                }
                let mut sum = 0.0;
                if state.ratios.len() < self.sum_period {
                    return IndicatorValue::None;
                }
                for &value in &state.ratios {
                    if value.is_nan() {
                        return IndicatorValue::None;
                    }
                    sum += value;
                }
                let required = self
                    .ema_period
                    .saturating_mul(2)
                    .saturating_add(self.sum_period);
                if state.count < required {
                    if sum.is_finite() {
                        state.pending.push((index, sum));
                    }
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(sum)
                }
            });
        let required = self
            .ema_period
            .saturating_mul(2)
            .saturating_add(self.sum_period);
        for &(index, value) in &self.stepper.state().pending {
            self.values.set(
                index,
                if self.stepper.state().count < required {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(value)
                },
            );
        }
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
