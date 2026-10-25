//! Momentum (MOM) Indicator
//!
//! Momentum measures the price change between the current close and the close
//! N periods ago, in price units. Unlike Rate of Change it is not normalised
//! to a percentage, so its scale follows the instrument's price.
//!
//! # Calculation
//! MOM = Close - Close[n]

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Momentum indicator
#[derive(Clone)]
pub struct Momentum {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl Momentum {
    /// Create a new Momentum indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 10)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.info,
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for Momentum {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Indicator for Momentum {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "Momentum"
    }

    fn desc(&self) -> &str {
        "Momentum - Price change over the period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = (0..data.len())
            .map(|i| match i.checked_sub(self.period) {
                Some(prev) => IndicatorValue::Single(data[i].close - data[prev].close),
                None => IndicatorValue::None,
            })
            .collect();
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
        vec![format!("MOM({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn matches_reference_values() {
        let closes = [10.0, 10.5, 11.25, 10.75, 12.0, 11.5, 13.25, 12.5];
        let start = Utc::now();
        let bars: Vec<Bar> = closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar {
                time: start + Duration::minutes(i as i64),
                open: c,
                high: c,
                low: c,
                close: c,
                volume: 1.0,
                colors: None,
                payload: None,
            })
            .collect();

        let mut momentum = Momentum::new(3);
        momentum.calculate(&bars);

        // close[i] - close[i - 3], computed independently.
        let expected = [
            None,
            None,
            None,
            Some(0.75),
            Some(1.5),
            Some(0.25),
            Some(2.5),
            Some(0.5),
        ];
        let actual: Vec<Option<f64>> = momentum
            .values()
            .to_rows()
            .iter()
            .map(|v| match v {
                IndicatorValue::Single(x) => Some(*x),
                _ => None,
            })
            .collect();
        assert_eq!(actual, expected);
    }
}
