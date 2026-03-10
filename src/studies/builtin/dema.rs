//! Double Exponential Moving Avg (DEMA) Indicator
//!
//! DEMA reduces the lag inherent in EMAs by applying EMA twice and
//! then subtracting a smoothed EMA.
//!
//! # Formula
//! DEMA = 2 * EMA(price, period) - EMA(EMA(price, period), period)
//!
//! # Example
//! ```ignore
//! use egui_charts::DoubleEMA;
//!
//! let mut dema = DoubleEMA::new(14);
//! dema.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Double Exponential Moving Avg indicator
#[derive(Clone)]
pub struct DEMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl DEMA {
    /// Create a new DEMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.cyan, // Cyan
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate EMA for a slice of values
    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        let mut ema = vec![0.0; data.len()];
        if data.is_empty() || period == 0 {
            return ema;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);

        // Initial EMA is SMA of first 'period' values
        if data.len() >= period {
            let initial_sum: f64 = data[..period].iter().sum();
            ema[period - 1] = initial_sum / period as f64;

            // Calculate subsequent EMA values
            for i in period..data.len() {
                ema[i] = (data[i] - ema[i - 1]) * multiplier + ema[i - 1];
            }
        }

        ema
    }
}

impl Indicator for DEMA {
    fn name(&self) -> &str {
        "DEMA"
    }

    fn desc(&self) -> &str {
        "Double Exponential Moving Avg - Reduces lag compared to EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Need 2 * period - 1 bars for valid DEMA
        let min_period = 2 * self.period - 1;

        // Extract close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate first EMA
        let ema1 = Self::calculate_ema(&closes, self.period);

        // Calculate EMA of EMA (starting from where first EMA is valid)
        let ema2 = Self::calculate_ema(&ema1, self.period);

        // Calculate DEMA = 2 * EMA1 - EMA2
        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let dema = 2.0 * ema1[i] - ema2[i];
                self.values.push(IndicatorValue::Single(dema));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        true
    }

    fn line_cnt(&self) -> usize {
        1
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
        vec![format!("DEMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 100.0 + i as f64;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_dema_calculation() {
        let bars = create_test_bars();
        let mut dema = DEMA::new(10);
        dema.calculate(&bars);

        assert_eq!(dema.values().len(), bars.len());

        // First 2*period - 2 values should be None
        let warmup = 2 * 10 - 2;
        for i in 0..warmup {
            assert!(matches!(dema.values()[i], IndicatorValue::None));
        }

        // Check remaining values are valid
        for value in dema.values().iter().skip(warmup) {
            if let IndicatorValue::Single(v) = value {
                assert!(*v > 0.0);
            }
        }
    }

    #[test]
    fn test_dema_less_lag_than_ema() {
        // In a trending market, DEMA should be closer to current price than EMA
        let bars = create_test_bars();
        let mut dema = DEMA::new(10);
        dema.calculate(&bars);

        // Get last DEMA value
        if let Some(IndicatorValue::Single(dema_val)) = dema.values().last() {
            let last_close = bars.last().unwrap().close;
            // DEMA should be relatively close to current price in uptrend
            assert!(
                (*dema_val - last_close).abs() < 15.0,
                "DEMA should track price closely"
            );
        }
    }

    #[test]
    fn test_dema_is_overlay() {
        let dema = DEMA::new(14);
        assert!(dema.is_overlay());
    }

    #[test]
    fn test_dema_empty_data() {
        let mut dema = DEMA::new(14);
        dema.calculate(&[]);
        assert!(dema.values().is_empty());
    }
}
