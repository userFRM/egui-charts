use crate::model::Bar;
/// Chandelier Exit
/// Trailing stop-loss indicator based on ATR
/// Developed by Chuck LeBeau
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ChandelierExit {
    /// ATR period (typically 22)
    period: usize,
    /// ATR multiplier (typically 3.0)
    multiplier: f64,
    values: Vec<IndicatorValue>,
    /// Colors: [Long exit, Short exit]
    colors: Vec<Color32>,
    visible: bool,
}

impl ChandelierExit {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period,
            multiplier,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Long exit
                DESIGN_TOKENS.semantic.extended.error,   // Red - Short exit
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for ChandelierExit {
    fn default() -> Self {
        Self::new(22, 3.0)
    }
}

impl Indicator for ChandelierExit {
    fn name(&self) -> &str {
        "Chandelier"
    }

    fn desc(&self) -> &str {
        "Chandelier Exit - ATR-based trailing stop"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate True Range
        let mut tr = Vec::with_capacity(data.len());
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            let high_low = data[i].high - data[i].low;
            let high_close = (data[i].high - data[i - 1].close).abs();
            let low_close = (data[i].low - data[i - 1].close).abs();
            tr.push(high_low.max(high_close).max(low_close));
        }

        // Calculate ATR using SMA
        let mut atr = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i < self.period - 1 {
                atr.push(f64::NAN);
            } else {
                let sum: f64 = tr[i + 1 - self.period..=i].iter().sum();
                atr.push(sum / self.period as f64);
            }
        }

        // Calculate Chandelier Exit
        for i in 0..data.len() {
            if i < self.period - 1 || atr[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                // Find highest high and lowest low in period
                let window = &data[i + 1 - self.period..=i];
                let highest = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                // Long exit = Highest High - ATR * multiplier
                let long_exit = highest - atr[i] * self.multiplier;
                // Short exit = Lowest Low + ATR * multiplier
                let short_exit = lowest + atr[i] * self.multiplier;

                self.values
                    .push(IndicatorValue::Multiple(vec![long_exit, short_exit]));
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
        true // Overlay on price chart
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
            format!("Long Exit({},{})", self.period, self.multiplier),
            format!("Short Exit({},{})", self.period, self.multiplier),
        ]
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
    fn test_chandelier_calculation() {
        let mut ce = ChandelierExit::new(5, 2.0);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(112.0, 107.0, 111.0),
        ];

        ce.calculate(&data);

        assert_eq!(ce.values.len(), 6);

        // Should have long and short exit values
        if let IndicatorValue::Multiple(v) = ce.values.last().unwrap() {
            assert_eq!(v.len(), 2);
            // Long exit should be below highest high
            assert!(v[0] < 112.0);
            // Short exit should be above lowest low
            assert!(v[1] > 98.0);
        }
    }
}
