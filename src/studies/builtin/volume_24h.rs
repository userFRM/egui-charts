//! 24-hour Volume
//!
//! The volume traded over the last 24 hours at each bar, whatever the chart interval: a
//! rolling sum over a fixed span of calendar time rather than a number of bars, so a
//! 1-minute and a 1-hour chart of the same market show the same turnover.
//!
//! # Calculation
//! Volume24h(t) = Sum(volume of the bars that open in (t - 24 h, t])
//!
//! where `t` is the open time of the current bar. A bar that opens exactly 24 hours before
//! the current one belongs to the previous day, so on an interval that divides the day the
//! window holds exactly one day of bars, and on daily bars it holds the bar alone. Bars
//! longer than a day (weekly, monthly) hold more than 24 hours of trading that their OHLCV
//! cannot split, so on those intervals the study has no value. The volume is summed in the
//! units the feed reports.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, Volume24h};
//!
//! // Hourly bars of 10 units: from the 25th bar on, the oldest bar leaves the window.
//! let bars: Vec<Bar> = (0..30)
//!     .map(|h| Bar::new(Utc.timestamp_opt(h * 3_600, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 10.0))
//!     .collect();
//! let mut study = Volume24h::new();
//! study.calculate(&bars);
//! assert_eq!(study.values().value(0, 0), Some(10.0));
//! assert_eq!(study.values().value(0, 23), Some(240.0));
//! assert_eq!(study.values().value(0, 29), Some(240.0));
//! ```

use crate::model::{Bar, BarClock, Timeframe};
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, TimeDelta, Utc};
use egui::Color32;
use std::collections::VecDeque;

/// Rolling volume over the last 24 hours of calendar time, in a separate pane.
///
/// A bar with a non-finite volume has no value and adds nothing. The chart supplies its
/// interval through [`Indicator::set_bar_clock`]; a direct caller on bars longer than a day
/// passes a clock of that interval, without which every bar is summed as an intraday bar.
#[derive(Clone)]
pub struct Volume24h {
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// Whether the chart interval is longer than a day.
    long_bars: bool,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    /// Open time and volume of each bar in the window, oldest first.
    window: VecDeque<(DateTime<Utc>, f64)>,
    sum: f64,
}

impl Volume24h {
    /// A 24-hour volume study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            long_bars: false,
            color: DESIGN_TOKENS.semantic.extended.info,
            visible: true,
        }
    }
}

impl Default for Volume24h {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Volume24h {
    fn name(&self) -> &str {
        "24-hour Volume"
    }

    fn desc(&self) -> &str {
        "24-hour Volume - Volume traded over the last 24 hours, on any interval"
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Volume
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.color,
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let long_bars = self.long_bars;
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if long_bars || !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                let start = bar
                    .time
                    .checked_sub_signed(TimeDelta::days(1))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                let mut largest = 0.0_f64;
                while let Some(&(time, volume)) = state.window.front()
                    && time <= start
                {
                    state.window.pop_front();
                    state.sum -= volume;
                    largest = largest.max(volume.abs());
                }
                if largest > state.sum.abs() {
                    // A removed volume that dominated the sum leaves its rounding behind:
                    // add the window again so the error does not linger.
                    state.sum = state.window.iter().map(|&(_, volume)| volume).sum();
                }
                state.window.push_back((bar.time, bar.volume));
                state.sum += bar.volume;
                // Overflow of a finite sum must not poison later ordinary windows.
                if !state.sum.is_finite() {
                    state.sum = state.window.iter().map(|&(_, volume)| volume).sum();
                }
                IndicatorValue::Single(state.sum)
            });
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let long_bars = clock
            .timeframe()
            .is_some_and(|timeframe| timeframe.duration_ms() > Timeframe::Day1.duration_ms());
        let changed = long_bars != self.long_bars;
        if changed {
            self.long_bars = long_bars;
            self.values.clear();
            self.stepper = Stepper::default();
        }
        changed
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
        if let Some(&color) = colors.first() {
            self.color = color;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Each value is the sum of its own window, even after a volume too large for the others to
    /// register in a running sum has left it.
    #[test]
    fn a_dominant_volume_leaves_no_residue() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let bars: Vec<Bar> = (0..60)
            .map(|h| {
                let volume = if h == 0 { 1e17 } else { 1.0 };
                Bar::new(start + TimeDelta::hours(h), 1.0, 1.0, 1.0, 1.0, volume)
            })
            .collect();
        let mut study = Volume24h::new();
        study.calculate(&bars);
        for bar in 24..60 {
            assert_eq!(study.values().value(0, bar), Some(24.0), "bar {bar}");
        }
    }
}
