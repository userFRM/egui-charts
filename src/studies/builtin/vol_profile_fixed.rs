use super::over_finite_bars;
use crate::model::Bar;
/// Volume Profile Fixed Range
/// Distributes volume across price bins using fixed price boundaries
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

#[derive(Clone)]
pub struct VolProfileFixed {
    row_count: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl VolProfileFixed {
    /// Create a profile with `row_count` price rows. Every bar carries all rows,
    /// so a row count above 10,000 is refused and yields `None` for every bar.
    pub fn new(row_count: usize) -> Self {
        Self {
            row_count: row_count.max(1),
            values: Vec::new(),
            colors: vec![Color32::from_rgb(41, 98, 255)], // Accent blue
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite high, low and volume.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();

        if data.is_empty() {
            return values;
        }
        if self.row_count > super::MAX_PROFILE_ROWS {
            values.resize(data.len(), IndicatorValue::None);
            return values;
        }

        // Use the full dataset price range as fixed boundaries
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
                values.push(IndicatorValue::Multiple(vec![0.0; self.row_count]));
            }
            return values;
        }

        let bin_size = range / self.row_count as f64;

        // Accumulate volume into fixed bins incrementally
        let mut cumulative_bins = vec![0.0_f64; self.row_count];

        for bar in data {
            // Distribute volume: assign to bins that the bar's range covers
            let lo_bin = ((bar.low - price_min) / bin_size).floor() as usize;
            let hi_bin = ((bar.high - price_min) / bin_size).floor() as usize;
            let lo_bin = lo_bin.min(self.row_count - 1);
            let hi_bin = hi_bin.min(self.row_count - 1);

            // A NaN or inverted bar has hi_bin < lo_bin and covers no bins.
            let covered = (hi_bin.saturating_sub(lo_bin) + 1) as f64;
            let vol_per_bin = bar.volume / covered;

            for bin in lo_bin..=hi_bin {
                cumulative_bins[bin] += vol_per_bin;
            }

            values.push(IndicatorValue::Multiple(cumulative_bins.clone()));
        }

        values
    }
}

impl Default for VolProfileFixed {
    fn default() -> Self {
        Self::new(24)
    }
}

impl Indicator for VolProfileFixed {
    fn name(&self) -> &str {
        "Vol Profile FR"
    }

    fn desc(&self) -> &str {
        "Volume Profile Fixed Range - Volume distribution with fixed price boundaries"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.volume.is_finite(),
            |data| self.compute(data),
        );
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
        vec![format!("Vol Profile FR({})", self.row_count)]
    }
}
