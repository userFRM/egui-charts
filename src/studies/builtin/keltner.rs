//! Keltner Channels Indicator
//!
//! Keltner Channels are volatility-based bands set above and below an EMA.
//! The bands are based on ATR (Avg True Range).
//!
//! # Components
//! - Middle Line: EMA of close price
//! - Upper Band: EMA + (ATR multiplier * ATR)
//! - Lower Band: EMA - (ATR multiplier * ATR)
//!
//! # Interpretation
//! - Price above upper band: Strong uptrend or overbought
//! - Price below lower band: Strong downtrend or oversold
//! - Channel squeeze (narrow bands): Low volatility, potential breakout
//!
//! # Example
//! ```ignore
//! use egui_charts::KeltnerChannels;
//!
//! let mut keltner = KeltnerChannels::new(20, 10, 2.0);
//! keltner.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Keltner Channels indicator
#[derive(Clone)]
pub struct KeltnerChannels {
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
    values: Vec<IndicatorValue>,
    upper_color: Color32,
    middle_color: Color32,
    lower_color: Color32,
    visible: bool,
}

impl KeltnerChannels {
    /// Create a new Keltner Channels indicator
    ///
    /// # Arguments
    /// * `ema_period` - EMA period for middle line (typically 20)
    /// * `atr_period` - ATR period (typically 10)
    /// * `multiplier` - ATR multiplier (typically 2.0)
    pub fn new(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period: ema_period.max(1),
            atr_period: atr_period.max(1),
            multiplier,
            values: Vec::new(),
            upper_color: DESIGN_TOKENS.semantic.extended.pink, // Pink
            middle_color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            lower_color: DESIGN_TOKENS.semantic.extended.pink, // Pink
            visible: true,
        }
    }

    /// Create with default params (20, 10, 2.0)
    pub fn default_params() -> Self {
        Self::new(20, 10, 2.0)
    }

    /// Set colors for upper, middle, and lower bands
    pub fn with_colors(mut self, upper: Color32, middle: Color32, lower: Color32) -> Self {
        self.upper_color = upper;
        self.middle_color = middle;
        self.lower_color = lower;
        self
    }

    /// Calculate True Range
    #[inline]
    fn true_range(bar: &Bar, prev_close: f64) -> f64 {
        let hl = bar.high - bar.low;
        let hc = (bar.high - prev_close).abs();
        let lc = (bar.low - prev_close).abs();
        hl.max(hc).max(lc)
    }
}

impl Indicator for KeltnerChannels {
    fn name(&self) -> &str {
        "Keltner"
    }

    fn desc(&self) -> &str {
        "Keltner Channels - Volatility-based envelope"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.ema_period.max(self.atr_period);
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate EMA of close prices
        let multiplier = 2.0 / (self.ema_period as f64 + 1.0);
        let mut ema = vec![0.0; data.len()];

        // Initial EMA is SMA
        let initial_sum: f64 = data[..self.ema_period].iter().map(|b| b.close).sum();
        ema[self.ema_period - 1] = initial_sum / self.ema_period as f64;

        // Subsequent EMA values
        for i in self.ema_period..data.len() {
            ema[i] = (data[i].close - ema[i - 1]) * multiplier + ema[i - 1];
        }

        // Calculate ATR
        let mut tr = Vec::with_capacity(data.len());
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            tr.push(Self::true_range(&data[i], data[i - 1].close));
        }

        // Calculate ATR using Wilder's smoothing
        let atr_multiplier = 1.0 / self.atr_period as f64;
        let mut atr = vec![0.0; data.len()];

        // Initial ATR is simple avg
        if self.atr_period <= data.len() {
            let initial_atr: f64 =
                tr[..self.atr_period].iter().sum::<f64>() / self.atr_period as f64;
            atr[self.atr_period - 1] = initial_atr;

            // Subsequent ATR values
            for i in self.atr_period..data.len() {
                atr[i] = atr[i - 1] + atr_multiplier * (tr[i] - atr[i - 1]);
            }
        }

        // Build output values
        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let middle = ema[i];
                let band_width = self.multiplier * atr[i];
                let upper = middle + band_width;
                let lower = middle - band_width;

                self.values
                    .push(IndicatorValue::Multiple(vec![middle, upper, lower]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.middle_color, self.upper_color, self.lower_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.middle_color = colors[0];
        }
        if colors.len() > 1 {
            self.upper_color = colors[1];
        }
        if colors.len() > 2 {
            self.lower_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        true // Keltner is drawn on main chart
    }

    fn line_cnt(&self) -> usize {
        3 // Middle, Upper, Lower
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
            format!("EMA({})", self.ema_period),
            format!("Upper({}x ATR)", self.multiplier),
            format!("Lower({}x ATR)", self.multiplier),
        ]
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
                let price = 100.0 + (i as f64 * 0.2).sin() * 10.0;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 2.0,
                    low: price - 2.0,
                    close: price + 0.5,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_keltner_ordering() {
        let bars = create_test_bars();
        let mut keltner = KeltnerChannels::new(20, 10, 2.0);
        keltner.calculate(&bars);

        for value in keltner.values() {
            if let IndicatorValue::Multiple(vals) = value {
                let middle = vals[0];
                let upper = vals[1];
                let lower = vals[2];

                assert!(upper >= middle, "Upper should be >= middle");
                assert!(middle >= lower, "Middle should be >= lower");
            }
        }
    }

    #[test]
    fn test_keltner_is_overlay() {
        let keltner = KeltnerChannels::new(20, 10, 2.0);
        assert!(keltner.is_overlay());
    }

    #[test]
    fn test_keltner_line_cnt() {
        let keltner = KeltnerChannels::new(20, 10, 2.0);
        assert_eq!(keltner.line_cnt(), 3);
    }

    #[test]
    fn test_keltner_empty_data() {
        let mut keltner = KeltnerChannels::new(20, 10, 2.0);
        keltner.calculate(&[]);
        assert!(keltner.values().is_empty());
    }
}
