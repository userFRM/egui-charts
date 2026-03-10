use crate::model::Bar;
/// Volatility Index (simplified VIX-like)
/// ATR-based volatility normalized as percentage of price
/// Formula: ATR(period) / close * 100 * sqrt(252)
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct VolatilityIndex {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl VolatilityIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![Color32::from_rgb(255, 152, 0)], // Orange
            visible: true,
        }
    }
}

impl Default for VolatilityIndex {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for VolatilityIndex {
    fn name(&self) -> &str {
        "Vol Index"
    }

    fn desc(&self) -> &str {
        "Volatility Index - ATR-based volatility as annualized percentage of price"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Calculate True Range series
        let mut tr_values = Vec::with_capacity(data.len());
        tr_values.push(data[0].high - data[0].low); // First bar: just H-L

        for i in 1..data.len() {
            let hl = data[i].high - data[i].low;
            let hc = (data[i].high - data[i - 1].close).abs();
            let lc = (data[i].low - data[i - 1].close).abs();
            tr_values.push(hl.max(hc).max(lc));
        }

        // Calculate ATR using EMA smoothing, then normalize
        let mult = 2.0 / (self.period as f64 + 1.0);
        let mut atr = tr_values[0];

        // First bar: no ATR warmup yet
        self.values.push(IndicatorValue::None);

        for i in 1..data.len() {
            atr = (tr_values[i] - atr) * mult + atr;

            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let close = data[i].close;
            if close > 0.0 {
                // ATR / close * 100 * sqrt(252) for annualized vol percentage
                let vol_pct = atr / close * 100.0 * 252.0_f64.sqrt();
                self.values.push(IndicatorValue::Single(vol_pct));
            } else {
                self.values.push(IndicatorValue::None);
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
        vec![format!("Vol Index({})", self.period)]
    }
}
