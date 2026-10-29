use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// VIDYA - Variable Index Dynamic Avg
/// Adaptive moving avg that adjusts based on volatility
/// Uses Chande Momentum Oscillator for volatility measurement
#[derive(Clone)]
pub struct VIDYA {
    /// Period for calculation
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(std::collections::VecDeque<f64>, Option<f64>)>,
    color: Color32,
    visible: bool,
}

impl VIDYA {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
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

impl Default for VIDYA {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for VIDYA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "VIDYA"
    }

    fn desc(&self) -> &str {
        "Variable Index Dynamic Avg - Volatility-adjusted MA"
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
                let previous = state.1.get_or_insert(bar.close);
                state.0.push_back(bar.close);
                if state.0.len() <= self.period {
                    return IndicatorValue::None;
                }
                while state.0.len() - 1 > self.period {
                    state.0.pop_front();
                }
                let (mut up, mut down) = (0.0, 0.0);
                for i in 1..state.0.len() {
                    let change = state.0[i] - state.0[i - 1];
                    if change > 0.0 {
                        up += change;
                    } else {
                        down += (-change).abs();
                    }
                }
                let vi = if up + down != 0.0 {
                    (up - down).abs() / (up + down)
                } else {
                    0.0
                };
                let sc = 2.0 / (self.period as f64 + 1.0);
                *previous = sc * vi * bar.close + (1.0 - sc * vi) * *previous;
                IndicatorValue::Single(*previous)
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
        vec![format!("VIDYA({})", self.period)]
    }
}

/// Zero Lag EMA
/// Reduces lag in EMA by using error correction
#[derive(Clone)]
pub struct ZLEMA {
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(std::collections::VecDeque<f64>, usize, Smoothing)>,
    color: Color32,
    visible: bool,
}

impl ZLEMA {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
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

impl Default for ZLEMA {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ZLEMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "ZLEMA"
    }

    fn desc(&self) -> &str {
        "Zero Lag EMA - Reduced lag moving avg"
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
                let lag = (self.period - 1) / 2;
                state.0.push_back(bar.close);
                let delagged = if state.0.len() > lag {
                    bar.close + (bar.close - state.0[state.0.len() - 1 - lag])
                } else {
                    bar.close
                };
                if state.0.len() > lag {
                    state.0.pop_front();
                }
                let ema = state.2.next(delagged, 1, 2.0 / (self.period as f64 + 1.0));
                state.1 += 1;
                if state.1 < self.period {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(ema)
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
        vec![format!("ZLEMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_bar;

    #[test]
    fn test_vidya() {
        let mut vidya = VIDYA::new(10);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        vidya.calculate(&data);

        assert_eq!(vidya.values.len(), 20);
    }

    #[test]
    fn test_zlema() {
        let mut zlema = ZLEMA::new(10);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        zlema.calculate(&data);

        assert_eq!(zlema.values.len(), 20);
    }
}
