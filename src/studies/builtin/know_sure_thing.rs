use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::rolling_mean;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Know Sure Thing (KST) Oscillator
/// Developed by Martin Pring
/// Combines multiple ROC values with different smoothing periods
#[derive(Clone)]
pub struct KnowSureThing {
    /// ROC periods (typically 10, 15, 20, 30)
    roc_periods: [usize; 4],
    /// SMA periods for smoothing (typically 10, 10, 10, 15)
    sma_periods: [usize; 4],
    /// Signal line SMA period (typically 9)
    signal_period: usize,
    values: IndicatorValues,
    /// Colors: [KST, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl KnowSureThing {
    /// Create with ROC periods 10/15/20/30, SMA periods 10/10/10/15 and signal period 9.
    pub fn new() -> Self {
        Self {
            roc_periods: [10, 15, 20, 30],
            sma_periods: [10, 10, 10, 15],
            signal_period: 9,
            values: IndicatorValues::new(),
            colors: vec![
                Color32::from_rgb(0, 150, 136), // KST
                Color32::from_rgb(244, 67, 54), // Signal
            ],
            visible: true,
        }
    }

    /// Set the four rate-of-change periods, their SMA periods and the signal period, each at least one.
    pub fn with_params(
        mut self,
        roc_periods: [usize; 4],
        sma_periods: [usize; 4],
        signal_period: usize,
    ) -> Self {
        self.roc_periods = roc_periods.map(|p| p.max(1));
        self.sma_periods = sma_periods.map(|p| p.max(1));
        self.signal_period = signal_period.max(1);
        self
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
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
}

impl Default for KnowSureThing {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowSureThing {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        // Get closes
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate ROC for each period
        let roc1 = Self::calculate_roc(&closes, self.roc_periods[0]);
        let roc2 = Self::calculate_roc(&closes, self.roc_periods[1]);
        let roc3 = Self::calculate_roc(&closes, self.roc_periods[2]);
        let roc4 = Self::calculate_roc(&closes, self.roc_periods[3]);

        // Smooth each ROC
        let sroc1 = rolling_mean(&roc1, self.sma_periods[0]);
        let sroc2 = rolling_mean(&roc2, self.sma_periods[1]);
        let sroc3 = rolling_mean(&roc3, self.sma_periods[2]);
        let sroc4 = rolling_mean(&roc4, self.sma_periods[3]);

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
        let signal = rolling_mean(&kst, self.signal_period);

        // Store results
        for i in 0..data.len() {
            if kst[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[kst[i], signal[i]]);
            }
        }

        values
    }
}

impl Indicator for KnowSureThing {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        roc_periods[0]: length("Roc periods 1", 1, usize::MAX),
        roc_periods[1]: length("Roc periods 2", 1, usize::MAX),
        roc_periods[2]: length("Roc periods 3", 1, usize::MAX),
        roc_periods[3]: length("Roc periods 4", 1, usize::MAX),
        sma_periods[0]: length("SMA periods 1", 1, usize::MAX),
        sma_periods[1]: length("SMA periods 2", 1, usize::MAX),
        sma_periods[2]: length("SMA periods 3", 1, usize::MAX),
        sma_periods[3]: length("SMA periods 4", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "KST"
    }

    fn desc(&self) -> &str {
        "Know Sure Thing - Multi-timeframe momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let lookback = self
            .roc_periods
            .iter()
            .zip(self.sma_periods)
            .map(|(roc, sma)| roc.saturating_add(sma - 1))
            .max()
            .unwrap()
            .saturating_add(self.signal_period - 1);
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(lookback, |b| b.close.is_finite()),
            |data| over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data)),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
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
