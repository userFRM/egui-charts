use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Chande Momentum Oscillator (CMO)
/// Measures momentum on both up and down days
/// Ranges from -100 to +100
#[derive(Clone)]
pub struct ChandeMomentumOscillator {
    /// Period for calculation (typically 14 or 20)
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl ChandeMomentumOscillator {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// Set the plot color.
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

impl ChandeMomentumOscillator {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // First value is None
        values.push(IndicatorValue::None);

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
                values.push(IndicatorValue::None);
            } else {
                let sum_up: f64 = gains[i + 1 - self.period..=i].iter().sum();
                let sum_down: f64 = losses[i + 1 - self.period..=i].iter().sum();

                let cmo = if sum_up + sum_down != 0.0 {
                    100.0 * (sum_up - sum_down) / (sum_up + sum_down)
                } else {
                    0.0
                };

                values.push(IndicatorValue::Single(cmo));
            }
        }
        values
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            |_data, from| from.saturating_sub(self.period),
            |data| self.compute(data),
        );
        self.values = values;
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
        vec![format!("CMO({})", self.period)]
    }
}
