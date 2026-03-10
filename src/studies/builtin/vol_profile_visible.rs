use crate::model::Bar;
/// Volume Profile Visible Range
/// Distributes volume across price bins within the visible range
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct VolProfileVisible {
    row_count: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl VolProfileVisible {
    pub fn new(row_count: usize) -> Self {
        Self {
            row_count,
            values: Vec::new(),
            colors: vec![Color32::from_rgb(41, 98, 255)], // Accent blue
            visible: true,
        }
    }
}

impl Default for VolProfileVisible {
    fn default() -> Self {
        Self::new(24)
    }
}

impl Indicator for VolProfileVisible {
    fn name(&self) -> &str {
        "Vol Profile VR"
    }

    fn desc(&self) -> &str {
        "Volume Profile Visible Range - Volume distribution across price levels"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() || self.row_count == 0 {
            return;
        }

        // Find price range across all bars
        let mut price_min = f64::MAX;
        let mut price_max = f64::MIN;
        for bar in data {
            if bar.low < price_min {
                price_min = bar.low;
            }
            if bar.high > price_max {
                price_max = bar.high;
            }
        }

        let range = price_max - price_min;
        if range <= 0.0 {
            for _ in data {
                self.values
                    .push(IndicatorValue::Multiple(vec![0.0; self.row_count]));
            }
            return;
        }

        let bin_size = range / self.row_count as f64;

        // Accumulate volume into bins incrementally
        let mut cumulative_bins = vec![0.0_f64; self.row_count];

        for bar in data {
            // Assign bar volume to the bin closest to the bar's typical price
            let typical = (bar.high + bar.low + bar.close) / 3.0;
            let bin_idx = ((typical - price_min) / bin_size).floor() as usize;
            let bin_idx = bin_idx.min(self.row_count - 1);
            cumulative_bins[bin_idx] += bar.volume;

            self.values
                .push(IndicatorValue::Multiple(cumulative_bins.clone()));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!("Vol Profile VR({})", self.row_count)]
    }
}
