use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

/// Spread Study - shows the spread between high and low of each bar (high - low).
#[derive(Clone)]
pub struct SpreadStudy {
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl SpreadStudy {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            colors: vec![Color32::from_rgb(0, 150, 136)],
            visible: true,
        }
    }
}

impl Default for SpreadStudy {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for SpreadStudy {
    fn name(&self) -> &str {
        "Spread"
    }

    fn desc(&self) -> &str {
        "Spread Study - High minus Low for each bar"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let spread = bar.high - bar.low;
            self.values.push(IndicatorValue::Single(spread));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
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
        vec!["Spread".to_string()]
    }
}
