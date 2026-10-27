use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use chrono::Duration;
use egui::Color32;
use std::collections::VecDeque;

/// Close position in the high-low range over the inclusive last 364 days.
///
/// `(close - lowest_low) / (highest_high - lowest_low) * 100`, or 50 for a flat range.
/// Bars before 52 weeks of history have no value, regardless of the bar interval.
#[derive(Clone)]
pub struct Week52HighLow {
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLow {
    /// Create a 52-week high/low percentage study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![palette::PINK],
            visible: true,
        }
    }
}

impl Default for Week52HighLow {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Week52HighLow {
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "52W HL%"
    }

    fn desc(&self) -> &str {
        "52-Week High/Low Percentage - Close position within the last 364 days"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                match state.next(bar) {
                    None => IndicatorValue::None,
                    Some((high, low)) => IndicatorValue::Single(if (high - low).abs() < 1e-15 {
                        50.0
                    } else {
                        (bar.close - low) / (high - low) * 100.0
                    }),
                }
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
        if !colors.is_empty() {
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
        vec!["52W HL%".to_string()]
    }
}

/// Highest high and lowest low over the inclusive last 364 days, drawn on the price pane.
/// Bars before 52 weeks of history have no value, regardless of the bar interval.
#[derive(Clone)]
pub struct Week52HighLowLevels {
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLowLevels {
    /// Create a 52-week high and low study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![palette::BLUE, palette::PINK],
            visible: true,
        }
    }
}

impl Default for Week52HighLowLevels {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Week52HighLowLevels {
    fn name(&self) -> &str {
        "52W High/Low"
    }

    fn desc(&self) -> &str {
        "52-Week High/Low - Highest high and lowest low within the last 364 days"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() {
                    return IndicatorValue::None;
                }
                match state.next(bar) {
                    Some((high, low)) => IndicatorValue::Multiple(vec![high, low]),
                    None => IndicatorValue::None,
                }
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
        for (kept, color) in self.colors.iter_mut().zip(colors) {
            *kept = color;
        }
    }

    fn is_overlay(&self) -> bool {
        true
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
        vec!["52W High".to_string(), "52W Low".to_string()]
    }
}

#[derive(Clone, Default)]
struct State {
    first: Option<chrono::DateTime<chrono::Utc>>,
    highs: VecDeque<(chrono::DateTime<chrono::Utc>, f64)>,
    lows: VecDeque<(chrono::DateTime<chrono::Utc>, f64)>,
}

impl State {
    /// Include the bar exactly 52 weeks before this one, expiring only older candidates.
    fn next(&mut self, bar: &Bar) -> Option<(f64, f64)> {
        let first = *self.first.get_or_insert(bar.time);
        while self
            .highs
            .front()
            .is_some_and(|&(time, _)| bar.time - time > Duration::weeks(52))
        {
            self.highs.pop_front();
        }
        while self
            .lows
            .front()
            .is_some_and(|&(time, _)| bar.time - time > Duration::weeks(52))
        {
            self.lows.pop_front();
        }
        while self
            .highs
            .back()
            .is_some_and(|&(_, price)| price <= bar.high)
        {
            self.highs.pop_back();
        }
        while self.lows.back().is_some_and(|&(_, price)| price >= bar.low) {
            self.lows.pop_back();
        }
        self.highs.push_back((bar.time, bar.high));
        self.lows.push_back((bar.time, bar.low));
        (bar.time - first >= Duration::weeks(52)).then(|| (self.highs[0].1, self.lows[0].1))
    }
}
