use crate::model::Bar;
/// Moving Average Channel
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct MAChannel {
    period: usize,
    offset_percent: f64,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl MAChannel {
    pub fn new(period: usize, offset_percent: f64) -> Self {
        Self {
            period,
            offset_percent,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Upper
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Middle
                DESIGN_TOKENS.semantic.extended.error,   // Red - Lower
            ],
            visible: true,
        }
    }
}

impl Default for MAChannel {
    fn default() -> Self {
        Self::new(20, 1.0)
    }
}

impl Indicator for MAChannel {
    fn name(&self) -> &str {
        "MA Channel"
    }

    fn desc(&self) -> &str {
        "Moving Average Channel - MA with upper and lower offset bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        let multiplier = self.offset_percent / 100.0;

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let start = i + 1 - self.period;
                let sma: f64 =
                    data[start..=i].iter().map(|b| b.close).sum::<f64>() / self.period as f64;

                let upper = sma * (1.0 + multiplier);
                let lower = sma * (1.0 - multiplier);

                self.values
                    .push(IndicatorValue::Multiple(vec![upper, sma, lower]));
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
        true
    }

    fn line_cnt(&self) -> usize {
        3
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
            format!("Upper(+{}%)", self.offset_percent),
            format!("MA({})", self.period),
            format!("Lower(-{}%)", self.offset_percent),
        ]
    }
}
