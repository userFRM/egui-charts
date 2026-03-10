use crate::model::Bar;
/// Elder Impulse System
/// Combines EMA slope and MACD histogram for trend signals
/// Green = bullish, Red = bearish, Blue = neutral
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Signal state for Elder Impulse
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImpulseSignal {
    /// EMA rising, MACD histogram rising
    Bullish,
    /// EMA falling, MACD histogram falling
    Bearish,
    /// Mixed signals
    Neutral,
}

#[derive(Clone)]
pub struct ElderImpulseSystem {
    /// EMA period (typically 13)
    ema_period: usize,
    /// MACD fast period
    macd_fast: usize,
    /// MACD slow period
    macd_slow: usize,
    /// MACD signal period
    macd_signal: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Bullish, Bearish, Neutral]
    colors: Vec<Color32>,
    visible: bool,
}

impl ElderImpulseSystem {
    pub fn new(ema_period: usize, macd_fast: usize, macd_slow: usize, macd_signal: usize) -> Self {
        Self {
            ema_period,
            macd_fast,
            macd_slow,
            macd_signal,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Bullish
                DESIGN_TOKENS.semantic.extended.error,   // Red - Bearish
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Neutral
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
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

impl Default for ElderImpulseSystem {
    fn default() -> Self {
        Self::new(13, 12, 26, 9)
    }
}

impl Indicator for ElderImpulseSystem {
    fn name(&self) -> &str {
        "Impulse"
    }

    fn desc(&self) -> &str {
        "Elder Impulse System - Trend direction with momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.macd_slow.max(self.ema_period) + self.macd_signal;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate EMA for slope
        let ema = Self::ema(&closes, self.ema_period);

        // Calculate MACD
        let ema_fast = Self::ema(&closes, self.macd_fast);
        let ema_slow = Self::ema(&closes, self.macd_slow);

        let mut macd: Vec<f64> = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            macd.push(ema_fast[i] - ema_slow[i]);
        }

        let signal = Self::ema(&macd, self.macd_signal);

        let mut histogram: Vec<f64> = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            histogram.push(macd[i] - signal[i]);
        }

        for i in 0..data.len() {
            if i < min_period - 1 || i == 0 {
                self.values.push(IndicatorValue::None);
            } else {
                let ema_rising = ema[i] > ema[i - 1];
                let hist_rising = histogram[i] > histogram[i - 1];

                // Signal encoding: 1.0 = bullish, -1.0 = bearish, 0.0 = neutral
                let signal_val = if ema_rising && hist_rising {
                    1.0 // Bullish
                } else if !ema_rising && !hist_rising {
                    -1.0 // Bearish
                } else {
                    0.0 // Neutral
                };

                self.values.push(IndicatorValue::Single(signal_val));
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
        if colors.len() >= 3 {
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
        vec!["Impulse".to_string()]
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
    fn test_elder_impulse() {
        let mut impulse = ElderImpulseSystem::new(5, 6, 12, 5);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        impulse.calculate(&data);

        assert_eq!(impulse.values.len(), 30);
    }
}
