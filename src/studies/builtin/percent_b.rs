use crate::model::Bar;
/// Percent B (%B)
/// Shows where price is relative to Bollinger Bands
/// 0 = at lower band, 1 = at upper band
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct PercentB {
    /// Period for Bollinger Bands (typically 20)
    period: usize,
    /// Standard deviation multiplier (typically 2.0)
    multiplier: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl PercentB {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period,
            multiplier,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for PercentB {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for PercentB {
    fn name(&self) -> &str {
        "%B"
    }

    fn desc(&self) -> &str {
        "Percent B - Price position within Bollinger Bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // Calculate SMA
                let sma: f64 = window.iter().map(|b| b.close).sum::<f64>() / self.period as f64;

                // Calculate standard deviation
                let variance: f64 = window.iter().map(|b| (b.close - sma).powi(2)).sum::<f64>()
                    / self.period as f64;
                let std_dev = variance.sqrt();

                // Calculate bands
                let upper = sma + self.multiplier * std_dev;
                let lower = sma - self.multiplier * std_dev;

                // Calculate %B
                let band_width = upper - lower;
                if band_width == 0.0 {
                    self.values.push(IndicatorValue::Single(0.5));
                } else {
                    let percent_b = (data[i].close - lower) / band_width;
                    self.values.push(IndicatorValue::Single(percent_b));
                }
            }
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
        vec![format!("%B({},{})", self.period, self.multiplier)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_percent_b() {
        let mut pb = PercentB::new(5, 2.0);

        // Constant prices = no volatility = %B should be ~0.5
        let data: Vec<Bar> = (0..10).map(|_| make_bar(100.0)).collect();

        pb.calculate(&data);

        if let IndicatorValue::Single(v) = pb.values.last().unwrap() {
            // With constant prices, %B should be 0.5 (middle of bands)
            // But with zero std dev, bands collapse
            assert!(v.is_finite());
        }
    }
}
