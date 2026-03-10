use crate::model::Bar;
/// Rainbow Moving Avg Oscillator
/// Measures distance of price from multiple moving avgs
/// Creates a "rainbow" effect with layered MAs
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct RainbowOscillator {
    /// Number of MA levels (typically 10)
    levels: usize,
    /// Base period for first SMA (typically 2)
    base_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl RainbowOscillator {
    pub fn new(levels: usize, base_period: usize) -> Self {
        Self {
            levels,
            base_period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().sum();
                result.push(sum / period as f64);
            }
        }

        result
    }
}

impl Default for RainbowOscillator {
    fn default() -> Self {
        Self::new(10, 2)
    }
}

impl Indicator for RainbowOscillator {
    fn name(&self) -> &str {
        "Rainbow"
    }

    fn desc(&self) -> &str {
        "Rainbow Oscillator - Multi-MA distance"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate recursive SMAs (each SMA is applied to the previous)
        let mut all_smas: Vec<Vec<f64>> = Vec::new();
        all_smas.push(Self::calculate_sma(&closes, self.base_period));

        for i in 1..self.levels {
            let prev_sma = &all_smas[i - 1];
            let smoothed = Self::calculate_sma(prev_sma, self.base_period);
            all_smas.push(smoothed);
        }

        // Calculate the oscillator
        // Rainbow Oscillator = 100 * (Close - Avg of all SMAs) / Avg of all SMAs
        for i in 0..data.len() {
            let mut sum = 0.0;
            let mut count = 0;
            let mut any_nan = false;

            for sma in &all_smas {
                if sma[i].is_nan() {
                    any_nan = true;
                    break;
                }
                sum += sma[i];
                count += 1;
            }

            if any_nan || count == 0 {
                self.values.push(IndicatorValue::None);
            } else {
                let avg_sma = sum / count as f64;
                if avg_sma == 0.0 {
                    self.values.push(IndicatorValue::None);
                } else {
                    let osc = 100.0 * (closes[i] - avg_sma) / avg_sma;
                    self.values.push(IndicatorValue::Single(osc));
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
        vec![format!("Rainbow({},{})", self.levels, self.base_period)]
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
    fn test_rainbow_calculation() {
        let mut rainbow = RainbowOscillator::new(5, 2);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        rainbow.calculate(&data);

        assert_eq!(rainbow.values.len(), 20);

        // Should have valid values
        let valid_cnt = rainbow
            .values
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
