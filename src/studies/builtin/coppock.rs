use crate::model::Bar;
/// Coppock Curve
/// A momentum indicator originally designed to identify buying opportunities in stock indices
/// Uses a weighted moving avg of the sum of two ROC periods
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct CoppockCurve {
    /// Long ROC period (typically 14)
    long_roc: usize,
    /// Short ROC period (typically 11)
    short_roc: usize,
    /// WMA period (typically 10)
    wma_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl CoppockCurve {
    pub fn new() -> Self {
        Self {
            long_roc: 14,
            short_roc: 11,
            wma_period: 10,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    pub fn with_periods(mut self, long_roc: usize, short_roc: usize, wma: usize) -> Self {
        self.long_roc = long_roc;
        self.short_roc = short_roc;
        self.wma_period = wma;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate Rate of Change
    fn calculate_roc(prices: &[f64], period: usize) -> Vec<f64> {
        let mut roc = Vec::with_capacity(prices.len());

        for i in 0..prices.len() {
            if i < period || prices[i - period] == 0.0 {
                roc.push(f64::NAN);
            } else {
                let change = (prices[i] - prices[i - period]) / prices[i - period] * 100.0;
                roc.push(change);
            }
        }

        roc
    }

    /// Calculate Weighted Moving Avg
    fn calculate_wma(data: &[f64], period: usize) -> Vec<f64> {
        let mut wma = Vec::with_capacity(data.len());
        let weight_sum: f64 = (1..=period).map(|i| i as f64).sum();

        for i in 0..data.len() {
            if i < period - 1 {
                wma.push(f64::NAN);
            } else {
                let mut weighted_sum = 0.0;
                let mut has_nan = false;

                for j in 0..period {
                    let value = data[i + 1 - period + j];
                    if value.is_nan() {
                        has_nan = true;
                        break;
                    }
                    weighted_sum += value * (j + 1) as f64;
                }

                if has_nan {
                    wma.push(f64::NAN);
                } else {
                    wma.push(weighted_sum / weight_sum);
                }
            }
        }

        wma
    }
}

impl Default for CoppockCurve {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for CoppockCurve {
    fn name(&self) -> &str {
        "Coppock"
    }

    fn desc(&self) -> &str {
        "Coppock Curve - Long-term momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let required = self.long_roc.max(self.short_roc) + self.wma_period;
        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate both ROCs
        let long_roc = Self::calculate_roc(&closes, self.long_roc);
        let short_roc = Self::calculate_roc(&closes, self.short_roc);

        // Sum of ROCs
        let roc_sum: Vec<f64> = long_roc
            .iter()
            .zip(short_roc.iter())
            .map(|(l, s)| {
                if l.is_nan() || s.is_nan() {
                    f64::NAN
                } else {
                    l + s
                }
            })
            .collect();

        // Apply WMA to the sum
        let coppock = Self::calculate_wma(&roc_sum, self.wma_period);

        // Store results
        for value in coppock {
            if value.is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(value));
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
        vec![format!(
            "Coppock({},{},{})",
            self.long_roc, self.short_roc, self.wma_period
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
    fn test_roc_calculation() {
        let prices = vec![100.0, 105.0, 110.0, 115.0, 120.0];
        let roc = CoppockCurve::calculate_roc(&prices, 2);

        assert_eq!(roc.len(), 5);
        assert!(roc[0].is_nan());
        assert!(roc[1].is_nan());
        // ROC at index 2: (110 - 100) / 100 * 100 = 10%
        assert!((roc[2] - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_wma_calculation() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let wma = CoppockCurve::calculate_wma(&data, 3);

        assert_eq!(wma.len(), 5);
        assert!(wma[0].is_nan());
        assert!(wma[1].is_nan());
        // WMA at index 2: (1*1 + 2*2 + 3*3) / (1+2+3) = (1 + 4 + 9) / 6 = 14/6 = 2.333
        assert!((wma[2] - 2.333).abs() < 0.01);
    }

    #[test]
    fn test_coppock_calculation() {
        let mut coppock = CoppockCurve::new().with_periods(5, 3, 3);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        coppock.calculate(&data);

        assert_eq!(coppock.values.len(), 30);
    }
}
