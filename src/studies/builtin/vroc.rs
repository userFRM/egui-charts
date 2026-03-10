use crate::model::Bar;
/// Volume Rate of Change (VROC)
/// Measures the rate of change in volume over a specified period
/// Helps identify volume trends and divergences
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VolumeRateOfChange {
    /// Lookback period
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl VolumeRateOfChange {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple, // Purple
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for VolumeRateOfChange {
    fn name(&self) -> &str {
        "VROC"
    }

    fn desc(&self) -> &str {
        "Volume Rate of Change - Volume momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                let curr_volume = data[i].volume;
                let past_volume = data[i - self.period].volume;

                if past_volume == 0.0 {
                    self.values.push(IndicatorValue::None);
                } else {
                    let vroc = (curr_volume - past_volume) / past_volume * 100.0;
                    self.values.push(IndicatorValue::Single(vroc));
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
        vec![format!("VROC({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume,
        }
    }

    #[test]
    fn test_vroc_calculation() {
        let mut vroc = VolumeRateOfChange::new(5);

        let data = vec![
            make_bar(1000.0),
            make_bar(1100.0),
            make_bar(1200.0),
            make_bar(1300.0),
            make_bar(1400.0),
            make_bar(1500.0), // +50% vs 1000
            make_bar(2200.0), // +100% vs 1100
        ];

        vroc.calculate(&data);

        assert_eq!(vroc.values.len(), 7);

        // First 5 should be None
        for i in 0..5 {
            assert!(matches!(vroc.values[i], IndicatorValue::None));
        }

        // Check calculation at index 5: (1500 - 1000) / 1000 * 100 = 50%
        if let IndicatorValue::Single(v) = vroc.values[5] {
            assert!((v - 50.0).abs() < 0.01);
        }
    }
}
