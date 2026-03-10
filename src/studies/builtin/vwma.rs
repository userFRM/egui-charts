use crate::model::Bar;
/// Volume Weighted Moving Avg (VWMA)
/// Moving avg weighted by volume - gives more importance to high-volume periods
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VWMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl VWMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.success, // Green
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for VWMA {
    fn name(&self) -> &str {
        "VWMA"
    }

    fn desc(&self) -> &str {
        "Volume Weighted Moving Avg - MA weighted by volume"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // VWMA = Sum(Price * Volume) / Sum(Volume)
                let price_volume_sum: f64 = window.iter().map(|bar| bar.close * bar.volume).sum();

                let volume_sum: f64 = window.iter().map(|bar| bar.volume).sum();

                let vwma = if volume_sum > 0.0 {
                    price_volume_sum / volume_sum
                } else {
                    // Fall back to simple avg if no volume
                    window.iter().map(|bar| bar.close).sum::<f64>() / self.period as f64
                };

                self.values.push(IndicatorValue::Single(vwma));
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
        true // Drawn on price chart
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
        vec![format!("VWMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume,
        }
    }

    #[test]
    fn test_vwma_calculation() {
        let mut vwma = VWMA::new(3);

        let data = vec![
            make_bar(100.0, 1000.0),
            make_bar(102.0, 2000.0),
            make_bar(101.0, 1500.0),
            make_bar(103.0, 1000.0),
        ];

        vwma.calculate(&data);

        assert_eq!(vwma.values.len(), 4);
        assert!(matches!(vwma.values[0], IndicatorValue::None));
        assert!(matches!(vwma.values[1], IndicatorValue::None));

        // Third bar VWMA = (100*1000 + 102*2000 + 101*1500) / (1000 + 2000 + 1500)
        // = (100000 + 204000 + 151500) / 4500 = 455500 / 4500 = 101.22
        if let IndicatorValue::Single(v) = vwma.values[2] {
            assert!((v - 101.22).abs() < 0.01);
        }
    }
}
