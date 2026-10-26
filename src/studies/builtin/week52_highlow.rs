use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
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
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLow {
    /// Create a 52-week high/low percentage study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.pink],
            visible: true,
        }
    }
}

impl Default for Week52HighLow {
    fn default() -> Self {
        Self::new()
    }
}

impl Week52HighLow {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        week52_ranges(data)
            .into_iter()
            .zip(data)
            .map(|(range, bar)| match range {
                None => IndicatorValue::None,
                Some((high, low)) => IndicatorValue::Single(if (high - low).abs() < 1e-15 {
                    50.0
                } else {
                    (bar.close - low) / (high - low) * 100.0
                }),
            })
            .collect()
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
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLowLevels {
    /// Create a 52-week high and low study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,
                DESIGN_TOKENS.semantic.extended.pink,
            ],
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
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| {
                week52_ranges(data)
                    .into_iter()
                    .map(|range| match range {
                        Some((high, low)) => IndicatorValue::Multiple(vec![high, low]),
                        None => IndicatorValue::None,
                    })
                    .collect()
            },
        );
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

/// Window extrema in linear time, including a bar exactly 52 weeks before the current one.
fn week52_ranges(data: &[Bar]) -> Vec<Option<(f64, f64)>> {
    let mut highs = VecDeque::<usize>::new();
    let mut lows = VecDeque::<usize>::new();
    data.iter()
        .enumerate()
        .map(|(i, bar)| {
            while highs
                .front()
                .is_some_and(|&j| bar.time - data[j].time > Duration::weeks(52))
            {
                highs.pop_front();
            }
            while lows
                .front()
                .is_some_and(|&j| bar.time - data[j].time > Duration::weeks(52))
            {
                lows.pop_front();
            }
            while highs.back().is_some_and(|&j| data[j].high <= bar.high) {
                highs.pop_back();
            }
            while lows.back().is_some_and(|&j| data[j].low >= bar.low) {
                lows.pop_back();
            }
            highs.push_back(i);
            lows.push_back(i);
            (bar.time - data[0].time >= Duration::weeks(52))
                .then(|| (data[highs[0]].high, data[lows[0]].low))
        })
        .collect()
}
