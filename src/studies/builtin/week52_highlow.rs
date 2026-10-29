use crate::model::Bar;
use crate::studies::palette;
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
    state: State,
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLow {
    /// Create a 52-week high/low percentage study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            state: State::default(),
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
        self.state.run(
            data,
            from,
            &mut self.values,
            |bar| [bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()),
            |bar, high, low| {
                IndicatorValue::Single(if high == low {
                    50.0
                } else {
                    (bar.close - low) / (high - low) * 100.0
                })
            },
        );
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
    state: State,
    colors: Vec<Color32>,
    visible: bool,
}

impl Week52HighLowLevels {
    /// Create a 52-week high and low study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            state: State::default(),
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
        self.state.run(
            data,
            from,
            &mut self.values,
            |bar| bar.high.is_finite() && bar.low.is_finite(),
            |_, high, low| IndicatorValue::Multiple(vec![high, low]),
        );
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
    processed: usize,
    first: Option<chrono::DateTime<chrono::Utc>>,
    extremes: [VecDeque<usize>; 2],
}

impl State {
    fn run(
        &mut self,
        data: &[Bar],
        from: usize,
        values: &mut IndicatorValues,
        usable: impl Fn(&Bar) -> bool,
        output: impl Fn(&Bar, f64, f64) -> IndicatorValue,
    ) {
        let len = values.len();
        let from = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            *self = Self::default();
            0
        };
        values.truncate(from);
        // Commit completed bars only. Revisions of the developing bar leave the queues intact.
        for at in self.processed..data.len() {
            let bar = &data[at];
            let last = at + 1 == data.len();
            let value = if usable(bar) {
                let first = self.first.unwrap_or(bar.time);
                let mut range = [bar.high, bar.low];
                for (line, queue) in self.extremes.iter_mut().enumerate() {
                    let expired = |i: usize| bar.time - data[i].time > Duration::weeks(52);
                    let price = |i: usize| if line == 0 { data[i].high } else { data[i].low };
                    let better = |a: f64, b: f64| if line == 0 { a > b } else { a < b };
                    if !last {
                        while queue.front().is_some_and(|&i| expired(i)) {
                            queue.pop_front();
                        }
                        while queue.back().is_some_and(|&i| !better(price(i), price(at))) {
                            queue.pop_back();
                        }
                        queue.push_back(at);
                    }
                    // Include the bar exactly 52 weeks before this one.
                    let start = queue.partition_point(|&i| expired(i));
                    if let Some(&i) = queue.get(start)
                        && better(price(i), range[line])
                    {
                        range[line] = price(i);
                    }
                }
                if !last {
                    self.first = Some(first);
                }
                if bar.time - first >= Duration::weeks(52) {
                    output(bar, range[0], range[1])
                } else {
                    IndicatorValue::None
                }
            } else {
                IndicatorValue::None
            };
            if at >= from {
                values.push(value);
            }
            if !last {
                self.processed = at + 1;
            }
        }
    }
}
