use crate::model::Bar;
/// Chaikin Volatility
/// Measures the rate of change of the trading range
/// High values indicate increased volatility
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ChaikinVolatility {
    /// EMA period (typically 10)
    ema_period: usize,
    /// ROC period (typically 10)
    roc_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ChaikinVolatility {
    pub fn new(ema_period: usize, roc_period: usize) -> Self {
        Self {
            ema_period,
            roc_period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_orange,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn ema(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());
        if data.is_empty() || period == 0 {
            return result;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        result.push(data[0]);

        for i in 1..data.len() {
            let ema = (data[i] - result[i - 1]) * multiplier + result[i - 1];
            result.push(ema);
        }

        result
    }
}

impl Default for ChaikinVolatility {
    fn default() -> Self {
        Self::new(10, 10)
    }
}

impl Indicator for ChaikinVolatility {
    fn name(&self) -> &str {
        "CV"
    }

    fn desc(&self) -> &str {
        "Chaikin Volatility - Rate of change of trading range"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.ema_period + self.roc_period;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate H-L range
        let hl_range: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();

        // Calculate EMA of H-L range
        let hl_ema = Self::ema(&hl_range, self.ema_period);

        // Calculate Rate of Change of EMA
        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let current = hl_ema[i];
                let past = hl_ema[i - self.roc_period];

                let cv = if past != 0.0 {
                    (current - past) / past * 100.0
                } else {
                    0.0
                };

                self.values.push(IndicatorValue::Single(cv));
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
        vec![format!("CV({},{})", self.ema_period, self.roc_period)]
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
    fn test_chaikin_volatility() {
        let mut cv = ChaikinVolatility::new(5, 5);

        let data: Vec<Bar> = (0..20)
            .map(|i| {
                let base = 100.0 + i as f64;
                let range = 2.0 + (i as f64 * 0.5); // Increasing range
                make_bar(base + range, base - range, base)
            })
            .collect();

        cv.calculate(&data);

        assert_eq!(cv.values.len(), 20);

        // With increasing range, CV should be positive
        if let IndicatorValue::Single(v) = cv.values.last().unwrap() {
            assert!(*v > 0.0, "CV should be positive with expanding range");
        }
    }
}
