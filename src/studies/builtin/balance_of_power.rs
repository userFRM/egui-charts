use crate::model::Bar;
/// Balance of Power (BoP)
/// Measures the strength of buyers vs sellers
/// BoP = (Close - Open) / (High - Low)
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct BalanceOfPower {
    /// SMA smoothing period (0 = no smoothing)
    smooth_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl BalanceOfPower {
    pub fn new(smooth_period: usize) -> Self {
        Self {
            smooth_period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        if period == 0 || period > data.len() {
            return data.to_vec();
        }

        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(data[i]); // Use raw value before enough data
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().sum();
                result.push(sum / period as f64);
            }
        }

        result
    }
}

impl Indicator for BalanceOfPower {
    fn name(&self) -> &str {
        "BoP"
    }

    fn desc(&self) -> &str {
        "Balance of Power - Measures buyer/seller strength"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Calculate raw BoP values
        let mut raw_bop = Vec::with_capacity(data.len());

        for bar in data {
            let range = bar.high - bar.low;
            if range == 0.0 {
                raw_bop.push(0.0);
            } else {
                let bop = (bar.close - bar.open) / range;
                raw_bop.push(bop);
            }
        }

        // Apply smoothing if configured
        let smoothed = if self.smooth_period > 1 {
            Self::calculate_sma(&raw_bop, self.smooth_period)
        } else {
            raw_bop
        };

        // Store results
        for value in smoothed {
            self.values.push(IndicatorValue::Single(value));
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
        if self.smooth_period > 1 {
            vec![format!("BoP({})", self.smooth_period)]
        } else {
            vec!["BoP".to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_bop_bullish() {
        let mut bop = BalanceOfPower::new(0);

        // Bullish bar: close at high
        let data = vec![make_bar(100.0, 110.0, 95.0, 110.0)];

        bop.calculate(&data);

        if let IndicatorValue::Single(v) = bop.values[0] {
            // (110 - 100) / (110 - 95) = 10/15 = 0.667
            assert!((v - 0.667).abs() < 0.01);
        }
    }

    #[test]
    fn test_bop_bearish() {
        let mut bop = BalanceOfPower::new(0);

        // Bearish bar: close at low
        let data = vec![make_bar(100.0, 105.0, 90.0, 90.0)];

        bop.calculate(&data);

        if let IndicatorValue::Single(v) = bop.values[0] {
            // (90 - 100) / (105 - 90) = -10/15 = -0.667
            assert!((v + 0.667).abs() < 0.01);
        }
    }

    #[test]
    fn test_bop_neutral() {
        let mut bop = BalanceOfPower::new(0);

        // Neutral bar: close = open
        let data = vec![make_bar(100.0, 110.0, 90.0, 100.0)];

        bop.calculate(&data);

        if let IndicatorValue::Single(v) = bop.values[0] {
            assert!((v - 0.0).abs() < 0.01);
        }
    }
}
