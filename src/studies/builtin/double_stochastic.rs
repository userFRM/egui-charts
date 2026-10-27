use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Double Stochastic
/// Applies the stochastic formula twice for smoother signals
/// More responsive to overbought/oversold conditions
#[derive(Clone)]
pub struct DoubleStochastic {
    /// Period for stochastic calculation
    period: usize,
    /// Smoothing period
    smooth: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl DoubleStochastic {
    /// Create with `period`, `smooth` in bars, each clamped to at least one.
    pub fn new(period: usize, smooth: usize) -> Self {
        Self {
            period: period.max(1),
            smooth: smooth.max(1),
            values: IndicatorValues::new(),
            color: palette::ORANGE, // Orange
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for DoubleStochastic {
    fn default() -> Self {
        Self::new(10, 3)
    }
}

impl DoubleStochastic {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let min_period = self.period.saturating_add(self.smooth.saturating_mul(2));
        if data.len() < min_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
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
        let smooth_stoch1 = helpers::sma(&stoch1, self.smooth);

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
        let ds = helpers::sma(&stoch2, self.smooth);

        for i in 0..data.len() {
            if i < min_period - 1 || ds[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push(IndicatorValue::Single(ds[i]));
            }
        }

        values
    }
}

impl Indicator for DoubleStochastic {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smooth: length("Smooth", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "DS"
    }

    fn desc(&self) -> &str {
        "Double Stochastic - Double-smoothed stochastic oscillator"
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
            usable_behind(
                self.period.saturating_add(self.smooth).saturating_mul(2),
                |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
            ),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
                    |data| self.compute(data),
                )
            },
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
            colors: None,
            payload: None,
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
