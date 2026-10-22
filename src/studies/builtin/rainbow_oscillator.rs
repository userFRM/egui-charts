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
            levels: levels.max(1),
            base_period: base_period.max(1),
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

        // Sum the recursive SMAs (each applied to the previous) bar by bar, keeping only
        // the latest level, so memory stays a few series whatever the level count.
        let mut level = Self::calculate_sma(&closes, self.base_period);
        let mut sums = level.clone();
        let mut missing: Vec<bool> = level.iter().map(|v| v.is_nan()).collect();
        let mut level_cnt = 1;
        // With a base period of 1 every level equals the closes, and once every bar misses a
        // level no deeper level can add a value: either way deeper levels change nothing.
        while level_cnt < self.levels && self.base_period > 1 && missing.contains(&false) {
            level = Self::calculate_sma(&level, self.base_period);
            for ((sum, m), v) in sums.iter_mut().zip(missing.iter_mut()).zip(&level) {
                *sum += v;
                *m |= v.is_nan();
            }
            level_cnt += 1;
        }

        // Calculate the oscillator
        // Rainbow Oscillator = 100 * (Close - Avg of all SMAs) / Avg of all SMAs
        for i in 0..data.len() {
            if missing[i] {
                self.values.push(IndicatorValue::None);
            } else {
                let avg_sma = sums[i] / level_cnt as f64;
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
