use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Anchored VWAP - VWAP calculated from a specific anchor bar index forward.
/// typical_price = (high + low + close) / 3
/// VWAP = cumsum(volume * typical_price) / cumsum(volume) from anchor_bar onward.
#[derive(Clone)]
pub struct AnchoredVWAP {
    anchor_bar: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl AnchoredVWAP {
    pub fn new(anchor_bar: usize) -> Self {
        Self {
            anchor_bar,
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.indicators.vwap],
            visible: true,
        }
    }

    #[inline]
    fn typical_price(bar: &Bar) -> f64 {
        (bar.high + bar.low + bar.close) / 3.0
    }
}

impl Default for AnchoredVWAP {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Indicator for AnchoredVWAP {
    fn name(&self) -> &str {
        "AVWAP"
    }

    fn desc(&self) -> &str {
        "Anchored VWAP - Volume weighted average price from anchor bar"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let mut cum_tp_vol = 0.0_f64;
        let mut cum_vol = 0.0_f64;

        for i in 0..data.len() {
            if i < self.anchor_bar {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let tp = Self::typical_price(&data[i]);
            cum_tp_vol += tp * data[i].volume;
            cum_vol += data[i].volume;

            if cum_vol.abs() < 1e-10 {
                self.values.push(IndicatorValue::Single(tp));
            } else {
                let vwap = cum_tp_vol / cum_vol;
                self.values.push(IndicatorValue::Single(vwap));
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
        vec![format!("AVWAP({})", self.anchor_bar)]
    }
}
