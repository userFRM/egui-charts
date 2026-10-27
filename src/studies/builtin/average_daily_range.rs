//! Average Daily Range (ADR)
//!
//! The average distance between each bar's high and low over the last `length` bars. Unlike the
//! true range of [`ATR`](super::ATR), it ignores gaps from the previous close. On a daily chart
//! it is the average daily range; on other intervals, the average range of that interval's bars.
//!
//! # Formula
//! ```text
//! ADR = SMA(high - low, length)
//! ```
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AverageDailyRange, Indicator};
//!
//! let mut adr = AverageDailyRange::new(14);
//! adr.calculate(&bars);
//! assert_eq!(adr.values().value(0, 13), Some(2.0));
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{rolling_mean, update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Simple average of the bar range.
#[derive(Clone)]
pub struct AverageDailyRange {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl AverageDailyRange {
    /// Create an Average Daily Range over `period` bars (conventionally 14).
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }
}

impl AverageDailyRange {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let ranges: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();
        rolling_mean(&ranges, self.period)
            .into_iter()
            .map(IndicatorValue::Single)
            .collect()
    }
}

impl Default for AverageDailyRange {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for AverageDailyRange {
    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "Average Daily Range"
    }

    fn desc(&self) -> &str {
        "Average Daily Range - Simple average of the high-low range"
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
            usable_behind(self.period - 1, |b| b.high.is_finite() && b.low.is_finite()),
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
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
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
        vec![format!("ADR({})", self.period)]
    }
}
