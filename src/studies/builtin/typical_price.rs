use crate::model::Bar;
/// Typical Price (HLC/3)
/// Simple avg of high, low, close
/// Also known as pivot point
/// Note: Same formula as [`HLC3`](super::HLC3) in average_price; kept for
/// backwards compatibility under the "TP" indicator name.
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct TypicalPrice {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl TypicalPrice {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for TypicalPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for TypicalPrice {
    fn name(&self) -> &str {
        "TP"
    }

    fn desc(&self) -> &str {
        "Typical Price - (High + Low + Close) / 3"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let tp = (bar.high + bar.low + bar.close) / 3.0;
            self.values.push(IndicatorValue::Single(tp));
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
        vec!["TP".to_string()]
    }
}

/// Weighted Close
#[derive(Clone)]
pub struct WeightedClose {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl WeightedClose {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for WeightedClose {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for WeightedClose {
    fn name(&self) -> &str {
        "WC"
    }

    fn desc(&self) -> &str {
        "Weighted Close - (High + Low + 2*Close) / 4"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let wc = (bar.high + bar.low + 2.0 * bar.close) / 4.0;
            self.values.push(IndicatorValue::Single(wc));
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
        vec!["WC".to_string()]
    }
}

/// Median Price (HL/2)
/// Note: Same formula as [`HL2`](super::HL2) in average_price; kept for
/// backwards compatibility under the "MP" indicator name.
#[derive(Clone)]
pub struct MedianPrice {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl MedianPrice {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for MedianPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for MedianPrice {
    fn name(&self) -> &str {
        "MP"
    }

    fn desc(&self) -> &str {
        "Median Price - (High + Low) / 2"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let mp = (bar.high + bar.low) / 2.0;
            self.values.push(IndicatorValue::Single(mp));
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
        vec!["MP".to_string()]
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
    fn test_typical_price() {
        let mut tp = TypicalPrice::new();
        let data = vec![make_bar(105.0, 95.0, 100.0)];

        tp.calculate(&data);

        if let IndicatorValue::Single(v) = tp.values[0] {
            // TP = (105 + 95 + 100) / 3 = 100
            assert!((v - 100.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_weighted_close() {
        let mut wc = WeightedClose::new();
        let data = vec![make_bar(105.0, 95.0, 100.0)];

        wc.calculate(&data);

        if let IndicatorValue::Single(v) = wc.values[0] {
            // WC = (105 + 95 + 200) / 4 = 100
            assert!((v - 100.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_median_price() {
        let mut mp = MedianPrice::new();
        let data = vec![make_bar(105.0, 95.0, 100.0)];

        mp.calculate(&data);

        if let IndicatorValue::Single(v) = mp.values[0] {
            // MP = (105 + 95) / 2 = 100
            assert!((v - 100.0).abs() < 0.01);
        }
    }
}
