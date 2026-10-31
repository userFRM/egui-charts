use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Smoothed Moving Average (Wilder's Smoothing)
#[derive(Clone)]
pub struct SmoothedMA {
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(usize, f64)>,
    color: Color32,
    visible: bool,
}

impl SmoothedMA {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::DEEP_PURPLE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for SmoothedMA {
    fn default() -> Self {
        Self::new(7)
    }
}

impl Indicator for SmoothedMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "SMMA"
    }

    fn desc(&self) -> &str {
        "Smoothed Moving Average - Wilder's smoothing method"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                if state.0 == 0 {
                    state.1 = -0.0;
                }
                if state.0 < self.period {
                    state.0 += 1;
                    state.1 += bar.close;
                    if state.0 < self.period {
                        return IndicatorValue::None;
                    }
                    state.1 /= self.period as f64;
                } else {
                    state.1 =
                        (state.1 * (self.period as f64 - 1.0) + bar.close) / self.period as f64;
                }
                IndicatorValue::Single(state.1)
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
        true
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
        vec![format!("SMMA({})", self.period)]
    }
}
