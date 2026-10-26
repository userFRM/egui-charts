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
            color: palette::BLUE,
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
        if count != 0.0 {
            for (i, bar) in data.iter().enumerate() {
                if price(bar).is_finite() {
                    self.values.set_value(0, i, mean);
                }
            }
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
