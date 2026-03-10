use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Advance/Decline Line - cumulative sum of +1 (up close) / -1 (down close).
#[derive(Clone)]
pub struct AdvanceDeclineLine {
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl AdvanceDeclineLine {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.info],
            visible: true,
        }
    }
}

impl Default for AdvanceDeclineLine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AdvanceDeclineLine {
    fn name(&self) -> &str {
        "A/D Line"
    }

    fn desc(&self) -> &str {
        "Advance/Decline Line - Cumulative up/down close count"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // First bar: no previous close, start at 0
        let mut cumulative = 0.0_f64;
        self.values.push(IndicatorValue::Single(cumulative));

        for i in 1..data.len() {
            if data[i].close > data[i - 1].close {
                cumulative += 1.0;
            } else if data[i].close < data[i - 1].close {
                cumulative -= 1.0;
            }
            self.values.push(IndicatorValue::Single(cumulative));
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
        vec!["A/D Line".to_string()]
    }
}
