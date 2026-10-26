use super::over_finite_bars;
use crate::model::Bar;
/// TRIX - Triple Exponential Avg
/// A momentum oscillator that shows the rate of change of a triple-smoothed EMA
/// Filters out insignificant price movements
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct TRIX {
    period: usize,
    signal_period: usize,
    values: IndicatorValues,
    /// Colors: [TRIX line, Signal line]
    colors: Vec<Color32>,
    visible: bool,
}

impl TRIX {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            signal_period: 9,
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - TRIX
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_signal(mut self, signal_period: usize) -> Self {
        self.signal_period = signal_period.max(1);
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
        if data.len() < period {
            return vec![f64::NAN; data.len()];
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

    /// The values over `data`, every bar of which has a finite close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        let required = self.period.saturating_mul(3).saturating_add(1);
        if data.len() < required {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
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
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[trix_values[i], signal[i]]);
            }
        }

        values
    }
}

/// Construct with the conventional default parameters.
impl Default for TRIX {
    fn default() -> Self {
        Self::new(15)
    }
}

impl Indicator for TRIX {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "TRIX"
    }

    fn desc(&self) -> &str {
        "TRIX - Rate of change of triple-smoothed EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
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
