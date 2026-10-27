use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Ehlers Fisher Transform (John Ehlers version)
/// Converts prices into a Gaussian normal distribution
/// More responsive than standard Fisher Transform
///
/// Related: [`FisherTransform`](super::FisherTransform) clamps the smoothed
/// value instead of the normalized price.
#[derive(Clone)]
pub struct EhlersFisher {
    /// Lookback period (typically 10)
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(std::collections::VecDeque<f64>, f64, f64, bool)>,
    /// Colors: [Fisher, Trigger]
    colors: Vec<Color32>,
    visible: bool,
}

impl EhlersFisher {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // Blue - Fisher
                palette::ORANGE, // Orange - Trigger
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

impl Default for EhlersFisher {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for EhlersFisher {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HL2,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Ehlers Fisher"
    }

    fn desc(&self) -> &str {
        "Ehlers Fisher Transform - Enhanced price distribution"
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
                let price = (bar.high + bar.low) / 2.0;
                state.0.push_back(price);
                if state.0.len() > self.period {
                    state.0.pop_front();
                }
                if state.0.len() < self.period {
                    return IndicatorValue::None;
                }
                let high = state.0.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let low = state.0.iter().copied().fold(f64::INFINITY, f64::min);
                let range = high - low;
                let normalized = if range != 0.0 {
                    2.0 * (price - low) / range - 1.0
                } else {
                    0.0
                };
                let value = 0.33 * normalized.clamp(-0.999, 0.999) + 0.67 * state.1;
                let signal = if state.3 { state.2 } else { f64::NAN };
                state.2 = 0.5 * ((1.0 + value) / (1.0 - value)).ln() + 0.5 * state.2;
                state.1 = value;
                state.3 = true;
                if state.2.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![state.2, signal])
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
        vec![format!("Fisher({})", self.period), "Trigger".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_ehlers_fisher_calculation() {
        let mut ef = EhlersFisher::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(112.0, 107.0, 111.0),
            make_bar(114.0, 109.0, 113.0),
        ];

        ef.calculate(&data);

        assert_eq!(ef.values.len(), 7);

        // Should have Fisher values
        let valid_cnt = ef
            .values
            .to_rows()
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
