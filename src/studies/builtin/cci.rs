//! Commodity Channel Index (CCI) Indicator
//!
//! CCI measures the current price level relative to an avg price level over
//! a given period. It oscillates above and below zero, helping identify cyclical
//! trends in a commodity or stock.
//!
//! # Interpretation
//! - Readings above +100: Overbought, potential reversal
//! - Readings below -100: Oversold, potential reversal
//! - Zero crossings can signal trend changes
//!
//! # Calculation
//! Typical Price (TP) = (High + Low + Close) / 3
//! SMA = Simple Moving Avg of TP
//! Mean Deviation = Avg of |TP - SMA|
//! CCI = (TP - SMA) / (0.015 * Mean Deviation)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{CCI, Indicator};
//!
//! let mut cci = CCI::new(20);
//! cci.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Commodity Channel Index indicator
#[derive(Clone)]
pub struct CCI {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl CCI {
    /// Create a new CCI indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::PURPLE,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate typical price (TP) for a bar
    #[inline]
    fn typical_price(bar: &Bar) -> f64 {
        (bar.high + bar.low + bar.close) / 3.0
    }
}

/// Construct with the conventional default parameters.
impl Default for CCI {
    fn default() -> Self {
        Self::new(20)
    }
}

impl CCI {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate typical prices
        let tp: Vec<f64> = data.iter().map(Self::typical_price).collect();

        for i in 0..data.len() {
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
            } else {
                // Calculate SMA of typical price
                let start = i + 1 - self.period;
                let sma: f64 = tp[start..=i].iter().sum::<f64>() / self.period as f64;

                // Calculate mean deviation
                let mean_dev: f64 =
                    tp[start..=i].iter().map(|p| (p - sma).abs()).sum::<f64>() / self.period as f64;

                // Calculate CCI
                if mean_dev.abs() < 1e-10 {
                    values.push(IndicatorValue::Single(0.0));
                } else {
                    // Lambert's constant 0.015 ensures ~70-80% of values fall between -100 and +100
                    let cci = (tp[i] - sma) / (0.015 * mean_dev);
                    values.push(IndicatorValue::Single(cci));
                }
            }
        }
        values
    }
}

impl Indicator for CCI {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HLC3,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "CCI"
    }

    fn desc(&self) -> &str {
        "Commodity Channel Index - Cyclical trend indicator"
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
            |_data, from| from.saturating_sub(self.period - 1),
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
        false // CCI is drawn in separate pane
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
        vec![format!("CCI({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_cci_flat_prices() {
        // A zero mean deviation is reported as 0 rather than dividing by zero.
        let start = Utc::now();
        let bars: Vec<Bar> = (0..30)
            .map(|i| Bar {
                time: start + Duration::minutes(i * 5),
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1000.0,
                colors: None,
                payload: None,
            })
            .collect();

        let mut cci = CCI::new(20);
        cci.calculate(&bars);

        for value in cci.values().to_rows().iter().skip(19) {
            assert!(
                matches!(value, IndicatorValue::Single(v) if *v == 0.0),
                "a window without deviation reads 0, got {value:?}"
            );
        }
    }
}
