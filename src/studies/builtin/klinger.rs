use crate::model::Bar;
/// Klinger Volume Oscillator
/// Measures long-term money flow trend while remaining sensitive to short-term fluctuations
/// Developed by Stephen Klinger
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct KlingerOscillator {
    /// Short EMA period (typically 34)
    short_period: usize,
    /// Long EMA period (typically 55)
    long_period: usize,
    /// Signal line period (typically 13)
    signal_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Klinger, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl KlingerOscillator {
    pub fn new() -> Self {
        Self {
            short_period: 34,
            long_period: 55,
            signal_period: 13,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Klinger
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_periods(mut self, short: usize, long: usize, signal: usize) -> Self {
        self.short_period = short;
        self.long_period = long;
        self.signal_period = signal;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // Find first valid value
        let mut first_sum = 0.0;
        let mut count = 0;
        let mut first_valid_idx = 0;

        for (i, &value) in data.iter().enumerate() {
            if !value.is_nan() {
                first_sum += value;
                count += 1;
                if count == period {
                    first_valid_idx = i;
                    break;
                }
            }
        }

        for i in 0..data.len() {
            if i < first_valid_idx {
                ema.push(f64::NAN);
            } else if i == first_valid_idx {
                ema.push(first_sum / period as f64);
            } else {
                let prev = ema[i - 1];
                if prev.is_nan() || data[i].is_nan() {
                    ema.push(f64::NAN);
                } else {
                    ema.push((data[i] - prev) * multiplier + prev);
                }
            }
        }

        ema
    }
}

impl Default for KlingerOscillator {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for KlingerOscillator {
    fn name(&self) -> &str {
        "Klinger"
    }

    fn desc(&self) -> &str {
        "Klinger Volume Oscillator - Long-term money flow trend"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate HLC (typical price) and trend
        let mut volume_force = Vec::with_capacity(data.len());
        volume_force.push(f64::NAN);

        let mut prev_hlc = (data[0].high + data[0].low + data[0].close) / 3.0;
        let mut prev_trend = 1;

        for i in 1..data.len() {
            let bar = &data[i];
            let hlc = (bar.high + bar.low + bar.close) / 3.0;

            // Determine trend
            let trend = if hlc > prev_hlc { 1 } else { -1 };

            // Calculate dm (direction multiplier)
            let dm = (bar.high - bar.low).abs();

            // Calculate cm (cumulative measure)
            let _cm = if trend == prev_trend {
                dm + (data[i - 1].high - data[i - 1].low).abs()
            } else {
                dm
            };

            // Volume Force = Volume * |2 * dm/cm - 1| * trend * 100
            // Simplified: VF = Volume * trend
            let vf = bar.volume * trend as f64;
            volume_force.push(vf);

            prev_hlc = hlc;
            prev_trend = trend;
        }

        // Calculate short and long EMAs of volume force
        let short_ema = Self::calculate_ema(&volume_force, self.short_period);
        let long_ema = Self::calculate_ema(&volume_force, self.long_period);

        // Calculate Klinger Oscillator
        let mut klinger = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if short_ema[i].is_nan() || long_ema[i].is_nan() {
                klinger.push(f64::NAN);
            } else {
                klinger.push(short_ema[i] - long_ema[i]);
            }
        }

        // Calculate signal line
        let signal = Self::calculate_ema(&klinger, self.signal_period);

        // Store results
        for i in 0..data.len() {
            if klinger[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else if signal[i].is_nan() {
                self.values.push(IndicatorValue::Single(klinger[i]));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![klinger[i], signal[i]]));
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
        vec![
            format!("KVO({},{})", self.short_period, self.long_period),
            format!("Signal({})", self.signal_period),
        ]
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
    fn test_klinger_calculation() {
        let mut klinger = KlingerOscillator::new().with_periods(5, 10, 3);

        let data: Vec<Bar> = (0..30)
            .map(|i| {
                make_bar(
                    102.0 + i as f64,
                    98.0 + i as f64,
                    100.0 + i as f64,
                    1000.0 + i as f64 * 100.0,
                )
            })
            .collect();

        klinger.calculate(&data);

        assert_eq!(klinger.values.len(), 30);

        // Should have valid values after warmup
        let valid_cnt = klinger
            .values
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
