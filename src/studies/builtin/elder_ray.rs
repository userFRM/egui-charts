use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Elder Ray Index (Bull Power / Bear Power)
/// Measures buying and selling pressure
/// Developed by Alexander Elder
#[derive(Clone)]
pub struct ElderRay {
    /// EMA period (typically 13)
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<Smoothing>,
    /// Colors: [Bull Power (green), Bear Power (red)]
    colors: Vec<Color32>,
    visible: bool,
}

impl ElderRay {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::GREEN, // Green - Bull Power
                palette::RED,   // Red - Bear Power
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ElderRay {
    fn default() -> Self {
        Self::new(13)
    }
}

impl Indicator for ElderRay {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "Elder Ray"
    }

    fn desc(&self) -> &str {
        "Elder Ray - Bull and Bear Power indicators"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                let ema = state.next(bar.close, self.period, 2.0 / (self.period as f64 + 1.0));
                if ema.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![bar.high - ema, bar.low - ema])
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
