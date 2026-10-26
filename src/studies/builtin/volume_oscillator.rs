use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Volume Oscillator
/// Measures the difference between two volume moving avgs
/// Similar to MACD but for volume
#[derive(Clone)]
pub struct VolumeOscillator {
    /// Short EMA period (typically 5)
    short_period: usize,
    /// Long EMA period (typically 10)
    long_period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl VolumeOscillator {
    /// Create with `short_period`, `long_period` in bars, each clamped to at least one.
    pub fn new(short_period: usize, long_period: usize) -> Self {
        Self {
            short_period: short_period.max(1),
            long_period: long_period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite volume.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.long_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Extract volumes
        let volumes: Vec<f64> = data.iter().map(|b| b.volume).collect();

        // Calculate short and long EMAs
        let short_ema = helpers::ema_sma(&volumes, self.short_period);
        let long_ema = helpers::ema_sma(&volumes, self.long_period);

        // Calculate oscillator as percentage
        for i in 0..data.len() {
            if short_ema[i].is_nan() || long_ema[i].is_nan() || long_ema[i] == 0.0 {
                values.push(IndicatorValue::None);
            } else {
                let osc = (short_ema[i] - long_ema[i]) / long_ema[i] * 100.0;
                values.push(IndicatorValue::Single(osc));
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for VolumeOscillator {
    fn default() -> Self {
        Self::new(5, 10)
    }
}

impl Indicator for VolumeOscillator {
    crate::studies::inputs::study_inputs!(self;
        short_period: length("Short period", 1, usize::MAX),
        long_period: length("Long period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "Volume Osc"
    }

    fn desc(&self) -> &str {
        "Volume Oscillator - Difference between volume EMAs"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.volume.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
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
        vec![format!(
            "VolOsc({},{})",
            self.short_period, self.long_period
        )]
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
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_volume_oscillator() {
        let mut vo = VolumeOscillator::new(3, 5);

        let data = vec![
            make_bar(1000.0),
            make_bar(1100.0),
            make_bar(1200.0),
            make_bar(1300.0),
            make_bar(1400.0),
            make_bar(1500.0),
            make_bar(1600.0),
        ];

        vo.calculate(&data);

        assert_eq!(vo.values.len(), 7);

        // Should have valid values after long period warmup
        let valid_cnt = vo
            .values
            .to_rows()
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        assert!(valid_cnt > 0);
    }

    #[test]
    fn test_volume_oscillator_rising() {
        let mut vo = VolumeOscillator::new(2, 4);

        // Rising volume - short EMA should be > long EMA = positive oscillator
        let data = vec![
            make_bar(100.0),
            make_bar(200.0),
            make_bar(300.0),
            make_bar(400.0),
            make_bar(500.0),
            make_bar(600.0),
        ];

        vo.calculate(&data);

        // Last value should be positive (rising volume)
        let rows = vo.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", vo.values.to_rows().last().unwrap());
        };
        assert!(*v > 0.0, "Oscillator should be positive for rising volume");
    }
}
