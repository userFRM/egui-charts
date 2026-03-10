use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Ratio Study - shows close/open ratio for each bar (intrabar momentum).
/// Formula: (close / open - 1.0) * 100.0
#[derive(Clone)]
pub struct RatioStudy {
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl RatioStudy {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.deep_orange],
            visible: true,
        }
    }
}

impl Default for RatioStudy {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for RatioStudy {
    fn name(&self) -> &str {
        "Ratio"
    }

    fn desc(&self) -> &str {
        "Ratio Study - Close/Open percentage change per bar"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            if bar.open.abs() < 1e-15 {
                self.values.push(IndicatorValue::Single(0.0));
            } else {
                let ratio = (bar.close / bar.open - 1.0) * 100.0;
                self.values.push(IndicatorValue::Single(ratio));
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
        vec!["Ratio".to_string()]
    }
}
