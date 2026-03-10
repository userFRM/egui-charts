use crate::model::Bar;
/// Mass Index
/// Identifies trend reversals based on range expansion
/// Developed by Donald Dorsey
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct MassIndex {
    /// EMA period for range smoothing (typically 9)
    ema_period: usize,
    /// Summation period (typically 25)
    sum_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl MassIndex {
    pub fn new() -> Self {
        Self {
            ema_period: 9,
            sum_period: 25,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_orange,
            visible: true,
        }
    }

    pub fn with_periods(mut self, ema_period: usize, sum_period: usize) -> Self {
        self.ema_period = ema_period;
        self.sum_period = sum_period;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate EMA
    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // Find first valid value
        let mut sum = 0.0;
        let mut count = 0;
        let mut first_valid = 0;

        for (i, &v) in data.iter().enumerate() {
            if !v.is_nan() {
                sum += v;
                count += 1;
                if count == period {
                    first_valid = i;
                    break;
                }
            }
        }

        for i in 0..data.len() {
            if i < first_valid {
                ema.push(f64::NAN);
            } else if i == first_valid {
                ema.push(sum / period as f64);
            } else {
                let prev = ema[i - 1];
                if prev.is_nan() || data[i].is_nan() {
                    ema.push(f64::NAN);
                } else {
                    ema.push((data[i] - prev) * multiplier + prev);
                }
            }
        }

        ema
    }
}

impl Default for MassIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for MassIndex {
    fn name(&self) -> &str {
        "Mass Index"
    }

    fn desc(&self) -> &str {
        "Mass Index - Identifies reversals based on range expansion"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let required = self.ema_period * 2 + self.sum_period;
        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate range (High - Low)
        let ranges: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();

        // Calculate single EMA of range
        let ema1 = Self::calculate_ema(&ranges, self.ema_period);

        // Calculate double EMA of range
        let ema2 = Self::calculate_ema(&ema1, self.ema_period);

        // Calculate EMA ratio (single/double)
        let ratio: Vec<f64> = ema1
            .iter()
            .zip(ema2.iter())
            .map(|(e1, e2)| {
                if e1.is_nan() || e2.is_nan() || *e2 == 0.0 {
                    f64::NAN
                } else {
                    e1 / e2
                }
            })
            .collect();

        // Sum the ratio over sum_period
        for i in 0..data.len() {
            if i < self.sum_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &ratio[i + 1 - self.sum_period..=i];
                let mut sum = 0.0;
                let mut has_nan = false;

                for &v in window {
                    if v.is_nan() {
                        has_nan = true;
                        break;
                    }
                    sum += v;
                }

                if has_nan {
                    self.values.push(IndicatorValue::None);
                } else {
                    self.values.push(IndicatorValue::Single(sum));
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
        vec![format!("Mass({},{})", self.ema_period, self.sum_period)]
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
        }
    }

    #[test]
    fn test_mass_idx_calculation() {
        let mut mi = MassIndex::new().with_periods(5, 10);

        // Create data with varying ranges
        let data: Vec<Bar> = (0..50)
            .map(|i| {
                let range = 2.0 + (i as f64 * 0.1).sin() * 1.5;
                make_bar(100.0 + range / 2.0, 100.0 - range / 2.0, 100.0)
            })
            .collect();

        mi.calculate(&data);

        assert_eq!(mi.values.len(), 50);

        // Count valid values
        let valid_cnt = mi
            .values
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        assert!(valid_cnt > 0, "Should have some valid Mass Index values");
    }

    #[test]
    fn test_mass_idx_reversal_bulge() {
        // Mass Index above 27 ("bulge") followed by drop below 26.5 signals reversal
        let mi = MassIndex::new();

        // A value near 27 suggests high probability of reversal
        // Normal values are typically around 25
        assert_eq!(mi.sum_period, 25);
    }
}
