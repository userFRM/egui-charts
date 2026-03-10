//! Triple Exponential Moving Avg (TEMA) Indicator
//!
//! TEMA further reduces lag compared to DEMA by applying EMA three times
//! with a specific formula.
//!
//! # Formula
//! TEMA = 3 * EMA1 - 3 * EMA2 + EMA3
//! Where:
//! - EMA1 = EMA(price, period)
//! - EMA2 = EMA(EMA1, period)
//! - EMA3 = EMA(EMA2, period)
//!
//! # Example
//! ```ignore
//! use egui_charts::TripleEMA;
//!
//! let mut tema = TripleEMA::new(14);
//! tema.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Triple Exponential Moving Avg indicator
#[derive(Clone)]
pub struct TEMA {
    period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl TEMA {
    /// Create a new TEMA indicator
    ///
    /// # Arguments
    /// * `period` - Number of periods for calculation
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.pink, // Pink
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

impl Indicator for TEMA {
    fn name(&self) -> &str {
        "TEMA"
    }

    fn desc(&self) -> &str {
        "Triple Exponential Moving Avg - Minimal lag moving avg"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Need 3 * period - 2 bars for valid TEMA
        let min_period = 3 * self.period - 2;

        // Extract close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();

        // Calculate first EMA
        let ema1 = Self::calculate_ema(&closes, self.period);

        // Calculate second EMA (EMA of EMA)
        let ema2 = Self::calculate_ema(&ema1, self.period);

        // Calculate third EMA (EMA of EMA of EMA)
        let ema3 = Self::calculate_ema(&ema2, self.period);

        // Calculate TEMA = 3 * EMA1 - 3 * EMA2 + EMA3
        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let tema = 3.0 * ema1[i] - 3.0 * ema2[i] + ema3[i];
                self.values.push(IndicatorValue::Single(tema));
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
        vec![format!("TEMA({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..60)
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
    fn test_tema_calculation() {
        let bars = create_test_bars();
        let mut tema = TEMA::new(10);
        tema.calculate(&bars);

        assert_eq!(tema.values().len(), bars.len());

        // First 3*period - 3 values should be None
        let warmup = 3 * 10 - 3;
        for i in 0..warmup {
            assert!(matches!(tema.values()[i], IndicatorValue::None));
        }

        // Check remaining values are valid
        for value in tema.values().iter().skip(warmup) {
            if let IndicatorValue::Single(v) = value {
                assert!(*v > 0.0);
            }
        }
    }

    #[test]
    fn test_tema_tracks_price_closely() {
        let bars = create_test_bars();
        let mut tema = TEMA::new(10);
        tema.calculate(&bars);

        // Get last TEMA value
        if let Some(IndicatorValue::Single(tema_val)) = tema.values().last() {
            let last_close = bars.last().unwrap().close;
            // TEMA should track price very closely in uptrend
            assert!(
                (*tema_val - last_close).abs() < 10.0,
                "TEMA should track price very closely"
            );
        }
    }

    #[test]
    fn test_tema_is_overlay() {
        let tema = TEMA::new(14);
        assert!(tema.is_overlay());
    }

    #[test]
    fn test_tema_empty_data() {
        let mut tema = TEMA::new(14);
        tema.calculate(&[]);
        assert!(tema.values().is_empty());
    }
}
