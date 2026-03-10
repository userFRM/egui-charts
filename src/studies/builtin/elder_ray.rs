use crate::model::Bar;
/// Elder Ray Index (Bull Power / Bear Power)
/// Measures buying and selling pressure
/// Developed by Alexander Elder
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct ElderRay {
    /// EMA period (typically 13)
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Bull Power (green), Bear Power (red)]
    colors: Vec<Color32>,
    visible: bool,
}

impl ElderRay {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.success, // Green - Bull Power
                DESIGN_TOKENS.semantic.extended.error,   // Red - Bear Power
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// Calculate EMA of close prices
    fn calculate_ema(data: &[Bar], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // First value is SMA
        let first_sma: f64 = data.iter().take(period).map(|b| b.close).sum::<f64>() / period as f64;

        for i in 0..data.len() {
            if i < period - 1 {
                ema.push(f64::NAN);
            } else if i == period - 1 {
                ema.push(first_sma);
            } else {
                let prev = ema[i - 1];
                ema.push((data[i].close - prev) * multiplier + prev);
            }
        }

        ema
    }
}

impl Indicator for ElderRay {
    fn name(&self) -> &str {
        "Elder Ray"
    }

    fn desc(&self) -> &str {
        "Elder Ray - Bull and Bear Power indicators"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate EMA
        let ema = Self::calculate_ema(data, self.period);

        // Calculate Bull Power and Bear Power
        for i in 0..data.len() {
            if ema[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                // Bull Power = High - EMA
                let bull_power = data[i].high - ema[i];
                // Bear Power = Low - EMA
                let bear_power = data[i].low - ema[i];

                self.values
                    .push(IndicatorValue::Multiple(vec![bull_power, bear_power]));
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
        false // Separate pane
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
            format!("Bull({})", self.period),
            format!("Bear({})", self.period),
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
    fn test_elder_ray_calculation() {
        let mut er = ElderRay::new(3);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(104.0, 99.0, 102.0),
            make_bar(106.0, 100.0, 104.0),
            make_bar(108.0, 102.0, 106.0),
        ];

        er.calculate(&data);

        assert_eq!(er.values.len(), 4);

        // First two should be None
        assert!(matches!(er.values[0], IndicatorValue::None));
        assert!(matches!(er.values[1], IndicatorValue::None));

        // Third should have values
        if let IndicatorValue::Multiple(v) = &er.values[2] {
            assert_eq!(v.len(), 2);
            // Bull power should be positive (high > ema typically in uptrend)
        }
    }

    #[test]
    fn test_elder_ray_interpretation() {
        // In an uptrend, bull power should be positive
        // In a downtrend, bear power should be negative
        let mut er = ElderRay::new(2);

        // Uptrend
        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(105.0, 100.0, 104.0),
            make_bar(110.0, 104.0, 108.0),
        ];

        er.calculate(&data);

        if let IndicatorValue::Multiple(v) = &er.values[2] {
            // Bull power should be positive in uptrend
            assert!(v[0] > 0.0);
        }
    }
}
