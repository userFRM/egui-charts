use crate::model::Bar;
/// Chande Momentum Oscillator (CMO)
/// Measures momentum on both up and down days
/// Ranges from -100 to +100
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ChandeMomentumOscillator {
    /// Period for calculation (typically 14 or 20)
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl ChandeMomentumOscillator {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for ChandeMomentumOscillator {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ChandeMomentumOscillator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "CMO"
    }

    fn desc(&self) -> &str {
        "Chande Momentum Oscillator - Measures momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // First value is None
        self.values.push(IndicatorValue::None);

        // Calculate daily changes
        let mut gains = Vec::with_capacity(data.len() - 1);
        let mut losses = Vec::with_capacity(data.len() - 1);

        for i in 1..data.len() {
            let change = data[i].close - data[i - 1].close;
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push((-change).abs());
            }
        }

        for i in 0..gains.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let sum_up: f64 = gains[i + 1 - self.period..=i].iter().sum();
                let sum_down: f64 = losses[i + 1 - self.period..=i].iter().sum();

                let cmo = if sum_up + sum_down != 0.0 {
                    100.0 * (sum_up - sum_down) / (sum_up + sum_down)
                } else {
                    0.0
                };

                self.values.push(IndicatorValue::Single(cmo));
            }
        }
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
        vec![format!("CMO({})", self.period)]
    }
}
