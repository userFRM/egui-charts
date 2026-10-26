use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Stochastic Momentum Index (SMI)
/// Enhanced stochastic that measures close relative to HL range midpoint
/// More responsive than standard stochastic
#[derive(Clone)]
pub struct StochasticMomentumIndex {
    /// Lookback period (typically 10)
    period: usize,
    /// First EMA smoothing (typically 3)
    smooth1: usize,
    /// Second EMA smoothing (typically 3)
    smooth2: usize,
    /// Signal line period (typically 10)
    signal_period: usize,
    values: IndicatorValues,
    /// Colors: [SMI, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl StochasticMomentumIndex {
    /// Create with `period`, `smooth1`, `smooth2`, `signal_period` in bars, each clamped to at
    /// least one.
    pub fn new(period: usize, smooth1: usize, smooth2: usize, signal_period: usize) -> Self {
        Self {
            period: period.max(1),
            smooth1: smooth1.max(1),
            smooth2: smooth2.max(1),
            signal_period: signal_period.max(1),
            values: IndicatorValues::new(),
            colors: vec![
                palette::BLUE,   // Blue - SMI
                palette::ORANGE, // Orange - Signal
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// The values over `data`, every bar of which has a finite high, low and close.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate highest high and lowest low
        let mut highest = Vec::with_capacity(data.len());
        let mut lowest = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < self.period - 1 {
                highest.push(f64::NAN);
                lowest.push(f64::NAN);
            } else {
                let window = &data[i + 1 - self.period..=i];
                let hh = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let ll = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                highest.push(hh);
                lowest.push(ll);
            }
        }

        // Calculate distance from HL midpoint: Close - (HH + LL) / 2
        let mut distance = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if highest[i].is_nan() || lowest[i].is_nan() {
                distance.push(f64::NAN);
            } else {
                let midpoint = (highest[i] + lowest[i]) / 2.0;
                distance.push(data[i].close - midpoint);
            }
        }

        // Calculate HL range: HH - LL
        let mut range = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if highest[i].is_nan() || lowest[i].is_nan() {
                range.push(f64::NAN);
            } else {
                range.push(highest[i] - lowest[i]);
            }
        }

        // Double EMA smoothing
        let distance_ema1 = helpers::ema(&distance, self.smooth1);
        let distance_ema2 = helpers::ema(&distance_ema1, self.smooth2);

        let range_ema1 = helpers::ema(&range, self.smooth1);
        let range_ema2 = helpers::ema(&range_ema1, self.smooth2);

        // Calculate SMI
        let mut smi = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if distance_ema2[i].is_nan() || range_ema2[i].is_nan() || range_ema2[i] == 0.0 {
                smi.push(f64::NAN);
            } else {
                let value = 100.0 * distance_ema2[i] / (range_ema2[i] / 2.0);
                smi.push(value);
            }
        }

        // Calculate signal line
        let signal = helpers::ema(&smi, self.signal_period);

        // Store results
        for i in 0..data.len() {
            if smi[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[smi[i], signal[i]]);
            }
        }

        values
    }
}

impl Default for StochasticMomentumIndex {
    fn default() -> Self {
        Self::new(10, 3, 3, 10)
    }
}

impl Indicator for StochasticMomentumIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smooth1: length("Smooth1", 1, usize::MAX),
        smooth2: length("Smooth2", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "SMI"
    }

    fn desc(&self) -> &str {
        "Stochastic Momentum Index - Enhanced stochastic"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
            |data| self.compute(data),
        );
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
        vec![
            format!("SMI({},{},{})", self.period, self.smooth1, self.smooth2),
            format!("Signal({})", self.signal_period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_smi_calculation() {
        let mut smi = StochasticMomentumIndex::new(5, 2, 2, 3);

        let data: Vec<Bar> = (0..20)
            .map(|i| make_bar(102.0 + i as f64, 98.0 + i as f64, 101.0 + i as f64))
            .collect();

        smi.calculate(&data);

        assert_eq!(smi.values.len(), 20);

        // Should have valid values
        let valid_cnt = smi
            .values
            .to_rows()
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
