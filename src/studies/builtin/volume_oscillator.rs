use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
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
    stepper: Stepper<[Smoothing; 2]>,
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
            stepper: Stepper::default(),
            color: palette::BLUE, // Blue
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                let short = state[0].next(
                    bar.volume,
                    self.short_period,
                    2.0 / (self.short_period as f64 + 1.0),
                );
                let long = state[1].next(
                    bar.volume,
                    self.long_period,
                    2.0 / (self.long_period as f64 + 1.0),
                );
                if short.is_nan() || long.is_nan() || long == 0.0 {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single((short - long) / long * 100.0)
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
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
