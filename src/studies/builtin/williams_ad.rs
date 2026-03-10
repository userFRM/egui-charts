use crate::model::Bar;
/// Williams Accumulation/Distribution
/// Developed by Larry Williams
/// Measures buying and selling pressure
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct WilliamsAD {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl WilliamsAD {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for WilliamsAD {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for WilliamsAD {
    fn name(&self) -> &str {
        "WAD"
    }

    fn desc(&self) -> &str {
        "Williams Accumulation/Distribution - Buying/selling pressure"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        let mut cumulative = 0.0;
        self.values.push(IndicatorValue::Single(0.0)); // First bar

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            let ad = if current.close > prev.close {
                // Accumulation: Close - True Low
                let true_low = current.low.min(prev.close);
                current.close - true_low
            } else if current.close < prev.close {
                // Distribution: Close - True High
                let true_high = current.high.max(prev.close);
                current.close - true_high
            } else {
                0.0
            };

            cumulative += ad;
            self.values.push(IndicatorValue::Single(cumulative));
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
        vec!["WAD".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn test_williams_ad() {
        let mut wad = WilliamsAD::new();

        let data = vec![
            make_bar(100.0, 102.0, 98.0, 101.0, 1000.0),
            make_bar(101.0, 104.0, 100.0, 103.0, 1200.0), // Up
            make_bar(103.0, 105.0, 101.0, 102.0, 800.0),  // Down
        ];

        wad.calculate(&data);

        assert_eq!(wad.values.len(), 3);
    }
}
