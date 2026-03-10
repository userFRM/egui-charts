use crate::model::Bar;
/// Schaff Trend Cycle (STC)
/// Combines MACD with stochastic to create a faster, more accurate oscillator
/// Oscillates between 0 and 100
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct SchaffTrendCycle {
    /// MACD short period (typically 23)
    macd_short: usize,
    /// MACD long period (typically 50)
    macd_long: usize,
    /// Stochastic cycle period (typically 10)
    cycle: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl SchaffTrendCycle {
    pub fn new() -> Self {
        Self {
            macd_short: 23,
            macd_long: 50,
            cycle: 10,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan,
            visible: true,
        }
    }

    pub fn with_periods(mut self, short: usize, long: usize, cycle: usize) -> Self {
        self.macd_short = short;
        self.macd_long = long;
        self.cycle = cycle;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // First value is SMA
        let first_sma: f64 = data.iter().take(period).sum::<f64>() / period as f64;

        for i in 0..data.len() {
            if i < period - 1 {
                ema.push(f64::NAN);
            } else if i == period - 1 {
                ema.push(first_sma);
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

    fn stochastic(values: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(values.len());

        for i in 0..values.len() {
            if i < period - 1 || values[i].is_nan() {
                result.push(f64::NAN);
                continue;
            }

            let window = &values[i + 1 - period..=i];
            let valid: Vec<f64> = window.iter().filter(|&&x| !x.is_nan()).copied().collect();

            if valid.is_empty() {
                result.push(f64::NAN);
                continue;
            }

            let high = valid.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let low = valid.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let range = high - low;

            if range == 0.0 {
                result.push(50.0); // Middle of range
            } else {
                let stoch = (values[i] - low) / range * 100.0;
                result.push(stoch);
            }
        }

        result
    }

    fn smooth_stochastic(values: &[f64], factor: f64) -> Vec<f64> {
        let mut result = Vec::with_capacity(values.len());
        let mut prev = 0.0;

        for &value in values {
            if value.is_nan() {
                result.push(f64::NAN);
            } else {
                let smoothed = prev + factor * (value - prev);
                result.push(smoothed);
                prev = smoothed;
            }
        }

        result
    }
}

impl Default for SchaffTrendCycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for SchaffTrendCycle {
    fn name(&self) -> &str {
        "STC"
    }

    fn desc(&self) -> &str {
        "Schaff Trend Cycle - Fast trend oscillator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.macd_long + self.cycle {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate MACD line (short EMA - long EMA)
        let short_ema = Self::calculate_ema(&closes, self.macd_short);
        let long_ema = Self::calculate_ema(&closes, self.macd_long);

        let mut macd = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if short_ema[i].is_nan() || long_ema[i].is_nan() {
                macd.push(f64::NAN);
            } else {
                macd.push(short_ema[i] - long_ema[i]);
            }
        }

        // First Stochastic of MACD
        let stoch1 = Self::stochastic(&macd, self.cycle);
        let smooth1 = Self::smooth_stochastic(&stoch1, 0.5);

        // Second Stochastic of smoothed values
        let stoch2 = Self::stochastic(&smooth1, self.cycle);
        let stc = Self::smooth_stochastic(&stoch2, 0.5);

        // Store results
        for value in stc {
            if value.is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                // Clamp to 0-100 range
                let clamped = value.clamp(0.0, 100.0);
                self.values.push(IndicatorValue::Single(clamped));
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
        vec![format!(
            "STC({},{},{})",
            self.macd_short, self.macd_long, self.cycle
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
    fn test_stc_calculation() {
        let mut stc = SchaffTrendCycle::new().with_periods(5, 10, 3);

        let data: Vec<Bar> = (0..50).map(|i| make_bar(100.0 + i as f64)).collect();

        stc.calculate(&data);

        assert_eq!(stc.values.len(), 50);

        // Check that valid values are between 0 and 100
        for value in &stc.values {
            if let IndicatorValue::Single(v) = value {
                assert!(*v >= 0.0 && *v <= 100.0, "STC should be 0-100, got {}", v);
            }
        }
    }

    #[test]
    fn test_stc_uptrend() {
        let mut stc = SchaffTrendCycle::new().with_periods(3, 6, 3);

        // Strong uptrend
        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64 * 2.0)).collect();

        stc.calculate(&data);

        // In uptrend, STC should be high (close to 100)
        if let IndicatorValue::Single(v) = stc.values.last().unwrap() {
            assert!(*v > 50.0, "STC should be high in uptrend, got {}", v);
        }
    }
}
