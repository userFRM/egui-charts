use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Force Index
/// Measures the force behind price movements using price change and volume
/// Developed by Alexander Elder
#[derive(Clone)]
pub struct ForceIndex {
    /// EMA period for smoothing (typically 13)
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(Option<f64>, Smoothing)>,
    color: Color32,
    visible: bool,
}

impl ForceIndex {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: Color32::from_rgb(244, 67, 54), // Teal
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ForceIndex {
    fn default() -> Self {
        Self::new(13)
    }
}

impl Indicator for ForceIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Force Index"
    }

    fn desc(&self) -> &str {
        "Force Index - Measures force behind price movements"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() || !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                let raw = state
                    .0
                    .map_or(f64::NAN, |previous| (bar.close - previous) * bar.volume);
                state.0 = Some(bar.close);
                IndicatorValue::Single(state.1.next(
                    raw,
                    self.period,
                    2.0 / (self.period as f64 + 1.0),
                ))
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
