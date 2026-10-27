//! Bollinger Bars
//!
//! John Bollinger's Bollinger Bars redraw each price bar at full candle width, so the whole
//! range traded in the period reads as a block: the body between open and close is coloured
//! up (close at or above open) or down, and the parts above and below it, the wicks, are
//! drawn at body width in a third colour, blue by default.
//!
//! [`Indicator::candles`] holds two candles per bar: first a block from the high to the low on
//! plot 0 ("Wicks"), then the body from the open to the close on plot 1 ("Up") or 2 ("Down"),
//! painted over the block. The output columns hold each bar's close in the up or down column.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{BollingerBars, Indicator};
//!
//! let mut bars_study = BollingerBars::new();
//! bars_study.calculate(&bars);
//! assert_eq!(bars_study.candles().len(), 2 * bars.len());
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyCandle};
use egui::Color32;

/// Bollinger Bars: full-width wick blocks under up or down bodies.
#[derive(Clone)]
pub struct BollingerBars {
    candles: Vec<StudyCandle>,
    values: IndicatorValues,
    colors: [Color32; 3],
    visible: bool,
}

impl BollingerBars {
    /// Create the study with blue wicks and the chart's up and down colours.
    pub fn new() -> Self {
        Self {
            candles: Vec::new(),
            values: IndicatorValues::new(),
            colors: [palette::ACCENT, palette::UP, palette::DOWN],
            visible: true,
        }
    }
}

/// Construct with the conventional colours.
impl Default for BollingerBars {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for BollingerBars {
    fn name(&self) -> &str {
        "Bollinger Bars"
    }

    fn desc(&self) -> &str {
        "Bollinger Bars - Full-width bodies with block wicks"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        self.candles.truncate(
            self.candles
                .partition_point(|candle| candle.bars.start < from),
        );
        self.values.truncate(from);
        // Every bar has all three columns, so an all-up series still reports three lines.
        self.values
            .resize(data.len(), IndicatorValue::Multiple(vec![f64::NAN; 3]));
        for (i, bar) in data.iter().enumerate().skip(from) {
            if ![bar.open, bar.high, bar.low, bar.close]
                .iter()
                .all(|p| p.is_finite())
            {
                continue;
            }
            let plot = if bar.close >= bar.open { 1 } else { 2 };
            self.values.set_value(plot, i, bar.close);
            let (top, bottom) = (bar.open.max(bar.close), bar.open.min(bar.close));
            self.candles.push(StudyCandle {
                bars: i..i + 1,
                bar: Bar {
                    open: bar.high,
                    close: bar.low,
                    ..bar.clone()
                },
                plot: 0,
            });
            self.candles.push(StudyCandle {
                bars: i..i + 1,
                bar: Bar {
                    high: top,
                    low: bottom,
                    ..bar.clone()
                },
                plot,
            });
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn candles(&self) -> &[StudyCandle] {
        &self.candles
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
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
        vec!["Wicks".into(), "Up".into(), "Down".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Each bar paints its high-low block before its body, the body spans only open to close,
    /// an unchanged close counts as up, and a bar with a non-finite price draws nothing.
    #[test]
    fn each_bar_paints_a_block_then_its_body() {
        let bar = |i: i64, [o, h, l, c]: [f64; 4]| {
            Bar::new(Utc.timestamp_opt(i, 0).unwrap(), o, h, l, c, 1.0)
        };
        let data = [
            bar(0, [10.0, 12.0, 9.0, 11.0]),
            bar(1, [11.0, 11.5, 8.0, 9.0]),
            bar(2, [f64::NAN, 11.0, 9.0, 10.0]),
            bar(3, [10.0, 10.5, 9.5, 10.0]),
        ];
        let mut study = BollingerBars::new();
        study.calculate(&data);
        let candles: Vec<_> = study
            .candles()
            .iter()
            .map(|c| {
                (
                    c.bars.clone(),
                    c.plot,
                    [c.bar.open, c.bar.high, c.bar.low, c.bar.close],
                )
            })
            .collect();
        assert_eq!(
            candles,
            [
                (0..1, 0, [12.0, 12.0, 9.0, 9.0]),
                (0..1, 1, [10.0, 11.0, 10.0, 11.0]),
                (1..2, 0, [11.5, 11.5, 8.0, 8.0]),
                (1..2, 2, [11.0, 11.0, 9.0, 9.0]),
                (3..4, 0, [10.5, 10.5, 9.5, 9.5]),
                (3..4, 1, [10.0, 10.0, 10.0, 10.0]),
            ]
        );
    }
}
