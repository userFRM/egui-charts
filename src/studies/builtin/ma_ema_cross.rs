use crate::model::Bar;
/// EMA Cross
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct EMACross {
    fast_period: usize,
    slow_period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl EMACross {
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

impl Default for EMACross {
    fn default() -> Self {
        Self::new(12, 26)
    }
}

impl Indicator for EMACross {
    fn name(&self) -> &str {
        "EMA Cross"
    }

    fn desc(&self) -> &str {
        "EMA Cross - Two EMA lines for crossover signals"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let fast_mult = 2.0 / (self.fast_period as f64 + 1.0);
        let slow_mult = 2.0 / (self.slow_period as f64 + 1.0);

        let mut fast_ema = data[0].close;
        let mut slow_ema = data[0].close;

        for bar in data {
            fast_ema = (bar.close - fast_ema) * fast_mult + fast_ema;
            slow_ema = (bar.close - slow_ema) * slow_mult + slow_ema;

            self.values
                .push(IndicatorValue::Multiple(vec![fast_ema, slow_ema]));
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
            format!("Fast EMA({})", self.fast_period),
            format!("Slow EMA({})", self.slow_period),
        ]
    }
}
