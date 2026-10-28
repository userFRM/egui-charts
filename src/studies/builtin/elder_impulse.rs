use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Signal state for Elder Impulse
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImpulseSignal {
    /// EMA rising, MACD histogram rising
    Bullish,
    /// EMA falling, MACD histogram falling
    Bearish,
    /// Mixed signals or an unchanged EMA or MACD histogram
    Neutral,
}

/// Elder Impulse System
/// Combines EMA slope and MACD histogram for trend signals
/// Green = bullish, Red = bearish, Blue = neutral
#[derive(Clone)]
pub struct ElderImpulseSystem {
    /// EMA period (typically 13)
    ema_period: usize,
    /// MACD fast period
    macd_fast: usize,
    /// MACD slow period
    macd_slow: usize,
    /// MACD signal period
    macd_signal: usize,
    values: IndicatorValues,
    stepper: Stepper<(usize, [Smoothing; 4], [f64; 2])>,
    /// Colors: [Bullish, Bearish, Neutral]
    colors: Vec<Color32>,
    visible: bool,
}

impl ElderImpulseSystem {
    /// Create with `ema_period`, `macd_fast`, `macd_slow`, `macd_signal` in bars, each clamped
    /// to at least one.
    pub fn new(ema_period: usize, macd_fast: usize, macd_slow: usize, macd_signal: usize) -> Self {
        Self {
            ema_period: ema_period.max(1),
            macd_fast: macd_fast.max(1),
            macd_slow: macd_slow.max(1),
            macd_signal: macd_signal.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::GREEN, // Green - Bullish
                palette::RED,   // Red - Bearish
                palette::BLUE,  // Blue - Neutral
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for ElderImpulseSystem {
    fn default() -> Self {
        Self::new(13, 12, 26, 9)
    }
}

impl Indicator for ElderImpulseSystem {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        ema_period: length("EMA period", 1, usize::MAX),
        macd_fast: length("MACD fast", 1, usize::MAX),
        macd_slow: length("MACD slow", 1, usize::MAX),
        macd_signal: length("MACD signal", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Impulse"
    }

    fn desc(&self) -> &str {
        "Elder Impulse System - Trend direction with momentum"
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
                state.0 += 1;
                let ema = state.1[0].next(bar.close, 1, 2.0 / (self.ema_period as f64 + 1.0));
                let fast = state.1[1].next(bar.close, 1, 2.0 / (self.macd_fast as f64 + 1.0));
                let slow = state.1[2].next(bar.close, 1, 2.0 / (self.macd_slow as f64 + 1.0));
                let macd = fast - slow;
                let signal = state.1[3].next(macd, 1, 2.0 / (self.macd_signal as f64 + 1.0));
                let histogram = macd - signal;
                let previous = std::mem::replace(&mut state.2, [ema, histogram]);
                let required = self
                    .macd_slow
                    .max(self.ema_period)
                    .saturating_add(self.macd_signal);
                if state.0 < required || state.0 <= 1 {
                    return IndicatorValue::None;
                }
                let rising = ema > previous[0];
                let hist_rising = histogram > previous[1];
                IndicatorValue::Single(if rising && hist_rising {
                    1.0
                } else if ema < previous[0] && histogram < previous[1] {
                    -1.0
                } else {
                    0.0
                })
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
        if colors.len() >= 3 {
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
        vec!["Impulse".to_string()]
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
    fn test_elder_impulse() {
        for direction in [0.0, 1.0, -1.0] {
            let mut impulse = ElderImpulseSystem::new(5, 6, 12, 5);
            // A constant has neither slope; accelerating rises/falls give both slopes that sign.
            let data: Vec<Bar> = (0..40)
                .map(|i| make_bar(100.0 + direction * (i * i) as f64))
                .collect();
            impulse.calculate(&data);
            assert_eq!(impulse.values.len(), data.len());
            for bar in 0..data.len() {
                assert_eq!(
                    impulse.values().value(0, bar),
                    (bar >= 16).then_some(direction),
                    "direction {direction}, bar {bar}"
                );
            }
        }
    }
}
