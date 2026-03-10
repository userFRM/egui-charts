use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// 52-Week High/Low Percentage
/// (close - lowest_low) / (highest_high - lowest_low) * 100
#[derive(Clone)]
pub struct Week52HighLow {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLow {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.pink],
            visible: true,
        }
    }
}

impl Default for Week52HighLow {
    fn default() -> Self {
        Self::new(252)
    }
}

impl Indicator for Week52HighLow {
    fn name(&self) -> &str {
        "52W HL%"
    }

    fn desc(&self) -> &str {
        "52-Week High/Low Percentage - Position within period range"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let window = &data[start..=i];

            let highest = window
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

            let range = highest - lowest;
            if range.abs() < 1e-15 {
                self.values.push(IndicatorValue::Single(50.0));
            } else {
                let pct = (data[i].close - lowest) / range * 100.0;
                self.values.push(IndicatorValue::Single(pct));
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
        vec![format!("52W HL%({})", self.period)]
    }
}
