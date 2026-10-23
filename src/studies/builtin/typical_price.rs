use crate::model::Bar;
/// Typical Price (HLC/3)
/// Simple avg of high, low, close
/// Also known as pivot point
/// Note: Same formula as [`HLC3`](super::HLC3) in average_price; kept for
/// backwards compatibility under the "TP" indicator name.
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct TypicalPrice {
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl TypicalPrice {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
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

    fn values(&self) -> &IndicatorValues {
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
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl WeightedClose {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
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

    fn values(&self) -> &IndicatorValues {
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
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl MedianPrice {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
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

    fn values(&self) -> &IndicatorValues {
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
