use crate::model::Bar;
/// True Strength Index (TSI)
/// Double-smoothed momentum indicator
/// Shows trend direction and overbought/oversold conditions
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct TrueStrengthIndex {
    /// Long period (typically 25)
    long_period: usize,
    /// Short period (typically 13)
    short_period: usize,
    /// Signal line period (typically 7)
    signal_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [TSI, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl TrueStrengthIndex {
    pub fn new(long_period: usize, short_period: usize, signal_period: usize) -> Self {
        Self {
            long_period,
            short_period,
            signal_period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,        // Blue - TSI
                DESIGN_TOKENS.semantic.extended.deep_orange, // Deep Orange - Signal
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

impl Default for TrueStrengthIndex {
    fn default() -> Self {
        Self::new(25, 13, 7)
    }
}

impl Indicator for TrueStrengthIndex {
    fn name(&self) -> &str {
        "TSI"
    }

    fn desc(&self) -> &str {
        "True Strength Index - Double-smoothed momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.long_period + self.short_period + 1;
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate price changes
        let mut pc: Vec<f64> = vec![0.0]; // First change is 0
        for i in 1..data.len() {
            pc.push(data[i].close - data[i - 1].close);
        }

        let abs_pc: Vec<f64> = pc.iter().map(|x| x.abs()).collect();

        // Double smooth the price change
        let pc_smooth1 = Self::ema(&pc, self.long_period);
        let pc_smooth2 = Self::ema(&pc_smooth1, self.short_period);

        // Double smooth the absolute price change
        let abs_pc_smooth1 = Self::ema(&abs_pc, self.long_period);
        let abs_pc_smooth2 = Self::ema(&abs_pc_smooth1, self.short_period);

        // Calculate TSI values
        let mut tsi_values: Vec<f64> = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if abs_pc_smooth2[i] != 0.0 {
                tsi_values.push(100.0 * pc_smooth2[i] / abs_pc_smooth2[i]);
            } else {
                tsi_values.push(0.0);
            }
        }

        // Calculate signal line
        let signal = Self::ema(&tsi_values, self.signal_period);

        // Build output
        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![tsi_values[i], signal[i]]));
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
            format!("TSI({},{})", self.long_period, self.short_period),
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
    fn test_tsi() {
        let mut tsi = TrueStrengthIndex::new(10, 5, 3);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        tsi.calculate(&data);

        assert_eq!(tsi.values.len(), 30);
    }
}
