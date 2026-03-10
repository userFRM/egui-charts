use crate::model::Bar;
/// Market Facilitation Index (MFI / BW MFI)
/// Developed by Bill Williams
/// Measures price movement per unit of volume
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct MarketFacilitationIndex {
    /// Divisor for volume (typically 1 for crypto, 1000000 for stocks)
    divisor: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl MarketFacilitationIndex {
    pub fn new() -> Self {
        Self {
            divisor: 1.0,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_divisor(mut self, divisor: f64) -> Self {
        self.divisor = divisor;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for MarketFacilitationIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for MarketFacilitationIndex {
    fn name(&self) -> &str {
        "MFI BW"
    }

    fn desc(&self) -> &str {
        "Market Facilitation Index - Price movement per volume"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let range = bar.high - bar.low;
            let volume = bar.volume / self.divisor;

            if volume == 0.0 {
                self.values.push(IndicatorValue::None);
            } else {
                let mfi = range / volume;
                self.values.push(IndicatorValue::Single(mfi));
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
        vec!["MFI".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close: (high + low) / 2.0,
            volume,
        }
    }

    #[test]
    fn test_mfi_calculation() {
        let mut mfi = MarketFacilitationIndex::new();

        let data = vec![
            make_bar(102.0, 98.0, 1000.0),  // Range=4, MFI=0.004
            make_bar(105.0, 100.0, 2000.0), // Range=5, MFI=0.0025
        ];

        mfi.calculate(&data);

        assert_eq!(mfi.values.len(), 2);

        if let IndicatorValue::Single(v) = mfi.values[0] {
            assert!((v - 0.004).abs() < 0.0001);
        }
    }

    #[test]
    fn test_mfi_zero_volume() {
        let mut mfi = MarketFacilitationIndex::new();

        let data = vec![make_bar(102.0, 98.0, 0.0)];

        mfi.calculate(&data);

        assert!(matches!(mfi.values[0], IndicatorValue::None));
    }
}
