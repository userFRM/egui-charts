use crate::model::Bar;
/// Price Oscillator (PPO - Percentage Price Oscillator)
/// Measures the percentage difference between two EMAs
/// Similar to MACD but expressed as percentage
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct PriceOscillator {
    /// Short EMA period (typically 12)
    short_period: usize,
    /// Long EMA period (typically 26)
    long_period: usize,
    /// Signal line EMA period (typically 9)
    signal_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [PPO line, Signal line]
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceOscillator {
    pub fn new(short_period: usize, long_period: usize, signal_period: usize) -> Self {
        Self {
            short_period,
            long_period,
            signal_period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - PPO
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
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
                if prev.is_nan() {
                    ema.push(f64::NAN);
                } else {
                    ema.push((data[i] - prev) * multiplier + prev);
                }
            }
        }

        ema
    }
}

impl Indicator for PriceOscillator {
    fn name(&self) -> &str {
        "PPO"
    }

    fn desc(&self) -> &str {
        "Percentage Price Oscillator - Percentage difference between EMAs"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.long_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Extract closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate short and long EMAs
        let short_ema = Self::calculate_ema(&closes, self.short_period);
        let long_ema = Self::calculate_ema(&closes, self.long_period);

        // Calculate PPO line
        let mut ppo_line = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if short_ema[i].is_nan() || long_ema[i].is_nan() || long_ema[i] == 0.0 {
                ppo_line.push(f64::NAN);
            } else {
                let ppo = (short_ema[i] - long_ema[i]) / long_ema[i] * 100.0;
                ppo_line.push(ppo);
            }
        }

        // Calculate signal line (EMA of PPO)
        let valid_ppo: Vec<f64> = ppo_line
            .iter()
            .map(|&v| if v.is_nan() { 0.0 } else { v })
            .collect();
        let signal_line = Self::calculate_ema(&valid_ppo, self.signal_period);

        // Store results
        for i in 0..data.len() {
            if ppo_line[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else if signal_line[i].is_nan() || i < self.long_period + self.signal_period - 2 {
                self.values.push(IndicatorValue::Single(ppo_line[i]));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![ppo_line[i], signal_line[i]]));
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
        vec![
            format!("PPO({},{})", self.short_period, self.long_period),
            format!("Signal({})", self.signal_period),
        ]
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
    fn test_ppo_calculation() {
        let mut ppo = PriceOscillator::new(5, 10, 3);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        ppo.calculate(&data);

        assert_eq!(ppo.values.len(), 30);

        // Should have valid values after long period warmup
        let valid_cnt = ppo
            .values
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }

    #[test]
    fn test_ppo_uptrend() {
        let mut ppo = PriceOscillator::new(3, 6, 2);

        // Strong uptrend
        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64 * 2.0)).collect();

        ppo.calculate(&data);

        // PPO should be positive in uptrend
        if let Some(last_val) = ppo.values.last() {
            let ppo_val = match last_val {
                IndicatorValue::Multiple(vals) => vals[0],
                IndicatorValue::Single(val) => *val,
                IndicatorValue::None => panic!("Expected non-None PPO value"),
            };
            assert!(ppo_val > 0.0, "PPO should be positive in uptrend");
        }
    }
}
