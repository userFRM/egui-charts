use crate::model::Bar;
/// Moving Average Cross (SMA Cross)
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct MACross {
    fast_period: usize,
    slow_period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl MACross {
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Fast
                DESIGN_TOKENS.semantic.extended.error,   // Red - Slow
            ],
            visible: true,
        }
    }
}

impl Default for MACross {
    fn default() -> Self {
        Self::new(9, 21)
    }
}

impl Indicator for MACross {
    fn name(&self) -> &str {
        "MA Cross"
    }

    fn desc(&self) -> &str {
        "Moving Average Cross - Two SMA lines for crossover signals"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.slow_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.slow_period {
                self.values.push(IndicatorValue::None);
            } else {
                let fast_start = i + 1 - self.fast_period;
                let fast_sma: f64 = data[fast_start..=i].iter().map(|b| b.close).sum::<f64>()
                    / self.fast_period as f64;

                let slow_start = i + 1 - self.slow_period;
                let slow_sma: f64 = data[slow_start..=i].iter().map(|b| b.close).sum::<f64>()
                    / self.slow_period as f64;

                self.values
                    .push(IndicatorValue::Multiple(vec![fast_sma, slow_sma]));
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
        if colors.len() >= 2 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
        2
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
            format!("Fast SMA({})", self.fast_period),
            format!("Slow SMA({})", self.slow_period),
        ]
    }
}
