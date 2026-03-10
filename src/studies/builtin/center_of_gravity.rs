use crate::model::Bar;
/// Center of Gravity Oscillator
/// Developed by John Ehlers
/// Uses a weighted moving avg to identify reversal points
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct CenterOfGravity {
    /// Lookback period (typically 10)
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [COG, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl CenterOfGravity {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.bullish,     // Teal - COG
                DESIGN_TOKENS.semantic.extended.deep_orange, // Deep Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for CenterOfGravity {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for CenterOfGravity {
    fn name(&self) -> &str {
        "COG"
    }

    fn desc(&self) -> &str {
        "Center of Gravity - Identifies price reversal points"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        let mut cog_values = Vec::with_capacity(data.len());
        let mut prev_cog = 0.0;

        for i in 0..data.len() {
            if i < self.period - 1 {
                cog_values.push((f64::NAN, f64::NAN));
                continue;
            }

            let window = &closes[i + 1 - self.period..=i];

            // Calculate numerator: sum of (price * (period - j + 1))
            let mut num = 0.0;
            let mut den = 0.0;

            for (j, &price) in window.iter().enumerate() {
                let weight = (self.period - j) as f64;
                num += weight * price;
                den += price;
            }

            let cog = if den != 0.0 { -num / den } else { 0.0 };
            let signal = prev_cog;

            prev_cog = cog;
            cog_values.push((cog, signal));
        }

        // Store results
        for (i, (cog, signal)) in cog_values.iter().enumerate() {
            if cog.is_nan() {
                self.values.push(IndicatorValue::None);
            } else if i < self.period || signal.is_nan() || *signal == 0.0 {
                self.values.push(IndicatorValue::Single(*cog));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![*cog, *signal]));
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
        vec![format!("COG({})", self.period), "Signal".to_string()]
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
    fn test_cog_calculation() {
        let mut cog = CenterOfGravity::new(5);

        let data = vec![
            make_bar(100.0),
            make_bar(102.0),
            make_bar(104.0),
            make_bar(103.0),
            make_bar(105.0),
            make_bar(107.0),
            make_bar(106.0),
        ];

        cog.calculate(&data);

        assert_eq!(cog.values.len(), 7);

        // Should have valid values after period
        let valid_cnt = cog
            .values
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
