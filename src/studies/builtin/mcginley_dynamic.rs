use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// McGinley Dynamic: an average that speeds up when price falls away from it and slows down when
/// price rises above it. `MD = MD[1] + (close - MD[1]) / (N x (close / MD[1])^4)` with N the
/// period, starting at the first close. While MD is zero, or the divisor is zero, MD keeps its
/// value.
#[derive(Clone)]
pub struct McGinleyDynamic {
    /// Period (typically 10 or 14)
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<Option<f64>>,
    color: Color32,
    visible: bool,
}

impl McGinleyDynamic {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::LIGHT_BLUE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for McGinleyDynamic {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for McGinleyDynamic {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "MD"
    }

    fn desc(&self) -> &str {
        "McGinley Dynamic - Self-adjusting moving avg"
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
                let md = state.get_or_insert(bar.close);
                if *md != 0.0 {
                    let ratio = bar.close / *md;
                    let divisor = self.period as f64 * ratio.powi(4);
                    if divisor != 0.0 {
                        *md += (bar.close - *md) / divisor;
                    }
                }
                IndicatorValue::Single(*md)
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
        true // Overlay on price chart
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
        vec![format!("MD({})", self.period)]
    }
}
