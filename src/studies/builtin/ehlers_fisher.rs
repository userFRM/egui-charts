use super::over_finite_bars;
use crate::model::Bar;
/// Ehlers Fisher Transform (John Ehlers version)
/// Converts prices into a Gaussian normal distribution
/// More responsive than standard Fisher Transform
///
/// Related: [`FisherTransform`](super::FisherTransform) clamps the smoothed
/// value instead of the normalized price.
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct EhlersFisher {
    /// Lookback period (typically 10)
    period: usize,
    values: IndicatorValues,
    /// Colors: [Fisher, Trigger]
    colors: Vec<Color32>,
    visible: bool,
}

impl EhlersFisher {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Fisher
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Trigger
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate midprice (HL2)
        let hl2: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();

        let mut fisher_values = Vec::with_capacity(data.len());
        let mut value1 = 0.0;
        let mut fisher = 0.0;

        for i in 0..data.len() {
            if i < self.period - 1 {
                fisher_values.push((f64::NAN, f64::NAN));
                continue;
            }

            // Find highest and lowest midprice in period
            let window = &hl2[i + 1 - self.period..=i];
            let highest = window.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let lowest = window.iter().fold(f64::INFINITY, |a, &b| a.min(b));

            let range = highest - lowest;

            // Normalize to range [-1, 1]
            let normalized = if range != 0.0 {
                2.0 * (hl2[i] - lowest) / range - 1.0
            } else {
                0.0
            };

            // Smoothing
            let new_value1 = 0.33 * normalized.clamp(-0.999, 0.999) + 0.67 * value1;

            // Fisher transform
            // The first value has no previous one to trigger on.
            let prev_fisher = if i + 1 == self.period {
                f64::NAN
            } else {
                fisher
            };
            fisher = 0.5 * ((1.0 + new_value1) / (1.0 - new_value1)).ln() + 0.5 * fisher;

            value1 = new_value1;
            fisher_values.push((fisher, prev_fisher));
        }

        // Store results
        for (fisher, trigger) in fisher_values {
            if fisher.is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[fisher, trigger]);
            }
        }
        values
    }
}

impl Default for EhlersFisher {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for EhlersFisher {
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
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
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
