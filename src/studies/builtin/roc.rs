//! Rate of Change (ROC) Indicator
//!
//! ROC is a momentum oscillator that measures the percentage change in price
//! between the current price and the price N periods ago.
//!
//! # Interpretation
//! - Positive ROC: Price is higher than N periods ago (bullish)
//! - Negative ROC: Price is lower than N periods ago (bearish)
//! - Zero crossings can signal trend changes
//! - Extreme readings may indicate overbought/oversold conditions
//!
//! # Calculation
//! ROC = ((Close - Close[n]) / Close[n]) * 100
//! A previous close of exactly zero has no defined rate of change.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{RateOfChange, Indicator};
//!
//! let mut roc = RateOfChange::new(12);
//! roc.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Rate of Change indicator
#[derive(Clone)]
pub struct RateOfChange {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl RateOfChange {
    /// Create a new ROC indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 12 or 25)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for RateOfChange {
    fn default() -> Self {
        Self::new(9)
    }
}

impl RateOfChange {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() <= self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
            } else {
                let prev_close = data[i - self.period].close;
                if prev_close == 0.0 {
                    values.push(IndicatorValue::None);
                } else {
                    let roc = ((data[i].close - prev_close) / prev_close) * 100.0;
                    values.push(IndicatorValue::Single(roc));
                }
            }
        }
        values
    }
}

impl Indicator for RateOfChange {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "ROC"
    }

    fn desc(&self) -> &str {
        "Rate of Change - Momentum oscillator (percentage)"
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
            |_data, from| from.saturating_sub(self.period),
            |data| self.compute(data),
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
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn is_overlay(&self) -> bool {
        false // ROC is drawn in separate pane
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
        vec![format!("ROC({})", self.period)]
    }
}
