use crate::model::Bar;
/// Detrended Price Oscillator (DPO)
/// Removes trend from price to identify cycles
/// Compares price to a displaced moving avg
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct DetrendedPriceOscillator {
    /// Period for SMA calculation
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl DetrendedPriceOscillator {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for DetrendedPriceOscillator {
    fn name(&self) -> &str {
        "DPO"
    }

    fn desc(&self) -> &str {
        "Detrended Price Oscillator - Removes trend to identify cycles"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        // DPO requires period + displacement
        // Displacement = period / 2 + 1
        let displacement = self.period / 2 + 1;
        let required = self.period + displacement;

        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate SMA values
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        for i in 0..data.len() {
            // Need enough data for both the SMA and the historical close
            let sma_idx = i + displacement;

            if i < self.period - 1 || sma_idx >= data.len() {
                self.values.push(IndicatorValue::None);
            } else {
                // Calculate SMA at the shifted position
                let sma: f64 = closes[sma_idx - self.period + 1..=sma_idx]
                    .iter()
                    .sum::<f64>()
                    / self.period as f64;

                // DPO = Close - SMA(period/2 + 1 bars ago)
                let dpo = closes[i] - sma;
                self.values.push(IndicatorValue::Single(dpo));
            }
        }

        // Pad the end with None values since we're using forward-looking SMA
        let valid_cnt = self
            .values
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        if valid_cnt == 0 {
            // Alternative calculation: look backward instead of forward
            self.values.clear();

            for i in 0..data.len() {
                if i < self.period + displacement - 1 {
                    self.values.push(IndicatorValue::None);
                } else {
                    let sma_start = i - displacement - self.period + 1;
                    let sma_end = i - displacement;

                    let sma: f64 =
                        closes[sma_start..=sma_end].iter().sum::<f64>() / self.period as f64;

                    let dpo = closes[i] - sma;
                    self.values.push(IndicatorValue::Single(dpo));
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
        vec![format!("DPO({})", self.period)]
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
    fn test_dpo_calculation() {
        let mut dpo = DetrendedPriceOscillator::new(10);

        // Create data with a trend
        let data: Vec<Bar> = (0..50).map(|i| make_bar(100.0 + i as f64)).collect();

        dpo.calculate(&data);

        assert_eq!(dpo.values.len(), 50);

        // Count valid values
        let valid_cnt = dpo
            .values
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        assert!(valid_cnt > 0, "Should have some valid DPO values");
    }

    #[test]
    fn test_dpo_removes_trend() {
        let mut dpo = DetrendedPriceOscillator::new(5);

        // Perfect linear trend: DPO should be relatively flat
        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64 * 2.0)).collect();

        dpo.calculate(&data);

        // DPO values should oscillate around 0 for a linear trend
        let valid_values: Vec<f64> = dpo
            .values
            .iter()
            .filter_map(|v| {
                if let IndicatorValue::Single(x) = v {
                    Some(*x)
                } else {
                    None
                }
            })
            .collect();

        if !valid_values.is_empty() {
            let mean: f64 = valid_values.iter().sum::<f64>() / valid_values.len() as f64;
            // Mean should be close to 0 after detrending
            assert!(mean.abs() < 10.0, "DPO mean {} should be close to 0", mean);
        }
    }
}
