use crate::model::Bar;
/// Double Stochastic
/// Applies the stochastic formula twice for smoother signals
/// More responsive to overbought/oversold conditions
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct DoubleStochastic {
    /// Period for stochastic calculation
    period: usize,
    /// Smoothing period
    smooth: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl DoubleStochastic {
    pub fn new(period: usize, smooth: usize) -> Self {
        Self {
            period,
            smooth,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn sma(data: &[f64], period: usize) -> Vec<f64> {
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

impl Default for DoubleStochastic {
    fn default() -> Self {
        Self::new(10, 3)
    }
}

impl Indicator for DoubleStochastic {
    fn name(&self) -> &str {
        "DS"
    }

    fn desc(&self) -> &str {
        "Double Stochastic - Double-smoothed stochastic oscillator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.period + self.smooth * 2;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // First stochastic
        let mut stoch1: Vec<f64> = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < self.period - 1 {
                stoch1.push(f64::NAN);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let high = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                let k = if high != low {
                    (data[i].close - low) / (high - low) * 100.0
                } else {
                    50.0
                };
                stoch1.push(k);
            }
        }

        // Smooth the first stochastic
        let smooth_stoch1 = Self::sma(&stoch1, self.smooth);

        // Second stochastic on the smoothed values
        let mut stoch2: Vec<f64> = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            let start = if i >= self.period - 1 {
                i + 1 - self.period
            } else {
                0
            };

            if i < self.period + self.smooth - 1 {
                stoch2.push(f64::NAN);
            } else {
                let window = &smooth_stoch1[start..=i];
                let valid_values: Vec<f64> =
                    window.iter().filter(|v| v.is_finite()).cloned().collect();

                if valid_values.len() >= self.period {
                    let high = valid_values
                        .iter()
                        .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                    let low = valid_values.iter().fold(f64::INFINITY, |a, &b| a.min(b));

                    let current = smooth_stoch1[i];
                    let k = if high != low && current.is_finite() {
                        (current - low) / (high - low) * 100.0
                    } else {
                        50.0
                    };
                    stoch2.push(k);
                } else {
                    stoch2.push(f64::NAN);
                }
            }
        }

        // Final smoothing
        let ds = Self::sma(&stoch2, self.smooth);

        for i in 0..data.len() {
            if i < min_period - 1 || ds[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(ds[i]));
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
        vec![format!("DS({},{})", self.period, self.smooth)]
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
    fn test_double_stochastic() {
        let mut ds = DoubleStochastic::new(5, 3);

        let data: Vec<Bar> = (0..30)
            .map(|i| {
                let base = 100.0 + i as f64;
                make_bar(base + 2.0, base - 2.0, base + 1.0)
            })
            .collect();

        ds.calculate(&data);

        assert_eq!(ds.values.len(), 30);
    }
}
