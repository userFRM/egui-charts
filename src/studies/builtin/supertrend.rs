//! SuperTrend Indicator
//!
//! SuperTrend is a trend-following indicator that uses ATR to calculate
//! dynamic support and resistance levels. It's useful for identifying
//! trend direction and potential entry/exit points.
//!
//! # Components
//! - Up Trend: the SuperTrend line while the trend is bullish (below price)
//! - Down Trend: the SuperTrend line while the trend is bearish (above price)
//!
//! Each plotted bar has a value on exactly one of the two lines; the other is NaN.
//!
//! The first bar with an ATR starts a down trend on the upper band. A down trend turns up when a
//! close rises above the upper band, and an up trend turns down when a close falls below the lower
//! band.
//! The first bar with a finite high, low and close has no value; with an ATR length of 1
//! its down trend still decides the second bar.
//!
//! # Interpretation
//! - SuperTrend below price (green): Bullish trend
//! - SuperTrend above price (red): Bearish trend
//! - Price crossing SuperTrend signals trend change
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{SuperTrend, Indicator};
//!
//! let mut st = SuperTrend::new(10, 3.0);
//! st.calculate(&bars);
//! ```

use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// SuperTrend indicator
#[derive(Clone, Debug)]
pub struct SuperTrend {
    atr_period: usize,
    multiplier: f64,
    values: IndicatorValues,
    stepper: Stepper<State>,
    bullish_color: Color32,
    bearish_color: Color32,
    visible: bool,
}

#[derive(Clone, Debug)]
struct State {
    usable: usize,
    previous: f64,
    atr: f64,
    upper: f64,
    lower: f64,
    trend: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            usable: 0,
            previous: 0.0,
            atr: -0.0,
            upper: 0.0,
            lower: 0.0,
            trend: 0.0,
        }
    }
}

impl SuperTrend {
    /// Create a new SuperTrend indicator
    ///
    /// # Arguments
    /// * `atr_period` - ATR period (typically 10)
    /// * `multiplier` - ATR multiplier (typically 3.0), clamped into 0.01 to `f64::MAX`, NaN to 0.01
    pub fn new(atr_period: usize, multiplier: f64) -> Self {
        Self {
            atr_period: atr_period.max(1),
            multiplier: clamp_param(multiplier, 0.01, f64::MAX),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            bullish_color: palette::GREEN, // Green
            bearish_color: palette::RED,   // Red
            visible: true,
        }
    }

    /// Set colors for bullish and bearish trends
    pub fn with_colors(mut self, bullish: Color32, bearish: Color32) -> Self {
        self.bullish_color = bullish;
        self.bearish_color = bearish;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for SuperTrend {
    fn default() -> Self {
        Self::new(10, 3.0)
    }
}

impl Indicator for SuperTrend {
    crate::studies::inputs::study_inputs!(self;
        atr_period: length("ATR Length", 1, usize::MAX),
        multiplier: number("Factor", 0.01, f64::MAX, 0.01)
    ; {});

    fn name(&self) -> &str {
        "SuperTrend"
    }

    fn desc(&self) -> &str {
        "SuperTrend - Trend following indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let period = self.atr_period;
        let atr_mult = 1.0 / period as f64;
        let multiplier = self.multiplier;
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() || !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let tr = if state.usable == 0 {
                    bar.high - bar.low
                } else {
                    ATR::true_range(bar, state.previous)
                };
                let previous = state.previous;
                state.previous = bar.close;
                state.usable += 1;
                if state.usable <= period {
                    state.atr += tr;
                    if state.usable < period {
                        return IndicatorValue::None;
                    }
                    state.atr /= period as f64;
                } else {
                    state.atr += atr_mult * (tr - state.atr);
                }
                let hl2 = (bar.high + bar.low) / 2.0;
                let upper = hl2 + multiplier * state.atr;
                let lower = hl2 - multiplier * state.atr;
                let bullish = if state.usable == period {
                    state.upper = upper;
                    state.lower = lower;
                    // A down trend until a close rises above the upper band, as published.
                    false
                } else {
                    let was_upper = state.trend == state.upper;
                    if upper < state.upper || previous > state.upper {
                        state.upper = upper;
                    }
                    if lower > state.lower || previous < state.lower {
                        state.lower = lower;
                    }
                    if was_upper {
                        bar.close > state.upper
                    } else {
                        bar.close.partial_cmp(&state.lower) != Some(std::cmp::Ordering::Less)
                    }
                };
                state.trend = if bullish { state.lower } else { state.upper };
                // The first bar has no previous close; its trend still seeds the next bar.
                if state.usable == 1 {
                    return IndicatorValue::None;
                }
                if bullish {
                    IndicatorValue::Multiple(vec![state.trend, f64::NAN])
                } else {
                    IndicatorValue::Multiple(vec![f64::NAN, state.trend])
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn alert_conditions(&self) -> Vec<String> {
        ["Trend turns up", "Trend turns down", "Trend change"]
            .map(str::to_owned)
            .into()
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        let up = |bar| {
            if self.values.value(0, bar).is_some() {
                Some(true)
            } else if self.values.value(1, bar).is_some() {
                Some(false)
            } else {
                None
            }
        };
        let now = up(bar)?;
        let before = up(bar.checked_sub(1)?)?;
        match condition {
            0 => Some(now && !before),
            1 => Some(!now && before),
            2 => Some(now != before),
            _ => None,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.bullish_color, self.bearish_color]
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors().get(line).copied().unwrap_or(Color32::WHITE),
            width: self.line_style(line).width,
            pattern: crate::model::LineStyle::Solid,
            // Each trend line holds values only while its trend lasts.
            plot_type: crate::studies::PlotType::LineWithBreaks,
            visible: true,
        }
    }

    fn draws_only_plots(&self) -> bool {
        true
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.bullish_color = colors[0];
        }
        if colors.len() > 1 {
            self.bearish_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        true // SuperTrend is drawn on main chart
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
        vec!["Up Trend".to_string(), "Down Trend".to_string()]
    }
}
