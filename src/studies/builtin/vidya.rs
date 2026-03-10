use crate::model::Bar;
/// VIDYA - Variable Index Dynamic Avg
/// Adaptive moving avg that adjusts based on volatility
/// Uses Chande Momentum Oscillator for volatility measurement
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VIDYA {
    /// Period for calculation
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl VIDYA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple, // Deep Purple
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for VIDYA {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for VIDYA {
    fn name(&self) -> &str {
        "VIDYA"
    }

    fn desc(&self) -> &str {
        "Variable Index Dynamic Avg - Volatility-adjusted MA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Standard smoothing constant
        let sc = 2.0 / (self.period as f64 + 1.0);

        // Initialize VIDYA with first close
        let mut vidya = data[0].close;

        // Need to calculate CMO for each bar
        for i in 0..data.len() {
            if i < self.period {
                // Need enough data for CMO calculation
                self.values.push(IndicatorValue::None);
            } else {
                // Calculate CMO (Chande Momentum Oscillator) for volatility
                let mut sum_up = 0.0;
                let mut sum_down = 0.0;

                for j in i + 1 - self.period..=i {
                    if j > 0 {
                        let change = data[j].close - data[j - 1].close;
                        if change > 0.0 {
                            sum_up += change;
                        } else {
                            sum_down += (-change).abs();
                        }
                    }
                }

                let vi = if sum_up + sum_down != 0.0 {
                    (sum_up - sum_down).abs() / (sum_up + sum_down)
                } else {
                    0.0
                };

                // Calculate VIDYA
                vidya = sc * vi * data[i].close + (1.0 - sc * vi) * vidya;

                self.values.push(IndicatorValue::Single(vidya));
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
        true
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
        vec![format!("VIDYA({})", self.period)]
    }
}

/// Zero Lag EMA
/// Reduces lag in EMA by using error correction
#[derive(Clone)]
pub struct ZLEMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ZLEMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
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

impl Default for ZLEMA {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ZLEMA {
    fn name(&self) -> &str {
        "ZLEMA"
    }

    fn desc(&self) -> &str {
        "Zero Lag EMA - Reduced lag moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let lag = (self.period - 1) / 2;

        // Create de-lagged data
        let mut delagged: Vec<f64> = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i >= lag {
                let correction = data[i].close - data[i - lag].close;
                delagged.push(data[i].close + correction);
            } else {
                delagged.push(data[i].close);
            }
        }

        // Apply EMA to de-lagged data
        let zlema = Self::ema(&delagged, self.period);

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(zlema[i]));
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
        true
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
        vec![format!("ZLEMA({})", self.period)]
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
    fn test_vidya() {
        let mut vidya = VIDYA::new(10);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        vidya.calculate(&data);

        assert_eq!(vidya.values.len(), 20);
    }

    #[test]
    fn test_zlema() {
        let mut zlema = ZLEMA::new(10);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        zlema.calculate(&data);

        assert_eq!(zlema.values.len(), 20);
    }
}
