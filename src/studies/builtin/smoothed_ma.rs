use crate::model::Bar;
/// Smoothed Moving Average (Wilder's Smoothing)
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct SmoothedMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl SmoothedMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.indicators.ma,
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for SmoothedMA {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for SmoothedMA {
    fn name(&self) -> &str {
        "SMMA"
    }

    fn desc(&self) -> &str {
        "Smoothed Moving Average - Wilder's smoothing method"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // First value is SMA
        let first_sma: f64 =
            data[..self.period].iter().map(|b| b.close).sum::<f64>() / self.period as f64;

        for _i in 0..self.period - 1 {
            self.values.push(IndicatorValue::None);
        }
        self.values.push(IndicatorValue::Single(first_sma));

        let mut prev = first_sma;
        for i in self.period..data.len() {
            let smoothed = (prev * (self.period as f64 - 1.0) + data[i].close) / self.period as f64;
            self.values.push(IndicatorValue::Single(smoothed));
            prev = smoothed;
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
        vec![format!("SMMA({})", self.period)]
    }
}
