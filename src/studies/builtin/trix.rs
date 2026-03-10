use crate::model::Bar;
/// TRIX - Triple Exponential Avg
/// A momentum oscillator that shows the rate of change of a triple-smoothed EMA
/// Filters out insignificant price movements
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct TRIX {
    period: usize,
    signal_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [TRIX line, Signal line]
    colors: Vec<Color32>,
    visible: bool,
}

impl TRIX {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            signal_period: 9,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - TRIX
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_signal(mut self, signal_period: usize) -> Self {
        self.signal_period = signal_period;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// Calculate EMA
    /// Handles NaN values in input by finding the first valid window
    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // Find first index where we have `period` consecutive valid values
        let mut first_valid_idx = None;
        for start in 0..=data.len().saturating_sub(period) {
            let window = &data[start..start + period];
            if window.iter().all(|v| !v.is_nan()) {
                first_valid_idx = Some(start + period - 1);
                break;
            }
        }

        for i in 0..data.len() {
            match first_valid_idx {
                Some(valid_start) if i < valid_start => {
                    ema.push(f64::NAN);
                }
                Some(valid_start) if i == valid_start => {
                    // Calculate SMA from the valid window
                    let window_start = valid_start + 1 - period;
                    let sma: f64 =
                        data[window_start..=valid_start].iter().sum::<f64>() / period as f64;
                    ema.push(sma);
                }
                Some(_) => {
                    let prev = ema[i - 1];
                    if prev.is_nan() || data[i].is_nan() {
                        ema.push(f64::NAN);
                    } else {
                        ema.push((data[i] - prev) * multiplier + prev);
                    }
                }
                None => {
                    ema.push(f64::NAN);
                }
            }
        }

        ema
    }
}

impl Indicator for TRIX {
    fn name(&self) -> &str {
        "TRIX"
    }

    fn desc(&self) -> &str {
        "TRIX - Rate of change of triple-smoothed EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let required = self.period * 3 + 1;
        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate triple EMA
        let ema1 = Self::calculate_ema(&closes, self.period);
        let ema2 = Self::calculate_ema(&ema1, self.period);
        let ema3 = Self::calculate_ema(&ema2, self.period);

        // Calculate TRIX (percent rate of change of ema3)
        let mut trix_values = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i < 1 || ema3[i].is_nan() || ema3[i - 1].is_nan() || ema3[i - 1] == 0.0 {
                trix_values.push(f64::NAN);
            } else {
                let roc = (ema3[i] - ema3[i - 1]) / ema3[i - 1] * 100.0;
                trix_values.push(roc);
            }
        }

        // Calculate signal line (EMA of TRIX)
        let signal = Self::calculate_ema(&trix_values, self.signal_period);

        // Store results
        for i in 0..data.len() {
            if trix_values[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else if signal[i].is_nan() {
                self.values.push(IndicatorValue::Single(trix_values[i]));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![trix_values[i], signal[i]]));
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
            format!("TRIX({})", self.period),
            format!("Signal({})", self.signal_period),
        ]
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
    fn test_ema_calculation() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let ema = TRIX::calculate_ema(&data, 3);

        assert_eq!(ema.len(), 10);
        assert!(ema[0].is_nan());
        assert!(ema[1].is_nan());
        assert!(!ema[2].is_nan()); // First valid EMA
    }

    #[test]
    fn test_trix_calculation() {
        let mut trix = TRIX::new(5);

        // Create enough data points
        let data: Vec<Bar> = (0..50).map(|i| make_bar(100.0 + i as f64)).collect();

        trix.calculate(&data);

        assert_eq!(trix.values.len(), 50);

        // Later values should have TRIX values
        let last = &trix.values[49];
        assert!(matches!(
            last,
            IndicatorValue::Multiple(_) | IndicatorValue::Single(_)
        ));
    }
}
