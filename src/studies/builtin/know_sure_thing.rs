use crate::model::Bar;
/// Know Sure Thing (KST) Oscillator
/// Developed by Martin Pring
/// Combines multiple ROC values with different smoothing periods
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct KnowSureThing {
    /// ROC periods (typically 10, 15, 20, 30)
    roc_periods: [usize; 4],
    /// SMA periods for smoothing (typically 10, 10, 10, 15)
    sma_periods: [usize; 4],
    /// Signal line SMA period (typically 9)
    signal_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [KST, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl KnowSureThing {
    pub fn new() -> Self {
        Self {
            roc_periods: [10, 15, 20, 30],
            sma_periods: [10, 10, 10, 15],
            signal_period: 9,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - KST
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_params(
        mut self,
        roc_periods: [usize; 4],
        sma_periods: [usize; 4],
        signal_period: usize,
    ) -> Self {
        self.roc_periods = roc_periods;
        self.sma_periods = sma_periods;
        self.signal_period = signal_period;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    fn calculate_roc(closes: &[f64], period: usize) -> Vec<f64> {
        let mut roc = Vec::with_capacity(closes.len());

        for i in 0..closes.len() {
            if i < period || closes[i - period] == 0.0 {
                roc.push(f64::NAN);
            } else {
                let change = (closes[i] - closes[i - period]) / closes[i - period] * 100.0;
                roc.push(change);
            }
        }

        roc
    }

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let valid: Vec<f64> = data[i + 1 - period..=i]
                    .iter()
                    .filter(|&&x| !x.is_nan())
                    .copied()
                    .collect();

                if valid.len() < period / 2 {
                    result.push(f64::NAN);
                } else {
                    result.push(valid.iter().sum::<f64>() / valid.len() as f64);
                }
            }
        }

        result
    }
}

impl Default for KnowSureThing {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for KnowSureThing {
    fn name(&self) -> &str {
        "KST"
    }

    fn desc(&self) -> &str {
        "Know Sure Thing - Multi-timeframe momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let max_period =
            self.roc_periods.iter().max().unwrap() + self.sma_periods.iter().max().unwrap();
        if data.len() < max_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate ROC for each period
        let roc1 = Self::calculate_roc(&closes, self.roc_periods[0]);
        let roc2 = Self::calculate_roc(&closes, self.roc_periods[1]);
        let roc3 = Self::calculate_roc(&closes, self.roc_periods[2]);
        let roc4 = Self::calculate_roc(&closes, self.roc_periods[3]);

        // Smooth each ROC
        let sroc1 = Self::calculate_sma(&roc1, self.sma_periods[0]);
        let sroc2 = Self::calculate_sma(&roc2, self.sma_periods[1]);
        let sroc3 = Self::calculate_sma(&roc3, self.sma_periods[2]);
        let sroc4 = Self::calculate_sma(&roc4, self.sma_periods[3]);

        // Calculate KST: (SROC1 * 1) + (SROC2 * 2) + (SROC3 * 3) + (SROC4 * 4)
        let mut kst = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if sroc1[i].is_nan() || sroc2[i].is_nan() || sroc3[i].is_nan() || sroc4[i].is_nan() {
                kst.push(f64::NAN);
            } else {
                let value = sroc1[i] * 1.0 + sroc2[i] * 2.0 + sroc3[i] * 3.0 + sroc4[i] * 4.0;
                kst.push(value);
            }
        }

        // Calculate signal line
        let signal = Self::calculate_sma(&kst, self.signal_period);

        // Store results
        for i in 0..data.len() {
            if kst[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else if signal[i].is_nan() {
                self.values.push(IndicatorValue::Single(kst[i]));
            } else {
                self.values
                    .push(IndicatorValue::Multiple(vec![kst[i], signal[i]]));
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
        vec!["KST".to_string(), format!("Signal({})", self.signal_period)]
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
    fn test_kst_calculation() {
        let mut kst = KnowSureThing::new().with_params([3, 5, 7, 10], [3, 3, 3, 5], 3);

        let data: Vec<Bar> = (0..50).map(|i| make_bar(100.0 + i as f64)).collect();

        kst.calculate(&data);

        assert_eq!(kst.values.len(), 50);

        // Should have valid values after warmup
        let valid_cnt = kst
            .values
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
