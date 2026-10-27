use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Percentage Price Oscillator (PPO): MACD taken as a percentage of the long average.
///
/// - PPO = 100 * (EMA(close, short) - EMA(close, long)) / EMA(close, long).
/// - Signal = EMA(PPO, signal length), computed only over the bars where the PPO is defined
///   and seeded with the mean of its first `signal` values, like the study's other averages,
///   which are seeded with the mean of their first values.
/// - Histogram = PPO - signal.
///
/// With lengths 12, 26 and 9 a bar has no value before bar 25, the PPO alone on bars 25 to 32,
/// and from bar 33, the mean of the PPO on bars 25 to 33 being the first signal, the PPO, the
/// signal and the histogram. A bar with a non-finite close has no value and is skipped by the
/// averages.
#[derive(Clone)]
pub struct PriceOscillator {
    /// Short EMA period (typically 12)
    short_period: usize,
    /// Long EMA period (typically 26)
    long_period: usize,
    /// Signal line EMA period (typically 9)
    signal_period: usize,
    values: IndicatorValues,
    stepper: Stepper<[Smoothing; 3]>,
    /// Colors: [PPO line, Signal line, Histogram]
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceOscillator {
    /// Create with `short_period`, `long_period`, `signal_period` in bars, each clamped to at
    /// least one.
    pub fn new(short_period: usize, long_period: usize, signal_period: usize) -> Self {
        Self {
            short_period: short_period.max(1),
            long_period: long_period.max(1),
            signal_period: signal_period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // Blue - PPO
                palette::ORANGE, // Orange - Signal
                palette::GREEN,  // Green - Histogram
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.set_colors(colors);
        self
    }
}

/// Resume the percentage difference, its signal over defined values, and histogram.
pub(super) fn percentage_oscillator(
    state: &mut [Smoothing; 3],
    value: f64,
    short: usize,
    long: usize,
    signal: usize,
) -> IndicatorValue {
    let short_value = state[0].next(value, short, 2.0 / (short as f64 + 1.0));
    let long_value = state[1].next(value, long, 2.0 / (long as f64 + 1.0));
    let line = if long_value == 0.0 {
        f64::NAN
    } else {
        (short_value - long_value) / long_value * 100.0
    };
    let signal = state[2].next(line, signal, 2.0 / (signal as f64 + 1.0));
    if line.is_finite() {
        IndicatorValue::Multiple(vec![line, signal, line - signal])
    } else {
        IndicatorValue::None
    }
}

/// Construct with the conventional default parameters.
impl Default for PriceOscillator {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl Indicator for PriceOscillator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        short_period: length("Short period", 1, usize::MAX),
        long_period: length("Long period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "PPO"
    }

    fn desc(&self) -> &str {
        "Percentage Price Oscillator - Percentage difference between EMAs"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                percentage_oscillator(
                    state,
                    bar.close,
                    self.short_period,
                    self.long_period,
                    self.signal_period,
                )
            });
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
        // Lines without a colour in `colors` keep theirs.
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
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

    fn line_count(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("PPO({},{})", self.short_period, self.long_period),
            format!("Signal({})", self.signal_period),
            "Histogram".to_string(),
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
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_ppo_calculation() {
        let mut ppo = PriceOscillator::new(5, 10, 3);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        ppo.calculate(&data);

        assert_eq!(ppo.values.len(), 30);

        // Should have valid values after long period warmup
        let valid_cnt = ppo
            .values
            .to_rows()
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }

    #[test]
    fn test_ppo_uptrend() {
        let mut ppo = PriceOscillator::new(3, 6, 2);

        // Strong uptrend
        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64 * 2.0)).collect();

        ppo.calculate(&data);

        // PPO should be positive in uptrend
        if let Some(last_val) = ppo.values.to_rows().last() {
            let ppo_val = match last_val {
                IndicatorValue::Multiple(vals) => vals[0],
                IndicatorValue::Single(val) => *val,
                IndicatorValue::None => panic!("Expected non-None PPO value"),
            };
            assert!(ppo_val > 0.0, "PPO should be positive in uptrend");
        }
    }
}
