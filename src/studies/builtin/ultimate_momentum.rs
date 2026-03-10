use crate::model::Bar;
/// Ultimate Momentum Oscillator
/// Custom composite momentum indicator
/// Combines multiple timeframes for confirmation
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct UltimateMomentum {
    /// Short period (typically 5)
    short_period: usize,
    /// Medium period (typically 10)
    medium_period: usize,
    /// Long period (typically 20)
    long_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl UltimateMomentum {
    pub fn new(short: usize, medium: usize, long: usize) -> Self {
        Self {
            short_period: short,
            medium_period: medium,
            long_period: long,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_roc(closes: &[f64], period: usize, index: usize) -> Option<f64> {
        if index < period || closes[index - period] == 0.0 {
            None
        } else {
            Some((closes[index] - closes[index - period]) / closes[index - period] * 100.0)
        }
    }
}

impl Default for UltimateMomentum {
    fn default() -> Self {
        Self::new(5, 10, 20)
    }
}

impl Indicator for UltimateMomentum {
    fn name(&self) -> &str {
        "UltMom"
    }

    fn desc(&self) -> &str {
        "Ultimate Momentum - Multi-timeframe momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.long_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        for i in 0..data.len() {
            let roc_short = Self::calculate_roc(&closes, self.short_period, i);
            let roc_medium = Self::calculate_roc(&closes, self.medium_period, i);
            let roc_long = Self::calculate_roc(&closes, self.long_period, i);

            match (roc_short, roc_medium, roc_long) {
                (Some(s), Some(m), Some(l)) => {
                    // Weighted combination: more weight to longer periods
                    let momentum = (s * 1.0 + m * 2.0 + l * 3.0) / 6.0;
                    self.values.push(IndicatorValue::Single(momentum));
                }
                _ => {
                    self.values.push(IndicatorValue::None);
                }
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!(
            "UltMom({},{},{})",
            self.short_period, self.medium_period, self.long_period
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_ultimate_momentum() {
        let mut um = UltimateMomentum::new(3, 5, 10);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        um.calculate(&data);

        assert_eq!(um.values.len(), 30);

        // In uptrend, momentum should be positive
        if let IndicatorValue::Single(v) = um.values.last().unwrap() {
            assert!(*v > 0.0, "Momentum should be positive in uptrend");
        }
    }
}
