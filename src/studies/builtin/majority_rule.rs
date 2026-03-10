use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Majority Rule - counts up vs down closes over period.
/// Outputs +1 if majority up, -1 if majority down, 0 if tie.
#[derive(Clone)]
pub struct MajorityRule {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl MajorityRule {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.cyan],
            visible: true,
        }
    }
}

impl Default for MajorityRule {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for MajorityRule {
    fn name(&self) -> &str {
        "MajRule"
    }

    fn desc(&self) -> &str {
        "Majority Rule - +1 if majority up closes, -1 if majority down, 0 if tie"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // First bar has no previous close to compare
        self.values.push(IndicatorValue::None);

        for i in 1..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let mut up_count: i32 = 0;
            let mut down_count: i32 = 0;

            for idx in start..=i {
                if idx == 0 {
                    continue;
                }
                if data[idx].close > data[idx - 1].close {
                    up_count += 1;
                } else if data[idx].close < data[idx - 1].close {
                    down_count += 1;
                }
            }

            let result = if up_count > down_count {
                1.0
            } else if down_count > up_count {
                -1.0
            } else {
                0.0
            };

            self.values.push(IndicatorValue::Single(result));
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
        vec![format!("MajRule({})", self.period)]
    }
}
