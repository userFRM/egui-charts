use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Bollinger Bands Width - measures how wide the Bollinger Bands are.
/// Formula: (upper_band - lower_band) / middle_band * 100
/// where middle = SMA(close, period), upper = middle + std_dev * stdev,
/// lower = middle - std_dev * stdev.
#[derive(Clone)]
pub struct BollingerBandsWidth {
    period: usize,
    std_dev: f64,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl BollingerBandsWidth {
    pub fn new(period: usize, std_dev: f64) -> Self {
        Self {
            period: period.max(1),
            std_dev,
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.purple],
            visible: true,
        }
    }
}

impl Default for BollingerBandsWidth {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for BollingerBandsWidth {
    fn name(&self) -> &str {
        "BBW"
    }

    fn desc(&self) -> &str {
        "Bollinger Bands Width - Band width as percentage of middle band"
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
            let prices: Vec<f64> = data[start..=i].iter().map(|b| b.close).collect();

            let sma = prices.iter().sum::<f64>() / self.period as f64;
            let variance =
                prices.iter().map(|p| (p - sma).powi(2)).sum::<f64>() / self.period as f64;
            let stdev = variance.sqrt();

            let upper = sma + self.std_dev * stdev;
            let lower = sma - self.std_dev * stdev;

            if sma.abs() < 1e-15 {
                self.values.push(IndicatorValue::Single(0.0));
            } else {
                let bbw = (upper - lower) / sma * 100.0;
                self.values.push(IndicatorValue::Single(bbw));
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
        vec![format!("BBW({}, {})", self.period, self.std_dev)]
    }
}
