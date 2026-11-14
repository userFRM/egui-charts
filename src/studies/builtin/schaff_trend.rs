use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Schaff Trend Cycle (STC)
/// Combines MACD with stochastic to create a faster, more accurate oscillator
/// Oscillates between 0 and 100
#[derive(Clone)]
pub struct SchaffTrendCycle {
    /// MACD short period (typically 23)
    macd_short: usize,
    /// MACD long period (typically 50)
    macd_long: usize,
    /// Stochastic cycle period (typically 10)
    cycle: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    index: usize,
    count: usize,
    ema: [Smoothing; 2],
    macd: std::collections::VecDeque<f64>,
    first: std::collections::VecDeque<f64>,
    smooth: [f64; 2],
    pending: Vec<(usize, f64)>,
}

impl SchaffTrendCycle {
    /// Create with MACD periods 23 and 50 and cycle length 10.
    pub fn new() -> Self {
        Self {
            macd_short: 23,
            macd_long: 50,
            cycle: 10,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PURPLE,
            visible: true,
        }
    }

    /// Set the short and long MACD periods and cycle length in bars, each at least one.
    pub fn with_periods(mut self, short: usize, long: usize, cycle: usize) -> Self {
        self.macd_short = short.max(1);
        self.macd_long = long.max(1);
        self.cycle = cycle.max(1);
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn stochastic(values: &std::collections::VecDeque<f64>, period: usize) -> f64 {
        let value = *values.back().unwrap();
        if values.len() < period || value.is_nan() {
            return f64::NAN;
        }
        let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let low = values.iter().copied().fold(f64::INFINITY, f64::min);
        let range = high - low;
        if range == 0.0 {
            50.0
        } else {
            (value - low) / range * 100.0
        }
    }
}

impl Default for SchaffTrendCycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for SchaffTrendCycle {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        macd_short: length("MACD short", 1, usize::MAX),
        macd_long: length("MACD long", 1, usize::MAX),
        cycle: length("Cycle", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "STC"
    }

    fn desc(&self) -> &str {
        "Schaff Trend Cycle - Fast trend oscillator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.index;
                state.index += 1;
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                state.count += 1;
                let short = state.ema[0].next(
                    bar.close,
                    self.macd_short,
                    2.0 / (self.macd_short as f64 + 1.0),
                );
                let long = state.ema[1].next(
                    bar.close,
                    self.macd_long,
                    2.0 / (self.macd_long as f64 + 1.0),
                );
                let macd = if short.is_nan() || long.is_nan() {
                    f64::NAN
                } else {
                    short - long
                };
                state.macd.push_back(macd);
                if state.macd.len() > self.cycle {
                    state.macd.pop_front();
                }
                let first = Self::stochastic(&state.macd, self.cycle);
                let smooth = if first.is_nan() {
                    f64::NAN
                } else {
                    state.smooth[0] += 0.5 * (first - state.smooth[0]);
                    state.smooth[0]
                };
                state.first.push_back(smooth);
                if state.first.len() > self.cycle {
                    state.first.pop_front();
                }
                let second = Self::stochastic(&state.first, self.cycle);
                if second.is_nan() {
                    return IndicatorValue::None;
                }
                state.smooth[1] += 0.5 * (second - state.smooth[1]);
                let value = state.smooth[1].clamp(0.0, 100.0);
                if state.count < self.macd_long.saturating_add(self.cycle) {
                    if value.is_finite() {
                        state.pending.push((index, value));
                    }
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(value)
                }
            });
        let required = self.macd_long.saturating_add(self.cycle);
        for &(index, value) in &self.stepper.state().pending {
            self.values.set(
                index,
                if self.stepper.state().count < required {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single(value)
                },
            );
        }
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
            "STC({},{},{})",
            self.macd_short, self.macd_long, self.cycle
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_bar;

    #[test]
    fn test_stc_calculation() {
        let mut stc = SchaffTrendCycle::new().with_periods(2, 3, 2);
        let data: Vec<Bar> = [100.0, 102.0, 101.0, 105.0, 103.0, 107.0]
            .into_iter()
            .map(make_bar)
            .collect();
        stc.calculate(&data);
        // Hand MACD from SMA seeds and EMA alpha 2/3,1/2: 0,2/3,2/9,20/27.
        // Two-bar stochastic: 50,100,0,100 (50 for the initial flat window).
        // Half-step smoothing: 25,62.5,31.25,65.625; second stochastic
        // 50,100,0,100 gives the same final half-step values.
        assert_eq!(stc.values().len(), 6);
        assert_eq!(stc.values().value(0, 0), None);
        assert_eq!(stc.values().value(0, 1), None);
        for (bar, want) in [(2, 25.0), (3, 62.5), (4, 31.25), (5, 65.625)] {
            assert_eq!(stc.values().value(0, bar), Some(want), "bar {bar}");
        }
    }
}
