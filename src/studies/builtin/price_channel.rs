use crate::model::Bar;
/// Price Channel
/// Simple highest high and lowest low channel
/// Also known as "N-bar high/low"
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct PriceChannel {
    /// Lookback period
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Upper, Lower, Middle]
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceChannel {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Upper
                DESIGN_TOKENS.semantic.extended.error,   // Red - Lower
                DESIGN_TOKENS.semantic.extended.gray,    // Gray - Middle (neutral)
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for PriceChannel {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for PriceChannel {
    fn name(&self) -> &str {
        "Price Channel"
    }

    fn desc(&self) -> &str {
        "Price Channel - Highest high / Lowest low bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let highest = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                let middle = (highest + lowest) / 2.0;

                self.values
                    .push(IndicatorValue::Multiple(vec![highest, lowest, middle]));
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
            format!("Upper({})", self.period),
            format!("Lower({})", self.period),
            "Middle".to_string(),
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
    fn test_price_channel() {
        let mut pc = PriceChannel::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 103.0),
            make_bar(106.0, 101.0, 105.0),
            make_bar(108.0, 103.0, 107.0),
            make_bar(110.0, 105.0, 109.0),
        ];

        pc.calculate(&data);

        assert_eq!(pc.values.len(), 5);

        // Last value should be highest=110, lowest=98, middle=104
        if let IndicatorValue::Multiple(v) = pc.values.last().unwrap() {
            assert_eq!(v.len(), 3);
            assert!((v[0] - 110.0).abs() < 0.01); // Upper
            assert!((v[1] - 98.0).abs() < 0.01); // Lower
            assert!((v[2] - 104.0).abs() < 0.01); // Middle
        }
    }
}
