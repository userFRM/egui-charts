use super::over_finite_bars;
use crate::model::Bar;
/// Volume Profile Visible Range
/// Distributes volume across price bins within the visible range
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

#[derive(Clone)]
pub struct VolProfileVisible {
    row_count: usize,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl VolProfileVisible {
    /// Create a profile with `row_count` price rows. Every bar carries all rows,
    /// so a row count above 10,000 is refused and yields `None` for every bar.
    pub fn new(row_count: usize) -> Self {
        Self {
            row_count: row_count.max(1),
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(41, 98, 255)], // Accent blue
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite high, low, close and volume.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.is_empty() {
            return values;
        }
        if self.row_count > super::MAX_PROFILE_ROWS {
            values.resize(data.len(), IndicatorValue::None);
            return values;
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
            let zeros = vec![0.0; self.row_count];
            for _ in data {
                values.push_lines(&zeros);
            }
            return values;
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

            values.push(IndicatorValue::Multiple(cumulative_bins.clone()));
        }

        values
    }
}

impl Default for VolProfileVisible {
    fn default() -> Self {
        Self::new(24)
    }
}

impl Indicator for VolProfileVisible {
    crate::studies::inputs::study_inputs!(self;
        row_count: length("Row count", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Vol Profile VR"
    }

    fn desc(&self) -> &str {
        "Volume Profile Visible Range - Volume distribution across price levels"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| {
                b.high.is_finite()
                    && b.low.is_finite()
                    && b.close.is_finite()
                    && b.volume.is_finite()
            },
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
        true
    }

    fn line_cnt(&self) -> usize {
        0 // A histogram of volumes per bar, not a price line (#167)
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
