//! Aroon Indicator
//!
//! Aroon identifies trend changes and trend strength by measuring the time
//! since the highest high and lowest low within a lookback period.
//!
//! # Components
//! - Aroon Up: ((period - days since highest high) / period) * 100
//! - Aroon Down: ((period - days since lowest low) / period) * 100
//! - Aroon Oscillator: Aroon Up - Aroon Down (optional)
//!
//! # Interpretation
//! - Aroon Up > 70 and Aroon Down < 30: Strong uptrend
//! - Aroon Down > 70 and Aroon Up < 30: Strong downtrend
//! - Crossovers indicate potential trend changes
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Aroon, Indicator};
//!
//! let mut aroon = Aroon::new(25);
//! aroon.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Aroon indicator
#[derive(Clone)]
pub struct Aroon {
    period: usize,
    values: IndicatorValues,
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

impl Aroon {
    /// Create a new Aroon indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 25)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            up_color: palette::DARK_ORANGE,
            down_color: palette::BLUE,
            visible: true,
        }
    }

    /// Create with default param (25)
    pub fn default_params() -> Self {
        Self::new(25)
    }

    /// Set colors for up and down lines
    pub fn with_colors(mut self, up: Color32, down: Color32) -> Self {
        self.up_color = up;
        self.down_color = down;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for Aroon {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Aroon {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            // Find highest high and lowest low positions in lookback
            let start = i - self.period;
            let mut highest_idx = start;
            let mut lowest_idx = start;
            let mut highest = data[start].high;
            let mut lowest = data[start].low;

            for j in (start + 1)..=i {
                if data[j].high >= highest {
                    highest = data[j].high;
                    highest_idx = j;
                }
                if data[j].low <= lowest {
                    lowest = data[j].low;
                    lowest_idx = j;
                }
            }

            // Days since highest/lowest
            let days_since_high = i - highest_idx;
            let days_since_low = i - lowest_idx;

            // Calculate Aroon Up and Down (0-100 scale)
            let aroon_up = ((self.period - days_since_high) as f64 / self.period as f64) * 100.0;
            let aroon_down = ((self.period - days_since_low) as f64 / self.period as f64) * 100.0;

            values.push_lines(&[aroon_up, aroon_down]);
        }

        values
    }
}

impl Indicator for Aroon {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Aroon"
    }

    fn desc(&self) -> &str {
        "Aroon - Trend identification indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period, |b| b.high.is_finite() && b.low.is_finite()),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite(),
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.up_color, self.down_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.up_color = colors[0];
        }
        if colors.len() > 1 {
            self.down_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false // Aroon is a separate oscillator pane
    }

    fn line_count(&self) -> usize {
        2 // Aroon Up and Aroon Down
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
        vec![
            format!("Aroon Up({})", self.period),
            format!("Aroon Down({})", self.period),
        ]
    }
}
