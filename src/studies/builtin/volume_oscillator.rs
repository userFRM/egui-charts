use crate::model::Bar;
/// Volume Oscillator
/// Measures the difference between two volume moving avgs
/// Similar to MACD but for volume
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VolumeOscillator {
    /// Short EMA period (typically 5)
    short_period: usize,
    /// Long EMA period (typically 10)
    long_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl VolumeOscillator {
    pub fn new(short_period: usize, long_period: usize) -> Self {
        Self {
            short_period,
            long_period,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // First value is SMA
        let first_sma: f64 = data.iter().take(period).sum::<f64>() / period as f64;

        for i in 0..data.len() {
            if i < period - 1 {
                ema.push(f64::NAN);
            } else if i == period - 1 {
                ema.push(first_sma);
            } else {
                let prev = ema[i - 1];
                ema.push((data[i] - prev) * multiplier + prev);
            }
        }

        ema
    }
}

impl Indicator for VolumeOscillator {
    fn name(&self) -> &str {
        "Volume Osc"
    }

    fn desc(&self) -> &str {
        "Volume Oscillator - Difference between volume EMAs"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.long_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Extract volumes
        let volumes: Vec<f64> = data.iter().map(|b| b.volume).collect();

        // Calculate short and long EMAs
        let short_ema = Self::calculate_ema(&volumes, self.short_period);
        let long_ema = Self::calculate_ema(&volumes, self.long_period);

        // Calculate oscillator as percentage
        for i in 0..data.len() {
            if short_ema[i].is_nan() || long_ema[i].is_nan() || long_ema[i] == 0.0 {
                self.values.push(IndicatorValue::None);
            } else {
                let osc = (short_ema[i] - long_ema[i]) / long_ema[i] * 100.0;
                self.values.push(IndicatorValue::Single(osc));
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
        if let IndicatorValue::Single(v) = vo.values.last().unwrap() {
            assert!(*v > 0.0, "Oscillator should be positive for rising volume");
        }
    }
}
