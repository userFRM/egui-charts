use crate::model::Bar;
/// Ease of Movement (EoM / EMV)
/// Relates price change to volume to identify the ease at which prices move
/// Developed by Richard Arms
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct EaseOfMovement {
    /// SMA smoothing period (typically 14)
    period: usize,
    /// Divisor for volume (typically 1,000,000 for stocks)
    divisor: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl EaseOfMovement {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            divisor: 1_000_000.0,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    pub fn with_divisor(mut self, divisor: f64) -> Self {
        self.divisor = divisor;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().sum();
                result.push(sum / period as f64);
            }
        }

        result
    }
}

impl Indicator for EaseOfMovement {
    fn name(&self) -> &str {
        "EoM"
    }

    fn desc(&self) -> &str {
        "Ease of Movement - Relates price change to volume"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate raw EoM values
        let mut raw_eom = Vec::with_capacity(data.len());
        raw_eom.push(f64::NAN); // First bar has no previous

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            // Distance Moved = ((High + Low) / 2) - ((Prev High + Prev Low) / 2)
            let distance_moved = (current.high + current.low) / 2.0 - (prev.high + prev.low) / 2.0;

            // Box Ratio = (Volume / divisor) / (High - Low)
            let range = current.high - current.low;
            if range == 0.0 || current.volume == 0.0 {
                raw_eom.push(0.0);
            } else {
                let box_ratio = (current.volume / self.divisor) / range;
                let eom = distance_moved / box_ratio;
                raw_eom.push(eom);
            }
        }

        // Apply SMA smoothing
        let smoothed = Self::calculate_sma(&raw_eom, self.period);

        // Store results
        for value in smoothed {
            if value.is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(value));
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
        vec![format!("EoM({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn test_eom_calculation() {
        let mut eom = EaseOfMovement::new(3).with_divisor(1.0);

        let data = vec![
            make_bar(105.0, 95.0, 100.0, 1000.0),
            make_bar(110.0, 100.0, 105.0, 1000.0),
            make_bar(115.0, 105.0, 110.0, 1000.0),
            make_bar(120.0, 110.0, 115.0, 1000.0),
            make_bar(125.0, 115.0, 120.0, 1000.0),
        ];

        eom.calculate(&data);

        assert_eq!(eom.values.len(), 5);

        // Should have valid values after period warmup
        let valid_cnt = eom
            .values
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        assert!(valid_cnt > 0);
    }
}
