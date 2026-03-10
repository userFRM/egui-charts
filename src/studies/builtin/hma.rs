//! Hull Moving Avg (HMA) Indicator
//!
//! HMA is a fast, smooth moving avg developed by Alan Hull that
//! significantly reduces lag while maintaining smoothness.
//!
//! # Formula
//! HMA = WMA(2 * WMA(price, n/2) - WMA(price, n), sqrt(n))
//!
//! # Example
//! ```ignore
//! use egui_charts::HMA;
//!
//! let mut hma = HMA::new(9);
//! hma.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Hull Moving Avg indicator
#[derive(Clone)]
pub struct HMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl HMA {
    /// Create a new HMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation (typically 9, 14, or 21)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.deep_purple, // Deep Purple
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate WMA for a slice of values
    fn calculate_wma(data: &[f64], period: usize) -> Vec<f64> {
        let mut wma = vec![f64::NAN; data.len()];
        if data.is_empty() || period == 0 {
            return wma;
        }

        let weight_sum = (period * (period + 1)) / 2;

        for i in (period - 1)..data.len() {
            let mut weighted_sum = 0.0;
            for j in 0..period {
                let weight = (period - j) as f64;
                weighted_sum += data[i - j] * weight;
            }
            wma[i] = weighted_sum / weight_sum as f64;
        }

        wma
    }
}

impl Indicator for HMA {
    fn name(&self) -> &str {
        "HMA"
    }

    fn desc(&self) -> &str {
        "Hull Moving Avg - Fast and smooth moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let half_period = (self.period / 2).max(1);
        let sqrt_period = (self.period as f64).sqrt() as usize;
        let sqrt_period = sqrt_period.max(1);

        // Extract close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Step 1: Calculate WMA(close, period/2)
        let wma_half = Self::calculate_wma(&closes, half_period);

        // Step 2: Calculate WMA(close, period)
        let wma_full = Self::calculate_wma(&closes, self.period);

        // Step 3: Calculate 2 * WMA(half) - WMA(full)
        let mut raw_hma = vec![f64::NAN; data.len()];
        for i in 0..data.len() {
            if !wma_half[i].is_nan() && !wma_full[i].is_nan() {
                raw_hma[i] = 2.0 * wma_half[i] - wma_full[i];
            }
        }

        // Step 4: Calculate WMA(raw_hma, sqrt(period))
        let hma = Self::calculate_wma(&raw_hma, sqrt_period);

        // Build output
        let min_period = self.period + sqrt_period - 1;
        for i in 0..data.len() {
            if i < min_period - 1 || hma[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(hma[i]));
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
        vec![format!("HMA({})", self.period)]
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
    fn test_hma_calculation() {
        let bars = create_test_bars();
        let mut hma = HMA::new(9);
        hma.calculate(&bars);

        assert_eq!(hma.values().len(), bars.len());

        // Check that we eventually get valid values
        let valid_cnt = hma
            .values()
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();
        assert!(valid_cnt > 0, "Should have some valid values");
    }

    #[test]
    fn test_hma_responsiveness() {
        // HMA should be very responsive to price changes
        let bars = create_test_bars();
        let mut hma = HMA::new(9);
        hma.calculate(&bars);

        // Get last HMA value
        let last_valid = hma.values().iter().rev().find_map(|v| {
            if let IndicatorValue::Single(val) = v {
                Some(*val)
            } else {
                None
            }
        });

        if let Some(hma_val) = last_valid {
            let last_close = bars.last().unwrap().close;
            // HMA should be very close to current price in uptrend
            assert!(
                (hma_val - last_close).abs() < 5.0,
                "HMA should be very responsive to price"
            );
        }
    }

    #[test]
    fn test_hma_is_overlay() {
        let hma = HMA::new(9);
        assert!(hma.is_overlay());
    }

    #[test]
    fn test_hma_empty_data() {
        let mut hma = HMA::new(9);
        hma.calculate(&[]);
        assert!(hma.values().is_empty());
    }

    #[test]
    fn test_hma_min_period() {
        // HMA should work with period 2
        let bars = create_test_bars();
        let mut hma = HMA::new(2);
        hma.calculate(&bars);
        assert_eq!(hma.values().len(), bars.len());
    }
}
