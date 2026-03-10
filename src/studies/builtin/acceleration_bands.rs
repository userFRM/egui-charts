use crate::model::Bar;
/// Acceleration Bands
/// Price envelope indicator that widens during high volatility
/// Uses a multiplier based on high-low range
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct AccelerationBands {
    /// Period for calculation
    period: usize,
    /// Band width factor (typically 4.0)
    factor: f64,
    values: Vec<IndicatorValue>,
    /// Colors: [Upper, Middle, Lower]
    colors: Vec<Color32>,
    visible: bool,
}

impl AccelerationBands {
    pub fn new(period: usize, factor: f64) -> Self {
        Self {
            period,
            factor,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Upper
                DESIGN_TOKENS.semantic.extended.favorite_gold, // Amber - Middle
                DESIGN_TOKENS.semantic.extended.error,   // Red - Lower
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for AccelerationBands {
    fn default() -> Self {
        Self::new(20, 4.0)
    }
}

impl Indicator for AccelerationBands {
    fn name(&self) -> &str {
        "AccBands"
    }

    fn desc(&self) -> &str {
        "Acceleration Bands - Volatility-based price envelope"
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

                // Calculate SMA of high, low, close
                let sma_high: f64 = window.iter().map(|b| b.high).sum::<f64>() / self.period as f64;
                let sma_low: f64 = window.iter().map(|b| b.low).sum::<f64>() / self.period as f64;
                let sma_close: f64 =
                    window.iter().map(|b| b.close).sum::<f64>() / self.period as f64;

                // Calculate acceleration factor
                let range: f64 = window
                    .iter()
                    .map(|b| {
                        let hl = b.high - b.low;
                        if b.high + b.low != 0.0 {
                            hl / (b.high + b.low)
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>()
                    / self.period as f64;

                let upper = sma_high * (1.0 + self.factor * range);
                let lower = sma_low * (1.0 - self.factor * range);

                self.values
                    .push(IndicatorValue::Multiple(vec![upper, sma_close, lower]));
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
            format!("Upper({})", self.period),
            "Middle".to_string(),
            format!("Lower({})", self.period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_acceleration_bands() {
        let mut ab = AccelerationBands::new(5, 4.0);

        let data: Vec<Bar> = (0..10)
            .map(|i| {
                let base = 100.0 + i as f64;
                make_bar(base + 2.0, base - 2.0, base)
            })
            .collect();

        ab.calculate(&data);

        assert_eq!(ab.values.len(), 10);

        // Upper should be > middle > lower
        if let IndicatorValue::Multiple(v) = ab.values.last().unwrap() {
            assert!(v[0] > v[1], "Upper should be > middle");
            assert!(v[1] > v[2], "Middle should be > lower");
        }
    }
}
