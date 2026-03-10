use crate::model::Bar;
/// Fisher Transform
/// Converts prices into a Gaussian normal distribution
/// Creates sharper turning points for identifying reversals
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct FisherTransform {
    /// Lookback period
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Fisher, Signal/Trigger]
    colors: Vec<Color32>,
    visible: bool,
}

impl FisherTransform {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Fisher
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Indicator for FisherTransform {
    fn name(&self) -> &str {
        "Fisher Transform"
    }

    fn desc(&self) -> &str {
        "Fisher Transform - Identifies price reversals with Gaussian distribution"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate HL2 (typical price simplified)
        let hl2: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();

        let mut fisher_values = Vec::with_capacity(data.len());
        let mut prev_val = 0.0;
        let mut prev_fisher = 0.0;

        for i in 0..data.len() {
            if i < self.period - 1 {
                fisher_values.push((f64::NAN, f64::NAN));
                continue;
            }

            // Find highest and lowest in period
            let window = &hl2[i + 1 - self.period..=i];
            let highest = window.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let lowest = window.iter().fold(f64::INFINITY, |a, &b| a.min(b));

            // Normalize to range [-1, 1]
            let range = highest - lowest;
            let normalized = if range > 0.0 {
                2.0 * ((hl2[i] - lowest) / range - 0.5)
            } else {
                0.0
            };

            // Smooth the normalized value
            let value = 0.66 * normalized + 0.67 * prev_val;
            // Clamp to avoid NaN from atanh
            let clamped = value.clamp(-0.999, 0.999);

            // Apply Fisher Transform
            let fisher = 0.5 * ((1.0 + clamped) / (1.0 - clamped)).ln();
            let signal = prev_fisher;

            prev_val = value;
            prev_fisher = fisher;

            fisher_values.push((fisher, signal));
        }

        // Store results
        for (fisher, signal) in fisher_values {
            if fisher.is_nan() {
                self.values.push(IndicatorValue::None);
            } else if signal.is_nan() || signal == 0.0 {
                self.values.push(IndicatorValue::Single(fisher));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![fisher, signal]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!("Fisher({})", self.period), "Signal".to_string()]
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
    fn test_fisher_transform() {
        let mut ft = FisherTransform::new(5);

        let data: Vec<Bar> = (0..20)
            .map(|i| make_bar(100.0 + i as f64, 98.0 + i as f64, 99.0 + i as f64))
            .collect();

        ft.calculate(&data);

        assert_eq!(ft.values.len(), 20);

        // First few should be None
        assert!(matches!(ft.values[0], IndicatorValue::None));

        // Later values should be present
        let last = &ft.values[19];
        assert!(matches!(
            last,
            IndicatorValue::Multiple(_) | IndicatorValue::Single(_)
        ));
    }

    #[test]
    fn test_fisher_bounds() {
        // Fisher transform should produce reasonable values
        let mut ft = FisherTransform::new(3);

        let data: Vec<Bar> = (0..10)
            .map(|i| make_bar(100.0 + i as f64 * 2.0, 98.0 + i as f64, 99.0 + i as f64))
            .collect();

        ft.calculate(&data);

        for value in &ft.values {
            let v = match value {
                IndicatorValue::Single(v) => *v,
                IndicatorValue::Multiple(vals) => vals[0],
                IndicatorValue::None => continue,
            };
            // Fisher values are typically between -2 and 2
            assert!(v.abs() < 10.0, "Fisher value {} out of expected range", v);
        }
    }
}
