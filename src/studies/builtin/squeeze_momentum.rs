use crate::model::Bar;
/// Squeeze Momentum Indicator
/// Combines Bollinger Bands and Keltner Channels to identify periods of low volatility
/// When BB is inside KC = "squeeze" (low volatility period)
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct SqueezeMomentum {
    /// Bollinger Bands period (typically 20)
    bb_period: usize,
    /// Bollinger Bands standard deviation multiplier (typically 2.0)
    bb_mult: f64,
    /// Keltner Channel period (typically 20)
    kc_period: usize,
    /// Keltner Channel ATR multiplier (typically 1.5)
    kc_mult: f64,
    /// Momentum period (typically 12)
    momentum_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Momentum positive, Momentum negative, Squeeze on, Squeeze off]
    colors: Vec<Color32>,
    visible: bool,
}

impl SqueezeMomentum {
    pub fn new() -> Self {
        Self {
            bb_period: 20,
            bb_mult: 2.0,
            kc_period: 20,
            kc_mult: 1.5,
            momentum_period: 12,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Momentum positive
                DESIGN_TOKENS.semantic.extended.error,   // Red - Momentum negative
                DESIGN_TOKENS.semantic.extended.disabled, // Gray - Squeeze on
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Squeeze off
            ],
            visible: true,
        }
    }

    pub fn with_bb_params(mut self, period: usize, mult: f64) -> Self {
        self.bb_period = period;
        self.bb_mult = mult;
        self
    }

    pub fn with_kc_params(mut self, period: usize, mult: f64) -> Self {
        self.kc_period = period;
        self.kc_mult = mult;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
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

    fn calculate_std_dev(data: &[f64], sma: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 || sma[i].is_nan() {
                result.push(f64::NAN);
            } else {
                let mean = sma[i];
                let variance: f64 = data[i + 1 - period..=i]
                    .iter()
                    .map(|&x| (x - mean).powi(2))
                    .sum::<f64>()
                    / period as f64;
                result.push(variance.sqrt());
            }
        }

        result
    }

    fn calculate_atr(data: &[Bar], period: usize) -> Vec<f64> {
        let mut tr = Vec::with_capacity(data.len());
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            let high_low = data[i].high - data[i].low;
            let high_close = (data[i].high - data[i - 1].close).abs();
            let low_close = (data[i].low - data[i - 1].close).abs();
            tr.push(high_low.max(high_close).max(low_close));
        }

        Self::calculate_sma(&tr, period)
    }

    fn linear_regression(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let window = &data[i + 1 - period..=i];
                let n = period as f64;

                let sum_x: f64 = (0..period).map(|x| x as f64).sum();
                let sum_y: f64 = window.iter().sum();
                let sum_xy: f64 = window.iter().enumerate().map(|(x, &y)| x as f64 * y).sum();
                let sum_xx: f64 = (0..period).map(|x| (x * x) as f64).sum();

                let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x * sum_x);
                let intercept = (sum_y - slope * sum_x) / n;

                // Value at the end of the regression line
                let value = intercept + slope * (period - 1) as f64;
                result.push(value);
            }
        }

        result
    }
}

impl Default for SqueezeMomentum {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for SqueezeMomentum {
    fn name(&self) -> &str {
        "Squeeze"
    }

    fn desc(&self) -> &str {
        "Squeeze Momentum - Identifies volatility breakouts"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let required = self.bb_period.max(self.kc_period).max(self.momentum_period);
        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate Bollinger Bands
        let bb_sma = Self::calculate_sma(&closes, self.bb_period);
        let bb_std = Self::calculate_std_dev(&closes, &bb_sma, self.bb_period);

        // Calculate Keltner Channels
        let kc_sma = Self::calculate_sma(&closes, self.kc_period);
        let atr = Self::calculate_atr(data, self.kc_period);

        // Calculate midline and deviations for squeeze calculation
        let hl2: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();
        let hl2_sma = Self::calculate_sma(&hl2, self.momentum_period);

        // Calculate momentum using linear regression
        let mut deviation = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if hl2_sma[i].is_nan() {
                deviation.push(f64::NAN);
            } else {
                deviation.push(closes[i] - hl2_sma[i]);
            }
        }
        let momentum = Self::linear_regression(&deviation, self.momentum_period);

        // Store results: [momentum, squeeze_state (1.0 = squeeze on, 0.0 = off)]
        for i in 0..data.len() {
            if bb_sma[i].is_nan()
                || bb_std[i].is_nan()
                || kc_sma[i].is_nan()
                || atr[i].is_nan()
                || momentum[i].is_nan()
            {
                self.values.push(IndicatorValue::None);
            } else {
                let bb_upper = bb_sma[i] + self.bb_mult * bb_std[i];
                let bb_lower = bb_sma[i] - self.bb_mult * bb_std[i];
                let kc_upper = kc_sma[i] + self.kc_mult * atr[i];
                let kc_lower = kc_sma[i] - self.kc_mult * atr[i];

                // Squeeze is ON when BB is inside KC
                let squeeze_on = bb_lower > kc_lower && bb_upper < kc_upper;

                self.values.push(IndicatorValue::Multiple(vec![
                    momentum[i],
                    if squeeze_on { 1.0 } else { 0.0 },
                ]));
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
        if colors.len() >= 4 {
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
        vec!["Momentum".to_string(), "Squeeze".to_string()]
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
    fn test_squeeze_calculation() {
        let mut squeeze = SqueezeMomentum::new();

        let data: Vec<Bar> = (0..50)
            .map(|i| make_bar(102.0 + i as f64, 98.0 + i as f64, 100.0 + i as f64))
            .collect();

        squeeze.calculate(&data);

        assert_eq!(squeeze.values.len(), 50);

        // Should have momentum and squeeze state
        if let IndicatorValue::Multiple(v) = squeeze.values.last().unwrap() {
            assert_eq!(v.len(), 2);
        }
    }
}
