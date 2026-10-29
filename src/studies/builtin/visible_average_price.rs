//! Arithmetic mean of a price source over the bars currently visible on the chart.

use crate::model::{Bar, PriceSource};
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;
use std::ops::Range;

/// Average of the visible bars' closes, recalculated when the chart pans or zooms.
#[derive(Clone)]
pub struct VisibleAveragePrice {
    source: PriceSource,
    range: Range<usize>,
    values: IndicatorValues,
    mean: Option<f64>,
    color: Color32,
    visible: bool,
}

impl VisibleAveragePrice {
    /// A close-price average, over all loaded bars until a visible range is supplied.
    pub fn new() -> Self {
        Self {
            source: PriceSource::Close,
            range: 0..usize::MAX,
            values: IndicatorValues::new(),
            mean: None,
            color: palette::GRAY,
            visible: true,
        }
    }
}

impl Default for VisibleAveragePrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VisibleAveragePrice {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(self.source))
    }

    crate::studies::inputs::study_inputs!(self;
        source: choice("Source", PriceSource::all())
    ; {});

    fn name(&self) -> &str {
        "Visible Average Price"
    }
    fn desc(&self) -> &str {
        "Arithmetic mean of the visible bars' price source"
    }
    fn set_visible_range(&mut self, range: Range<usize>) -> bool {
        let changed = self.range != range;
        self.range = range;
        changed
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
        let end = self.range.end.min(data.len());
        let start = self.range.start.min(end);
        let price = |bar: &Bar| self.source.compute(bar.open, bar.high, bar.low, bar.close);
        let (mut mean, mut count) = (0.0, 0.0);
        for value in data[start..end].iter().map(price).filter(|v| v.is_finite()) {
            count += 1.0;
            mean = mean * ((count - 1.0) / count) + value / count;
        }
        self.mean = (count != 0.0).then_some(mean);
        if count != 0.0 {
            for (i, bar) in data.iter().enumerate() {
                if price(bar).is_finite() {
                    self.values.set_value(0, i, mean);
                }
            }
        }
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        let start = from.min(self.values.len()).min(data.len());
        if start < self.range.end || from == 0 {
            // A changed visible price moves the average on every bar carrying that level.
            self.calculate(data);
            return;
        }
        self.values.truncate(start);
        for bar in &data[start..] {
            let price = self.source.compute(bar.open, bar.high, bar.low, bar.close);
            self.values.push_lines(&[if price.is_finite() {
                self.mean.unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }]);
        }
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(color) = colors.first() {
            self.color = *color;
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
        vec!["Avg Price".into()]
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Bar;
    use crate::studies::*;
    use chrono::{TimeZone, Utc};

    /// The visible mean uses exactly the supplied bars and changes with the viewport.
    #[test]
    fn visible_average_follows_the_visible_range() {
        let bars: Vec<_> = [2.0, 4.0, 12.0, 20.0]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), c, c, c, c, 1.0))
            .collect();
        let mut study = ConfiguredStudy::new(
            IndicatorFactory::new()
                .create("Visible Average Price")
                .expect("Visible Average Price study"),
        );
        for (range, expected) in [(0..2, Some(3.0)), (1..4, Some(12.0)), (4..99, None)] {
            study.set_visible_range(range.clone());
            study.calculate(&bars);
            for i in 0..bars.len() {
                assert_eq!(study.values().value(0, i), expected);
            }
        }
        #[cfg(feature = "widget")]
        {
            use crate::widget::{Chart, test_frames::frame};
            let bars: Vec<_> = (0..100)
                .map(|i| {
                    let c = i as f64;
                    Bar::new(Utc.timestamp_opt(i, 0).unwrap(), c, c, c, c, 1.0)
                })
                .collect();
            let mut chart = Chart::new(crate::model::BarData::from_bars(bars));
            use crate::studies::{InputValue, StudySource};
            let source = chart.add_indicator(Box::new(SMA::new(1)));
            let average = chart.add_indicator(Box::new(study));
            chart
                .set_indicator_source(
                    average,
                    StudySource::Study {
                        id: source.0,
                        plot: 0,
                    },
                )
                .unwrap();
            let consumer = chart.add_indicator(Box::new(SMA::new(1)));
            chart
                .set_indicator_source(
                    consumer,
                    StudySource::Study {
                        id: average.0,
                        plot: 0,
                    },
                )
                .unwrap();
            assert!(
                chart
                    .indicators()
                    .find(|(id, _)| *id == average)
                    .unwrap()
                    .1
                    .inputs()
                    .iter()
                    .any(|input| input.id == "source"
                        && matches!(input.value, InputValue::Source(_)))
            );
            let ctx = egui::Context::default();
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.set_visible_bars(20);
            for (time, start) in [(1.0, 10), (2.0, 50)] {
                chart.set_start_idx(start);
                frame(&ctx, &mut chart, time, Vec::new());
                let (start, end) = chart.state().visible_range();
                let expected = (start + end - 1) as f64 / 2.0;
                for id in [average, consumer] {
                    let study = chart
                        .indicators()
                        .find(|(candidate, _)| *candidate == id)
                        .unwrap()
                        .1;
                    assert!((study.values().value(0, start).unwrap() - expected).abs() < 1e-10);
                }
            }
        }
    }
}
